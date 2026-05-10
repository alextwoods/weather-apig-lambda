/**
 * URL builder functions for weather API endpoints.
 * Each function constructs the path + query string for a specific endpoint.
 */

/**
 * Maps short model names (used in UI/URL state) to the full API suffixes
 * expected by the backend.
 */
const MODEL_NAME_TO_SUFFIX: Record<string, string> = {
    ecmwf: 'ecmwf_ifs025_ensemble',
    gfs: 'ncep_gefs_seamless',
    icon: 'icon_seamless_eps',
    gem: 'gem_global_ensemble',
    bom: 'bom_access_global_ensemble',
};

/** Convert an array of short model names to their API suffixes. */
function toApiModelNames(models: string[]): string[] {
    return models.map(m => MODEL_NAME_TO_SUFFIX[m] ?? m);
}

export interface ForecastParams {
    lat: number;
    lon: number;
    marine_lat?: number;
    marine_lon?: number;
    station_id?: string;
    models?: string[];
    force_refresh?: boolean;
    refresh_source?: string;
}

export interface MembersParams {
    variable: string;
    lat: number;
    lon: number;
    models?: string[];
}

function buildQueryString(params: Record<string, string | undefined>): string {
    const entries = Object.entries(params).filter(
        (entry): entry is [string, string] => entry[1] !== undefined
    );
    if (entries.length === 0) return '';
    return '?' + entries.map(([k, v]) => `${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join('&');
}

export function forecastUrl(params: ForecastParams): string {
    const query: Record<string, string | undefined> = {
        lat: String(params.lat),
        lon: String(params.lon),
        marine_lat: params.marine_lat !== undefined ? String(params.marine_lat) : undefined,
        marine_lon: params.marine_lon !== undefined ? String(params.marine_lon) : undefined,
        station_id: params.station_id,
        models: params.models ? toApiModelNames(params.models).join(',') : undefined,
        force_refresh: params.force_refresh ? 'true' : undefined,
        refresh_source: params.refresh_source,
    };
    return `/forecast${buildQueryString(query)}`;
}

export function forecastMembersUrl(params: MembersParams): string {
    const query: Record<string, string | undefined> = {
        variable: params.variable,
        lat: String(params.lat),
        lon: String(params.lon),
        models: params.models ? toApiModelNames(params.models).join(',') : undefined,
    };
    return `/forecast/members${buildQueryString(query)}`;
}

export function geocodeUrl(query: string): string {
    return `/geocode${buildQueryString({ q: query })}`;
}

export function observationStationsUrl(lat: number, lon: number): string {
    return `/stations/observations${buildQueryString({ lat: String(lat), lon: String(lon) })}`;
}

export function marineStationsUrl(lat: number, lon: number): string {
    return `/stations/marine${buildQueryString({ lat: String(lat), lon: String(lon) })}`;
}
