// Feature: weather-web-frontend, Property 2: Local storage state round-trip
import { describe, it, expect, beforeEach } from "vitest";
import * as fc from "fast-check";
import { saveState, loadState, clearState, type StoredState } from "../../src/state/local-storage";
import type { UnitPreferences } from "../../src/units/types";
import type { ZoomLevel } from "../../src/state/url-state";

/**
 * Property 2: Local storage state round-trip
 *
 * For any valid StoredState object (with arbitrary location, marine coordinates,
 * station ID, model selections, unit preferences, overlays, zoom level, and API key),
 * persisting the state to local storage and then reading it back SHALL produce a
 * StoredState equivalent to the original.
 *
 * Validates: Requirements 2.6, 3.5, 21.6
 */

// --- localStorage mock ---

let storage: Map<string, string>;

beforeEach(() => {
    storage = new Map();
    const localStorageMock = {
        getItem: (key: string): string | null => storage.get(key) ?? null,
        setItem: (key: string, value: string): void => {
            storage.set(key, value);
        },
        removeItem: (key: string): void => {
            storage.delete(key);
        },
        clear: (): void => {
            storage.clear();
        },
        get length(): number {
            return storage.size;
        },
        key: (index: number): string | null => {
            const keys = [...storage.keys()];
            return keys[index] ?? null;
        },
    };
    Object.defineProperty(globalThis, "localStorage", {
        value: localStorageMock,
        writable: true,
        configurable: true,
    });
});

// --- Generators ---

const VALID_ZOOM_LEVELS: ZoomLevel[] = ["2h", "6h", "12h", "24h"];

// Generate finite doubles that pass isFinite() validation.
// Exclude -0 since JSON.stringify(-0) === "0", so -0 cannot survive a JSON round-trip.
const noNegZero = (n: number) => (Object.is(n, -0) ? 0 : n);
const latArb = fc.double({ min: -90, max: 90, noNaN: true, noDefaultInfinity: true }).map(noNegZero);
const lonArb = fc.double({ min: -180, max: 180, noNaN: true, noDefaultInfinity: true }).map(noNegZero);

const unitPreferencesArb: fc.Arbitrary<UnitPreferences> = fc.record({
    temperature: fc.constantFrom("C" as const, "F" as const),
    wind: fc.constantFrom("kmh" as const, "mph" as const, "kts" as const, "ms" as const),
    pressure: fc.constantFrom("hPa" as const, "inHg" as const, "mmHg" as const),
    precipitation: fc.constantFrom("mm" as const, "in" as const),
    wave: fc.constantFrom("m" as const, "ft" as const),
});

const zoomArb: fc.Arbitrary<ZoomLevel> = fc.constantFrom(...VALID_ZOOM_LEVELS);

const locationArb = fc.option(
    fc.record({
        lat: latArb,
        lon: lonArb,
        name: fc.string({ minLength: 1, maxLength: 50 }),
    }),
    { nil: null },
);

const marineArb = fc.option(
    fc.record({
        lat: latArb,
        lon: lonArb,
    }),
    { nil: null },
);

const modelsArb: fc.Arbitrary<string[]> = fc.array(
    fc.constantFrom("ecmwf", "gfs", "icon", "gem", "bom"),
    { minLength: 0, maxLength: 5 },
);

const overlaysArb: fc.Arbitrary<string[]> = fc.array(
    fc.constantFrom("hrrr", "obs", "extended"),
    { minLength: 0, maxLength: 3 },
);

const storedStateArb: fc.Arbitrary<StoredState> = fc.record({
    location: locationArb,
    marine: marineArb,
    stationId: fc.option(fc.string({ minLength: 1, maxLength: 10 }), { nil: null }),
    models: modelsArb,
    units: unitPreferencesArb,
    overlays: overlaysArb,
    zoom: zoomArb,
    apiKey: fc.option(fc.string({ minLength: 1, maxLength: 40 }), { nil: null }),
});

// --- Test ---

describe("Property 2: Local storage state round-trip", () => {
    it("save → load produces equivalent StoredState", () => {
        fc.assert(
            fc.property(storedStateArb, (state) => {
                // Clear any previous state
                clearState();

                // Save and reload
                saveState(state);
                const loaded = loadState();

                // Must not be null
                expect(loaded).not.toBeNull();

                // Deep equality check
                expect(loaded).toEqual(state);
            }),
            { numRuns: 100 },
        );
    });
});
