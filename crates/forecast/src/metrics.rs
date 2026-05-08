use std::time::{SystemTime, UNIX_EPOCH};

/// CloudWatch Embedded Metric Format (EMF) namespace for all weather API cache metrics.
pub const METRICS_NAMESPACE: &str = "WeatherApi/Cache";

/// Cache type dimension values.
pub enum CacheType {
    Forecast,
    Metadata,
}

impl CacheType {
    pub fn as_str(&self) -> &'static str {
        match self {
            CacheType::Forecast => "forecast",
            CacheType::Metadata => "metadata",
        }
    }
}

/// Cache outcome dimension values for the forecast cache.
pub enum ForecastCacheOutcome {
    /// Both core (S3) and volatile (DynamoDB) caches were fresh.
    FullHit,
    /// Core cache was fresh but volatile cache was stale/missing.
    PartialHit,
    /// Core cache was stale/missing (full pipeline required).
    Miss,
    /// Cache lookup was bypassed (force_refresh or refresh_source).
    Bypass,
}

impl ForecastCacheOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            ForecastCacheOutcome::FullHit => "full_hit",
            ForecastCacheOutcome::PartialHit => "partial_hit",
            ForecastCacheOutcome::Miss => "miss",
            ForecastCacheOutcome::Bypass => "bypass",
        }
    }
}

/// Cache outcome dimension values for the metadata cache.
pub enum MetadataCacheOutcome {
    /// DynamoDB metadata cache was fresh.
    Hit,
    /// DynamoDB metadata cache was stale/missing.
    Miss,
}

impl MetadataCacheOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            MetadataCacheOutcome::Hit => "hit",
            MetadataCacheOutcome::Miss => "miss",
        }
    }
}

/// Builds the EMF JSON string for a cache metric without printing it.
///
/// This is extracted as a public function for testability (property tests can
/// verify the JSON structure without capturing stdout).
pub fn build_emf_json(cache_type: &str, outcome: &str) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let emf_log = serde_json::json!({
        "_aws": {
            "Timestamp": timestamp,
            "CloudWatchMetrics": [{
                "Namespace": METRICS_NAMESPACE,
                "Dimensions": [["CacheType", "Outcome"]],
                "Metrics": [{
                    "Name": "CacheOutcome",
                    "Unit": "Count"
                }]
            }]
        },
        "CacheType": cache_type,
        "Outcome": outcome,
        "CacheOutcome": 1
    });

    match serde_json::to_string(&emf_log) {
        Ok(json_str) => json_str,
        Err(e) => {
            tracing::warn!("Failed to serialize EMF metric JSON: {}", e);
            String::new()
        }
    }
}

/// Emits a single CacheOutcome metric using CloudWatch Embedded Metric Format.
///
/// Writes a JSON line to stdout that CloudWatch Logs automatically parses into
/// a metric data point. Zero latency, no API calls. If serialization fails,
/// logs a warning and returns without panicking.
pub fn emit_cache_metric(cache_type: &str, outcome: &str) {
    let json_str = build_emf_json(cache_type, outcome);
    if !json_str.is_empty() {
        println!("{}", json_str);
    }
}

/// Convenience: emit a forecast cache outcome metric.
pub fn emit_forecast_cache_metric(outcome: ForecastCacheOutcome) {
    emit_cache_metric("forecast", outcome.as_str());
}

/// Convenience: emit a metadata cache outcome metric.
pub fn emit_metadata_cache_metric(outcome: MetadataCacheOutcome) {
    emit_cache_metric("metadata", outcome.as_str());
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn test_cache_type_as_str() {
        assert_eq!(CacheType::Forecast.as_str(), "forecast");
        assert_eq!(CacheType::Metadata.as_str(), "metadata");
    }

    #[test]
    fn test_forecast_cache_outcome_as_str() {
        assert_eq!(ForecastCacheOutcome::FullHit.as_str(), "full_hit");
        assert_eq!(ForecastCacheOutcome::PartialHit.as_str(), "partial_hit");
        assert_eq!(ForecastCacheOutcome::Miss.as_str(), "miss");
        assert_eq!(ForecastCacheOutcome::Bypass.as_str(), "bypass");
    }

    #[test]
    fn test_metadata_cache_outcome_as_str() {
        assert_eq!(MetadataCacheOutcome::Hit.as_str(), "hit");
        assert_eq!(MetadataCacheOutcome::Miss.as_str(), "miss");
    }

    #[test]
    fn test_build_emf_json_produces_valid_json() {
        let json_str = build_emf_json("forecast", "full_hit");
        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .expect("build_emf_json should produce valid JSON");

        // Verify structure
        assert_eq!(parsed["CacheType"], "forecast");
        assert_eq!(parsed["Outcome"], "full_hit");
        assert_eq!(parsed["CacheOutcome"], 1);
        assert_eq!(
            parsed["_aws"]["CloudWatchMetrics"][0]["Namespace"],
            "WeatherApi/Cache"
        );
        assert_eq!(
            parsed["_aws"]["CloudWatchMetrics"][0]["Dimensions"],
            serde_json::json!([["CacheType", "Outcome"]])
        );
        assert_eq!(
            parsed["_aws"]["CloudWatchMetrics"][0]["Metrics"][0]["Name"],
            "CacheOutcome"
        );
        assert_eq!(
            parsed["_aws"]["CloudWatchMetrics"][0]["Metrics"][0]["Unit"],
            "Count"
        );
        // Timestamp should be a positive number
        assert!(parsed["_aws"]["Timestamp"].as_u64().unwrap() > 0);
    }

    #[test]
    fn test_build_emf_json_metadata_hit() {
        let json_str = build_emf_json("metadata", "hit");
        let parsed: serde_json::Value = serde_json::from_str(&json_str)
            .expect("build_emf_json should produce valid JSON");

        assert_eq!(parsed["CacheType"], "metadata");
        assert_eq!(parsed["Outcome"], "hit");
        assert_eq!(parsed["CacheOutcome"], 1);
    }

    // **Validates: Requirements 1.8, 1.9**
    //
    // Property 5: Outcome string mapping consistency
    // For any ForecastCacheOutcome or MetadataCacheOutcome variant, as_str() returns the
    // expected string and is deterministic (calling it twice yields the same result).
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_forecast_outcome_string_mapping_consistency(
            variant_index in 0u8..4u8,
        ) {
            let outcome = match variant_index {
                0 => ForecastCacheOutcome::FullHit,
                1 => ForecastCacheOutcome::PartialHit,
                2 => ForecastCacheOutcome::Miss,
                3 => ForecastCacheOutcome::Bypass,
                _ => unreachable!(),
            };

            let expected = match variant_index {
                0 => "full_hit",
                1 => "partial_hit",
                2 => "miss",
                3 => "bypass",
                _ => unreachable!(),
            };

            // Verify correct string mapping
            prop_assert_eq!(outcome.as_str(), expected);

            // Verify determinism: construct the same variant again and check as_str() matches
            let outcome_again = match variant_index {
                0 => ForecastCacheOutcome::FullHit,
                1 => ForecastCacheOutcome::PartialHit,
                2 => ForecastCacheOutcome::Miss,
                3 => ForecastCacheOutcome::Bypass,
                _ => unreachable!(),
            };
            prop_assert_eq!(outcome.as_str(), outcome_again.as_str());
        }

        #[test]
        fn prop_metadata_outcome_string_mapping_consistency(
            variant_index in 0u8..2u8,
        ) {
            let outcome = match variant_index {
                0 => MetadataCacheOutcome::Hit,
                1 => MetadataCacheOutcome::Miss,
                _ => unreachable!(),
            };

            let expected = match variant_index {
                0 => "hit",
                1 => "miss",
                _ => unreachable!(),
            };

            // Verify correct string mapping
            prop_assert_eq!(outcome.as_str(), expected);

            // Verify determinism: construct the same variant again and check as_str() matches
            let outcome_again = match variant_index {
                0 => MetadataCacheOutcome::Hit,
                1 => MetadataCacheOutcome::Miss,
                _ => unreachable!(),
            };
            prop_assert_eq!(outcome.as_str(), outcome_again.as_str());
        }
    }

    // **Validates: Requirements 1.1, 1.2, 1.3, 1.4, 1.5, 1.6**
    //
    // Property 1: EMF JSON structure validity
    // For any valid (cache_type, outcome) pair, build_emf_json produces a JSON string that
    // conforms to the CloudWatch EMF specification with the correct namespace, dimensions,
    // metrics, and dimension values.
    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        #[test]
        fn prop_emf_json_structure_validity(
            cache_type in proptest::sample::select(vec!["forecast", "metadata"]),
            outcome in proptest::sample::select(vec!["full_hit", "partial_hit", "miss", "bypass", "hit"]),
        ) {
            let json_str = build_emf_json(cache_type, outcome);

            // (a) Output is valid JSON
            let parsed: serde_json::Value = serde_json::from_str(&json_str)
                .expect("build_emf_json must produce valid JSON");

            // (b) Contains _aws.CloudWatchMetrics as non-empty array
            let cw_metrics = parsed["_aws"]["CloudWatchMetrics"].as_array()
                .expect("_aws.CloudWatchMetrics must be an array");
            prop_assert!(!cw_metrics.is_empty(), "CloudWatchMetrics must be non-empty");

            // (c) _aws.Timestamp is a positive integer
            let timestamp = parsed["_aws"]["Timestamp"].as_u64()
                .expect("_aws.Timestamp must be a positive integer");
            prop_assert!(timestamp > 0, "Timestamp must be positive, got {}", timestamp);

            // (d) CacheType matches input
            prop_assert_eq!(
                parsed["CacheType"].as_str().unwrap(),
                cache_type,
                "CacheType must match input"
            );

            // (e) Outcome matches input
            prop_assert_eq!(
                parsed["Outcome"].as_str().unwrap(),
                outcome,
                "Outcome must match input"
            );

            // (f) CacheOutcome equals 1
            prop_assert_eq!(
                parsed["CacheOutcome"].as_u64().unwrap(),
                1,
                "CacheOutcome must equal 1"
            );

            // (g) Namespace is "WeatherApi/Cache"
            prop_assert_eq!(
                cw_metrics[0]["Namespace"].as_str().unwrap(),
                "WeatherApi/Cache",
                "Namespace must be WeatherApi/Cache"
            );

            // (h) Dimensions is [["CacheType", "Outcome"]]
            let dimensions = &cw_metrics[0]["Dimensions"];
            prop_assert_eq!(
                dimensions,
                &serde_json::json!([["CacheType", "Outcome"]]),
                "Dimensions must be [[\"CacheType\", \"Outcome\"]]"
            );

            // (i) Metrics[0].Name is "CacheOutcome" and Unit is "Count"
            let metrics = cw_metrics[0]["Metrics"].as_array()
                .expect("Metrics must be an array");
            prop_assert!(!metrics.is_empty(), "Metrics must be non-empty");
            prop_assert_eq!(
                metrics[0]["Name"].as_str().unwrap(),
                "CacheOutcome",
                "Metrics[0].Name must be CacheOutcome"
            );
            prop_assert_eq!(
                metrics[0]["Unit"].as_str().unwrap(),
                "Count",
                "Metrics[0].Unit must be Count"
            );
        }
    }
}
