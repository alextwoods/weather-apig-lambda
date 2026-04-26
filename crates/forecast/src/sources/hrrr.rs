use chrono::{DateTime, NaiveDateTime, TimeDelta, Utc};
use serde_json::Value;

// ---------------------------------------------------------------------------
// HRRR API URL builder
// ---------------------------------------------------------------------------

/// HRRR variables requested from the Open-Meteo GFS API.
const HRRR_VARIABLES: [&str; 9] = [
    "temperature_2m",
    "apparent_temperature",
    "dew_point_2m",
    "wind_speed_10m",
    "wind_gusts_10m",
    "wind_direction_10m",
    "surface_pressure",
    "precipitation",
    "precipitation_probability",
];

/// Builds the Open-Meteo GFS API URL for HRRR data at the given coordinates.
///
/// Requests 9 HRRR variables with 2 forecast days and 24 past hours.
pub fn build_hrrr_url(lat: f64, lon: f64) -> String {
    let variables = HRRR_VARIABLES.join(",");
    format!(
        "https://api.open-meteo.com/v1/gfs\
         ?latitude={lat}&longitude={lon}\
         &hourly={variables}\
         &forecast_days=2\
         &past_hours=24"
    )
}

// ---------------------------------------------------------------------------
// HrrrFetcher
// ---------------------------------------------------------------------------

/// Fetches and parses HRRR deterministic forecast data from the Open-Meteo
/// GFS API.
pub struct HrrrFetcher;

impl HrrrFetcher {
    pub fn source_id() -> &'static str {
        "hrrr"
    }

    pub fn ttl_secs() -> u64 {
        3600
    }

    pub fn is_cacheable() -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Parsed HRRR data
// ---------------------------------------------------------------------------

/// Parsed HRRR forecast data from the Open-Meteo GFS API.
#[derive(Debug, Clone)]
pub struct HrrrData {
    /// ISO 8601 time strings for each hourly step.
    pub times: Vec<String>,
    /// Temperature at 2m in °C.
    pub temperature_2m: Vec<Option<f64>>,
    /// Apparent (feels-like) temperature in °C.
    pub apparent_temperature: Vec<Option<f64>>,
    /// Dew point at 2m in °C.
    pub dew_point_2m: Vec<Option<f64>>,
    /// Wind speed at 10m in km/h.
    pub wind_speed_10m: Vec<Option<f64>>,
    /// Wind gusts at 10m in km/h.
    pub wind_gusts_10m: Vec<Option<f64>>,
    /// Wind direction at 10m in degrees.
    pub wind_direction_10m: Vec<Option<f64>>,
    /// Surface pressure in hPa.
    pub surface_pressure: Vec<Option<f64>>,
    /// Precipitation in mm.
    pub precipitation: Vec<Option<f64>>,
    /// Precipitation probability in %.
    pub precipitation_probability: Vec<Option<f64>>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parses the raw JSON response from the Open-Meteo GFS API for HRRR data.
pub fn parse_hrrr_response(raw: &[u8]) -> Result<HrrrData, String> {
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

    Ok(HrrrData {
        times,
        temperature_2m: extract_array("temperature_2m"),
        apparent_temperature: extract_array("apparent_temperature"),
        dew_point_2m: extract_array("dew_point_2m"),
        wind_speed_10m: extract_array("wind_speed_10m"),
        wind_gusts_10m: extract_array("wind_gusts_10m"),
        wind_direction_10m: extract_array("wind_direction_10m"),
        surface_pressure: extract_array("surface_pressure"),
        precipitation: extract_array("precipitation"),
        precipitation_probability: extract_array("precipitation_probability"),
    })
}

// ---------------------------------------------------------------------------
// 12-hour time filter
// ---------------------------------------------------------------------------

/// Filters HRRR data to retain only entries within 12 hours before the
/// reference time and all future entries.
///
/// Parses each time string as a `NaiveDateTime` (format `%Y-%m-%dT%H:%M`),
/// finds the first index where `time >= reference_time - 12 hours`, and
/// slices all parallel arrays from that index onward.
pub fn filter_to_recent(data: HrrrData, reference_time: DateTime<Utc>) -> HrrrData {
    let cutoff = reference_time - TimeDelta::hours(12);

    let start_index = data
        .times
        .iter()
        .position(|t| {
            parse_time_str(t)
                .map(|dt| dt >= cutoff)
                .unwrap_or(false)
        })
        .unwrap_or(data.times.len());

    let slice_vec_f64 = |v: &[Option<f64>]| -> Vec<Option<f64>> {
        if start_index < v.len() {
            v[start_index..].to_vec()
        } else {
            vec![]
        }
    };

    HrrrData {
        times: data.times[start_index..].to_vec(),
        temperature_2m: slice_vec_f64(&data.temperature_2m),
        apparent_temperature: slice_vec_f64(&data.apparent_temperature),
        dew_point_2m: slice_vec_f64(&data.dew_point_2m),
        wind_speed_10m: slice_vec_f64(&data.wind_speed_10m),
        wind_gusts_10m: slice_vec_f64(&data.wind_gusts_10m),
        wind_direction_10m: slice_vec_f64(&data.wind_direction_10m),
        surface_pressure: slice_vec_f64(&data.surface_pressure),
        precipitation: slice_vec_f64(&data.precipitation),
        precipitation_probability: slice_vec_f64(&data.precipitation_probability),
    }
}

/// Parses an Open-Meteo time string (`YYYY-MM-DDTHH:MM`) into a UTC
/// `DateTime`.
pub fn parse_time_str(s: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .ok()
        .map(|ndt| ndt.and_utc())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_build_hrrr_url() {
        let url = build_hrrr_url(47.61, -122.33);
        assert!(url.starts_with("https://api.open-meteo.com/v1/gfs?"));
        assert!(url.contains("latitude=47.61"));
        assert!(url.contains("longitude=-122.33"));
        assert!(url.contains("forecast_days=2"));
        assert!(url.contains("past_hours=24"));
        assert!(url.contains("temperature_2m"));
        assert!(url.contains("apparent_temperature"));
        assert!(url.contains("dew_point_2m"));
        assert!(url.contains("wind_speed_10m"));
        assert!(url.contains("wind_gusts_10m"));
        assert!(url.contains("wind_direction_10m"));
        assert!(url.contains("surface_pressure"));
        assert!(url.contains("precipitation"));
        assert!(url.contains("precipitation_probability"));
    }

    #[test]
    fn test_build_hrrr_url_negative_coords() {
        let url = build_hrrr_url(-33.87, 151.21);
        assert!(url.contains("latitude=-33.87"));
        assert!(url.contains("longitude=151.21"));
    }

    #[test]
    fn test_source_metadata() {
        assert_eq!(HrrrFetcher::source_id(), "hrrr");
        assert_eq!(HrrrFetcher::ttl_secs(), 3600);
        assert!(HrrrFetcher::is_cacheable());
    }

    fn synthetic_hrrr_json() -> Vec<u8> {
        let json = serde_json::json!({
            "hourly": {
                "time": [
                    "2026-04-24T00:00",
                    "2026-04-24T01:00",
                    "2026-04-24T02:00"
                ],
                "temperature_2m": [10.0, 11.0, 12.0],
                "apparent_temperature": [8.0, 9.0, 10.0],
                "dew_point_2m": [5.0, 6.0, 7.0],
                "wind_speed_10m": [15.0, 16.0, 17.0],
                "wind_gusts_10m": [25.0, 26.0, 27.0],
                "wind_direction_10m": [180.0, 190.0, 200.0],
                "surface_pressure": [1013.0, 1012.0, 1011.0],
                "precipitation": [0.0, 0.1, 0.0],
                "precipitation_probability": [10.0, 20.0, 5.0]
            }
        });
        serde_json::to_vec(&json).unwrap()
    }

    #[test]
    fn test_parse_hrrr_response_times() {
        let raw = synthetic_hrrr_json();
        let data = parse_hrrr_response(&raw).unwrap();
        assert_eq!(data.times.len(), 3);
        assert_eq!(data.times[0], "2026-04-24T00:00");
    }

    #[test]
    fn test_parse_hrrr_response_data() {
        let raw = synthetic_hrrr_json();
        let data = parse_hrrr_response(&raw).unwrap();
        assert_eq!(data.temperature_2m.len(), 3);
        assert_eq!(data.temperature_2m[0], Some(10.0));
        assert_eq!(data.apparent_temperature[1], Some(9.0));
        assert_eq!(data.dew_point_2m[2], Some(7.0));
        assert_eq!(data.wind_speed_10m[0], Some(15.0));
        assert_eq!(data.wind_gusts_10m[1], Some(26.0));
        assert_eq!(data.wind_direction_10m[2], Some(200.0));
        assert_eq!(data.surface_pressure[0], Some(1013.0));
        assert_eq!(data.precipitation[1], Some(0.1));
        assert_eq!(data.precipitation_probability[2], Some(5.0));
    }

    #[test]
    fn test_parse_hrrr_response_null_handling() {
        let json = serde_json::json!({
            "hourly": {
                "time": ["2026-04-24T00:00"],
                "temperature_2m": [null],
                "apparent_temperature": [null],
                "dew_point_2m": [null],
                "wind_speed_10m": [null],
                "wind_gusts_10m": [null],
                "wind_direction_10m": [null],
                "surface_pressure": [null],
                "precipitation": [null],
                "precipitation_probability": [null]
            }
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_hrrr_response(&raw).unwrap();
        assert_eq!(data.temperature_2m[0], None);
        assert_eq!(data.precipitation[0], None);
    }

    #[test]
    fn test_parse_hrrr_response_missing_hourly() {
        let json = serde_json::json!({"daily": {}});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_hrrr_response(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("hourly"));
    }

    #[test]
    fn test_parse_hrrr_response_missing_time() {
        let json = serde_json::json!({"hourly": {"temperature_2m": [1.0]}});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_hrrr_response(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("time"));
    }

    #[test]
    fn test_parse_hrrr_response_partial_data() {
        let json = serde_json::json!({
            "hourly": {
                "time": ["2026-04-24T00:00"],
                "temperature_2m": [15.0]
            }
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let data = parse_hrrr_response(&raw).unwrap();
        assert_eq!(data.temperature_2m.len(), 1);
        assert!(data.apparent_temperature.is_empty());
        assert!(data.precipitation.is_empty());
    }

    // -----------------------------------------------------------------------
    // Time filter tests
    // -----------------------------------------------------------------------

    /// Build HRRR data with entries at -13h, -12h, -6h, 0h, +6h relative to
    /// a reference time of 2026-04-24T12:00Z.
    ///
    /// Expected: entries at -13h are dropped; -12h through +6h are retained.
    fn time_filter_test_data() -> (HrrrData, DateTime<Utc>) {
        let reference = Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap();
        // -13h = 2026-04-23T23:00
        // -12h = 2026-04-24T00:00
        //  -6h = 2026-04-24T06:00
        //   0h = 2026-04-24T12:00
        //  +6h = 2026-04-24T18:00
        let times = vec![
            "2026-04-23T23:00".to_string(),
            "2026-04-24T00:00".to_string(),
            "2026-04-24T06:00".to_string(),
            "2026-04-24T12:00".to_string(),
            "2026-04-24T18:00".to_string(),
        ];
        let make_vals = |vals: Vec<f64>| vals.into_iter().map(Some).collect();
        let data = HrrrData {
            times,
            temperature_2m: make_vals(vec![1.0, 2.0, 3.0, 4.0, 5.0]),
            apparent_temperature: make_vals(vec![0.5, 1.5, 2.5, 3.5, 4.5]),
            dew_point_2m: make_vals(vec![-1.0, 0.0, 1.0, 2.0, 3.0]),
            wind_speed_10m: make_vals(vec![10.0, 11.0, 12.0, 13.0, 14.0]),
            wind_gusts_10m: make_vals(vec![20.0, 21.0, 22.0, 23.0, 24.0]),
            wind_direction_10m: make_vals(vec![180.0, 190.0, 200.0, 210.0, 220.0]),
            surface_pressure: make_vals(vec![1013.0, 1012.0, 1011.0, 1010.0, 1009.0]),
            precipitation: make_vals(vec![0.0, 0.1, 0.2, 0.3, 0.4]),
            precipitation_probability: make_vals(vec![5.0, 10.0, 15.0, 20.0, 25.0]),
        };
        (data, reference)
    }

    #[test]
    fn test_filter_to_recent_drops_old_entries() {
        let (data, reference) = time_filter_test_data();
        let filtered = filter_to_recent(data, reference);

        // -13h entry should be dropped; 4 entries remain
        assert_eq!(filtered.times.len(), 4);
        assert_eq!(filtered.times[0], "2026-04-24T00:00");
        assert_eq!(filtered.times[3], "2026-04-24T18:00");
    }

    #[test]
    fn test_filter_to_recent_parallel_arrays_same_length() {
        let (data, reference) = time_filter_test_data();
        let filtered = filter_to_recent(data, reference);
        let len = filtered.times.len();

        assert_eq!(filtered.temperature_2m.len(), len);
        assert_eq!(filtered.apparent_temperature.len(), len);
        assert_eq!(filtered.dew_point_2m.len(), len);
        assert_eq!(filtered.wind_speed_10m.len(), len);
        assert_eq!(filtered.wind_gusts_10m.len(), len);
        assert_eq!(filtered.wind_direction_10m.len(), len);
        assert_eq!(filtered.surface_pressure.len(), len);
        assert_eq!(filtered.precipitation.len(), len);
        assert_eq!(filtered.precipitation_probability.len(), len);
    }

    #[test]
    fn test_filter_to_recent_preserves_data_values() {
        let (data, reference) = time_filter_test_data();
        let filtered = filter_to_recent(data, reference);

        // First retained entry is the -12h entry (index 1 in original)
        assert_eq!(filtered.temperature_2m[0], Some(2.0));
        assert_eq!(filtered.precipitation[3], Some(0.4));
    }

    #[test]
    fn test_filter_to_recent_all_old() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 25, 12, 0, 0).unwrap();
        let data = HrrrData {
            times: vec!["2026-04-24T00:00".to_string()],
            temperature_2m: vec![Some(10.0)],
            apparent_temperature: vec![Some(8.0)],
            dew_point_2m: vec![Some(5.0)],
            wind_speed_10m: vec![Some(15.0)],
            wind_gusts_10m: vec![Some(25.0)],
            wind_direction_10m: vec![Some(180.0)],
            surface_pressure: vec![Some(1013.0)],
            precipitation: vec![Some(0.0)],
            precipitation_probability: vec![Some(10.0)],
        };
        let filtered = filter_to_recent(data, reference);
        assert!(filtered.times.is_empty());
        assert!(filtered.temperature_2m.is_empty());
    }

    #[test]
    fn test_filter_to_recent_all_recent() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 24, 6, 0, 0).unwrap();
        let data = HrrrData {
            times: vec![
                "2026-04-24T00:00".to_string(),
                "2026-04-24T01:00".to_string(),
            ],
            temperature_2m: vec![Some(10.0), Some(11.0)],
            apparent_temperature: vec![Some(8.0), Some(9.0)],
            dew_point_2m: vec![Some(5.0), Some(6.0)],
            wind_speed_10m: vec![Some(15.0), Some(16.0)],
            wind_gusts_10m: vec![Some(25.0), Some(26.0)],
            wind_direction_10m: vec![Some(180.0), Some(190.0)],
            surface_pressure: vec![Some(1013.0), Some(1012.0)],
            precipitation: vec![Some(0.0), Some(0.1)],
            precipitation_probability: vec![Some(10.0), Some(20.0)],
        };
        let filtered = filter_to_recent(data, reference);
        assert_eq!(filtered.times.len(), 2);
    }

    #[test]
    fn test_parse_time_str() {
        let dt = parse_time_str("2026-04-24T12:00").unwrap();
        assert_eq!(dt, Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap());
    }

    #[test]
    fn test_parse_time_str_invalid() {
        assert!(parse_time_str("not-a-date").is_none());
    }

    // -------------------------------------------------------------------
    // Property test — Property 4: Time filtering preserves only recent
    // entries
    // -------------------------------------------------------------------

    /// Feature: weather-backend-api, Property 4: Time filtering preserves only recent entries
    ///
    /// **Validates: Requirements 2.7, 2.10, 10.2, 10.3, 11.3**
    mod prop_time_filter {
        use super::*;
        use chrono::TimeDelta;
        use proptest::prelude::*;

        /// Strategy to generate a reference time and a set of timestamped
        /// entries spanning 0–48 hours around the reference.
        ///
        /// Returns `(HrrrData, DateTime<Utc>)` where the data has between
        /// 1 and 72 entries with times spread across a 48-hour window.
        fn time_filter_strategy(
        ) -> impl Strategy<Value = (HrrrData, DateTime<Utc>)> {
            // Base reference time: 2026-04-24T12:00Z + random offset 0–168h
            (0i64..168).prop_flat_map(|ref_offset_hours| {
                let reference = Utc
                    .with_ymd_and_hms(2026, 4, 24, 12, 0, 0)
                    .unwrap()
                    + TimeDelta::hours(ref_offset_hours);

                // Generate 1–72 hour offsets relative to reference, spanning
                // -24h to +24h (i.e. 0–48h window).
                let offsets = prop::collection::vec(-24i64..=24, 1..=72usize);

                (Just(reference), offsets)
            })
            .prop_flat_map(|(reference, offsets)| {
                let n = offsets.len();
                // Sort offsets so times are in chronological order
                let mut sorted_offsets = offsets;
                sorted_offsets.sort();

                let times: Vec<String> = sorted_offsets
                    .iter()
                    .map(|&off| {
                        let dt = reference + TimeDelta::hours(off);
                        dt.format("%Y-%m-%dT%H:%M").to_string()
                    })
                    .collect();

                // Generate parallel data arrays
                let data_strat = prop::collection::vec(
                    prop::option::of(-50.0f64..50.0),
                    n,
                );

                (
                    Just(reference),
                    Just(times),
                    data_strat.clone(), // temperature_2m
                    data_strat.clone(), // apparent_temperature
                    data_strat.clone(), // dew_point_2m
                    data_strat.clone(), // wind_speed_10m
                    data_strat.clone(), // wind_gusts_10m
                    data_strat.clone(), // wind_direction_10m
                    data_strat.clone(), // surface_pressure
                    data_strat.clone(), // precipitation
                    data_strat,         // precipitation_probability
                )
            })
            .prop_map(
                |(
                    reference,
                    times,
                    temp,
                    apparent,
                    dew,
                    wind_speed,
                    wind_gusts,
                    wind_dir,
                    pressure,
                    precip,
                    precip_prob,
                )| {
                    let data = HrrrData {
                        times,
                        temperature_2m: temp,
                        apparent_temperature: apparent,
                        dew_point_2m: dew,
                        wind_speed_10m: wind_speed,
                        wind_gusts_10m: wind_gusts,
                        wind_direction_10m: wind_dir,
                        surface_pressure: pressure,
                        precipitation: precip,
                        precipitation_probability: precip_prob,
                    };
                    (data, reference)
                },
            )
        }

        proptest! {
            #[test]
            fn prop_time_filter_preserves_recent(
                (data, reference) in time_filter_strategy()
            ) {
                let cutoff = reference - TimeDelta::hours(12);
                let original_times = data.times.clone();
                let filtered = filter_to_recent(data, reference);

                // 1. Every remaining entry has time >= cutoff
                for t in &filtered.times {
                    let dt = parse_time_str(t).expect("filtered time should parse");
                    prop_assert!(
                        dt >= cutoff,
                        "Filtered entry {} is before cutoff {}",
                        t,
                        cutoff,
                    );
                }

                // 2. No entry within the window is dropped
                for t in &original_times {
                    if let Some(dt) = parse_time_str(t) {
                        if dt >= cutoff {
                            prop_assert!(
                                filtered.times.contains(t),
                                "Entry {} is within window but was dropped",
                                t,
                            );
                        }
                    }
                }

                // 3. All parallel data arrays have the same length as times
                let len = filtered.times.len();
                prop_assert_eq!(filtered.temperature_2m.len(), len);
                prop_assert_eq!(filtered.apparent_temperature.len(), len);
                prop_assert_eq!(filtered.dew_point_2m.len(), len);
                prop_assert_eq!(filtered.wind_speed_10m.len(), len);
                prop_assert_eq!(filtered.wind_gusts_10m.len(), len);
                prop_assert_eq!(filtered.wind_direction_10m.len(), len);
                prop_assert_eq!(filtered.surface_pressure.len(), len);
                prop_assert_eq!(filtered.precipitation.len(), len);
                prop_assert_eq!(filtered.precipitation_probability.len(), len);
            }
        }
    }
}
