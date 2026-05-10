import { useState, useCallback, useRef, useEffect } from 'preact/hooks';
import { WeatherApiClient, ApiError } from './api/client';
import type { ForecastParams } from './api/endpoints';
import type { OverlayType, ZoomLevel } from './state/url-state';
import { AppStore, useCreateAppStore } from './state/app-store';
import { loadState, saveState } from './state/local-storage';

import { LocationSearch } from './components/location-search';
import { ModelToggle } from './components/model-toggle';
import { OverlayToggle } from './components/overlay-toggle';
import { ZoomPicker } from './components/zoom-picker';
import { RefreshControls } from './components/refresh-controls';
import { ApiKeyPrompt } from './components/api-key-prompt';
import { LoadingIndicator } from './components/loading-indicator';
import { SettingsPanel } from './components/settings-panel';

import { HudPanel } from './panels/hud-panel';
import { TemperaturePanel } from './panels/temperature-panel';
import { WindPanel } from './panels/wind-panel';
import { AtmosphericPanel } from './panels/atmospheric-panel';
import { PrecipitationPanel } from './panels/precipitation-panel';
import { SolarPanel } from './panels/solar-panel';
import { AirQualityPanel } from './panels/air-quality-panel';
import { PressurePanel } from './panels/pressure-panel';
import { MarinePanel } from './panels/marine-panel';
import { DataTable } from './panels/data-table';

/** Load API key from local storage on startup. */
function loadApiKey(): string | null {
    const stored = loadState();
    return stored?.apiKey ?? null;
}

/**
 * Top-level App component.
 * Composes the full UI: Header, MainContent (HUD + chart panels or data table),
 * Settings panel, API key prompt, and loading indicator.
 *
 * Wires the initialization flow (URL → local storage → search prompt)
 * and forecast fetch on location selection, model toggle, and manual refresh.
 *
 * Validates: Requirements 2.3, 2.4, 2.5, 2.7, 4.7, 27.3
 */
export function App() {
    const [apiKey, setApiKey] = useState<string | null>(loadApiKey);
    const [apiClient] = useState(() => new WeatherApiClient(apiKey));
    const store = useCreateAppStore(apiKey);
    const { state, dispatch } = store;

    const [settingsOpen, setSettingsOpen] = useState(false);

    // Keep a ref to the latest state for use in async callbacks
    const stateRef = useRef(state);
    stateRef.current = state;

    // Keep apiClient in sync with apiKey changes
    useEffect(() => {
        if (apiKey) {
            apiClient.setApiKey(apiKey);
        }
    }, [apiKey, apiClient]);

    // --- Forecast fetching ---

    const fetchForecast = useCallback(
        async (options?: { forceRefresh?: boolean; refreshSource?: string }) => {
            const currentState = stateRef.current;
            const location = currentState.appState.location;
            if (!location) return;

            dispatch({ type: 'SET_LOADING', payload: true });

            const params: ForecastParams = {
                lat: location.lat,
                lon: location.lon,
                models: [...currentState.appState.models],
            };

            if (currentState.appState.marine) {
                params.marine_lat = currentState.appState.marine.lat;
                params.marine_lon = currentState.appState.marine.lon;
            }

            if (currentState.appState.stationId) {
                params.station_id = currentState.appState.stationId;
            }

            if (options?.forceRefresh) {
                params.force_refresh = true;
            }

            if (options?.refreshSource) {
                params.refresh_source = options.refreshSource;
            }

            try {
                const data = await apiClient.forecast(params);
                dispatch({ type: 'SET_FORECAST_DATA', payload: data });
                dispatch({ type: 'SET_ERROR', payload: null });
            } catch (err) {
                if (err instanceof ApiError && err.status === 403) {
                    // 403 → show API key prompt, retain previous data
                    dispatch({ type: 'SET_NEEDS_API_KEY', payload: true });
                } else {
                    // Network or other error → show error, retain previous data
                    const message = err instanceof Error ? err.message : 'Failed to fetch forecast';
                    dispatch({ type: 'SET_ERROR', payload: message });
                }
            } finally {
                dispatch({ type: 'SET_LOADING', payload: false });
            }
        },
        [apiClient, dispatch],
    );

    // Fetch on initial load if we have a location
    useEffect(() => {
        if (state.appState.location && !state.forecastData) {
            fetchForecast();
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    // --- Event handlers ---

    const handleLocationSelect = useCallback(
        (location: { lat: number; lon: number; name: string }) => {
            dispatch({ type: 'SET_LOCATION', payload: location });
            // Fetch immediately after location selection
            // Use setTimeout to ensure state is updated before fetch reads it
            setTimeout(() => fetchForecast(), 0);
        },
        [dispatch, fetchForecast],
    );

    const handleModelsChange = useCallback(
        (models: Set<string>) => {
            dispatch({ type: 'SET_MODELS', payload: models });
            // ModelToggle already debounces by 300ms before calling this,
            // so we can fetch immediately here
            setTimeout(() => fetchForecast(), 0);
        },
        [dispatch, fetchForecast],
    );

    const handleOverlaysChange = useCallback(
        (overlays: Set<OverlayType>) => {
            dispatch({ type: 'SET_OVERLAYS', payload: overlays });
        },
        [dispatch],
    );

    const handleZoomChange = useCallback(
        (zoom: ZoomLevel) => {
            dispatch({ type: 'SET_ZOOM', payload: zoom });
        },
        [dispatch],
    );

    const handleFullRefresh = useCallback(() => {
        fetchForecast({ forceRefresh: true });
    }, [fetchForecast]);

    const handleSourceRefresh = useCallback(
        (source: string) => {
            fetchForecast({ refreshSource: source });
        },
        [fetchForecast],
    );

    const handleApiKeySubmit = useCallback(
        (key: string) => {
            setApiKey(key);
            apiClient.setApiKey(key);
            dispatch({ type: 'SET_NEEDS_API_KEY', payload: false });

            // Persist the API key to local storage
            const stored = loadState();
            if (stored) {
                saveState({ ...stored, apiKey: key });
            }

            // Re-fetch with the new key
            setTimeout(() => fetchForecast(), 0);
        },
        [apiClient, dispatch, fetchForecast],
    );

    const handleApiKeyClose = useCallback(() => {
        dispatch({ type: 'SET_NEEDS_API_KEY', payload: false });
    }, [dispatch]);

    const handleUnitsChange = useCallback(
        (units: import('./units/types').UnitPreferences) => {
            dispatch({ type: 'SET_UNITS', payload: units });
        },
        [dispatch],
    );

    // Determine available data sources for refresh controls
    const cacheSources = state.forecastData
        ? Object.keys(state.forecastData.cache)
        : [];

    // Determine overlay availability from forecast data
    const hrrrAvailable = !!state.forecastData?.hrrr;
    const observationsAvailable = !!state.forecastData?.observations;

    return (
        <AppStore.Provider value={store}>
            <div class="app">
                {/* Header bar */}
                <header class="header">
                    <LocationSearch
                        onLocationSelect={handleLocationSelect}
                        apiClient={apiClient}
                        onApiKeyNeeded={() => dispatch({ type: 'SET_NEEDS_API_KEY', payload: true })}
                    />
                    <div class="header__controls">
                        <ModelToggle
                            enabledModels={state.appState.models}
                            onModelsChange={handleModelsChange}
                        />
                        <OverlayToggle
                            activeOverlays={state.appState.overlays}
                            onOverlaysChange={handleOverlaysChange}
                            hrrrAvailable={hrrrAvailable}
                            observationsAvailable={observationsAvailable}
                        />
                        <ZoomPicker
                            zoom={state.appState.zoom}
                            onZoomChange={handleZoomChange}
                        />
                        <RefreshControls
                            onFullRefresh={handleFullRefresh}
                            onSourceRefresh={handleSourceRefresh}
                            isLoading={state.isLoading}
                            sources={cacheSources}
                        />
                    </div>
                    <button
                        type="button"
                        class="toggle-bar__button"
                        onClick={() => setSettingsOpen(true)}
                        aria-label="Open settings"
                    >
                        ⚙
                    </button>
                </header>

                {/* Main content area */}
                <main class="main">
                    <div class="content">
                        {/* Loading indicator for initial load */}
                        {state.isLoading && !state.forecastData && (
                            <LoadingIndicator isLoading={true} message="Loading forecast..." />
                        )}

                        {/* Error display */}
                        {state.error && (
                            <div class="panel" role="alert">
                                <div class="panel__header">Error</div>
                                <div class="panel__body">{state.error}</div>
                            </div>
                        )}

                        {/* Location search prompt */}
                        {state.needsLocationSearch && !state.forecastData && (
                            <div class="panel">
                                <div class="panel__header">Welcome</div>
                                <div class="panel__body">
                                    Search for a location to view the forecast.
                                </div>
                            </div>
                        )}

                        {/* Forecast content */}
                        {state.forecastData && (
                            <div class="panels">
                                {state.appState.viewMode === 'chart' ? (
                                    <>
                                        <HudPanel
                                            forecast={state.forecastData}
                                            units={state.appState.units}
                                        />
                                        <TemperaturePanel
                                            forecast={state.forecastData}
                                            units={state.appState.units}
                                            overlays={state.appState.overlays}
                                            zoom={state.appState.zoom}
                                        />
                                        <WindPanel
                                            forecast={state.forecastData}
                                            units={state.appState.units}
                                            overlays={state.appState.overlays}
                                            zoom={state.appState.zoom}
                                        />
                                        <AtmosphericPanel
                                            forecast={state.forecastData}
                                            units={state.appState.units}
                                            overlays={state.appState.overlays}
                                            zoom={state.appState.zoom}
                                        />
                                        <PrecipitationPanel
                                            forecast={state.forecastData}
                                            units={state.appState.units}
                                            overlays={state.appState.overlays}
                                            zoom={state.appState.zoom}
                                        />
                                        <SolarPanel
                                            forecast={state.forecastData}
                                            units={state.appState.units}
                                            overlays={state.appState.overlays}
                                            zoom={state.appState.zoom}
                                        />
                                        <AirQualityPanel
                                            forecast={state.forecastData}
                                            units={state.appState.units}
                                            overlays={state.appState.overlays}
                                            zoom={state.appState.zoom}
                                        />
                                        <PressurePanel
                                            forecast={state.forecastData}
                                            units={state.appState.units}
                                            overlays={state.appState.overlays}
                                            zoom={state.appState.zoom}
                                        />
                                        <MarinePanel
                                            forecast={state.forecastData}
                                            units={state.appState.units}
                                            overlays={state.appState.overlays}
                                            zoom={state.appState.zoom}
                                        />
                                    </>
                                ) : (
                                    <DataTable
                                        forecast={state.forecastData}
                                        units={state.appState.units}
                                    />
                                )}
                            </div>
                        )}
                    </div>
                </main>

                {/* Settings panel */}
                <SettingsPanel
                    isOpen={settingsOpen}
                    onClose={() => setSettingsOpen(false)}
                    units={state.appState.units}
                    onUnitsChange={handleUnitsChange}
                />

                {/* API key prompt modal */}
                <ApiKeyPrompt
                    isOpen={state.needsApiKey}
                    onSubmit={handleApiKeySubmit}
                    onClose={handleApiKeyClose}
                />

                {/* Subtle loading indicator during refresh (when data already exists) */}
                {state.isLoading && state.forecastData && (
                    <LoadingIndicator isLoading={true} message="Refreshing..." />
                )}
            </div>
        </AppStore.Provider>
    );
}
