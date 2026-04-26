use std::sync::Arc;

use lambda_http::{run, service_fn, Body, Error, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::task::JoinSet;

/// Shared HTTP client, created once per Lambda cold start.
struct AppState {
    http_client: reqwest::Client,
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
        url: "https://ensemble-api.open-meteo.com/v1/ensemble?latitude=0&longitude=0&models=ecmwf_ifs025_ensemble&hourly=temperature_2m&forecast_days=1",
    },
    ModelEndpoint {
        key: "gfs_gefs",
        url: "https://ensemble-api.open-meteo.com/v1/ensemble?latitude=0&longitude=0&models=ncep_gefs_seamless&hourly=temperature_2m&forecast_days=1",
    },
    ModelEndpoint {
        key: "icon",
        url: "https://ensemble-api.open-meteo.com/v1/ensemble?latitude=0&longitude=0&models=icon_seamless_eps&hourly=temperature_2m&forecast_days=1",
    },
    ModelEndpoint {
        key: "gem",
        url: "https://ensemble-api.open-meteo.com/v1/ensemble?latitude=0&longitude=0&models=gem_global_ensemble&hourly=temperature_2m&forecast_days=1",
    },
    ModelEndpoint {
        key: "bom_access",
        url: "https://ensemble-api.open-meteo.com/v1/ensemble?latitude=0&longitude=0&models=bom_access_global_ensemble&hourly=temperature_2m&forecast_days=1",
    },
    ModelEndpoint {
        key: "hrrr",
        url: "https://api.open-meteo.com/v1/gfs?latitude=0&longitude=0&hourly=temperature_2m&forecast_days=1",
    },
    ModelEndpoint {
        key: "marine",
        url: "https://marine-api.open-meteo.com/v1/marine?latitude=0&longitude=0&hourly=wave_height&forecast_days=1",
    },
    ModelEndpoint {
        key: "air_quality",
        url: "https://air-quality-api.open-meteo.com/v1/air-quality?latitude=0&longitude=0&hourly=us_aqi&forecast_days=1",
    },
    ModelEndpoint {
        key: "uv_gfs",
        url: "https://api.open-meteo.com/v1/forecast?latitude=0&longitude=0&hourly=uv_index&forecast_days=1",
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
        last_run_initialisation_time: body
            .get("last_run_initialisation_time")
            .and_then(|v| v.as_str())
            .map(String::from),
        last_run_availability_time: body
            .get("last_run_availability_time")
            .and_then(|v| v.as_str())
            .map(String::from),
        update_interval_seconds: body
            .get("update_interval_seconds")
            .and_then(|v| v.as_f64()),
    })
}

// ---------------------------------------------------------------------------
// Lambda handler
// ---------------------------------------------------------------------------

/// Metadata Lambda handler.
///
/// Fetches metadata from all 9 Open-Meteo model endpoints concurrently and
/// returns a JSON object keyed by model name. Individual failures produce
/// `null` for that model without failing the entire response.
async fn handler(state: &AppState, _event: Request) -> Result<Response<Body>, Error> {
    let mut join_set = JoinSet::new();

    for endpoint in &MODEL_ENDPOINTS {
        let client = state.http_client.clone();
        let url = endpoint.url.to_string();
        let key = endpoint.key.to_string();

        join_set.spawn(async move {
            let result = fetch_model_metadata(&client, &url).await;
            (key, result)
        });
    }

    let mut models = serde_json::Map::new();

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok((key, Ok(metadata))) => {
                models.insert(
                    key,
                    serde_json::to_value(metadata).unwrap_or(Value::Null),
                );
            }
            Ok((key, Err(_))) => {
                // Individual model failure → null
                models.insert(key, Value::Null);
            }
            Err(_join_err) => {
                // Task panicked — skip silently
            }
        }
    }

    let response_body = serde_json::json!({ "models": models });

    Ok(Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .body(Body::from(response_body.to_string()))
        .map_err(Box::new)?)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let state = Arc::new(AppState {
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("failed to build HTTP client"),
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
    /// to Open-Meteo API domains.
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
            // Each URL should request at least one hourly variable
            assert!(
                endpoint.url.contains("hourly="),
                "endpoint {} URL should request hourly data: {}",
                endpoint.key,
                endpoint.url
            );
        }
    }
}
