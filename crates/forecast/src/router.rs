use std::io::Write;
use std::time::Duration;

use flate2::write::GzEncoder;
use flate2::Compression;
use lambda_http::{Body, Request, RequestExt, Response};
use tracing::{info, warn};

use crate::cache::{cache_key, DynamoCacheStore, S3CacheStore};
use crate::fetcher::{fetch_all_sources, fetch_ensemble_per_model};
use crate::location_tracker::record_access_with_params;
use crate::metrics::{emit_forecast_cache_metric, ForecastCacheOutcome};
use crate::model_selector::parse_model_selection;
use crate::models::{AppState, FetchParams, WEATHER_VARIABLES};
use crate::response::{build_members_response, build_response, ForecastResponse};
use crate::response_cache::{
    check_core_cache, check_volatile_cache, core_cache_key, merge_cached_response,
    store_core_cache, store_volatile_cache, volatile_cache_key, CoreResponseData, VolatileData,
};

/// Routes an incoming API Gateway request to the appropriate handler.
pub async fn route(event: &Request, state: &AppState) -> Result<Response<Body>, lambda_http::Error> {
    let path = event.uri().path();
    let method = event.method().as_str();

    info!(method = method, path = path, "Routing request");

    match (method, path) {
        ("GET", "/forecast/members") => handle_forecast_members(event, state).await,
        ("GET", "/forecast") => handle_forecast(event, state).await,
        ("GET", "/stations/observations") => handle_nearby_observation_stations(event, state).await,
        ("GET", "/stations/marine") => handle_nearby_marine_stations(event, state).await,
        _ => {
            warn!(method = method, path = path, "No route matched");
            not_found()
        }
    }
}

/// GET /forecast — fetch upstream data, compute statistics, return aggregated response.
async fn handle_forecast(
    event: &Request,
    state: &AppState,
) -> Result<Response<Body>, lambda_http::Error> {
    let handler_start = std::time::Instant::now();
    let params = event.query_string_parameters();

    let lat = match params.first("lat").and_then(|v| v.parse::<f64>().ok()) {
        Some(v) if (-90.0..=90.0).contains(&v) => v,
        Some(_) => return bad_request("Invalid latitude: must be between -90 and 90"),
        None => return bad_request("Missing required parameter: lat"),
    };

    let lon = match params.first("lon").and_then(|v| v.parse::<f64>().ok()) {
        Some(v) if (-180.0..=180.0).contains(&v) => v,
        Some(_) => return bad_request("Invalid longitude: must be between -180 and 180"),
        None => return bad_request("Missing required parameter: lon"),
    };

    let marine_lat = params.first("marine_lat").and_then(|v| v.parse::<f64>().ok());
    let marine_lon = params.first("marine_lon").and_then(|v| v.parse::<f64>().ok());
    let station_id = params.first("station_id").map(|s| s.to_string());
    let force_refresh = params
        .first("force_refresh")
        .map(|v| v == "true")
        .unwrap_or(false);
    let refresh_source = params.first("refresh_source").map(|s| s.to_string());

    // Parse and validate optional `models` parameter
    let models_param = params.first("models");
    let selected = match parse_model_selection(models_param) {
        Ok(s) => s,
        Err(e) => return bad_request(&e.to_error_message()),
    };

    // Parse and validate optional `forecast_days` parameter (1–35, default 10)
    let forecast_days = match params.first("forecast_days") {
        Some(v) => match v.parse::<u32>() {
            Ok(d) if (1..=35).contains(&d) => d,
            _ => {
                return bad_request(
                    "Invalid forecast_days: must be an integer between 1 and 35",
                )
            }
        },
        None => 10,
    };

    let fetch_params = FetchParams {
        lat,
        lon,
        marine_lat,
        marine_lon,
        station_id,
        force_refresh,
        refresh_source,
        models: models_param.map(|s| s.split(',').map(|v| v.trim().to_string()).collect()),
        forecast_days,
    };

    // Build cache stores for two-tier cache
    let s3_cache = S3CacheStore::new(
        state.s3_client.clone(),
        state.config.cache_bucket.clone(),
    );
    let ddb_cache = DynamoCacheStore::new(
        state.ddb_client.clone(),
        state.config.cache_table.clone(),
    );

    // --- Two-tier cache check (skip if force_refresh or refresh_source) ---
    let cache_check_start = std::time::Instant::now();
    if !fetch_params.force_refresh && fetch_params.refresh_source.is_none() {
        let c_key = core_cache_key(
            fetch_params.lat,
            fetch_params.lon,
            fetch_params.models.as_deref(),
            fetch_params.forecast_days,
        );

        if let Some(core_data) = check_core_cache(&s3_cache, &c_key).await {
            let v_key = volatile_cache_key(fetch_params.lat, fetch_params.lon);

            if let Some(vol_data) = check_volatile_cache(&ddb_cache, &v_key).await {
                // Full cache hit — merge and return immediately
                let cache_check_ms = cache_check_start.elapsed().as_millis();
                info!(cache_check_ms = cache_check_ms, "Full two-tier cache hit, merging and returning");
                emit_forecast_cache_metric(ForecastCacheOutcome::FullHit);
                let forecast_response = merge_cached_response(core_data, vol_data);
                let result = serialize_and_compress_response(&forecast_response);
                let total_ms = handler_start.elapsed().as_millis();
                info!(total_ms = total_ms, cache_check_ms = cache_check_ms, "Request complete (full cache hit)");
                return result;
            }
            // Partial hit — core fresh but volatile stale.
            // Fall through to full pipeline (volatile-only fetch optimization deferred).
            let cache_check_ms = cache_check_start.elapsed().as_millis();
            info!(cache_check_ms = cache_check_ms, "Partial cache hit (core fresh, volatile stale), falling through to full pipeline");
            emit_forecast_cache_metric(ForecastCacheOutcome::PartialHit);
        } else {
            let cache_check_ms = cache_check_start.elapsed().as_millis();
            info!(cache_check_ms = cache_check_ms, "Core cache miss");
            emit_forecast_cache_metric(ForecastCacheOutcome::Miss);
        }
    } else {
        emit_forecast_cache_metric(ForecastCacheOutcome::Bypass);
    }

    // Fire-and-forget: record this location access with parameter combination
    // for cache warming. Errors are logged inside and never affect the response.
    {
        let ddb_client = state.ddb_client.clone();
        let tracker_table = state.config.tracker_table.clone();
        let key = cache_key(fetch_params.lat, fetch_params.lon);
        let models = fetch_params.models.clone();
        let forecast_days = fetch_params.forecast_days;
        tokio::spawn(async move {
            record_access_with_params(
                &ddb_client,
                &tracker_table,
                &key,
                models.as_deref(),
                forecast_days,
            )
            .await;
        });
    }

    // Fetch all upstream sources (two-phase orchestration with caching)
    info!("Starting forecast fetch");
    let fetch_start = std::time::Instant::now();
    let results = fetch_all_sources(state, &fetch_params, &selected.models).await;
    let fetch_ms = fetch_start.elapsed().as_millis();
    info!(fetch_ms = fetch_ms, "Fetch complete, building response");

    // Build the complete forecast response from all source results
    let build_start = std::time::Instant::now();
    let forecast_response = build_response(results, &fetch_params);
    let build_ms = build_start.elapsed().as_millis();
    info!(build_ms = build_ms, "Response built, serializing JSON");

    // --- Store both cache tiers after building response (fire-and-forget) ---
    {
        let core_key = core_cache_key(
            fetch_params.lat,
            fetch_params.lon,
            fetch_params.models.as_deref(),
            fetch_params.forecast_days,
        );
        let vol_key = volatile_cache_key(fetch_params.lat, fetch_params.lon);

        // Extract core data from the response
        let core_data = CoreResponseData {
            ensemble: forecast_response.ensemble.clone(),
            marine: forecast_response.marine.clone(),
            uv: forecast_response.uv.clone(),
            air_quality: forecast_response.air_quality.clone(),
            tides: forecast_response.tides.clone(),
            water_temperature: forecast_response.water_temperature.clone(),
            ciops_sst: forecast_response.ciops_sst.clone(),
            astronomy: forecast_response.astronomy.clone(),
            cache: forecast_response
                .cache
                .iter()
                .filter(|(k, _)| !["observations", "hrrr"].contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };

        // Extract volatile data from the response
        let vol_data = VolatileData {
            observations: forecast_response.observations.clone(),
            hrrr: forecast_response.hrrr.clone(),
            cache: forecast_response
                .cache
                .iter()
                .filter(|(k, _)| ["observations", "hrrr"].contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        };

        // Fire-and-forget cache stores
        tokio::spawn(async move {
            store_core_cache(&s3_cache, &core_key, &core_data).await;
            store_volatile_cache(&ddb_cache, &vol_key, &vol_data).await;
        });
    }

    // Serialize to JSON
    let json_body = serde_json::to_string(&forecast_response)
        .map_err(|e| lambda_http::Error::from(format!("JSON serialization error: {e}")))?;

    info!(json_bytes = json_body.len(), "JSON serialized, compressing with gzip");

    // Gzip-compress the response to stay under Lambda's 6MB response limit.
    // The ensemble response with statistics is typically a few MB
    // of JSON but compresses to well under 1MB.
    let compressed = gzip_compress(json_body.as_bytes())
        .map_err(|e| lambda_http::Error::from(format!("Gzip compression error: {e}")))?;

    info!(
        json_bytes = json_body.len(),
        gzip_bytes = compressed.len(),
        ratio = format!("{:.1}%", (compressed.len() as f64 / json_body.len() as f64) * 100.0),
        "Returning compressed forecast response"
    );

    let total_ms = handler_start.elapsed().as_millis();
    info!(
        total_ms = total_ms,
        fetch_ms = fetch_ms,
        build_ms = build_ms,
        json_bytes = json_body.len(),
        gzip_bytes = compressed.len(),
        "Request complete (cache miss pipeline)"
    );

    let resp = Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .header("Content-Encoding", "gzip")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::Binary(compressed))
        .map_err(Box::new)?;
    Ok(resp)
}

/// GET /forecast/members — fetch ensemble member data for a single variable.
async fn handle_forecast_members(
    event: &Request,
    state: &AppState,
) -> Result<Response<Body>, lambda_http::Error> {
    let params = event.query_string_parameters();

    // Parse and validate required lat/lon
    let lat = match params.first("lat").and_then(|v| v.parse::<f64>().ok()) {
        Some(v) if (-90.0..=90.0).contains(&v) => v,
        Some(_) => return bad_request("Invalid latitude: must be between -90 and 90"),
        None => return bad_request("Missing required parameter: lat"),
    };

    let lon = match params.first("lon").and_then(|v| v.parse::<f64>().ok()) {
        Some(v) if (-180.0..=180.0).contains(&v) => v,
        Some(_) => return bad_request("Invalid longitude: must be between -180 and 180"),
        None => return bad_request("Missing required parameter: lon"),
    };

    // Parse and validate required `variable` parameter
    let variable = match params.first("variable") {
        Some(v) => v.to_string(),
        None => return bad_request("Missing required parameter: variable"),
    };

    if !WEATHER_VARIABLES.contains(&variable.as_str()) {
        let valid_list = WEATHER_VARIABLES.join(", ");
        return bad_request(&format!(
            "Invalid variable: '{}'. Valid variables: {}",
            variable, valid_list
        ));
    }

    // Parse and validate optional `models` parameter
    let models_param = params.first("models");
    let selected = match parse_model_selection(models_param) {
        Ok(s) => s,
        Err(e) => return bad_request(&e.to_error_message()),
    };

    // Parse optional marine coordinates (for cache key alignment)
    let marine_lat = params.first("marine_lat").and_then(|v| v.parse::<f64>().ok());
    let marine_lon = params.first("marine_lon").and_then(|v| v.parse::<f64>().ok());

    // Parse and validate optional `forecast_days` parameter (1–35, default 10)
    let forecast_days = match params.first("forecast_days") {
        Some(v) => match v.parse::<u32>() {
            Ok(d) if (1..=35).contains(&d) => d,
            _ => {
                return bad_request(
                    "Invalid forecast_days: must be an integer between 1 and 35",
                )
            }
        },
        None => 10,
    };

    let fetch_params = FetchParams {
        lat,
        lon,
        marine_lat,
        marine_lon,
        station_id: None,
        force_refresh: false,
        refresh_source: None,
        models: models_param.map(|s| s.split(',').map(|v| v.trim().to_string()).collect()),
        forecast_days,
    };

    // Build S3 cache store for ensemble per-model caching
    let s3_cache = S3CacheStore::new(
        state.s3_client.clone(),
        state.config.cache_bucket.clone(),
    );

    // Fetch only ensemble data (per-model), not all sources
    let timeout = Duration::from_secs(state.config.default_timeout_secs);
    info!(variable = %variable, "Starting members fetch");
    let result = fetch_ensemble_per_model(
        &state.http_client,
        &s3_cache,
        &fetch_params,
        &selected.models,
        timeout,
    )
    .await;

    // Extract data from the result
    let data = match result.data() {
        Some(d) => d,
        None => {
            let err_msg = result
                .error_message()
                .unwrap_or("Ensemble data unavailable");
            warn!(error = %err_msg, "Members fetch failed");
            return json_response(
                502,
                &serde_json::json!({ "error": format!("Ensemble data unavailable: {err_msg}") })
                    .to_string(),
            );
        }
    };

    // Build the members response for the requested variable
    let model_suffixes: Vec<&str> = selected
        .models
        .iter()
        .map(|m| m.api_key_suffix)
        .collect();
    let members_response = build_members_response(data, &variable, &model_suffixes, fetch_params.forecast_days);
    info!("Members response built, serializing JSON");

    // Serialize to JSON
    let json_body = serde_json::to_string(&members_response)
        .map_err(|e| lambda_http::Error::from(format!("JSON serialization error: {e}")))?;

    info!(json_bytes = json_body.len(), "JSON serialized, compressing with gzip");

    // Gzip-compress the response
    let compressed = gzip_compress(json_body.as_bytes())
        .map_err(|e| lambda_http::Error::from(format!("Gzip compression error: {e}")))?;

    info!(
        json_bytes = json_body.len(),
        gzip_bytes = compressed.len(),
        ratio = format!("{:.1}%", (compressed.len() as f64 / json_body.len() as f64) * 100.0),
        "Returning compressed members response"
    );

    let resp = Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .header("Content-Encoding", "gzip")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::Binary(compressed))
        .map_err(Box::new)?;
    Ok(resp)
}

/// GET /stations/observations — discover nearby NWS observation stations.
async fn handle_nearby_observation_stations(
    _event: &Request,
    _state: &AppState,
) -> Result<Response<Body>, lambda_http::Error> {
    // TODO: implement in task 21
    json_response(200, r#"{"stations":[]}"#)
}

/// GET /stations/marine — search bundled NOAA station registry.
async fn handle_nearby_marine_stations(
    _event: &Request,
    _state: &AppState,
) -> Result<Response<Body>, lambda_http::Error> {
    // TODO: implement in task 21
    json_response(200, r#"{"stations":[]}"#)
}

// ---------------------------------------------------------------------------
// Response helpers
// ---------------------------------------------------------------------------

/// Serializes a ForecastResponse to JSON, gzip-compresses it, and returns the
/// HTTP response. Used for returning cached responses without going through
/// the full pipeline.
fn serialize_and_compress_response(
    forecast_response: &ForecastResponse,
) -> Result<Response<Body>, lambda_http::Error> {
    let json_body = serde_json::to_string(forecast_response)
        .map_err(|e| lambda_http::Error::from(format!("JSON serialization error: {e}")))?;

    let compressed = gzip_compress(json_body.as_bytes())
        .map_err(|e| lambda_http::Error::from(format!("Gzip compression error: {e}")))?;

    info!(
        json_bytes = json_body.len(),
        gzip_bytes = compressed.len(),
        "Returning compressed forecast response (from cache)"
    );

    let resp = Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .header("Content-Encoding", "gzip")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::Binary(compressed))
        .map_err(Box::new)?;
    Ok(resp)
}

/// Gzip-compress a byte slice.
fn gzip_compress(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
    encoder.write_all(data)?;
    encoder.finish()
}

fn json_response(status: u16, body: &str) -> Result<Response<Body>, lambda_http::Error> {
    let resp = Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::from(body.to_string()))
        .map_err(Box::new)?;
    Ok(resp)
}

fn bad_request(message: &str) -> Result<Response<Body>, lambda_http::Error> {
    let body = serde_json::json!({ "error": message }).to_string();
    json_response(400, &body)
}

fn not_found() -> Result<Response<Body>, lambda_http::Error> {
    json_response(404, r#"{"error":"Not found"}"#)
}
