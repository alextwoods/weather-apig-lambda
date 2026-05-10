// Feature: weather-web-frontend, Property 7: Zoom level produces correct time axis range
import { describe, it, expect } from "vitest";
import * as fc from "fast-check";
import {
    ZOOM_DURATION_SECONDS,
    computeTimeRange,
    computeXScaleRange,
    getZoomDurationSeconds,
} from "../../src/charts/zoom";
import type { ZoomLevel } from "../../src/state/url-state";

/**
 * Property 7: Zoom level produces correct time axis range
 *
 * For any valid zoom level (2h, 6h, 12h, 24h) and for any positive viewport
 * width in pixels, the computed time axis range (max - min in seconds) SHALL
 * equal the zoom level's duration in seconds, ensuring the chart displays
 * exactly the selected time span per screen width.
 *
 * **Validates: Requirements 7.3, 20.2**
 */
describe("Property 7: Zoom level produces correct time axis range", () => {
    const zoomLevelArb = fc.constantFrom<ZoomLevel>("2h", "6h", "12h", "24h");
    const viewportWidthArb = fc.integer({ min: 320, max: 3840 });

    it("computeTimeRange durationSeconds equals expected zoom duration", () => {
        fc.assert(
            fc.property(zoomLevelArb, viewportWidthArb, (zoom, width) => {
                const result = computeTimeRange(zoom, width);
                const expected = ZOOM_DURATION_SECONDS[zoom];
                expect(result.durationSeconds).toBe(expected);
            }),
            { numRuns: 100 },
        );
    });

    it("computeTimeRange max - min equals zoom duration", () => {
        fc.assert(
            fc.property(zoomLevelArb, viewportWidthArb, (zoom, width) => {
                const result = computeTimeRange(zoom, width);
                const expected = ZOOM_DURATION_SECONDS[zoom];
                expect(result.max - result.min).toBe(expected);
            }),
            { numRuns: 100 },
        );
    });

    it("computeXScaleRange span equals zoom duration for any start time", () => {
        fc.assert(
            fc.property(
                zoomLevelArb,
                fc.integer({ min: 0, max: 2_000_000_000 }),
                (zoom, startTime) => {
                    const [min, max] = computeXScaleRange(zoom, startTime);
                    const expected = ZOOM_DURATION_SECONDS[zoom];
                    expect(max - min).toBe(expected);
                },
            ),
            { numRuns: 100 },
        );
    });

    it("getZoomDurationSeconds matches ZOOM_DURATION_SECONDS lookup", () => {
        fc.assert(
            fc.property(zoomLevelArb, (zoom) => {
                expect(getZoomDurationSeconds(zoom)).toBe(ZOOM_DURATION_SECONDS[zoom]);
            }),
            { numRuns: 100 },
        );
    });
});
