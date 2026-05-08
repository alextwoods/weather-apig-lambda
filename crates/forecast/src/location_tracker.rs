use std::collections::HashMap;

use aws_sdk_dynamodb::types::AttributeValue;
use chrono::Utc;

use crate::models::ENSEMBLE_MODELS;

// ---------------------------------------------------------------------------
// Location access tracking
// ---------------------------------------------------------------------------

/// Records a location access in the tracker table (fire-and-forget).
///
/// Performs a DynamoDB PutItem with:
/// - `cache_key` (String, partition key) — the coordinate pair, e.g. "47.61_-122.33"
/// - `last_accessed` (String) — ISO 8601 UTC timestamp
/// - `expires_at` (Number) — Unix epoch timestamp, 7 days from now (DynamoDB TTL)
///
/// Errors are logged and swallowed so that tracking never blocks or fails
/// the forecast response.
pub async fn record_access(
    client: &aws_sdk_dynamodb::Client,
    table_name: &str,
    cache_key: &str,
) {
    let now = Utc::now();
    let last_accessed = now.to_rfc3339();
    let expires_at = now.timestamp() + 7 * 24 * 60 * 60; // 7 days

    if let Err(err) = client
        .put_item()
        .table_name(table_name)
        .item("cache_key", AttributeValue::S(cache_key.to_string()))
        .item("last_accessed", AttributeValue::S(last_accessed))
        .item("expires_at", AttributeValue::N(expires_at.to_string()))
        .send()
        .await
    {
        tracing::warn!(
            table = %table_name,
            cache_key = %cache_key,
            error = %err,
            "Failed to record location access"
        );
    }
}

/// Scans the tracker table and returns all active location cache keys.
///
/// Performs a full DynamoDB Scan, handling pagination, and collects every
/// `cache_key` value into a `Vec<String>`. Errors are logged and result
/// in an empty vector so that callers can degrade gracefully.
pub async fn scan_active_locations(
    client: &aws_sdk_dynamodb::Client,
    table_name: &str,
) -> Vec<String> {
    let mut cache_keys = Vec::new();
    let mut exclusive_start_key = None;

    loop {
        let mut scan = client.scan().table_name(table_name);

        if let Some(start_key) = exclusive_start_key.take() {
            scan = scan.set_exclusive_start_key(Some(start_key));
        }

        match scan.send().await {
            Ok(output) => {
                if let Some(items) = output.items {
                    for item in &items {
                        if let Some(val) = item.get("cache_key") {
                            if let Ok(key) = val.as_s() {
                                cache_keys.push(key.clone());
                            }
                        }
                    }
                }

                // Continue scanning if there are more pages.
                match output.last_evaluated_key {
                    Some(last_key) if !last_key.is_empty() => {
                        exclusive_start_key = Some(last_key);
                    }
                    _ => break,
                }
            }
            Err(err) => {
                tracing::warn!(
                    table = %table_name,
                    error = %err,
                    "Failed to scan location tracker table"
                );
                break;
            }
        }
    }

    cache_keys
}

// ---------------------------------------------------------------------------
// Parameter combination encoding/decoding
// ---------------------------------------------------------------------------

/// Encodes a parameter combination as a string for storage.
///
/// Format: `{sorted_models_or_all}:{forecast_days}`
/// Examples: "all:10", "ecmwf_ifs025_ensemble,ncep_gefs_seamless:7"
pub fn encode_param_combination(models: Option<&[String]>, forecast_days: u32) -> String {
    let models_segment = match models {
        None => "all".to_string(),
        Some(list) => {
            let mut sorted: Vec<&str> = list.iter().map(|s| s.as_str()).collect();
            sorted.sort();
            // Check if this is the full set of 5 models
            let mut all_sorted: Vec<&str> =
                ENSEMBLE_MODELS.iter().map(|m| m.api_key_suffix).collect();
            all_sorted.sort();
            if sorted == all_sorted {
                "all".to_string()
            } else {
                sorted.join(",")
            }
        }
    };

    format!("{}:{}", models_segment, forecast_days)
}

/// Decodes a parameter combination string back into (models, forecast_days).
///
/// Returns `None` if the string cannot be parsed.
pub fn decode_param_combination(encoded: &str) -> Option<(Option<Vec<String>>, u32)> {
    let colon_pos = encoded.rfind(':')?;
    let models_segment = &encoded[..colon_pos];
    let days_str = &encoded[colon_pos + 1..];

    let forecast_days: u32 = days_str.parse().ok()?;

    let models = if models_segment == "all" {
        None
    } else {
        Some(
            models_segment
                .split(',')
                .map(|s| s.to_string())
                .collect(),
        )
    };

    Some((models, forecast_days))
}

// ---------------------------------------------------------------------------
// Parameter combination tracking with LRU eviction
// ---------------------------------------------------------------------------

/// Records a location access with parameter combination tracking.
///
/// Performs a DynamoDB operation that:
/// - Updates `last_accessed` timestamp
/// - Adds the parameter combination to the `param_combinations` set
/// - Maintains the `param_last_used` map for LRU tracking
/// - Enforces the 10-combination limit with LRU eviction
///
/// Errors are logged and swallowed (fire-and-forget).
pub async fn record_access_with_params(
    client: &aws_sdk_dynamodb::Client,
    table_name: &str,
    cache_key: &str,
    models: Option<&[String]>,
    forecast_days: u32,
) {
    let now = Utc::now();
    let last_accessed = now.to_rfc3339();
    let expires_at = now.timestamp() + 7 * 24 * 60 * 60; // 7 days
    let encoded_combo = encode_param_combination(models, forecast_days);

    // Read current item to check param_combinations count
    let current_item = match client
        .get_item()
        .table_name(table_name)
        .key("cache_key", AttributeValue::S(cache_key.to_string()))
        .send()
        .await
    {
        Ok(output) => output.item,
        Err(err) => {
            tracing::warn!(
                table = %table_name,
                cache_key = %cache_key,
                error = %err,
                "Failed to read location tracker item for param tracking"
            );
            // Fall back to simple record_access behavior
            record_access(client, table_name, cache_key).await;
            return;
        }
    };

    // Extract current param_combinations and param_last_used
    let mut param_combinations: Vec<String> = current_item
        .as_ref()
        .and_then(|item| item.get("param_combinations"))
        .and_then(|val| val.as_ss().ok())
        .cloned()
        .unwrap_or_default();

    let mut param_last_used: HashMap<String, String> = current_item
        .as_ref()
        .and_then(|item| item.get("param_last_used"))
        .and_then(|val| val.as_m().ok())
        .map(|m| {
            m.iter()
                .filter_map(|(k, v)| v.as_s().ok().map(|s| (k.clone(), s.clone())))
                .collect()
        })
        .unwrap_or_default();

    // Check if this combination already exists
    let already_exists = param_combinations.contains(&encoded_combo);

    if !already_exists && param_combinations.len() >= 10 {
        // Need to evict the LRU combination
        if let Some(lru_combo) = find_lru_combination(&param_combinations, &param_last_used) {
            param_combinations.retain(|c| c != &lru_combo);
            param_last_used.remove(&lru_combo);
        }
    }

    // Add the new combination (if not already present)
    if !already_exists {
        param_combinations.push(encoded_combo.clone());
    }

    // Update the timestamp for this combination
    param_last_used.insert(encoded_combo, last_accessed.clone());

    // Build the param_last_used map as DynamoDB AttributeValue
    let param_last_used_av: HashMap<String, AttributeValue> = param_last_used
        .into_iter()
        .map(|(k, v)| (k, AttributeValue::S(v)))
        .collect();

    // Write the updated item
    let mut put = client
        .put_item()
        .table_name(table_name)
        .item("cache_key", AttributeValue::S(cache_key.to_string()))
        .item("last_accessed", AttributeValue::S(last_accessed))
        .item("expires_at", AttributeValue::N(expires_at.to_string()));

    if !param_combinations.is_empty() {
        put = put.item(
            "param_combinations",
            AttributeValue::Ss(param_combinations),
        );
    }

    if !param_last_used_av.is_empty() {
        put = put.item("param_last_used", AttributeValue::M(param_last_used_av));
    }

    if let Err(err) = put.send().await {
        tracing::warn!(
            table = %table_name,
            cache_key = %cache_key,
            error = %err,
            "Failed to record location access with params"
        );
    }
}

/// Finds the least-recently-used combination from the set based on
/// `param_last_used` timestamps.
///
/// This is a pure function extracted for testability.
pub fn find_lru_combination(
    combinations: &[String],
    last_used: &HashMap<String, String>,
) -> Option<String> {
    combinations
        .iter()
        .min_by_key(|combo| last_used.get(*combo).cloned().unwrap_or_default())
        .cloned()
}

/// Retrieves the parameter combinations for a location from the tracker.
///
/// Performs a DynamoDB GetItem to read `param_combinations` StringSet.
/// Decodes each combination string and returns as `Vec<(Option<Vec<String>>, u32)>`.
/// Returns empty Vec on errors or missing item.
pub async fn get_param_combinations(
    client: &aws_sdk_dynamodb::Client,
    table_name: &str,
    cache_key: &str,
) -> Vec<(Option<Vec<String>>, u32)> {
    let result = match client
        .get_item()
        .table_name(table_name)
        .key("cache_key", AttributeValue::S(cache_key.to_string()))
        .send()
        .await
    {
        Ok(output) => output,
        Err(err) => {
            tracing::warn!(
                table = %table_name,
                cache_key = %cache_key,
                error = %err,
                "Failed to read param combinations from location tracker"
            );
            return Vec::new();
        }
    };

    let item = match result.item {
        Some(item) => item,
        None => return Vec::new(),
    };

    let combinations = match item.get("param_combinations") {
        Some(val) => match val.as_ss() {
            Ok(ss) => ss.clone(),
            Err(_) => return Vec::new(),
        },
        None => return Vec::new(),
    };

    combinations
        .iter()
        .filter_map(|encoded| decode_param_combination(encoded))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;
    use proptest::sample::subsequence;

    const ALL_MODEL_SUFFIXES: [&str; 5] = [
        "bom_access_global_ensemble",
        "ecmwf_ifs025_ensemble",
        "gem_global_ensemble",
        "icon_seamless_eps",
        "ncep_gefs_seamless",
    ];

    /// Feature: response-cache-warming, Property 7: Parameter combination encoding round-trip
    ///
    /// **Validates: Requirements 4.2**
    mod prop_param_combination_round_trip {
        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            #[test]
            fn prop_none_models_round_trip(
                forecast_days in 1u32..=35,
            ) {
                let encoded = encode_param_combination(None, forecast_days);
                let decoded = decode_param_combination(&encoded);

                prop_assert_eq!(decoded.clone(), Some((None, forecast_days)),
                    "None models round-trip failed: encoded='{}', decoded={:?}",
                    encoded, decoded);
            }

            #[test]
            fn prop_subset_models_round_trip(
                forecast_days in 1u32..=35,
                model_indices in subsequence((0..5).collect::<Vec<usize>>(), 1..=4),
            ) {
                let models: Vec<String> = model_indices.iter()
                    .map(|&i| ALL_MODEL_SUFFIXES[i].to_string())
                    .collect();

                let encoded = encode_param_combination(Some(&models), forecast_days);
                let decoded = decode_param_combination(&encoded);

                // The decoded models should be sorted (encoding sorts them)
                let mut expected_models: Vec<String> = models.clone();
                expected_models.sort();

                prop_assert_eq!(decoded.clone(), Some((Some(expected_models), forecast_days)),
                    "Subset models round-trip failed: input={:?}, encoded='{}', decoded={:?}",
                    models, encoded, decoded);
            }

            #[test]
            fn prop_all_models_explicit_canonicalizes(
                forecast_days in 1u32..=35,
            ) {
                let all_models: Vec<String> = ALL_MODEL_SUFFIXES.iter()
                    .map(|s| s.to_string())
                    .collect();

                let encoded = encode_param_combination(Some(&all_models), forecast_days);

                // Should canonicalize to "all:N"
                prop_assert_eq!(&encoded, &format!("all:{}", forecast_days),
                    "All models should canonicalize to 'all': got '{}'", encoded);

                // Decoding should give (None, days)
                let decoded = decode_param_combination(&encoded);
                prop_assert_eq!(decoded.clone(), Some((None, forecast_days)),
                    "Decoding 'all:N' should give (None, N): got {:?}", decoded);
            }
        }
    }

    /// Feature: response-cache-warming, Property 8: Location tracker bounded LRU
    ///
    /// **Validates: Requirements 4.3, 4.4**
    mod prop_bounded_lru {
        use super::*;
        use proptest::collection::vec;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            #[test]
            fn prop_lru_never_exceeds_10(
                // Generate a sequence of 5-20 unique combination strings
                num_combos in 5usize..=20,
            ) {
                // Simulate recording combinations with incrementing timestamps
                let mut combinations: Vec<String> = Vec::new();
                let mut last_used: HashMap<String, String> = HashMap::new();

                for i in 0..num_combos {
                    let combo = format!("model_{}:{}", i % 7, (i % 35) + 1);
                    let timestamp = format!("2025-01-15T{:02}:00:00Z", i % 24);

                    let already_exists = combinations.contains(&combo);

                    if !already_exists && combinations.len() >= 10 {
                        // Evict LRU
                        if let Some(lru) = find_lru_combination(&combinations, &last_used) {
                            combinations.retain(|c| c != &lru);
                            last_used.remove(&lru);
                        }
                    }

                    if !already_exists {
                        combinations.push(combo.clone());
                    }
                    last_used.insert(combo, timestamp);

                    // Invariant: never exceeds 10
                    prop_assert!(combinations.len() <= 10,
                        "Combinations exceeded 10: len={} after inserting combo {}",
                        combinations.len(), i);
                }
            }

            #[test]
            fn prop_lru_evicts_oldest_timestamp(
                // Generate timestamps as offsets to ensure ordering
                timestamps in vec(0u32..1000, 10..=10),
            ) {
                // Set up 10 combinations with known timestamps
                let mut combinations: Vec<String> = Vec::new();
                let mut last_used: HashMap<String, String> = HashMap::new();

                for (i, &ts) in timestamps.iter().enumerate() {
                    let combo = format!("combo_{}:10", i);
                    let timestamp = format!("2025-01-15T00:{:02}:{:02}Z", ts / 60, ts % 60);
                    combinations.push(combo.clone());
                    last_used.insert(combo, timestamp);
                }

                prop_assert_eq!(combinations.len(), 10);

                // Find the LRU — should be the one with the smallest timestamp
                let lru = find_lru_combination(&combinations, &last_used);
                prop_assert!(lru.is_some(), "Should find an LRU combination");

                let lru_combo = lru.unwrap();
                let lru_timestamp = last_used.get(&lru_combo).unwrap().clone();

                // Verify it has the oldest (lexicographically smallest) timestamp
                for (combo, ts) in &last_used {
                    prop_assert!(
                        ts >= &lru_timestamp,
                        "LRU combo '{}' (ts={}) is not the oldest: '{}' has ts={}",
                        lru_combo, lru_timestamp, combo, ts
                    );
                }
            }
        }
    }
}
