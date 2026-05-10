// Feature: weather-web-frontend, Property 3: Unit conversion round-trip
import { describe, it, expect } from "vitest";
import * as fc from "fast-check";
import {
    convertTemp,
    convertTempInverse,
    convertWind,
    convertWindInverse,
    convertPressure,
    convertPressureInverse,
    convertPrecip,
    convertPrecipInverse,
    convertWave,
    convertWaveInverse,
} from "../../src/units/converter";

/**
 * Property 3: Unit conversion round-trip
 *
 * For any valid numeric input value and for any supported unit type,
 * converting from the metric base unit to the target display unit and
 * then converting back SHALL produce a value within 0.01 of the original input.
 *
 * Validates: Requirements 22.1, 22.2, 22.3, 22.4, 22.5, 22.6
 */
describe("Property 3: Unit conversion round-trip", () => {
    it("temperature: Celsius → Fahrenheit → Celsius round-trip", () => {
        fc.assert(
            fc.property(
                fc.double({ min: -100, max: 100, noNaN: true }),
                (celsius) => {
                    const converted = convertTemp(celsius, "F");
                    const roundTripped = convertTempInverse(converted, "F");
                    expect(roundTripped).toBeCloseTo(celsius, 2);
                },
            ),
            { numRuns: 100 },
        );
    });

    it("wind: km/h → mph → km/h round-trip", () => {
        fc.assert(
            fc.property(
                fc.double({ min: 0, max: 100, noNaN: true }),
                (kmh) => {
                    const converted = convertWind(kmh, "mph");
                    const roundTripped = convertWindInverse(converted, "mph");
                    expect(roundTripped).toBeCloseTo(kmh, 2);
                },
            ),
            { numRuns: 100 },
        );
    });

    it("wind: km/h → knots → km/h round-trip", () => {
        fc.assert(
            fc.property(
                fc.double({ min: 0, max: 100, noNaN: true }),
                (kmh) => {
                    const converted = convertWind(kmh, "kts");
                    const roundTripped = convertWindInverse(converted, "kts");
                    expect(roundTripped).toBeCloseTo(kmh, 2);
                },
            ),
            { numRuns: 100 },
        );
    });

    it("wind: km/h → m/s → km/h round-trip", () => {
        fc.assert(
            fc.property(
                fc.double({ min: 0, max: 100, noNaN: true }),
                (kmh) => {
                    const converted = convertWind(kmh, "ms");
                    const roundTripped = convertWindInverse(converted, "ms");
                    expect(roundTripped).toBeCloseTo(kmh, 2);
                },
            ),
            { numRuns: 100 },
        );
    });

    it("pressure: hPa → inHg → hPa round-trip", () => {
        fc.assert(
            fc.property(
                fc.double({ min: 900, max: 1100, noNaN: true }),
                (hpa) => {
                    const converted = convertPressure(hpa, "inHg");
                    const roundTripped = convertPressureInverse(converted, "inHg");
                    expect(roundTripped).toBeCloseTo(hpa, 2);
                },
            ),
            { numRuns: 100 },
        );
    });

    it("pressure: hPa → mmHg → hPa round-trip", () => {
        fc.assert(
            fc.property(
                fc.double({ min: 900, max: 1100, noNaN: true }),
                (hpa) => {
                    const converted = convertPressure(hpa, "mmHg");
                    const roundTripped = convertPressureInverse(converted, "mmHg");
                    expect(roundTripped).toBeCloseTo(hpa, 2);
                },
            ),
            { numRuns: 100 },
        );
    });

    it("precipitation: mm → inches → mm round-trip", () => {
        fc.assert(
            fc.property(
                fc.double({ min: 0, max: 100, noNaN: true }),
                (mm) => {
                    const converted = convertPrecip(mm, "in");
                    const roundTripped = convertPrecipInverse(converted, "in");
                    expect(roundTripped).toBeCloseTo(mm, 2);
                },
            ),
            { numRuns: 100 },
        );
    });

    it("wave: meters → feet → meters round-trip", () => {
        fc.assert(
            fc.property(
                fc.double({ min: 0, max: 100, noNaN: true }),
                (meters) => {
                    const converted = convertWave(meters, "ft");
                    const roundTripped = convertWaveInverse(converted, "ft");
                    expect(roundTripped).toBeCloseTo(meters, 2);
                },
            ),
            { numRuns: 100 },
        );
    });
});
