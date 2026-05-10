import { useState, useEffect, useCallback } from 'preact/hooks';
import type { WeatherApiClient } from '../api/client';
import type { StationResult } from '../api/types';

export interface StationSelectorProps {
    type: 'observations' | 'marine';
    lat: number;
    lon: number;
    selectedStationId: string | null;
    onStationSelect: (stationId: string) => void;
    apiClient: WeatherApiClient;
}

/**
 * Station Selector component.
 * Fetches and displays nearby stations (NWS observation or NOAA marine)
 * sorted by distance. Highlights the currently selected station and
 * calls onStationSelect when the user picks a station.
 */
export function StationSelector({
    type,
    lat,
    lon,
    selectedStationId,
    onStationSelect,
    apiClient,
}: StationSelectorProps) {
    const [stations, setStations] = useState<StationResult[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);

    const fetchStations = useCallback(async () => {
        setIsLoading(true);
        setError(null);

        try {
            const results = type === 'observations'
                ? await apiClient.observationStations(lat, lon)
                : await apiClient.marineStations(lat, lon);

            // Sort by distance (API should return sorted, but ensure it)
            const sorted = [...results].sort((a, b) => a.distance_km - b.distance_km);
            setStations(sorted);
        } catch {
            setError('Failed to load stations');
            setStations([]);
        } finally {
            setIsLoading(false);
        }
    }, [type, lat, lon, apiClient]);

    // Fetch stations on mount and when lat/lon/type changes
    useEffect(() => {
        fetchStations();
    }, [fetchStations]);

    const title = type === 'observations'
        ? 'Observation Stations'
        : 'Marine Stations';

    return (
        <div class="station-selector">
            <div class="station-selector__header">
                <h3 class="station-selector__title">{title}</h3>
            </div>
            <div class="station-selector__body">
                {isLoading && (
                    <div class="station-selector__loading" aria-live="polite">
                        Loading stations...
                    </div>
                )}
                {error && (
                    <div class="station-selector__error" role="alert">
                        {error}
                    </div>
                )}
                {!isLoading && !error && stations.length === 0 && (
                    <div class="station-selector__empty">
                        No stations found nearby.
                    </div>
                )}
                {!isLoading && stations.length > 0 && (
                    <ul class="station-selector__list" role="listbox" aria-label={title}>
                        {stations.map((station) => {
                            const isSelected = station.id === selectedStationId;
                            return (
                                <li key={station.id} role="option" aria-selected={isSelected}>
                                    <button
                                        type="button"
                                        class={`station-selector__item${isSelected ? ' station-selector__item--selected' : ''}`}
                                        onClick={() => onStationSelect(station.id)}
                                    >
                                        <span class="station-selector__item-name">
                                            {station.name}
                                        </span>
                                        <span class="station-selector__item-meta">
                                            <span class="station-selector__item-id">
                                                {station.id}
                                            </span>
                                            <span class="station-selector__item-distance">
                                                {formatDistance(station.distance_km)}
                                            </span>
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

/**
 * Formats a distance in km to a human-readable string.
 * Shows one decimal place for distances under 10km, otherwise rounds to integer.
 */
function formatDistance(km: number): string {
    if (km < 10) {
        return `${km.toFixed(1)} km`;
    }
    return `${Math.round(km)} km`;
}
