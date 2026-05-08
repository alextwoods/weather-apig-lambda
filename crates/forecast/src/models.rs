use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Ensemble model registry
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnsembleModel {
    pub name: &'static str,
    pub api_key_suffix: &'static str,
    pub member_count: usize,
}

pub const ENSEMBLE_MODELS: [EnsembleModel; 5] = [
    EnsembleModel {
        name: "ECMWF IFS 0.25°",
        api_key_suffix: "ecmwf_ifs025_ensemble",
        member_count: 51,
    },
    EnsembleModel {
        name: "GFS Seamless",
        api_key_suffix: "ncep_gefs_seamless",
        member_count: 31,
    },
    EnsembleModel {
        name: "ICON Seamless",
        api_key_suffix: "icon_seamless_eps",
        member_count: 40,
    },
    EnsembleModel {
        name: "GEM Global",
        api_key_suffix: "gem_global_ensemble",
        member_count: 21,
    },
    EnsembleModel {
        name: "BOM ACCESS",
        api_key_suffix: "bom_access_global_ensemble",
        member_count: 18,
    },
];

// ---------------------------------------------------------------------------
// Weather variables requested from the ensemble API
// ---------------------------------------------------------------------------

pub const WEATHER_VARIABLES: [&str; 11] = [
    "temperature_2m",
    "relative_humidity_2m",
    "apparent_temperature",
    "cloud_cover",
    "wind_speed_10m",
    "wind_gusts_10m",
    "wind_direction_10m",
    "dew_point_2m",
    "precipitation",
    "pressure_msl",
    "shortwave_radiation",
];

// ---------------------------------------------------------------------------
// Application state — shared across requests within a Lambda invocation
// ---------------------------------------------------------------------------

pub struct AppState {
    pub http_client: reqwest::Client,
    pub s3_client: aws_sdk_s3::Client,
    pub ddb_client: aws_sdk_dynamodb::Client,
    pub config: AppConfig,
}

pub struct AppConfig {
    /// S3 bucket name for large cached responses (ensemble, marine).
    pub cache_bucket: String,
    /// DynamoDB table name for smaller cached responses (HRRR, UV, air quality).
    pub cache_table: String,
    /// DynamoDB table name for location access tracking.
    pub tracker_table: String,
    /// Per-source HTTP timeout in seconds (default 15).
    pub default_timeout_secs: u64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            cache_bucket: String::new(),
            cache_table: String::new(),
            tracker_table: String::new(),
            default_timeout_secs: 15,
        }
    }
}

// ---------------------------------------------------------------------------
// Forecast request parameters
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchParams {
    pub lat: f64,
    pub lon: f64,
    pub marine_lat: Option<f64>,
    pub marine_lon: Option<f64>,
    pub station_id: Option<String>,
    pub force_refresh: bool,
    pub refresh_source: Option<String>,
    pub models: Option<Vec<String>>,
    /// Number of forecast days to include in the response (1–35, default 10).
    pub forecast_days: u32,
}

// ---------------------------------------------------------------------------
// Bounding boxes for conditional supplementary fetches
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct BoundingBox {
    pub lat_min: f64,
    pub lat_max: f64,
    pub lon_min: f64,
    pub lon_max: f64,
}

impl BoundingBox {
    pub const fn contains(&self, lat: f64, lon: f64) -> bool {
        lat >= self.lat_min && lat <= self.lat_max && lon >= self.lon_min && lon <= self.lon_max
    }
}

/// Puget Sound — triggers NOAA tide/water-temp fetches.
pub const PUGET_SOUND_BOX: BoundingBox = BoundingBox {
    lat_min: 47.0,
    lat_max: 48.8,
    lon_min: -123.5,
    lon_max: -122.0,
};

/// Salish Sea — triggers CIOPS SST fetches.
pub const SALISH_SEA_BOX: BoundingBox = BoundingBox {
    lat_min: 46.998,
    lat_max: 50.994,
    lon_min: -126.204,
    lon_max: -121.109,
};

// ---------------------------------------------------------------------------
// NOAA station registry (hardcoded Puget Sound)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoaaStation {
    pub id: &'static str,
    pub name: &'static str,
    pub lat: f64,
    pub lon: f64,
}

pub const PUGET_SOUND_STATIONS: [NoaaStation; 6] = [
    NoaaStation {
        id: "9447130",
        name: "Seattle",
        lat: 47.6026,
        lon: -122.3393,
    },
    NoaaStation {
        id: "9446484",
        name: "Tacoma",
        lat: 47.2690,
        lon: -122.4132,
    },
    NoaaStation {
        id: "9444900",
        name: "Port Townsend",
        lat: 48.1129,
        lon: -122.7595,
    },
    NoaaStation {
        id: "9447110",
        name: "Anacortes",
        lat: 48.5117,
        lon: -122.6767,
    },
    NoaaStation {
        id: "9449880",
        name: "Friday Harbor",
        lat: 48.5469,
        lon: -123.0128,
    },
    NoaaStation {
        id: "9440910",
        name: "Olympia",
        lat: 47.0483,
        lon: -122.9050,
    },
];

// ---------------------------------------------------------------------------
// Haversine distance utility
// ---------------------------------------------------------------------------

/// Computes the great-circle distance in kilometres between two points on
/// Earth using the Haversine formula.
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    const R: f64 = 6371.0; // Earth radius in km
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    R * c
}

// ---------------------------------------------------------------------------
// Nearest Puget Sound station (declaration — full impl in task 4.3)
// ---------------------------------------------------------------------------

/// Returns the nearest NOAA station within 50 km of the given coordinates,
/// or `None` if no station is close enough.
pub fn nearest_puget_sound_station(lat: f64, lon: f64) -> Option<&'static NoaaStation> {
    PUGET_SOUND_STATIONS
        .iter()
        .map(|s| (s, haversine_km(lat, lon, s.lat, s.lon)))
        .filter(|(_, d)| *d <= 50.0)
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(s, _)| s)
}

/// Returns all NOAA stations within 100 km of the given coordinates, sorted
/// by distance (nearest first).
///
/// Used for water temperature fallback: if the nearest station doesn't offer
/// water temperature data, the caller can try the next-nearest station.
pub fn nearby_puget_sound_stations(lat: f64, lon: f64) -> Vec<&'static NoaaStation> {
    let mut stations: Vec<(&NoaaStation, f64)> = PUGET_SOUND_STATIONS
        .iter()
        .map(|s| (s, haversine_km(lat, lon, s.lat, s.lon)))
        .filter(|(_, d)| *d <= 100.0)
        .collect();
    stations.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
    stations.into_iter().map(|(s, _)| s).collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ensemble_model_count() {
        assert_eq!(ENSEMBLE_MODELS.len(), 5);
        let total_members: usize = ENSEMBLE_MODELS.iter().map(|m| m.member_count).sum();
        assert_eq!(total_members, 161);
    }

    #[test]
    fn test_weather_variables_count() {
        assert_eq!(WEATHER_VARIABLES.len(), 11);
    }

    #[test]
    fn test_bounding_box_contains() {
        // Seattle is inside Puget Sound box
        assert!(PUGET_SOUND_BOX.contains(47.6062, -122.3321));
        // Seattle is inside Salish Sea box
        assert!(SALISH_SEA_BOX.contains(47.6062, -122.3321));
        // New York is outside both
        assert!(!PUGET_SOUND_BOX.contains(40.7128, -74.0060));
        assert!(!SALISH_SEA_BOX.contains(40.7128, -74.0060));
        // Edge: exactly on boundary
        assert!(PUGET_SOUND_BOX.contains(47.0, -123.5));
        assert!(PUGET_SOUND_BOX.contains(48.8, -122.0));
    }

    #[test]
    fn test_haversine_seattle_to_tacoma() {
        // Seattle (47.6062, -122.3321) to Tacoma (47.2529, -122.4443)
        let d = haversine_km(47.6062, -122.3321, 47.2529, -122.4443);
        // Expected ~40 km
        assert!(d > 35.0 && d < 50.0, "Seattle-Tacoma distance: {d} km");
    }

    #[test]
    fn test_haversine_same_point() {
        let d = haversine_km(47.6062, -122.3321, 47.6062, -122.3321);
        assert!(d.abs() < 0.001, "Same point distance should be ~0: {d}");
    }

    #[test]
    fn test_puget_sound_stations_count() {
        assert_eq!(PUGET_SOUND_STATIONS.len(), 6);
    }

    #[test]
    fn test_nearest_station_seattle() {
        // Near Seattle — should return station 9447130
        let station = nearest_puget_sound_station(47.6062, -122.3321);
        assert!(station.is_some());
        assert_eq!(station.unwrap().id, "9447130");
    }

    #[test]
    fn test_nearby_stations_seattle() {
        // Near Seattle — should return multiple stations sorted by distance
        let stations = nearby_puget_sound_stations(47.6062, -122.3321);
        assert!(!stations.is_empty());
        // Seattle should be first (nearest)
        assert_eq!(stations[0].id, "9447130");
        // Should include Tacoma (within 100km)
        assert!(stations.iter().any(|s| s.id == "9446484"));
    }

    #[test]
    fn test_nearby_stations_far_away() {
        // New York — no station within 100 km
        let stations = nearby_puget_sound_stations(40.7128, -74.0060);
        assert!(stations.is_empty());
    }

    #[test]
    fn test_nearest_station_far_away() {
        // New York — no station within 50 km
        let station = nearest_puget_sound_station(40.7128, -74.0060);
        assert!(station.is_none());
    }

    #[test]
    fn test_app_config_default() {
        let config = AppConfig::default();
        assert_eq!(config.default_timeout_secs, 15);
        assert!(config.cache_bucket.is_empty());
        assert!(config.cache_table.is_empty());
        assert!(config.tracker_table.is_empty());
    }

    /// Feature: weather-backend-api, Property 9: Nearest Puget Sound station selection
    ///
    /// **Validates: Requirements 7.7**
    mod prop_nearest_station {
        use super::*;
        use proptest::prelude::*;

        proptest! {
            #[test]
            fn prop_nearest_station_within_bounding_box(
                lat in 47.0f64..48.8f64,
                lon in -123.5f64..-122.0f64,
            ) {
                let result = nearest_puget_sound_station(lat, lon);

                // Compute distances to all stations
                let distances: Vec<(&NoaaStation, f64)> = PUGET_SOUND_STATIONS
                    .iter()
                    .map(|s| (s, haversine_km(lat, lon, s.lat, s.lon)))
                    .collect();

                let nearest_within_50 = distances
                    .iter()
                    .filter(|(_, d)| *d <= 50.0)
                    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());

                match (result, nearest_within_50) {
                    (Some(station), Some((expected, dist))) => {
                        // Returned station must be the one with the smallest distance
                        prop_assert_eq!(
                            station.id, expected.id,
                            "For ({}, {}): expected station {} (dist={:.2}km) but got {} (dist={:.2}km)",
                            lat, lon, expected.id, dist,
                            station.id, haversine_km(lat, lon, station.lat, station.lon)
                        );

                        // Distance must be ≤ 50km
                        let actual_dist = haversine_km(lat, lon, station.lat, station.lon);
                        prop_assert!(
                            actual_dist <= 50.0,
                            "Station {} at ({}, {}) is {:.2}km away, exceeds 50km limit",
                            station.id, lat, lon, actual_dist
                        );
                    }
                    (None, None) => {
                        // No station within 50km — correct
                    }
                    (Some(station), None) => {
                        prop_assert!(
                            false,
                            "Function returned station {} but no station is within 50km of ({}, {})",
                            station.id, lat, lon
                        );
                    }
                    (None, Some((expected, dist))) => {
                        prop_assert!(
                            false,
                            "Function returned None but station {} is {:.2}km from ({}, {})",
                            expected.id, dist, lat, lon
                        );
                    }
                }
            }

            #[test]
            fn prop_nearest_station_outside_bounding_box(
                lat in prop::strategy::Union::new(vec![
                    (-90.0f64..46.0f64).boxed(),
                    (50.0f64..90.0f64).boxed(),
                ]),
                lon in prop::strategy::Union::new(vec![
                    (-180.0f64..-125.0f64).boxed(),
                    (-121.0f64..180.0f64).boxed(),
                ]),
            ) {
                // Coordinates far outside the Puget Sound area — all stations
                // should be well beyond 50km, so the function should return None.
                let result = nearest_puget_sound_station(lat, lon);

                // Verify independently: check if any station is actually within 50km
                let any_within_50 = PUGET_SOUND_STATIONS
                    .iter()
                    .any(|s| haversine_km(lat, lon, s.lat, s.lon) <= 50.0);

                if !any_within_50 {
                    prop_assert!(
                        result.is_none(),
                        "No station within 50km of ({}, {}) but function returned {:?}",
                        lat, lon, result.map(|s| s.id)
                    );
                } else if let Some(station) = result {
                    // If a station happens to be within 50km (unlikely with these ranges),
                    // verify it's the nearest one
                    let actual_dist = haversine_km(lat, lon, station.lat, station.lon);
                    prop_assert!(
                        actual_dist <= 50.0,
                        "Returned station {} is {:.2}km away, exceeds 50km",
                        station.id, actual_dist
                    );

                    // Verify it's the nearest
                    for s in PUGET_SOUND_STATIONS.iter() {
                        let d = haversine_km(lat, lon, s.lat, s.lon);
                        if d <= 50.0 {
                            prop_assert!(
                                actual_dist <= d + 1e-9,
                                "Station {} ({:.2}km) is closer than returned station {} ({:.2}km)",
                                s.id, d, station.id, actual_dist
                            );
                        }
                    }
                }
            }
        }
    }
}
