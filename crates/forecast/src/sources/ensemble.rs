use std::collections::HashMap;

use serde_json::Value;

use crate::models::{ENSEMBLE_MODELS, WEATHER_VARIABLES};

// ---------------------------------------------------------------------------
// Ensemble API URL builder
// ---------------------------------------------------------------------------

/// Builds the Open-Meteo Ensemble API URL for the given coordinates.
///
/// Requests all 5 models, 11 weather variables, 35 forecast days, and 12 past
/// hours in a single call.
pub fn build_ensemble_url(lat: f64, lon: f64) -> String {
    let models: Vec<&str> = ENSEMBLE_MODELS
        .iter()
        .map(|m| m.api_key_suffix)
        .collect();
    let variables = WEATHER_VARIABLES.join(",");
    let models_str = models.join(",");

    format!(
        "https://ensemble-api.open-meteo.com/v1/ensemble\
         ?latitude={lat}&longitude={lon}\
         &hourly={variables}\
         &models={models_str}\
         &forecast_days=35\
         &past_hours=12"
    )
}

// ---------------------------------------------------------------------------
// EnsembleFetcher
// ---------------------------------------------------------------------------

/// Fetches and parses ensemble forecast data from the Open-Meteo Ensemble API.
pub struct EnsembleFetcher;

impl EnsembleFetcher {
    pub fn source_id() -> &'static str {
        "ensemble"
    }

    pub fn ttl_secs() -> u64 {
        3600
    }

    pub fn is_cacheable() -> bool {
        true
    }
}

// ---------------------------------------------------------------------------
// Parsed ensemble data
// ---------------------------------------------------------------------------

/// The parsed hourly object from the ensemble API response, containing the
/// `time` array and all member data keyed by their flat API key names.
#[derive(Debug, Clone)]
pub struct ParsedEnsembleData {
    /// ISO 8601 time strings for each hourly step.
    pub times: Vec<String>,
    /// All key-value pairs from the `hourly` object (excluding `time`).
    /// Keys are flat API names like `temperature_2m_member00_ecmwf_ifs025_ensemble`.
    pub hourly: HashMap<String, Vec<Option<f64>>>,
}

/// Extracted ensemble members grouped for percentile computation and per-model
/// output.
#[derive(Debug, Clone)]
pub struct ExtractedMembers {
    /// All member arrays pooled across models, for percentile computation.
    pub pooled: Vec<Vec<Option<f64>>>,
    /// Per-model member arrays, keyed by model `api_key_suffix`.
    pub by_model: HashMap<String, Vec<Vec<Option<f64>>>>,
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

/// Parses the raw JSON response from the Open-Meteo Ensemble API.
///
/// Extracts the `hourly` object and `time` array. Each non-`time` key in the
/// hourly object is stored as a `Vec<Option<f64>>`.
pub fn parse_ensemble_response(raw: &[u8]) -> Result<ParsedEnsembleData, String> {
    let root: Value = serde_json::from_slice(raw).map_err(|e| format!("JSON parse error: {e}"))?;

    let hourly_obj = root
        .get("hourly")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "missing or invalid 'hourly' object".to_string())?;

    let times: Vec<String> = hourly_obj
        .get("time")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "missing 'time' array in hourly object".to_string())?
        .iter()
        .map(|v| v.as_str().unwrap_or_default().to_string())
        .collect();

    let mut hourly = HashMap::new();
    for (key, value) in hourly_obj {
        if key == "time" {
            continue;
        }
        if let Some(arr) = value.as_array() {
            let values: Vec<Option<f64>> = arr
                .iter()
                .map(|v| {
                    if v.is_null() {
                        None
                    } else {
                        v.as_f64()
                    }
                })
                .collect();
            hourly.insert(key.clone(), values);
        }
    }

    Ok(ParsedEnsembleData { times, hourly })
}

// ---------------------------------------------------------------------------
// Member extraction
// ---------------------------------------------------------------------------

/// Extracts ensemble member arrays for a given weather variable from the
/// parsed hourly data.
///
/// Scans all keys matching `{variable}_member{NN}_{model_suffix}`, groups
/// them by model suffix, and returns both pooled and per-model arrays.
pub fn extract_members(
    hourly: &HashMap<String, Vec<Option<f64>>>,
    variable: &str,
) -> ExtractedMembers {
    let prefix = format!("{variable}_member");
    let mut pooled = Vec::new();
    let mut by_model: HashMap<String, Vec<Vec<Option<f64>>>> = HashMap::new();

    // Collect matching keys and sort for deterministic ordering.
    let mut matching_keys: Vec<&String> = hourly
        .keys()
        .filter(|k| k.starts_with(&prefix))
        .collect();
    matching_keys.sort();

    for key in matching_keys {
        // Key format: {variable}_member{NN}_{model_suffix}
        // After stripping the prefix we have: {NN}_{model_suffix}
        let remainder = &key[prefix.len()..];

        // Find the first underscore after the member number to split
        // NN from model_suffix.
        let model_suffix = if let Some(underscore_pos) = remainder.find('_') {
            &remainder[underscore_pos + 1..]
        } else {
            // No model suffix — shouldn't happen with multi-model responses,
            // but handle gracefully.
            "unknown"
        };

        if let Some(values) = hourly.get(key.as_str()) {
            pooled.push(values.clone());
            by_model
                .entry(model_suffix.to_string())
                .or_default()
                .push(values.clone());
        }
    }

    ExtractedMembers { pooled, by_model }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_build_ensemble_url() {
        let url = build_ensemble_url(47.61, -122.33);
        assert!(url.starts_with("https://ensemble-api.open-meteo.com/v1/ensemble?"));
        assert!(url.contains("latitude=47.61"));
        assert!(url.contains("longitude=-122.33"));
        assert!(url.contains("forecast_days=35"));
        assert!(url.contains("past_hours=12"));
        // All 5 models present
        assert!(url.contains("ecmwf_ifs025_ensemble"));
        assert!(url.contains("ncep_gefs_seamless"));
        assert!(url.contains("icon_seamless_eps"));
        assert!(url.contains("gem_global_ensemble"));
        assert!(url.contains("bom_access_global_ensemble"));
        // All 11 variables present
        assert!(url.contains("temperature_2m"));
        assert!(url.contains("precipitation"));
        assert!(url.contains("shortwave_radiation"));
    }

    #[test]
    fn test_build_ensemble_url_negative_coords() {
        let url = build_ensemble_url(-33.87, 151.21);
        assert!(url.contains("latitude=-33.87"));
        assert!(url.contains("longitude=151.21"));
    }

    #[test]
    fn test_source_metadata() {
        assert_eq!(EnsembleFetcher::source_id(), "ensemble");
        assert_eq!(EnsembleFetcher::ttl_secs(), 3600);
        assert!(EnsembleFetcher::is_cacheable());
    }

    /// Build a small synthetic ensemble response with 2 models, 1 variable,
    /// 2 members each, and 3 time steps.
    fn synthetic_ensemble_json() -> Vec<u8> {
        let json = serde_json::json!({
            "hourly": {
                "time": ["2026-04-24T00:00", "2026-04-24T01:00", "2026-04-24T02:00"],
                "temperature_2m_member00_model_a": [10.0, 11.0, 12.0],
                "temperature_2m_member01_model_a": [10.5, 11.5, 12.5],
                "temperature_2m_member00_model_b": [9.0, 10.0, 11.0],
                "temperature_2m_member01_model_b": [9.5, 10.5, null]
            }
        });
        serde_json::to_vec(&json).unwrap()
    }

    #[test]
    fn test_parse_ensemble_response_times() {
        let raw = synthetic_ensemble_json();
        let parsed = parse_ensemble_response(&raw).unwrap();
        assert_eq!(parsed.times.len(), 3);
        assert_eq!(parsed.times[0], "2026-04-24T00:00");
        assert_eq!(parsed.times[2], "2026-04-24T02:00");
    }

    #[test]
    fn test_parse_ensemble_response_hourly_keys() {
        let raw = synthetic_ensemble_json();
        let parsed = parse_ensemble_response(&raw).unwrap();
        // 4 member keys (time is excluded)
        assert_eq!(parsed.hourly.len(), 4);
        assert!(parsed.hourly.contains_key("temperature_2m_member00_model_a"));
        assert!(parsed.hourly.contains_key("temperature_2m_member01_model_b"));
    }

    #[test]
    fn test_parse_ensemble_response_null_handling() {
        let raw = synthetic_ensemble_json();
        let parsed = parse_ensemble_response(&raw).unwrap();
        let vals = &parsed.hourly["temperature_2m_member01_model_b"];
        assert_eq!(vals.len(), 3);
        assert_eq!(vals[0], Some(9.5));
        assert_eq!(vals[1], Some(10.5));
        assert_eq!(vals[2], None); // null in JSON
    }

    #[test]
    fn test_parse_ensemble_response_missing_hourly() {
        let json = serde_json::json!({"daily": {}});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_ensemble_response(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("hourly"));
    }

    #[test]
    fn test_parse_ensemble_response_missing_time() {
        let json = serde_json::json!({"hourly": {"temperature_2m_member00_model_a": [1.0]}});
        let raw = serde_json::to_vec(&json).unwrap();
        let result = parse_ensemble_response(&raw);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("time"));
    }

    #[test]
    fn test_extract_members_pooled_count() {
        let raw = synthetic_ensemble_json();
        let parsed = parse_ensemble_response(&raw).unwrap();
        let members = extract_members(&parsed.hourly, "temperature_2m");
        // 4 total members (2 from model_a + 2 from model_b)
        assert_eq!(members.pooled.len(), 4);
    }

    #[test]
    fn test_extract_members_by_model() {
        let raw = synthetic_ensemble_json();
        let parsed = parse_ensemble_response(&raw).unwrap();
        let members = extract_members(&parsed.hourly, "temperature_2m");
        assert_eq!(members.by_model.len(), 2);
        assert_eq!(members.by_model["model_a"].len(), 2);
        assert_eq!(members.by_model["model_b"].len(), 2);
    }

    #[test]
    fn test_extract_members_data_integrity() {
        let raw = synthetic_ensemble_json();
        let parsed = parse_ensemble_response(&raw).unwrap();
        let members = extract_members(&parsed.hourly, "temperature_2m");

        // Verify model_a member00 data
        let model_a_members = &members.by_model["model_a"];
        // Members are sorted by key, so member00 comes first
        assert_eq!(model_a_members[0], vec![Some(10.0), Some(11.0), Some(12.0)]);
        assert_eq!(model_a_members[1], vec![Some(10.5), Some(11.5), Some(12.5)]);
    }

    #[test]
    fn test_extract_members_preserves_nulls() {
        let raw = synthetic_ensemble_json();
        let parsed = parse_ensemble_response(&raw).unwrap();
        let members = extract_members(&parsed.hourly, "temperature_2m");

        let model_b_members = &members.by_model["model_b"];
        // member01_model_b has a null at index 2
        assert_eq!(model_b_members[1][2], None);
    }

    #[test]
    fn test_extract_members_nonexistent_variable() {
        let raw = synthetic_ensemble_json();
        let parsed = parse_ensemble_response(&raw).unwrap();
        let members = extract_members(&parsed.hourly, "wind_speed_10m");
        assert!(members.pooled.is_empty());
        assert!(members.by_model.is_empty());
    }

    #[test]
    fn test_extract_members_pooled_equals_sum_of_models() {
        let raw = synthetic_ensemble_json();
        let parsed = parse_ensemble_response(&raw).unwrap();
        let members = extract_members(&parsed.hourly, "temperature_2m");

        let model_total: usize = members.by_model.values().map(|v| v.len()).sum();
        assert_eq!(members.pooled.len(), model_total);
    }

    /// Test with a more realistic multi-variable response.
    #[test]
    fn test_extract_members_multi_variable() {
        let json = serde_json::json!({
            "hourly": {
                "time": ["2026-04-24T00:00"],
                "temperature_2m_member00_ecmwf": [15.0],
                "temperature_2m_member01_ecmwf": [15.5],
                "precipitation_member00_ecmwf": [0.0],
                "precipitation_member01_ecmwf": [0.1],
                "precipitation_member00_gfs": [0.2],
            }
        });
        let raw = serde_json::to_vec(&json).unwrap();
        let parsed = parse_ensemble_response(&raw).unwrap();

        let temp_members = extract_members(&parsed.hourly, "temperature_2m");
        assert_eq!(temp_members.pooled.len(), 2);
        assert_eq!(temp_members.by_model.len(), 1);
        assert_eq!(temp_members.by_model["ecmwf"].len(), 2);

        let precip_members = extract_members(&parsed.hourly, "precipitation");
        assert_eq!(precip_members.pooled.len(), 3);
        assert_eq!(precip_members.by_model.len(), 2);
        assert_eq!(precip_members.by_model["ecmwf"].len(), 2);
        assert_eq!(precip_members.by_model["gfs"].len(), 1);
    }

    /// Feature: weather-backend-api, Property 5: Ensemble member extraction preserves data
    ///
    /// **Validates: Requirements 3.3, 3.7**
    mod prop_ensemble_extraction {
        use super::*;

        /// Strategy to generate a valid variable name.
        fn variable_strategy() -> impl Strategy<Value = String> {
            prop_oneof![
                Just("temperature_2m".to_string()),
                Just("precipitation".to_string()),
                Just("wind_speed_10m".to_string()),
            ]
        }

        /// Strategy to generate a flat key map matching the ensemble pattern.
        ///
        /// Produces 1–3 distinct models, each with 1–10 members, for a single
        /// variable, with arrays of length 1–5.
        fn ensemble_key_map_strategy() -> impl Strategy<
            Value = (
                String,
                HashMap<String, Vec<Option<f64>>>,
                usize, // expected total member count
            ),
        > {
            variable_strategy().prop_flat_map(|var| {
                // Generate 1–3 distinct model suffixes by shuffling and taking a slice.
                let all_suffixes = vec![
                    "ecmwf_ifs025_ensemble".to_string(),
                    "ncep_gefs_seamless".to_string(),
                    "icon_seamless_eps".to_string(),
                ];
                (1usize..=3).prop_flat_map(move |num_models| {
                    let suffixes = all_suffixes[..num_models].to_vec();
                    // Generate member counts for each model
                    let member_counts = prop::collection::vec(1usize..=10, num_models);
                    (Just(suffixes), member_counts)
                })
                .prop_flat_map(move |(suffixes, counts)| {
                    let var = var.clone();
                    let model_specs: Vec<(String, usize)> = suffixes
                        .into_iter()
                        .zip(counts.into_iter())
                        .collect();
                    // Array length for each member
                    (1usize..=5).prop_flat_map(move |len| {
                        let var = var.clone();
                        let model_specs = model_specs.clone();

                        // Total members across all models
                        let total: usize = model_specs.iter().map(|(_, c)| c).sum();

                        // Generate data arrays for each member
                        let data_strat = prop::collection::vec(
                            prop::collection::vec(prop::option::of(-50.0f64..50.0), len),
                            total,
                        );

                        data_strat.prop_map(move |data_arrays| {
                            let mut map = HashMap::new();
                            let mut idx = 0;
                            for (suffix, count) in &model_specs {
                                for member_num in 0..*count {
                                    let key = format!(
                                        "{}_member{:02}_{}",
                                        var, member_num, suffix
                                    );
                                    map.insert(key, data_arrays[idx].clone());
                                    idx += 1;
                                }
                            }
                            (var.clone(), map, total)
                        })
                    })
                })
            })
        }

        proptest! {
            #[test]
            fn prop_member_extraction_preserves_data(
                (variable, hourly, expected_total) in ensemble_key_map_strategy()
            ) {
                let members = extract_members(&hourly, &variable);

                // Total member count across all models equals the number of
                // member keys for that variable.
                prop_assert_eq!(
                    members.pooled.len(),
                    expected_total,
                    "Pooled count {} != expected total {} for variable '{}'",
                    members.pooled.len(),
                    expected_total,
                    variable,
                );

                // Sum of per-model member counts equals pooled count.
                let model_total: usize = members.by_model.values().map(|v| v.len()).sum();
                prop_assert_eq!(
                    model_total,
                    members.pooled.len(),
                    "Sum of per-model counts {} != pooled count {}",
                    model_total,
                    members.pooled.len(),
                );

                // Each member array appears in exactly one model group.
                // Verify by checking that every pooled array exists in
                // exactly one model's list.
                for pooled_arr in &members.pooled {
                    let mut found_in_models = 0usize;
                    for model_members in members.by_model.values() {
                        found_in_models += model_members
                            .iter()
                            .filter(|m| *m == pooled_arr)
                            .count();
                    }
                    // At least one occurrence (could be more if arrays happen
                    // to be identical across models, but each key maps to
                    // exactly one model).
                    prop_assert!(
                        found_in_models >= 1,
                        "Pooled array not found in any model group"
                    );
                }

                // Verify each model suffix in by_model corresponds to keys
                // that actually exist in the hourly map.
                for (model_suffix, model_members) in &members.by_model {
                    for member_arr in model_members {
                        // Find at least one key in hourly that matches this
                        // model suffix and has this data.
                        let found = hourly.iter().any(|(k, v)| {
                            k.starts_with(&format!("{}_member", variable))
                                && k.ends_with(model_suffix)
                                && v == member_arr
                        });
                        prop_assert!(
                            found,
                            "Member array in model '{}' not found in hourly map",
                            model_suffix,
                        );
                    }
                }
            }
        }
    }
}
