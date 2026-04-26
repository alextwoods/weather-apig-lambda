# Ensemble Weather Backend API — Client Requirements Specification

This document describes the current behavior of the EnsembleWeather iOS application from the perspective of a client that will consume a new backend API. It details every external data source, how data is fetched and aggregated, how caching works, how location selection works, and what computations are currently performed on-device that should move to the server. The goal is to provide enough detail for an implementer to build a backend API that can power both the existing iOS app (refactored) and a new web app.

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Location Selection and Geocoding](#2-location-selection-and-geocoding)
3. [External Data Sources](#3-external-data-sources)
   - 3.1 [Open-Meteo Ensemble API](#31-open-meteo-ensemble-api)
   - 3.2 [Open-Meteo Marine API](#32-open-meteo-marine-api)
   - 3.3 [Open-Meteo HRRR / GFS Deterministic API](#33-open-meteo-hrrr--gfs-deterministic-api)
   - 3.4 [Open-Meteo UV Forecast API](#34-open-meteo-uv-forecast-api)
   - 3.5 [Open-Meteo Air Quality API](#35-open-meteo-air-quality-api)
   - 3.6 [NOAA CO-OPS Tides & Currents API](#36-noaa-co-ops-tides--currents-api)
   - 3.7 [ECCC CIOPS-Salish Sea WMS API](#37-eccc-ciops-salish-sea-wms-api)
   - 3.8 [NWS Observation Stations API](#38-nws-observation-stations-api)
   - 3.9 [Open-Meteo Geocoding API](#39-open-meteo-geocoding-api)
   - 3.10 [Open-Meteo Model Metadata API](#310-open-meteo-model-metadata-api)
4. [Data Aggregation and Computation](#4-data-aggregation-and-computation)
   - 4.1 [Ensemble Member Extraction](#41-ensemble-member-extraction)
   - 4.2 [Percentile Statistics](#42-percentile-statistics)
   - 4.3 [Precipitation Probability](#43-precipitation-probability)
   - 4.4 [Daily Section Aggregation](#44-daily-section-aggregation)
   - 4.5 [HRRR Time Filtering](#45-hrrr-time-filtering)
   - 4.6 [Observation Time Filtering](#46-observation-time-filtering)
   - 4.7 [Compass Direction Conversion](#47-compass-direction-conversion)
   - 4.8 [Astronomical Calculations](#48-astronomical-calculations)
5. [Caching Strategy](#5-caching-strategy)
6. [Fetch Orchestration](#6-fetch-orchestration)
   - 6.1 [Concurrent Fetch Pattern](#61-concurrent-fetch-pattern)
   - 6.2 [Conditional Supplementary Fetches](#62-conditional-supplementary-fetches)
   - 6.3 [Error Isolation](#63-error-isolation)
   - 6.4 [Throttle Handling](#64-throttle-handling)
   - 6.5 [Per-Source Refresh](#65-per-source-refresh)
   - 6.6 [Model Selection Recomputation](#66-model-selection-recomputation)
7. [User Preferences](#7-user-preferences)
8. [Unit Conversion](#8-unit-conversion)
9. [Ensemble Models](#9-ensemble-models)
10. [Weather Variables](#10-weather-variables)
11. [Data Structures the API Must Provide](#11-data-structures-the-api-must-provide)
12. [Responsibilities: API vs. Client](#12-responsibilities-api-vs-client)
13. [Requirements for the Backend API](#13-requirements-for-the-backend-api)

---

## 1. Architecture Overview

The iOS app currently acts as a thick client. It directly calls 9+ external APIs, aggregates their responses, runs statistical computations (percentile engine, precipitation probability), caches raw responses to disk, and manages all orchestration logic (concurrent fetches, conditional activation of supplementary sources, throttle detection, per-source error isolation).

The proposed backend API will absorb all of this:
- Fetching from upstream sources
- Caching upstream responses
- Running percentile and probability computations
- Aggregating and aligning time series from different sources
- Conditional activation of regional data sources (NOAA, CIOPS)
- Throttle detection and graceful degradation

The clients (iOS app, web app) will make a small number of calls to the backend API and receive pre-computed, ready-to-display data. The clients remain responsible for:
- Unit conversion (display preference)
- UI-specific formatting (cell text, colors, layout)
- Location persistence (which location the user selected)
- Device GPS access
- Rendering charts and tables

---

## 2. Location Selection and Geocoding

### Current Behavior

The app supports three types of forecast locations:

| Type | Description |
|------|-------------|
| `city` | Selected via geocoding search (Open-Meteo Geocoding API) |
| `weatherStation` | Selected from a list of nearby NWS observation stations |
| `pinDrop` | User drops a pin on a map at arbitrary coordinates |

All location types resolve to a `(latitude, longitude)` pair that is used for all forecast fetches.

### Separate Marine Location

The app supports an optional separate marine location, independent of the forecast location. Marine location types:

| Type | Description |
|------|-------------|
| `default` | Uses the forecast location coordinates |
| `marineStation` | A specific NOAA buoy or tide station |
| `pinDrop` | User drops a pin on a map for marine data |

When a marine location is set, marine-related fetches (Open-Meteo Marine, NOAA tides/water temp, CIOPS SST) use the marine coordinates. All other fetches (ensemble, HRRR, UV, air quality, observations) use the forecast coordinates.

### Separate Observation Station

The user can optionally select a specific NWS observation station. When set, observations are fetched from that station. When not set, the app automatically finds the nearest station using the NWS API.

### Geocoding

The app uses the Open-Meteo Geocoding API to search for locations by name. This returns structured results with `id`, `name`, `country`, `countryCode`, `admin1` (state/province), `latitude`, and `longitude`. The app formats display names as "City, ST" for US locations (using state abbreviation) or "City, Country" for international locations.

### Location Persistence

The selected forecast location, marine location, and observation station are each persisted independently to UserDefaults as JSON. On app launch, the persisted location is loaded and used immediately. The app also supports legacy migration from an older `LocationResult` format to the current `ForecastLocation` format.

### What the API Needs

- A geocoding endpoint (or the API can proxy the Open-Meteo Geocoding API)
- All forecast endpoints accept `(latitude, longitude)` as the primary input
- Support for separate marine coordinates on marine-related endpoints
- A way to search for nearby observation stations given a coordinate
- A way to search for nearby NOAA marine stations given a coordinate

---

## 3. External Data Sources

### 3.1 Open-Meteo Ensemble API

**Base URL:** `https://ensemble-api.open-meteo.com/v1/ensemble`

**Purpose:** Primary forecast data. Fetches hourly ensemble member data from 5 global weather models simultaneously.

**Request Parameters:**
- `latitude`: Forecast latitude
- `longitude`: Forecast longitude
- `hourly`: Comma-separated list of weather variables (see Section 10)
- `models`: `ecmwf_ifs025,gfs_seamless,icon_seamless,gem_global_ensemble,bom_access_global_ensemble`
- `forecast_days`: `35`
- `past_hours`: `12`

**Response Structure:**
The response contains `latitude`, `longitude`, `generationtime_ms`, `hourly`, and `hourly_units`.

The `hourly` object contains:
- `time`: Array of ISO 8601 time strings (`"yyyy-MM-dd'T'HH:mm"`, UTC, no seconds)
- Dynamic member keys in the format `{variable}_member{NN}` for single-model responses, or `{variable}_member{NN}_{model_suffix}` for multi-model responses. Each key maps to an array of `Double?` values aligned with the `time` array.

**Example member keys (multi-model):**
- `temperature_2m_member00_ecmwf_ifs025_ensemble`
- `temperature_2m_member01_ncep_gefs_seamless`
- `wind_speed_10m_member00_icon_seamless_eps`

**Model API Key Suffixes:**

| Model | API Key Suffix | Member Count |
|-------|---------------|-------------|
| ECMWF IFS 0.25° | `ecmwf_ifs025_ensemble` | 51 |
| GFS Seamless (GEFS) | `ncep_gefs_seamless` | 31 |
| ICON Seamless | `icon_seamless_eps` | 40 |
| GEM Global | `gem_global_ensemble` | 21 |
| BOM ACCESS Global | `bom_access_global_ensemble` | 18 |

**Total ensemble members across all models: 161**

**Variables requested (all at once):**
- `temperature_2m`
- `relative_humidity_2m`
- `apparent_temperature`
- `cloud_cover`
- `wind_speed_10m`
- `wind_gusts_10m`
- `wind_direction_10m`
- `dew_point_2m`
- `precipitation`
- `pressure_msl`
- `shortwave_radiation`

**Critical behavior:** The app always requests ALL variables and ALL models in a single API call. This is the largest and most important data source.

---

### 3.2 Open-Meteo Marine API

**Base URL:** `https://marine-api.open-meteo.com/v1/marine`

**Purpose:** Wave height, wave period, wave direction, and sea surface temperature forecasts.

**Request Parameters:**
- `latitude`: Marine location latitude
- `longitude`: Marine location longitude
- `hourly`: `wave_height,wave_period,wave_direction,sea_surface_temperature`
- `forecast_days`: `7`
- `past_hours`: `12`

**Response Structure:**
```json
{
  "latitude": 47.6,
  "longitude": -122.33,
  "hourly": {
    "time": ["2026-04-24T00:00", ...],
    "wave_height": [0.5, ...],
    "wave_period": [4.2, ...],
    "wave_direction": [180.0, ...],
    "sea_surface_temperature": [null, ...]
  }
}
Important: For inland/coastal locations (like Puget Sound), the marine API often returns wave data but null for all SST values. This condition triggers supplementary NOAA and CIOPS fetches (see Section 6.2).

3.3 Open-Meteo HRRR / GFS Deterministic API
Base URL: https://api.open-meteo.com/v1/gfs

Purpose: High-resolution (3km) deterministic forecast from the HRRR model. Used as a high-quality proxy for recent actuals because HRRR assimilates recent radar and surface observations. Displayed as an overlay on ensemble charts.

Request Parameters:

latitude: Forecast latitude
longitude: Forecast longitude
hourly: temperature_2m,apparent_temperature,dew_point_2m,wind_speed_10m,wind_gusts_10m,wind_direction_10m,surface_pressure,precipitation,precipitation_probability
forecast_days: 2
past_hours: 24
Response Structure: Single-value (not ensemble) hourly arrays for each variable. All values are Double?.

Post-processing: After fetching, the app filters out entries older than 12 hours before the current time. This ensures HRRR data never extends further back than the ensemble's 12-hour historical window.

3.4 Open-Meteo UV Forecast API
Base URL: https://api.open-meteo.com/v1/forecast

Purpose: UV index forecast. Not available on the ensemble API, so fetched separately as a deterministic single-value forecast.

Request Parameters:

latitude: Forecast latitude
longitude: Forecast longitude
hourly: uv_index,uv_index_clear_sky
forecast_days: 16
past_hours: 12
Response Structure:

{
  "hourly": {
    "time": ["2026-04-24T00:00", ...],
    "uv_index": [0.0, ...],
    "uv_index_clear_sky": [0.0, ...]
  }
}
3.5 Open-Meteo Air Quality API
Base URL: https://air-quality-api.open-meteo.com/v1/air-quality

Purpose: Air quality index and particulate matter forecasts.

Request Parameters:

latitude: Forecast latitude
longitude: Forecast longitude
hourly: us_aqi,pm2_5,pm10
forecast_days: 7
past_hours: 12
Response Structure:

{
  "hourly": {
    "time": ["2026-04-24T00:00", ...],
    "us_aqi": [42.0, ...],
    "pm2_5": [10.5, ...],
    "pm10": [15.2, ...]
  }
}
AQI Categories (US EPA breakpoints):

Category	AQI Range
Good	0–50
Moderate	51–100
Unhealthy (Sensitive)	101–150
Unhealthy	151–200
Very Unhealthy	201–300
Hazardous	301–500
3.6 NOAA CO-OPS Tides & Currents API
Base URL: https://api.tidesandcurrents.noaa.gov/api/prod/datagetter

Purpose: Water temperature observations and tide predictions for Puget Sound stations.

This source is conditionally activated (see Section 6.2). It is only fetched when:

The marine response has wave data but ALL sea surface temperature values are null
The location is within the Puget Sound bounding box (lat 47.0–48.8, lon -123.5 to -122.0)
A nearby NOAA station exists in the hardcoded registry (within ~50km)
Water Temperature Request:

station: Station ID (e.g., 9447130 for Seattle)
product: water_temperature
date: latest
units: metric
time_zone: lst_ldt
format: json
Returns the most recent observed water temperature in Celsius with a timestamp.

Tide Predictions Request:

station: Station ID
product: predictions
begin_date: Start date (yyyyMMdd)
end_date: End date (yyyyMMdd)
datum: MLLW
units: metric
time_zone: lst_ldt
interval: 6 (6-minute intervals for smooth charting)
format: json
The date range for tide predictions is derived from the marine forecast time range (first and last marine forecast timestamps).

Returns an array of {t: "yyyy-MM-dd HH:mm", v: "1.234"} predictions.

Hardcoded Puget Sound Stations:

ID	Name	Latitude	Longitude
9447130	Seattle	47.6026	-122.3393
9446484	Tacoma	47.2690	-122.4132
9444900	Port Townsend	48.1129	-122.7595
9447110	Anacortes	48.5117	-122.6767
9449880	Friday Harbor	48.5469	-123.0128
9440910	Olympia	47.0483	-122.9050
The app also bundles a larger JSON file (noaa_stations.json) of all coastal NOAA CO-OPS tide stations and NDBC buoy stations, used for the marine station selection UI. The nearbyStations function searches this list within a configurable radius (default 100km) using the Haversine formula.

Water temperature and tide predictions are fetched concurrently and independently. Failure of one does not affect the other.

3.7 ECCC CIOPS-Salish Sea WMS API
Base URL: https://geo.weather.gc.ca/geomet

Purpose: Water temperature forecasts from the ECCC CIOPS-Salish Sea 500m resolution ocean model.

This source is conditionally activated (see Section 6.2). It is only fetched when:

The marine response has wave data but ALL sea surface temperature values are null
The location is within the CIOPS-Salish Sea bounding box (lat 46.998–50.994, lon -126.204 to -121.109)
Request: WMS GetFeatureInfo for a single point and time step.

Parameters:

SERVICE: WMS
VERSION: 1.3.0
REQUEST: GetFeatureInfo
LAYERS / QUERY_LAYERS: CIOPS-SalishSea_500m_SeaWaterPotentialTemp_0.5m
INFO_FORMAT: application/json
CRS: EPSG:4326
BBOX: {lat-0.005},{lon-0.005},{lat+0.005},{lon+0.005} (~500m box around the point)
WIDTH: 2, HEIGHT: 2, I: 1, J: 1
TIME: ISO 8601 datetime (e.g., 2026-04-24T18:00:00Z)
Fetching pattern: The client generates 9 time steps at 6-hour intervals (00Z, 06Z, 12Z, 18Z) starting from the current time rounded down to the nearest 6-hour boundary, extending 48 hours into the future. Each time step requires a separate HTTP request. All 9 requests are made concurrently using a task group.

Response: GeoJSON FeatureCollection. The temperature value is in Kelvin and must be converted to Celsius (subtract 273.15). Individual time-step failures produce nil entries rather than failing the entire fetch.

Result: A CIOPSSSTData structure with parallel arrays of times: [Date] and temperaturesCelsius: [Double?].

3.8 NWS Observation Stations API
Purpose: Recent weather observations from NOAA/NWS observation stations.

Station Discovery:

URL: https://api.weather.gov/points/{lat},{lon}/stations
Coordinates rounded to 4 decimal places
Headers: User-Agent: EnsembleWeather/1.3.0, Accept: application/geo+json
Returns a GeoJSON FeatureCollection of nearby stations sorted by proximity
The app computes Haversine distance from the search coordinate to each station
Observation Fetch:

URL: https://api.weather.gov/stations/{stationId}/observations?limit=25
Headers: User-Agent: EnsembleWeather/1.3.0, Accept: application/geo+json
Returns the 25 most recent observations from the station
Unit conversions applied during parsing:

Temperature: Celsius (as-is from API)
Wind speed: m/s → km/h (multiply by 3.6)
Wind direction: degrees (as-is)
Pressure: Pa → hPa (divide by 100)
Post-processing: Observations older than 12 hours before the current time are filtered out, matching the ensemble's 12-hour historical window.

Result: Array of NOAAObservation with timestamp, temperatureCelsius, windSpeedKmh, windDirectionDegrees, pressureHPa (all optional except timestamp), plus the WeatherStation used.

3.9 Open-Meteo Geocoding API
Base URL: https://geocoding-api.open-meteo.com/v1/search

Parameters:

name: Search query string
count: 10
language: en
format: json
Response: Array of results with id, name, country, country_code, admin1, latitude, longitude.

3.10 Open-Meteo Model Metadata API
Purpose: Provides model initialization times, data availability times, and update intervals. These calls are NOT counted toward daily API request limits.

Endpoints:

Source	Metadata URL
ECMWF IFS	https://ensemble-api.open-meteo.com/data/ecmwf_ifs025_ensemble/static/meta.json
GFS/GEFS	https://ensemble-api.open-meteo.com/data/ncep_gefs025/static/meta.json
ICON	https://ensemble-api.open-meteo.com/data/dwd_icon_eps/static/meta.json
GEM	https://ensemble-api.open-meteo.com/data/cmc_gem_geps/static/meta.json
BOM ACCESS	https://ensemble-api.open-meteo.com/data/bom_access_global_ensemble/static/meta.json
HRRR	https://api.open-meteo.com/data/ncep_hrrr_conus/static/meta.json
Marine	https://marine-api.open-meteo.com/data/ncep_gfswave025/static/meta.json
Air Quality	https://air-quality-api.open-meteo.com/data/cams_global/static/meta.json
UV (GFS)	https://api.open-meteo.com/data/ncep_gfs025/static/meta.json
Response fields:

last_run_initialisation_time: Unix timestamp of the model's initialization/reference time
last_run_availability_time: Unix timestamp of when data became available on the API
update_interval_seconds: Typical interval between model updates
These are fetched on demand (not on every forecast load) when the user views the data source status panel.

4. Data Aggregation and Computation
4.1 Ensemble Member Extraction
The raw ensemble response contains member data as flat keys. The app extracts member arrays for a given variable using prefix matching:

All models combined: Find all keys matching {variable}_member{NN}*, validate that digits follow the _member prefix, sort by key, return the value arrays. This pools all 161 members together.

Filtered by model set: When the user has toggled specific models on/off, extraction filters keys by checking whether they contain the model's apiKeySuffix. For single-model responses (where no keys contain any model suffix), all members are attributed to a fallback model.

Per-model extraction: Same as filtered, but for a single model. Used for the detail chart where each model's members are color-coded.

The extraction also produces MemberLine objects for the detail chart, each carrying the original API key as identity, the source EnsembleModel, and the value array.

4.2 Percentile Statistics
For each weather variable, the PercentileEngine computes per-time-step statistics from the extracted member arrays:

p10 (10th percentile)
p25 (25th percentile)
median (50th percentile)
p75 (75th percentile)
p90 (90th percentile)
Algorithm: At each time step, collect all non-nil values from all members, sort them, and compute each percentile using linear interpolation between the two nearest ranks.

Percentile formula: For a sorted array of n values and percentile p:

rank = p * (n - 1)
lower = floor(rank), upper = ceil(rank)
fraction = rank - lower
result = values[lower] + fraction * (values[upper] - values[lower])
The result is a ForecastStatistics struct per variable containing parallel arrays (one value per time step) for each percentile band, plus a timeStepCount.

4.3 Precipitation Probability
Computed from ensemble precipitation member arrays:

Single-threshold (any precipitation):

At each time step, count members with precipitation > 0.1 mm
Probability = (precipitating members / total non-nil members) × 100%
Multi-threshold:

Tier	Threshold
Any	> 0.1 mm/hr
Moderate	> 2.5 mm/hr
Heavy	> 7.5 mm/hr
All three thresholds are computed in a single pass per time step for efficiency. The result is a PrecipitationProbability struct with three parallel [Double?] arrays.

4.4 Daily Section Aggregation
The app groups hourly time steps into calendar days and computes daily summaries:

For each day:

High temperature: Maximum of median temperature values in the day's hours
Low temperature: Minimum of median temperature values in the day's hours
Total precipitation: Sum of median precipitation values in the day's hours
Max wind: Maximum of median wind speed values in the day's hours
Dominant wind direction: Mode (most frequent) compass direction from median wind direction values
Each daily section also carries the index range into the hourly arrays, enabling the UI to render hourly rows grouped under day headers.

4.5 HRRR Time Filtering
After fetching HRRR data (which includes 24 hours of past data), the app filters out entries older than 12 hours before the current time. This aligns the HRRR historical window with the ensemble's past_hours: 12 parameter. All hourly arrays are sliced to the same start index.

4.6 Observation Time Filtering
Similarly, observations older than 12 hours before the current time are filtered out after fetching.

4.7 Compass Direction Conversion
Wind direction in degrees is converted to a 16-point compass rose direction (N, NNE, NE, ENE, E, ESE, SE, SSE, S, SSW, SW, WSW, W, WNW, NW, NNW). Each direction spans a 22.5° sector. The input is normalized to [0, 360).

4.8 Astronomical Calculations
The app computes sun and moon altitude (elevation angle above the horizon) for chart overlays:

Sun altitude: Uses the NOAA solar calculator algorithm. Negative values indicate the sun is below the horizon (used for night shading on charts).
Moon altitude: Uses a low-precision method (~1–2° accuracy) sufficient for visual overlay on tide charts.
Both take (date, latitude, longitude) as inputs and return altitude in degrees.

5. Caching Strategy
Current Client-Side Caching
The app uses a file-based cache in the device's caches directory:

Cache key: Coordinates rounded to 2 decimal places. Example: lat=47.6062, lon=-122.3321 → "47.61_-122.33"

Separate cache files per data source:

{key}_ensemble.json
{key}_marine.json
{key}_uv.json
{key}_airquality.json
{key}_hrrr.json
TTL: 1 hour (3600 seconds), matching Open-Meteo's model update cadence. Freshness is determined by the file's modification date.

Cache behavior:

On forecast load, cached data is loaded immediately for instant display
If the cache is fresh (within TTL), the network fetch is skipped entirely
If the cache is stale, a network fetch is made; on success, the cache is updated
On network failure, cached data continues to display (graceful degradation)
On throttle (HTTP 420), a user-friendly warning is shown while cached data persists
Corrupted cache files are automatically removed (treated as cache miss)
Per-source cache invalidation: Each source can be individually invalidated (file deleted) to force a refresh, used by the per-source refresh feature.

Not cached: NOAA observations (always fetched fresh), NOAA water temperature, NOAA tide predictions, CIOPS SST.

What the API Should Do
The backend API should implement server-side caching with similar semantics:

Cache upstream API responses keyed by coordinate (with appropriate rounding)
Use a TTL aligned with each upstream source's update cadence
Return cache metadata (age, freshness) so clients can display staleness indicators
Support force-refresh requests that bypass the cache
Handle upstream throttling gracefully, serving stale cached data with appropriate headers
6. Fetch Orchestration
6.1 Concurrent Fetch Pattern
The app fetches all data sources concurrently using Swift's async let:

async let ensemble = fetchEnsembleIfNeeded(...)
async let marine = fetchMarineIfNeeded(...)
async let uv = fetchUVIfNeeded(...)
async let airQuality = fetchAirQualityIfNeeded(...)
async let hrrr = fetchHRRRIfNeeded(...)
async let observations = fetchObservationsIfNeeded(...)
All six are awaited together. Each handles its own errors internally.

6.2 Conditional Supplementary Fetches
Some data sources are only activated based on conditions:

NOAA Tides & Water Temperature — activated when ALL of:

Marine response has wave data (at least one non-nil wave height)
ALL sea surface temperature values in the marine response are null
Location is within the Puget Sound bounding box (lat 47.0–48.8, lon -123.5 to -122.0)
A nearby NOAA station exists in the registry (within ~50km)
CIOPS SST — activated when ALL of:

Marine response has wave data but ALL SST values are null (same as NOAA condition 1+2)
Location is within the CIOPS-Salish Sea bounding box (lat 46.998–50.994, lon -126.204 to -121.109)
Both NOAA and CIOPS are checked after the marine fetch completes (or after marine cache is loaded). They are fetched concurrently with each other.

6.3 Error Isolation
Each data source is independently error-isolated:

Ensemble errors set errorMessage (primary error)
Marine errors set marineErrorMessage (independent of ensemble)
All other sources (HRRR, UV, air quality, observations, NOAA, CIOPS) append to a supplementaryErrors array and record per-source error details, but never affect other sources
NOAA water temperature and tide prediction failures are independent of each other
6.4 Throttle Handling
When the Open-Meteo API returns HTTP 420 (rate limit):

If cached data is available, it continues to display
A user-friendly warning message is shown: "Forecast data may be outdated — the weather service daily request limit has been reached. Showing cached data."
The isThrottleWarning flag is set so the UI can style the warning differently from hard errors
6.5 Per-Source Refresh
Each data source can be individually force-refreshed:

The source's cache is invalidated (file deleted) 2