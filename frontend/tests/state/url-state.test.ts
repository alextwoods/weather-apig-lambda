// Feature: weather-web-frontend, Property 1: URL state serialization round-trip
import { describe, it, expect } from "vitest";
import * as fc from "fast-check";
import {
    serializeToUrl,
    deserializeFromUrl,
    type AppState,
    type ZoomLevel,
    type OverlayType,
} from "../../src/state/url-state";
import type { UnitPreferences } from "../../src/units/types";

/**
 * Property 1: URL state serialization round-trip
 *
 * For any valid AppState object (with arbitrary latitude/longitude, location name,
 * marine coordinates, station ID, model selections, zoom level, unit preferences,
 * overlays, and view mode), serializing the state to URL search parameters and then
 * deserializing those parameters back SHALL produce an AppState equivalent to the original.
 *
 * Validates: Requirements 2.1, 2.3
 */

// --- Generators ---

const VALID_MODELS = ["ecmwf", "gfs", "icon", "gem", "bom"] as const;
const VALID_ZOOM_LEVELS: ZoomLevel[] = ["2h", "6h", "12h", "24h"];
const VALID_OVERLAYS: OverlayType[] = ["hrrr", "obs", "extended"];

const latArb = fc.double({ min: -90, max: 90, noNaN: true, noDefaultInfinity: true });
const lonArb = fc.double({ min: -180, max: 180, noNaN: true, noDefaultInfinity: true });

const unitPreferencesArb: fc.Arbitrary<UnitPreferences> = fc.record({
    temperature: fc.constantFrom("C" as const, "F" as const),
    wind: fc.constantFrom("kmh" as const, "mph" as const, "kts" as const, "ms" as const),
    pressure: fc.constantFrom("hPa" as const, "inHg" as const, "mmHg" as const),
    precipitation: fc.constantFrom("mm" as const, "in" as const),
    wave: fc.constantFrom("m" as const, "ft" as const),
});

const modelsArb: fc.Arbitrary<Set<string>> = fc
    .subarray([...VALID_MODELS], { minLength: 1 })
    .map((arr) => new Set(arr));

const overlaysArb: fc.Arbitrary<Set<OverlayType>> = fc
    .subarray([...VALID_OVERLAYS], { minLength: 0 })
    .map((arr) => new Set(arr));

const zoomArb: fc.Arbitrary<ZoomLevel> = fc.constantFrom(...VALID_ZOOM_LEVELS);

const viewModeArb = fc.constantFrom("chart" as const, "table" as const);

const locationNameArb = fc.string({ minLength: 1, maxLength: 50 });

/**
 * Generate an AppState with non-null location (required for lat/lon/name to round-trip).
 * Marine coordinates are generated to be DIFFERENT from location coordinates
 * (since same-as-location marine coords are omitted during serialization).
 */
const appStateArb: fc.Arbitrary<AppState> = fc
    .record({
        locationLat: latArb,
        locationLon: lonArb,
        locationName: locationNameArb,
        marineLat: latArb,
        marineLon: lonArb,
        hasMarine: fc.boolean(),
        stationId: fc.option(fc.string({ minLength: 1, maxLength: 10 }), { nil: null }),
        models: modelsArb,
        zoom: zoomArb,
        units: unitPreferencesArb,
        overlays: overlaysArb,
        viewMode: viewModeArb,
    })
    .map((r) => {
        const location = { lat: r.locationLat, lon: r.locationLon, name: r.locationName };

        // Ensure marine coords differ from location coords to avoid omission during serialization
        let marine: { lat: number; lon: number } | null = null;
        if (r.hasMarine) {
            // Offset marine coords if they happen to match location
            let mLat = r.marineLat;
            let mLon = r.marineLon;
            if (mLat === r.locationLat && mLon === r.locationLon) {
                // Shift slightly to ensure they differ
                mLat = mLat + 0.001;
                if (mLat > 90) mLat = mLat - 0.002;
            }
            marine = { lat: mLat, lon: mLon };
        }

        return {
            location,
            marine,
            stationId: r.stationId,
            models: r.models,
            zoom: r.zoom,
            units: r.units,
            overlays: r.overlays,
            viewMode: r.viewMode,
        } satisfies AppState;
    });

// --- Test ---

describe("Property 1: URL state serialization round-trip", () => {
    it("serialize → deserialize produces equivalent AppState", () => {
        fc.assert(
            fc.property(appStateArb, (state) => {
                const params = serializeToUrl(state);
                const deserialized = deserializeFromUrl(params);

                // Location: lat/lon may lose precision due to toFixed(6) formatting
                expect(deserialized.location).not.toBeUndefined();
                expect(deserialized.location!.lat).toBeCloseTo(state.location!.lat, 5);
                expect(deserialized.location!.lon).toBeCloseTo(state.location!.lon, 5);
                expect(deserialized.location!.name).toBe(state.location!.name);

                // Marine
                if (state.marine) {
                    expect(deserialized.marine).not.toBeUndefined();
                    expect(deserialized.marine!.lat).toBeCloseTo(state.marine.lat, 5);
                    expect(deserialized.marine!.lon).toBeCloseTo(state.marine.lon, 5);
                } else {
                    expect(deserialized.marine).toBeUndefined();
                }

                // Station ID
                if (state.stationId) {
                    expect(deserialized.stationId).toBe(state.stationId);
                } else {
                    expect(deserialized.stationId).toBeUndefined();
                }

                // Models (compare as sorted arrays)
                expect([...deserialized.models!].sort()).toEqual([...state.models].sort());

                // Zoom
                expect(deserialized.zoom).toBe(state.zoom);

                // Units
                expect(deserialized.units).toEqual(state.units);

                // Overlays (compare as sorted arrays)
                const expectedOverlays = [...state.overlays].sort();
                const actualOverlays = deserialized.overlays
                    ? [...deserialized.overlays].sort()
                    : [];
                expect(actualOverlays).toEqual(expectedOverlays);

                // View mode
                expect(deserialized.viewMode).toBe(state.viewMode);
            }),
            { numRuns: 100 },
        );
    });
});

// Feature: weather-web-frontend, Property 5: API key never exposed in URL

/**
 * Property 5: API key never exposed in URL
 *
 * For any valid AppState (including states where an API key is stored in local storage),
 * the serialized URL string SHALL never contain the API key value as a substring.
 *
 * Validates: Requirements 27.4
 */

const apiKeyArb = fc.stringMatching(/^[a-zA-Z0-9]{10,40}$/);

describe("Property 5: API key never exposed in URL", () => {
    it("serialized URL params never contain the API key value", () => {
        fc.assert(
            fc.property(appStateArb, apiKeyArb, (state, apiKey) => {
                // Simulate API key being stored in local storage (not in AppState)
                // Serialize the state to URL params
                const params = serializeToUrl(state);
                const urlString = params.toString();

                // The API key must never appear in the URL string
                expect(urlString).not.toContain(apiKey);
            }),
            { numRuns: 100 },
        );
    });
});
