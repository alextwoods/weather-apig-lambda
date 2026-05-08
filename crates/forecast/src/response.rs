use std::collections::HashMap;

use chrono::{DateTime, NaiveDateTime, Utc};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::compute::aggregation::{compute_daily_sections, DailySection};
use crate::compute::astronomy::{moon_altitude, sun_altitude};
use crate::compute::percentile::{compute_percentiles, PercentileStats};
use crate::compute::probability::{compute_precip_probability, PrecipProbability};
use crate::fetcher::{AllSourceResults, CacheMeta, SourceResult};
use crate::models::{FetchParams, WEATHER_VARIABLES};
use crate::sources::ensemble::{extract_members, ParsedEnsembleData};

// ---------------------------------------------------------------------------
// Response structs — serialized to JSON for the client
// ---------------------------------------------------------------------------

/// Top-level forecast response containing all source sections, cache metadata,
/// and per-source error messages.
#[derive(Debug, Serialize)]
pub struct ForecastResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ensemble: Option<EnsembleResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub marine: Option<MarineResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hrrr: Option<HrrrResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uv: Option<UvResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub air_quality: Option<AirQualityResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observations: Option<ObservationsResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tides: Option<TidesResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub water_temperature: Option<WaterTemperatureResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ciops_sst: Option<CiopsSstResponse>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub astronomy: Option<AstronomyResponse>,
    pub cache: HashMap<String, CacheMetadata>,
    pub errors: HashMap<String, Option<String>>,
}

/// Ensemble forecast section with percentile statistics, precipitation
/// probability, and daily aggregations.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EnsembleResponse {
    pub times: Vec<String>,
    pub statistics: HashMap<String, PercentileStatsResponse>,
    pub precipitation_probability: PrecipProbabilityResponse,
    pub daily_sections: Vec<DailySectionResponse>,
}

/// Percentile statistics for a single weather variable.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PercentileStatsResponse {
    pub p10: Vec<Option<f64>>,
    pub p25: Vec<Option<f64>>,
    pub median: Vec<Option<f64>>,
    pub p75: Vec<Option<f64>>,
    pub p90: Vec<Option<f64>>,
}

impl From<PercentileStats> for PercentileStatsResponse {
    fn from(stats: PercentileStats) -> Self {
        Self {
            p10: stats.p10,
            p25: stats.p25,
            median: stats.median,
            p75: stats.p75,
            p90: stats.p90,
        }
    }
}

/// Precipitation probability at three thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrecipProbabilityResponse {
    pub any: Vec<Option<f64>>,
    pub moderate: Vec<Option<f64>>,
    pub heavy: Vec<Option<f64>>,
}

impl From<PrecipProbability> for PrecipProbabilityResponse {
    fn from(prob: PrecipProbability) -> Self {
        Self {
            any: prob.any,
            moderate: prob.moderate,
            heavy: prob.heavy,
        }
    }
}

/// Daily aggregation section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DailySectionResponse {
    pub date: String,
    pub start_index: usize,
    pub end_index: usize,
    pub high_temp: Option<f64>,
    pub low_temp: Option<f64>,
    pub total_precip: Option<f64>,
    pub max_wind: Option<f64>,
    pub dominant_wind_direction: Option<String>,
}

impl From<DailySection> for DailySectionResponse {
    fn from(section: DailySection) -> Self {
        Self {
            date: section.date,
            start_index: section.start_index,
            end_index: section.end_index,
            high_temp: section.high_temp,
            low_temp: section.low_temp,
            total_precip: section.total_precip,
            max_wind: section.max_wind,
            dominant_wind_direction: section.dominant_wind_direction,
        }
    }
}

/// Marine forecast section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MarineResponse {
    pub times: Vec<String>,
    pub wave_height: Vec<Option<f64>>,
    pub wave_period: Vec<Option<f64>>,
    pub wave_direction: Vec<Option<f64>>,
    pub sea_surface_temperature: Vec<Option<f64>>,
}

/// HRRR (High-Resolution Rapid Refresh) forecast section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HrrrResponse {
    pub times: Vec<String>,
    pub temperature_2m: Vec<Option<f64>>,
    pub apparent_temperature: Vec<Option<f64>>,
    pub dew_point_2m: Vec<Option<f64>>,
    pub wind_speed_10m: Vec<Option<f64>>,
    pub wind_gusts_10m: Vec<Option<f64>>,
    pub wind_direction_10m: Vec<Option<f64>>,
    pub surface_pressure: Vec<Option<f64>>,
    pub precipitation: Vec<Option<f64>>,
    pub precipitation_probability: Vec<Option<f64>>,
}

/// UV index forecast section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UvResponse {
    pub times: Vec<String>,
    pub uv_index: Vec<Option<f64>>,
    pub uv_index_clear_sky: Vec<Option<f64>>,
}

/// Air quality forecast section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AirQualityResponse {
    pub times: Vec<String>,
    pub us_aqi: Vec<Option<f64>>,
    pub pm2_5: Vec<Option<f64>>,
    pub pm10: Vec<Option<f64>>,
}

/// Station metadata included in observation and tide responses.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StationResponse {
    pub id: String,
    pub name: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub distance_km: Option<f64>,
}

/// Observation entry in the response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservationEntryResponse {
    pub timestamp: String,
    pub temperature_celsius: Option<f64>,
    pub wind_speed_kmh: Option<f64>,
    pub wind_direction_degrees: Option<f64>,
    pub pressure_hpa: Option<f64>,
}

/// NWS observations section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservationsResponse {
    pub station: StationResponse,
    pub entries: Vec<ObservationEntryResponse>,
}

/// Tide prediction entry in the response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TidePredictionResponse {
    pub time: String,
    pub height_m: f64,
}

/// NOAA tide predictions section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TidesResponse {
    pub station: StationResponse,
    pub predictions: Vec<TidePredictionResponse>,
}

/// NOAA water temperature section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WaterTemperatureResponse {
    pub station: StationResponse,
    pub temperature_celsius: Option<f64>,
    pub timestamp: Option<String>,
}

/// ECCC CIOPS Salish Sea SST section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CiopsSstResponse {
    pub times: Vec<String>,
    pub temperatures_celsius: Vec<Option<f64>>,
}

/// Sun and moon altitude section.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AstronomyResponse {
    pub times: Vec<String>,
    pub sun_altitude: Vec<f64>,
    pub moon_altitude: Vec<f64>,
}

/// Response from `GET /forecast/members` containing raw ensemble member data
/// for a single weather variable, grouped by model, with percentile statistics.
#[derive(Debug, Serialize)]
pub struct MembersResponse {
    pub times: Vec<String>,
    pub variable: String,
    pub members_by_model: HashMap<String, Vec<Vec<Option<f64>>>>,
    pub statistics: PercentileStatsResponse,
}

/// Cache metadata for a single source, included in the response so clients
/// can display data freshness information.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CacheMetadata {
    pub age_seconds: u64,
    pub is_fresh: bool,
    pub fetched_at: String,
}

impl From<&CacheMeta> for CacheMetadata {
    fn from(meta: &CacheMeta) -> Self {
        Self {
            age_seconds: meta.age_seconds,
            is_fresh: meta.is_fresh,
            fetched_at: meta.fetched_at.clone(),
        }
    }
}


// ---------------------------------------------------------------------------
// Helper: parse an ISO 8601 time string into a UTC DateTime
// ---------------------------------------------------------------------------

/// Parses an Open-Meteo style time string (`YYYY-MM-DDTHH:MM`) into a UTC
/// `DateTime`. Returns `None` if the string cannot be parsed.
fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M")
        .ok()
        .map(|ndt| ndt.and_utc())
}

// ---------------------------------------------------------------------------
// Helper: compute the truncation cutoff for a given forecast horizon
// ---------------------------------------------------------------------------

/// Computes the number of time steps to keep for a given forecast horizon.
///
/// The cutoff is `min(times_len, 12 + forecast_days * 24)` — 12 past hours
/// plus the requested number of forecast days in hourly steps. If the data
/// has fewer time steps than the computed cutoff, the full data is kept.
fn truncate_to_horizon(times_len: usize, forecast_days: u32) -> usize {
    let cutoff = 12 + (forecast_days as usize) * 24;
    times_len.min(cutoff)
}

/// Truncates a `Vec<T>` to at most `len` elements.
fn truncate_vec<T: Clone>(v: &[T], len: usize) -> Vec<T> {
    v[..v.len().min(len)].to_vec()
}

/// Finds the cutoff index for a secondary source (HRRR, UV, air quality,
/// marine, CIOPS SST) based on the ensemble's last retained time.
///
/// Scans the secondary source's times array and returns the index of the
/// first entry that is strictly after `cutoff_time`, or the full length if
/// all entries are within range. This ensures secondary sources don't extend
/// beyond the ensemble's truncated horizon.
fn secondary_cutoff(times: &[String], cutoff_time: &DateTime<Utc>) -> usize {
    times
        .iter()
        .position(|t| {
            parse_time(t)
                .map(|dt| dt > *cutoff_time)
                .unwrap_or(false)
        })
        .unwrap_or(times.len())
}

// ---------------------------------------------------------------------------
// Helper: collect cache metadata from a SourceResult
// ---------------------------------------------------------------------------

/// Extracts cache metadata from a `SourceResult` if available.
fn extract_cache_meta<T>(result: &SourceResult<T>) -> Option<CacheMetadata> {
    result.cache_meta().map(CacheMetadata::from)
}

/// Extracts the error message from a `SourceResult` if it represents a
/// failure or stale fallback.
fn extract_error<T>(result: &SourceResult<T>) -> Option<String> {
    result.error_message().map(|s| s.to_string())
}

// ---------------------------------------------------------------------------
// build_response — assembles the complete forecast JSON response
// ---------------------------------------------------------------------------

/// Assembles the complete forecast JSON response from all source results and
/// computed data.
///
/// This function:
/// 1. Extracts ensemble members for each of the 11 weather variables
/// 2. Computes percentile statistics for each variable using pooled members
/// 3. Computes precipitation probability from precipitation members
/// 4. Computes daily sections from the median arrays
/// 5. Computes sun and moon altitude for each ensemble time step
/// 6. Assembles all source data into the response structure
/// 7. Includes cache metadata and per-source error messages
pub fn build_response(results: AllSourceResults, params: &FetchParams) -> ForecastResponse {
    // -------------------------------------------------------------------
    // Compute the ensemble truncation cutoff based on forecast_days.
    // This is applied BEFORE computing statistics to save computation.
    // -------------------------------------------------------------------
    let ensemble_cutoff = results.ensemble.data().map(|data| {
        truncate_to_horizon(data.times.len(), params.forecast_days)
    });

    // The last ensemble time after truncation, used to truncate secondary
    // sources (HRRR, UV, air quality, marine, CIOPS SST) so they don't
    // extend beyond the ensemble's horizon.
    let ensemble_cutoff_time: Option<DateTime<Utc>> = results
        .ensemble
        .data()
        .and_then(|data| {
            let cutoff = truncate_to_horizon(data.times.len(), params.forecast_days);
            if cutoff > 0 {
                data.times.get(cutoff - 1).and_then(|t| parse_time(t))
            } else {
                None
            }
        });

    // -------------------------------------------------------------------
    // Ensemble processing + concurrent astronomy computation
    // -------------------------------------------------------------------

    // If ensemble data is available, kick off the astronomy computation on
    // a separate thread *before* computing statistics. The astronomy work
    // (trig for every time step) is independent of the statistics and can
    // run concurrently using Rayon's par_iter inside the spawned thread.
    // Truncation is applied to the times before spawning the thread.
    let astronomy_handle = results.ensemble.data().map(|data| {
        let cutoff = ensemble_cutoff.unwrap_or(data.times.len());
        let times = truncate_vec(&data.times, cutoff);
        let lat = params.lat;
        let lon = params.lon;

        std::thread::spawn(move || {
            let results: Vec<(f64, f64)> = times
                .par_iter()
                .map(|time_str| {
                    if let Some(dt) = parse_time(time_str) {
                        (sun_altitude(dt, lat, lon), moon_altitude(dt, lat, lon))
                    } else {
                        (0.0, 0.0)
                    }
                })
                .collect();

            let (sun_alts, moon_alts): (Vec<f64>, Vec<f64>) = results.into_iter().unzip();

            AstronomyResponse {
                times,
                sun_altitude: sun_alts,
                moon_altitude: moon_alts,
            }
        })
    });

    let ensemble = results.ensemble.data().map(|data| {
        // Truncate ensemble data to the requested forecast horizon BEFORE
        // computing statistics. This saves computation proportional to the
        // reduction in time steps (e.g. 10 days vs 35 days = ~70% less).
        let cutoff = ensemble_cutoff.unwrap_or(data.times.len());
        let truncated_times = truncate_vec(&data.times, cutoff);
        let time_step_count = truncated_times.len();

        // Truncate member arrays before extraction by truncating the
        // underlying hourly HashMap values.
        let truncated_hourly: HashMap<String, Vec<Option<f64>>> = data
            .hourly
            .iter()
            .map(|(k, v)| (k.clone(), truncate_vec(v, cutoff)))
            .collect();

        // Parallel extraction and percentile computation across all 11 weather
        // variables using Rayon. Each variable is independent so this is safe
        // to parallelise.
        let stats_vec: Vec<(String, PercentileStatsResponse, Vec<Option<f64>>, Vec<Vec<Option<f64>>>)> =
            WEATHER_VARIABLES
                .par_iter()
                .map(|variable| {
                    let extracted = extract_members(&truncated_hourly, variable);
                    let stats = compute_percentiles(&extracted.pooled, time_step_count);
                    let median = stats.median.clone();
                    let pooled = extracted.pooled;
                    (variable.to_string(), PercentileStatsResponse::from(stats), median, pooled)
                })
                .collect();

        // Build the statistics HashMap and extract the precipitation members
        // and median arrays needed for probability and daily section computation.
        let mut statistics: HashMap<String, PercentileStatsResponse> = HashMap::new();
        let mut precip_pooled: Vec<Vec<Option<f64>>> = Vec::new();
        let mut median_temp: Vec<Option<f64>> = Vec::new();
        let mut median_precip: Vec<Option<f64>> = Vec::new();
        let mut median_wind_speed: Vec<Option<f64>> = Vec::new();
        let mut median_wind_direction: Vec<Option<f64>> = Vec::new();

        for (variable_name, stats_response, median, pooled) in stats_vec {
            match variable_name.as_str() {
                "temperature_2m" => median_temp = median,
                "precipitation" => {
                    median_precip = median;
                    precip_pooled = pooled;
                }
                "wind_speed_10m" => median_wind_speed = median,
                "wind_direction_10m" => median_wind_direction = median,
                _ => {}
            }
            statistics.insert(variable_name, stats_response);
        }

        // Compute precipitation probability from truncated members
        let precip_prob = compute_precip_probability(&precip_pooled, time_step_count);

        // Compute daily sections from truncated median arrays
        let daily_sections = compute_daily_sections(
            &truncated_times,
            &median_temp,
            &median_precip,
            &median_wind_speed,
            &median_wind_direction,
        );

        EnsembleResponse {
            times: truncated_times,
            statistics,
            precipitation_probability: PrecipProbabilityResponse::from(precip_prob),
            daily_sections: daily_sections.into_iter().map(DailySectionResponse::from).collect(),
        }
    });

    // -------------------------------------------------------------------
    // Astronomy — join the concurrent computation started above
    // -------------------------------------------------------------------
    let astronomy = astronomy_handle.map(|handle| {
        handle.join().expect("astronomy thread panicked")
    });

    // -------------------------------------------------------------------
    // Marine — truncate to ensemble cutoff time
    // -------------------------------------------------------------------
    let marine = results.marine.data().map(|data| {
        let cutoff = ensemble_cutoff_time
            .map(|ct| secondary_cutoff(&data.times, &ct))
            .unwrap_or(data.times.len());
        MarineResponse {
            times: truncate_vec(&data.times, cutoff),
            wave_height: truncate_vec(&data.wave_height, cutoff),
            wave_period: truncate_vec(&data.wave_period, cutoff),
            wave_direction: truncate_vec(&data.wave_direction, cutoff),
            sea_surface_temperature: truncate_vec(&data.sea_surface_temperature, cutoff),
        }
    });

    // -------------------------------------------------------------------
    // HRRR — truncate to ensemble cutoff time
    // -------------------------------------------------------------------
    let hrrr = results.hrrr.data().map(|data| {
        let cutoff = ensemble_cutoff_time
            .map(|ct| secondary_cutoff(&data.times, &ct))
            .unwrap_or(data.times.len());
        HrrrResponse {
            times: truncate_vec(&data.times, cutoff),
            temperature_2m: truncate_vec(&data.temperature_2m, cutoff),
            apparent_temperature: truncate_vec(&data.apparent_temperature, cutoff),
            dew_point_2m: truncate_vec(&data.dew_point_2m, cutoff),
            wind_speed_10m: truncate_vec(&data.wind_speed_10m, cutoff),
            wind_gusts_10m: truncate_vec(&data.wind_gusts_10m, cutoff),
            wind_direction_10m: truncate_vec(&data.wind_direction_10m, cutoff),
            surface_pressure: truncate_vec(&data.surface_pressure, cutoff),
            precipitation: truncate_vec(&data.precipitation, cutoff),
            precipitation_probability: truncate_vec(&data.precipitation_probability, cutoff),
        }
    });

    // -------------------------------------------------------------------
    // UV — truncate to ensemble cutoff time
    // -------------------------------------------------------------------
    let uv = results.uv.data().map(|data| {
        let cutoff = ensemble_cutoff_time
            .map(|ct| secondary_cutoff(&data.times, &ct))
            .unwrap_or(data.times.len());
        UvResponse {
            times: truncate_vec(&data.times, cutoff),
            uv_index: truncate_vec(&data.uv_index, cutoff),
            uv_index_clear_sky: truncate_vec(&data.uv_index_clear_sky, cutoff),
        }
    });

    // -------------------------------------------------------------------
    // Air quality — truncate to ensemble cutoff time
    // -------------------------------------------------------------------
    let air_quality = results.air_quality.data().map(|data| {
        let cutoff = ensemble_cutoff_time
            .map(|ct| secondary_cutoff(&data.times, &ct))
            .unwrap_or(data.times.len());
        AirQualityResponse {
            times: truncate_vec(&data.times, cutoff),
            us_aqi: truncate_vec(&data.us_aqi, cutoff),
            pm2_5: truncate_vec(&data.pm2_5, cutoff),
            pm10: truncate_vec(&data.pm10, cutoff),
        }
    });

    // -------------------------------------------------------------------
    // Observations
    // -------------------------------------------------------------------
    let observations = results.observations.data().map(|data| ObservationsResponse {
        station: StationResponse {
            id: data.station.id.clone(),
            name: data.station.name.clone(),
            latitude: Some(data.station.latitude),
            longitude: Some(data.station.longitude),
            distance_km: Some(data.station.distance_km),
        },
        entries: data
            .entries
            .iter()
            .map(|e| ObservationEntryResponse {
                timestamp: e.timestamp.clone(),
                temperature_celsius: e.temperature_celsius,
                wind_speed_kmh: e.wind_speed_kmh,
                wind_direction_degrees: e.wind_direction_degrees,
                pressure_hpa: e.pressure_hpa,
            })
            .collect(),
    });

    // -------------------------------------------------------------------
    // Tides
    // -------------------------------------------------------------------
    let tides = results.tides.data().map(|data| TidesResponse {
        station: StationResponse {
            id: data.station_id.clone(),
            name: data.station_name.clone(),
            latitude: None,
            longitude: None,
            distance_km: None,
        },
        predictions: data
            .predictions
            .iter()
            .map(|p| TidePredictionResponse {
                time: p.time.clone(),
                height_m: p.height_m,
            })
            .collect(),
    });

    // -------------------------------------------------------------------
    // Water temperature
    // -------------------------------------------------------------------
    let water_temperature = results.water_temperature.data().map(|data| {
        WaterTemperatureResponse {
            station: StationResponse {
                id: data.station_id.clone(),
                name: data.station_name.clone(),
                latitude: None,
                longitude: None,
                distance_km: None,
            },
            temperature_celsius: data.temperature_celsius,
            timestamp: data.timestamp.clone(),
        }
    });

    // -------------------------------------------------------------------
    // CIOPS SST — truncate to ensemble cutoff time
    // -------------------------------------------------------------------
    let ciops_sst = results.ciops_sst.data().map(|data| {
        // CIOPS times use a different format (ISO 8601 with timezone offset),
        // so we parse with the standard DateTime parser rather than parse_time.
        let cutoff = ensemble_cutoff_time
            .map(|ct| {
                data.times
                    .iter()
                    .position(|t| {
                        DateTime::parse_from_rfc3339(t)
                            .or_else(|_| DateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S%:z"))
                            .map(|dt| dt.with_timezone(&Utc) > ct)
                            .unwrap_or(false)
                    })
                    .unwrap_or(data.times.len())
            })
            .unwrap_or(data.times.len());
        CiopsSstResponse {
            times: truncate_vec(&data.times, cutoff),
            temperatures_celsius: truncate_vec(&data.temperatures_celsius, cutoff),
        }
    });

    // -------------------------------------------------------------------
    // Cache metadata
    // -------------------------------------------------------------------
    let mut cache = HashMap::new();
    if let Some(meta) = extract_cache_meta(&results.ensemble) {
        cache.insert("ensemble".to_string(), meta);
    }
    if let Some(meta) = extract_cache_meta(&results.marine) {
        cache.insert("marine".to_string(), meta);
    }
    if let Some(meta) = extract_cache_meta(&results.hrrr) {
        cache.insert("hrrr".to_string(), meta);
    }
    if let Some(meta) = extract_cache_meta(&results.uv) {
        cache.insert("uv".to_string(), meta);
    }
    if let Some(meta) = extract_cache_meta(&results.air_quality) {
        cache.insert("air_quality".to_string(), meta);
    }

    // -------------------------------------------------------------------
    // Per-source errors
    // -------------------------------------------------------------------
    let mut errors = HashMap::new();
    errors.insert("ensemble".to_string(), extract_error(&results.ensemble));
    errors.insert("marine".to_string(), extract_error(&results.marine));
    errors.insert("hrrr".to_string(), extract_error(&results.hrrr));
    errors.insert("uv".to_string(), extract_error(&results.uv));
    errors.insert("air_quality".to_string(), extract_error(&results.air_quality));
    errors.insert("observations".to_string(), extract_error(&results.observations));
    errors.insert("tides".to_string(), extract_error(&results.tides));
    errors.insert(
        "water_temperature".to_string(),
        extract_error(&results.water_temperature),
    );
    errors.insert("ciops_sst".to_string(), extract_error(&results.ciops_sst));

    ForecastResponse {
        ensemble,
        marine,
        hrrr,
        uv,
        air_quality,
        observations,
        tides,
        water_temperature,
        ciops_sst,
        astronomy,
        cache,
        errors,
    }
}

// ---------------------------------------------------------------------------
// build_members_response — assembles the /forecast/members JSON response
// ---------------------------------------------------------------------------

/// Assembles the members endpoint response for a single weather variable.
///
/// Extracts ensemble members for the given variable from the parsed data,
/// filters to only the selected model suffixes, computes percentile statistics
/// from the selected models' pooled members, and returns the response.
pub fn build_members_response(
    data: &ParsedEnsembleData,
    variable: &str,
    selected_model_suffixes: &[&str],
    forecast_days: u32,
) -> MembersResponse {
    let extracted = extract_members(&data.hourly, variable);

    // Truncate to the requested forecast horizon
    let cutoff = truncate_to_horizon(data.times.len(), forecast_days);
    let truncated_times = truncate_vec(&data.times, cutoff);
    let time_step_count = truncated_times.len();

    // Filter by_model to only include the selected model suffixes,
    // and truncate each member array to the cutoff
    let filtered_by_model: HashMap<String, Vec<Vec<Option<f64>>>> = extracted
        .by_model
        .into_iter()
        .filter(|(suffix, _)| selected_model_suffixes.contains(&suffix.as_str()))
        .map(|(suffix, members)| {
            let truncated_members: Vec<Vec<Option<f64>>> = members
                .into_iter()
                .map(|m| truncate_vec(&m, cutoff))
                .collect();
            (suffix, truncated_members)
        })
        .collect();

    // Pool members from only the selected models for percentile computation
    let pooled: Vec<Vec<Option<f64>>> = filtered_by_model
        .values()
        .flat_map(|members| members.iter().cloned())
        .collect();

    let stats = compute_percentiles(&pooled, time_step_count);

    MembersResponse {
        times: truncated_times,
        variable: variable.to_string(),
        members_by_model: filtered_by_model,
        statistics: PercentileStatsResponse::from(stats),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;
    use crate::fetcher::CacheMeta;
    use crate::sources::ciops_sst::CiopsSstData;
    use crate::sources::ensemble::ParsedEnsembleData;
    use crate::sources::hrrr::HrrrData;
    use crate::sources::marine::MarineData;
    use crate::sources::noaa_tides::{TidesData, TidePrediction};
    use crate::sources::noaa_water_temp::WaterTemperatureData;
    use crate::sources::observations::{ObservationData, ObservationEntry, StationInfo};

    fn default_params() -> FetchParams {
        FetchParams {
            lat: 47.61,
            lon: -122.33,
            marine_lat: None,
            marine_lon: None,
            station_id: None,
            force_refresh: false,
            refresh_source: None,
            models: None,
            forecast_days: 10,
        }
    }

    fn fresh_cache_meta() -> CacheMeta {
        CacheMeta {
            age_seconds: 100,
            is_fresh: true,
            fetched_at: "2026-04-24T14:30:00+00:00".to_string(),
        }
    }

    fn make_ensemble_data() -> ParsedEnsembleData {
        let mut hourly = HashMap::new();
        // Create 2 members for temperature_2m from 2 models, 3 time steps
        hourly.insert(
            "temperature_2m_member00_ecmwf".to_string(),
            vec![Some(10.0), Some(12.0), Some(14.0)],
        );
        hourly.insert(
            "temperature_2m_member01_ecmwf".to_string(),
            vec![Some(11.0), Some(13.0), Some(15.0)],
        );
        // Precipitation members
        hourly.insert(
            "precipitation_member00_ecmwf".to_string(),
            vec![Some(0.0), Some(0.5), Some(3.0)],
        );
        hourly.insert(
            "precipitation_member01_ecmwf".to_string(),
            vec![Some(0.2), Some(0.0), Some(8.0)],
        );
        // Wind speed members
        hourly.insert(
            "wind_speed_10m_member00_ecmwf".to_string(),
            vec![Some(10.0), Some(15.0), Some(20.0)],
        );
        // Wind direction members
        hourly.insert(
            "wind_direction_10m_member00_ecmwf".to_string(),
            vec![Some(180.0), Some(200.0), Some(220.0)],
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

    fn all_skipped_results() -> AllSourceResults {
        AllSourceResults {
            ensemble: SourceResult::Skipped,
            marine: SourceResult::Skipped,
            hrrr: SourceResult::Skipped,
            uv: SourceResult::Skipped,
            air_quality: SourceResult::Skipped,
            observations: SourceResult::Skipped,
            tides: SourceResult::Skipped,
            water_temperature: SourceResult::Skipped,
            ciops_sst: SourceResult::Skipped,
        }
    }

    #[test]
    fn test_build_response_all_skipped() {
        let results = all_skipped_results();
        let params = default_params();
        let response = build_response(results, &params);

        assert!(response.ensemble.is_none());
        assert!(response.marine.is_none());
        assert!(response.hrrr.is_none());
        assert!(response.uv.is_none());
        assert!(response.air_quality.is_none());
        assert!(response.observations.is_none());
        assert!(response.tides.is_none());
        assert!(response.water_temperature.is_none());
        assert!(response.ciops_sst.is_none());
        assert!(response.astronomy.is_none());
        assert!(response.cache.is_empty());
    }

    #[test]
    fn test_build_response_ensemble_statistics() {
        let ensemble_data = make_ensemble_data();
        let mut results = all_skipped_results();
        results.ensemble = SourceResult::Fresh(ensemble_data, fresh_cache_meta());

        let params = default_params();
        let response = build_response(results, &params);

        let ens = response.ensemble.as_ref().unwrap();
        assert_eq!(ens.times.len(), 3);

        // Check that temperature_2m statistics were computed
        let temp_stats = ens.statistics.get("temperature_2m").unwrap();
        assert_eq!(temp_stats.median.len(), 3);
        // With 2 members [10, 11] at t=0, median should be 10.5
        assert_eq!(temp_stats.median[0], Some(10.5));

        // Check precipitation probability was computed
        assert_eq!(ens.precipitation_probability.any.len(), 3);
    }

    #[test]
    fn test_build_response_ensemble_daily_sections() {
        let ensemble_data = make_ensemble_data();
        let mut results = all_skipped_results();
        results.ensemble = SourceResult::Fresh(ensemble_data, fresh_cache_meta());

        let params = default_params();
        let response = build_response(results, &params);

        let ens = response.ensemble.as_ref().unwrap();
        // All 3 time steps are on the same day
        assert_eq!(ens.daily_sections.len(), 1);
        assert_eq!(ens.daily_sections[0].date, "2026-04-24");
        assert_eq!(ens.daily_sections[0].start_index, 0);
        assert_eq!(ens.daily_sections[0].end_index, 2);
    }

    #[test]
    fn test_build_response_astronomy() {
        let ensemble_data = make_ensemble_data();
        let mut results = all_skipped_results();
        results.ensemble = SourceResult::Fresh(ensemble_data, fresh_cache_meta());

        let params = default_params();
        let response = build_response(results, &params);

        let astro = response.astronomy.as_ref().unwrap();
        assert_eq!(astro.times.len(), 3);
        assert_eq!(astro.sun_altitude.len(), 3);
        assert_eq!(astro.moon_altitude.len(), 3);

        // Verify sun and moon altitudes are computed for each time step
        // (We don't assert specific values since they depend on the exact
        // date/time and location, but we verify the arrays are populated
        // and values are in the valid range.)
        for &alt in &astro.sun_altitude {
            assert!(
                (-90.0..=90.0).contains(&alt),
                "Sun altitude {alt} out of range"
            );
        }
        for &alt in &astro.moon_altitude {
            assert!(
                (-90.0..=90.0).contains(&alt),
                "Moon altitude {alt} out of range"
            );
        }
    }

    #[test]
    fn test_build_response_marine() {
        let marine_data = MarineData {
            times: vec!["2026-04-24T00:00".to_string()],
            wave_height: vec![Some(1.5)],
            wave_period: vec![Some(6.0)],
            wave_direction: vec![Some(210.0)],
            sea_surface_temperature: vec![Some(10.5)],
        };
        let mut results = all_skipped_results();
        results.marine = SourceResult::Refreshed(marine_data, fresh_cache_meta());

        let params = default_params();
        let response = build_response(results, &params);

        let marine = response.marine.as_ref().unwrap();
        assert_eq!(marine.times.len(), 1);
        assert_eq!(marine.wave_height[0], Some(1.5));
        assert_eq!(marine.sea_surface_temperature[0], Some(10.5));
    }

    #[test]
    fn test_build_response_hrrr() {
        let hrrr_data = HrrrData {
            times: vec!["2026-04-24T00:00".to_string()],
            temperature_2m: vec![Some(12.0)],
            apparent_temperature: vec![Some(10.0)],
            dew_point_2m: vec![Some(8.0)],
            wind_speed_10m: vec![Some(15.0)],
            wind_gusts_10m: vec![Some(25.0)],
            wind_direction_10m: vec![Some(180.0)],
            surface_pressure: vec![Some(1013.0)],
            precipitation: vec![Some(0.0)],
            precipitation_probability: vec![Some(10.0)],
        };
        let mut results = all_skipped_results();
        results.hrrr = SourceResult::Fresh(hrrr_data, fresh_cache_meta());

        let params = default_params();
        let response = build_response(results, &params);

        let hrrr = response.hrrr.as_ref().unwrap();
        assert_eq!(hrrr.temperature_2m[0], Some(12.0));
        assert_eq!(hrrr.surface_pressure[0], Some(1013.0));
    }

    #[test]
    fn test_build_response_observations() {
        let obs_data = ObservationData {
            station: StationInfo {
                id: "KBFI".to_string(),
                name: "Seattle, Boeing Field".to_string(),
                latitude: 47.53,
                longitude: -122.30,
                distance_km: 8.2,
            },
            entries: vec![ObservationEntry {
                timestamp: "2026-04-24T15:53:00Z".to_string(),
                temperature_celsius: Some(14.4),
                wind_speed_kmh: Some(18.5),
                wind_direction_degrees: Some(200.0),
                pressure_hpa: Some(1013.2),
            }],
        };
        let mut results = all_skipped_results();
        results.observations = SourceResult::Refreshed(obs_data, fresh_cache_meta());

        let params = default_params();
        let response = build_response(results, &params);

        let obs = response.observations.as_ref().unwrap();
        assert_eq!(obs.station.id, "KBFI");
        assert_eq!(obs.station.latitude, Some(47.53));
        assert_eq!(obs.station.distance_km, Some(8.2));
        assert_eq!(obs.entries.len(), 1);
        assert_eq!(obs.entries[0].temperature_celsius, Some(14.4));
    }

    #[test]
    fn test_build_response_tides() {
        let tides_data = TidesData {
            station_id: "9447130".to_string(),
            station_name: "Seattle".to_string(),
            predictions: vec![TidePrediction {
                time: "2026-04-24 00:00".to_string(),
                height_m: 1.234,
            }],
        };
        let mut results = all_skipped_results();
        results.tides = SourceResult::Refreshed(tides_data, fresh_cache_meta());

        let params = default_params();
        let response = build_response(results, &params);

        let tides = response.tides.as_ref().unwrap();
        assert_eq!(tides.station.id, "9447130");
        assert_eq!(tides.predictions.len(), 1);
        assert!((tides.predictions[0].height_m - 1.234).abs() < 1e-9);
    }

    #[test]
    fn test_build_response_water_temperature() {
        let wt_data = WaterTemperatureData {
            station_id: "9447130".to_string(),
            station_name: "Seattle".to_string(),
            temperature_celsius: Some(10.5),
            timestamp: Some("2026-04-24T14:00:00Z".to_string()),
        };
        let mut results = all_skipped_results();
        results.water_temperature = SourceResult::Refreshed(wt_data, fresh_cache_meta());

        let params = default_params();
        let response = build_response(results, &params);

        let wt = response.water_temperature.as_ref().unwrap();
        assert_eq!(wt.station.id, "9447130");
        assert_eq!(wt.temperature_celsius, Some(10.5));
    }

    #[test]
    fn test_build_response_ciops_sst() {
        let ciops_data = CiopsSstData {
            times: vec!["2026-04-24T12:00:00+00:00".to_string()],
            temperatures_celsius: vec![Some(10.0)],
        };
        let mut results = all_skipped_results();
        results.ciops_sst = SourceResult::Refreshed(ciops_data, fresh_cache_meta());

        let params = default_params();
        let response = build_response(results, &params);

        let ciops = response.ciops_sst.as_ref().unwrap();
        assert_eq!(ciops.times.len(), 1);
        assert_eq!(ciops.temperatures_celsius[0], Some(10.0));
    }

    #[test]
    fn test_build_response_cache_metadata() {
        let marine_data = MarineData {
            times: vec![],
            wave_height: vec![],
            wave_period: vec![],
            wave_direction: vec![],
            sea_surface_temperature: vec![],
        };
        let mut results = all_skipped_results();
        results.marine = SourceResult::Fresh(marine_data, fresh_cache_meta());

        let params = default_params();
        let response = build_response(results, &params);

        assert!(response.cache.contains_key("marine"));
        let meta = &response.cache["marine"];
        assert_eq!(meta.age_seconds, 100);
        assert!(meta.is_fresh);
    }

    #[test]
    fn test_build_response_error_messages() {
        let mut results = all_skipped_results();
        results.air_quality =
            SourceResult::Failed("upstream timeout after 15s".to_string());

        let params = default_params();
        let response = build_response(results, &params);

        assert_eq!(
            response.errors.get("air_quality").unwrap(),
            &Some("upstream timeout after 15s".to_string())
        );
        // Other sources should have None errors
        assert_eq!(response.errors.get("ensemble").unwrap(), &None);
    }

    #[test]
    fn test_build_response_stale_includes_error() {
        let marine_data = MarineData {
            times: vec![],
            wave_height: vec![],
            wave_period: vec![],
            wave_direction: vec![],
            sea_surface_temperature: vec![],
        };
        let mut results = all_skipped_results();
        results.marine = SourceResult::Stale(
            marine_data,
            fresh_cache_meta(),
            "upstream timeout".to_string(),
        );

        let params = default_params();
        let response = build_response(results, &params);

        // Marine data should still be present
        assert!(response.marine.is_some());
        // But the error should be recorded
        assert_eq!(
            response.errors.get("marine").unwrap(),
            &Some("upstream timeout".to_string())
        );
        // Cache metadata should be present
        assert!(response.cache.contains_key("marine"));
    }

    #[test]
    fn test_build_response_serializes_to_json() {
        let ensemble_data = make_ensemble_data();
        let mut results = all_skipped_results();
        results.ensemble = SourceResult::Fresh(ensemble_data, fresh_cache_meta());

        let params = default_params();
        let response = build_response(results, &params);

        // Verify the response can be serialized to JSON without errors
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"ensemble\""));
        assert!(json.contains("\"statistics\""));
        assert!(json.contains("\"temperature_2m\""));
        assert!(json.contains("\"cache\""));
        assert!(json.contains("\"errors\""));
        assert!(!json.contains("\"members_by_model\""), "members_by_model should not be in forecast response");
    }

    #[test]
    fn test_parse_time_valid() {
        let dt = parse_time("2026-04-24T00:00").unwrap();
        assert_eq!(dt.year(), 2026);
        assert_eq!(dt.month(), 4);
        assert_eq!(dt.day(), 24);
    }

    #[test]
    fn test_parse_time_invalid() {
        assert!(parse_time("not-a-time").is_none());
    }

    #[test]
    fn test_cache_metadata_from_cache_meta() {
        let meta = fresh_cache_meta();
        let cache_metadata = CacheMetadata::from(&meta);
        assert_eq!(cache_metadata.age_seconds, 100);
        assert!(cache_metadata.is_fresh);
        assert_eq!(cache_metadata.fetched_at, "2026-04-24T14:30:00+00:00");
    }

    // -----------------------------------------------------------------------
    // Integration-style tests — exercise the full build_response pipeline
    // with various AllSourceResults configurations
    // -----------------------------------------------------------------------

    use crate::sources::uv::UvData;
    use crate::sources::air_quality::AirQualityData;

    /// Helper: build a fully-populated AllSourceResults with realistic data
    /// for all primary sources.
    fn all_sources_populated() -> AllSourceResults {
        let ensemble_data = make_ensemble_data();
        let marine_data = MarineData {
            times: vec!["2026-04-24T00:00".to_string(), "2026-04-24T01:00".to_string()],
            wave_height: vec![Some(1.5), Some(1.8)],
            wave_period: vec![Some(6.0), Some(6.5)],
            wave_direction: vec![Some(210.0), Some(215.0)],
            sea_surface_temperature: vec![Some(10.5), Some(10.6)],
        };
        let hrrr_data = HrrrData {
            times: vec!["2026-04-24T00:00".to_string(), "2026-04-24T01:00".to_string()],
            temperature_2m: vec![Some(12.0), Some(13.0)],
            apparent_temperature: vec![Some(10.0), Some(11.0)],
            dew_point_2m: vec![Some(8.0), Some(9.0)],
            wind_speed_10m: vec![Some(15.0), Some(18.0)],
            wind_gusts_10m: vec![Some(25.0), Some(28.0)],
            wind_direction_10m: vec![Some(180.0), Some(190.0)],
            surface_pressure: vec![Some(1013.0), Some(1012.0)],
            precipitation: vec![Some(0.0), Some(0.5)],
            precipitation_probability: vec![Some(10.0), Some(30.0)],
        };
        let uv_data = UvData {
            times: vec!["2026-04-24T00:00".to_string(), "2026-04-24T01:00".to_string()],
            uv_index: vec![Some(0.0), Some(2.5)],
            uv_index_clear_sky: vec![Some(0.0), Some(3.1)],
        };
        let aq_data = AirQualityData {
            times: vec!["2026-04-24T00:00".to_string(), "2026-04-24T01:00".to_string()],
            us_aqi: vec![Some(42.0), Some(45.0)],
            pm2_5: vec![Some(10.2), Some(11.5)],
            pm10: vec![Some(18.0), Some(20.0)],
        };
        let obs_data = ObservationData {
            station: StationInfo {
                id: "KBFI".to_string(),
                name: "Seattle, Boeing Field".to_string(),
                latitude: 47.53,
                longitude: -122.30,
                distance_km: 8.2,
            },
            entries: vec![ObservationEntry {
                timestamp: "2026-04-24T15:53:00Z".to_string(),
                temperature_celsius: Some(14.4),
                wind_speed_kmh: Some(18.5),
                wind_direction_degrees: Some(200.0),
                pressure_hpa: Some(1013.2),
            }],
        };
        let tides_data = TidesData {
            station_id: "9447130".to_string(),
            station_name: "Seattle".to_string(),
            predictions: vec![
                TidePrediction { time: "2026-04-24 00:00".to_string(), height_m: 1.234 },
                TidePrediction { time: "2026-04-24 06:00".to_string(), height_m: 3.456 },
            ],
        };
        let wt_data = WaterTemperatureData {
            station_id: "9447130".to_string(),
            station_name: "Seattle".to_string(),
            temperature_celsius: Some(10.5),
            timestamp: Some("2026-04-24T14:00:00Z".to_string()),
        };
        let ciops_data = CiopsSstData {
            times: vec![
                "2026-04-24T12:00:00+00:00".to_string(),
                "2026-04-24T18:00:00+00:00".to_string(),
            ],
            temperatures_celsius: vec![Some(10.0), Some(10.2)],
        };

        AllSourceResults {
            ensemble: SourceResult::Fresh(ensemble_data, fresh_cache_meta()),
            marine: SourceResult::Refreshed(marine_data, fresh_cache_meta()),
            hrrr: SourceResult::Fresh(hrrr_data, fresh_cache_meta()),
            uv: SourceResult::Fresh(uv_data, fresh_cache_meta()),
            air_quality: SourceResult::Fresh(aq_data, fresh_cache_meta()),
            observations: SourceResult::Refreshed(obs_data, fresh_cache_meta()),
            tides: SourceResult::Refreshed(tides_data, fresh_cache_meta()),
            water_temperature: SourceResult::Refreshed(wt_data, fresh_cache_meta()),
            ciops_sst: SourceResult::Refreshed(ciops_data, fresh_cache_meta()),
        }
    }

    /// Integration test: full forecast with all sources populated.
    /// Verifies the response structure has all fields present and the full
    /// pipeline (ensemble → percentile → probability → aggregation →
    /// astronomy → response assembly) works end-to-end.
    #[test]
    fn test_full_forecast_all_sources_populated() {
        let results = all_sources_populated();
        let params = default_params();
        let response = build_response(results, &params);

        // All source sections should be present
        assert!(response.ensemble.is_some(), "ensemble should be present");
        assert!(response.marine.is_some(), "marine should be present");
        assert!(response.hrrr.is_some(), "hrrr should be present");
        assert!(response.uv.is_some(), "uv should be present");
        assert!(response.air_quality.is_some(), "air_quality should be present");
        assert!(response.observations.is_some(), "observations should be present");
        assert!(response.tides.is_some(), "tides should be present");
        assert!(response.water_temperature.is_some(), "water_temperature should be present");
        assert!(response.ciops_sst.is_some(), "ciops_sst should be present");
        assert!(response.astronomy.is_some(), "astronomy should be present");

        // Ensemble sub-fields should be populated
        let ens = response.ensemble.as_ref().unwrap();
        assert!(!ens.times.is_empty(), "ensemble times should not be empty");
        assert!(!ens.statistics.is_empty(), "ensemble statistics should not be empty");
        assert!(ens.statistics.contains_key("temperature_2m"));
        assert!(ens.statistics.contains_key("precipitation"));
        assert_eq!(ens.precipitation_probability.any.len(), ens.times.len());
        assert!(!ens.daily_sections.is_empty(), "daily sections should not be empty");

        // Astronomy should align with ensemble times
        let astro = response.astronomy.as_ref().unwrap();
        assert_eq!(astro.times.len(), ens.times.len());
        assert_eq!(astro.sun_altitude.len(), ens.times.len());
        assert_eq!(astro.moon_altitude.len(), ens.times.len());

        // Cache metadata should be present for cached sources
        assert!(response.cache.contains_key("ensemble"));
        assert!(response.cache.contains_key("marine"));
        assert!(response.cache.contains_key("hrrr"));
        assert!(response.cache.contains_key("uv"));
        assert!(response.cache.contains_key("air_quality"));

        // Errors map should have entries for all sources
        assert!(response.errors.contains_key("ensemble"));
        assert!(response.errors.contains_key("marine"));
        assert!(response.errors.contains_key("hrrr"));
        assert!(response.errors.contains_key("uv"));
        assert!(response.errors.contains_key("air_quality"));
        assert!(response.errors.contains_key("observations"));
        assert!(response.errors.contains_key("tides"));
        assert!(response.errors.contains_key("water_temperature"));
        assert!(response.errors.contains_key("ciops_sst"));

        // No errors when all sources succeed
        for (_key, err) in &response.errors {
            assert_eq!(err, &None, "no errors expected when all sources succeed");
        }

        // Verify the full response serializes to valid JSON
        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("ensemble").is_some());
        assert!(parsed.get("cache").is_some());
        assert!(parsed.get("errors").is_some());
        assert!(parsed.get("astronomy").is_some());

        // members_by_model should not be in the ensemble response
        let ens_json = parsed.get("ensemble").unwrap();
        assert!(ens_json.get("members_by_model").is_none(), "members_by_model should not be in ensemble response");
    }

    /// Integration test: one source failing while others succeed.
    /// Verifies error isolation — the failed source returns null data and
    /// an error message, while other sources are unaffected.
    #[test]
    fn test_forecast_one_source_failing_error_isolation() {
        let mut results = all_sources_populated();
        // Simulate air_quality upstream failure
        results.air_quality = SourceResult::Failed("upstream timeout after 15s".to_string());

        let params = default_params();
        let response = build_response(results, &params);

        // Air quality should be absent (null in JSON)
        assert!(response.air_quality.is_none(), "failed source should be None");

        // Error message should be recorded for air_quality
        assert_eq!(
            response.errors.get("air_quality").unwrap(),
            &Some("upstream timeout after 15s".to_string()),
        );

        // All other sources should still be present and unaffected
        assert!(response.ensemble.is_some(), "ensemble should be unaffected");
        assert!(response.marine.is_some(), "marine should be unaffected");
        assert!(response.hrrr.is_some(), "hrrr should be unaffected");
        assert!(response.uv.is_some(), "uv should be unaffected");
        assert!(response.observations.is_some(), "observations should be unaffected");
        assert!(response.tides.is_some(), "tides should be unaffected");
        assert!(response.water_temperature.is_some(), "water_temperature should be unaffected");
        assert!(response.ciops_sst.is_some(), "ciops_sst should be unaffected");
        assert!(response.astronomy.is_some(), "astronomy should be unaffected");

        // Other sources should have no errors
        assert_eq!(response.errors.get("ensemble").unwrap(), &None);
        assert_eq!(response.errors.get("marine").unwrap(), &None);
        assert_eq!(response.errors.get("hrrr").unwrap(), &None);

        // No cache entry for the failed source
        assert!(!response.cache.contains_key("air_quality"));
    }

    /// Integration test: stale data fallback.
    /// Verifies that when a source returns stale cached data (upstream failed
    /// but stale cache exists), the data is still present with a staleness
    /// indicator and error message.
    #[test]
    fn test_forecast_stale_data_fallback() {
        let marine_data = MarineData {
            times: vec!["2026-04-24T00:00".to_string()],
            wave_height: vec![Some(1.5)],
            wave_period: vec![Some(6.0)],
            wave_direction: vec![Some(210.0)],
            sea_surface_temperature: vec![Some(10.5)],
        };
        let stale_cache_meta = CacheMeta {
            age_seconds: 7200, // 2 hours old — past the 1-hour TTL
            is_fresh: false,
            fetched_at: "2026-04-24T12:30:00+00:00".to_string(),
        };

        let mut results = all_sources_populated();
        results.marine = SourceResult::Stale(
            marine_data,
            stale_cache_meta,
            "upstream timeout".to_string(),
        );

        let params = default_params();
        let response = build_response(results, &params);

        // Marine data should still be present (stale fallback)
        let marine = response.marine.as_ref().unwrap();
        assert_eq!(marine.wave_height[0], Some(1.5));

        // Error message should be recorded
        assert_eq!(
            response.errors.get("marine").unwrap(),
            &Some("upstream timeout".to_string()),
        );

        // Cache metadata should reflect staleness
        let cache_meta = response.cache.get("marine").unwrap();
        assert!(!cache_meta.is_fresh, "stale data should have is_fresh=false");
        assert_eq!(cache_meta.age_seconds, 7200);
    }

    /// Integration test: Failed result produces null data and error message.
    /// Verifies that a SourceResult::Failed produces None for that source's
    /// data and includes the error message in the errors map.
    #[test]
    fn test_forecast_failed_result_null_data_and_error() {
        let mut results = all_skipped_results();
        // Only ensemble succeeds; marine and hrrr fail
        results.ensemble = SourceResult::Fresh(make_ensemble_data(), fresh_cache_meta());
        results.marine = SourceResult::Failed("HTTP 500: Internal Server Error".to_string());
        results.hrrr = SourceResult::Failed("network error: DNS resolution failed".to_string());

        let params = default_params();
        let response = build_response(results, &params);

        // Ensemble should be present
        assert!(response.ensemble.is_some());
        assert!(response.astronomy.is_some()); // derived from ensemble

        // Failed sources should be None
        assert!(response.marine.is_none());
        assert!(response.hrrr.is_none());

        // Error messages should be present
        assert_eq!(
            response.errors.get("marine").unwrap(),
            &Some("HTTP 500: Internal Server Error".to_string()),
        );
        assert_eq!(
            response.errors.get("hrrr").unwrap(),
            &Some("network error: DNS resolution failed".to_string()),
        );

        // No cache entries for failed sources
        assert!(!response.cache.contains_key("marine"));
        assert!(!response.cache.contains_key("hrrr"));

        // Ensemble cache should be present
        assert!(response.cache.contains_key("ensemble"));
    }

    /// Integration test: conditional sources (tides, water_temperature,
    /// ciops_sst) as Skipped.
    /// Verifies that skipped conditional sources are absent from the response
    /// and do not appear in cache metadata or produce error entries.
    #[test]
    fn test_forecast_conditional_sources_skipped() {
        let mut results = all_sources_populated();
        // Simulate a non-coastal location where conditional sources are skipped
        results.tides = SourceResult::Skipped;
        results.water_temperature = SourceResult::Skipped;
        results.ciops_sst = SourceResult::Skipped;

        let params = default_params();
        let response = build_response(results, &params);

        // Conditional sources should be absent
        assert!(response.tides.is_none(), "skipped tides should be None");
        assert!(
            response.water_temperature.is_none(),
            "skipped water_temperature should be None"
        );
        assert!(response.ciops_sst.is_none(), "skipped ciops_sst should be None");

        // No cache entries for skipped sources
        assert!(!response.cache.contains_key("tides"));
        assert!(!response.cache.contains_key("water_temperature"));
        assert!(!response.cache.contains_key("ciops_sst"));

        // Primary sources should still be present
        assert!(response.ensemble.is_some());
        assert!(response.marine.is_some());
        assert!(response.hrrr.is_some());
        assert!(response.uv.is_some());
        assert!(response.air_quality.is_some());
        assert!(response.observations.is_some());

        // Verify the response serializes correctly with absent conditional sources
        let json = serde_json::to_string(&response).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        // skip_serializing_if = "Option::is_none" means these keys should be absent
        assert!(parsed.get("tides").is_none(), "tides should not appear in JSON");
        assert!(
            parsed.get("water_temperature").is_none(),
            "water_temperature should not appear in JSON"
        );
        assert!(
            parsed.get("ciops_sst").is_none(),
            "ciops_sst should not appear in JSON"
        );
    }
}
