// Feature: weather-web-frontend, Property 6: AQI category mapping completeness
import { describe, it, expect } from "vitest";
import * as fc from "fast-check";
import { getAqiCategory } from "../../src/units/aqi";

/**
 * Property 6: AQI category mapping completeness
 *
 * For any integer AQI value in the range [0, 500], the EPA category mapping
 * function SHALL return exactly one valid category with the correct color,
 * and the category boundaries SHALL be mutually exclusive and collectively
 * exhaustive over the range.
 *
 * **Validates: Requirements 5.5, 13.1, 13.3**
 */

const VALID_CATEGORIES = [
    "Good",
    "Moderate",
    "Unhealthy for Sensitive Groups",
    "Unhealthy",
    "Very Unhealthy",
    "Hazardous",
] as const;

const VALID_COLORS = [
    "#00e400",
    "#ffff00",
    "#ff7e00",
    "#ff0000",
    "#8f3f97",
    "#7e0023",
] as const;

const CATEGORY_COLOR_MAP: Record<string, string> = {
    "Good": "#00e400",
    "Moderate": "#ffff00",
    "Unhealthy for Sensitive Groups": "#ff7e00",
    "Unhealthy": "#ff0000",
    "Very Unhealthy": "#8f3f97",
    "Hazardous": "#7e0023",
};

describe("Property 6: AQI category mapping completeness", () => {
    it("returns exactly one valid category for any AQI value in [0, 500]", () => {
        fc.assert(
            fc.property(
                fc.integer({ min: 0, max: 500 }),
                (aqi) => {
                    const result = getAqiCategory(aqi);

                    // Exactly one valid category is returned
                    expect(VALID_CATEGORIES).toContain(result.category);

                    // Exactly one valid color is returned
                    expect(VALID_COLORS).toContain(result.color);

                    // The category-color pairing is correct
                    expect(result.color).toBe(CATEGORY_COLOR_MAP[result.category]);
                },
            ),
            { numRuns: 100 },
        );
    });

    it("boundaries are mutually exclusive and collectively exhaustive", () => {
        fc.assert(
            fc.property(
                fc.integer({ min: 0, max: 500 }),
                (aqi) => {
                    const result = getAqiCategory(aqi);

                    // Verify the correct category is assigned based on EPA breakpoints
                    if (aqi <= 50) {
                        expect(result.category).toBe("Good");
                    } else if (aqi <= 100) {
                        expect(result.category).toBe("Moderate");
                    } else if (aqi <= 150) {
                        expect(result.category).toBe("Unhealthy for Sensitive Groups");
                    } else if (aqi <= 200) {
                        expect(result.category).toBe("Unhealthy");
                    } else if (aqi <= 300) {
                        expect(result.category).toBe("Very Unhealthy");
                    } else {
                        expect(result.category).toBe("Hazardous");
                    }
                },
            ),
            { numRuns: 100 },
        );
    });
});
