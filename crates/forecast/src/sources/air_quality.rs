use serde_json::Value;

// ---------------------------------------------------------------------------
// Air Quality API URL builder
// ---------------------------------------------------------------------------

/// Air quality variables requested from the Open-Meteo Air Quality API.
const AIR_QUALITY_VARIABLES: [&str; 3] = ["us_aqi", "pm2_5", "pm10"];

/// Builds the Open-Meteo Air Quality API URL for the given coordinates.
///
/// Requests US AQI, PM2.5, and PM10 with 7 forecast days and 12 past hours.
pub fn build_air_quality_url(lat: f64, lon: f64) -> String {
    let variables = AIR_QUALITY_VARIABLES.join(",");
    format!(
        "https://air-quality-api.open-meteo.com/v1/air-quality\
         ?latitude={lat}&longitude={lon}\
         &hourly={variables}\
         &forecast_days=7\
         &past_hours=12"
    )
}

// ---------------------------------------------------------------------------
// AirQualityFetcher
// ---------------------------------------------------------------------------

/// Fetches and parses air quality data from the Open-Meteo Air Quality API.
pub struct AirQualityFetcher;

impl AirQualityFetcher {
    pub fn source_id() -> &'static str {
        "air_quality"
    }

    pub fn ttl_secs() -> u64 {
        3600
    }

    pub fn is_cacheable() -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Parsed air quality data
// ---------------------------------------------------------------------------

/// Parsed air quality data from the Open-Meteo Air Quality API.
#[derive(Debug, Clone)]
pub struct AirQualityData {
    /// ISO 8601 time strings for each hourly step.
    pub times: Vec<String>,
    /// US Air Quality Index values.
    pub us_aqi: Vec<Option<f64>>,
    /// PM2.5 concentration in µg/m³.
    pub pm2_5: Vec<Option<f64>>,
    /// PM10 concentration in µg/m³.
    pub pm10: Vec<Option<f64>>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parses the raw JSON response from the Open-Meteo Air Quality API.
pub fn parse_air_quality_response(raw: &[u8]) -> Result<AirQualityData, String> {
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

    Ok(AirQualityData {
        times,
        us_aqi: extract_array("us_aqi"),
        pm2_5: extract_array("pm2_5"),
        pm10: extract_array("pm10"),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_air_quality_url() {
        let url = build_air_quality_url(47.61, -122.33);
        assert!(url.starts_with(
            "https://air-quality-api.open-meteo.com/v1/air-quality?"
        ));
        assert!(url.contains("latitude=47.61"));
        assert!(url.contains("longitude=-122.33"));
        assert!(url.contains("forecast_days=7"));
        assert!(url.contains("past_hours=12"));
        assert!(url.contains("us_aqi"));
        assert!(url.contains("pm2_5"));
        assert!(url.contains("pm10"));
    }

    #[test]
    fn test_build_air_quality_url_negative_coords() {
        let url = build_air_quality_url(-33.87, 151.21);
        assert!(url.contains("latitude=-33.87"));
        assert!(url.contains("longitude=151.21"));
    }

    #[test]
    fn test_source_metadata() {
        assert_eq!(AirQualityFetcher::source_id(), "air_quality");
        assert_eq!(AirQualityFetcher::ttl_secs(), 3600);
        assert!(AirQualityFetcher::is_cacheable());
    }

    fn synthetic_air_quality_json() -> Vec<u8> {
        let json = serde_json::json!({
            "hourly": {
                "time": [
                    "2026-04-24T00:00",
                    "2026-04-24T01:00",
                    "2026-04-24T02:00"
                ],
                "us_aqi": [42, 45, 50],
                "pm2_5": [10.2, 11.5, 12.8],
                "pm10": [18.0, 20.0, 22.5]
            }
        });
        serde_json::to_vec(&json).unwrap()
    }

    #[test]
    fn test_parse_air_quality_response_times() {
        let raw = synthetic_air_quality_json();
        let data = parse_air_quality_response(&raw).unwrap();
        assert_eq!(data.times.len(), 3);
        assert_eq!(data.times[0], "2026-04-24T00:00");
    }

    #[test]
    fn test_parse_air_quality_response_data() {
        let raw = synthetic_air_quality_json();
        let data = parse_air_quality_response(&raw).unwrap();
        assert_eq!(data.us_aqi.len(), 3);
        assert_eq!(data.us_aqi[0], Some(42.0));
        assert_eq!(data.us_aqi[2], Some(50.0));
        assert_eq!(data.pm2_5[1], Some(11.5));
        assert_eq!(data.pm10[2], Some(22.5));
    }

    #[test]
    fn test_parse_air_quality_response_null_handling() {
        let json = serde_json::json!({
            "hourly": {
                "time": ["2026-04-24T00:00"],
                "us_aqi": [null],
                "pm2_5": [null],
                "pm10": [null]
            }
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_air_quality_response(&raw).unwrap();
        assert_eq!(data.us_aqi[0], None);
        assert_eq!(data.pm2_5[0], None);
        assert_eq!(data.pm10[0], None);
    }

    #[test]
    fn test_parse_air_quality_response_missing_hourly() {
        let json = serde_json::json!({"daily": {}});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_air_quality_response(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("hourly"));
    }

    #[test]
    fn test_parse_air_quality_response_missing_time() {
        let json = serde_json::json!({"hourly": {"us_aqi": [42]}});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_air_quality_response(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("time"));
    }

    #[test]
    fn test_parse_air_quality_response_partial_data() {
        let json = serde_json::json!({
            "hourly": {
                "time": ["2026-04-24T00:00"],
                "us_aqi": [42]
            }
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_air_quality_response(&raw).unwrap();
        assert_eq!(data.us_aqi.len(), 1);
        assert!(data.pm2_5.is_empty());
        assert!(data.pm10.is_empty());
    }
}
