use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use futures::future::join_all;
use serde::Serialize;
use tokio::task::JoinSet;
use tracing::{info, warn};

use crate::cache::{cache_key, CacheEntry, CacheStore, DynamoCacheStore, S3CacheStore};
use crate::models::{
    nearest_puget_sound_station, nearby_puget_sound_stations, AppState, EnsembleModel, FetchParams,
    ENSEMBLE_MODELS, PUGET_SOUND_BOX, SALISH_SEA_BOX,
};
use crate::sources::ensemble_splitter::{
    deserialize_per_model, merge_ensemble_models, serialize_per_model, split_ensemble_by_model,
};
use crate::sources::air_quality::{
    build_air_quality_url, parse_air_quality_response, AirQualityData, AirQualityFetcher,
};
use crate::sources::ciops_sst::{
    build_ciops_wms_url, generate_ciops_time_steps, parse_ciops_feature_info, CiopsSstData,
};
use crate::sources::ensemble::{
    build_ensemble_url, parse_ensemble_response, EnsembleFetcher, ParsedEnsembleData,
};
use crate::sources::hrrr::{
    build_hrrr_url, filter_to_recent, parse_hrrr_response, HrrrData, HrrrFetcher,
};
use crate::sources::marine::{
    build_marine_url, parse_marine_response, MarineData, MarineFetcher,
};
use crate::sources::noaa_tides::{
    build_tides_url, deserialize_tides, parse_tides_response, serialize_tides, TidesData,
};
use crate::sources::noaa_water_temp::{
    build_water_temp_url, deserialize_water_temperature, parse_water_temp_response,
    serialize_water_temperature, WaterTemperatureData,
};
use crate::sources::observations::{
    build_observation_url, build_station_discovery_url, deserialize_observations,
    filter_observations_to_recent, parse_observations, parse_station_discovery, ObservationData,
    serialize_observations,
};
use crate::sources::uv::{build_uv_url, parse_uv_response, UvData, UvFetcher};

// ---------------------------------------------------------------------------
// CacheMeta — metadata about a cached entry returned to clients
// ---------------------------------------------------------------------------

/// Metadata about a cached entry, included in the response so clients can
/// display data freshness information.
#[derive(Debug, Clone, Serialize)]
pub struct CacheMeta {
    /// Age of the cached entry in seconds.
    pub age_seconds: u64,
    /// Whether the cached entry is still within its TTL.
    pub is_fresh: bool,
    /// ISO 8601 timestamp of when the entry was originally fetched.
    pub fetched_at: String,
}

impl CacheMeta {
    /// Build a `CacheMeta` from a `CacheEntry`.
    fn from_entry(entry: &CacheEntry) -> Self {
        Self {
            age_seconds: entry.age_secs(),
            is_fresh: entry.is_fresh(),
            fetched_at: entry.stored_at.to_rfc3339(),
        }
    }

    /// Build a `CacheMeta` representing a freshly-fetched result (age 0).
    fn fresh_now() -> Self {
        Self {
            age_seconds: 0,
            is_fresh: true,
            fetched_at: Utc::now().to_rfc3339(),
        }
    }
}

// ---------------------------------------------------------------------------
// UpstreamError — classification of upstream fetch failures
// ---------------------------------------------------------------------------

/// Classifies errors from upstream HTTP fetches so the orchestrator can
/// decide on fallback behaviour (stale cache, throttle warning, etc.).
#[derive(Debug)]
pub enum UpstreamError {
    /// The HTTP request timed out.
    Timeout,
    /// The upstream returned a non-2xx status code.
    HttpError(u16, String),
    /// The upstream returned HTTP 420 (rate limit).
    Throttled,
    /// The response body could not be parsed.
    ParseError(String),
    /// A network-level error (DNS failure, connection refused, etc.).
    NetworkError(String),
}

impl std::fmt::Display for UpstreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UpstreamError::Timeout => write!(f, "upstream timeout"),
            UpstreamError::HttpError(code, msg) => write!(f, "HTTP {code}: {msg}"),
            UpstreamError::Throttled => write!(f, "upstream throttled (HTTP 420)"),
            UpstreamError::ParseError(msg) => write!(f, "parse error: {msg}"),
            UpstreamError::NetworkError(msg) => write!(f, "network error: {msg}"),
        }
    }
}

// ---------------------------------------------------------------------------
// SourceResult — outcome of fetching a single data source
// ---------------------------------------------------------------------------

/// The outcome of fetching a single upstream data source, capturing whether
/// the data came from cache, was freshly fetched, or fell back to stale data.
#[derive(Debug)]
pub enum SourceResult<T> {
    /// Data came from cache and is within its TTL.
    Fresh(T, CacheMeta),
    /// Data was freshly fetched from upstream and the cache was updated.
    Refreshed(T, CacheMeta),
    /// Upstream failed but stale cached data is available. The `String` is
    /// the error message from the upstream failure.
    Stale(T, CacheMeta, String),
    /// Upstream returned HTTP 420 (throttled); returning cached data with a
    /// throttle warning.
    Throttled(T, CacheMeta),
    /// Upstream failed and no cached data is available.
    Failed(String),
    /// This source was not applicable for the request (e.g., NOAA outside
    /// Puget Sound).
    Skipped,
}

impl<T> SourceResult<T> {
    /// Returns a reference to the data if available.
    pub fn data(&self) -> Option<&T> {
        match self {
            SourceResult::Fresh(d, _)
            | SourceResult::Refreshed(d, _)
            | SourceResult::Stale(d, _, _)
            | SourceResult::Throttled(d, _) => Some(d),
            SourceResult::Failed(_) | SourceResult::Skipped => None,
        }
    }

    /// Returns the cache metadata if available.
    pub fn cache_meta(&self) -> Option<&CacheMeta> {
        match self {
            SourceResult::Fresh(_, m)
            | SourceResult::Refreshed(_, m)
            | SourceResult::Stale(_, m, _)
            | SourceResult::Throttled(_, m) => Some(m),
            SourceResult::Failed(_) | SourceResult::Skipped => None,
        }
    }

    /// Returns the error message if the result represents a failure or stale
    /// fallback.
    pub fn error_message(&self) -> Option<&str> {
        match self {
            SourceResult::Stale(_, _, e) | SourceResult::Failed(e) => Some(e),
            _ => None,
        }
    }

    /// Returns `true` if this result was throttled.
    pub fn is_throttled(&self) -> bool {
        matches!(self, SourceResult::Throttled(_, _))
    }
}

// ---------------------------------------------------------------------------
// AllSourceResults — collected results from all upstream sources
// ---------------------------------------------------------------------------

/// Holds the results from all upstream data sources after the two-phase
/// fetch completes.
pub struct AllSourceResults {
    pub ensemble: SourceResult<ParsedEnsembleData>,
    pub marine: SourceResult<MarineData>,
    pub hrrr: SourceResult<HrrrData>,
    pub uv: SourceResult<UvData>,
    pub air_quality: SourceResult<AirQualityData>,
    pub observations: SourceResult<ObservationData>,
    pub tides: SourceResult<TidesData>,
    pub water_temperature: SourceResult<WaterTemperatureData>,
    pub ciops_sst: SourceResult<CiopsSstData>,
}


// ---------------------------------------------------------------------------
// HTTP fetch helper — classifies reqwest errors into UpstreamError
// ---------------------------------------------------------------------------

/// Makes an HTTP GET request and returns the raw response bytes, or an
/// `UpstreamError` if the request fails.
async fn http_get(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> Result<Vec<u8>, UpstreamError> {
    let response = client
        .get(url)
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                UpstreamError::Timeout
            } else if e.is_connect() {
                UpstreamError::NetworkError(e.to_string())
            } else {
                UpstreamError::NetworkError(e.to_string())
            }
        })?;

    let status = response.status().as_u16();
    if status == 420 {
        return Err(UpstreamError::Throttled);
    }
    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(UpstreamError::HttpError(status, body));
    }

    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| UpstreamError::NetworkError(e.to_string()))
}

/// Makes an HTTP GET request with NWS-required headers.
async fn http_get_nws(
    client: &reqwest::Client,
    url: &str,
    timeout: Duration,
) -> Result<Vec<u8>, UpstreamError> {
    let response = client
        .get(url)
        .header("User-Agent", "EnsembleWeather/1.0.0")
        .header("Accept", "application/geo+json")
        .timeout(timeout)
        .send()
        .await
        .map_err(|e| {
            if e.is_timeout() {
                UpstreamError::Timeout
            } else {
                UpstreamError::NetworkError(e.to_string())
            }
        })?;

    let status = response.status().as_u16();
    if status == 420 {
        return Err(UpstreamError::Throttled);
    }
    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(UpstreamError::HttpError(status, body));
    }

    response
        .bytes()
        .await
        .map(|b| b.to_vec())
        .map_err(|e| UpstreamError::NetworkError(e.to_string()))
}

// ---------------------------------------------------------------------------
// fetch_with_cache — generic cache-aware fetch for a single source
// ---------------------------------------------------------------------------

/// Fetches data for a single source with cache-aware logic:
///
/// 1. Check cache → if fresh and not force-refreshing, return `Fresh`.
/// 2. If stale or miss, fetch upstream.
/// 3. On success → return `Refreshed` and update cache.
/// 4. On HTTP 420 with cache → return `Throttled`.
/// 5. On failure with stale cache → return `Stale`.
/// 6. On failure without cache → return `Failed`.
///
/// The `parse_fn` converts raw bytes into the typed data `T`.
/// The `fetch_fn` is an async closure that performs the HTTP request.
async fn fetch_with_cache<T, F, Fut>(
    cache: &dyn CacheStore,
    ck: &str,
    source_id: &str,
    ttl_secs: u64,
    force_refresh: bool,
    parse_fn: F,
    fetch_fn: Fut,
) -> SourceResult<T>
where
    T: Clone,
    F: Fn(&[u8]) -> Result<T, String>,
    Fut: std::future::Future<Output = Result<Vec<u8>, UpstreamError>>,
{
    // Step 1: Check cache (unless force-refreshing)
    let cached = if force_refresh {
        info!(source = source_id, "Bypassing cache (force refresh)");
        None
    } else {
        cache.get(ck, source_id).await
    };

    if let Some(ref entry) = cached {
        if entry.is_fresh() {
            // Cache hit — parse and return
            match parse_fn(&entry.data) {
                Ok(data) => {
                    info!(source = source_id, age_secs = entry.age_secs(), "Cache hit (fresh)");
                    return SourceResult::Fresh(data, CacheMeta::from_entry(entry));
                }
                Err(_) => {
                    warn!(source = source_id, "Cached data corrupt, fetching upstream");
                }
            }
        } else {
            info!(source = source_id, age_secs = entry.age_secs(), "Cache stale, fetching upstream");
        }
    } else if !force_refresh {
        info!(source = source_id, "Cache miss");
    }

    // Step 2: Fetch upstream
    info!(source = source_id, "Fetching upstream");
    let fetch_start = std::time::Instant::now();
    match fetch_fn.await {
        Ok(raw) => {
            let fetch_elapsed_ms = fetch_start.elapsed().as_millis();
            info!(source = source_id, bytes = raw.len(), elapsed_ms = fetch_elapsed_ms, "Upstream response received");
            // Parse the response
            match parse_fn(&raw) {
                Ok(data) => {
                    // Update cache (fire-and-forget; errors are non-fatal)
                    let _ = cache.put(ck, source_id, &raw, ttl_secs).await;
                    info!(source = source_id, "Cached updated");
                    SourceResult::Refreshed(data, CacheMeta::fresh_now())
                }
                Err(e) => {
                    warn!(source = source_id, error = %e, "Parse failed after upstream fetch");
                    // Parse failed — fall back to stale cache if available
                    if let Some(ref entry) = cached {
                        if let Ok(stale_data) = parse_fn(&entry.data) {
                            return SourceResult::Stale(
                                stale_data,
                                CacheMeta::from_entry(entry),
                                format!("parse error: {e}"),
                            );
                        }
                    }
                    SourceResult::Failed(format!("parse error: {e}"))
                }
            }
        }
        Err(UpstreamError::Throttled) => {
            warn!(source = source_id, "Upstream throttled (HTTP 420)");
            // HTTP 420 — use cached data if available
            if let Some(ref entry) = cached {
                if let Ok(data) = parse_fn(&entry.data) {
                    return SourceResult::Throttled(data, CacheMeta::from_entry(entry));
                }
            }
            SourceResult::Failed("upstream throttled (HTTP 420), no cached data available".into())
        }
        Err(err) => {
            warn!(source = source_id, error = %err, "Upstream fetch failed");
            // Other upstream failure — fall back to stale cache
            let err_msg = err.to_string();
            if let Some(ref entry) = cached {
                if let Ok(stale_data) = parse_fn(&entry.data) {
                    return SourceResult::Stale(
                        stale_data,
                        CacheMeta::from_entry(entry),
                        err_msg,
                    );
                }
            }
            SourceResult::Failed(err_msg)
        }
    }
}


// ---------------------------------------------------------------------------
// Per-source fetch functions
// ---------------------------------------------------------------------------

/// Determines whether a specific source should bypass cache based on the
/// `force_refresh` flag or the `refresh_source` parameter.
fn should_force_refresh(params: &FetchParams, source_id: &str) -> bool {
    if params.force_refresh {
        return true;
    }
    if let Some(ref rs) = params.refresh_source {
        return rs == source_id;
    }
    false
}

/// Fetch ensemble data with per-model caching.
///
/// Checks per-model S3 cache freshness for each selected model concurrently.
/// If all selected models are fresh, loads and merges their cached data.
/// If any selected model is stale or missing, fetches from upstream, splits
/// by model, caches all 5 models separately, then merges only the selected.
pub async fn fetch_ensemble_per_model(
    client: &reqwest::Client,
    cache: &dyn CacheStore,
    params: &FetchParams,
    selected_models: &[&EnsembleModel],
    timeout: Duration,
) -> SourceResult<ParsedEnsembleData> {
    let ck = cache_key(params.lat, params.lon);
    let force = should_force_refresh(params, EnsembleFetcher::source_id());

    // Build per-model source IDs (e.g., "ensemble_ecmwf_ifs025_ensemble")
    let model_source_ids: Vec<(&EnsembleModel, String)> = selected_models
        .iter()
        .map(|m| (*m, format!("ensemble_{}", m.api_key_suffix)))
        .collect();

    // -----------------------------------------------------------------------
    // Step 1: Check per-model cache freshness concurrently
    // -----------------------------------------------------------------------
    if !force {
        let mut cache_futures = Vec::with_capacity(model_source_ids.len());
        for (model, source_id) in &model_source_ids {
            cache_futures.push(async {
                let entry = cache.get(&ck, source_id).await;
                (*model, source_id.as_str(), entry)
            });
        }

        let cache_results = join_all(cache_futures).await;

        // Check if all selected models have fresh cache entries
        let all_fresh = cache_results
            .iter()
            .all(|(_, _, entry)| entry.as_ref().map(|e| e.is_fresh()).unwrap_or(false));

        if all_fresh {
            info!("All selected ensemble models have fresh cache entries");

            // Load and merge cached data for selected models
            let mut per_model_data = Vec::with_capacity(selected_models.len());
            let mut times: Option<Vec<String>> = None;
            let mut oldest_entry: Option<&CacheEntry> = None;

            for (_, _, entry) in &cache_results {
                let entry = entry.as_ref().unwrap(); // safe: all_fresh guarantees Some
                match deserialize_per_model(&entry.data) {
                    Ok((t, model_data)) => {
                        if times.is_none() {
                            times = Some(t);
                        }
                        per_model_data.push(model_data);

                        // Track the oldest entry for CacheMeta
                        match oldest_entry {
                            None => oldest_entry = Some(entry),
                            Some(prev) if entry.age_secs() > prev.age_secs() => {
                                oldest_entry = Some(entry)
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Cached per-model data corrupt, fetching upstream");
                        // Fall through to upstream fetch
                        return fetch_ensemble_upstream(
                            client, cache, params, selected_models, &ck, timeout, None,
                        )
                        .await;
                    }
                }
            }

            let times = times.unwrap_or_default();
            let model_refs: Vec<&_> = per_model_data.iter().collect();
            let merged = merge_ensemble_models(times, &model_refs);
            let meta = CacheMeta::from_entry(oldest_entry.unwrap());

            return SourceResult::Fresh(merged, meta);
        }

        // Collect stale entries for fallback
        let stale_entries: Vec<_> = cache_results
            .into_iter()
            .map(|(model, source_id, entry)| (model, source_id.to_string(), entry))
            .collect();

        return fetch_ensemble_upstream(
            client,
            cache,
            params,
            selected_models,
            &ck,
            timeout,
            Some(stale_entries),
        )
        .await;
    }

    // force_refresh — skip cache entirely
    fetch_ensemble_upstream(client, cache, params, selected_models, &ck, timeout, None).await
}

/// Helper: fetch ensemble data from upstream, split by model, cache all 5,
/// and merge only the selected models.
async fn fetch_ensemble_upstream(
    client: &reqwest::Client,
    cache: &dyn CacheStore,
    params: &FetchParams,
    selected_models: &[&EnsembleModel],
    ck: &str,
    timeout: Duration,
    stale_entries: Option<Vec<(&EnsembleModel, String, Option<CacheEntry>)>>,
) -> SourceResult<ParsedEnsembleData> {
    let url = build_ensemble_url(params.lat, params.lon);

    info!(source = "ensemble", "Fetching upstream (per-model)");
    match http_get(client, &url, timeout).await {
        Ok(raw) => {
            info!(source = "ensemble", bytes = raw.len(), "Upstream response received");
            match parse_ensemble_response(&raw) {
                Ok(combined) => {
                    // Split by model and cache ALL 5 models
                    let split = split_ensemble_by_model(&combined);
                    for model in &ENSEMBLE_MODELS {
                        let source_id = format!("ensemble_{}", model.api_key_suffix);
                        if let Some(model_data) = split.get(model.api_key_suffix) {
                            if let Ok(bytes) = serialize_per_model(&combined.times, model_data) {
                                let _ = cache
                                    .put(ck, &source_id, &bytes, EnsembleFetcher::ttl_secs())
                                    .await;
                            }
                        }
                    }

                    // Merge only the selected models
                    let selected_data: Vec<&_> = selected_models
                        .iter()
                        .filter_map(|m| split.get(m.api_key_suffix))
                        .collect();
                    let merged = merge_ensemble_models(combined.times, &selected_data);

                    SourceResult::Refreshed(merged, CacheMeta::fresh_now())
                }
                Err(e) => {
                    warn!(source = "ensemble", error = %e, "Parse failed after upstream fetch");
                    // Fall back to stale cache if available
                    try_stale_fallback(selected_models, stale_entries, format!("parse error: {e}"))
                }
            }
        }
        Err(UpstreamError::Throttled) => {
            warn!(source = "ensemble", "Upstream throttled (HTTP 420)");
            // Try to return cached data (even stale) on throttle
            if let Some(stale) = stale_entries {
                if let Some(merged) = try_merge_stale_cache(selected_models, &stale) {
                    let meta = stale_cache_meta(&stale);
                    return SourceResult::Throttled(merged, meta);
                }
            }
            SourceResult::Failed(
                "upstream throttled (HTTP 420), no cached data available".into(),
            )
        }
        Err(err) => {
            warn!(source = "ensemble", error = %err, "Upstream fetch failed");
            let err_msg = err.to_string();
            try_stale_fallback(selected_models, stale_entries, err_msg)
        }
    }
}

/// Attempt to merge stale cached data for the selected models.
/// Returns `None` if any selected model lacks a cache entry.
fn try_merge_stale_cache(
    selected_models: &[&EnsembleModel],
    stale_entries: &[(&EnsembleModel, String, Option<CacheEntry>)],
) -> Option<ParsedEnsembleData> {
    let mut per_model_data = Vec::with_capacity(selected_models.len());
    let mut times: Option<Vec<String>> = None;

    for model in selected_models {
        // Find the stale entry for this model
        let entry = stale_entries
            .iter()
            .find(|(m, _, _)| m.api_key_suffix == model.api_key_suffix)
            .and_then(|(_, _, e)| e.as_ref())?;

        let (t, model_data) = deserialize_per_model(&entry.data).ok()?;
        if times.is_none() {
            times = Some(t);
        }
        per_model_data.push(model_data);
    }

    let times = times.unwrap_or_default();
    let model_refs: Vec<&_> = per_model_data.iter().collect();
    Some(merge_ensemble_models(times, &model_refs))
}

/// Build a CacheMeta from the oldest stale entry.
fn stale_cache_meta(stale_entries: &[(&EnsembleModel, String, Option<CacheEntry>)]) -> CacheMeta {
    let oldest = stale_entries
        .iter()
        .filter_map(|(_, _, e)| e.as_ref())
        .max_by_key(|e| e.age_secs());

    match oldest {
        Some(entry) => CacheMeta::from_entry(entry),
        None => CacheMeta::fresh_now(),
    }
}

/// Try to fall back to stale cached data, or return Failed.
fn try_stale_fallback(
    selected_models: &[&EnsembleModel],
    stale_entries: Option<Vec<(&EnsembleModel, String, Option<CacheEntry>)>>,
    err_msg: String,
) -> SourceResult<ParsedEnsembleData> {
    if let Some(stale) = stale_entries {
        if let Some(merged) = try_merge_stale_cache(selected_models, &stale) {
            let meta = stale_cache_meta(&stale);
            return SourceResult::Stale(merged, meta, err_msg);
        }
    }
    SourceResult::Failed(err_msg)
}

/// Fetch marine data with cache support.
async fn fetch_marine(
    client: &reqwest::Client,
    cache: &dyn CacheStore,
    params: &FetchParams,
    timeout: Duration,
) -> SourceResult<MarineData> {
    let mlat = params.marine_lat.unwrap_or(params.lat);
    let mlon = params.marine_lon.unwrap_or(params.lon);
    let ck = cache_key(params.lat, params.lon); // Use primary location key to align with cache warmer
    let url = build_marine_url(mlat, mlon);
    let force = should_force_refresh(params, MarineFetcher::source_id());

    fetch_with_cache(
        cache,
        &ck,
        MarineFetcher::source_id(),
        MarineFetcher::ttl_secs(),
        force,
        |raw| parse_marine_response(raw),
        http_get(client, &url, timeout),
    )
    .await
}

/// Fetch HRRR data with cache support. Applies the 12-hour time filter
/// after parsing.
async fn fetch_hrrr(
    client: &reqwest::Client,
    cache: &dyn CacheStore,
    params: &FetchParams,
    timeout: Duration,
) -> SourceResult<HrrrData> {
    let ck = cache_key(params.lat, params.lon);
    let url = build_hrrr_url(params.lat, params.lon);
    let force = should_force_refresh(params, HrrrFetcher::source_id());
    let now = Utc::now();

    let result = fetch_with_cache(
        cache,
        &ck,
        HrrrFetcher::source_id(),
        HrrrFetcher::ttl_secs(),
        force,
        |raw| parse_hrrr_response(raw),
        http_get(client, &url, timeout),
    )
    .await;

    // Apply the 12-hour time filter to the parsed data
    match result {
        SourceResult::Fresh(data, meta) => {
            SourceResult::Fresh(filter_to_recent(data, now), meta)
        }
        SourceResult::Refreshed(data, meta) => {
            SourceResult::Refreshed(filter_to_recent(data, now), meta)
        }
        SourceResult::Stale(data, meta, err) => {
            SourceResult::Stale(filter_to_recent(data, now), meta, err)
        }
        SourceResult::Throttled(data, meta) => {
            SourceResult::Throttled(filter_to_recent(data, now), meta)
        }
        other => other,
    }
}

/// Fetch UV data with cache support.
async fn fetch_uv(
    client: &reqwest::Client,
    cache: &dyn CacheStore,
    params: &FetchParams,
    timeout: Duration,
) -> SourceResult<UvData> {
    let ck = cache_key(params.lat, params.lon);
    let url = build_uv_url(params.lat, params.lon);
    let force = should_force_refresh(params, UvFetcher::source_id());

    fetch_with_cache(
        cache,
        &ck,
        UvFetcher::source_id(),
        UvFetcher::ttl_secs(),
        force,
        |raw| parse_uv_response(raw),
        http_get(client, &url, timeout),
    )
    .await
}

/// Fetch air quality data with cache support.
async fn fetch_air_quality(
    client: &reqwest::Client,
    cache: &dyn CacheStore,
    params: &FetchParams,
    timeout: Duration,
) -> SourceResult<AirQualityData> {
    let ck = cache_key(params.lat, params.lon);
    let url = build_air_quality_url(params.lat, params.lon);
    let force = should_force_refresh(params, AirQualityFetcher::source_id());

    fetch_with_cache(
        cache,
        &ck,
        AirQualityFetcher::source_id(),
        AirQualityFetcher::ttl_secs(),
        force,
        |raw| parse_air_quality_response(raw),
        http_get(client, &url, timeout),
    )
    .await
}

/// Fetch NWS observations with DynamoDB cache support (300s TTL).
///
/// The cached data includes both the station discovery result and the
/// observation entries, so a cache hit avoids both NWS API calls.
///
/// If a `station_id` is provided, fetches directly from that station.
/// Otherwise, discovers the nearest station via the NWS points API.
async fn fetch_observations(
    client: &reqwest::Client,
    cache: &dyn CacheStore,
    params: &FetchParams,
    timeout: Duration,
) -> SourceResult<ObservationData> {
    let ck = cache_key(params.lat, params.lon);
    let force = should_force_refresh(params, "observations");
    let now = Utc::now();

    let result = fetch_with_cache(
        cache,
        &ck,
        "observations",
        1800, // 30-minute TTL
        force,
        |raw| {
            deserialize_observations(raw)
                .map_err(|e| format!("observation deserialize error: {e}"))
        },
        async {
            // Perform station discovery (if needed) and observation fetch,
            // then serialize the combined result for caching.

            // Step 1: Determine the station to fetch from
            let station_info = if let Some(ref sid) = params.station_id {
                crate::sources::observations::StationInfo {
                    id: sid.clone(),
                    name: String::new(),
                    latitude: params.lat,
                    longitude: params.lon,
                    distance_km: 0.0,
                }
            } else {
                let discovery_url = build_station_discovery_url(params.lat, params.lon);
                let raw = http_get_nws(client, &discovery_url, timeout).await?;
                parse_station_discovery(&raw, params.lat, params.lon)
                    .map_err(|e| UpstreamError::ParseError(format!("station discovery: {e}")))?
            };

            // Step 2: Fetch observations from the station
            let obs_url = build_observation_url(&station_info.id);
            let raw = http_get_nws(client, &obs_url, timeout).await?;
            let entries = parse_observations(&raw)
                .map_err(|e| UpstreamError::ParseError(format!("observations: {e}")))?;

            // Step 3: Filter to recent 12 hours
            let filtered = filter_observations_to_recent(entries, now);

            let data = ObservationData {
                station: station_info,
                entries: filtered,
            };

            // Serialize the combined station + entries for caching
            serialize_observations(&data)
                .map_err(|e| UpstreamError::ParseError(format!("observation serialize: {e}")))
        },
    )
    .await;

    result
}

/// Fetch NOAA water temperature with DynamoDB cache support (900s TTL).
///
/// Tries the given station first. If it fails (e.g., the station doesn't
/// offer water temperature), tries fallback stations in order of proximity.
///
/// The cached data includes the successful station's result, so a cache hit
/// avoids all NOAA API calls. Uses the primary location cache key so the
/// cache warmer can keep it warm.
async fn fetch_water_temperature(
    client: &reqwest::Client,
    cache: &dyn CacheStore,
    primary_cache_key: &str,
    stations: &[(&str, &str)], // (station_id, station_name) pairs, nearest first
    timeout: Duration,
) -> SourceResult<WaterTemperatureData> {
    fetch_with_cache(
        cache,
        primary_cache_key,
        "water_temperature",
        3600, // 1-hour TTL (water temp changes slowly)
        false,
        |raw| {
            deserialize_water_temperature(raw)
                .map_err(|e| format!("water_temperature deserialize error: {e}"))
        },
        async {
            // Try each station in order until one succeeds with a temperature
            for (station_id, station_name) in stations {
                let url = build_water_temp_url(station_id);
                match http_get(client, &url, timeout).await {
                    Ok(raw) => {
                        match parse_water_temp_response(&raw, station_id, station_name) {
                            Ok(data) if data.temperature_celsius.is_some() => {
                                info!(
                                    station_id = station_id,
                                    station_name = station_name,
                                    "Water temperature fetched successfully"
                                );
                                return serialize_water_temperature(&data)
                                    .map_err(|e| UpstreamError::ParseError(
                                        format!("water_temperature serialize: {e}")
                                    ));
                            }
                            Ok(_) => {
                                warn!(
                                    station_id = station_id,
                                    station_name = station_name,
                                    "Water temperature returned null, trying next station"
                                );
                                continue;
                            }
                            Err(e) => {
                                warn!(
                                    station_id = station_id,
                                    station_name = station_name,
                                    error = %e,
                                    "Water temperature parse failed, trying next station"
                                );
                                continue;
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            station_id = station_id,
                            station_name = station_name,
                            error = %e,
                            "Water temperature not available at this station, trying next"
                        );
                        continue;
                    }
                }
            }

            Err(UpstreamError::ParseError(
                "water temperature not available at any nearby NOAA station".to_string(),
            ))
        },
    )
    .await
}

/// Fetch NOAA tide predictions with DynamoDB cache support (3600s TTL).
///
/// The cached data is serialized as JSON. On cache hit, the data is
/// deserialized without any NOAA API calls.
///
/// Fetch NOAA tide predictions with DynamoDB cache support (3600s TTL).
///
/// The cached data is serialized as JSON. On cache hit, the data is
/// deserialized without any NOAA API calls.
///
/// Uses the primary location cache key so the cache warmer can keep it warm.
async fn fetch_tides(
    client: &reqwest::Client,
    cache: &dyn CacheStore,
    primary_cache_key: &str,
    station_id: &str,
    station_name: &str,
    begin_date: &str,
    end_date: &str,
    timeout: Duration,
) -> SourceResult<TidesData> {
    fetch_with_cache(
        cache,
        primary_cache_key,
        "tides",
        3600, // 1-hour TTL
        false,
        |raw| {
            deserialize_tides(raw)
                .map_err(|e| format!("tides deserialize error: {e}"))
        },
        async {
            let url = build_tides_url(station_id, begin_date, end_date);
            let raw = http_get(client, &url, timeout).await?;
            let data = parse_tides_response(&raw, station_id, station_name)
                .map_err(|e| UpstreamError::ParseError(format!("tides: {e}")))?;

            serialize_tides(&data)
                .map_err(|e| UpstreamError::ParseError(format!("tides serialize: {e}")))
        },
    )
    .await
}

/// Fetch CIOPS SST data by making 9 concurrent WMS requests (not cached).
async fn fetch_ciops_sst(
    client: &reqwest::Client,
    lat: f64,
    lon: f64,
    timeout: Duration,
) -> SourceResult<CiopsSstData> {
    let time_steps = generate_ciops_time_steps(Utc::now());

    let mut join_set = JoinSet::new();
    for ts in &time_steps {
        let url = build_ciops_wms_url(lat, lon, ts);
        let client = client.clone();
        let ts_clone = *ts;
        let timeout = timeout;
        join_set.spawn(async move {
            let result = http_get(&client, &url, timeout).await;
            (ts_clone, result)
        });
    }

    let mut results: Vec<(chrono::DateTime<Utc>, Option<f64>)> = Vec::with_capacity(9);
    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok((ts, Ok(raw))) => {
                let temp = parse_ciops_feature_info(&raw);
                results.push((ts, temp));
            }
            Ok((ts, Err(_))) => {
                // Per-time-step failure → None entry
                results.push((ts, None));
            }
            Err(_join_err) => {
                // Task panicked — skip this time step
            }
        }
    }

    // Sort by time step to maintain chronological order
    results.sort_by_key(|(ts, _)| *ts);

    let times: Vec<String> = results.iter().map(|(ts, _)| ts.to_rfc3339()).collect();
    let temperatures_celsius: Vec<Option<f64>> = results.iter().map(|(_, t)| *t).collect();

    let data = CiopsSstData {
        times,
        temperatures_celsius,
    };

    SourceResult::Refreshed(data, CacheMeta::fresh_now())
}


// ---------------------------------------------------------------------------
// fetch_all_sources — two-phase orchestration
// ---------------------------------------------------------------------------

/// Orchestrates the two-phase fetch of all upstream data sources.
///
/// **Phase 1**: Spawn concurrent tasks for all primary sources (ensemble,
/// marine, HRRR, UV, air quality, observations).
///
/// **Phase 2**: After marine completes, evaluate conditional fetch triggers:
/// - If marine SST is all null AND location is in Puget Sound box AND a
///   nearby NOAA station exists → fetch NOAA water temp + tides.
/// - If marine SST is all null AND location is in Salish Sea box → fetch
///   CIOPS SST.
///
/// Returns an `AllSourceResults` with the outcome of every source.
pub async fn fetch_all_sources(
    state: &AppState,
    params: &FetchParams,
    selected_models: &[&EnsembleModel],
) -> AllSourceResults {
    let orchestration_start = std::time::Instant::now();
    let timeout = Duration::from_secs(state.config.default_timeout_secs);
    let client = &state.http_client;

    info!(
        lat = params.lat,
        lon = params.lon,
        timeout_secs = state.config.default_timeout_secs,
        "Starting fetch orchestration"
    );

    // Build cache stores
    let s3_cache: Arc<dyn CacheStore> = Arc::new(S3CacheStore::new(
        state.s3_client.clone(),
        state.config.cache_bucket.clone(),
    ));
    let ddb_cache: Arc<dyn CacheStore> = Arc::new(DynamoCacheStore::new(
        state.ddb_client.clone(),
        state.config.cache_table.clone(),
    ));

    // -----------------------------------------------------------------------
    // Phase 1: Primary sources — all concurrent
    // -----------------------------------------------------------------------

    // We need marine to complete before Phase 2, so we use individual tasks
    // rather than a single JoinSet, allowing us to await marine first.

    let client_clone = client.clone();
    let s3_cache_clone = Arc::clone(&s3_cache);
    let params_clone = params.clone();
    let selected_models_owned: Vec<&'static EnsembleModel> = selected_models
        .iter()
        .map(|m| {
            // Map back to 'static references from ENSEMBLE_MODELS
            ENSEMBLE_MODELS
                .iter()
                .find(|em| em.api_key_suffix == m.api_key_suffix)
                .expect("selected_models should only contain valid ENSEMBLE_MODELS entries")
        })
        .collect();
    let ensemble_task = tokio::spawn(async move {
        let model_refs: Vec<&EnsembleModel> = selected_models_owned.iter().copied().collect();
        fetch_ensemble_per_model(
            &client_clone,
            s3_cache_clone.as_ref(),
            &params_clone,
            &model_refs,
            timeout,
        )
        .await
    });

    let client_clone = client.clone();
    let s3_cache_clone = Arc::clone(&s3_cache);
    let params_clone = params.clone();
    let marine_task = tokio::spawn(async move {
        fetch_marine(&client_clone, s3_cache_clone.as_ref(), &params_clone, timeout).await
    });

    let client_clone = client.clone();
    let ddb_cache_clone = Arc::clone(&ddb_cache);
    let params_clone = params.clone();
    let hrrr_task = tokio::spawn(async move {
        fetch_hrrr(&client_clone, ddb_cache_clone.as_ref(), &params_clone, timeout).await
    });

    let client_clone = client.clone();
    let ddb_cache_clone = Arc::clone(&ddb_cache);
    let params_clone = params.clone();
    let uv_task = tokio::spawn(async move {
        fetch_uv(&client_clone, ddb_cache_clone.as_ref(), &params_clone, timeout).await
    });

    let client_clone = client.clone();
    let ddb_cache_clone = Arc::clone(&ddb_cache);
    let params_clone = params.clone();
    let air_quality_task = tokio::spawn(async move {
        fetch_air_quality(&client_clone, ddb_cache_clone.as_ref(), &params_clone, timeout).await
    });

    let client_clone = client.clone();
    let ddb_cache_clone = Arc::clone(&ddb_cache);
    let params_clone = params.clone();
    let observations_task = tokio::spawn(async move {
        fetch_observations(&client_clone, ddb_cache_clone.as_ref(), &params_clone, timeout).await
    });

    // -----------------------------------------------------------------------
    // Phase 2 speculative cache check: Before awaiting marine, check if
    // NOAA tides/water_temp are already cached for the marine coordinates.
    // A cache hit implies the location was previously eligible (since the
    // cache key encodes the marine coordinates and data was only cached
    // after a successful fetch for an eligible location).
    // -----------------------------------------------------------------------

    let mlat = params.marine_lat.unwrap_or(params.lat);
    let mlon = params.marine_lon.unwrap_or(params.lon);
    let primary_ck = cache_key(params.lat, params.lon);

    // For Puget Sound locations, we know the marine API won't have SST data,
    // so we can start tides/water_temperature fetches immediately in Phase 1
    // rather than waiting for the marine result to confirm sst_is_null.
    // This removes ~450ms from the critical path.
    let (tides_task, water_temp_task, speculative_tides, speculative_water_temp) =
        if PUGET_SOUND_BOX.contains(mlat, mlon) {
            // First try the speculative cache check — if both are cached, skip fetching.
            let (tides_entry, water_temp_entry) = tokio::join!(
                ddb_cache.get(&primary_ck, "tides"),
                ddb_cache.get(&primary_ck, "water_temperature"),
            );

            let tides_fresh = tides_entry.as_ref().map(|e| e.is_fresh()).unwrap_or(false);
            let wt_fresh = water_temp_entry.as_ref().map(|e| e.is_fresh()).unwrap_or(false);

            if tides_fresh && wt_fresh {
                // Parse the cached data — if parsing fails, fall back to fetching.
                let tides_parsed = tides_entry.as_ref().and_then(|entry| {
                    deserialize_tides(&entry.data).ok().map(|data| {
                        info!(source = "tides", age_secs = entry.age_secs(), "Speculative cache hit (fresh)");
                        SourceResult::Fresh(data, CacheMeta::from_entry(entry))
                    })
                });
                let wt_parsed = water_temp_entry.as_ref().and_then(|entry| {
                    deserialize_water_temperature(&entry.data).ok().map(|data| {
                        info!(source = "water_temperature", age_secs = entry.age_secs(), "Speculative cache hit (fresh)");
                        SourceResult::Fresh(data, CacheMeta::from_entry(entry))
                    })
                });

                match (tides_parsed, wt_parsed) {
                    (Some(t), Some(w)) => {
                        info!("Speculative NOAA cache hit — returning tides and water_temperature from cache without waiting for marine");
                        (None, None, Some(t), Some(w))
                    }
                    _ => {
                        info!("Speculative NOAA cache parse failed, spawning fetch tasks");
                        // Fall through to spawn fetch tasks below
                        (None, None, None, None)
                    }
                }
            } else {
                info!(
                    tides_cached = tides_fresh,
                    water_temp_cached = wt_fresh,
                    "Speculative NOAA cache miss, spawning fetch tasks immediately (Puget Sound location)"
                );
                (None, None, None, None)
            }
        } else {
            (None, None, None, None)
        };

    // If we're in Puget Sound and didn't get a speculative cache hit, spawn
    // tides/water_temp fetches now (Phase 1) rather than waiting for marine.
    let (tides_task, water_temp_task) = if PUGET_SOUND_BOX.contains(mlat, mlon)
        && speculative_tides.is_none()
        && tides_task.is_none()
    {
        if let Some(station) = nearest_puget_sound_station(mlat, mlon) {
            let now = Utc::now();
            let begin = now.format("%Y%m%d").to_string();
            let end = (now + chrono::Duration::days(7)).format("%Y%m%d").to_string();

            let client_clone = client.clone();
            let ddb_cache_clone = Arc::clone(&ddb_cache);
            let sid = station.id.to_string();
            let sname = station.name.to_string();
            let begin_clone = begin.clone();
            let end_clone = end.clone();
            let primary_ck_clone = primary_ck.clone();
            let tides = tokio::spawn(async move {
                fetch_tides(
                    &client_clone,
                    ddb_cache_clone.as_ref(),
                    &primary_ck_clone,
                    &sid,
                    &sname,
                    &begin_clone,
                    &end_clone,
                    timeout,
                )
                .await
            });

            let nearby = nearby_puget_sound_stations(mlat, mlon);
            let station_pairs: Vec<(String, String)> = nearby
                .iter()
                .map(|s| (s.id.to_string(), s.name.to_string()))
                .collect();
            let client_clone = client.clone();
            let ddb_cache_clone = Arc::clone(&ddb_cache);
            let primary_ck_clone = primary_ck.clone();
            let water_temp = tokio::spawn(async move {
                let pairs: Vec<(&str, &str)> = station_pairs
                    .iter()
                    .map(|(id, name)| (id.as_str(), name.as_str()))
                    .collect();
                fetch_water_temperature(
                    &client_clone,
                    ddb_cache_clone.as_ref(),
                    &primary_ck_clone,
                    &pairs,
                    timeout,
                )
                .await
            });

            (Some(tides), Some(water_temp))
        } else {
            (tides_task, water_temp_task)
        }
    } else {
        (tides_task, water_temp_task)
    };

    // CIOPS SST (Salish Sea) — spawn immediately based on geography alone.
    // For Salish Sea locations, the marine API never has SST data, so we don't
    // need to wait for the marine result to confirm sst_is_null.
    let ciops_task = if SALISH_SEA_BOX.contains(mlat, mlon) {
        let client_clone = client.clone();
        Some(tokio::spawn(async move {
            fetch_ciops_sst(&client_clone, mlat, mlon, timeout).await
        }))
    } else {
        None
    };

    // Await marine — we still need its result for the response data.
    info!("Awaiting marine result for Phase 2 decisions");
    let marine_await_start = std::time::Instant::now();
    let marine_result = marine_task
        .await
        .unwrap_or_else(|e| SourceResult::Failed(format!("marine task panicked: {e}")));
    info!(elapsed_ms = marine_await_start.elapsed().as_millis(), "Marine result received");

    // -----------------------------------------------------------------------
    // All conditional sources (tides, water_temp, CIOPS SST) are now spawned
    // in Phase 1 based on geography alone. No Phase 2 decisions needed.
    // -----------------------------------------------------------------------

    info!(
        in_puget_sound = PUGET_SOUND_BOX.contains(mlat, mlon),
        in_salish_sea = SALISH_SEA_BOX.contains(mlat, mlon),
        speculative_hit = speculative_tides.is_some(),
        "Phase 1 complete"
    );

    // -----------------------------------------------------------------------
    // Await all remaining tasks
    // -----------------------------------------------------------------------

    info!("Awaiting remaining Phase 1 tasks");
    let phase1_await_start = std::time::Instant::now();

    let ensemble_result = ensemble_task
        .await
        .unwrap_or_else(|e| SourceResult::Failed(format!("ensemble task panicked: {e}")));
    info!(elapsed_ms = phase1_await_start.elapsed().as_millis(), "Ensemble complete");

    let hrrr_result = hrrr_task
        .await
        .unwrap_or_else(|e| SourceResult::Failed(format!("hrrr task panicked: {e}")));
    info!(elapsed_ms = phase1_await_start.elapsed().as_millis(), "HRRR complete");

    let uv_result = uv_task
        .await
        .unwrap_or_else(|e| SourceResult::Failed(format!("uv task panicked: {e}")));
    info!(elapsed_ms = phase1_await_start.elapsed().as_millis(), "UV complete");

    let air_quality_result = air_quality_task
        .await
        .unwrap_or_else(|e| SourceResult::Failed(format!("air_quality task panicked: {e}")));
    info!(elapsed_ms = phase1_await_start.elapsed().as_millis(), "Air quality complete");

    let observations_result = observations_task
        .await
        .unwrap_or_else(|e| SourceResult::Failed(format!("observations task panicked: {e}")));
    info!(elapsed_ms = phase1_await_start.elapsed().as_millis(), "Observations complete");

    let tides_result = match (speculative_tides, tides_task) {
        (Some(result), _) => result,
        (None, Some(task)) => task
            .await
            .unwrap_or_else(|e| SourceResult::Failed(format!("tides task panicked: {e}"))),
        (None, None) => SourceResult::Skipped,
    };

    let water_temp_result = match (speculative_water_temp, water_temp_task) {
        (Some(result), _) => result,
        (None, Some(task)) => task
            .await
            .unwrap_or_else(|e| SourceResult::Failed(format!("water_temp task panicked: {e}"))),
        (None, None) => SourceResult::Skipped,
    };

    let ciops_result = match ciops_task {
        Some(task) => task
            .await
            .unwrap_or_else(|e| SourceResult::Failed(format!("ciops task panicked: {e}"))),
        None => SourceResult::Skipped,
    };

    info!(total_orchestration_ms = orchestration_start.elapsed().as_millis(), "All sources complete, returning results");

    AllSourceResults {
        ensemble: ensemble_result,
        marine: marine_result,
        hrrr: hrrr_result,
        uv: uv_result,
        air_quality: air_quality_result,
        observations: observations_result,
        tides: tides_result,
        water_temperature: water_temp_result,
        ciops_sst: ciops_result,
    }
}
