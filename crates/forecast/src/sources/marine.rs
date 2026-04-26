use serde_json::Value;

// ---------------------------------------------------------------------------
// Marine API URL builder
// ---------------------------------------------------------------------------

/// Marine variables requested from the Open-Meteo Marine API.
const MARINE_VARIABLES: [&str; 4] = [
    "wave_height",
    "wave_period",
    "wave_direction",
    "sea_surface_temperature",
];

/// Builds the Open-Meteo Marine API URL for the given coordinates.
///
/// Requests wave height, wave period, wave direction, and sea surface
/// temperature with 7 forecast days and 12 past hours.
pub fn build_marine_url(lat: f64, lon: f64) -> String {
    let variables = MARINE_VARIABLES.join(",");
    format!(
        "https://marine-api.open-meteo.com/v1/marine\
         ?latitude={lat}&longitude={lon}\
         &hourly={variables}\
         &forecast_days=7\
         &past_hours=12"
    )
}

// ---------------------------------------------------------------------------
// MarineFetcher
// ---------------------------------------------------------------------------

/// Fetches and parses marine forecast data from the Open-Meteo Marine API.
pub struct MarineFetcher;

impl MarineFetcher {
    pub fn source_id() -> &'static str {
        "marine"
    }

    pub fn ttl_secs() -> u64 {
        3600
    }

    pub fn is_cacheable() -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Parsed marine data
// ---------------------------------------------------------------------------

/// Parsed marine forecast data from the Open-Meteo Marine API.
#[derive(Debug, Clone)]
pub struct MarineData {
    /// ISO 8601 time strings for each hourly step.
    pub times: Vec<String>,
    /// Significant wave height in metres.
    pub wave_height: Vec<Option<f64>>,
    /// Wave period in seconds.
    pub wave_period: Vec<Option<f64>>,
    /// Wave direction in degrees.
    pub wave_direction: Vec<Option<f64>>,
    /// Sea surface temperature in °C (often null for coastal/inland locations).
    pub sea_surface_temperature: Vec<Option<f64>>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parses the raw JSON response from the Open-Meteo Marine API.
pub fn parse_marine_response(raw: &[u8]) -> Result<MarineData, String> {
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

    Ok(MarineData {
        times,
        wave_height: extract_array("wave_height"),
        wave_period: extract_array("wave_period"),
        wave_direction: extract_array("wave_direction"),
        sea_surface_temperature: extract_array("sea_surface_temperature"),
    })
}

// ---------------------------------------------------------------------------
// SST null detection
// ---------------------------------------------------------------------------

/// Returns `true` if all sea surface temperature values in the marine data
/// are `None` (null).
///
/// This is used to decide whether to trigger conditional supplementary
/// fetches from NOAA CO-OPS or ECCC CIOPS for water temperature data.
pub fn all_sst_null(marine_data: &MarineData) -> bool {
    marine_data
        .sea_surface_temperature
        .iter()
        .all(|v| v.is_none())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_marine_url() {
        let url = build_marine_url(47.61, -122.33);
        assert!(url.starts_with("https://marine-api.open-meteo.com/v1/marine?"));
        assert!(url.contains("latitude=47.61"));
        assert!(url.contains("longitude=-122.33"));
        assert!(url.contains("forecast_days=7"));
        assert!(url.contains("past_hours=12"));
        assert!(url.contains("wave_height"));
        assert!(url.contains("wave_period"));
        assert!(url.contains("wave_direction"));
        assert!(url.contains("sea_surface_temperature"));
    }

    #[test]
    fn test_build_marine_url_negative_coords() {
        let url = build_marine_url(-33.87, 151.21);
        assert!(url.contains("latitude=-33.87"));
        assert!(url.contains("longitude=151.21"));
    }

    #[test]
    fn test_source_metadata() {
        assert_eq!(MarineFetcher::source_id(), "marine");
        assert_eq!(MarineFetcher::ttl_secs(), 3600);
        assert!(MarineFetcher::is_cacheable());
    }

    fn synthetic_marine_json() -> Vec<u8> {
        let json = serde_json::json!({
            "hourly": {
                "time": ["2026-04-24T00:00", "2026-04-24T01:00", "2026-04-24T02:00"],
                "wave_height": [1.2, 1.3, 1.1],
                "wave_period": [6.0, 6.5, 5.8],
                "wave_direction": [220.0, 225.0, 210.0],
                "sea_surface_temperature": [10.5, 10.6, 10.4]
            }
        });
        serde_json::to_vec(&json).unwrap()
    }

    fn synthetic_marine_null_sst_json() -> Vec<u8> {
        let json = serde_json::json!({
            "hourly": {
                "time": ["2026-04-24T00:00", "2026-04-24T01:00"],
                "wave_height": [1.2, 1.3],
                "wave_period": [6.0, 6.5],
                "wave_direction": [220.0, 225.0],
                "sea_surface_temperature": [null, null]
            }
        });
        serde_json::to_vec(&json).unwrap()
    }

    #[test]
    fn test_parse_marine_response_times() {
        let raw = synthetic_marine_json();
        let data = parse_marine_response(&raw).unwrap();
        assert_eq!(data.times.len(), 3);
        assert_eq!(data.times[0], "2026-04-24T00:00");
    }

    #[test]
    fn test_parse_marine_response_wave_data() {
        let raw = synthetic_marine_json();
        let data = parse_marine_response(&raw).unwrap();
        assert_eq!(data.wave_height.len(), 3);
        assert_eq!(data.wave_height[0], Some(1.2));
        assert_eq!(data.wave_period[1], Some(6.5));
        assert_eq!(data.wave_direction[2], Some(210.0));
    }

    #[test]
    fn test_parse_marine_response_sst() {
        let raw = synthetic_marine_json();
        let data = parse_marine_response(&raw).unwrap();
        assert_eq!(data.sea_surface_temperature.len(), 3);
        assert_eq!(data.sea_surface_temperature[0], Some(10.5));
    }

    #[test]
    fn test_parse_marine_response_null_sst() {
        let raw = synthetic_marine_null_sst_json();
        let data = parse_marine_response(&raw).unwrap();
        assert_eq!(data.sea_surface_temperature.len(), 2);
        assert_eq!(data.sea_surface_temperature[0], None);
        assert_eq!(data.sea_surface_temperature[1], None);
    }

    #[test]
    fn test_parse_marine_response_missing_hourly() {
        let json = serde_json::json!({"daily": {}});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_marine_response(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("hourly"));
    }

    #[test]
    fn test_parse_marine_response_missing_time() {
        let json = serde_json::json!({"hourly": {"wave_height": [1.0]}});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_marine_response(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("time"));
    }

    #[test]
    fn test_all_sst_null_true() {
        let raw = synthetic_marine_null_sst_json();
        let data = parse_marine_response(&raw).unwrap();
        assert!(all_sst_null(&data));
    }

    #[test]
    fn test_all_sst_null_false() {
        let raw = synthetic_marine_json();
        let data = parse_marine_response(&raw).unwrap();
        assert!(!all_sst_null(&data));
    }

    #[test]
    fn test_all_sst_null_empty() {
        let data = MarineData {
            times: vec![],
            wave_height: vec![],
            wave_period: vec![],
            wave_direction: vec![],
            sea_surface_temperature: vec![],
        };
        // Empty iterator → all() returns true
        assert!(all_sst_null(&data));
    }

    #[test]
    fn test_all_sst_null_mixed() {
        let data = MarineData {
            times: vec!["t1".to_string(), "t2".to_string()],
            wave_height: vec![Some(1.0), Some(1.0)],
            wave_period: vec![Some(6.0), Some(6.0)],
            wave_direction: vec![Some(200.0), Some(200.0)],
            sea_surface_temperature: vec![None, Some(10.0)],
        };
        assert!(!all_sst_null(&data));
    }

    #[test]
    fn test_parse_marine_response_partial_data() {
        // Response with some variables missing — should return empty vecs
        let json = serde_json::json!({
            "hourly": {
                "time": ["2026-04-24T00:00"],
                "wave_height": [1.5]
            }
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_marine_response(&raw).unwrap();
        assert_eq!(data.wave_height.len(), 1);
        assert!(data.wave_period.is_empty());
        assert!(data.wave_direction.is_empty());
        assert!(data.sea_surface_temperature.is_empty());
    }
}
