use std::io::{Read, Write};
use std::sync::Arc;

use aws_sdk_dynamodb::primitives::Blob;
use aws_sdk_dynamodb::types::AttributeValue;
use chrono::{DateTime, Utc};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use forecast::metrics::{emit_metadata_cache_metric, MetadataCacheOutcome};
use lambda_http::{run, service_fn, Body, Error, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::task::JoinSet;
use tracing::{info, warn};

/// TTL for the models metadata cache (30 minutes).
const MODELS_METADATA_TTL_SECS: u64 = 1800;

/// Fixed cache key for models metadata.
const MODELS_METADATA_CACHE_KEY: &str = "models_metadata";

/// Fixed source key for models metadata.
const MODELS_METADATA_SOURCE: &str = "metadata";

/// Shared state, created once per Lambda cold start.
struct AppState {
    http_client: reqwest::Client,
    ddb_client: aws_sdk_dynamodb::Client,
    cache_table: String,
}

// ---------------------------------------------------------------------------
// Model metadata definitions
// ---------------------------------------------------------------------------

/// A model whose metadata we want to fetch from Open-Meteo.
struct ModelEndpoint {
    /// Key used in the response JSON (e.g. "ecmwf_ifs025").
    key: &'static str,
    /// Full URL to fetch a minimal response that includes metadata fields.
    url: &'static str,
}

const MODEL_ENDPOINTS: [ModelEndpoint; 9] = [
    ModelEndpoint {
        key: "ecmwf_ifs025",
        url: "https://ensemble-api.open-meteo.com/data/ecmwf_ifs025_ensemble/static/meta.json",
    },
    ModelEndpoint {
        key: "gfs_gefs",
        url: "https://ensemble-api.open-meteo.com/data/ncep_gefs025/static/meta.json",
    },
    ModelEndpoint {
        key: "icon",
        url: "https://ensemble-api.open-meteo.com/data/icon_seamless_eps/static/meta.json",
    },
    ModelEndpoint {
        key: "gem",
        url: "https://ensemble-api.open-meteo.com/data/cmc_gem_geps/static/meta.json",
    },
    ModelEndpoint {
        key: "bom_access",
        url: "https://ensemble-api.open-meteo.com/data/bom_access_global_ensemble/static/meta.json",
    },
    ModelEndpoint {
        key: "hrrr",
        url: "https://api.open-meteo.com/data/ncep_hrrr_conus/static/meta.json",
    },
    ModelEndpoint {
        key: "marine",
        url: "https://marine-api.open-meteo.com/data/ecmwf_wam025/static/meta.json",
    },
    ModelEndpoint {
        key: "air_quality",
        url: "https://air-quality-api.open-meteo.com/data/cams_global/static/meta.json",
    },
    ModelEndpoint {
        key: "uv_gfs",
        url: "https://api.open-meteo.com/data/ncep_gfs025/static/meta.json",
    },
];

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModelMetadata {
    last_run_initialisation_time: Option<String>,
    last_run_availability_time: Option<String>,
    update_interval_seconds: Option<f64>,
}

// ---------------------------------------------------------------------------
// Metadata extraction
// ---------------------------------------------------------------------------

/// Convert a Unix timestamp (integer seconds) from the JSON response to an ISO 8601 string.
fn unix_timestamp_to_iso(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|v| v.as_i64())
        .and_then(|ts| DateTime::from_timestamp(ts, 0))
        .map(|dt: DateTime<Utc>| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

/// Fetch a single model endpoint and extract metadata fields from the response.
async fn fetch_model_metadata(
    client: &reqwest::Client,
    url: &str,
) -> Result<ModelMetadata, String> {
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("request failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("HTTP {}", resp.status().as_u16()));
    }

    let body: Value = resp
        .json()
        .await
        .map_err(|e| format!("failed to parse JSON: {}", e))?;

    Ok(ModelMetadata {
        last_run_initialisation_time: unix_timestamp_to_iso(
            body.get("last_run_initialisation_time"),
        ),
        last_run_availability_time: unix_timestamp_to_iso(
            body.get("last_run_availability_time"),
        ),
        update_interval_seconds: body
            .get("update_interval_seconds")
            .and_then(|v| v.as_f64()),
    })
}

// ---------------------------------------------------------------------------
// Cache helpers
// ---------------------------------------------------------------------------

/// Checks the DynamoDB metadata cache for a fresh entry.
///
/// Returns the cached JSON string if fresh, None otherwise.
async fn check_metadata_cache(state: &AppState) -> Option<String> {
    if state.cache_table.is_empty() {
        return None;
    }

    let result = state
        .ddb_client
        .get_item()
        .table_name(&state.cache_table)
        .key(
            "cache_key",
            AttributeValue::S(MODELS_METADATA_CACHE_KEY.to_string()),
        )
        .key(
            "source",
            AttributeValue::S(MODELS_METADATA_SOURCE.to_string()),
        )
        .send()
        .await;

    let result = match result {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, "Failed to read metadata cache from DynamoDB");
            return None;
        }
    };

    let item = result.item()?;

    // Check freshness
    let stored_at_str = item.get("stored_at")?.as_s().ok()?;
    let stored_at = DateTime::parse_from_rfc3339(stored_at_str)
        .ok()?
        .with_timezone(&Utc);
    let elapsed = Utc::now().signed_duration_since(stored_at).num_seconds();
    if elapsed < 0 || (elapsed as u64) >= MODELS_METADATA_TTL_SECS {
        return None;
    }

    // Decompress data
    let compressed = item.get("data")?.as_b().ok()?.as_ref().to_vec();
    let mut decoder = GzDecoder::new(&compressed[..]);
    let mut json_str = String::new();
    if let Err(e) = decoder.read_to_string(&mut json_str) {
        warn!(error = %e, "Failed to decompress cached metadata");
        return None;
    }

    Some(json_str)
}

/// Stores the metadata response in DynamoDB cache (gzip-compressed JSON).
///
/// Logs warnings on errors but never fails the request.
async fn store_metadata_cache(state: &AppState, json_body: &str) {
    if state.cache_table.is_empty() {
        return;
    }

    let now = Utc::now();
    let expires_at = now.timestamp() + MODELS_METADATA_TTL_SECS as i64;

    // Gzip compress
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    if let Err(e) = encoder.write_all(json_body.as_bytes()) {
        warn!(error = %e, "Failed to compress metadata for cache");
        return;
    }
    let compressed = match encoder.finish() {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "Failed to finish gzip compression for metadata cache");
            return;
        }
    };

    if let Err(e) = state
        .ddb_client
        .put_item()
        .table_name(&state.cache_table)
        .item(
            "cache_key",
            AttributeValue::S(MODELS_METADATA_CACHE_KEY.to_string()),
        )
        .item(
            "source",
            AttributeValue::S(MODELS_METADATA_SOURCE.to_string()),
        )
        .item("data", AttributeValue::B(Blob::new(compressed)))
        .item("stored_at", AttributeValue::S(now.to_rfc3339()))
        .item("expires_at", AttributeValue::N(expires_at.to_string()))
        .send()
        .await
    {
        warn!(error = %e, "Failed to store metadata in DynamoDB cache");
    }
}

// ---------------------------------------------------------------------------
// Lambda handler
// ---------------------------------------------------------------------------

/// Metadata Lambda handler.
///
/// Checks the DynamoDB cache first. On a cache hit, returns the cached response
/// immediately. On a miss, fetches metadata from all 9 Open-Meteo model
/// endpoints concurrently, stores the result in cache, and returns a JSON object
/// keyed by model name. Individual fetch failures produce `null` for that model
/// without failing the entire response.
async fn handler(state: &AppState, _event: Request) -> Result<Response<Body>, Error> {
    let handler_start = std::time::Instant::now();

    // Check cache first
    let cache_check_start = std::time::Instant::now();
    if let Some(cached_json) = check_metadata_cache(state).await {
        let cache_check_ms = cache_check_start.elapsed().as_millis();
        info!(cache_check_ms = cache_check_ms, "Metadata cache hit, returning cached response");
        emit_metadata_cache_metric(MetadataCacheOutcome::Hit);
        let total_ms = handler_start.elapsed().as_millis();
        info!(total_ms = total_ms, cache_check_ms = cache_check_ms, "Request complete (cache hit)");
        return Ok(Response::builder()
            .status(200)
            .header("Content-Type", "application/json")
            .body(Body::from(cached_json))
            .map_err(Box::new)?);
    }
    let cache_check_ms = cache_check_start.elapsed().as_millis();
    info!(cache_check_ms = cache_check_ms, "Metadata cache miss");

    // Cache miss — fetch from all 9 endpoints
    emit_metadata_cache_metric(MetadataCacheOutcome::Miss);
    let fetch_start = std::time::Instant::now();
    let mut join_set = JoinSet::new();

    for endpoint in &MODEL_ENDPOINTS {
        let client = state.http_client.clone();
        let url = endpoint.url.to_string();
        let key = endpoint.key.to_string();

        join_set.spawn(async move {
            let start = std::time::Instant::now();
            let result = fetch_model_metadata(&client, &url).await;
            let elapsed_ms = start.elapsed().as_millis();
            (key, result, elapsed_ms)
        });
    }

    let mut models = serde_json::Map::new();

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok((key, Ok(metadata), elapsed_ms)) => {
                info!(source = %key, elapsed_ms = elapsed_ms, "Model metadata fetched");
                models.insert(
                    key,
                    serde_json::to_value(metadata).unwrap_or(Value::Null),
                );
            }
            Ok((key, Err(e), elapsed_ms)) => {
                warn!(source = %key, elapsed_ms = elapsed_ms, error = %e, "Model metadata fetch failed");
                models.insert(key, Value::Null);
            }
            Err(_join_err) => {
                // Task panicked — skip silently
            }
        }
    }
    let fetch_ms = fetch_start.elapsed().as_millis();

    let response_body = serde_json::json!({ "models": models });
    let json_str = response_body.to_string();

    // Store in cache (fire-and-forget style — log errors but don't fail)
    store_metadata_cache(state, &json_str).await;

    let total_ms = handler_start.elapsed().as_millis();
    info!(
        total_ms = total_ms,
        cache_check_ms = cache_check_ms,
        fetch_ms = fetch_ms,
        models_fetched = models.len(),
        "Request complete (cache miss)"
    );

    Ok(Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(Body::from(json_str))
        .map_err(Box::new)?)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_ansi(false)
        .without_time()
        .init();

    let cache_table =
        std::env::var("CACHE_TABLE").unwrap_or_else(|_| String::new());

    let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let ddb_client = aws_sdk_dynamodb::Client::new(&aws_config);

    let state = Arc::new(AppState {
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client"),
        ddb_client,
        cache_table,
    });

    run(service_fn(move |event: Request| {
        let state = Arc::clone(&state);
        async move { handler(&state, event).await }
    }))
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_endpoints_count() {
        assert_eq!(MODEL_ENDPOINTS.len(), 9);
    }

    #[test]
    fn test_model_endpoint_keys_unique() {
        let keys: Vec<&str> = MODEL_ENDPOINTS.iter().map(|e| e.key).collect();
        let mut deduped = keys.clone();
        deduped.sort();
        deduped.dedup();
        assert_eq!(keys.len(), deduped.len(), "Model endpoint keys must be unique");
    }

    #[test]
    fn test_model_metadata_serialization() {
        let meta = ModelMetadata {
            last_run_initialisation_time: Some("2026-04-24T00:00:00Z".to_string()),
            last_run_availability_time: Some("2026-04-24T06:30:00Z".to_string()),
            update_interval_seconds: Some(21600.0),
        };
        let json = serde_json::to_value(&meta).unwrap();
        assert_eq!(
            json["last_run_initialisation_time"],
            "2026-04-24T00:00:00Z"
        );
        assert_eq!(json["update_interval_seconds"], 21600.0);
    }

    #[test]
    fn test_model_metadata_null_fields() {
        let meta = ModelMetadata {
            last_run_initialisation_time: None,
            last_run_availability_time: None,
            update_interval_seconds: None,
        };
        let json = serde_json::to_value(&meta).unwrap();
        assert!(json["last_run_initialisation_time"].is_null());
        assert!(json["last_run_availability_time"].is_null());
        assert!(json["update_interval_seconds"].is_null());
    }

    /// Verifies the response structure when all models return null,
    /// simulating a scenario where every upstream metadata fetch fails.
    /// Each model key should map to null, and the overall response should
    /// still be valid JSON with the "models" wrapper.
    #[test]
    fn test_all_models_null_response_structure() {
        // Simulate the response assembly when all model fetches fail:
        // each key maps to Value::Null
        let mut models = serde_json::Map::new();
        for endpoint in &MODEL_ENDPOINTS {
            models.insert(endpoint.key.to_string(), serde_json::Value::Null);
        }

        let response_body = serde_json::json!({ "models": models });
        let json_str = response_body.to_string();

        // Parse back and verify structure
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let models_obj = parsed.get("models").unwrap().as_object().unwrap();

        // All 9 model keys should be present
        assert_eq!(models_obj.len(), 9);

        // Each model should be null
        for endpoint in &MODEL_ENDPOINTS {
            assert!(
                models_obj.contains_key(endpoint.key),
                "missing model key: {}",
                endpoint.key
            );
            assert!(
                models_obj[endpoint.key].is_null(),
                "model {} should be null when fetch fails",
                endpoint.key
            );
        }

        // Verify specific expected keys are present
        assert!(models_obj.contains_key("ecmwf_ifs025"));
        assert!(models_obj.contains_key("gfs_gefs"));
        assert!(models_obj.contains_key("icon"));
        assert!(models_obj.contains_key("gem"));
        assert!(models_obj.contains_key("bom_access"));
        assert!(models_obj.contains_key("hrrr"));
        assert!(models_obj.contains_key("marine"));
        assert!(models_obj.contains_key("air_quality"));
        assert!(models_obj.contains_key("uv_gfs"));
    }

    /// Verifies the response structure when some models succeed and others
    /// fail, ensuring error isolation at the model level.
    #[test]
    fn test_partial_model_failure_response_structure() {
        let mut models = serde_json::Map::new();

        // Simulate 2 successful models
        let success_meta = ModelMetadata {
            last_run_initialisation_time: Some("2026-04-24T00:00:00Z".to_string()),
            last_run_availability_time: Some("2026-04-24T06:30:00Z".to_string()),
            update_interval_seconds: Some(21600.0),
        };
        models.insert(
            "ecmwf_ifs025".to_string(),
            serde_json::to_value(&success_meta).unwrap(),
        );
        models.insert(
            "hrrr".to_string(),
            serde_json::to_value(&success_meta).unwrap(),
        );

        // Simulate remaining 7 models failing
        for endpoint in &MODEL_ENDPOINTS {
            if endpoint.key != "ecmwf_ifs025" && endpoint.key != "hrrr" {
                models.insert(endpoint.key.to_string(), serde_json::Value::Null);
            }
        }

        let response_body = serde_json::json!({ "models": models });
        let parsed: serde_json::Value =
            serde_json::from_str(&response_body.to_string()).unwrap();
        let models_obj = parsed.get("models").unwrap().as_object().unwrap();

        // All 9 keys should be present
        assert_eq!(models_obj.len(), 9);

        // Successful models should have data
        assert!(!models_obj["ecmwf_ifs025"].is_null());
        assert_eq!(
            models_obj["ecmwf_ifs025"]["last_run_initialisation_time"],
            "2026-04-24T00:00:00Z"
        );

        // Failed models should be null
        assert!(models_obj["icon"].is_null());
        assert!(models_obj["gem"].is_null());
        assert!(models_obj["marine"].is_null());
    }

    /// Verifies that all model endpoint URLs are valid HTTPS URLs pointing
    /// to Open-Meteo API domains and using the metadata JSON format.
    #[test]
    fn test_model_endpoint_urls_valid() {
        for endpoint in &MODEL_ENDPOINTS {
            assert!(
                endpoint.url.starts_with("https://"),
                "endpoint {} URL should use HTTPS: {}",
                endpoint.key,
                endpoint.url
            );
            assert!(
                endpoint.url.contains("open-meteo.com"),
                "endpoint {} URL should point to open-meteo.com: {}",
                endpoint.key,
                endpoint.url
            );
            // Each URL should point to the static metadata JSON
            assert!(
                endpoint.url.ends_with("/static/meta.json"),
                "endpoint {} URL should be a metadata endpoint: {}",
                endpoint.key,
                endpoint.url
            );
        }
    }

    #[test]
    fn test_unix_timestamp_to_iso_valid() {
        // 1724796000 = Tue Aug 27, 2024, 22:00:00 UTC
        let value = serde_json::json!(1724796000);
        let result = unix_timestamp_to_iso(Some(&value));
        assert_eq!(result, Some("2024-08-27T22:00:00Z".to_string()));
    }

    #[test]
    fn test_unix_timestamp_to_iso_none() {
        assert_eq!(unix_timestamp_to_iso(None), None);
    }

    #[test]
    fn test_unix_timestamp_to_iso_non_integer() {
        let value = serde_json::json!("not a number");
        let result = unix_timestamp_to_iso(Some(&value));
        assert_eq!(result, None);
    }

    #[test]
    fn test_cache_constants() {
        assert_eq!(MODELS_METADATA_TTL_SECS, 1800);
        assert_eq!(MODELS_METADATA_CACHE_KEY, "models_metadata");
        assert_eq!(MODELS_METADATA_SOURCE, "metadata");
    }

    #[test]
    fn test_gzip_round_trip() {
        let original = r#"{"models":{"ecmwf_ifs025":{"last_run_initialisation_time":"2026-04-24T00:00:00Z"}}}"#;

        // Compress
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(original.as_bytes()).unwrap();
        let compressed = encoder.finish().unwrap();

        // Decompress
        let mut decoder = GzDecoder::new(&compressed[..]);
        let mut decompressed = String::new();
        decoder.read_to_string(&mut decompressed).unwrap();

        assert_eq!(original, decompressed);
    }
}
