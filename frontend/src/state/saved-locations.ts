/**
 * Saved locations persistence layer.
 * Stores a list of named locations (with optional marine + station overrides)
 * in localStorage for quick switching.
 */

const STORAGE_KEY = 'weather-saved-locations';

export interface SavedLocation {
    id: string;              // Unique ID (timestamp-based)
    name: string;            // User-provided display name
    lat: number;
    lon: number;
    marine?: { lat: number; lon: number } | null;
    stationId?: string | null;
}

/**
 * Load all saved locations from localStorage.
 */
export function loadSavedLocations(): SavedLocation[] {
    try {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (!raw) return [];
        const parsed = JSON.parse(raw);
        if (!Array.isArray(parsed)) return [];
        return parsed.filter(isValidSavedLocation);
    } catch {
        return [];
    }
}

/**
 * Save the full list of locations to localStorage.
 */
export function saveSavedLocations(locations: SavedLocation[]): void {
    try {
        localStorage.setItem(STORAGE_KEY, JSON.stringify(locations));
    } catch {
        // Storage full or unavailable
    }
}

/**
 * Add a new saved location. Returns the updated list.
 */
export function addSavedLocation(
    locations: SavedLocation[],
    location: Omit<SavedLocation, 'id'>,
): SavedLocation[] {
    const newLocation: SavedLocation = {
        ...location,
        id: `loc_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
    };
    const updated = [...locations, newLocation];
    saveSavedLocations(updated);
    return updated;
}

/**
 * Remove a saved location by ID. Returns the updated list.
 */
export function removeSavedLocation(
    locations: SavedLocation[],
    id: string,
): SavedLocation[] {
    const updated = locations.filter(l => l.id !== id);
    saveSavedLocations(updated);
    return updated;
}

/**
 * Update a saved location's name. Returns the updated list.
 */
export function renameSavedLocation(
    locations: SavedLocation[],
    id: string,
    newName: string,
): SavedLocation[] {
    const updated = locations.map(l => l.id === id ? { ...l, name: newName } : l);
    saveSavedLocations(updated);
    return updated;
}

// --- Validation ---

function isValidSavedLocation(value: unknown): value is SavedLocation {
    if (typeof value !== 'object' || value === null) return false;
    const obj = value as Record<string, unknown>;
    return (
        typeof obj.id === 'string' &&
        typeof obj.name === 'string' &&
        typeof obj.lat === 'number' && isFinite(obj.lat) &&
        typeof obj.lon === 'number' && isFinite(obj.lon)
    );
}
