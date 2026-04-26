use serde_json::Value;

// ---------------------------------------------------------------------------
// UV API URL builder
// ---------------------------------------------------------------------------

/// UV variables requested from the Open-Meteo Forecast API.
const UV_VARIABLES: [&str; 2] = ["uv_index", "uv_index_clear_sky"];

/// Builds the Open-Meteo Forecast API URL for UV data at the given
/// coordinates.
///
/// Requests UV index and UV index clear sky with 16 forecast days and 12
/// past hours.
pub fn build_uv_url(lat: f64, lon: f64) -> String {
    let variables = UV_VARIABLES.join(",");
    format!(
        "https://api.open-meteo.com/v1/forecast\
         ?latitude={lat}&longitude={lon}\
         &hourly={variables}\
         &forecast_days=16\
         &past_hours=12"
    )
}

// ---------------------------------------------------------------------------
// UvFetcher
// ---------------------------------------------------------------------------

/// Fetches and parses UV forecast data from the Open-Meteo Forecast API.
pub struct UvFetcher;

impl UvFetcher {
    pub fn source_id() -> &'static str {
        "uv"
    }

    pub fn ttl_secs() -> u64 {
        3600
    }

    pub fn is_cacheable() -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Parsed UV data
// ---------------------------------------------------------------------------

/// Parsed UV forecast data from the Open-Meteo Forecast API.
#[derive(Debug, Clone)]
pub struct UvData {
    /// ISO 8601 time strings for each hourly step.
    pub times: Vec<String>,
    /// UV index values.
    pub uv_index: Vec<Option<f64>>,
    /// UV index under clear sky conditions.
    pub uv_index_clear_sky: Vec<Option<f64>>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parses the raw JSON response from the Open-Meteo Forecast API for UV data.
pub fn parse_uv_response(raw: &[u8]) -> Result<UvData, String> {
    let root: Value = serde_json::from_slice(raw).map_err(|e| format!("JSON parse error: {e}"))?;

    let hourly = root
        .get("hourly")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "missing or invalid 'hourly' object".to_string())?;

    let times: Vec<String> = hourly
        .get("time")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing 'time' array in hourly object".to_string())?
        .iter()
        .map(|v| v.as_str().unwrap_or_default().to_string())
        .collect();

    let extract_array = |key: &str| -> Vec<Option<f64>> {
        hourly
            .get(key)
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|v| if v.is_null() { None } else { v.as_f64() })
                    .collect()
            })
            .unwrap_or_default()
    };

    Ok(UvData {
        times,
        uv_index: extract_array("uv_index"),
        uv_index_clear_sky: extract_array("uv_index_clear_sky"),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_uv_url() {
        let url = build_uv_url(47.61, -122.33);
        assert!(url.starts_with("https://api.open-meteo.com/v1/forecast?"));
        assert!(url.contains("latitude=47.61"));
        assert!(url.contains("longitude=-122.33"));
        assert!(url.contains("forecast_days=16"));
        assert!(url.contains("past_hours=12"));
        assert!(url.contains("uv_index"));
        assert!(url.contains("uv_index_clear_sky"));
    }

    #[test]
    fn test_build_uv_url_negative_coords() {
        let url = build_uv_url(-33.87, 151.21);
        assert!(url.contains("latitude=-33.87"));
        assert!(url.contains("longitude=151.21"));
    }

    #[test]
    fn test_source_metadata() {
        assert_eq!(UvFetcher::source_id(), "uv");
        assert_eq!(UvFetcher::ttl_secs(), 3600);
        assert!(UvFetcher::is_cacheable());
    }

    fn synthetic_uv_json() -> Vec<u8> {
        let json = serde_json::json!({
            "hourly": {
                "time": [
                    "2026-04-24T00:00",
                    "2026-04-24T01:00",
                    "2026-04-24T02:00"
                ],
                "uv_index": [0.0, 0.5, 2.3],
                "uv_index_clear_sky": [0.0, 0.8, 3.1]
            }
        });
        serde_json::to_vec(&json).unwrap()
    }

    #[test]
    fn test_parse_uv_response_times() {
        let raw = synthetic_uv_json();
        let data = parse_uv_response(&raw).unwrap();
        assert_eq!(data.times.len(), 3);
        assert_eq!(data.times[0], "2026-04-24T00:00");
    }

    #[test]
    fn test_parse_uv_response_data() {
        let raw = synthetic_uv_json();
        let data = parse_uv_response(&raw).unwrap();
        assert_eq!(data.uv_index.len(), 3);
        assert_eq!(data.uv_index[0], Some(0.0));
        assert_eq!(data.uv_index[2], Some(2.3));
        assert_eq!(data.uv_index_clear_sky[1], Some(0.8));
        assert_eq!(data.uv_index_clear_sky[2], Some(3.1));
    }

    #[test]
    fn test_parse_uv_response_null_handling() {
        let json = serde_json::json!({
            "hourly": {
                "time": ["2026-04-24T00:00"],
                "uv_index": [null],
                "uv_index_clear_sky": [null]
            }
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_uv_response(&raw).unwrap();
        assert_eq!(data.uv_index[0], None);
        assert_eq!(data.uv_index_clear_sky[0], None);
    }

    #[test]
    fn test_parse_uv_response_missing_hourly() {
        let json = serde_json::json!({"daily": {}});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_uv_response(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("hourly"));
    }

    #[test]
    fn test_parse_uv_response_missing_time() {
        let json = serde_json::json!({"hourly": {"uv_index": [1.0]}});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_uv_response(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("time"));
    }

    #[test]
    fn test_parse_uv_response_partial_data() {
        let json = serde_json::json!({
            "hourly": {
                "time": ["2026-04-24T00:00"],
                "uv_index": [5.0]
            }
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_uv_response(&raw).unwrap();
        assert_eq!(data.uv_index.len(), 1);
        assert!(data.uv_index_clear_sky.is_empty());
    }
}
