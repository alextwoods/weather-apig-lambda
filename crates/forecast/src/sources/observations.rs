use chrono::{DateTime, TimeDelta, Utc};
use serde_json::Value;

use crate::models::haversine_km;

// ---------------------------------------------------------------------------
// NWS Observations API constants
// ---------------------------------------------------------------------------

/// Required `User-Agent` header for NWS API requests.
pub const NWS_USER_AGENT: &str = "EnsembleWeather/1.0.0";

/// Required `Accept` header for NWS API requests.
pub const NWS_ACCEPT: &str = "application/geo+json";

// ---------------------------------------------------------------------------
// ObservationsFetcher
// ---------------------------------------------------------------------------

/// Fetches and parses NWS observation data.
///
/// Observations are NOT cacheable — they are always fetched fresh from the
/// NWS API.
pub struct ObservationsFetcher;

impl ObservationsFetcher {
    pub fn source_id() -> &'static str {
        "observations"
    }

    pub fn is_cacheable() -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// URL builders
// ---------------------------------------------------------------------------

/// Builds the NWS station discovery URL for the given coordinates.
///
/// Returns the URL for `https://api.weather.gov/points/{lat},{lon}/stations`.
pub fn build_station_discovery_url(lat: f64, lon: f64) -> String {
    format!("https://api.weather.gov/points/{lat},{lon}/stations")
}

/// Builds the NWS observation fetch URL for a specific station.
///
/// Returns the URL for
/// `https://api.weather.gov/stations/{station_id}/observations?limit=25`.
pub fn build_observation_url(station_id: &str) -> String {
    format!("https://api.weather.gov/stations/{station_id}/observations?limit=25")
}

// ---------------------------------------------------------------------------
// Unit conversions
// ---------------------------------------------------------------------------

/// Converts wind speed from m/s to km/h (multiply by 3.6).
pub fn wind_speed_ms_to_kmh(ms: f64) -> f64 {
    ms * 3.6
}

/// Converts barometric pressure from Pa to hPa (divide by 100).
pub fn pressure_pa_to_hpa(pa: f64) -> f64 {
    pa / 100.0
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// Information about the observation station.
#[derive(Debug, Clone)]
pub struct StationInfo {
    /// Station identifier (e.g., "KBFI").
    pub id: String,
    /// Human-readable station name.
    pub name: String,
    /// Station latitude.
    pub latitude: f64,
    /// Station longitude.
    pub longitude: f64,
    /// Distance in km from the search coordinate to this station.
    pub distance_km: f64,
}

/// A single observation entry from the NWS API.
#[derive(Debug, Clone)]
pub struct ObservationEntry {
    /// ISO 8601 timestamp of the observation.
    pub timestamp: String,
    /// Temperature in degrees Celsius.
    pub temperature_celsius: Option<f64>,
    /// Wind speed in km/h (converted from m/s).
    pub wind_speed_kmh: Option<f64>,
    /// Wind direction in degrees.
    pub wind_direction_degrees: Option<f64>,
    /// Barometric pressure in hPa (converted from Pa).
    pub pressure_hpa: Option<f64>,
}

/// Parsed observation data including station info and observation entries.
#[derive(Debug, Clone)]
pub struct ObservationData {
    /// Metadata about the observation station.
    pub station: StationInfo,
    /// Observation entries, filtered and converted.
    pub entries: Vec<ObservationEntry>,
}

// ---------------------------------------------------------------------------
// Parsing — station discovery response
// ---------------------------------------------------------------------------

/// Extracts the first station's ID, name, and coordinates from the NWS
/// station discovery GeoJSON response.
///
/// The response has the shape:
/// ```json
/// {
///   "features": [
///     {
///       "properties": {
///         "stationIdentifier": "KBFI",
///         "name": "Seattle, Boeing Field"
///       },
///       "geometry": {
///         "coordinates": [-122.30, 47.53]
///       }
///     }
///   ]
/// }
/// ```
pub fn parse_station_discovery(
    raw: &[u8],
    search_lat: f64,
    search_lon: f64,
) -> Result<StationInfo, String> {
    let root: Value = serde_json::from_slice(raw).map_err(|e| format!("JSON parse error: {e}"))?;

    let features = root
        .get("features")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing or invalid 'features' array".to_string())?;

    let feature = features
        .first()
        .ok_or_else(|| "no stations found in response".to_string())?;

    let props = feature
        .get("properties")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "missing 'properties' in station feature".to_string())?;

    let id = props
        .get("stationIdentifier")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "missing 'stationIdentifier'".to_string())?
        .to_string();

    let name = props
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let coords = feature
        .get("geometry")
        .and_then(|v| v.get("coordinates"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing 'geometry.coordinates'".to_string())?;

    // GeoJSON coordinates are [longitude, latitude]
    let longitude = coords
        .first()
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "invalid longitude in coordinates".to_string())?;
    let latitude = coords
        .get(1)
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "invalid latitude in coordinates".to_string())?;

    let distance_km = haversine_km(search_lat, search_lon, latitude, longitude);

    Ok(StationInfo {
        id,
        name,
        latitude,
        longitude,
        distance_km,
    })
}

// ---------------------------------------------------------------------------
// Parsing — observation entries
// ---------------------------------------------------------------------------

/// Parses the NWS observation GeoJSON response into a list of
/// `ObservationEntry` values with unit conversions applied.
///
/// The response has the shape:
/// ```json
/// {
///   "features": [
///     {
///       "properties": {
///         "timestamp": "2026-04-24T15:53:00+00:00",
///         "temperature": { "value": 14.4, "unitCode": "wmoUnit:degC" },
///         "windSpeed": { "value": 5.14, "unitCode": "wmoUnit:m/s" },
///         "windDirection": { "value": 200, "unitCode": "wmoUnit:degree_(angle)" },
///         "barometricPressure": { "value": 101320, "unitCode": "wmoUnit:Pa" }
///       }
///     }
///   ]
/// }
/// ```
pub fn parse_observations(raw: &[u8]) -> Result<Vec<ObservationEntry>, String> {
    let root: Value = serde_json::from_slice(raw).map_err(|e| format!("JSON parse error: {e}"))?;

    let features = root
        .get("features")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing or invalid 'features' array".to_string())?;

    let mut entries = Vec::with_capacity(features.len());

    for feature in features {
        let props = match feature.get("properties").and_then(|v| v.as_object()) {
            Some(p) => p,
            None => continue,
        };

        let timestamp = match props.get("timestamp").and_then(|v| v.as_str()) {
            Some(t) => t.to_string(),
            None => continue,
        };

        let temperature_celsius = props
            .get("temperature")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_f64());

        let wind_speed_kmh = props
            .get("windSpeed")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_f64())
            .map(wind_speed_ms_to_kmh);

        let wind_direction_degrees = props
            .get("windDirection")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_f64());

        let pressure_hpa = props
            .get("barometricPressure")
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_f64())
            .map(pressure_pa_to_hpa);

        entries.push(ObservationEntry {
            timestamp,
            temperature_celsius,
            wind_speed_kmh,
            wind_direction_degrees,
            pressure_hpa,
        });
    }

    Ok(entries)
}

// ---------------------------------------------------------------------------
// 12-hour time filter
// ---------------------------------------------------------------------------

/// Filters observation entries to retain only those within 12 hours before
/// the reference time and all future entries.
///
/// Parses each entry's timestamp as an ISO 8601 datetime with timezone
/// offset (e.g., `2026-04-24T15:53:00+00:00`).
pub fn filter_observations_to_recent(
    entries: Vec<ObservationEntry>,
    reference_time: DateTime<Utc>,
) -> Vec<ObservationEntry> {
    let cutoff = reference_time - TimeDelta::hours(12);

    entries
        .into_iter()
        .filter(|entry| {
            parse_observation_timestamp(&entry.timestamp)
                .map(|dt| dt >= cutoff)
                .unwrap_or(false)
        })
        .collect()
}

/// Parses an NWS observation timestamp string into a UTC `DateTime`.
///
/// Handles ISO 8601 with timezone offset (e.g., `2026-04-24T15:53:00+00:00`).
pub fn parse_observation_timestamp(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // -------------------------------------------------------------------
    // Unit conversion tests
    // -------------------------------------------------------------------

    #[test]
    fn test_wind_speed_conversion() {
        let kmh = wind_speed_ms_to_kmh(10.0);
        assert!((kmh - 36.0).abs() < 1e-9, "10 m/s should be 36 km/h, got {kmh}");
    }

    #[test]
    fn test_wind_speed_conversion_zero() {
        assert!((wind_speed_ms_to_kmh(0.0)).abs() < 1e-9);
    }

    #[test]
    fn test_wind_speed_conversion_fractional() {
        let kmh = wind_speed_ms_to_kmh(5.14);
        let expected = 5.14 * 3.6;
        assert!(
            (kmh - expected).abs() < 1e-9,
            "5.14 m/s should be {expected} km/h, got {kmh}"
        );
    }

    #[test]
    fn test_pressure_conversion() {
        let hpa = pressure_pa_to_hpa(101325.0);
        assert!(
            (hpa - 1013.25).abs() < 1e-9,
            "101325 Pa should be 1013.25 hPa, got {hpa}"
        );
    }

    #[test]
    fn test_pressure_conversion_zero() {
        assert!((pressure_pa_to_hpa(0.0)).abs() < 1e-9);
    }

    #[test]
    fn test_pressure_conversion_exact() {
        let hpa = pressure_pa_to_hpa(101320.0);
        assert!(
            (hpa - 1013.20).abs() < 1e-9,
            "101320 Pa should be 1013.20 hPa, got {hpa}"
        );
    }

    // -------------------------------------------------------------------
    // URL builder tests
    // -------------------------------------------------------------------

    #[test]
    fn test_build_station_discovery_url() {
        let url = build_station_discovery_url(47.61, -122.33);
        assert_eq!(
            url,
            "https://api.weather.gov/points/47.61,-122.33/stations"
        );
    }

    #[test]
    fn test_build_observation_url() {
        let url = build_observation_url("KBFI");
        assert_eq!(
            url,
            "https://api.weather.gov/stations/KBFI/observations?limit=25"
        );
    }

    // -------------------------------------------------------------------
    // Source metadata tests
    // -------------------------------------------------------------------

    #[test]
    fn test_source_metadata() {
        assert_eq!(ObservationsFetcher::source_id(), "observations");
        assert!(!ObservationsFetcher::is_cacheable());
    }

    // -------------------------------------------------------------------
    // Station discovery parsing tests
    // -------------------------------------------------------------------

    fn synthetic_station_discovery_json() -> Vec<u8> {
        let json = serde_json::json!({
            "features": [
                {
                    "properties": {
                        "stationIdentifier": "KBFI",
                        "name": "Seattle, Boeing Field"
                    },
                    "geometry": {
                        "coordinates": [-122.30, 47.53]
                    }
                },
                {
                    "properties": {
                        "stationIdentifier": "KSEA",
                        "name": "Seattle-Tacoma International Airport"
                    },
                    "geometry": {
                        "coordinates": [-122.31, 47.45]
                    }
                }
            ]
        });
        serde_json::to_vec(&json).unwrap()
    }

    #[test]
    fn test_parse_station_discovery() {
        let raw = synthetic_station_discovery_json();
        let station = parse_station_discovery(&raw, 47.60, -122.33).unwrap();
        assert_eq!(station.id, "KBFI");
        assert_eq!(station.name, "Seattle, Boeing Field");
        assert!((station.latitude - 47.53).abs() < 1e-6);
        assert!((station.longitude - (-122.30)).abs() < 1e-6);
        // Distance from (47.60, -122.33) to (47.53, -122.30) should be a few km
        assert!(station.distance_km > 0.0 && station.distance_km < 20.0);
    }

    #[test]
    fn test_parse_station_discovery_empty_features() {
        let json = serde_json::json!({"features": []});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_station_discovery(&raw, 47.60, -122.33);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("no stations found"));
    }

    #[test]
    fn test_parse_station_discovery_missing_features() {
        let json = serde_json::json!({"type": "FeatureCollection"});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_station_discovery(&raw, 47.60, -122.33);
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------
    // Observation parsing tests
    // -------------------------------------------------------------------

    fn synthetic_observations_json() -> Vec<u8> {
        let json = serde_json::json!({
            "features": [
                {
                    "properties": {
                        "timestamp": "2026-04-24T15:53:00+00:00",
                        "temperature": { "value": 14.4, "unitCode": "wmoUnit:degC" },
                        "windSpeed": { "value": 5.14, "unitCode": "wmoUnit:m/s" },
                        "windDirection": { "value": 200, "unitCode": "wmoUnit:degree_(angle)" },
                        "barometricPressure": { "value": 101320, "unitCode": "wmoUnit:Pa" }
                    }
                },
                {
                    "properties": {
                        "timestamp": "2026-04-24T14:53:00+00:00",
                        "temperature": { "value": 13.2, "unitCode": "wmoUnit:degC" },
                        "windSpeed": { "value": 3.0, "unitCode": "wmoUnit:m/s" },
                        "windDirection": { "value": 180, "unitCode": "wmoUnit:degree_(angle)" },
                        "barometricPressure": { "value": 101400, "unitCode": "wmoUnit:Pa" }
                    }
                }
            ]
        });
        serde_json::to_vec(&json).unwrap()
    }

    #[test]
    fn test_parse_observations() {
        let raw = synthetic_observations_json();
        let entries = parse_observations(&raw).unwrap();
        assert_eq!(entries.len(), 2);

        let first = &entries[0];
        assert_eq!(first.timestamp, "2026-04-24T15:53:00+00:00");
        assert_eq!(first.temperature_celsius, Some(14.4));
        // 5.14 m/s * 3.6 = 18.504 km/h
        assert!(
            (first.wind_speed_kmh.unwrap() - 18.504).abs() < 1e-6,
            "wind speed: {:?}",
            first.wind_speed_kmh
        );
        assert_eq!(first.wind_direction_degrees, Some(200.0));
        // 101320 Pa / 100 = 1013.20 hPa
        assert!(
            (first.pressure_hpa.unwrap() - 1013.20).abs() < 1e-6,
            "pressure: {:?}",
            first.pressure_hpa
        );
    }

    #[test]
    fn test_parse_observations_null_values() {
        let json = serde_json::json!({
            "features": [
                {
                    "properties": {
                        "timestamp": "2026-04-24T15:53:00+00:00",
                        "temperature": { "value": null, "unitCode": "wmoUnit:degC" },
                        "windSpeed": { "value": null, "unitCode": "wmoUnit:m/s" },
                        "windDirection": { "value": null, "unitCode": "wmoUnit:degree_(angle)" },
                        "barometricPressure": { "value": null, "unitCode": "wmoUnit:Pa" }
                    }
                }
            ]
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let entries = parse_observations(&raw).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].temperature_celsius, None);
        assert_eq!(entries[0].wind_speed_kmh, None);
        assert_eq!(entries[0].wind_direction_degrees, None);
        assert_eq!(entries[0].pressure_hpa, None);
    }

    #[test]
    fn test_parse_observations_missing_features() {
        let json = serde_json::json!({"type": "FeatureCollection"});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_observations(&raw);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_observations_empty_features() {
        let json = serde_json::json!({"features": []});
        let raw = serde_json::to_vec(&json).unwrap();
        let entries = parse_observations(&raw).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_parse_observations_skips_missing_timestamp() {
        let json = serde_json::json!({
            "features": [
                {
                    "properties": {
                        "temperature": { "value": 14.4, "unitCode": "wmoUnit:degC" }
                    }
                }
            ]
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let entries = parse_observations(&raw).unwrap();
        assert!(entries.is_empty());
    }

    // -------------------------------------------------------------------
    // Timestamp parsing tests
    // -------------------------------------------------------------------

    #[test]
    fn test_parse_observation_timestamp() {
        let dt = parse_observation_timestamp("2026-04-24T15:53:00+00:00").unwrap();
        assert_eq!(
            dt,
            Utc.with_ymd_and_hms(2026, 4, 24, 15, 53, 0).unwrap()
        );
    }

    #[test]
    fn test_parse_observation_timestamp_with_offset() {
        // -07:00 offset → should convert to UTC
        let dt = parse_observation_timestamp("2026-04-24T08:53:00-07:00").unwrap();
        assert_eq!(
            dt,
            Utc.with_ymd_and_hms(2026, 4, 24, 15, 53, 0).unwrap()
        );
    }

    #[test]
    fn test_parse_observation_timestamp_invalid() {
        assert!(parse_observation_timestamp("not-a-date").is_none());
    }

    // -------------------------------------------------------------------
    // Time filter tests
    // -------------------------------------------------------------------

    fn make_entry(timestamp: &str) -> ObservationEntry {
        ObservationEntry {
            timestamp: timestamp.to_string(),
            temperature_celsius: Some(10.0),
            wind_speed_kmh: Some(20.0),
            wind_direction_degrees: Some(180.0),
            pressure_hpa: Some(1013.0),
        }
    }

    #[test]
    fn test_filter_observations_to_recent() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap();
        let entries = vec![
            make_entry("2026-04-23T23:00:00+00:00"), // -13h → dropped
            make_entry("2026-04-24T00:00:00+00:00"), // -12h → kept
            make_entry("2026-04-24T06:00:00+00:00"), // -6h  → kept
            make_entry("2026-04-24T12:00:00+00:00"), // 0h   → kept
            make_entry("2026-04-24T18:00:00+00:00"), // +6h  → kept
        ];

        let filtered = filter_observations_to_recent(entries, reference);
        assert_eq!(filtered.len(), 4);
        assert_eq!(filtered[0].timestamp, "2026-04-24T00:00:00+00:00");
        assert_eq!(filtered[3].timestamp, "2026-04-24T18:00:00+00:00");
    }

    #[test]
    fn test_filter_observations_all_old() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 25, 12, 0, 0).unwrap();
        let entries = vec![make_entry("2026-04-24T00:00:00+00:00")];

        let filtered = filter_observations_to_recent(entries, reference);
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_filter_observations_all_recent() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 24, 6, 0, 0).unwrap();
        let entries = vec![
            make_entry("2026-04-24T00:00:00+00:00"),
            make_entry("2026-04-24T01:00:00+00:00"),
        ];

        let filtered = filter_observations_to_recent(entries, reference);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_observations_invalid_timestamps_dropped() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap();
        let entries = vec![
            make_entry("not-a-date"),
            make_entry("2026-04-24T11:00:00+00:00"),
        ];

        let filtered = filter_observations_to_recent(entries, reference);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].timestamp, "2026-04-24T11:00:00+00:00");
    }


    // -------------------------------------------------------------------
    // Property test — Property 10: Observation unit conversion
    // -------------------------------------------------------------------

    /// Feature: weather-backend-api, Property 10: Observation unit conversion
    ///
    /// **Validates: Requirements 11.4**
    mod prop_observation_unit_conversion {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn prop_wind_speed_and_pressure_conversion(
                wind_speed_ms in 0.0f64..200.0,
                pressure_pa in 0.0f64..200_000.0,
            ) {
                let wind_speed_kmh = wind_speed_ms_to_kmh(wind_speed_ms);
                let pressure_hpa = pressure_pa_to_hpa(pressure_pa);

                // Verify wind speed: kmh == ms * 3.6
                let expected_kmh = wind_speed_ms * 3.6;
                prop_assert!(
                    (wind_speed_kmh - expected_kmh).abs() < 1e-9,
                    "Wind speed conversion failed: {} m/s → {} km/h, expected {}",
                    wind_speed_ms, wind_speed_kmh, expected_kmh,
                );

                // Verify pressure: hpa == pa / 100.0
                let expected_hpa = pressure_pa / 100.0;
                prop_assert!(
                    (pressure_hpa - expected_hpa).abs() < 1e-9,
                    "Pressure conversion failed: {} Pa → {} hPa, expected {}",
                    pressure_pa, pressure_hpa, expected_hpa,
                );
            }
        }
    }
}
