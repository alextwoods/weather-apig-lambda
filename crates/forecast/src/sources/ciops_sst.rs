use chrono::{DateTime, TimeDelta, Timelike, Utc};
use serde_json::Value;

// ---------------------------------------------------------------------------
// CIOPS-Salish Sea WMS constants
// ---------------------------------------------------------------------------

/// WMS layer for CIOPS-Salish Sea sea water potential temperature at depth 0.
const CIOPS_LAYER: &str = "CIOPS-SalishSea_2km_SeaWaterPotentialTemperature-Depth0";

/// Offset to subtract from Kelvin to get Celsius.
const KELVIN_OFFSET: f64 = 273.15;

// ---------------------------------------------------------------------------
// CiopsSstFetcher
// ---------------------------------------------------------------------------

/// Fetches and parses sea surface temperature data from the ECCC
/// CIOPS-Salish Sea WMS API.
///
/// CIOPS SST data is NOT cacheable — it is always fetched fresh.
pub struct CiopsSstFetcher;

impl CiopsSstFetcher {
    pub fn source_id() -> &'static str {
        "ciops_sst"
    }

    pub fn is_cacheable() -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Parsed CIOPS SST data
// ---------------------------------------------------------------------------

/// Parsed CIOPS SST data assembled from multiple WMS GetFeatureInfo requests.
#[derive(Debug, Clone)]
pub struct CiopsSstData {
    /// ISO 8601 time strings for each time step.
    pub times: Vec<String>,
    /// Sea surface temperature in °C for each time step, or `None` if the
    /// request for that time step failed.
    pub temperatures_celsius: Vec<Option<f64>>,
}

// ---------------------------------------------------------------------------
// CIOPS time step generation
// ---------------------------------------------------------------------------

/// Generates 9 CIOPS time steps at 6-hour intervals starting from the
/// reference time rounded down to the nearest 6-hour boundary (00Z, 06Z,
/// 12Z, or 18Z).
///
/// This produces a 48-hour forecast window from the rounded base time.
pub fn generate_ciops_time_steps(reference: DateTime<Utc>) -> Vec<DateTime<Utc>> {
    let hour = reference.hour();
    let rounded_hour = (hour / 6) * 6;
    let base = reference
        .date_naive()
        .and_hms_opt(rounded_hour, 0, 0)
        .unwrap()
        .and_utc();
    (0..9).map(|i| base + TimeDelta::hours(i * 6)).collect()
}

// ---------------------------------------------------------------------------
// WMS URL builder
// ---------------------------------------------------------------------------

/// Builds a WMS GetFeatureInfo URL for a single CIOPS time step.
///
/// The BBOX is a small 0.02° × 0.02° box centred on the given coordinates.
/// The grid is 3×3 pixels and the query point is the centre pixel (I=1, J=1).
pub fn build_ciops_wms_url(lat: f64, lon: f64, time: &DateTime<Utc>) -> String {
    let time_iso = time.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let bbox_lat_min = lat - 0.01;
    let bbox_lon_min = lon - 0.01;
    let bbox_lat_max = lat + 0.01;
    let bbox_lon_max = lon + 0.01;

    format!(
        "https://geo.weather.gc.ca/geomet\
         ?SERVICE=WMS\
         &VERSION=1.3.0\
         &REQUEST=GetFeatureInfo\
         &LAYERS={CIOPS_LAYER}\
         &CRS=EPSG:4326\
         &BBOX={bbox_lat_min},{bbox_lon_min},{bbox_lat_max},{bbox_lon_max}\
         &WIDTH=3\
         &HEIGHT=3\
         &I=1\
         &J=1\
         &INFO_FORMAT=application/json\
         &TIME={time_iso}"
    )
}

// ---------------------------------------------------------------------------
// Kelvin → Celsius conversion
// ---------------------------------------------------------------------------

/// Converts a temperature from Kelvin to Celsius.
pub fn kelvin_to_celsius(kelvin: f64) -> f64 {
    kelvin - KELVIN_OFFSET
}

// ---------------------------------------------------------------------------
// Parsing — single time step GeoJSON response
// ---------------------------------------------------------------------------

/// Parses a single WMS GetFeatureInfo GeoJSON response and extracts the
/// temperature value, converting from Kelvin to Celsius.
///
/// Expected response format (GeoJSON FeatureCollection):
/// ```json
/// {
///   "type": "FeatureCollection",
///   "features": [
///     {
///       "properties": {
///         "CIOPS-SalishSea_2km_SeaWaterPotentialTemperature-Depth0": 283.15
///       }
///     }
///   ]
/// }
/// ```
///
/// Returns `None` if the response cannot be parsed or contains no data.
pub fn parse_ciops_feature_info(raw: &[u8]) -> Option<f64> {
    let root: Value = serde_json::from_slice(raw).ok()?;

    let features = root.get("features")?.as_array()?;
    let feature = features.first()?;
    let props = feature.get("properties")?.as_object()?;

    let kelvin = props.get(CIOPS_LAYER)?.as_f64()?;
    Some(kelvin_to_celsius(kelvin))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // -------------------------------------------------------------------
    // Source metadata tests
    // -------------------------------------------------------------------

    #[test]
    fn test_source_metadata() {
        assert_eq!(CiopsSstFetcher::source_id(), "ciops_sst");
        assert!(!CiopsSstFetcher::is_cacheable());
    }

    // -------------------------------------------------------------------
    // CIOPS time step generation tests
    // -------------------------------------------------------------------

    #[test]
    fn test_generate_ciops_time_steps_at_14_30() {
        // 2026-04-24T14:30:00Z → rounds down to 12:00Z
        let reference = Utc.with_ymd_and_hms(2026, 4, 24, 14, 30, 0).unwrap();
        let steps = generate_ciops_time_steps(reference);

        assert_eq!(steps.len(), 9);

        // First step at 12:00Z
        assert_eq!(steps[0], Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap());

        // 6-hour spacing
        for i in 1..steps.len() {
            let diff = steps[i] - steps[i - 1];
            assert_eq!(diff, TimeDelta::hours(6));
        }

        // Last step: 12:00Z + 48h = 2026-04-26T12:00Z
        assert_eq!(
            steps[8],
            Utc.with_ymd_and_hms(2026, 4, 26, 12, 0, 0).unwrap()
        );
    }

    #[test]
    fn test_generate_ciops_time_steps_at_boundary() {
        // Exactly on a 6-hour boundary: 2026-04-24T06:00:00Z
        let reference = Utc.with_ymd_and_hms(2026, 4, 24, 6, 0, 0).unwrap();
        let steps = generate_ciops_time_steps(reference);

        assert_eq!(steps.len(), 9);
        assert_eq!(steps[0], Utc.with_ymd_and_hms(2026, 4, 24, 6, 0, 0).unwrap());
        assert_eq!(steps[8], Utc.with_ymd_and_hms(2026, 4, 26, 6, 0, 0).unwrap());
    }

    #[test]
    fn test_generate_ciops_time_steps_at_midnight() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 24, 0, 0, 0).unwrap();
        let steps = generate_ciops_time_steps(reference);

        assert_eq!(steps[0], Utc.with_ymd_and_hms(2026, 4, 24, 0, 0, 0).unwrap());
        assert_eq!(steps[8], Utc.with_ymd_and_hms(2026, 4, 26, 0, 0, 0).unwrap());
    }

    #[test]
    fn test_generate_ciops_time_steps_at_23_59() {
        // 23:59 → rounds down to 18:00
        let reference = Utc.with_ymd_and_hms(2026, 4, 24, 23, 59, 0).unwrap();
        let steps = generate_ciops_time_steps(reference);

        assert_eq!(steps[0], Utc.with_ymd_and_hms(2026, 4, 24, 18, 0, 0).unwrap());
    }

    #[test]
    fn test_generate_ciops_time_steps_span_48_hours() {
        let reference = Utc.with_ymd_and_hms(2026, 4, 24, 14, 30, 0).unwrap();
        let steps = generate_ciops_time_steps(reference);

        let span = steps[8] - steps[0];
        assert_eq!(span, TimeDelta::hours(48));
    }

    // -------------------------------------------------------------------
    // WMS URL builder tests
    // -------------------------------------------------------------------

    #[test]
    fn test_build_ciops_wms_url() {
        let time = Utc.with_ymd_and_hms(2026, 4, 24, 12, 0, 0).unwrap();
        let url = build_ciops_wms_url(48.5, -123.5, &time);

        assert!(url.starts_with("https://geo.weather.gc.ca/geomet?"));
        assert!(url.contains("SERVICE=WMS"));
        assert!(url.contains("VERSION=1.3.0"));
        assert!(url.contains("REQUEST=GetFeatureInfo"));
        assert!(url.contains(&format!("LAYERS={CIOPS_LAYER}")));
        assert!(url.contains("CRS=EPSG:4326"));
        assert!(url.contains("WIDTH=3"));
        assert!(url.contains("HEIGHT=3"));
        assert!(url.contains("I=1"));
        assert!(url.contains("J=1"));
        assert!(url.contains("INFO_FORMAT=application/json"));
        assert!(url.contains("TIME=2026-04-24T12:00:00Z"));
        // BBOX should be lat-0.01,lon-0.01,lat+0.01,lon+0.01
        assert!(url.contains("BBOX=48.49,-123.51,48.51,-123.49"));
    }

    // -------------------------------------------------------------------
    // Kelvin → Celsius conversion tests
    // -------------------------------------------------------------------

    #[test]
    fn test_kelvin_to_celsius_283_15() {
        let celsius = kelvin_to_celsius(283.15);
        assert!(
            (celsius - 10.0).abs() < 1e-9,
            "283.15K should be 10.0°C, got {celsius}"
        );
    }

    #[test]
    fn test_kelvin_to_celsius_273_15() {
        let celsius = kelvin_to_celsius(273.15);
        assert!(
            celsius.abs() < 1e-9,
            "273.15K should be 0.0°C, got {celsius}"
        );
    }

    #[test]
    fn test_kelvin_to_celsius_373_15() {
        let celsius = kelvin_to_celsius(373.15);
        assert!(
            (celsius - 100.0).abs() < 1e-9,
            "373.15K should be 100.0°C, got {celsius}"
        );
    }

    // -------------------------------------------------------------------
    // GeoJSON parsing tests
    // -------------------------------------------------------------------

    fn synthetic_geojson(kelvin: f64) -> Vec<u8> {
        let json = serde_json::json!({
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "properties": {
                        CIOPS_LAYER: kelvin
                    },
                    "geometry": {
                        "type": "Point",
                        "coordinates": [-123.5, 48.5]
                    }
                }
            ]
        });
        serde_json::to_vec(&json).unwrap()
    }

    #[test]
    fn test_parse_ciops_feature_info() {
        let raw = synthetic_geojson(283.15);
        let celsius = parse_ciops_feature_info(&raw);
        assert!(celsius.is_some());
        assert!((celsius.unwrap() - 10.0).abs() < 1e-9);
    }

    #[test]
    fn test_parse_ciops_feature_info_cold_water() {
        let raw = synthetic_geojson(275.65); // 2.5°C
        let celsius = parse_ciops_feature_info(&raw).unwrap();
        assert!((celsius - 2.5).abs() < 1e-9);
    }

    #[test]
    fn test_parse_ciops_feature_info_empty_features() {
        let json = serde_json::json!({
            "type": "FeatureCollection",
            "features": []
        });
        let raw = serde_json::to_vec(&json).unwrap();
        assert!(parse_ciops_feature_info(&raw).is_none());
    }

    #[test]
    fn test_parse_ciops_feature_info_missing_property() {
        let json = serde_json::json!({
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "properties": {
                        "some_other_layer": 283.15
                    }
                }
            ]
        });
        let raw = serde_json::to_vec(&json).unwrap();
        assert!(parse_ciops_feature_info(&raw).is_none());
    }

    #[test]
    fn test_parse_ciops_feature_info_null_value() {
        let json = serde_json::json!({
            "type": "FeatureCollection",
            "features": [
                {
                    "type": "Feature",
                    "properties": {
                        CIOPS_LAYER: null
                    }
                }
            ]
        });
        let raw = serde_json::to_vec(&json).unwrap();
        assert!(parse_ciops_feature_info(&raw).is_none());
    }

    #[test]
    fn test_parse_ciops_feature_info_invalid_json() {
        let raw = b"not json";
        assert!(parse_ciops_feature_info(raw).is_none());
    }

    #[test]
    fn test_parse_ciops_feature_info_missing_features() {
        let json = serde_json::json!({ "type": "FeatureCollection" });
        let raw = serde_json::to_vec(&json).unwrap();
        assert!(parse_ciops_feature_info(&raw).is_none());
    }

    // -------------------------------------------------------------------
    // Property test — Property 8: CIOPS time step generation
    // -------------------------------------------------------------------

    /// Feature: weather-backend-api, Property 8: CIOPS time step generation
    ///
    /// **Validates: Requirements 7.4**
    mod prop_ciops_time_steps {
        use super::*;
        use proptest::prelude::*;

        /// Strategy to generate random `DateTime<Utc>` values.
        ///
        /// Generates timestamps between 2020-01-01 and 2035-12-31 to cover
        /// a wide range of dates while staying within reasonable bounds.
        fn arb_datetime() -> impl Strategy<Value = DateTime<Utc>> {
            // Unix timestamps from 2020-01-01 to 2035-12-31
            (1_577_836_800i64..2_082_758_400i64).prop_map(|ts| {
                DateTime::from_timestamp(ts, 0).unwrap()
            })
        }

        proptest! {
            #[test]
            fn prop_ciops_time_step_generation(
                reference in arb_datetime(),
            ) {
                let steps = generate_ciops_time_steps(reference);

                // (a) Exactly 9 entries
                prop_assert_eq!(
                    steps.len(), 9,
                    "Expected 9 time steps, got {}",
                    steps.len()
                );

                // (b) 6-hour spacing between consecutive entries
                for i in 1..steps.len() {
                    let diff = steps[i] - steps[i - 1];
                    prop_assert_eq!(
                        diff,
                        TimeDelta::hours(6),
                        "Steps {} and {} differ by {:?}, expected 6 hours",
                        i - 1, i, diff
                    );
                }

                // (c) First entry at reference time rounded down to nearest
                //     6-hour boundary (00Z, 06Z, 12Z, or 18Z)
                let first_hour = steps[0].hour();
                prop_assert!(
                    first_hour % 6 == 0,
                    "First step hour {} is not a 6-hour boundary",
                    first_hour
                );
                prop_assert_eq!(
                    steps[0].minute(), 0,
                    "First step should have 0 minutes"
                );
                prop_assert_eq!(
                    steps[0].second(), 0,
                    "First step should have 0 seconds"
                );

                // The first step should be ≤ reference and the next 6-hour
                // boundary should be > reference
                prop_assert!(
                    steps[0] <= reference,
                    "First step {:?} should be ≤ reference {:?}",
                    steps[0], reference
                );
                let next_boundary = steps[0] + TimeDelta::hours(6);
                prop_assert!(
                    next_boundary > reference,
                    "Next boundary {:?} should be > reference {:?}",
                    next_boundary, reference
                );

                // (d) Span of 48 hours from first to last
                let span = steps[8] - steps[0];
                prop_assert_eq!(
                    span,
                    TimeDelta::hours(48),
                    "Span from first to last step should be 48 hours, got {:?}",
                    span
                );
            }
        }
    }
}
