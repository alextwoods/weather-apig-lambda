import type { ZoomLevel } from './url-state';
import type { UnitPreferences } from '../units/types';

const STORAGE_KEY = 'weather-app-state';

export interface StoredState {
    location: { lat: number; lon: number; name: string } | null;
    marine: { lat: number; lon: number } | null;
    stationId: string | null;
    models: string[];
    units: UnitPreferences;
    overlays: string[];
    zoom: ZoomLevel;
}

/**
 * Persists the given state to localStorage as JSON.
 */
export function saveState(state: StoredState): void {
    try {
        const json = JSON.stringify(state);
        localStorage.setItem(STORAGE_KEY, json);
    } catch {
        // Storage full or unavailable — silently ignore
    }
}

/**
 * Loads persisted state from localStorage.
 * Returns null if no state is stored or if the stored data is corrupted.
 * Corrupted data is cleared automatically.
 */
export function loadState(): StoredState | null {
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (raw === null) {
            return null;
        }
        const parsed: unknown = JSON.parse(raw);
        if (!isValidStoredState(parsed)) {
            clearState();
            return null;
        }
        return parsed;
    } catch {
        // JSON.parse failed or localStorage unavailable — clear corrupted data
        clearState();
        return null;
    }
}

/**
 * Removes the persisted state from localStorage.
 */
export function clearState(): void {
    try {
        localStorage.removeItem(STORAGE_KEY);
    } catch {
        // localStorage unavailable — silently ignore
    }
}

// --- Validation ---

function isValidStoredState(value: unknown): value is StoredState {
    if (typeof value !== 'object' || value === null) {
        return false;
    }

    const obj = value as Record<string, unknown>;

    // location: { lat, lon, name } | null
    if (obj.location !== null) {
        if (!isValidLocation(obj.location)) return false;
    }

    // marine: { lat, lon } | null
    if (obj.marine !== null) {
        if (!isValidMarine(obj.marine)) return false;
    }

    // stationId: string | null
    if (obj.stationId !== null && typeof obj.stationId !== 'string') {
        return false;
    }

    // models: string[]
    if (!Array.isArray(obj.models) || !obj.models.every(m => typeof m === 'string')) {
        return false;
    }

    // units: UnitPreferences
    if (!isValidUnitPreferences(obj.units)) {
        return false;
    }

    // overlays: string[]
    if (!Array.isArray(obj.overlays) || !obj.overlays.every(o => typeof o === 'string')) {
        return false;
    }

    // zoom: ZoomLevel
    if (!isValidZoomLevel(obj.zoom)) {
        return false;
    }

    return true;
}

function isValidLocation(value: unknown): value is { lat: number; lon: number; name: string } {
    if (typeof value !== 'object' || value === null) return false;
    const loc = value as Record<string, unknown>;
    return (
        typeof loc.lat === 'number' && isFinite(loc.lat) &&
        typeof loc.lon === 'number' && isFinite(loc.lon) &&
        typeof loc.name === 'string'
    );
}

function isValidMarine(value: unknown): value is { lat: number; lon: number } {
    if (typeof value !== 'object' || value === null) return false;
    const m = value as Record<string, unknown>;
    return (
        typeof m.lat === 'number' && isFinite(m.lat) &&
        typeof m.lon === 'number' && isFinite(m.lon)
    );
}

const VALID_TEMP_UNITS = new Set(['C', 'F']);
const VALID_WIND_UNITS = new Set(['kmh', 'mph', 'kts', 'ms']);
const VALID_PRESSURE_UNITS = new Set(['hPa', 'inHg', 'mmHg']);
const VALID_PRECIP_UNITS = new Set(['mm', 'in']);
const VALID_WAVE_UNITS = new Set(['m', 'ft']);

function isValidUnitPreferences(value: unknown): value is UnitPreferences {
    if (typeof value !== 'object' || value === null) return false;
    const u = value as Record<string, unknown>;
    return (
        VALID_TEMP_UNITS.has(u.temperature as string) &&
        VALID_WIND_UNITS.has(u.wind as string) &&
        VALID_PRESSURE_UNITS.has(u.pressure as string) &&
        VALID_PRECIP_UNITS.has(u.precipitation as string) &&
        VALID_WAVE_UNITS.has(u.wave as string)
    );
}

const VALID_ZOOM_LEVELS = new Set(['3d', '5d', '7d', '10d']);
/** Legacy zoom levels that should be migrated to the new default */
const LEGACY_ZOOM_LEVELS = new Set(['2h', '6h', '12h', '24h']);

function isValidZoomLevel(value: unknown): value is ZoomLevel {
    return typeof value === 'string' && (VALID_ZOOM_LEVELS.has(value) || LEGACY_ZOOM_LEVELS.has(value));
}
