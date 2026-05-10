import uPlot from 'uplot';

/**
 * Shared sync key used by all chart panels.
 * uPlot instances that share the same sync key will have their
 * crosshairs synchronized — hovering one panel shows the crosshair
 * at the same time position on all other panels.
 */
export const CHART_SYNC_KEY = 'weather-charts';

/**
 * Returns a uPlot sync instance for the shared chart group.
 * All chart panels should use this same sync instance so that
 * cursor position is coordinated across panels.
 *
 * uPlot.sync() is idempotent for a given key — calling it multiple
 * times with the same key returns the same sync group.
 */
export function getChartSync(): uPlot.SyncPubSub {
    return uPlot.sync(CHART_SYNC_KEY);
}
