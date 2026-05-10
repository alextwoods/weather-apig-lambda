import type { UnitPreferences, TempUnit, WindUnit, PressureUnit, PrecipUnit, WaveUnit } from '../units/types';

// --- Domain types for URL state ---

export type ZoomLevel = '2h' | '6h' | '12h' | '24h';
export type OverlayType = 'hrrr' | 'obs' | 'extended';
export type ModelShortName = 'ecmwf' | 'gfs' | 'icon' | 'gem' | 'bom';

export interface AppState {
    location: { lat: number; lon: number; name: string } | null;
    marine: { lat: number; lon: number } | null;
    stationId: string | null;
    models: Set<string>;
    zoom: ZoomLevel;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    viewMode: 'chart' | 'table';
}

// --- Valid value sets for parsing ---

const VALID_ZOOM_LEVELS: ReadonlySet<string> = new Set(['2h', '6h', '12h', '24h']);
const VALID_OVERLAY_TYPES: ReadonlySet<string> = new Set(['hrrr', 'obs', 'extended']);
const VALID_MODEL_NAMES: ReadonlySet<string> = new Set(['ecmwf', 'gfs', 'icon', 'gem', 'bom']);
const VALID_TEMP_UNITS: ReadonlySet<string> = new Set(['C', 'F']);
const VALID_WIND_UNITS: ReadonlySet<string> = new Set(['kmh', 'mph', 'kts', 'ms']);
const VALID_PRESSURE_UNITS: ReadonlySet<string> = new Set(['hPa', 'inHg', 'mmHg']);
const VALID_PRECIP_UNITS: ReadonlySet<string> = new Set(['mm', 'in']);
const VALID_WAVE_UNITS: ReadonlySet<string> = new Set(['m', 'ft']);
const VALID_VIEW_MODES: ReadonlySet<string> = new Set(['chart', 'table']);

// --- Serialization ---

/**
 * Serializes an AppState into URL search parameters.
 * Omits parameters that are null/empty/default where appropriate.
 * Marine coordinates are omitted if they match the location coordinates.
 */
export function serializeToUrl(state: AppState): URLSearchParams {
    const params = new URLSearchParams();

    // Location
    if (state.location) {
        params.set('lat', formatCoord(state.location.lat));
        params.set('lon', formatCoord(state.location.lon));
        params.set('name', state.location.name);
    }

    // Marine (omit if same as location)
    if (state.marine) {
        const sameAsLocation = state.location &&
            state.marine.lat === state.location.lat &&
            state.marine.lon === state.location.lon;
        if (!sameAsLocation) {
            params.set('mlat', formatCoord(state.marine.lat));
            params.set('mlon', formatCoord(state.marine.lon));
        }
    }

    // Station ID
    if (state.stationId) {
        params.set('sid', state.stationId);
    }

    // Models (comma-separated, sorted for stable URLs)
    if (state.models.size > 0) {
        params.set('models', [...state.models].sort().join(','));
    }

    // Zoom
    params.set('zoom', state.zoom);

    // Units (ordered: temp, wind, pressure, precip, wave)
    params.set('units', [
        state.units.temperature,
        state.units.wind,
        state.units.pressure,
        state.units.precipitation,
        state.units.wave,
    ].join(','));

    // Overlays (comma-separated, sorted for stable URLs)
    if (state.overlays.size > 0) {
        params.set('overlays', [...state.overlays].sort().join(','));
    }

    // View mode
    params.set('view', state.viewMode);

    return params;
}

// --- Deserialization ---

/**
 * Deserializes URL search parameters into a partial AppState.
 * Returns only the fields that are present and valid in the URL.
 * Invalid or missing parameters are simply omitted from the result.
 */
export function deserializeFromUrl(params: URLSearchParams): Partial<AppState> {
    const result: Partial<AppState> = {};

    // Location
    const lat = parseFloat(params.get('lat') ?? '');
    const lon = parseFloat(params.get('lon') ?? '');
    const name = params.get('name');
    if (isFinite(lat) && isFinite(lon) && name !== null) {
        result.location = { lat, lon, name };
    }

    // Marine
    const mlat = parseFloat(params.get('mlat') ?? '');
    const mlon = parseFloat(params.get('mlon') ?? '');
    if (isFinite(mlat) && isFinite(mlon)) {
        result.marine = { lat: mlat, lon: mlon };
    }

    // Station ID
    const sid = params.get('sid');
    if (sid !== null && sid.length > 0) {
        result.stationId = sid;
    }

    // Models
    const modelsStr = params.get('models');
    if (modelsStr !== null && modelsStr.length > 0) {
        const models = modelsStr.split(',').filter(m => VALID_MODEL_NAMES.has(m));
        if (models.length > 0) {
            result.models = new Set(models);
        }
    }

    // Zoom
    const zoom = params.get('zoom');
    if (zoom !== null && VALID_ZOOM_LEVELS.has(zoom)) {
        result.zoom = zoom as ZoomLevel;
    }

    // Units
    const unitsStr = params.get('units');
    if (unitsStr !== null) {
        const parts = unitsStr.split(',');
        if (parts.length === 5) {
            const [temp, wind, pressure, precip, wave] = parts;
            if (
                VALID_TEMP_UNITS.has(temp) &&
                VALID_WIND_UNITS.has(wind) &&
                VALID_PRESSURE_UNITS.has(pressure) &&
                VALID_PRECIP_UNITS.has(precip) &&
                VALID_WAVE_UNITS.has(wave)
            ) {
                result.units = {
                    temperature: temp as TempUnit,
                    wind: wind as WindUnit,
                    pressure: pressure as PressureUnit,
                    precipitation: precip as PrecipUnit,
                    wave: wave as WaveUnit,
                };
            }
        }
    }

    // Overlays
    const overlaysStr = params.get('overlays');
    if (overlaysStr !== null && overlaysStr.length > 0) {
        const overlays = overlaysStr.split(',').filter(o => VALID_OVERLAY_TYPES.has(o));
        if (overlays.length > 0) {
            result.overlays = new Set(overlays as OverlayType[]);
        }
    }

    // View mode
    const view = params.get('view');
    if (view !== null && VALID_VIEW_MODES.has(view)) {
        result.viewMode = view as 'chart' | 'table';
    }

    return result;
}

// --- Push state ---

/**
 * Updates the browser URL with the serialized state using history.replaceState
 * to avoid polluting browser history.
 */
export function pushState(state: AppState): void {
    const params = serializeToUrl(state);
    const url = `${window.location.pathname}?${params.toString()}`;
    history.replaceState(null, '', url);
}

// --- Helpers ---

/**
 * Formats a coordinate number to a reasonable precision.
 * Uses up to 6 decimal places (sub-meter precision) but trims trailing zeros.
 */
function formatCoord(value: number): string {
    // Use toFixed(6) for sub-meter precision, then strip trailing zeros
    const fixed = value.toFixed(6);
    return fixed.replace(/\.?0+$/, '') || '0';
}
