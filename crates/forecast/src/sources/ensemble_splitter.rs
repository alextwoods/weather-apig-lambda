use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::sources::ensemble::ParsedEnsembleData;

// ---------------------------------------------------------------------------
// Per-model ensemble data
// ---------------------------------------------------------------------------

/// Per-model ensemble data extracted from the combined upstream response.
///
/// Contains only the hourly keys belonging to a single ensemble model.
/// Keys are the full flat API names (e.g.,
/// `temperature_2m_member00_ecmwf_ifs025_ensemble`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerModelEnsembleData {
    /// The model's API key suffix (e.g., `"ecmwf_ifs025_ensemble"`).
    pub model_suffix: String,
    /// Hourly data keys belonging to this model.
    pub hourly: HashMap<String, Vec<Option<f64>>>,
}

// ---------------------------------------------------------------------------
// Split
// ---------------------------------------------------------------------------

/// Splits a combined [`ParsedEnsembleData`] into per-model data.
///
/// Examines each key in the hourly map, extracts the model suffix using the
/// same logic as [`extract_members`](super::ensemble::extract_members) — find
/// `_member` in the key, skip the member number digits, then take everything
/// after the next underscore — and groups keys by that suffix.
pub fn split_ensemble_by_model(
    combined: &ParsedEnsembleData,
) -> HashMap<String, PerModelEnsembleData> {
    let mut result: HashMap<String, PerModelEnsembleData> = HashMap::new();

    for (key, values) in &combined.hourly {
        let model_suffix = extract_model_suffix(key);

        let entry = result
            .entry(model_suffix.clone())
            .or_insert_with(|| PerModelEnsembleData {
                model_suffix,
                hourly: HashMap::new(),
            });

        entry.hourly.insert(key.clone(), values.clone());
    }

    result
}

/// Extracts the model suffix from an hourly key.
///
/// Key format: `{variable}_member{NN}_{model_suffix}`
///
/// Finds `_member` in the key, then locates the first underscore after the
/// member number digits to split off the model suffix. Falls back to
/// `"unknown"` if the pattern doesn't match.
fn extract_model_suffix(key: &str) -> String {
    // Find "_member" in the key
    let member_marker = "_member";
    let Some(member_pos) = key.find(member_marker) else {
        return "unknown".to_string();
    };

    // After "_member" we have "{NN}_{model_suffix}"
    let remainder = &key[member_pos + member_marker.len()..];

    // Find the first underscore after the member number digits
    if let Some(underscore_pos) = remainder.find('_') {
        remainder[underscore_pos + 1..].to_string()
    } else {
        "unknown".to_string()
    }
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

/// Merges multiple per-model ensemble data sets into a single
/// [`ParsedEnsembleData`].
///
/// The resulting hourly map contains only the keys from the provided models.
pub fn merge_ensemble_models(
    times: Vec<String>,
    models: &[&PerModelEnsembleData],
) -> ParsedEnsembleData {
    let mut hourly = HashMap::new();

    for model in models {
        for (key, values) in &model.hourly {
            hourly.insert(key.clone(), values.clone());
        }
    }

    ParsedEnsembleData { times, hourly }
}

// ---------------------------------------------------------------------------
// Serialization for S3 caching
// ---------------------------------------------------------------------------

/// Intermediate JSON structure for per-model S3 cache objects.
#[derive(Serialize, Deserialize)]
struct PerModelCacheFormat {
    times: Vec<String>,
    model_suffix: String,
    hourly: HashMap<String, Vec<Option<f64>>>,
}

/// Serializes per-model ensemble data to JSON bytes for S3 caching.
///
/// The output format is a JSON object with `times`, `model_suffix`, and
/// `hourly` keys.
pub fn serialize_per_model(
    times: &[String],
    model_data: &PerModelEnsembleData,
) -> Result<Vec<u8>, String> {
    let cache_obj = PerModelCacheFormat {
        times: times.to_vec(),
        model_suffix: model_data.model_suffix.clone(),
        hourly: model_data.hourly.clone(),
    };

    serde_json::to_vec(&cache_obj).map_err(|e| format!("serialize error: {e}"))
}

/// Deserializes per-model ensemble data from cached JSON bytes.
///
/// Returns the `times` array and the [`PerModelEnsembleData`].
pub fn deserialize_per_model(raw: &[u8]) -> Result<(Vec<String>, PerModelEnsembleData), String> {
    let cache_obj: PerModelCacheFormat =
        serde_json::from_slice(raw).map_err(|e| format!("deserialize error: {e}"))?;

    let model_data = PerModelEnsembleData {
        model_suffix: cache_obj.model_suffix,
        hourly: cache_obj.hourly,
    };

    Ok((cache_obj.times, model_data))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a small synthetic combined ensemble response with 2 models,
    /// 1 variable, 2 members each, and 3 time steps.
    fn synthetic_combined() -> ParsedEnsembleData {
        let mut hourly = HashMap::new();
        hourly.insert(
            "temperature_2m_member00_ecmwf_ifs025_ensemble".to_string(),
            vec![Some(10.0), Some(11.0), Some(12.0)],
        );
        hourly.insert(
            "temperature_2m_member01_ecmwf_ifs025_ensemble".to_string(),
            vec![Some(10.5), Some(11.5), Some(12.5)],
        );
        hourly.insert(
            "temperature_2m_member00_ncep_gefs_seamless".to_string(),
            vec![Some(9.0), Some(10.0), Some(11.0)],
        );
        hourly.insert(
            "temperature_2m_member01_ncep_gefs_seamless".to_string(),
            vec![Some(9.5), Some(10.5), None],
        );

        ParsedEnsembleData {
            times: vec![
                "2026-04-24T00:00".to_string(),
                "2026-04-24T01:00".to_string(),
                "2026-04-24T02:00".to_string(),
            ],
            hourly,
        }
    }

    #[test]
    fn test_extract_model_suffix_standard_key() {
        assert_eq!(
            extract_model_suffix("temperature_2m_member00_ecmwf_ifs025_ensemble"),
            "ecmwf_ifs025_ensemble"
        );
    }

    #[test]
    fn test_extract_model_suffix_two_digit_member() {
        assert_eq!(
            extract_model_suffix("precipitation_member50_bom_access_global_ensemble"),
            "bom_access_global_ensemble"
        );
    }

    #[test]
    fn test_extract_model_suffix_no_member_marker() {
        assert_eq!(extract_model_suffix("time"), "unknown");
    }

    #[test]
    fn test_split_two_models() {
        let combined = synthetic_combined();
        let split = split_ensemble_by_model(&combined);

        assert_eq!(split.len(), 2);
        assert!(split.contains_key("ecmwf_ifs025_ensemble"));
        assert!(split.contains_key("ncep_gefs_seamless"));

        let ecmwf = &split["ecmwf_ifs025_ensemble"];
        assert_eq!(ecmwf.model_suffix, "ecmwf_ifs025_ensemble");
        assert_eq!(ecmwf.hourly.len(), 2);
        assert!(ecmwf
            .hourly
            .contains_key("temperature_2m_member00_ecmwf_ifs025_ensemble"));
        assert!(ecmwf
            .hourly
            .contains_key("temperature_2m_member01_ecmwf_ifs025_ensemble"));

        let ncep = &split["ncep_gefs_seamless"];
        assert_eq!(ncep.model_suffix, "ncep_gefs_seamless");
        assert_eq!(ncep.hourly.len(), 2);
    }

    #[test]
    fn test_split_single_model() {
        let mut hourly = HashMap::new();
        hourly.insert(
            "wind_speed_10m_member00_icon_seamless_eps".to_string(),
            vec![Some(5.0)],
        );
        let combined = ParsedEnsembleData {
            times: vec!["2026-04-24T00:00".to_string()],
            hourly,
        };

        let split = split_ensemble_by_model(&combined);
        assert_eq!(split.len(), 1);
        assert!(split.contains_key("icon_seamless_eps"));
    }

    #[test]
    fn test_split_preserves_null_values() {
        let combined = synthetic_combined();
        let split = split_ensemble_by_model(&combined);

        let ncep = &split["ncep_gefs_seamless"];
        let vals = &ncep.hourly["temperature_2m_member01_ncep_gefs_seamless"];
        assert_eq!(vals[2], None);
    }

    #[test]
    fn test_merge_two_models() {
        let combined = synthetic_combined();
        let split = split_ensemble_by_model(&combined);

        let ecmwf = &split["ecmwf_ifs025_ensemble"];
        let ncep = &split["ncep_gefs_seamless"];

        let merged = merge_ensemble_models(combined.times.clone(), &[ecmwf, ncep]);

        assert_eq!(merged.times, combined.times);
        assert_eq!(merged.hourly.len(), 4);
        assert_eq!(
            merged.hourly["temperature_2m_member00_ecmwf_ifs025_ensemble"],
            vec![Some(10.0), Some(11.0), Some(12.0)]
        );
    }

    #[test]
    fn test_merge_zero_models() {
        let merged = merge_ensemble_models(
            vec!["2026-04-24T00:00".to_string()],
            &[],
        );
        assert_eq!(merged.times.len(), 1);
        assert!(merged.hourly.is_empty());
    }

    #[test]
    fn test_merge_subset_of_models() {
        let combined = synthetic_combined();
        let split = split_ensemble_by_model(&combined);

        let ecmwf = &split["ecmwf_ifs025_ensemble"];
        let merged = merge_ensemble_models(combined.times.clone(), &[ecmwf]);

        // Only ECMWF keys should be present
        assert_eq!(merged.hourly.len(), 2);
        assert!(merged
            .hourly
            .contains_key("temperature_2m_member00_ecmwf_ifs025_ensemble"));
        assert!(!merged
            .hourly
            .contains_key("temperature_2m_member00_ncep_gefs_seamless"));
    }

    #[test]
    fn test_serialize_deserialize_round_trip() {
        let combined = synthetic_combined();
        let split = split_ensemble_by_model(&combined);
        let ecmwf = &split["ecmwf_ifs025_ensemble"];

        let bytes = serialize_per_model(&combined.times, ecmwf).unwrap();
        let (times, deserialized) = deserialize_per_model(&bytes).unwrap();

        assert_eq!(times, combined.times);
        assert_eq!(deserialized.model_suffix, ecmwf.model_suffix);
        assert_eq!(deserialized.hourly.len(), ecmwf.hourly.len());
        for (key, values) in &ecmwf.hourly {
            assert_eq!(&deserialized.hourly[key], values);
        }
    }

    #[test]
    fn test_serialize_deserialize_preserves_nulls() {
        let combined = synthetic_combined();
        let split = split_ensemble_by_model(&combined);
        let ncep = &split["ncep_gefs_seamless"];

        let bytes = serialize_per_model(&combined.times, ncep).unwrap();
        let (_, deserialized) = deserialize_per_model(&bytes).unwrap();

        let vals = &deserialized.hourly["temperature_2m_member01_ncep_gefs_seamless"];
        assert_eq!(vals[2], None);
    }

    #[test]
    fn test_deserialize_invalid_json() {
        let result = deserialize_per_model(b"not json");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("deserialize error"));
    }

    #[test]
    fn test_split_merge_round_trip_all_models() {
        let combined = synthetic_combined();
        let split = split_ensemble_by_model(&combined);

        let all_models: Vec<&PerModelEnsembleData> = split.values().collect();
        let merged = merge_ensemble_models(combined.times.clone(), &all_models);

        assert_eq!(merged.times, combined.times);
        assert_eq!(merged.hourly.len(), combined.hourly.len());
        for (key, values) in &combined.hourly {
            assert_eq!(&merged.hourly[key], values);
        }
    }
}
