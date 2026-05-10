import type { ZoomLevel } from '../state/url-state';

// --- Zoom level duration mapping (seconds) ---

export const ZOOM_DURATION_SECONDS: Record<ZoomLevel, number> = {
    '2h': 7200,
    '6h': 21600,
    '12h': 43200,
    '24h': 86400,
};

/**
 * Returns the duration in seconds for a given zoom level.
 */
export function getZoomDurationSeconds(zoomLevel: ZoomLevel): number {
    return ZOOM_DURATION_SECONDS[zoomLevel];
}

/**
 * Computes the time range (in seconds) that should be displayed for the given zoom level.
 *
 * The `durationSeconds` is the zoom level's duration (independent of viewport width).
 * `min` and `max` represent the initial visible window starting from 0.
 * Used to configure the uPlot x-axis scale.
 */
export function computeTimeRange(
    zoomLevel: ZoomLevel,
    _viewportWidth: number,
): { min: number; max: number; durationSeconds: number } {
    const durationSeconds = ZOOM_DURATION_SECONDS[zoomLevel];
    return {
        min: 0,
        max: durationSeconds,
        durationSeconds,
    };
}

/**
 * Given a zoom level and a start time (unix seconds), returns [min, max] for the x-axis scale.
 * max = startTime + zoomDurationSeconds
 *
 * Used to set the uPlot x-axis scale range for horizontal panning within the forecast time range.
 */
export function computeXScaleRange(
    zoomLevel: ZoomLevel,
    startTime: number,
): [number, number] {
    const durationSeconds = ZOOM_DURATION_SECONDS[zoomLevel];
    return [startTime, startTime + durationSeconds];
}
