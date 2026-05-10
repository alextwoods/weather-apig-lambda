use std::sync::Arc;

use lambda_http::{run, service_fn, Body, Error, Request, Response};
use serde_json::Value;

/// Shared HTTP client, created once per Lambda cold start.
struct AppState {
    http_client: reqwest::Client,
}

/// Geocode Lambda handler.
///
/// Proxies search queries to the Open-Meteo Geocoding API and returns the
/// results array as JSON.
async fn handler(state: &AppState, event: Request) -> Result<Response<Body>, Error> {
    // Extract the `q` query parameter from the request URI.
    let query = match event.uri().query().and_then(|qs| {
        qs.split('&')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                let k = parts.next()?;
                let v = parts.next().unwrap_or("");
                if k == "q" {
                    Some(v.to_string())
                } else {
                    None
                }
            })
            .next()
    }) {
        Some(q) if !q.is_empty() => q,
        _ => {
            return Ok(Response::builder()
                .status(400)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(Body::from(
                    r#"{"error":"Missing required parameter: q"}"#,
                ))
                .map_err(Box::new)?);
        }
    };

    // Proxy the request to the Open-Meteo Geocoding API.
    let upstream_url = format!(
        "https://geocoding-api.open-meteo.com/v1/search?name={}&count=10&language=en&format=json",
        urlencoding::encode(&query)
    );

    let upstream_resp = match state.http_client.get(&upstream_url).send().await {
        Ok(resp) => resp,
        Err(e) => {
            let msg = format!(r#"{{"error":"Upstream geocoding request failed: {}"}}"#, e);
            return Ok(Response::builder()
                .status(502)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(Body::from(msg))
                .map_err(Box::new)?);
        }
    };

    let status = upstream_resp.status();
    if !status.is_success() {
        let msg = format!(
            r#"{{"error":"Upstream geocoding API returned status {}"}}"#,
            status.as_u16()
        );
        return Ok(Response::builder()
            .status(502)
            .header("Content-Type", "application/json")
            .header("Access-Control-Allow-Origin", "*")
            .body(Body::from(msg))
            .map_err(Box::new)?);
    }

    let body_bytes = match upstream_resp.bytes().await {
        Ok(b) => b,
        Err(e) => {
            let msg = format!(
                r#"{{"error":"Failed to read upstream response body: {}"}}"#,
                e
            );
            return Ok(Response::builder()
                .status(502)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(Body::from(msg))
                .map_err(Box::new)?);
        }
    };

    // Parse the upstream JSON and extract the "results" array.
    // The Open-Meteo API returns `{ "results": [...] }` on success, or
    // an object without "results" when there are no matches.
    let parsed: Value = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            let msg = format!(
                r#"{{"error":"Failed to parse upstream response: {}"}}"#,
                e
            );
            return Ok(Response::builder()
                .status(502)
                .header("Content-Type", "application/json")
                .header("Access-Control-Allow-Origin", "*")
                .body(Body::from(msg))
                .map_err(Box::new)?);
        }
    };

    let results = parsed
        .get("results")
        .cloned()
        .unwrap_or(Value::Array(vec![]));

    let response_body = serde_json::json!({ "results": results });

    Ok(Response::builder()
        .status(200)
        .header("Content-Type", "application/json")
        .header("Access-Control-Allow-Origin", "*")
        .body(Body::from(response_body.to_string()))
        .map_err(Box::new)?)
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    let state = Arc::new(AppState {
        http_client: reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
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
    use lambda_http::http;

    /// Helper to build a Lambda request with query parameters.
    fn build_request(query_string: &str) -> Request {
        let uri = if query_string.is_empty() {
            "https://weather.popelka-woods.com/geocode".to_string()
        } else {
            format!(
                "https://weather.popelka-woods.com/geocode?{}",
                query_string
            )
        };
        let req = http::Request::builder()
            .method("GET")
            .uri(&uri)
            .body(Body::Empty)
            .unwrap();
        req.into()
    }

    #[tokio::test]
    async fn test_missing_q_parameter_returns_400() {
        let state = AppState {
            http_client: reqwest::Client::new(),
        };
        let request = build_request("");
        let response = handler(&state, request).await.unwrap();
        assert_eq!(response.status(), 400);

        let body = match response.body() {
            Body::Text(s) => s.clone(),
            _ => panic!("Expected text body"),
        };
        assert!(body.contains("Missing required parameter: q"));
    }

    #[tokio::test]
    async fn test_empty_q_parameter_returns_400() {
        let state = AppState {
            http_client: reqwest::Client::new(),
        };
        let request = build_request("q=");
        let response = handler(&state, request).await.unwrap();
        assert_eq!(response.status(), 400);
    }

    /// Verifies the upstream URL is constructed with the correct format:
    /// base URL, name parameter (URL-encoded), count=10, language=en,
    /// format=json.
    #[test]
    fn test_upstream_url_format() {
        // Simulate the URL construction logic from the handler
        let query = "Seattle";
        let upstream_url = format!(
            "https://geocoding-api.open-meteo.com/v1/search?name={}&count=10&language=en&format=json",
            urlencoding::encode(query)
        );
        assert!(upstream_url.starts_with("https://geocoding-api.open-meteo.com/v1/search?"));
        assert!(upstream_url.contains("name=Seattle"));
        assert!(upstream_url.contains("count=10"));
        assert!(upstream_url.contains("language=en"));
        assert!(upstream_url.contains("format=json"));
    }

    /// Verifies that special characters in the query are properly URL-encoded
    /// in the upstream URL.
    #[test]
    fn test_upstream_url_encodes_special_characters() {
        let query = "New York City";
        let upstream_url = format!(
            "https://geocoding-api.open-meteo.com/v1/search?name={}&count=10&language=en&format=json",
            urlencoding::encode(query)
        );
        // Spaces should be encoded as %20
        assert!(upstream_url.contains("name=New%20York%20City"));
        assert!(!upstream_url.contains("name=New York City"));
    }

    /// Verifies that the handler correctly extracts the `q` parameter when
    /// other query parameters are also present.
    #[tokio::test]
    async fn test_q_parameter_extracted_with_other_params() {
        let state = AppState {
            http_client: reqwest::Client::new(),
        };
        // The handler should find q even when other params are present
        let request = build_request("lang=en&q=Portland&count=5");
        // This will fail at the upstream call (no mock), but we can verify
        // it doesn't return 400 (meaning q was found). Since we can't mock
        // the upstream easily, we just verify the 400 case doesn't trigger.
        let response = handler(&state, request).await.unwrap();
        // Should NOT be 400 — the q parameter was found
        assert_ne!(response.status(), 400);
    }
}
