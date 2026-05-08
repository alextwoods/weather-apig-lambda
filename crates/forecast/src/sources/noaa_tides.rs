use serde::{Deserialize, Serialize};
use serde_json::Value;

// ---------------------------------------------------------------------------
// NOAA CO-OPS Tide Predictions API URL builder
// ---------------------------------------------------------------------------

/// Builds the NOAA CO-OPS API URL for tide predictions at the given station
/// over the specified date range.
///
/// Uses `product=predictions`, `datum=MLLW`, `units=metric`, `interval=6`
/// (6-minute intervals for high resolution), `time_zone=gmt`, and
/// `format=json`.
///
/// `begin_date` and `end_date` should be formatted as `YYYYMMDD`.
pub fn build_tides_url(station_id: &str, begin_date: &str, end_date: &str) -> String {
    format!(
        "https://api.tidesandcurrents.noaa.gov/api/prod/datagetter\
         ?station={station_id}\
         &product=predictions\
         &begin_date={begin_date}\
         &end_date={end_date}\
         &datum=MLLW\
         &units=metric\
         &time_zone=gmt\
         &interval=6\
         &format=json"
    )
}

// ---------------------------------------------------------------------------
// NoaaTidesFetcher
// ---------------------------------------------------------------------------

/// Fetches and parses tide prediction data from the NOAA CO-OPS API.
///
/// Tide predictions are cached in DynamoDB with a 3600-second (1-hour) TTL.
pub struct NoaaTidesFetcher;

impl NoaaTidesFetcher {
    pub fn source_id() -> &'static str {
        "noaa_tides"
    }

    pub fn is_cacheable() -> bool {
        true
    }

    pub fn ttl_secs() -> u64 {
        3600
    }
}

// ---------------------------------------------------------------------------
// Parsed tide data
// ---------------------------------------------------------------------------

/// A single tide prediction entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TidePrediction {
    /// Prediction time (e.g., "2026-04-24 00:00").
    pub time: String,
    /// Predicted water level height in metres relative to MLLW datum.
    pub height_m: f64,
}

/// Parsed tide prediction data from the NOAA CO-OPS API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TidesData {
    /// NOAA station identifier (e.g., "9447130").
    pub station_id: String,
    /// Human-readable station name (e.g., "Seattle").
    pub station_name: String,
    /// Tide predictions sorted chronologically.
    pub predictions: Vec<TidePrediction>,
}

// ---------------------------------------------------------------------------
// Serialization / deserialization
// ---------------------------------------------------------------------------

/// Serializes `TidesData` to JSON bytes for caching.
pub fn serialize_tides(data: &TidesData) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(data)
}

/// Deserializes `TidesData` from JSON bytes (cache read).
pub fn deserialize_tides(bytes: &[u8]) -> Result<TidesData, serde_json::Error> {
    serde_json::from_slice(bytes)
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parses the raw JSON response from the NOAA CO-OPS tide predictions API.
///
/// Expected response format:
/// ```json
/// {
///   "predictions": [
///     { "t": "2026-04-24 00:00", "v": "1.234" },
///     { "t": "2026-04-24 00:06", "v": "1.245" }
///   ]
/// }
/// ```
///
/// The `station_id` and `station_name` are passed in because the NOAA
/// response does not include them in the predictions payload.
pub fn parse_tides_response(
    raw: &[u8],
    station_id: &str,
    station_name: &str,
) -> Result<TidesData, String> {
    let root: Value = serde_json::from_slice(raw).map_err(|e| format!("JSON parse error: {e}"))?;

    // Check for an error response from NOAA
    if let Some(err) = root.get("error") {
        let msg = err
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown NOAA error");
        return Err(format!("NOAA API error: {msg}"));
    }

    let predictions_arr = root
        .get("predictions")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing or invalid 'predictions' array".to_string())?;

    let mut predictions = Vec::with_capacity(predictions_arr.len());

    for entry in predictions_arr {
        let time = match entry.get("t").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };

        let height_m = match entry.get("v").and_then(|v| v.as_str()) {
            Some(s) => match s.parse::<f64>() {
                Ok(h) => h,
                Err(_) => continue,
            },
            None => continue,
        };

        predictions.push(TidePrediction { time, height_m });
    }

    Ok(TidesData {
        station_id: station_id.to_string(),
        station_name: station_name.to_string(),
        predictions,
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
    fn test_build_tides_url() {
        let url = build_tides_url("9447130", "20260424", "20260501");
        assert!(url.starts_with(
            "https://api.tidesandcurrents.noaa.gov/api/prod/datagetter?"
        ));
        assert!(url.contains("station=9447130"));
        assert!(url.contains("product=predictions"));
        assert!(url.contains("begin_date=20260424"));
        assert!(url.contains("end_date=20260501"));
        assert!(url.contains("datum=MLLW"));
        assert!(url.contains("units=metric"));
        assert!(url.contains("time_zone=gmt"));
        assert!(url.contains("interval=6"));
        assert!(url.contains("format=json"));
    }

    #[test]
    fn test_build_tides_url_different_station() {
        let url = build_tides_url("9446484", "20260101", "20260108");
        assert!(url.contains("station=9446484"));
        assert!(url.contains("begin_date=20260101"));
        assert!(url.contains("end_date=20260108"));
    }

    // -------------------------------------------------------------------
    // Source metadata tests
    // -------------------------------------------------------------------

    #[test]
    fn test_source_metadata() {
        assert_eq!(NoaaTidesFetcher::source_id(), "noaa_tides");
        assert!(NoaaTidesFetcher::is_cacheable());
        assert_eq!(NoaaTidesFetcher::ttl_secs(), 3600);
    }

    // -------------------------------------------------------------------
    // Parsing tests
    // -------------------------------------------------------------------

    fn synthetic_tides_json() -> Vec<u8> {
        let json = serde_json::json!({
            "predictions": [
                { "t": "2026-04-24 00:00", "v": "1.234" },
                { "t": "2026-04-24 00:06", "v": "1.245" },
                { "t": "2026-04-24 00:12", "v": "1.256" }
            ]
        });
        serde_json::to_vec(&json).unwrap()
    }

    #[test]
    fn test_parse_tides_response() {
        let raw = synthetic_tides_json();
        let data = parse_tides_response(&raw, "9447130", "Seattle").unwrap();
        assert_eq!(data.station_id, "9447130");
        assert_eq!(data.station_name, "Seattle");
        assert_eq!(data.predictions.len(), 3);
    }

    #[test]
    fn test_parse_tides_response_prediction_values() {
        let raw = synthetic_tides_json();
        let data = parse_tides_response(&raw, "9447130", "Seattle").unwrap();
        assert_eq!(data.predictions[0].time, "2026-04-24 00:00");
        assert!((data.predictions[0].height_m - 1.234).abs() < 1e-9);
        assert_eq!(data.predictions[1].time, "2026-04-24 00:06");
        assert!((data.predictions[1].height_m - 1.245).abs() < 1e-9);
        assert_eq!(data.predictions[2].time, "2026-04-24 00:12");
        assert!((data.predictions[2].height_m - 1.256).abs() < 1e-9);
    }

    #[test]
    fn test_parse_tides_response_negative_height() {
        let json = serde_json::json!({
            "predictions": [
                { "t": "2026-04-24 06:00", "v": "-0.312" }
            ]
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_tides_response(&raw, "9447130", "Seattle").unwrap();
        assert_eq!(data.predictions.len(), 1);
        assert!((data.predictions[0].height_m - (-0.312)).abs() < 1e-9);
    }

    #[test]
    fn test_parse_tides_response_empty_predictions() {
        let json = serde_json::json!({ "predictions": [] });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_tides_response(&raw, "9447130", "Seattle").unwrap();
        assert!(data.predictions.is_empty());
    }

    #[test]
    fn test_parse_tides_response_missing_predictions() {
        let json = serde_json::json!({ "metadata": {} });
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_tides_response(&raw, "9447130", "Seattle");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("predictions"));
    }

    #[test]
    fn test_parse_tides_response_error_response() {
        let json = serde_json::json!({
            "error": {
                "message": "Station not found"
            }
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_tides_response(&raw, "0000000", "Unknown");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Station not found"));
    }

    #[test]
    fn test_parse_tides_response_invalid_json() {
        let raw = b"not json";
        let result = parse_tides_response(raw, "9447130", "Seattle");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JSON parse error"));
    }

    #[test]
    fn test_parse_tides_response_skips_invalid_entries() {
        let json = serde_json::json!({
            "predictions": [
                { "t": "2026-04-24 00:00", "v": "1.234" },
                { "t": "2026-04-24 00:06" },
                { "v": "1.300" },
                { "t": "2026-04-24 00:18", "v": "not_a_number" },
                { "t": "2026-04-24 00:24", "v": "1.400" }
            ]
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_tides_response(&raw, "9447130", "Seattle").unwrap();
        // Only entries with both valid "t" and parseable "v" are kept
        assert_eq!(data.predictions.len(), 2);
        assert_eq!(data.predictions[0].time, "2026-04-24 00:00");
        assert_eq!(data.predictions[1].time, "2026-04-24 00:24");
    }

    // -------------------------------------------------------------------
    // Serialization round-trip tests
    // -------------------------------------------------------------------

    #[test]
    fn test_serialize_deserialize_tides_round_trip() {
        let original = TidesData {
            station_id: "9447130".to_string(),
            station_name: "Seattle".to_string(),
            predictions: vec![
                TidePrediction {
                    time: "2026-04-24 00:00".to_string(),
                    height_m: 1.234,
                },
                TidePrediction {
                    time: "2026-04-24 06:00".to_string(),
                    height_m: -0.312,
                },
            ],
        };

        let bytes = serialize_tides(&original).unwrap();
        let restored = deserialize_tides(&bytes).unwrap();

        assert_eq!(restored.station_id, original.station_id);
        assert_eq!(restored.station_name, original.station_name);
        assert_eq!(restored.predictions.len(), original.predictions.len());
        for (a, b) in restored.predictions.iter().zip(&original.predictions) {
            assert_eq!(a.time, b.time);
            assert!((a.height_m - b.height_m).abs() < 1e-9);
        }
    }

    #[test]
    fn test_serialize_deserialize_tides_empty_predictions() {
        let original = TidesData {
            station_id: "9447130".to_string(),
            station_name: "Seattle".to_string(),
            predictions: vec![],
        };

        let bytes = serialize_tides(&original).unwrap();
        let restored = deserialize_tides(&bytes).unwrap();

        assert_eq!(restored.station_id, original.station_id);
        assert!(restored.predictions.is_empty());
    }
}
