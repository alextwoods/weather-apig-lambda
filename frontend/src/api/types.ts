export interface ForecastResponse {
    ensemble: {
        times: string[];
        statistics: Record<string, PercentileStats>;
        precipitation_probability: {
            any: (number | null)[];
            moderate: (number | null)[];
            heavy: (number | null)[];
        };
        daily_sections: DailySection[];
    };
    marine?: {
        times: string[];
        wave_height: (number | null)[];
        wave_period: (number | null)[];
        wave_direction: (number | null)[];
        sea_surface_temperature: (number | null)[];
    };
    hrrr?: {
        times: string[];
        temperature_2m: (number | null)[];
        apparent_temperature: (number | null)[];
        wind_speed_10m: (number | null)[];
        wind_gusts_10m: (number | null)[];
        wind_direction_10m: (number | null)[];
        surface_pressure: (number | null)[];
        precipitation: (number | null)[];
        precipitation_probability: (number | null)[];
    };
    uv?: {
        times: string[];
        uv_index: (number | null)[];
        uv_index_clear_sky: (number | null)[];
    };
    air_quality?: {
        times: string[];
        us_aqi: (number | null)[];
        pm2_5: (number | null)[];
        pm10: (number | null)[];
    };
    observations?: {
        station: StationInfo;
        entries: ObservationEntry[];
    };
    tides?: {
        station: { id: string; name: string };
        predictions: { time: string; height_m: number }[];
    };
    water_temperature?: {
        station: { id: string; name: string };
        temperature_celsius: number;
        timestamp: string;
    };
    astronomy: {
        times: string[];
        sun_altitude: number[];
        moon_altitude: number[];
    };
    cache: Record<string, CacheInfo>;
    errors: Record<string, string | null>;
}

export interface PercentileStats {
    p10: (number | null)[];
    p25: (number | null)[];
    median: (number | null)[];
    p75: (number | null)[];
    p90: (number | null)[];
}

export interface DailySection {
    date: string;
    start_index: number;
    end_index: number;
    high_temp: number | null;
    low_temp: number | null;
    total_precip: number | null;
    max_wind: number | null;
    dominant_wind_direction: string | null;
}

export interface CacheInfo {
    age_seconds: number;
    is_fresh: boolean;
    fetched_at: string;
}

export interface StationInfo {
    id: string;
    name: string;
    latitude: number;
    longitude: number;
    distance_km: number;
}

export interface ObservationEntry {
    timestamp: string;
    temperature_celsius: number | null;
    wind_speed_kmh: number | null;
    wind_direction_degrees: number | null;
    pressure_hpa: number | null;
}

export interface MembersResponse {
    times: string[];
    statistics: PercentileStats;
    members_by_model: Record<string, number[][]>;
}

export interface GeocodeResult {
    name: string;
    latitude: number;
    longitude: number;
    country: string;
    admin1?: string;
}

export type GeocodeResponse = GeocodeResult[];

export interface StationResult {
    id: string;
    name: string;
    latitude: number;
    longitude: number;
    distance_km: number;
}

export type StationsResponse = StationResult[];
