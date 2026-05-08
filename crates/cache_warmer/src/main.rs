use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use lambda_runtime::{service_fn, Error, LambdaEvent};
use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use forecast::cache::{CacheEntry, CacheStore, DynamoCacheStore, S3CacheStore};
use forecast::fetcher::{AllSourceResults, CacheMeta, SourceResult};
use forecast::location_tracker::{get_param_combinations, scan_active_locations};
use forecast::models::{
    nearby_puget_sound_stations, nearest_puget_sound_station, FetchParams, ENSEMBLE_MODELS,
    PUGET_SOUND_BOX,
};
use forecast::response::build_response;
use forecast::response_cache::{
    core_cache_key, serialize_core_response, CoreResponseData, CORE_RESPONSE_TTL_SECS,
    MODELS_METADATA_CACHE_KEY, MODELS_METADATA_TTL_SECS,
};
use forecast::sources::air_quality::{build_air_quality_url, parse_air_quality_response, AirQualityFetcher};
use forecast::sources::ensemble::{build_ensemble_url, parse_ensemble_response, EnsembleFetcher};
use forecast::sources::ensemble_splitter::{
    deserialize_per_model, merge_ensemble_models, serialize_per_model, split_ensemble_by_model,
};
use forecast::sources::hrrr::{build_hrrr_url, parse_hrrr_response, HrrrFetcher};
use forecast::sources::marine::{build_marine_url, parse_marine_response, MarineFetcher};
use forecast::sources::noaa_tides::{build_tides_url, parse_tides_response, serialize_tides, NoaaTidesFetcher};
use forecast::sources::noaa_water_temp::{
    build_water_temp_url, parse_water_temp_response, serialize_water_temperature, NoaaWaterTempFetcher,
};
use forecast::sources::observations::{
    build_observation_url, build_station_discovery_url,
    parse_observations, parse_station_discovery, serialize_observations, filter_observations_to_recent,
    ObservationData, ObservationsFetcher, NWS_USER_AGENT, NWS_ACCEPT,
};
use forecast::sources::uv::{build_uv_url, parse_uv_response, UvFetcher};

// ---------------------------------------------------------------------------
// EMF metrics for cache warmer observability
// ---------------------------------------------------------------------------

/// Emits a CloudWatch EMF metric for the cache warmer run summary.
fn emit_warmer_run_metric(
    locations_found: usize,
    locations_processed: usize,
    sources_checked: usize,
    sources_refreshed: usize,
    sources_skipped: usize,
    errors: usize,
    elapsed_ms: i64,
) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let emf = serde_json::json!({
        "_aws": {
            "Timestamp": timestamp,
            "CloudWatchMetrics": [{
                "Namespace": "WeatherApi/CacheWarmer",
                "Dimensions": [["Metric"]],
                "Metrics": [
                    { "Name": "LocationsFound", "Unit": "Count" },
                    { "Name": "LocationsProcessed", "Unit": "Count" },
                    { "Name": "SourcesChecked", "Unit": "Count" },
                    { "Name": "SourcesRefreshed", "Unit": "Count" },
                    { "Name": "SourcesSkipped", "Unit": "Count" },
                    { "Name": "Errors", "Unit": "Count" },
                    { "Name": "ElapsedMs", "Unit": "Milliseconds" }
                ]
            }]
        },
        "Metric": "RunSummary",
        "LocationsFound": locations_found,
        "LocationsProcessed": locations_processed,
        "SourcesChecked": sources_checked,
        "SourcesRefreshed": sources_refreshed,
        "SourcesSkipped": sources_skipped,
        "Errors": errors,
        "ElapsedMs": elapsed_ms
    });

    if let Ok(json_str) = serde_json::to_string(&emf) {
        println!("{}", json_str);
    }
}

/// Emits a CloudWatch EMF metric for a single upstream fetch latency.
fn emit_warmer_fetch_latency(source: &str, elapsed_ms: u64, success: bool) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    let outcome = if success { "success" } else { "error" };

    let emf = serde_json::json!({
        "_aws": {
            "Timestamp": timestamp,
            "CloudWatchMetrics": [{
                "Namespace": "WeatherApi/CacheWarmer",
                "Dimensions": [["Source", "Outcome"]],
                "Metrics": [
                    { "Name": "FetchLatency", "Unit": "Milliseconds" },
                    { "Name": "FetchCount", "Unit": "Count" }
                ]
            }]
        },
        "Source": source,
        "Outcome": outcome,
        "FetchLatency": elapsed_ms,
        "FetchCount": 1
    });

    if let Ok(json_str) = serde_json::to_string(&emf) {
        println!("{}", json_str);
    }
}
// ---------------------------------------------------------------------------
// Source descriptor — enumerates all cacheable sources the warmer checks
// ---------------------------------------------------------------------------

/// Describes a single cacheable source for a given location.
struct SourceDescriptor {
    /// Human-readable name for logging.
    name: &'static str,
    /// The source ID used as the cache sort key.
    source_id: String,
    /// TTL in seconds for this source.
    ttl_secs: u64,
    /// Which cache backend to use.
    backend: CacheBackend,
}

#[derive(Clone, Copy)]
enum CacheBackend {
    S3,
    DynamoDB,
}

/// Builds the list of all cacheable sources for a location.
///
/// Returns 9 sources total:
/// - 5 ensemble per-model sources (S3)
/// - 1 marine source (S3)
/// - 1 HRRR source (DynamoDB)
/// - 1 UV source (DynamoDB)
/// - 1 air quality source (DynamoDB)
fn cacheable_sources() -> Vec<SourceDescriptor> {
    let mut sources = Vec::with_capacity(9);

    // 5 ensemble per-model sources
    for model in &ENSEMBLE_MODELS {
        sources.push(SourceDescriptor {
            name: "ensemble",
            source_id: format!("ensemble_{}", model.api_key_suffix),
            ttl_secs: EnsembleFetcher::ttl_secs(),
            backend: CacheBackend::S3,
        });
    }

    // Marine (S3)
    sources.push(SourceDescriptor {
        name: "marine",
        source_id: MarineFetcher::source_id().to_string(),
        ttl_secs: MarineFetcher::ttl_secs(),
        backend: CacheBackend::S3,
    });

    // HRRR (DynamoDB)
    sources.push(SourceDescriptor {
        name: "hrrr",
        source_id: HrrrFetcher::source_id().to_string(),
        ttl_secs: HrrrFetcher::ttl_secs(),
        backend: CacheBackend::DynamoDB,
    });

    // UV (DynamoDB)
    sources.push(SourceDescriptor {
        name: "uv",
        source_id: UvFetcher::source_id().to_string(),
        ttl_secs: UvFetcher::ttl_secs(),
        backend: CacheBackend::DynamoDB,
    });

    // Air quality (DynamoDB)
    sources.push(SourceDescriptor {
        name: "air_quality",
        source_id: AirQualityFetcher::source_id().to_string(),
        ttl_secs: AirQualityFetcher::ttl_secs(),
        backend: CacheBackend::DynamoDB,
    });

    sources
}

// ---------------------------------------------------------------------------
// Near-expiry check
// ---------------------------------------------------------------------------

/// The number of seconds before TTL expiry at which a source is considered
/// "near-expiry" and should be proactively refreshed. With a 30-minute
/// warmer interval, this must be at least 1800s to ensure entries don't
/// expire between runs.
const NEAR_EXPIRY_BUFFER_SECS: u64 = 1800; // 30 minutes

/// Returns `true` if the cache entry needs refreshing — either it's stale
/// (past TTL) or within `NEAR_EXPIRY_BUFFER_SECS` of expiring.
fn needs_refresh(entry: &Option<CacheEntry>, ttl_secs: u64) -> bool {
    match entry {
        None => true, // cache miss
        Some(e) => {
            let age = e.age_secs();
            // Stale: age >= ttl_secs
            // Near-expiry: age >= ttl_secs - buffer (i.e., within 10 min of expiring)
            let near_expiry_threshold = ttl_secs.saturating_sub(NEAR_EXPIRY_BUFFER_SECS);
            age >= near_expiry_threshold
        }
    }
}

// ---------------------------------------------------------------------------
// Upstream fetch + cache update helpers
// ---------------------------------------------------------------------------

/// Makes an HTTP GET request and returns the raw response bytes.
async fn http_get(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Failed to read response body: {e}"))
}

/// Makes an HTTP GET request to the NWS API with required headers.
async fn http_get_nws(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .header("User-Agent", NWS_USER_AGENT)
        .header("Accept", NWS_ACCEPT)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {e}"))?;

    if !response.status().is_success() {
        let status = response.status().as_u16();
        let body = response.text().await.unwrap_or_default();
        return Err(format!("HTTP {status}: {body}"));
    }

    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| format!("Failed to read response body: {e}"))
}

/// Fetches ensemble data from upstream, splits by model, and caches all 5
/// per-model entries in S3.
///
/// Returns the number of models successfully cached.
async fn refresh_ensemble(
    client: &reqwest::Client,
    cache: &dyn CacheStore,
    lat: f64,
    lon: f64,
    cache_key: &str,
    timeout: Duration,
) -> Result<usize, String> {
    let url = build_ensemble_url(lat, lon);
    let raw = http_get(client, &url, timeout).await?;
    let combined = parse_ensemble_response(&raw).map_err(|e| format!("parse error: {e}"))?;
    let split = split_ensemble_by_model(&combined);

    let mut cached_count = 0;
    for model in &ENSEMBLE_MODELS {
        let source_id = format!("ensemble_{}", model.api_key_suffix);
        if let Some(model_data) = split.get(model.api_key_suffix) {
            match serialize_per_model(&combined.times, model_data) {
                Ok(bytes) => {
                    if let Err(e) = cache.put(cache_key, &source_id, &bytes, EnsembleFetcher::ttl_secs()).await {
                        tracing::warn!(
                            source = %source_id,
                            error = %e,
                            "Failed to cache ensemble per-model data"
                        );
                    } else {
                        cached_count += 1;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        source = %source_id,
                        error = %e,
                        "Failed to serialize ensemble per-model data"
                    );
                }
            }
        }
    }

    Ok(cached_count)
}

/// Fetches a single non-ensemble source from upstream, parses it to validate,
/// and caches the raw bytes.
async fn refresh_simple_source(
    client: &reqwest::Client,
    cache: &dyn CacheStore,
    url: &str,
    cache_key: &str,
    source_id: &str,
    ttl_secs: u64,
    parse_fn: fn(&[u8]) -> Result<(), String>,
    timeout: Duration,
) -> Result<(), String> {
    let start = std::time::Instant::now();
    let raw = http_get(client, url, timeout).await.map_err(|e| {
        emit_warmer_fetch_latency(source_id, start.elapsed().as_millis() as u64, false);
        e
    })?;
    parse_fn(&raw).map_err(|e| {
        emit_warmer_fetch_latency(source_id, start.elapsed().as_millis() as u64, false);
        e
    })?;
    let result = cache
        .put(cache_key, source_id, &raw, ttl_secs)
        .await
        .map_err(|e| format!("cache put failed: {e}"));
    emit_warmer_fetch_latency(source_id, start.elapsed().as_millis() as u64, result.is_ok());
    result
}

// ---------------------------------------------------------------------------
// Per-location warming logic
// ---------------------------------------------------------------------------

/// Result of warming a single location.
struct LocationWarmResult {
    cache_key: String,
    sources_checked: usize,
    sources_refreshed: usize,
    errors: Vec<String>,
}

/// Warms all cacheable sources for a single location.
async fn warm_location(
    client: &reqwest::Client,
    s3_cache: &dyn CacheStore,
    ddb_cache: &dyn CacheStore,
    location_cache_key: &str,
    timeout: Duration,
) -> LocationWarmResult {
    let mut result = LocationWarmResult {
        cache_key: location_cache_key.to_string(),
        sources_checked: 0,
        sources_refreshed: 0,
        errors: Vec::new(),
    };

    // Parse lat/lon from cache key (format: "47.61_-122.33")
    let (lat, lon) = match parse_cache_key(location_cache_key) {
        Some(coords) => coords,
        None => {
            result.errors.push(format!(
                "Invalid cache key format: {}",
                location_cache_key
            ));
            return result;
        }
    };

    let sources = cacheable_sources();

    // Check which ensemble per-model sources need refresh
    let mut ensemble_needs_refresh = false;
    for source in &sources {
        if source.name == "ensemble" {
            let cache: &dyn CacheStore = match source.backend {
                CacheBackend::S3 => s3_cache,
                CacheBackend::DynamoDB => ddb_cache,
            };
            let entry = cache.get(location_cache_key, &source.source_id).await;
            result.sources_checked += 1;

            if needs_refresh(&entry, source.ttl_secs) {
                ensemble_needs_refresh = true;
                break; // If any model needs refresh, we fetch the full ensemble
            }
        }
    }

    // Refresh ensemble if any per-model source needs it
    if ensemble_needs_refresh {
        match refresh_ensemble(client, s3_cache, lat, lon, location_cache_key, timeout).await {
            Ok(count) => {
                tracing::info!(
                    cache_key = %location_cache_key,
                    models_cached = count,
                    "Ensemble refreshed"
                );
                result.sources_refreshed += count;
            }
            Err(e) => {
                tracing::warn!(
                    cache_key = %location_cache_key,
                    error = %e,
                    "Ensemble refresh failed"
                );
                result.errors.push(format!("ensemble: {e}"));
            }
        }
    }

    // Check and refresh non-ensemble sources individually
    for source in &sources {
        if source.name == "ensemble" {
            continue; // Already handled above
        }

        let cache: &dyn CacheStore = match source.backend {
            CacheBackend::S3 => s3_cache,
            CacheBackend::DynamoDB => ddb_cache,
        };

        let entry = cache.get(location_cache_key, &source.source_id).await;
        result.sources_checked += 1;

        if !needs_refresh(&entry, source.ttl_secs) {
            continue;
        }

        // Build the URL and parse function for this source
        let (url, parse_fn): (String, fn(&[u8]) -> Result<(), String>) = match source.name {
            "marine" => (
                build_marine_url(lat, lon),
                |raw| parse_marine_response(raw).map(|_| ()),
            ),
            "hrrr" => (
                build_hrrr_url(lat, lon),
                |raw| parse_hrrr_response(raw).map(|_| ()),
            ),
            "uv" => (
                build_uv_url(lat, lon),
                |raw| parse_uv_response(raw).map(|_| ()),
            ),
            "air_quality" => (
                build_air_quality_url(lat, lon),
                |raw| parse_air_quality_response(raw).map(|_| ()),
            ),
            other => {
                result.errors.push(format!("Unknown source: {other}"));
                continue;
            }
        };

        match refresh_simple_source(
            client,
            cache,
            &url,
            location_cache_key,
            &source.source_id,
            source.ttl_secs,
            parse_fn,
            timeout,
        )
        .await
        {
            Ok(()) => {
                tracing::info!(
                    cache_key = %location_cache_key,
                    source = %source.source_id,
                    "Source refreshed"
                );
                result.sources_refreshed += 1;
            }
            Err(e) => {
                tracing::warn!(
                    cache_key = %location_cache_key,
                    source = %source.source_id,
                    error = %e,
                    "Source refresh failed"
                );
                result.errors.push(format!("{}: {e}", source.source_id));
            }
        }
    }

    // --- Warm observations ---
    let obs_ttl = ObservationsFetcher::ttl_secs();
    let obs_entry = ddb_cache.get(location_cache_key, "observations").await;
    result.sources_checked += 1;

    if needs_refresh(&obs_entry, obs_ttl) {
        let obs_start = std::time::Instant::now();
        // Step 1: Station discovery
        let discovery_url = build_station_discovery_url(lat, lon);
        match http_get_nws(client, &discovery_url, timeout).await {
            Ok(raw) => {
                match parse_station_discovery(&raw, lat, lon) {
                    Ok(station_info) => {
                        // Step 2: Fetch observations from the station
                        let obs_url = build_observation_url(&station_info.id);
                        match http_get_nws(client, &obs_url, timeout).await {
                            Ok(obs_raw) => {
                                match parse_observations(&obs_raw) {
                                    Ok(entries) => {
                                        let filtered = filter_observations_to_recent(entries, Utc::now());
                                        let data = ObservationData {
                                            station: station_info,
                                            entries: filtered,
                                        };
                                        match serialize_observations(&data) {
                                            Ok(bytes) => {
                                                if ddb_cache
                                                    .put(location_cache_key, "observations", &bytes, obs_ttl)
                                                    .await
                                                    .is_ok()
                                                {
                                                    tracing::info!(
                                                        cache_key = %location_cache_key,
                                                        source = "observations",
                                                        station_id = %data.station.id,
                                                        "Source refreshed"
                                                    );
                                                    emit_warmer_fetch_latency("observations", obs_start.elapsed().as_millis() as u64, true);
                                                    result.sources_refreshed += 1;
                                                }
                                            }
                                            Err(e) => {
                                                emit_warmer_fetch_latency("observations", obs_start.elapsed().as_millis() as u64, false);
                                                result.errors.push(format!("observations serialize: {e}"));
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        emit_warmer_fetch_latency("observations", obs_start.elapsed().as_millis() as u64, false);
                                        result.errors.push(format!("observations parse: {e}"));
                                    }
                                }
                            }
                            Err(e) => {
                                emit_warmer_fetch_latency("observations", obs_start.elapsed().as_millis() as u64, false);
                                result.errors.push(format!("observations fetch: {e}"));
                            }
                        }
                    }
                    Err(e) => {
                        emit_warmer_fetch_latency("observations", obs_start.elapsed().as_millis() as u64, false);
                        result.errors.push(format!("observations discovery: {e}"));
                    }
                }
            }
            Err(e) => {
                emit_warmer_fetch_latency("observations", obs_start.elapsed().as_millis() as u64, false);
                result.errors.push(format!("observations discovery: {e}"));
            }
        }
    }

    // --- Warm tides and water_temperature for Puget Sound locations ---
    if PUGET_SOUND_BOX.contains(lat, lon) {
        // Tides
        let tides_ttl = NoaaTidesFetcher::ttl_secs();
        let tides_entry = ddb_cache.get(location_cache_key, "tides").await;
        result.sources_checked += 1;

        if needs_refresh(&tides_entry, tides_ttl) {
            if let Some(station) = nearest_puget_sound_station(lat, lon) {
                let now = Utc::now();
                let begin = now.format("%Y%m%d").to_string();
                let end = (now + chrono::Duration::days(7)).format("%Y%m%d").to_string();
                let url = build_tides_url(station.id, &begin, &end);

                match http_get(client, &url, timeout).await {
                    Ok(raw) => {
                        match parse_tides_response(&raw, station.id, station.name) {
                            Ok(data) => {
                                match serialize_tides(&data) {
                                    Ok(bytes) => {
                                        if ddb_cache
                                            .put(location_cache_key, "tides", &bytes, tides_ttl)
                                            .await
                                            .is_ok()
                                        {
                                            tracing::info!(
                                                cache_key = %location_cache_key,
                                                source = "tides",
                                                station_id = station.id,
                                                "Source refreshed"
                                            );
                                            result.sources_refreshed += 1;
                                        }
                                    }
                                    Err(e) => {
                                        result.errors.push(format!("tides serialize: {e}"));
                                    }
                                }
                            }
                            Err(_) => {
                                result.errors.push("tides: parse failed".to_string());
                            }
                        }
                    }
                    Err(e) => {
                        result.errors.push(format!("tides: {e}"));
                    }
                }
            }
        }

        // Water temperature
        let wt_ttl = NoaaWaterTempFetcher::ttl_secs();
        let wt_entry = ddb_cache.get(location_cache_key, "water_temperature").await;
        result.sources_checked += 1;

        if needs_refresh(&wt_entry, wt_ttl) {
            let nearby = nearby_puget_sound_stations(lat, lon);
            let mut wt_refreshed = false;

            for station in nearby {
                let url = build_water_temp_url(station.id);
                match http_get(client, &url, timeout).await {
                    Ok(raw) => {
                        match parse_water_temp_response(&raw, station.id, station.name) {
                            Ok(data) if data.temperature_celsius.is_some() => {
                                match serialize_water_temperature(&data) {
                                    Ok(bytes) => {
                                        if ddb_cache
                                            .put(location_cache_key, "water_temperature", &bytes, wt_ttl)
                                            .await
                                            .is_ok()
                                        {
                                            tracing::info!(
                                                cache_key = %location_cache_key,
                                                source = "water_temperature",
                                                station_id = station.id,
                                                "Source refreshed"
                                            );
                                            result.sources_refreshed += 1;
                                            wt_refreshed = true;
                                            break;
                                        }
                                    }
                                    Err(_) => continue,
                                }
                            }
                            _ => continue, // Station didn't have data, try next
                        }
                    }
                    Err(_) => continue,
                }
            }

            if !wt_refreshed {
                result
                    .errors
                    .push("water_temperature: all stations failed".to_string());
            }
        }
    }

    result
}

/// Parses a cache key like "47.61_-122.33" into (lat, lon).
fn parse_cache_key(key: &str) -> Option<(f64, f64)> {
    let parts: Vec<&str> = key.splitn(2, '_').collect();
    if parts.len() != 2 {
        return None;
    }
    let lat = parts[0].parse::<f64>().ok()?;
    let lon = parts[1].parse::<f64>().ok()?;
    Some((lat, lon))
}

// ---------------------------------------------------------------------------
// Core response warming
// ---------------------------------------------------------------------------

/// Result of warming core responses for a single location.
struct CoreWarmResult {
    combinations_warmed: usize,
    errors: Vec<String>,
}

/// Warms core response caches for all tracked parameter combinations at a location.
///
/// After per-source warming completes, this function:
/// 1. Reads parameter combinations from the location tracker
/// 2. For each combination, builds the core response from cached per-source data
/// 3. Stores the result in S3 via the core response cache
async fn warm_core_responses_for_location(
    s3_cache: &dyn CacheStore,
    ddb_cache: &dyn CacheStore,
    ddb_client: &aws_sdk_dynamodb::Client,
    tracker_table: &str,
    location_cache_key: &str,
    lat: f64,
    lon: f64,
) -> CoreWarmResult {
    let mut result = CoreWarmResult {
        combinations_warmed: 0,
        errors: Vec::new(),
    };

    // Read parameter combinations from the tracker
    let combinations = get_param_combinations(ddb_client, tracker_table, location_cache_key).await;

    if combinations.is_empty() {
        return result;
    }

    tracing::info!(
        cache_key = %location_cache_key,
        combinations = combinations.len(),
        "Warming core responses for parameter combinations"
    );

    for (models, forecast_days) in &combinations {
        let models_slice = models.as_deref();
        let combo_key = core_cache_key(lat, lon, models_slice, *forecast_days);

        match build_core_response_from_cache(
            s3_cache,
            ddb_cache,
            location_cache_key,
            lat,
            lon,
            models_slice,
            *forecast_days,
        )
        .await
        {
            Ok(core_data) => {
                // Serialize and store in S3
                match serialize_core_response(&core_data) {
                    Ok(bytes) => {
                        if let Err(e) = s3_cache
                            .put(&combo_key, "core_response", &bytes, CORE_RESPONSE_TTL_SECS)
                            .await
                        {
                            tracing::warn!(
                                key = %combo_key,
                                error = %e,
                                "Failed to store core response in S3"
                            );
                            result.errors.push(format!("{combo_key}: cache put failed: {e}"));
                        } else {
                            result.combinations_warmed += 1;
                        }
                    }
                    Err(e) => {
                        tracing::warn!(key = %combo_key, error = %e, "Failed to serialize core response");
                        result.errors.push(format!("{combo_key}: serialize failed: {e}"));
                    }
                }
            }
            Err(e) => {
                tracing::warn!(key = %combo_key, error = %e, "Failed to build core response from cache");
                result.errors.push(format!("{combo_key}: {e}"));
            }
        }
    }

    result
}

/// Builds a CoreResponseData from cached per-source data.
///
/// Reads ensemble (per-model), marine, UV, and air quality from cache,
/// constructs AllSourceResults, calls build_response, and extracts the core
/// portion.
async fn build_core_response_from_cache(
    s3_cache: &dyn CacheStore,
    ddb_cache: &dyn CacheStore,
    location_cache_key: &str,
    lat: f64,
    lon: f64,
    models: Option<&[String]>,
    forecast_days: u32,
) -> Result<CoreResponseData, String> {
    // Determine which models to read
    let model_suffixes: Vec<&str> = match models {
        None => ENSEMBLE_MODELS.iter().map(|m| m.api_key_suffix).collect(),
        Some(list) => {
            let mut suffixes = Vec::new();
            for model_name in list {
                match ENSEMBLE_MODELS.iter().find(|m| m.api_key_suffix == model_name.as_str()) {
                    Some(m) => suffixes.push(m.api_key_suffix),
                    None => return Err(format!("unknown model: {model_name}")),
                }
            }
            suffixes
        }
    };

    // Read per-model ensemble caches from S3 and merge
    let mut per_model_data = Vec::new();
    let mut times: Option<Vec<String>> = None;

    for suffix in &model_suffixes {
        let source_id = format!("ensemble_{suffix}");
        let entry = s3_cache
            .get(location_cache_key, &source_id)
            .await
            .ok_or_else(|| format!("ensemble cache miss for {source_id}"))?;

        let (t, model_data) = deserialize_per_model(&entry.data)
            .map_err(|e| format!("ensemble deserialize error for {source_id}: {e}"))?;

        if times.is_none() {
            times = Some(t);
        }
        per_model_data.push(model_data);
    }

    let ensemble_times = times.unwrap_or_default();
    let model_refs: Vec<_> = per_model_data.iter().collect();
    let ensemble_data = merge_ensemble_models(ensemble_times, &model_refs);

    let fresh_meta = CacheMeta {
        age_seconds: 0,
        is_fresh: true,
        fetched_at: Utc::now().to_rfc3339(),
    };

    // Read marine cache from S3
    let marine_result = match s3_cache.get(location_cache_key, MarineFetcher::source_id()).await {
        Some(entry) => match parse_marine_response(&entry.data) {
            Ok(data) => SourceResult::Fresh(data, CacheMeta {
                age_seconds: entry.age_secs(),
                is_fresh: entry.is_fresh(),
                fetched_at: entry.stored_at.to_rfc3339(),
            }),
            Err(_) => SourceResult::Skipped,
        },
        None => SourceResult::Skipped,
    };

    // Read HRRR cache from DynamoDB (volatile, but needed for build_response)
    let hrrr_result = match ddb_cache.get(location_cache_key, HrrrFetcher::source_id()).await {
        Some(entry) => match parse_hrrr_response(&entry.data) {
            Ok(data) => SourceResult::Fresh(data, CacheMeta {
                age_seconds: entry.age_secs(),
                is_fresh: entry.is_fresh(),
                fetched_at: entry.stored_at.to_rfc3339(),
            }),
            Err(_) => SourceResult::Skipped,
        },
        None => SourceResult::Skipped,
    };

    // Read UV cache from DynamoDB
    let uv_result = match ddb_cache.get(location_cache_key, UvFetcher::source_id()).await {
        Some(entry) => match parse_uv_response(&entry.data) {
            Ok(data) => SourceResult::Fresh(data, CacheMeta {
                age_seconds: entry.age_secs(),
                is_fresh: entry.is_fresh(),
                fetched_at: entry.stored_at.to_rfc3339(),
            }),
            Err(_) => SourceResult::Skipped,
        },
        None => SourceResult::Skipped,
    };

    // Read air quality cache from DynamoDB
    let air_quality_result = match ddb_cache.get(location_cache_key, AirQualityFetcher::source_id()).await {
        Some(entry) => match parse_air_quality_response(&entry.data) {
            Ok(data) => SourceResult::Fresh(data, CacheMeta {
                age_seconds: entry.age_secs(),
                is_fresh: entry.is_fresh(),
                fetched_at: entry.stored_at.to_rfc3339(),
            }),
            Err(_) => SourceResult::Skipped,
        },
        None => SourceResult::Skipped,
    };

    // Observations, tides, water_temperature, ciops_sst are not part of core warming
    let all_results = AllSourceResults {
        ensemble: SourceResult::Fresh(ensemble_data, fresh_meta),
        marine: marine_result,
        hrrr: hrrr_result,
        uv: uv_result,
        air_quality: air_quality_result,
        observations: SourceResult::Skipped,
        tides: SourceResult::Skipped,
        water_temperature: SourceResult::Skipped,
        ciops_sst: SourceResult::Skipped,
    };

    // Build the full response using the same logic as the forecast Lambda
    let fetch_params = FetchParams {
        lat,
        lon,
        marine_lat: None,
        marine_lon: None,
        station_id: None,
        force_refresh: false,
        refresh_source: None,
        models: models.map(|m| m.to_vec()),
        forecast_days,
    };

    let response = build_response(all_results, &fetch_params);

    // Extract core data from the response (same logic as router.rs)
    Ok(CoreResponseData {
        ensemble: response.ensemble,
        marine: response.marine,
        uv: response.uv,
        air_quality: response.air_quality,
        tides: response.tides,
        water_temperature: response.water_temperature,
        ciops_sst: response.ciops_sst,
        astronomy: response.astronomy,
        cache: response
            .cache
            .into_iter()
            .filter(|(k, _)| !["observations", "hrrr"].contains(&k.as_str()))
            .collect(),
    })
}

// ---------------------------------------------------------------------------
// Models metadata warming
// ---------------------------------------------------------------------------

/// Model metadata endpoint definition (mirrors the metadata lambda).
struct ModelMetadataEndpoint {
    key: &'static str,
    url: &'static str,
}

const METADATA_ENDPOINTS: [ModelMetadataEndpoint; 9] = [
    ModelMetadataEndpoint {
        key: "ecmwf_ifs025",
        url: "https://ensemble-api.open-meteo.com/data/ecmwf_ifs025_ensemble/static/meta.json",
    },
    ModelMetadataEndpoint {
        key: "gfs_gefs",
        url: "https://ensemble-api.open-meteo.com/data/ncep_gefs025/static/meta.json",
    },
    ModelMetadataEndpoint {
        key: "icon",
        url: "https://ensemble-api.open-meteo.com/data/icon_seamless_eps/static/meta.json",
    },
    ModelMetadataEndpoint {
        key: "gem",
        url: "https://ensemble-api.open-meteo.com/data/cmc_gem_geps/static/meta.json",
    },
    ModelMetadataEndpoint {
        key: "bom_access",
        url: "https://ensemble-api.open-meteo.com/data/bom_access_global_ensemble/static/meta.json",
    },
    ModelMetadataEndpoint {
        key: "hrrr",
        url: "https://api.open-meteo.com/data/ncep_hrrr_conus/static/meta.json",
    },
    ModelMetadataEndpoint {
        key: "marine",
        url: "https://marine-api.open-meteo.com/data/ecmwf_wam025/static/meta.json",
    },
    ModelMetadataEndpoint {
        key: "air_quality",
        url: "https://air-quality-api.open-meteo.com/data/cams_global/static/meta.json",
    },
    ModelMetadataEndpoint {
        key: "uv_gfs",
        url: "https://api.open-meteo.com/data/ncep_gfs025/static/meta.json",
    },
];

/// Warms the models metadata cache by fetching from all 9 upstream endpoints
/// and storing the aggregated result in DynamoDB.
///
/// Handles partial failures: stores partial result with null for failed models.
async fn warm_models_metadata(
    http_client: &reqwest::Client,
    ddb_cache: &dyn CacheStore,
    timeout: Duration,
) -> (bool, Vec<String>) {
    let mut errors = Vec::new();
    let mut join_set = JoinSet::new();

    for endpoint in &METADATA_ENDPOINTS {
        let client = http_client.clone();
        let url = endpoint.url.to_string();
        let key = endpoint.key.to_string();
        let t = timeout;

        join_set.spawn(async move {
            let result = client
                .get(&url)
                .timeout(t)
                .send()
                .await
                .map_err(|e| format!("request failed: {e}"))
                .and_then(|resp| {
                    if resp.status().is_success() {
                        Ok(resp)
                    } else {
                        Err(format!("HTTP {}", resp.status().as_u16()))
                    }
                });

            match result {
                Ok(resp) => {
                    let body: Result<Value, String> = resp
                        .json()
                        .await
                        .map_err(|e| format!("JSON parse failed: {e}"));
                    (key, body)
                }
                Err(e) => (key, Err(e)),
            }
        });
    }

    let mut models = serde_json::Map::new();

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok((key, Ok(body))) => {
                // Extract metadata fields from the response
                let metadata = serde_json::json!({
                    "last_run_initialisation_time": unix_timestamp_to_iso(body.get("last_run_initialisation_time")),
                    "last_run_availability_time": unix_timestamp_to_iso(body.get("last_run_availability_time")),
                    "update_interval_seconds": body.get("update_interval_seconds").and_then(|v| v.as_f64()),
                });
                models.insert(key, metadata);
            }
            Ok((key, Err(e))) => {
                tracing::warn!(model = %key, error = %e, "Failed to fetch model metadata");
                errors.push(format!("{key}: {e}"));
                models.insert(key, Value::Null);
            }
            Err(join_err) => {
                tracing::warn!(error = %join_err, "Model metadata fetch task panicked");
                errors.push(format!("task panic: {join_err}"));
            }
        }
    }

    let response_body = serde_json::json!({ "models": models });
    let json_bytes = serde_json::to_vec(&response_body).unwrap_or_default();

    // Store in DynamoDB cache
    if let Err(e) = ddb_cache
        .put(
            MODELS_METADATA_CACHE_KEY,
            "metadata",
            &json_bytes,
            MODELS_METADATA_TTL_SECS,
        )
        .await
    {
        tracing::warn!(error = %e, "Failed to store models metadata in DynamoDB cache");
        errors.push(format!("cache put failed: {e}"));
        return (false, errors);
    }

    tracing::info!(
        models_fetched = models.len(),
        errors = errors.len(),
        "Models metadata warmed"
    );

    (true, errors)
}

/// Convert a Unix timestamp (integer seconds) from a JSON value to an ISO 8601 string.
fn unix_timestamp_to_iso(value: Option<&Value>) -> Option<String> {
    use chrono::DateTime;
    value
        .and_then(|v| v.as_i64())
        .and_then(|ts| DateTime::from_timestamp(ts, 0))
        .map(|dt: DateTime<Utc>| dt.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}

// ---------------------------------------------------------------------------
// Lambda handler
// ---------------------------------------------------------------------------

/// Minimum remaining time (in seconds) before we stop processing locations.
const TIMEOUT_BUFFER_SECS: u64 = 30;

/// Default HTTP timeout for upstream API requests (seconds).
const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 20;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_ansi(false)
        .without_time()
        .init();

    lambda_runtime::run(service_fn(handler)).await
}

async fn handler(event: LambdaEvent<Value>) -> Result<Value, Error> {
    let start = Utc::now();
    tracing::info!("Cache warmer invoked");

    // Read configuration from environment
    let tracker_table = std::env::var("LOCATION_TRACKER_TABLE")
        .unwrap_or_else(|_| "weather-location-tracker".to_string());
    let cache_bucket = std::env::var("CACHE_BUCKET")
        .unwrap_or_else(|_| String::new());
    let cache_table = std::env::var("CACHE_TABLE")
        .unwrap_or_else(|_| String::new());
    let concurrency_limit: usize = std::env::var("CONCURRENCY_LIMIT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);

    // Extract the Lambda deadline from the context for timeout tracking
    let deadline = event.context.deadline;

    // Initialize AWS clients
    let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let ddb_client = aws_sdk_dynamodb::Client::new(&aws_config);
    let s3_client = aws_sdk_s3::Client::new(&aws_config);
    let http_client = reqwest::Client::new();

    // Build cache stores
    let s3_cache: Arc<dyn CacheStore> = Arc::new(S3CacheStore::new(s3_client, cache_bucket));
    let ddb_cache: Arc<dyn CacheStore> = Arc::new(DynamoCacheStore::new(
        ddb_client.clone(),
        cache_table,
    ));

    let http_timeout = Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECS);

    // Step 1: Scan the location tracker table for active locations
    tracing::info!(table = %tracker_table, "Scanning location tracker table");
    let locations = scan_active_locations(&ddb_client, &tracker_table).await;
    tracing::info!(count = locations.len(), "Active locations found");

    if locations.is_empty() {
        return Ok(serde_json::json!({
            "status": "ok",
            "message": "No active locations to warm",
            "locations_found": 0,
            "locations_processed": 0,
            "sources_refreshed": 0,
            "errors": 0
        }));
    }

    // Step 2: Process locations concurrently with semaphore-based limiting
    let semaphore = Arc::new(Semaphore::new(concurrency_limit));
    let mut handles = Vec::with_capacity(locations.len());

    let mut locations_skipped = 0usize;

    for location in &locations {
        // Check remaining Lambda execution time.
        // `deadline` is milliseconds since Unix epoch (lambda_runtime 0.13).
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or(Duration::ZERO)
            .as_millis() as u64;
        let remaining_secs = deadline.saturating_sub(now_ms) / 1000;

        if remaining_secs < TIMEOUT_BUFFER_SECS {
            tracing::warn!(
                remaining_secs = remaining_secs,
                location = %location,
                "Approaching Lambda timeout, skipping remaining locations"
            );
            locations_skipped += locations.len() - handles.len();
            break;
        }

        let permit = semaphore.clone();
        let client = http_client.clone();
        let s3 = Arc::clone(&s3_cache);
        let ddb = Arc::clone(&ddb_cache);
        let ddb_cl = ddb_client.clone();
        let tracker = tracker_table.clone();
        let loc = location.clone();

        let handle = tokio::spawn(async move {
            let _permit = permit.acquire().await.expect("semaphore closed");

            // Phase 1: Warm per-source caches
            let warm_result = warm_location(&client, s3.as_ref(), ddb.as_ref(), &loc, http_timeout).await;

            // Phase 2: Warm core responses for tracked parameter combinations
            let (core_lat, core_lon) = parse_cache_key(&loc).unwrap_or((0.0, 0.0));
            let core_result = warm_core_responses_for_location(
                s3.as_ref(),
                ddb.as_ref(),
                &ddb_cl,
                &tracker,
                &loc,
                core_lat,
                core_lon,
            )
            .await;

            (warm_result, core_result)
        });

        handles.push(handle);
    }

    // Collect results
    let mut total_sources_checked = 0usize;
    let mut total_sources_refreshed = 0usize;
    let mut total_errors = 0usize;
    let mut total_core_warmed = 0usize;
    let mut locations_processed = 0usize;

    for handle in handles {
        match handle.await {
            Ok((warm_result, core_result)) => {
                locations_processed += 1;
                total_sources_checked += warm_result.sources_checked;
                total_sources_refreshed += warm_result.sources_refreshed;
                total_errors += warm_result.errors.len();
                total_core_warmed += core_result.combinations_warmed;
                total_errors += core_result.errors.len();

                if !warm_result.errors.is_empty() {
                    tracing::warn!(
                        cache_key = %warm_result.cache_key,
                        errors = ?warm_result.errors,
                        "Location had errors during per-source warming"
                    );
                }
                if !core_result.errors.is_empty() {
                    tracing::warn!(
                        cache_key = %warm_result.cache_key,
                        errors = ?core_result.errors,
                        "Location had errors during core response warming"
                    );
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Location warming task panicked");
                total_errors += 1;
            }
        }
    }

    // Step 3: Warm models metadata (after all location warming is complete)
    let (metadata_success, metadata_errors) =
        warm_models_metadata(&http_client, ddb_cache.as_ref(), http_timeout).await;
    total_errors += metadata_errors.len();

    let elapsed_ms = Utc::now()
        .signed_duration_since(start)
        .num_milliseconds();

    tracing::info!(
        locations_found = locations.len(),
        locations_processed = locations_processed,
        locations_skipped = locations_skipped,
        sources_checked = total_sources_checked,
        sources_refreshed = total_sources_refreshed,
        core_responses_warmed = total_core_warmed,
        metadata_warmed = metadata_success,
        errors = total_errors,
        elapsed_ms = elapsed_ms,
        "Cache warmer complete"
    );

    // Emit EMF metrics for dashboard visibility
    let sources_skipped = total_sources_checked.saturating_sub(total_sources_refreshed);
    emit_warmer_run_metric(
        locations.len(),
        locations_processed,
        total_sources_checked,
        total_sources_refreshed,
        sources_skipped,
        total_errors,
        elapsed_ms,
    );

    Ok(serde_json::json!({
        "status": "ok",
        "locations_found": locations.len(),
        "locations_processed": locations_processed,
        "locations_skipped": locations_skipped,
        "sources_checked": total_sources_checked,
        "sources_refreshed": total_sources_refreshed,
        "core_responses_warmed": total_core_warmed,
        "metadata_warmed": metadata_success,
        "errors": total_errors,
        "elapsed_ms": elapsed_ms
    }))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cache_key_valid() {
        assert_eq!(parse_cache_key("47.61_-122.33"), Some((47.61, -122.33)));
    }

    #[test]
    fn test_parse_cache_key_negative_lat() {
        assert_eq!(parse_cache_key("-33.87_151.21"), Some((-33.87, 151.21)));
    }

    #[test]
    fn test_parse_cache_key_zero() {
        assert_eq!(parse_cache_key("0.00_0.00"), Some((0.0, 0.0)));
    }

    #[test]
    fn test_parse_cache_key_invalid_no_underscore() {
        assert_eq!(parse_cache_key("47.61"), None);
    }

    #[test]
    fn test_parse_cache_key_invalid_not_numbers() {
        assert_eq!(parse_cache_key("abc_def"), None);
    }

    #[test]
    fn test_parse_cache_key_negative_lon() {
        // Cache key with negative longitude: "47.61_-122.33"
        // splitn(2, '_') splits into ["47.61", "-122.33"]
        let result = parse_cache_key("47.61_-122.33");
        assert_eq!(result, Some((47.61, -122.33)));
    }

    #[test]
    fn test_needs_refresh_none_entry() {
        assert!(needs_refresh(&None, 3600));
    }

    #[test]
    fn test_needs_refresh_fresh_entry() {
        let entry = CacheEntry {
            data: vec![],
            stored_at: Utc::now() - chrono::Duration::seconds(100),
            ttl_secs: 3600,
        };
        // age=100, threshold=3600-600=3000 → 100 < 3000 → no refresh needed
        assert!(!needs_refresh(&Some(entry), 3600));
    }

    #[test]
    fn test_needs_refresh_near_expiry() {
        let entry = CacheEntry {
            data: vec![],
            stored_at: Utc::now() - chrono::Duration::seconds(3100),
            ttl_secs: 3600,
        };
        // age=3100, threshold=3600-600=3000 → 3100 >= 3000 → needs refresh
        assert!(needs_refresh(&Some(entry), 3600));
    }

    #[test]
    fn test_needs_refresh_stale_entry() {
        let entry = CacheEntry {
            data: vec![],
            stored_at: Utc::now() - chrono::Duration::seconds(4000),
            ttl_secs: 3600,
        };
        // age=4000, threshold=3000 → 4000 >= 3000 → needs refresh
        assert!(needs_refresh(&Some(entry), 3600));
    }

    #[test]
    fn test_needs_refresh_exactly_at_threshold() {
        let entry = CacheEntry {
            data: vec![],
            stored_at: Utc::now() - chrono::Duration::seconds(3000),
            ttl_secs: 3600,
        };
        // age=3000, threshold=3000 → 3000 >= 3000 → needs refresh
        assert!(needs_refresh(&Some(entry), 3600));
    }

    #[test]
    fn test_needs_refresh_short_ttl() {
        // TTL shorter than the buffer — threshold saturates to 0
        let entry = CacheEntry {
            data: vec![],
            stored_at: Utc::now() - chrono::Duration::seconds(1),
            ttl_secs: 300,
        };
        // ttl=300, buffer=600, threshold=saturating_sub → 0
        // age=1, 1 >= 0 → needs refresh
        assert!(needs_refresh(&Some(entry), 300));
    }

    #[test]
    fn test_cacheable_sources_count() {
        let sources = cacheable_sources();
        // 5 ensemble + 1 marine + 1 hrrr + 1 uv + 1 air_quality = 9
        assert_eq!(sources.len(), 9);
    }

    #[test]
    fn test_cacheable_sources_ensemble_models() {
        let sources = cacheable_sources();
        let ensemble_sources: Vec<_> = sources
            .iter()
            .filter(|s| s.name == "ensemble")
            .collect();
        assert_eq!(ensemble_sources.len(), 5);

        // Verify each model has a unique source_id
        let source_ids: Vec<&str> = ensemble_sources
            .iter()
            .map(|s| s.source_id.as_str())
            .collect();
        assert!(source_ids.contains(&"ensemble_ecmwf_ifs025_ensemble"));
        assert!(source_ids.contains(&"ensemble_ncep_gefs_seamless"));
        assert!(source_ids.contains(&"ensemble_icon_seamless_eps"));
        assert!(source_ids.contains(&"ensemble_gem_global_ensemble"));
        assert!(source_ids.contains(&"ensemble_bom_access_global_ensemble"));
    }

    #[test]
    fn test_cacheable_sources_non_ensemble() {
        let sources = cacheable_sources();
        let non_ensemble: Vec<_> = sources
            .iter()
            .filter(|s| s.name != "ensemble")
            .collect();
        assert_eq!(non_ensemble.len(), 4);

        let names: Vec<&str> = non_ensemble.iter().map(|s| s.name).collect();
        assert!(names.contains(&"marine"));
        assert!(names.contains(&"hrrr"));
        assert!(names.contains(&"uv"));
        assert!(names.contains(&"air_quality"));
    }
}
