use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// NOAA CO-OPS Water Temperature API URL builder
// ---------------------------------------------------------------------------

/// Builds the NOAA CO-OPS API URL for the latest water temperature reading
/// at the given station.
///
/// Uses `product=water_temperature`, `date=latest`, `units=metric`,
/// `time_zone=gmt`, and `format=json`.
pub fn build_water_temp_url(station_id: &str) -> String {
    format!(
        "https://api.tidesandcurrents.noaa.gov/api/prod/datagetter\
         ?station={station_id}\
         &product=water_temperature\
         &date=latest\
         &units=metric\
         &time_zone=gmt\
         &format=json"
    )
}

// ---------------------------------------------------------------------------
// NoaaWaterTempFetcher
// ---------------------------------------------------------------------------

/// Fetches and parses water temperature data from the NOAA CO-OPS API.
///
/// Water temperature data is cached in DynamoDB with a 3600-second
/// (1-hour) TTL. Water temperature changes slowly (fractions of a degree
/// per hour), so hourly freshness is sufficient.
pub struct NoaaWaterTempFetcher;

impl NoaaWaterTempFetcher {
    pub fn source_id() -> &'static str {
        "noaa_water_temp"
    }

    pub fn is_cacheable() -> bool {
        true
    }

    pub fn ttl_secs() -> u64 {
        3600
    }
}

// ---------------------------------------------------------------------------
// Parsed water temperature data
// ---------------------------------------------------------------------------

/// Parsed water temperature data from the NOAA CO-OPS API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WaterTemperatureData {
    /// NOAA station identifier (e.g., "9447130").
    pub station_id: String,
    /// Human-readable station name (e.g., "Seattle").
    pub station_name: String,
    /// Latest water temperature in degrees Celsius, or `None` if unavailable.
    pub temperature_celsius: Option<f64>,
    /// Timestamp of the reading (e.g., "2026-04-24 14:00"), or `None` if
    /// unavailable.
    pub timestamp: Option<String>,
}

// ---------------------------------------------------------------------------
// Serialization / deserialization
// ---------------------------------------------------------------------------

/// Serializes `WaterTemperatureData` to JSON bytes for caching.
pub fn serialize_water_temperature(
    data: &WaterTemperatureData,
) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(data)
}

/// Deserializes `WaterTemperatureData` from JSON bytes (cache read).
pub fn deserialize_water_temperature(
    bytes: &[u8],
) -> Result<WaterTemperatureData, serde_json::Error> {
    serde_json::from_slice(bytes)
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parses the raw JSON response from the NOAA CO-OPS water temperature API.
///
/// Expected response format:
/// ```json
/// {
///   "data": [
///     { "t": "2026-04-24 14:00", "v": "10.5" }
///   ]
/// }
/// ```
///
/// The `station_id` and `station_name` are passed in because the NOAA
/// response does not always include them in the data payload.
pub fn parse_water_temp_response(
    raw: &[u8],
    station_id: &str,
    station_name: &str,
) -> Result<WaterTemperatureData, String> {
    let root: Value = serde_json::from_slice(raw).map_err(|e| format!("JSON parse error: {e}"))?;

    // Check for an error response from NOAA
    if let Some(err) = root.get("error") {
        let msg = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown NOAA error");
        return Err(format!("NOAA API error: {msg}"));
    }

    let data = root
        .get("data")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing or invalid 'data' array".to_string())?;

    // Extract the most recent reading (first element)
    let (temperature_celsius, timestamp) = if let Some(entry) = data.first() {
        let temp = entry
            .get("v")
            .and_then(|v| v.as_str())
            .and_then(|s| s.parse::<f64>().ok());

        let time = entry
            .get("t")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        (temp, time)
    } else {
        (None, None)
    };

    Ok(WaterTemperatureData {
        station_id: station_id.to_string(),
        station_name: station_name.to_string(),
        temperature_celsius,
        timestamp,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------
    // URL builder tests
    // -------------------------------------------------------------------

    #[test]
    fn test_build_water_temp_url() {
        let url = build_water_temp_url("9447130");
        assert!(url.starts_with(
            "https://api.tidesandcurrents.noaa.gov/api/prod/datagetter?"
        ));
        assert!(url.contains("station=9447130"));
        assert!(url.contains("product=water_temperature"));
        assert!(url.contains("date=latest"));
        assert!(url.contains("units=metric"));
        assert!(url.contains("time_zone=gmt"));
        assert!(url.contains("format=json"));
    }

    #[test]
    fn test_build_water_temp_url_different_station() {
        let url = build_water_temp_url("9446484");
        assert!(url.contains("station=9446484"));
    }

    // -------------------------------------------------------------------
    // Source metadata tests
    // -------------------------------------------------------------------

    #[test]
    fn test_source_metadata() {
        assert_eq!(NoaaWaterTempFetcher::source_id(), "noaa_water_temp");
        assert!(NoaaWaterTempFetcher::is_cacheable());
        assert_eq!(NoaaWaterTempFetcher::ttl_secs(), 3600);
    }

    // -------------------------------------------------------------------
    // Parsing tests
    // -------------------------------------------------------------------

    fn synthetic_water_temp_json() -> Vec<u8> {
        let json = serde_json::json!({
            "data": [
                { "t": "2026-04-24 14:00", "v": "10.5" }
            ]
        });
        serde_json::to_vec(&json).unwrap()
    }

    #[test]
    fn test_parse_water_temp_response() {
        let raw = synthetic_water_temp_json();
        let data = parse_water_temp_response(&raw, "9447130", "Seattle").unwrap();
        assert_eq!(data.station_id, "9447130");
        assert_eq!(data.station_name, "Seattle");
        assert_eq!(data.temperature_celsius, Some(10.5));
        assert_eq!(data.timestamp, Some("2026-04-24 14:00".to_string()));
    }

    #[test]
    fn test_parse_water_temp_response_multiple_entries() {
        // Should use the first (most recent) entry
        let json = serde_json::json!({
            "data": [
                { "t": "2026-04-24 14:00", "v": "10.5" },
                { "t": "2026-04-24 13:54", "v": "10.3" }
            ]
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_water_temp_response(&raw, "9447130", "Seattle").unwrap();
        assert_eq!(data.temperature_celsius, Some(10.5));
        assert_eq!(data.timestamp, Some("2026-04-24 14:00".to_string()));
    }

    #[test]
    fn test_parse_water_temp_response_empty_data() {
        let json = serde_json::json!({ "data": [] });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_water_temp_response(&raw, "9447130", "Seattle").unwrap();
        assert_eq!(data.temperature_celsius, None);
        assert_eq!(data.timestamp, None);
    }

    #[test]
    fn test_parse_water_temp_response_missing_data() {
        let json = serde_json::json!({ "metadata": {} });
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_water_temp_response(&raw, "9447130", "Seattle");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("data"));
    }

    #[test]
    fn test_parse_water_temp_response_error_response() {
        let json = serde_json::json!({
            "error": {
                "message": "No data was found"
            }
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_water_temp_response(&raw, "9447130", "Seattle");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("No data was found"));
    }

    #[test]
    fn test_parse_water_temp_response_invalid_json() {
        let raw = b"not json";
        let result = parse_water_temp_response(raw, "9447130", "Seattle");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JSON parse error"));
    }

    #[test]
    fn test_parse_water_temp_response_non_numeric_value() {
        let json = serde_json::json!({
            "data": [
                { "t": "2026-04-24 14:00", "v": "" }
            ]
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_water_temp_response(&raw, "9447130", "Seattle").unwrap();
        // Empty string can't be parsed as f64 → None
        assert_eq!(data.temperature_celsius, None);
        assert_eq!(data.timestamp, Some("2026-04-24 14:00".to_string()));
    }

    #[test]
    fn test_parse_water_temp_response_negative_temp() {
        let json = serde_json::json!({
            "data": [
                { "t": "2026-01-15 08:00", "v": "-1.2" }
            ]
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_water_temp_response(&raw, "9447130", "Seattle").unwrap();
        assert_eq!(data.temperature_celsius, Some(-1.2));
    }

    // -------------------------------------------------------------------
    // Serialization round-trip tests
    // -------------------------------------------------------------------

    #[test]
    fn test_serialize_deserialize_water_temp_round_trip() {
        let original = WaterTemperatureData {
            station_id: "9447130".to_string(),
            station_name: "Seattle".to_string(),
            temperature_celsius: Some(10.5),
            timestamp: Some("2026-04-24 14:00".to_string()),
        };

        let bytes = serialize_water_temperature(&original).unwrap();
        let restored = deserialize_water_temperature(&bytes).unwrap();

        assert_eq!(restored.station_id, original.station_id);
        assert_eq!(restored.station_name, original.station_name);
        assert_eq!(restored.temperature_celsius, original.temperature_celsius);
        assert_eq!(restored.timestamp, original.timestamp);
    }

    #[test]
    fn test_serialize_deserialize_water_temp_none_values() {
        let original = WaterTemperatureData {
            station_id: "9447130".to_string(),
            station_name: "Seattle".to_string(),
            temperature_celsius: None,
            timestamp: None,
        };

        let bytes = serialize_water_temperature(&original).unwrap();
        let restored = deserialize_water_temperature(&bytes).unwrap();

        assert_eq!(restored.station_id, original.station_id);
        assert_eq!(restored.temperature_celsius, None);
        assert_eq!(restored.timestamp, None);
    }
}
