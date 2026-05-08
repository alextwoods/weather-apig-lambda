use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::cache::{CacheStore, DynamoCacheStore, S3CacheStore};
use crate::models::ENSEMBLE_MODELS;
use crate::response::{
    AirQualityResponse, AstronomyResponse, CacheMetadata, CiopsSstResponse, EnsembleResponse,
    ForecastResponse, HrrrResponse, MarineResponse, ObservationsResponse, TidesResponse,
    UvResponse, WaterTemperatureResponse,
};

/// TTL for the core response cache (30 minutes).
pub const CORE_RESPONSE_TTL_SECS: u64 = 1800;

/// TTL for the volatile data cache (5 minutes).
pub const VOLATILE_DATA_TTL_SECS: u64 = 300;

/// TTL for the models metadata cache (30 minutes).
pub const MODELS_METADATA_TTL_SECS: u64 = 1800;

/// The fixed global cache key for models metadata.
pub const MODELS_METADATA_CACHE_KEY: &str = "models_metadata";

/// The "core" portion of a forecast response — everything except observations
/// and HRRR. Serialized with bincode for compact, fast storage.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct CoreResponseData {
    pub ensemble: Option<EnsembleResponse>,
    pub marine: Option<MarineResponse>,
    pub uv: Option<UvResponse>,
    pub air_quality: Option<AirQualityResponse>,
    pub tides: Option<TidesResponse>,
    pub water_temperature: Option<WaterTemperatureResponse>,
    pub ciops_sst: Option<CiopsSstResponse>,
    pub astronomy: Option<AstronomyResponse>,
    pub cache: HashMap<String, CacheMetadata>,
}

/// The "volatile" portion — observations and HRRR, which update frequently.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct VolatileData {
    pub observations: Option<ObservationsResponse>,
    pub hrrr: Option<HrrrResponse>,
    pub cache: HashMap<String, CacheMetadata>,
}

// ---------------------------------------------------------------------------
// Serialization
// ---------------------------------------------------------------------------

/// Serialize CoreResponseData to bytes (bincode format).
pub fn serialize_core_response(data: &CoreResponseData) -> Result<Vec<u8>, String> {
    bincode::serialize(data).map_err(|e| format!("bincode serialize error (core): {e}"))
}

/// Deserialize CoreResponseData from bytes (bincode format).
pub fn deserialize_core_response(bytes: &[u8]) -> Result<CoreResponseData, String> {
    bincode::deserialize(bytes).map_err(|e| format!("bincode deserialize error (core): {e}"))
}

/// Serialize VolatileData to bytes (bincode format).
pub fn serialize_volatile_data(data: &VolatileData) -> Result<Vec<u8>, String> {
    bincode::serialize(data).map_err(|e| format!("bincode serialize error (volatile): {e}"))
}

/// Deserialize VolatileData from bytes (bincode format).
pub fn deserialize_volatile_data(bytes: &[u8]) -> Result<VolatileData, String> {
    bincode::deserialize(bytes).map_err(|e| format!("bincode deserialize error (volatile): {e}"))
}

// ---------------------------------------------------------------------------
// Cache Key Generation
// ---------------------------------------------------------------------------

/// Generates the S3 object key for a core response cache entry.
///
/// Format: `core_response/{lat:.2}_{lon:.2}/{sorted_models}/{forecast_days}`
///
/// - `models`: sorted alphabetically, joined with commas. If all 5 models
///   are selected (or None is passed), uses the canonical value "all".
/// - `forecast_days`: the integer forecast horizon.
pub fn core_cache_key(lat: f64, lon: f64, models: Option<&[String]>, forecast_days: u32) -> String {
    let coord = format!("{:.2}_{:.2}", lat, lon);

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

    format!("core_response/{}/{}/{}", coord, models_segment, forecast_days)
}

/// Generates the DynamoDB cache key for a volatile data entry.
///
/// Format: `volatile/{lat:.2}_{lon:.2}`
pub fn volatile_cache_key(lat: f64, lon: f64) -> String {
    format!("volatile/{:.2}_{:.2}", lat, lon)
}

// ---------------------------------------------------------------------------
// Merge
// ---------------------------------------------------------------------------

/// Merges core response data with volatile data into a complete ForecastResponse.
///
/// The merged response includes:
/// - All sections from CoreResponseData
/// - observations and hrrr from VolatileData
/// - Combined cache metadata from both tiers
/// - Empty errors map (no errors on cache hit)
pub fn merge_cached_response(core: CoreResponseData, volatile: VolatileData) -> ForecastResponse {
    // Combine cache metadata from both tiers
    let mut cache = core.cache;
    cache.extend(volatile.cache);

    ForecastResponse {
        ensemble: core.ensemble,
        marine: core.marine,
        hrrr: volatile.hrrr,
        uv: core.uv,
        air_quality: core.air_quality,
        observations: volatile.observations,
        tides: core.tides,
        water_temperature: core.water_temperature,
        ciops_sst: core.ciops_sst,
        astronomy: core.astronomy,
        cache,
        errors: HashMap::new(),
    }
}

// ---------------------------------------------------------------------------
// Cache Freshness
// ---------------------------------------------------------------------------

/// Checks whether a cached entry is still fresh based on its stored-at timestamp
/// and TTL.
///
/// Returns `true` if the elapsed time since `stored_at` is strictly less than
/// `ttl_secs`. Returns `false` if parsing fails or elapsed >= ttl_secs.
pub fn is_cache_fresh(stored_at: &str, ttl_secs: u64) -> bool {
    let stored = match DateTime::parse_from_rfc3339(stored_at) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => return false,
    };

    let elapsed = Utc::now().signed_duration_since(stored);
    let elapsed_secs = elapsed.num_seconds();

    if elapsed_secs < 0 {
        // stored_at is in the future (clock skew) — treat as fresh
        return true;
    }

    (elapsed_secs as u64) < ttl_secs
}

// ---------------------------------------------------------------------------
// Core Response Cache Helpers (S3)
// ---------------------------------------------------------------------------

/// Checks the S3 core response cache for a fresh entry.
///
/// Reads from S3, checks freshness via stored-at metadata, and deserializes
/// bincode. Returns `None` on miss, stale entry, or any error.
pub async fn check_core_cache(
    s3_cache: &S3CacheStore,
    key: &str,
) -> Option<CoreResponseData> {
    let entry = s3_cache.get(key, "core_response").await?;

    if !entry.is_fresh() {
        tracing::info!(key = %key, age_secs = entry.age_secs(), "Core response cache stale");
        return None;
    }

    match deserialize_core_response(&entry.data) {
        Ok(data) => {
            tracing::info!(key = %key, age_secs = entry.age_secs(), "Core response cache hit");
            Some(data)
        }
        Err(e) => {
            tracing::warn!(key = %key, error = %e, "Core response cache deserialization failed");
            None
        }
    }
}

/// Stores a core response in the S3 cache.
///
/// Serializes to bincode and stores in S3 with stored-at and ttl-secs metadata.
/// Logs warnings on errors, never propagates.
pub async fn store_core_cache(
    s3_cache: &S3CacheStore,
    key: &str,
    data: &CoreResponseData,
) {
    let bytes = match serialize_core_response(data) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(key = %key, error = %e, "Failed to serialize core response for caching");
            return;
        }
    };

    if let Err(e) = s3_cache.put(key, "core_response", &bytes, CORE_RESPONSE_TTL_SECS).await {
        tracing::warn!(key = %key, error = %e, "Failed to store core response in S3 cache");
    }
}

// ---------------------------------------------------------------------------
// Volatile Data Cache Helpers (DynamoDB)
// ---------------------------------------------------------------------------

/// Checks the DynamoDB volatile data cache for a fresh entry.
///
/// Reads from DynamoDB, checks freshness, and deserializes bincode.
/// Returns `None` on miss, stale entry, or any error.
pub async fn check_volatile_cache(
    ddb_cache: &DynamoCacheStore,
    key: &str,
) -> Option<VolatileData> {
    let entry = ddb_cache.get(key, "volatile_data").await?;

    if !entry.is_fresh() {
        tracing::info!(key = %key, age_secs = entry.age_secs(), "Volatile data cache stale");
        return None;
    }

    match deserialize_volatile_data(&entry.data) {
        Ok(data) => {
            tracing::info!(key = %key, age_secs = entry.age_secs(), "Volatile data cache hit");
            Some(data)
        }
        Err(e) => {
            tracing::warn!(key = %key, error = %e, "Volatile data cache deserialization failed");
            None
        }
    }
}

/// Stores volatile data in the DynamoDB cache.
///
/// Serializes to bincode and stores in DynamoDB with stored_at, expires_at (TTL),
/// and source="volatile_data". Logs warnings on errors, never propagates.
pub async fn store_volatile_cache(
    ddb_cache: &DynamoCacheStore,
    key: &str,
    data: &VolatileData,
) {
    let bytes = match serialize_volatile_data(data) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(key = %key, error = %e, "Failed to serialize volatile data for caching");
            return;
        }
    };

    if let Err(e) = ddb_cache.put(key, "volatile_data", &bytes, VOLATILE_DATA_TTL_SECS).await {
        tracing::warn!(key = %key, error = %e, "Failed to store volatile data in DynamoDB cache");
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response::{
        DailySectionResponse, ObservationEntryResponse, PercentileStatsResponse,
        PrecipProbabilityResponse, StationResponse, TidePredictionResponse,
    };
    use proptest::prelude::*;
    use proptest::collection::{hash_map, vec};

    // Strategy for f64 values that excludes NaN (NaN != NaN breaks equality)
    fn finite_f64() -> impl Strategy<Value = f64> {
        -1000.0f64..1000.0f64
    }

    fn optional_finite_f64() -> impl Strategy<Value = Option<f64>> {
        prop_oneof![
            Just(None),
            finite_f64().prop_map(Some),
        ]
    }

    fn short_string() -> impl Strategy<Value = String> {
        "[a-zA-Z0-9_\\-]{0,20}"
    }

    fn short_vec_optional_f64() -> impl Strategy<Value = Vec<Option<f64>>> {
        vec(optional_finite_f64(), 0..5)
    }

    fn short_vec_f64() -> impl Strategy<Value = Vec<f64>> {
        vec(finite_f64(), 0..5)
    }

    fn short_vec_string() -> impl Strategy<Value = Vec<String>> {
        vec(short_string(), 0..5)
    }

    // --- Strategy for PercentileStatsResponse ---
    prop_compose! {
        fn arb_percentile_stats()(
            p10 in short_vec_optional_f64(),
            p25 in short_vec_optional_f64(),
            median in short_vec_optional_f64(),
            p75 in short_vec_optional_f64(),
            p90 in short_vec_optional_f64(),
        ) -> PercentileStatsResponse {
            PercentileStatsResponse { p10, p25, median, p75, p90 }
        }
    }

    // --- Strategy for PrecipProbabilityResponse ---
    prop_compose! {
        fn arb_precip_probability()(
            any in short_vec_optional_f64(),
            moderate in short_vec_optional_f64(),
            heavy in short_vec_optional_f64(),
        ) -> PrecipProbabilityResponse {
            PrecipProbabilityResponse { any, moderate, heavy }
        }
    }

    // --- Strategy for DailySectionResponse ---
    prop_compose! {
        fn arb_daily_section()(
            date in short_string(),
            start_index in 0usize..100,
            end_index in 0usize..100,
            high_temp in optional_finite_f64(),
            low_temp in optional_finite_f64(),
            total_precip in optional_finite_f64(),
            max_wind in optional_finite_f64(),
            dominant_wind_direction in prop_oneof![Just(None), short_string().prop_map(Some)],
        ) -> DailySectionResponse {
            DailySectionResponse {
                date, start_index, end_index, high_temp, low_temp,
                total_precip, max_wind, dominant_wind_direction,
            }
        }
    }

    // --- Strategy for EnsembleResponse ---
    prop_compose! {
        fn arb_ensemble()(
            times in short_vec_string(),
            statistics in hash_map(short_string(), arb_percentile_stats(), 0..3),
            precipitation_probability in arb_precip_probability(),
            daily_sections in vec(arb_daily_section(), 0..3),
        ) -> EnsembleResponse {
            EnsembleResponse { times, statistics, precipitation_probability, daily_sections }
        }
    }

    // --- Strategy for MarineResponse ---
    prop_compose! {
        fn arb_marine()(
            times in short_vec_string(),
            wave_height in short_vec_optional_f64(),
            wave_period in short_vec_optional_f64(),
            wave_direction in short_vec_optional_f64(),
            sea_surface_temperature in short_vec_optional_f64(),
        ) -> MarineResponse {
            MarineResponse { times, wave_height, wave_period, wave_direction, sea_surface_temperature }
        }
    }

    // --- Strategy for HrrrResponse ---
    prop_compose! {
        fn arb_hrrr()(
            times in short_vec_string(),
            temperature_2m in short_vec_optional_f64(),
            apparent_temperature in short_vec_optional_f64(),
            dew_point_2m in short_vec_optional_f64(),
            wind_speed_10m in short_vec_optional_f64(),
            wind_gusts_10m in short_vec_optional_f64(),
            wind_direction_10m in short_vec_optional_f64(),
            surface_pressure in short_vec_optional_f64(),
            precipitation in short_vec_optional_f64(),
            precipitation_probability in short_vec_optional_f64(),
        ) -> HrrrResponse {
            HrrrResponse {
                times, temperature_2m, apparent_temperature, dew_point_2m,
                wind_speed_10m, wind_gusts_10m, wind_direction_10m,
                surface_pressure, precipitation, precipitation_probability,
            }
        }
    }

    // --- Strategy for UvResponse ---
    prop_compose! {
        fn arb_uv()(
            times in short_vec_string(),
            uv_index in short_vec_optional_f64(),
            uv_index_clear_sky in short_vec_optional_f64(),
        ) -> UvResponse {
            UvResponse { times, uv_index, uv_index_clear_sky }
        }
    }

    // --- Strategy for AirQualityResponse ---
    prop_compose! {
        fn arb_air_quality()(
            times in short_vec_string(),
            us_aqi in short_vec_optional_f64(),
            pm2_5 in short_vec_optional_f64(),
            pm10 in short_vec_optional_f64(),
        ) -> AirQualityResponse {
            AirQualityResponse { times, us_aqi, pm2_5, pm10 }
        }
    }

    // --- Strategy for StationResponse ---
    prop_compose! {
        fn arb_station()(
            id in short_string(),
            name in short_string(),
            latitude in optional_finite_f64(),
            longitude in optional_finite_f64(),
            distance_km in optional_finite_f64(),
        ) -> StationResponse {
            StationResponse { id, name, latitude, longitude, distance_km }
        }
    }

    // --- Strategy for ObservationEntryResponse ---
    prop_compose! {
        fn arb_observation_entry()(
            timestamp in short_string(),
            temperature_celsius in optional_finite_f64(),
            wind_speed_kmh in optional_finite_f64(),
            wind_direction_degrees in optional_finite_f64(),
            pressure_hpa in optional_finite_f64(),
        ) -> ObservationEntryResponse {
            ObservationEntryResponse {
                timestamp, temperature_celsius, wind_speed_kmh,
                wind_direction_degrees, pressure_hpa,
            }
        }
    }

    // --- Strategy for ObservationsResponse ---
    prop_compose! {
        fn arb_observations()(
            station in arb_station(),
            entries in vec(arb_observation_entry(), 0..3),
        ) -> ObservationsResponse {
            ObservationsResponse { station, entries }
        }
    }

    // --- Strategy for TidePredictionResponse ---
    prop_compose! {
        fn arb_tide_prediction()(
            time in short_string(),
            height_m in finite_f64(),
        ) -> TidePredictionResponse {
            TidePredictionResponse { time, height_m }
        }
    }

    // --- Strategy for TidesResponse ---
    prop_compose! {
        fn arb_tides()(
            station in arb_station(),
            predictions in vec(arb_tide_prediction(), 0..3),
        ) -> TidesResponse {
            TidesResponse { station, predictions }
        }
    }

    // --- Strategy for WaterTemperatureResponse ---
    prop_compose! {
        fn arb_water_temperature()(
            station in arb_station(),
            temperature_celsius in optional_finite_f64(),
            timestamp in prop_oneof![Just(None), short_string().prop_map(Some)],
        ) -> WaterTemperatureResponse {
            WaterTemperatureResponse { station, temperature_celsius, timestamp }
        }
    }

    // --- Strategy for CiopsSstResponse ---
    prop_compose! {
        fn arb_ciops_sst()(
            times in short_vec_string(),
            temperatures_celsius in short_vec_optional_f64(),
        ) -> CiopsSstResponse {
            CiopsSstResponse { times, temperatures_celsius }
        }
    }

    // --- Strategy for AstronomyResponse ---
    prop_compose! {
        fn arb_astronomy()(
            times in short_vec_string(),
            sun_altitude in short_vec_f64(),
            moon_altitude in short_vec_f64(),
        ) -> AstronomyResponse {
            AstronomyResponse { times, sun_altitude, moon_altitude }
        }
    }

    // --- Strategy for CacheMetadata ---
    prop_compose! {
        fn arb_cache_metadata()(
            age_seconds in 0u64..100_000,
            is_fresh in any::<bool>(),
            fetched_at in short_string(),
        ) -> CacheMetadata {
            CacheMetadata { age_seconds, is_fresh, fetched_at }
        }
    }

    // --- Strategy for CoreResponseData ---
    prop_compose! {
        fn arb_core_response_data()(
            ensemble in prop_oneof![Just(None), arb_ensemble().prop_map(Some)],
            marine in prop_oneof![Just(None), arb_marine().prop_map(Some)],
            uv in prop_oneof![Just(None), arb_uv().prop_map(Some)],
            air_quality in prop_oneof![Just(None), arb_air_quality().prop_map(Some)],
            tides in prop_oneof![Just(None), arb_tides().prop_map(Some)],
            water_temperature in prop_oneof![Just(None), arb_water_temperature().prop_map(Some)],
            ciops_sst in prop_oneof![Just(None), arb_ciops_sst().prop_map(Some)],
            astronomy in prop_oneof![Just(None), arb_astronomy().prop_map(Some)],
            cache in hash_map(short_string(), arb_cache_metadata(), 0..3),
        ) -> CoreResponseData {
            CoreResponseData {
                ensemble, marine, uv, air_quality, tides,
                water_temperature, ciops_sst, astronomy, cache,
            }
        }
    }

    // --- Strategy for VolatileData ---
    prop_compose! {
        fn arb_volatile_data()(
            observations in prop_oneof![Just(None), arb_observations().prop_map(Some)],
            hrrr in prop_oneof![Just(None), arb_hrrr().prop_map(Some)],
            cache in hash_map(short_string(), arb_cache_metadata(), 0..3),
        ) -> VolatileData {
            VolatileData { observations, hrrr, cache }
        }
    }

    /// Feature: response-cache-warming, Property 1: Core response serialization round-trip
    ///
    /// **Validates: Requirements 1.1, 1.2**
    mod prop_core_response_round_trip {
        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            #[test]
            fn prop_core_response_serialization_round_trip(
                data in arb_core_response_data()
            ) {
                let bytes = serialize_core_response(&data)
                    .expect("serialization should succeed");
                let deserialized = deserialize_core_response(&bytes)
                    .expect("deserialization should succeed");
                prop_assert_eq!(deserialized, data);
            }
        }
    }

    /// Feature: response-cache-warming, Property 4: Volatile data serialization round-trip
    ///
    /// **Validates: Requirements 2.1, 2.2, 2.4**
    mod prop_volatile_data_round_trip {
        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            #[test]
            fn prop_volatile_data_serialization_round_trip(
                data in arb_volatile_data()
            ) {
                let bytes = serialize_volatile_data(&data)
                    .expect("serialization should succeed");
                let deserialized = deserialize_volatile_data(&bytes)
                    .expect("deserialization should succeed");
                prop_assert_eq!(deserialized, data);
            }
        }
    }

    /// Feature: response-cache-warming, Property 2: Core cache key determinism
    ///
    /// **Validates: Requirements 1.3, 1.5, 7.1, 7.2, 7.3**
    mod prop_core_cache_key_determinism {
        use super::*;
        use proptest::sample::subsequence;

        const ALL_MODEL_SUFFIXES: [&str; 5] = [
            "bom_access_global_ensemble",
            "ecmwf_ifs025_ensemble",
            "gem_global_ensemble",
            "icon_seamless_eps",
            "ncep_gefs_seamless",
        ];

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            #[test]
            fn prop_same_models_different_order_same_key(
                lat in -90.0f64..90.0f64,
                lon in -180.0f64..180.0f64,
                forecast_days in 1u32..=35,
                // Generate a subset of models (1-5 models)
                model_indices in subsequence((0..5).collect::<Vec<usize>>(), 1..=5),
            ) {
                let models: Vec<String> = model_indices.iter()
                    .map(|&i| ALL_MODEL_SUFFIXES[i].to_string())
                    .collect();

                // Create a reversed copy (different order)
                let mut reversed = models.clone();
                reversed.reverse();

                let key1 = core_cache_key(lat, lon, Some(&models), forecast_days);
                let key2 = core_cache_key(lat, lon, Some(&reversed), forecast_days);

                prop_assert_eq!(&key1, &key2,
                    "Same models in different order should produce same key: {:?} vs {:?}",
                    models, reversed);
            }

            #[test]
            fn prop_all_models_explicit_uses_all(
                lat in -90.0f64..90.0f64,
                lon in -180.0f64..180.0f64,
                forecast_days in 1u32..=35,
            ) {
                let all_models: Vec<String> = ALL_MODEL_SUFFIXES.iter()
                    .map(|s| s.to_string())
                    .collect();

                let key_explicit = core_cache_key(lat, lon, Some(&all_models), forecast_days);
                let key_none = core_cache_key(lat, lon, None, forecast_days);

                // Both should use "all" in the models segment
                prop_assert_eq!(&key_explicit, &key_none,
                    "Explicit all models should equal None (all): {} vs {}",
                    key_explicit, key_none);

                // Verify the key contains "all" as the models segment
                prop_assert!(key_explicit.contains("/all/"),
                    "Key should contain '/all/' segment: {}", key_explicit);
            }

            #[test]
            fn prop_deterministic_same_inputs_same_key(
                lat in -90.0f64..90.0f64,
                lon in -180.0f64..180.0f64,
                forecast_days in 1u32..=35,
                model_indices in subsequence((0..5).collect::<Vec<usize>>(), 1..=5),
            ) {
                let models: Vec<String> = model_indices.iter()
                    .map(|&i| ALL_MODEL_SUFFIXES[i].to_string())
                    .collect();

                let key1 = core_cache_key(lat, lon, Some(&models), forecast_days);
                let key2 = core_cache_key(lat, lon, Some(&models), forecast_days);

                prop_assert_eq!(&key1, &key2, "Same inputs should always produce same key");
            }
        }
    }

    /// Feature: response-cache-warming, Property 3: Core cache key uniqueness
    ///
    /// **Validates: Requirements 7.4**
    mod prop_core_cache_key_uniqueness {
        use super::*;
        use proptest::sample::subsequence;

        const ALL_MODEL_SUFFIXES: [&str; 5] = [
            "bom_access_global_ensemble",
            "ecmwf_ifs025_ensemble",
            "gem_global_ensemble",
            "icon_seamless_eps",
            "ncep_gefs_seamless",
        ];

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            #[test]
            fn prop_different_coords_different_key(
                lat1 in -90.0f64..90.0f64,
                lon1 in -180.0f64..180.0f64,
                lat2 in -90.0f64..90.0f64,
                lon2 in -180.0f64..180.0f64,
                forecast_days in 1u32..=35,
            ) {
                // Only test when rounded coordinates are actually different
                let rounded_lat1 = format!("{:.2}", lat1);
                let rounded_lat2 = format!("{:.2}", lat2);
                let rounded_lon1 = format!("{:.2}", lon1);
                let rounded_lon2 = format!("{:.2}", lon2);

                if rounded_lat1 != rounded_lat2 || rounded_lon1 != rounded_lon2 {
                    let key1 = core_cache_key(lat1, lon1, None, forecast_days);
                    let key2 = core_cache_key(lat2, lon2, None, forecast_days);
                    prop_assert_ne!(key1, key2,
                        "Different coords ({},{}) vs ({},{}) should produce different keys",
                        lat1, lon1, lat2, lon2);
                }
            }

            #[test]
            fn prop_different_models_different_key(
                lat in -90.0f64..90.0f64,
                lon in -180.0f64..180.0f64,
                forecast_days in 1u32..=35,
                indices1 in subsequence((0..5).collect::<Vec<usize>>(), 1..=4),
                indices2 in subsequence((0..5).collect::<Vec<usize>>(), 1..=4),
            ) {
                let models1: Vec<String> = indices1.iter()
                    .map(|&i| ALL_MODEL_SUFFIXES[i].to_string())
                    .collect();
                let models2: Vec<String> = indices2.iter()
                    .map(|&i| ALL_MODEL_SUFFIXES[i].to_string())
                    .collect();

                // Only test when model sets are actually different
                let mut sorted1: Vec<&str> = models1.iter().map(|s| s.as_str()).collect();
                sorted1.sort();
                let mut sorted2: Vec<&str> = models2.iter().map(|s| s.as_str()).collect();
                sorted2.sort();

                if sorted1 != sorted2 {
                    let key1 = core_cache_key(lat, lon, Some(&models1), forecast_days);
                    let key2 = core_cache_key(lat, lon, Some(&models2), forecast_days);
                    prop_assert_ne!(key1, key2,
                        "Different model sets {:?} vs {:?} should produce different keys",
                        sorted1, sorted2);
                }
            }

            #[test]
            fn prop_different_forecast_days_different_key(
                lat in -90.0f64..90.0f64,
                lon in -180.0f64..180.0f64,
                days1 in 1u32..=35,
                days2 in 1u32..=35,
            ) {
                if days1 != days2 {
                    let key1 = core_cache_key(lat, lon, None, days1);
                    let key2 = core_cache_key(lat, lon, None, days2);
                    prop_assert_ne!(key1, key2,
                        "Different forecast_days {} vs {} should produce different keys",
                        days1, days2);
                }
            }
        }
    }

    /// Feature: response-cache-warming, Property 5: Volatile cache key format
    ///
    /// **Validates: Requirements 2.3**
    mod prop_volatile_cache_key_format {
        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            #[test]
            fn prop_volatile_key_matches_format(
                lat in -90.0f64..90.0f64,
                lon in -180.0f64..180.0f64,
            ) {
                let key = volatile_cache_key(lat, lon);

                // Verify format: volatile/{lat:.2}_{lon:.2}
                let expected = format!("volatile/{:.2}_{:.2}", lat, lon);
                prop_assert_eq!(&key, &expected,
                    "volatile_cache_key({}, {}) = '{}', expected '{}'",
                    lat, lon, key, expected);

                // Verify it starts with "volatile/"
                prop_assert!(key.starts_with("volatile/"),
                    "Key should start with 'volatile/': {}", key);
            }

            #[test]
            fn prop_volatile_key_same_rounding_same_key(
                base_lat in -89.0f64..89.0f64,
                base_lon in -179.0f64..179.0f64,
                offset_lat in -0.004f64..0.004f64,
                offset_lon in -0.004f64..0.004f64,
            ) {
                let lat1 = base_lat;
                let lon1 = base_lon;
                let lat2 = base_lat + offset_lat;
                let lon2 = base_lon + offset_lon;

                // Check if they round to the same 2-decimal value
                let rounded_lat1 = format!("{:.2}", lat1);
                let rounded_lat2 = format!("{:.2}", lat2);
                let rounded_lon1 = format!("{:.2}", lon1);
                let rounded_lon2 = format!("{:.2}", lon2);

                if rounded_lat1 == rounded_lat2 && rounded_lon1 == rounded_lon2 {
                    let key1 = volatile_cache_key(lat1, lon1);
                    let key2 = volatile_cache_key(lat2, lon2);
                    prop_assert_eq!(&key1, &key2,
                        "({}, {}) and ({}, {}) round the same but got different keys: '{}' vs '{}'",
                        lat1, lon1, lat2, lon2, key1, key2);
                }
            }
        }
    }

    /// Feature: response-cache-warming, Property 6: Cache merge preserves all data
    ///
    /// **Validates: Requirements 3.3**
    mod prop_cache_merge_preserves_data {
        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            #[test]
            fn prop_merge_preserves_all_sections(
                core in arb_core_response_data(),
                volatile in arb_volatile_data(),
            ) {
                // Clone inputs for comparison after merge (merge consumes them)
                let core_ensemble = core.ensemble.clone();
                let core_marine = core.marine.clone();
                let core_uv = core.uv.clone();
                let core_air_quality = core.air_quality.clone();
                let core_tides = core.tides.clone();
                let core_water_temperature = core.water_temperature.clone();
                let core_ciops_sst = core.ciops_sst.clone();
                let core_astronomy = core.astronomy.clone();
                let core_cache = core.cache.clone();

                let vol_observations = volatile.observations.clone();
                let vol_hrrr = volatile.hrrr.clone();
                let vol_cache = volatile.cache.clone();

                let merged = merge_cached_response(core, volatile);

                // Verify all core sections are preserved
                prop_assert_eq!(&merged.ensemble, &core_ensemble);
                prop_assert_eq!(&merged.marine, &core_marine);
                prop_assert_eq!(&merged.uv, &core_uv);
                prop_assert_eq!(&merged.air_quality, &core_air_quality);
                prop_assert_eq!(&merged.tides, &core_tides);
                prop_assert_eq!(&merged.water_temperature, &core_water_temperature);
                prop_assert_eq!(&merged.ciops_sst, &core_ciops_sst);
                prop_assert_eq!(&merged.astronomy, &core_astronomy);

                // Verify volatile sections are preserved
                prop_assert_eq!(&merged.observations, &vol_observations);
                prop_assert_eq!(&merged.hrrr, &vol_hrrr);

                // Verify cache metadata is the union of both maps
                for (key, value) in &core_cache {
                    if !vol_cache.contains_key(key) {
                        prop_assert_eq!(merged.cache.get(key), Some(value),
                            "Core cache key '{}' missing from merged cache", key);
                    }
                }
                for (key, value) in &vol_cache {
                    prop_assert_eq!(merged.cache.get(key), Some(value),
                        "Volatile cache key '{}' missing or wrong in merged cache", key);
                }

                // Verify errors map is empty
                prop_assert!(merged.errors.is_empty(),
                    "Errors map should be empty on cache hit, got {:?}", merged.errors);
            }
        }
    }

    /// Feature: response-cache-warming, Property 9: Cache freshness correctness
    ///
    /// **Validates: Requirements 6.3, 6.4**
    mod prop_cache_freshness {
        use super::*;
        use chrono::Duration;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(100))]

            #[test]
            fn prop_freshness_matches_elapsed_less_than_ttl(
                // Generate elapsed seconds (0 to 7200 = 2 hours)
                elapsed_secs in 0u64..7200,
                // Generate TTL (1 to 3600 = 1 hour)
                ttl_secs in 1u64..3600,
            ) {
                // Create a stored_at timestamp that is `elapsed_secs` ago
                let stored_at = (Utc::now() - Duration::seconds(elapsed_secs as i64)).to_rfc3339();

                let result = is_cache_fresh(&stored_at, ttl_secs);

                // Allow 1 second of clock drift during test execution
                if elapsed_secs + 1 < ttl_secs {
                    prop_assert!(result,
                        "Expected fresh: elapsed={}, ttl={}, stored_at={}",
                        elapsed_secs, ttl_secs, stored_at);
                } else if elapsed_secs > ttl_secs + 1 {
                    prop_assert!(!result,
                        "Expected stale: elapsed={}, ttl={}, stored_at={}",
                        elapsed_secs, ttl_secs, stored_at);
                }
                // At the boundary (within ±1 second), either result is acceptable
                // due to clock drift between generating stored_at and calling is_cache_fresh
            }

            #[test]
            fn prop_invalid_timestamp_returns_false(
                garbage in "[a-z]{5,20}",
                ttl_secs in 1u64..3600,
            ) {
                let result = is_cache_fresh(&garbage, ttl_secs);
                prop_assert!(!result,
                    "Invalid timestamp '{}' should return false", garbage);
            }

            #[test]
            fn prop_future_timestamp_returns_true(
                // Future offset (1 to 3600 seconds in the future)
                future_secs in 1i64..3600,
                ttl_secs in 1u64..3600,
            ) {
                let stored_at = (Utc::now() + Duration::seconds(future_secs)).to_rfc3339();
                let result = is_cache_fresh(&stored_at, ttl_secs);
                prop_assert!(result,
                    "Future timestamp should be treated as fresh: stored_at={}", stored_at);
            }
        }
    }
}
