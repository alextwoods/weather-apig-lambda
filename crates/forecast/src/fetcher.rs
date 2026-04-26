use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use serde::Serialize;
use tokio::task::JoinSet;

use crate::cache::{cache_key, CacheEntry, CacheStore, DynamoCacheStore, S3CacheStore};
use crate::models::{
    nearest_puget_sound_station, AppState, FetchParams, PUGET_SOUND_BOX, SALISH_SEA_BOX,
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
    all_sst_null, build_marine_url, parse_marine_response, MarineData, MarineFetcher,
};
use crate::sources::noaa_tides::{build_tides_url, parse_tides_response, TidesData};
use crate::sources::noaa_water_temp::{
    build_water_temp_url, parse_water_temp_response, WaterTemperatureData,
};
use crate::sources::observations::{
    build_observation_url, build_station_discovery_url, filter_observations_to_recent,
    parse_observations, parse_station_discovery, ObservationData,
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
        None
    } else {
        cache.get(ck, source_id).await
    };

    if let Some(ref entry) = cached {
        if entry.is_fresh() {
            // Cache hit — parse and return
            match parse_fn(&entry.data) {
                Ok(data) => return SourceResult::Fresh(data, CacheMeta::from_entry(entry)),
                Err(_) => {
                    // Cached data is corrupt — treat as cache miss and fetch upstream
                }
            }
        }
    }

    // Step 2: Fetch upstream
    match fetch_fn.await {
        Ok(raw) => {
            // Parse the response
            match parse_fn(&raw) {
                Ok(data) => {
                    // Update cache (fire-and-forget; errors are non-fatal)
                    let _ = cache.put(ck, source_id, &raw, ttl_secs).await;
                    SourceResult::Refreshed(data, CacheMeta::fresh_now())
                }
                Err(e) => {
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
            // HTTP 420 — use cached data if available
            if let Some(ref entry) = cached {
                if let Ok(data) = parse_fn(&entry.data) {
                    return SourceResult::Throttled(data, CacheMeta::from_entry(entry));
                }
            }
            SourceResult::Failed("upstream throttled (HTTP 420), no cached data available".into())
        }
        Err(err) => {
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
// fetch_without_cache — for non-cacheable sources
// ---------------------------------------------------------------------------

/// Fetches data for a non-cacheable source (observations, NOAA, CIOPS).
/// Returns `Refreshed` on success or `Failed` on error.
async fn fetch_without_cache<T, F, Fut>(
    parse_fn: F,
    fetch_fn: Fut,
) -> SourceResult<T>
where
    T: Clone,
    F: Fn(&[u8]) -> Result<T, String>,
    Fut: std::future::Future<Output = Result<Vec<u8>, UpstreamError>>,
{
    match fetch_fn.await {
        Ok(raw) => match parse_fn(&raw) {
            Ok(data) => SourceResult::Refreshed(data, CacheMeta::fresh_now()),
            Err(e) => SourceResult::Failed(format!("parse error: {e}")),
        },
        Err(err) => SourceResult::Failed(err.to_string()),
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

/// Fetch ensemble data with cache support.
async fn fetch_ensemble(
    client: &reqwest::Client,
    cache: &dyn CacheStore,
    params: &FetchParams,
    timeout: Duration,
) -> SourceResult<ParsedEnsembleData> {
    let ck = cache_key(params.lat, params.lon);
    let url = build_ensemble_url(params.lat, params.lon);
    let force = should_force_refresh(params, EnsembleFetcher::source_id());

    fetch_with_cache(
        cache,
        &ck,
        EnsembleFetcher::source_id(),
        EnsembleFetcher::ttl_secs(),
        force,
        |raw| parse_ensemble_response(raw),
        http_get(client, &url, timeout),
    )
    .await
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
    let ck = cache_key(mlat, mlon);
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

/// Fetch NWS observations (not cached).
///
/// If a `station_id` is provided, fetches directly from that station.
/// Otherwise, discovers the nearest station via the NWS points API.
async fn fetch_observations(
    client: &reqwest::Client,
    params: &FetchParams,
    timeout: Duration,
) -> SourceResult<ObservationData> {
    let now = Utc::now();

    // Step 1: Determine the station to fetch from
    let station_info = if let Some(ref sid) = params.station_id {
        // Use the provided station ID — we don't have full metadata, so
        // construct a minimal StationInfo. The observation response will
        // fill in details.
        crate::sources::observations::StationInfo {
            id: sid.clone(),
            name: String::new(),
            latitude: params.lat,
            longitude: params.lon,
            distance_km: 0.0,
        }
    } else {
        // Discover the nearest station
        let discovery_url = build_station_discovery_url(params.lat, params.lon);
        let raw = match http_get_nws(client, &discovery_url, timeout).await {
            Ok(r) => r,
            Err(e) => return SourceResult::Failed(format!("station discovery failed: {e}")),
        };
        match parse_station_discovery(&raw, params.lat, params.lon) {
            Ok(info) => info,
            Err(e) => return SourceResult::Failed(format!("station discovery parse error: {e}")),
        }
    };

    // Step 2: Fetch observations from the station
    let obs_url = build_observation_url(&station_info.id);
    let raw = match http_get_nws(client, &obs_url, timeout).await {
        Ok(r) => r,
        Err(e) => return SourceResult::Failed(format!("observation fetch failed: {e}")),
    };

    let entries = match parse_observations(&raw) {
        Ok(e) => e,
        Err(e) => return SourceResult::Failed(format!("observation parse error: {e}")),
    };

    // Step 3: Filter to recent 12 hours
    let filtered = filter_observations_to_recent(entries, now);

    let data = ObservationData {
        station: station_info,
        entries: filtered,
    };

    SourceResult::Refreshed(data, CacheMeta::fresh_now())
}

/// Fetch NOAA water temperature (not cached).
async fn fetch_water_temperature(
    client: &reqwest::Client,
    station_id: &str,
    station_name: &str,
    timeout: Duration,
) -> SourceResult<WaterTemperatureData> {
    let url = build_water_temp_url(station_id);
    fetch_without_cache(
        |raw| parse_water_temp_response(raw, station_id, station_name),
        http_get(client, &url, timeout),
    )
    .await
}

/// Fetch NOAA tide predictions (not cached).
async fn fetch_tides(
    client: &reqwest::Client,
    station_id: &str,
    station_name: &str,
    begin_date: &str,
    end_date: &str,
    timeout: Duration,
) -> SourceResult<TidesData> {
    let url = build_tides_url(station_id, begin_date, end_date);
    fetch_without_cache(
        |raw| parse_tides_response(raw, station_id, station_name),
        http_get(client, &url, timeout),
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
pub async fn fetch_all_sources(state: &AppState, params: &FetchParams) -> AllSourceResults {
    let timeout = Duration::from_secs(state.config.default_timeout_secs);
    let client = &state.http_client;

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
    let ensemble_task = tokio::spawn(async move {
        fetch_ensemble(&client_clone, s3_cache_clone.as_ref(), &params_clone, timeout).await
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
    let params_clone = params.clone();
    let observations_task = tokio::spawn(async move {
        fetch_observations(&client_clone, &params_clone, timeout).await
    });

    // Await marine first — we need its result for Phase 2 decisions
    let marine_result = marine_task
        .await
        .unwrap_or_else(|e| SourceResult::Failed(format!("marine task panicked: {e}")));

    // -----------------------------------------------------------------------
    // Phase 2: Conditional sources based on marine result
    // -----------------------------------------------------------------------

    let mlat = params.marine_lat.unwrap_or(params.lat);
    let mlon = params.marine_lon.unwrap_or(params.lon);

    let sst_is_null = marine_result
        .data()
        .map(|d| all_sst_null(d))
        .unwrap_or(false);

    // NOAA tides + water temperature (Puget Sound)
    let (tides_task, water_temp_task) = if sst_is_null && PUGET_SOUND_BOX.contains(mlat, mlon) {
        if let Some(station) = nearest_puget_sound_station(mlat, mlon) {
            // Compute tide date range from marine times (or use a default 7-day window)
            let now = Utc::now();
            let begin = now.format("%Y%m%d").to_string();
            let end = (now + chrono::Duration::days(7)).format("%Y%m%d").to_string();

            let client_clone = client.clone();
            let sid = station.id.to_string();
            let sname = station.name.to_string();
            let begin_clone = begin.clone();
            let end_clone = end.clone();
            let tides = tokio::spawn(async move {
                fetch_tides(&client_clone, &sid, &sname, &begin_clone, &end_clone, timeout).await
            });

            let client_clone = client.clone();
            let sid = station.id.to_string();
            let sname = station.name.to_string();
            let water_temp = tokio::spawn(async move {
                fetch_water_temperature(&client_clone, &sid, &sname, timeout).await
            });

            (Some(tides), Some(water_temp))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    // CIOPS SST (Salish Sea)
    let ciops_task = if sst_is_null && SALISH_SEA_BOX.contains(mlat, mlon) {
        let client_clone = client.clone();
        Some(tokio::spawn(async move {
            fetch_ciops_sst(&client_clone, mlat, mlon, timeout).await
        }))
    } else {
        None
    };

    // -----------------------------------------------------------------------
    // Await all remaining tasks
    // -----------------------------------------------------------------------

    let ensemble_result = ensemble_task
        .await
        .unwrap_or_else(|e| SourceResult::Failed(format!("ensemble task panicked: {e}")));

    let hrrr_result = hrrr_task
        .await
        .unwrap_or_else(|e| SourceResult::Failed(format!("hrrr task panicked: {e}")));

    let uv_result = uv_task
        .await
        .unwrap_or_else(|e| SourceResult::Failed(format!("uv task panicked: {e}")));

    let air_quality_result = air_quality_task
        .await
        .unwrap_or_else(|e| SourceResult::Failed(format!("air_quality task panicked: {e}")));

    let observations_result = observations_task
        .await
        .unwrap_or_else(|e| SourceResult::Failed(format!("observations task panicked: {e}")));

    let tides_result = match tides_task {
        Some(task) => task
            .await
            .unwrap_or_else(|e| SourceResult::Failed(format!("tides task panicked: {e}"))),
        None => SourceResult::Skipped,
    };

    let water_temp_result = match water_temp_task {
        Some(task) => task
            .await
            .unwrap_or_else(|e| SourceResult::Failed(format!("water_temp task panicked: {e}"))),
        None => SourceResult::Skipped,
    };

    let ciops_result = match ciops_task {
        Some(task) => task
            .await
            .unwrap_or_else(|e| SourceResult::Failed(format!("ciops task panicked: {e}"))),
        None => SourceResult::Skipped,
    };

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
