import { useState, useCallback, useRef, useEffect } from 'preact/hooks';
import type { WeatherApiClient } from '../api/client';
import type { GeocodeResult, StationResult } from '../api/types';
import {
    type SavedLocation,
    loadSavedLocations,
    addSavedLocation,
    removeSavedLocation,
} from '../state/saved-locations';

export interface LocationManagerProps {
    isOpen: boolean;
    onClose: () => void;
    onLocationSelect: (location: { lat: number; lon: number; name: string }) => void;
    onMarineSelect: (marine: { lat: number; lon: number } | null) => void;
    onStationSelect: (stationId: string | null) => void;
    currentLocation: { lat: number; lon: number; name: string } | null;
    currentMarine: { lat: number; lon: number } | null;
    currentStationId: string | null;
    apiClient: WeatherApiClient;
}

type Tab = 'locations' | 'marine' | 'observations';

/**
 * Format distance for display per iOS spec:
 * < 1 km: shown in meters
 * 1-10 km: one decimal
 * > 10 km: integer
 */
function formatDistance(km: number): string {
    if (km < 1) return `${Math.round(km * 1000)} m`;
    if (km < 10) return `${km.toFixed(1)} km`;
    return `${Math.round(km)} km`;
}

/**
 * Location Manager modal.
 * Provides:
 * - Quick-switch between saved locations
 * - Save current location with a custom name
 * - Search for new locations (geocode)
 * - Select marine station
 * - Select observation station
 */
export function LocationManager({
    isOpen,
    onClose,
    onLocationSelect,
    onMarineSelect,
    onStationSelect,
    currentLocation,
    currentMarine,
    currentStationId,
    apiClient,
}: LocationManagerProps) {
    const [tab, setTab] = useState<Tab>('locations');
    const [savedLocations, setSavedLocations] = useState<SavedLocation[]>(() => loadSavedLocations());

    // Search state
    const [searchQuery, setSearchQuery] = useState('');
    const [searchResults, setSearchResults] = useState<GeocodeResult[]>([]);
    const [isSearching, setIsSearching] = useState(false);
    const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

    // Save location state
    const [saveName, setSaveName] = useState('');
    const [showSaveForm, setShowSaveForm] = useState(false);

    // Station state
    const [marineStations, setMarineStations] = useState<StationResult[]>([]);
    const [obsStations, setObsStations] = useState<StationResult[]>([]);
    const [isLoadingStations, setIsLoadingStations] = useState(false);

    // Reload saved locations when modal opens
    useEffect(() => {
        if (isOpen) {
            setSavedLocations(loadSavedLocations());
            setSearchQuery('');
            setSearchResults([]);
            setShowSaveForm(false);
            setSaveName('');
        }
    }, [isOpen]);

    // Fetch stations when switching to marine/observations tab
    useEffect(() => {
        if (!isOpen || !currentLocation) return;

        if (tab === 'marine') {
            fetchMarineStations();
        } else if (tab === 'observations') {
            fetchObsStations();
        }
    }, [tab, isOpen, currentLocation?.lat, currentLocation?.lon]);

    const fetchMarineStations = useCallback(async () => {
        if (!currentLocation) return;
        setIsLoadingStations(true);
        try {
            const results = await apiClient.marineStations(currentLocation.lat, currentLocation.lon);
            setMarineStations(results.sort((a, b) => a.distance_km - b.distance_km));
        } catch {
            setMarineStations([]);
        } finally {
            setIsLoadingStations(false);
        }
    }, [currentLocation, apiClient]);

    const fetchObsStations = useCallback(async () => {
        if (!currentLocation) return;
        setIsLoadingStations(true);
        try {
            const results = await apiClient.observationStations(currentLocation.lat, currentLocation.lon);
            setObsStations(results.sort((a, b) => a.distance_km - b.distance_km));
        } catch {
            setObsStations([]);
        } finally {
            setIsLoadingStations(false);
        }
    }, [currentLocation, apiClient]);

    // --- Search ---

    const handleSearchInput = useCallback((e: Event) => {
        const value = (e.target as HTMLInputElement).value;
        setSearchQuery(value);

        if (debounceRef.current) clearTimeout(debounceRef.current);

        if (value.trim().length < 2) {
            setSearchResults([]);
            return;
        }

        debounceRef.current = setTimeout(async () => {
            setIsSearching(true);
            try {
                const results = await apiClient.geocode(value.trim());
                setSearchResults(results);
            } catch {
                setSearchResults([]);
            } finally {
                setIsSearching(false);
            }
        }, 300);
    }, [apiClient]);

    // --- Location actions ---

    const handleSelectSearchResult = useCallback((result: GeocodeResult) => {
        onLocationSelect({
            lat: result.latitude,
            lon: result.longitude,
            name: result.name,
        });
        onClose();
    }, [onLocationSelect, onClose]);

    const handleSelectSaved = useCallback((loc: SavedLocation) => {
        onLocationSelect({ lat: loc.lat, lon: loc.lon, name: loc.name });
        if (loc.marine !== undefined) {
            onMarineSelect(loc.marine ?? null);
        }
        if (loc.stationId !== undefined) {
            onStationSelect(loc.stationId ?? null);
        }
        onClose();
    }, [onLocationSelect, onMarineSelect, onStationSelect, onClose]);

    const handleSaveCurrent = useCallback(() => {
        if (!currentLocation || !saveName.trim()) return;
        const updated = addSavedLocation(savedLocations, {
            name: saveName.trim(),
            lat: currentLocation.lat,
            lon: currentLocation.lon,
            marine: currentMarine,
            stationId: currentStationId,
        });
        setSavedLocations(updated);
        setShowSaveForm(false);
        setSaveName('');
    }, [currentLocation, currentMarine, currentStationId, saveName, savedLocations]);

    const handleDeleteSaved = useCallback((id: string) => {
        const updated = removeSavedLocation(savedLocations, id);
        setSavedLocations(updated);
    }, [savedLocations]);

    // --- Marine actions ---

    const handleSelectMarine = useCallback((station: StationResult) => {
        onMarineSelect({ lat: station.latitude, lon: station.longitude });
    }, [onMarineSelect]);

    const handleClearMarine = useCallback(() => {
        onMarineSelect(null);
    }, [onMarineSelect]);

    // --- Observation station actions ---

    const handleSelectObsStation = useCallback((station: StationResult) => {
        onStationSelect(station.id);
    }, [onStationSelect]);

    const handleClearObsStation = useCallback(() => {
        onStationSelect(null);
    }, [onStationSelect]);

    if (!isOpen) return null;

    return (
        <div class="modal-overlay" onClick={onClose}>
            <div class="loc-manager" onClick={(e) => e.stopPropagation()}>
                {/* Header */}
                <div class="loc-manager__header">
                    <h2 class="loc-manager__title">Manage Locations</h2>
                    <button type="button" class="loc-manager__close" onClick={onClose} aria-label="Close">
                        ✕
                    </button>
                </div>

                {/* Tabs */}
                <div class="loc-manager__tabs">
                    <button
                        type="button"
                        class={`loc-manager__tab${tab === 'locations' ? ' loc-manager__tab--active' : ''}`}
                        onClick={() => setTab('locations')}
                    >
                        📍 Locations
                    </button>
                    <button
                        type="button"
                        class={`loc-manager__tab${tab === 'marine' ? ' loc-manager__tab--active' : ''}`}
                        onClick={() => setTab('marine')}
                        disabled={!currentLocation}
                    >
                        🌊 Marine
                    </button>
                    <button
                        type="button"
                        class={`loc-manager__tab${tab === 'observations' ? ' loc-manager__tab--active' : ''}`}
                        onClick={() => setTab('observations')}
                        disabled={!currentLocation}
                    >
                        📡 Station
                    </button>
                </div>

                {/* Tab content */}
                <div class="loc-manager__body">
                    {tab === 'locations' && (
                        <LocationsTab
                            savedLocations={savedLocations}
                            currentLocation={currentLocation}
                            searchQuery={searchQuery}
                            searchResults={searchResults}
                            isSearching={isSearching}
                            showSaveForm={showSaveForm}
                            saveName={saveName}
                            onSearchInput={handleSearchInput}
                            onSelectSearchResult={handleSelectSearchResult}
                            onSelectSaved={handleSelectSaved}
                            onDeleteSaved={handleDeleteSaved}
                            onShowSaveForm={() => { setShowSaveForm(true); setSaveName(currentLocation?.name ?? ''); }}
                            onSaveNameChange={setSaveName}
                            onSaveCurrent={handleSaveCurrent}
                            onCancelSave={() => setShowSaveForm(false)}
                            onDropPin={(lat, lon, name) => {
                                onLocationSelect({ lat, lon, name });
                                onClose();
                            }}
                        />
                    )}
                    {tab === 'marine' && (
                        <MarineTab
                            stations={marineStations}
                            isLoading={isLoadingStations}
                            currentMarine={currentMarine}
                            onSelectStation={handleSelectMarine}
                            onClear={handleClearMarine}
                        />
                    )}
                    {tab === 'observations' && (
                        <ObservationsTab
                            stations={obsStations}
                            isLoading={isLoadingStations}
                            currentStationId={currentStationId}
                            onSelectStation={handleSelectObsStation}
                            onClear={handleClearObsStation}
                        />
                    )}
                </div>
            </div>
        </div>
    );
}

// --- Sub-components ---

interface LocationsTabProps {
    savedLocations: SavedLocation[];
    currentLocation: { lat: number; lon: number; name: string } | null;
    searchQuery: string;
    searchResults: GeocodeResult[];
    isSearching: boolean;
    showSaveForm: boolean;
    saveName: string;
    onSearchInput: (e: Event) => void;
    onSelectSearchResult: (result: GeocodeResult) => void;
    onSelectSaved: (loc: SavedLocation) => void;
    onDeleteSaved: (id: string) => void;
    onShowSaveForm: () => void;
    onSaveNameChange: (name: string) => void;
    onSaveCurrent: () => void;
    onCancelSave: () => void;
    onDropPin: (lat: number, lon: number, name: string) => void;
}

function LocationsTab({
    savedLocations,
    currentLocation,
    searchQuery,
    searchResults,
    isSearching,
    showSaveForm,
    saveName,
    onSearchInput,
    onSelectSearchResult,
    onSelectSaved,
    onDeleteSaved,
    onShowSaveForm,
    onSaveNameChange,
    onSaveCurrent,
    onCancelSave,
    onDropPin,
}: LocationsTabProps) {
    const [pinLat, setPinLat] = useState('');
    const [pinLon, setPinLon] = useState('');
    const [pinName, setPinName] = useState('');

    const handleDropPin = () => {
        const lat = parseFloat(pinLat);
        const lon = parseFloat(pinLon);
        if (isFinite(lat) && isFinite(lon) && lat >= -90 && lat <= 90 && lon >= -180 && lon <= 180) {
            onDropPin(lat, lon, pinName.trim() || `${lat.toFixed(3)}, ${lon.toFixed(3)}`);
            setPinLat('');
            setPinLon('');
            setPinName('');
        }
    };
    return (
        <div class="loc-tab">
            {/* Current location + save button */}
            {currentLocation && (
                <div class="loc-tab__section">
                    <div class="loc-tab__section-label">Current Location</div>
                    <div class="loc-tab__current">
                        <div class="loc-tab__current-info">
                            <span class="loc-tab__current-name">{currentLocation.name}</span>
                            <span class="loc-tab__current-coords">
                                {currentLocation.lat.toFixed(3)}, {currentLocation.lon.toFixed(3)}
                            </span>
                        </div>
                        {!showSaveForm && (
                            <button
                                type="button"
                                class="loc-tab__save-btn"
                                onClick={onShowSaveForm}
                            >
                                ⭐ Save
                            </button>
                        )}
                    </div>
                    {showSaveForm && (
                        <div class="loc-tab__save-form">
                            <input
                                type="text"
                                class="loc-tab__save-input"
                                placeholder="Location name..."
                                value={saveName}
                                onInput={(e) => onSaveNameChange((e.target as HTMLInputElement).value)}
                                onKeyDown={(e) => { if (e.key === 'Enter') onSaveCurrent(); }}
                                autoFocus
                            />
                            <button
                                type="button"
                                class="loc-tab__save-confirm"
                                onClick={onSaveCurrent}
                                disabled={!saveName.trim()}
                            >
                                Save
                            </button>
                            <button
                                type="button"
                                class="loc-tab__save-cancel"
                                onClick={onCancelSave}
                            >
                                Cancel
                            </button>
                        </div>
                    )}
                </div>
            )}

            {/* Saved locations */}
            {savedLocations.length > 0 && (
                <div class="loc-tab__section">
                    <div class="loc-tab__section-label">Saved Locations</div>
                    <ul class="loc-tab__list">
                        {savedLocations.map((loc) => {
                            const isCurrent = currentLocation &&
                                Math.abs(loc.lat - currentLocation.lat) < 0.001 &&
                                Math.abs(loc.lon - currentLocation.lon) < 0.001;
                            return (
                                <li key={loc.id} class="loc-tab__item">
                                    <button
                                        type="button"
                                        class={`loc-tab__item-btn${isCurrent ? ' loc-tab__item-btn--active' : ''}`}
                                        onClick={() => onSelectSaved(loc)}
                                    >
                                        <span class="loc-tab__item-name">{loc.name}</span>
                                        <span class="loc-tab__item-coords">
                                            {loc.lat.toFixed(2)}, {loc.lon.toFixed(2)}
                                        </span>
                                    </button>
                                    <button
                                        type="button"
                                        class="loc-tab__item-delete"
                                        onClick={() => onDeleteSaved(loc.id)}
                                        aria-label={`Delete ${loc.name}`}
                                    >
                                        ✕
                                    </button>
                                </li>
                            );
                        })}
                    </ul>
                </div>
            )}

            {/* Search */}
            <div class="loc-tab__section">
                <div class="loc-tab__section-label">Search New Location</div>
                <input
                    type="text"
                    class="loc-tab__search-input"
                    placeholder="City or place name..."
                    value={searchQuery}
                    onInput={onSearchInput}
                />
                {isSearching && (
                    <div class="loc-tab__searching">Searching...</div>
                )}
                {searchResults.length > 0 && (
                    <ul class="loc-tab__list">
                        {searchResults.map((result, i) => (
                            <li key={i} class="loc-tab__item">
                                <button
                                    type="button"
                                    class="loc-tab__item-btn"
                                    onClick={() => onSelectSearchResult(result)}
                                >
                                    <span class="loc-tab__item-name">{result.name}</span>
                                    <span class="loc-tab__item-coords">
                                        {[result.admin1, result.country].filter(Boolean).join(', ')}
                                    </span>
                                </button>
                            </li>
                        ))}
                    </ul>
                )}
            </div>

            {/* Drop Pin — manual coordinates */}
            <div class="loc-tab__section">
                <div class="loc-tab__section-label">📌 Drop Pin (Coordinates)</div>
                <div class="loc-tab__pin-form">
                    <input
                        type="text"
                        class="loc-tab__pin-input"
                        placeholder="Latitude (e.g. 47.67)"
                        value={pinLat}
                        onInput={(e) => setPinLat((e.target as HTMLInputElement).value)}
                    />
                    <input
                        type="text"
                        class="loc-tab__pin-input"
                        placeholder="Longitude (e.g. -122.37)"
                        value={pinLon}
                        onInput={(e) => setPinLon((e.target as HTMLInputElement).value)}
                    />
                    <input
                        type="text"
                        class="loc-tab__pin-input loc-tab__pin-input--name"
                        placeholder="Name (optional)"
                        value={pinName}
                        onInput={(e) => setPinName((e.target as HTMLInputElement).value)}
                    />
                    <button
                        type="button"
                        class="loc-tab__save-confirm"
                        onClick={handleDropPin}
                        disabled={!pinLat || !pinLon}
                    >
                        Go
                    </button>
                </div>
            </div>
        </div>
    );
}

interface MarineTabProps {
    stations: StationResult[];
    isLoading: boolean;
    currentMarine: { lat: number; lon: number } | null;
    onSelectStation: (station: StationResult) => void;
    onClear: () => void;
}

function MarineTab({ stations, isLoading, currentMarine, onSelectStation, onClear }: MarineTabProps) {
    return (
        <div class="loc-tab">
            {/* Current marine location */}
            {currentMarine && (
                <div class="loc-tab__section">
                    <div class="loc-tab__section-label">Current Marine Location</div>
                    <div class="loc-tab__current">
                        <span class="loc-tab__current-coords">
                            {currentMarine.lat.toFixed(3)}, {currentMarine.lon.toFixed(3)}
                        </span>
                        <button type="button" class="loc-tab__clear-btn" onClick={onClear}>
                            ✕ Clear
                        </button>
                    </div>
                </div>
            )}

            {/* Station list */}
            <div class="loc-tab__section">
                <div class="loc-tab__section-label">Nearby Marine Stations</div>
                {isLoading && <div class="loc-tab__searching">Finding nearby stations...</div>}
                {!isLoading && stations.length === 0 && (
                    <div class="loc-tab__empty">No marine stations found nearby.</div>
                )}
                {!isLoading && stations.length > 0 && (
                    <ul class="loc-tab__list">
                        {stations.map((station) => {
                            const isSelected = currentMarine &&
                                Math.abs(station.latitude - currentMarine.lat) < 0.001 &&
                                Math.abs(station.longitude - currentMarine.lon) < 0.001;
                            return (
                                <li key={station.id} class="loc-tab__item">
                                    <button
                                        type="button"
                                        class={`loc-tab__item-btn${isSelected ? ' loc-tab__item-btn--active' : ''}`}
                                        onClick={() => onSelectStation(station)}
                                    >
                                        <span class="loc-tab__item-name">
                                            🌊 {station.name}
                                        </span>
                                        <span class="loc-tab__item-coords">
                                            {station.id} · {formatDistance(station.distance_km)}
                                        </span>
                                    </button>
                                </li>
                            );
                        })}
                    </ul>
                )}
            </div>
        </div>
    );
}

interface ObservationsTabProps {
    stations: StationResult[];
    isLoading: boolean;
    currentStationId: string | null;
    onSelectStation: (station: StationResult) => void;
    onClear: () => void;
}

function ObservationsTab({ stations, isLoading, currentStationId, onSelectStation, onClear }: ObservationsTabProps) {
    return (
        <div class="loc-tab">
            {/* Current station */}
            {currentStationId && (
                <div class="loc-tab__section">
                    <div class="loc-tab__section-label">Current Observation Station</div>
                    <div class="loc-tab__current">
                        <span class="loc-tab__current-name">{currentStationId}</span>
                        <button type="button" class="loc-tab__clear-btn" onClick={onClear}>
                            ✕ Use Nearest (Auto)
                        </button>
                    </div>
                </div>
            )}

            {/* Station list */}
            <div class="loc-tab__section">
                <div class="loc-tab__section-label">Nearby Observation Stations</div>
                {isLoading && <div class="loc-tab__searching">Finding nearby stations...</div>}
                {!isLoading && stations.length === 0 && (
                    <div class="loc-tab__empty">No observation stations found nearby.</div>
                )}
                {!isLoading && stations.length > 0 && (
                    <ul class="loc-tab__list">
                        {stations.map((station) => {
                            const isSelected = station.id === currentStationId;
                            return (
                                <li key={station.id} class="loc-tab__item">
                                    <button
                                        type="button"
                                        class={`loc-tab__item-btn${isSelected ? ' loc-tab__item-btn--active' : ''}`}
                                        onClick={() => onSelectStation(station)}
                                    >
                                        <span class="loc-tab__item-name">
                                            📡 {station.name}
                                        </span>
                                        <span class="loc-tab__item-coords">
                                            {station.id} · {formatDistance(station.distance_km)}
                                        </span>
                                    </button>
                                </li>
                            );
                        })}
                    </ul>
                )}
            </div>
        </div>
    );
}
