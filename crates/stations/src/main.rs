use std::sync::Arc;

use lambda_http::{run, service_fn, Body, Error, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// Bundled NOAA station registry
// ---------------------------------------------------------------------------

/// The raw JSON bytes of the NOAA station registry, compiled into the binary.
const NOAA_STATIONS_JSON: &[u8] = include_bytes!("../../../data/noaa_stations.json");

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarineStation {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub station_type: String,
    pub lat: f64,
    pub lon: f64,
}

/// A station result with computed distance from the search coordinate.
#[derive(Debug, Clone, Serialize)]
pub struct StationResult {
    pub id: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub distance_km: f64,
}

// ---------------------------------------------------------------------------
// Haversine distance utility (duplicated from forecast crate)
// ---------------------------------------------------------------------------

/// Computes the great-circle distance in kilometres between two points on
/// Earth using the Haversine formula.
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6371.0; // Earth radius in km
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    R * c
}

// ---------------------------------------------------------------------------
// Station search logic
// ---------------------------------------------------------------------------

/// Search the bundled NOAA station registry for stations within `radius_km`
/// of the given coordinate. Returns results sorted by ascending distance.
pub fn search_marine_stations(
    stations: &[MarineStation],
    lat: f64,
    lon: f64,
    radius_km: f64,
) -> Vec<StationResult> {
    let mut results: Vec<StationResult> = stations
        .iter()
        .filter_map(|s| {
            let d = haversine_km(lat, lon, s.lat, s.lon);
            if d <= radius_km {
                Some(StationResult {
                    id: s.id.clone(),
                    name: s.name.clone(),
                    latitude: s.lat,
                    longitude: s.lon,
                    distance_km: (d * 100.0).round() / 100.0, // round to 2 decimal places
                })
            } else {
                None
            }
        })
        .collect();

    results.sort_by(|a, b| {
        a.distance_km
            .partial_cmp(&b.distance_km)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    results
}

// ---------------------------------------------------------------------------
// Shared HTTP client state
// ---------------------------------------------------------------------------

struct AppState {
    http_client: reqwest::Client,
    marine_stations: Vec<MarineStation>,
}

// ---------------------------------------------------------------------------
// Query parameter parsing
// ---------------------------------------------------------------------------

/// Extract a query parameter value from the request URI.
fn get_query_param(event: &Request, key: &str) -> Option<String> {
    event.uri().query().and_then(|qs| {
        qs.split('&')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                let k = parts.next()?;
                let v = parts.next().unwrap_or("");
                if k == key {
                    Some(v.to_string())
                } else {
                    None
                }
            })
            .next()
    })
}

fn parse_lat_lon(event: &Request) -> Result<(f64, f64), Response<Body>> {
    let lat_str = get_query_param(event, "lat").ok_or_else(|| {
        Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(Body::from(
                r#"{"error":"Missing required parameter: lat"}"#,
            ))
            .unwrap()
    })?;

    let lon_str = get_query_param(event, "lon").ok_or_else(|| {
        Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(Body::from(
                r#"{"error":"Missing required parameter: lon"}"#,
            ))
            .unwrap()
    })?;

    let lat: f64 = lat_str.parse().map_err(|_| {
        Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(Body::from(
                r#"{"error":"Invalid latitude: must be a number between -90 and 90"}"#,
            ))
            .unwrap()
    })?;

    let lon: f64 = lon_str.parse().map_err(|_| {
        Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(Body::from(
                r#"{"error":"Invalid longitude: must be a number between -180 and 180"}"#,
            ))
            .unwrap()
    })?;

    if !(-90.0..=90.0).contains(&lat) {
        return Err(Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(Body::from(
                r#"{"error":"Invalid latitude: must be between -90 and 90"}"#,
            ))
            .unwrap());
    }

    if !(-180.0..=180.0).contains(&lon) {
        return Err(Response::builder()
            .status(400)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(Body::from(
                r#"{"error":"Invalid longitude: must be between -180 and 180"}"#,
            ))
            .unwrap());
    }

    Ok((lat, lon))
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

/// `GET /stations/observations` — discover nearby NWS observation stations.
///
/// Queries the NWS points API to find observation stations near the given
/// coordinates, computes Haversine distance, and returns them sorted by
/// distance.
async fn handle_observations(
    client: &reqwest::Client,
    lat: f64,
    lon: f64,
) -> Result<Response<Body>, Error> {
    let points_url = format!(
        "https://api.weather.gov/points/{:.4},{:.4}/stations",
        lat, lon
    );

    let resp = match client
        .get(&points_url)
        .header("User-Agent", "EnsembleWeather/1.0.0")
        .header("Accept", "application/geo+json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let msg = format!(r#"{{"error":"NWS API request failed: {}"}}"#, e);
            return Ok(Response::builder()
                .status(502)
                .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
                .body(Body::from(msg))
                .map_err(Box::new)?);
        }
    };

    if !resp.status().is_success() {
        let msg = format!(
            r#"{{"error":"NWS API returned status {}"}}"#,
            resp.status().as_u16()
        );
        return Ok(Response::builder()
            .status(502)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(Body::from(msg))
            .map_err(Box::new)?);
    }

    let body: Value = match resp.json().await {
        Ok(v) => v,
        Err(e) => {
            let msg = format!(r#"{{"error":"Failed to parse NWS response: {}"}}"#, e);
            return Ok(Response::builder()
                .status(502)
                .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
                .body(Body::from(msg))
                .map_err(Box::new)?);
        }
    };

    // The NWS response is GeoJSON with features containing station info.
    let features = body
        .get("features")
        .and_then(|f| f.as_array())
        .cloned()
        .unwrap_or_default();

    let mut stations: Vec<StationResult> = features
        .iter()
        .filter_map(|feature| {
            let props = feature.get("properties")?;
            let station_id = props.get("stationIdentifier")?.as_str()?;
            let name = props.get("name")?.as_str()?;

            // Coordinates are in GeoJSON [lon, lat] order.
            let coords = feature
                .get("geometry")?
                .get("coordinates")?
                .as_array()?;
            let slon = coords.first()?.as_f64()?;
            let slat = coords.get(1)?.as_f64()?;

            let d = haversine_km(lat, lon, slat, slon);

            Some(StationResult {
                id: station_id.to_string(),
                name: name.to_string(),
                latitude: slat,
                longitude: slon,
                distance_km: (d * 100.0).round() / 100.0,
            })
        })
        .collect();

    stations.sort_by(|a, b| {
        a.distance_km
            .partial_cmp(&b.distance_km)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let response_body = serde_json::json!({ "stations": stations });

    Ok(Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
        .body(Body::from(response_body.to_string()))
        .map_err(Box::new)?)
}

/// `GET /stations/marine` — search bundled NOAA station registry.
///
/// Searches the compiled-in station registry for stations within a
/// configurable radius (default 100km) and returns them sorted by distance.
fn handle_marine(
    stations: &[MarineStation],
    lat: f64,
    lon: f64,
    radius_km: f64,
) -> Result<Response<Body>, Error> {
    let results = search_marine_stations(stations, lat, lon, radius_km);
    let response_body = serde_json::json!({ "stations": results });

    Ok(Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
        .body(Body::from(response_body.to_string()))
        .map_err(Box::new)?)
}

// ---------------------------------------------------------------------------
// Lambda handler — route dispatch
// ---------------------------------------------------------------------------

async fn handler(state: &AppState, event: Request) -> Result<Response<Body>, Error> {
    let path = event.uri().path();

    match path {
        "/stations/observations" => {
            let (lat, lon) = match parse_lat_lon(&event) {
                Ok(coords) => coords,
                Err(resp) => return Ok(resp),
            };
            handle_observations(&state.http_client, lat, lon).await
        }
        "/stations/marine" => {
            let (lat, lon) = match parse_lat_lon(&event) {
                Ok(coords) => coords,
                Err(resp) => return Ok(resp),
            };

            // Optional radius parameter (default 100km).
            let radius_km = get_query_param(&event, "radius_km")
                .and_then(|r| r.parse::<f64>().ok())
                .unwrap_or(100.0);

            handle_marine(&state.marine_stations, lat, lon, radius_km)
        }
        _ => Ok(Response::builder()
            .status(404)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(Body::from(r#"{"error":"Not found"}"#))
            .map_err(Box::new)?),
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Parse the bundled station registry at startup.
    let marine_stations: Vec<MarineStation> =
        serde_json::from_slice(NOAA_STATIONS_JSON).expect("failed to parse noaa_stations.json");

    let state = Arc::new(AppState {
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build HTTP client"),
        marine_stations,
    });

    run(service_fn(move |event: Request| {
        let state = Arc::clone(&state);
        async move { handler(&state, event).await }
    }))
    .await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use lambda_http::http;

    fn sample_stations() -> Vec<MarineStation> {
        vec![
            MarineStation {
                id: "9447130".to_string(),
                name: "Seattle".to_string(),
                station_type: "tide".to_string(),
                lat: 47.6026,
                lon: -122.3393,
            },
            MarineStation {
                id: "9446484".to_string(),
                name: "Tacoma".to_string(),
                station_type: "tide".to_string(),
                lat: 47.2690,
                lon: -122.4132,
            },
            MarineStation {
                id: "9444900".to_string(),
                name: "Port Townsend".to_string(),
                station_type: "tide".to_string(),
                lat: 48.1129,
                lon: -122.7595,
            },
            MarineStation {
                id: "46029".to_string(),
                name: "Columbia River Bar".to_string(),
                station_type: "buoy".to_string(),
                lat: 46.163,
                lon: -124.487,
            },
        ]
    }

    #[test]
    fn test_haversine_same_point() {
        let d = haversine_km(47.6, -122.3, 47.6, -122.3);
        assert!(d.abs() < 0.001);
    }

    #[test]
    fn test_haversine_seattle_to_tacoma() {
        let d = haversine_km(47.6026, -122.3393, 47.2690, -122.4132);
        assert!(d > 30.0 && d < 45.0, "Seattle-Tacoma distance: {d} km");
    }

    #[test]
    fn test_search_marine_stations_within_radius() {
        let stations = sample_stations();
        // Search from Seattle with 50km radius
        let results = search_marine_stations(&stations, 47.6062, -122.3321, 50.0);

        // Seattle and Tacoma should be within 50km; Port Townsend and Columbia
        // River Bar are farther away.
        assert!(
            results.len() >= 1,
            "Expected at least 1 station within 50km of Seattle"
        );

        // Seattle station should be first (closest)
        assert_eq!(results[0].id, "9447130");

        // All results should be within 50km
        for r in &results {
            assert!(
                r.distance_km <= 50.0,
                "Station {} at {:.2}km exceeds 50km radius",
                r.id,
                r.distance_km
            );
        }
    }

    #[test]
    fn test_search_marine_stations_sorted_by_distance() {
        let stations = sample_stations();
        // Large radius to include all stations
        let results = search_marine_stations(&stations, 47.6062, -122.3321, 500.0);

        for i in 1..results.len() {
            assert!(
                results[i].distance_km >= results[i - 1].distance_km,
                "Results not sorted: station {} ({:.2}km) before station {} ({:.2}km)",
                results[i - 1].id,
                results[i - 1].distance_km,
                results[i].id,
                results[i].distance_km
            );
        }
    }

    #[test]
    fn test_search_marine_stations_empty_for_far_location() {
        let stations = sample_stations();
        // Search from Tokyo — no stations within 100km
        let results = search_marine_stations(&stations, 35.6762, 139.6503, 100.0);
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_marine_stations_zero_radius() {
        let stations = sample_stations();
        let results = search_marine_stations(&stations, 47.6026, -122.3393, 0.0);
        // Only exact match (distance ~0) should be included
        // Due to floating point, the station at the exact coordinate has distance ~0
        // which is <= 0.0
        assert!(results.len() <= 1);
    }

    #[test]
    fn test_bundled_stations_parse() {
        let stations: Vec<MarineStation> =
            serde_json::from_slice(NOAA_STATIONS_JSON).expect("failed to parse bundled stations");
        assert!(
            stations.len() >= 10,
            "Expected at least 10 bundled stations, got {}",
            stations.len()
        );
    }

    #[test]
    fn test_search_includes_all_within_radius() {
        let stations = sample_stations();
        let lat = 47.6062;
        let lon = -122.3321;
        let radius = 500.0;

        let results = search_marine_stations(&stations, lat, lon, radius);

        // Independently verify: count how many stations are within radius
        let expected_count = stations
            .iter()
            .filter(|s| haversine_km(lat, lon, s.lat, s.lon) <= radius)
            .count();

        assert_eq!(
            results.len(),
            expected_count,
            "search_marine_stations dropped or added stations"
        );
    }

    fn build_request(path: &str, query: &str) -> Request {
        let uri = if query.is_empty() {
            format!("https://weather.popelka-woods.com{}", path)
        } else {
            format!("https://weather.popelka-woods.com{}?{}", path, query)
        };
        let req = http::Request::builder()
            .method("GET")
            .uri(&uri)
            .body(Body::Empty)
            .unwrap();
        req.into()
    }

    #[tokio::test]
    async fn test_unknown_route_returns_404() {
        let state = AppState {
            http_client: reqwest::Client::new(),
            marine_stations: vec![],
        };
        let request = build_request("/stations/unknown", "");
        let response = handler(&state, request).await.unwrap();
        assert_eq!(response.status(), 404);
    }

    #[tokio::test]
    async fn test_missing_lat_returns_400() {
        let state = AppState {
            http_client: reqwest::Client::new(),
            marine_stations: vec![],
        };
        let request = build_request("/stations/marine", "lon=-122.33");
        let response = handler(&state, request).await.unwrap();
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn test_missing_lon_returns_400() {
        let state = AppState {
            http_client: reqwest::Client::new(),
            marine_stations: vec![],
        };
        let request = build_request("/stations/marine", "lat=47.6");
        let response = handler(&state, request).await.unwrap();
        assert_eq!(response.status(), 400);
    }

    #[tokio::test]
    async fn test_marine_handler_returns_stations() {
        let state = AppState {
            http_client: reqwest::Client::new(),
            marine_stations: sample_stations(),
        };
        let request = build_request("/stations/marine", "lat=47.6062&lon=-122.3321");
        let response = handler(&state, request).await.unwrap();
        assert_eq!(response.status(), 200);

        let body = match response.body() {
            Body::Text(s) => s.clone(),
            _ => panic!("Expected text body"),
        };
        let parsed: Value = serde_json::from_str(&body).unwrap();
        assert!(parsed["stations"].is_array());
    }

    #[tokio::test]
    async fn test_marine_handler_custom_radius() {
        let state = AppState {
            http_client: reqwest::Client::new(),
            marine_stations: sample_stations(),
        };
        // Very small radius — should return fewer stations
        let request = build_request("/stations/marine", "lat=47.6062&lon=-122.3321&radius_km=1");
        let response = handler(&state, request).await.unwrap();
        assert_eq!(response.status(), 200);

        let body = match response.body() {
            Body::Text(s) => s.clone(),
            _ => panic!("Expected text body"),
        };
        let parsed: Value = serde_json::from_str(&body).unwrap();
        let stations = parsed["stations"].as_array().unwrap();
        // With 1km radius from Seattle, only the Seattle station (if any) should match
        assert!(stations.len() <= 1);
    }

    /// Feature: weather-backend-api, Property 11: Station search results are sorted by distance and within radius
    ///
    /// **Validates: Requirements 14.1, 14.2**
    mod prop_station_search {
        use super::*;
        use proptest::prelude::*;

        /// Strategy to generate a random marine station.
        fn arb_station() -> impl Strategy<Value = MarineStation> {
            (
                "[a-z]{3,6}",                // id
                "[A-Z][a-z]{3,10}",          // name
                prop_oneof!["tide", "buoy"], // type
                -90.0f64..90.0f64,           // lat
                -180.0f64..180.0f64,         // lon
            )
                .prop_map(|(id, name, station_type, lat, lon)| MarineStation {
                    id,
                    name,
                    station_type,
                    lat,
                    lon,
                })
        }

        proptest! {
            #![proptest_config(proptest::test_runner::Config::with_cases(200))]

            #[test]
            fn prop_station_search_sorted_and_within_radius(
                search_lat in -90.0f64..90.0f64,
                search_lon in -180.0f64..180.0f64,
                stations in proptest::collection::vec(arb_station(), 0..30),
                radius_km in 1.0f64..500.0f64,
            ) {
                let results = search_marine_stations(&stations, search_lat, search_lon, radius_km);

                // Property (a): All returned stations have distance ≤ radius
                for r in &results {
                    // Recompute the raw distance (before rounding) to check the
                    // radius constraint. The function rounds distance_km to 2
                    // decimal places, so we check the raw Haversine value.
                    let raw_dist = haversine_km(search_lat, search_lon, r.latitude, r.longitude);
                    prop_assert!(
                        raw_dist <= radius_km + 1e-9,
                        "Station {} has raw distance {:.4}km which exceeds radius {:.2}km",
                        r.id, raw_dist, radius_km
                    );
                }

                // Property (b): Results are sorted in ascending order of distance
                for i in 1..results.len() {
                    prop_assert!(
                        results[i].distance_km >= results[i - 1].distance_km,
                        "Results not sorted: [{i}] {:.2}km < [{prev}] {:.2}km",
                        results[i].distance_km,
                        results[i - 1].distance_km,
                        prev = i - 1
                    );
                }

                // Property (c): Every station within the radius is included
                // (no valid stations dropped)
                let expected_count = stations
                    .iter()
                    .filter(|s| haversine_km(search_lat, search_lon, s.lat, s.lon) <= radius_km)
                    .count();

                prop_assert_eq!(
                    results.len(),
                    expected_count,
                    "Expected {} stations within {:.2}km but got {}",
                    expected_count, radius_km, results.len()
                );
            }
        }
    }
}
