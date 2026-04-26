use lambda_http::{Body, Request, RequestExt, Response};

use crate::fetcher::fetch_all_sources;
use crate::models::{AppState, FetchParams};
use crate::response::build_response;

/// Routes an incoming API Gateway request to the appropriate handler.
pub async fn route(event: &Request, state: &AppState) -> Result<Response<Body>, lambda_http::Error> {
    let path = event.uri().path();
    let method = event.method().as_str();

    match (method, path) {
        ("GET", "/forecast") => handle_forecast(event, state).await,
        ("GET", "/stations/observations") => handle_nearby_observation_stations(event, state).await,
        ("GET", "/stations/marine") => handle_nearby_marine_stations(event, state).await,
        _ => not_found(),
    }
}

/// GET /forecast — fetch upstream data, compute statistics, return aggregated response.
async fn handle_forecast(
    event: &Request,
    state: &AppState,
) -> Result<Response<Body>, lambda_http::Error> {
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

    let fetch_params = FetchParams {
        lat,
        lon,
        marine_lat,
        marine_lon,
        station_id,
        force_refresh,
        refresh_source,
    };

    // Fetch all upstream sources (two-phase orchestration with caching)
    let results = fetch_all_sources(state, &fetch_params).await;

    // Build the complete forecast response from all source results
    let forecast_response = build_response(results, &fetch_params);

    // Serialize to JSON
    let body = serde_json::to_string(&forecast_response)
        .map_err(|e| lambda_http::Error::from(format!("JSON serialization error: {e}")))?;

    json_response(200, &body)
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

fn json_response(status: u16, body: &str) -> Result<Response<Body>, lambda_http::Error> {
    let resp = Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
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
