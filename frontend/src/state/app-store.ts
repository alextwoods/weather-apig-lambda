import { createContext } from 'preact';
import { useContext, useReducer, useEffect, useRef } from 'preact/hooks';
import type { ForecastResponse } from '../api/types';
import type { AppState, OverlayType, ZoomLevel } from './url-state';
import { deserializeFromUrl, pushState } from './url-state';
import { loadState, saveState, type StoredState } from './local-storage';
import type { UnitPreferences } from '../units/types';

// --- Store state shape ---

export interface StoreState {
    /** Core view state (serialized to URL) */
    appState: AppState;
    /** Fetched forecast data */
    forecastData: ForecastResponse | null;
    /** Whether a fetch is in progress */
    isLoading: boolean;
    /** Error message from last failed operation */
    error: string | null;
    /** Whether the location search prompt should be shown */
    needsLocationSearch: boolean;
}

// --- Actions ---

export type StoreAction =
    | { type: 'SET_LOCATION'; payload: { lat: number; lon: number; name: string } }
    | { type: 'SET_MARINE'; payload: { lat: number; lon: number } | null }
    | { type: 'SET_STATION_ID'; payload: string | null }
    | { type: 'SET_MODELS'; payload: Set<string> }
    | { type: 'SET_ZOOM'; payload: ZoomLevel }
    | { type: 'SET_UNITS'; payload: UnitPreferences }
    | { type: 'SET_OVERLAYS'; payload: Set<OverlayType> }
    | { type: 'SET_VIEW_MODE'; payload: 'chart' | 'table' }
    | { type: 'SET_FORECAST_DATA'; payload: ForecastResponse }
    | { type: 'SET_LOADING'; payload: boolean }
    | { type: 'SET_ERROR'; payload: string | null }
    | { type: 'SET_NEEDS_LOCATION_SEARCH'; payload: boolean }
    | { type: 'SET_APP_STATE'; payload: AppState };

// --- Default state ---

export const DEFAULT_UNITS: UnitPreferences = {
    temperature: 'C',
    wind: 'kmh',
    pressure: 'hPa',
    precipitation: 'mm',
    wave: 'm',
};

export const DEFAULT_APP_STATE: AppState = {
    location: null,
    marine: null,
    stationId: null,
    models: new Set(['ecmwf', 'gfs', 'icon', 'gem', 'bom']),
    zoom: '5d',
    units: DEFAULT_UNITS,
    overlays: new Set<OverlayType>(),
    viewMode: 'chart',
};

export const INITIAL_STORE_STATE: StoreState = {
    appState: DEFAULT_APP_STATE,
    forecastData: null,
    isLoading: false,
    error: null,
    needsLocationSearch: false,
};

// --- Reducer ---

export function storeReducer(state: StoreState, action: StoreAction): StoreState {
    switch (action.type) {
        case 'SET_LOCATION':
            return {
                ...state,
                appState: { ...state.appState, location: action.payload },
                needsLocationSearch: false,
            };
        case 'SET_MARINE':
            return {
                ...state,
                appState: { ...state.appState, marine: action.payload },
            };
        case 'SET_STATION_ID':
            return {
                ...state,
                appState: { ...state.appState, stationId: action.payload },
            };
        case 'SET_MODELS':
            return {
                ...state,
                appState: { ...state.appState, models: action.payload },
            };
        case 'SET_ZOOM':
            return {
                ...state,
                appState: { ...state.appState, zoom: action.payload },
            };
        case 'SET_UNITS':
            return {
                ...state,
                appState: { ...state.appState, units: action.payload },
            };
        case 'SET_OVERLAYS':
            return {
                ...state,
                appState: { ...state.appState, overlays: action.payload },
            };
        case 'SET_VIEW_MODE':
            return {
                ...state,
                appState: { ...state.appState, viewMode: action.payload },
            };
        case 'SET_FORECAST_DATA':
            return { ...state, forecastData: action.payload, error: null };
        case 'SET_LOADING':
            return { ...state, isLoading: action.payload };
        case 'SET_ERROR':
            return { ...state, error: action.payload };
        case 'SET_NEEDS_LOCATION_SEARCH':
            return { ...state, needsLocationSearch: action.payload };
        case 'SET_APP_STATE':
            return { ...state, appState: action.payload };
        default:
            return state;
    }
}

// --- Initialization logic ---

/**
 * Determines the initial store state by checking URL params first,
 * then falling back to local storage, then showing the search prompt.
 *
 * Precedence: URL params > local storage > show search prompt
 */
export function initializeState(): StoreState {
    // 1. Try URL params first
    const urlParams = new URLSearchParams(window.location.search);
    const urlState = deserializeFromUrl(urlParams);

    // If URL has a location, it takes full precedence
    if (urlState.location) {
        const appState: AppState = {
            ...DEFAULT_APP_STATE,
            ...urlState,
            // Ensure Sets are properly initialized from partial
            models: urlState.models ?? DEFAULT_APP_STATE.models,
            overlays: urlState.overlays ?? DEFAULT_APP_STATE.overlays,
        };
        return {
            ...INITIAL_STORE_STATE,
            appState,
        };
    }

    // 2. Fall back to local storage
    const stored = loadState();
    if (stored && stored.location) {
        const appState = storedStateToAppState(stored);
        return {
            ...INITIAL_STORE_STATE,
            appState,
        };
    }

    // 3. No URL params and no local storage — show location search prompt
    return {
        ...INITIAL_STORE_STATE,
        needsLocationSearch: true,
    };
}

/**
 * Converts a StoredState (from local storage) into an AppState.
 * StoredState uses arrays for models/overlays; AppState uses Sets.
 */
function storedStateToAppState(stored: StoredState): AppState {
    // Migrate legacy zoom levels to the new default
    const validZoomLevels = new Set(['3d', '5d', '7d', '10d']);
    const zoom: ZoomLevel = validZoomLevels.has(stored.zoom) ? stored.zoom : DEFAULT_APP_STATE.zoom;

    return {
        location: stored.location,
        marine: stored.marine,
        stationId: stored.stationId,
        models: new Set(stored.models.length > 0 ? stored.models : ['ecmwf', 'gfs', 'icon', 'gem', 'bom']),
        zoom,
        units: stored.units,
        overlays: new Set(stored.overlays as OverlayType[]),
        viewMode: 'chart',
    };
}

/**
 * Converts the current AppState into a StoredState for local storage persistence.
 */
function appStateToStoredState(appState: AppState): StoredState {
    return {
        location: appState.location,
        marine: appState.marine,
        stationId: appState.stationId,
        models: [...appState.models],
        units: appState.units,
        overlays: [...appState.overlays],
        zoom: appState.zoom,
    };
}

// --- Context ---

export interface AppStoreContext {
    state: StoreState;
    dispatch: (action: StoreAction) => void;
}

export const AppStore = createContext<AppStoreContext>({
    state: INITIAL_STORE_STATE,
    dispatch: () => { },
});

// --- Hook ---

/**
 * Custom hook that provides the current store state and dispatch function.
 * Also provides convenience methods for common state updates that
 * automatically sync to URL and local storage.
 */
export function useAppStore() {
    const { state, dispatch } = useContext(AppStore);
    return { state, dispatch };
}

/**
 * Hook that initializes the store with useReducer and wires up
 * URL and local storage synchronization. Use this in the top-level
 * App component to create the store provider value.
 */
export function useCreateAppStore(): AppStoreContext {
    const [state, dispatch] = useReducer(storeReducer, undefined, initializeState);

    // Sync appState changes to URL and local storage
    const prevAppStateRef = useRef(state.appState);
    useEffect(() => {
        if (prevAppStateRef.current !== state.appState) {
            prevAppStateRef.current = state.appState;

            // Update URL
            pushState(state.appState);

            // Persist to local storage
            saveState(appStateToStoredState(state.appState));
        }
    }, [state.appState]);

    // On initial load, if we restored from local storage, update the URL
    useEffect(() => {
        if (state.appState.location && !window.location.search) {
            pushState(state.appState);
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, []);

    return { state, dispatch };
}
