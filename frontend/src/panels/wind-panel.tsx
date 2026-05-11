import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { convertWind } from '../units/converter';
import { buildMultiFanChartData, buildMultiFanChartOptions, type MultiFanChartConfig } from '../charts/fan-chart';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';
import { VARIABLE_COLORS } from '../charts/colors';
import { timesToUnixSeconds, parseUtcMs } from '../api/time-utils';
import uPlot from 'uplot';

export interface WindPanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/** Wind unit label for axis display. */
function windUnitLabel(unit: UnitPreferences['wind']): string {
    switch (unit) {
        case 'kmh': return 'km/h';
        case 'mph': return 'mph';
        case 'kts': return 'kn';
        case 'ms': return 'm/s';
    }
}

/** Convert degrees to a wind arrow character (meteorological convention: direction wind blows TOWARD). */
function windArrow(degrees: number): string {
    // Add 180 for meteorological convention (direction wind is blowing toward)
    const adjusted = (degrees + 180) % 360;
    const index = Math.round(adjusted / 45) % 8;
    const arrows = ['↑', '↗', '→', '↘', '↓', '↙', '←', '↖'];
    return arrows[index];
}

/**
 * Wind Panel.
 * Displays a single chart with wind speed (green) and wind gusts (red) overlaid.
 * Gusts are rendered behind speed. Wind direction arrows shown every 6 time steps.
 * Includes optional HRRR overlay lines and observation markers.
 *
 * Colors: Speed=green, Gusts=red
 * HRRR: dashed green (speed) and dashed red (gusts) at 0.5 opacity
 * Observations: yellow dots
 */
export function WindPanel({ forecast, units, overlays, zoom }: WindPanelProps) {
    const { ensemble, hrrr, observations, astronomy } = forecast;
    const speedStats = ensemble.statistics.wind_speed_10m;

    if (!speedStats) return null;

    const gustStats = ensemble.statistics.wind_gusts_10m;
    const directionStats = ensemble.statistics.wind_direction_10m;

    const times = timesToUnixSeconds(ensemble.times);
    const unitConverter = (v: number) => convertWind(v, units.wind);
    const unitLabel = windUnitLabel(units.wind);

    // Build multi-series: gusts first (behind), then speed (in front)
    const seriesConfigs: MultiFanChartConfig['series'] = [];
    if (gustStats) {
        seriesConfigs.push({ stats: gustStats, color: 'windGusts', label: 'Gusts' });
    }
    seriesConfigs.push({ stats: speedStats, color: 'windSpeed', label: 'Speed' });

    // HRRR overlays
    const hrrrLines: MultiFanChartConfig['hrrr'] = [];
    if (overlays.has('hrrr') && hrrr?.wind_speed_10m) {
        hrrrLines.push({
            data: hrrr.wind_speed_10m,
            color: 'rgba(74, 222, 128, 0.5)',
            label: 'HRRR Speed',
        });
    }
    if (overlays.has('hrrr') && hrrr?.wind_gusts_10m) {
        hrrrLines.push({
            data: hrrr.wind_gusts_10m,
            color: 'rgba(248, 113, 113, 0.5)',
            label: 'HRRR Gusts',
        });
    }

    // Observation markers (wind speed)
    const obsMarkers = overlays.has('obs') && observations?.entries
        ? observations.entries
            .filter(e => e.wind_speed_kmh != null)
            .map(e => ({
                time: parseUtcMs(e.timestamp) / 1000 | 0,
                value: e.wind_speed_kmh!,
            }))
        : undefined;

    const config: MultiFanChartConfig = {
        times,
        series: seriesConfigs,
        hrrr: hrrrLines.length > 0 ? hrrrLines : undefined,
        observations: obsMarkers,
        sunAltitude: astronomy?.sun_altitude,
        unitConverter,
        axisLabel: `Wind (${unitLabel})`,
        syncKey: CHART_SYNC_KEY,
        zoomLevel: zoom,
        height: 220,
    };

    const data = buildMultiFanChartData(config);
    const options = buildMultiFanChartOptions(config);

    // Wind direction arrows drawn on the speed median line via draw hook
    const directionMedian = directionStats?.median ?? null;

    // Determine the speed median series index in the data array
    // Layout: [times, ...gusts(5 if present), ...speed(5)]
    // Speed median is at index: gustStats ? 8 : 3
    const speedMedianIdx = gustStats ? 8 : 3;

    if (directionMedian && directionMedian.length > 0) {
        const speedMedianData = data[speedMedianIdx] as (number | null | undefined)[];
        const arrowHook = (u: uPlot) => {
            const ctx = u.ctx;
            ctx.save();
            ctx.fillStyle = VARIABLE_COLORS.windSpeed.stroke;
            ctx.font = `${11 * devicePixelRatio}px -apple-system, sans-serif`;
            ctx.textAlign = 'center';
            ctx.textBaseline = 'bottom';

            for (let i = 0; i < directionMedian.length; i += 6) {
                const dir = directionMedian[i];
                const speed = speedMedianData?.[i];
                if (dir == null || speed == null) continue;

                const cx = u.valToPos(times[i], 'x', true);
                const cy = u.valToPos(speed, 'y', true);

                if (cx >= u.bbox.left && cx <= u.bbox.left + u.bbox.width &&
                    cy >= u.bbox.top && cy <= u.bbox.top + u.bbox.height) {
                    ctx.fillText(windArrow(dir), cx, cy - 4 * devicePixelRatio);
                }
            }
            ctx.restore();
        };

        if (!options.hooks) options.hooks = {};
        if (!options.hooks.draw) options.hooks.draw = [];
        (options.hooks.draw as ((u: uPlot) => void)[]).push(arrowHook);
    }

    // Current values for HUD row
    const now = new Date();
    const nowIdx = findNearestIndex(ensemble.times, now);
    const currentSpeed = speedStats.median[nowIdx];
    const currentP10 = speedStats.p10[nowIdx];
    const currentP90 = speedStats.p90[nowIdx];

    // Observation value (within 2 hours)
    let obsWind: number | null = null;
    if (overlays.has('obs') && observations?.entries) {
        const recentObs = observations.entries.find(e => {
            const diff = Math.abs(now.getTime() - parseUtcMs(e.timestamp));
            return diff < 2 * 3600 * 1000 && e.wind_speed_kmh != null;
        });
        if (recentObs) obsWind = recentObs.wind_speed_kmh;
    }

    // HRRR value (within 1 hour)
    let hrrrWind: number | null = null;
    if (overlays.has('hrrr') && hrrr?.wind_speed_10m && hrrr.times) {
        const hrrrIdx = findNearestIndex(hrrr.times, now);
        const diff = Math.abs(now.getTime() - parseUtcMs(hrrr.times[hrrrIdx]));
        if (diff < 3600 * 1000) {
            hrrrWind = hrrr.wind_speed_10m[hrrrIdx] ?? null;
        }
    }

    return (
        <div class="panel">
            <div class="panel__header">
                <span class="panel__title">Wind ({unitLabel})</span>
                <div class="panel__legend">
                    <span class="panel__legend-item">
                        <span class="panel__legend-dot" style={{ backgroundColor: VARIABLE_COLORS.windSpeed.stroke }} />
                        Speed
                    </span>
                    {gustStats && (
                        <span class="panel__legend-item">
                            <span class="panel__legend-dot" style={{ backgroundColor: VARIABLE_COLORS.windGusts.stroke }} />
                            Gusts
                        </span>
                    )}
                </div>
            </div>
            {/* HUD row */}
            <div class="panel__hud-row">
                {obsWind != null && (
                    <span class="panel__hud-chip panel__hud-chip--obs">
                        <span class="panel__hud-chip-label">Obs:</span>
                        {Math.round(unitConverter(obsWind))} {unitLabel}
                    </span>
                )}
                {hrrrWind != null && (
                    <span class="panel__hud-chip panel__hud-chip--hrrr-wind">
                        <span class="panel__hud-chip-label">HRRR:</span>
                        {Math.round(unitConverter(hrrrWind))} {unitLabel}
                    </span>
                )}
                {currentSpeed != null && (
                    <span class="panel__hud-chip panel__hud-chip--ens-wind">
                        <span class="panel__hud-chip-label">Ens:</span>
                        {Math.round(unitConverter(currentSpeed))}
                        {currentP10 != null && currentP90 != null && (
                            <span class="panel__hud-chip-range">
                                ({Math.round(unitConverter(currentP10))}–{Math.round(unitConverter(currentP90))})
                            </span>
                        )}
                        {' '}{unitLabel}
                    </span>
                )}
            </div>
            <div class="panel__body">
                <ChartWrapper options={options} data={data} height={220} />
            </div>
        </div>
    );
}

/** Find the index in `times` whose ISO timestamp is closest to `now`. */
function findNearestIndex(times: string[], now: Date): number {
    if (times.length === 0) return 0;
    const nowMs = now.getTime();
    let bestIndex = 0;
    let bestDiff = Math.abs(parseUtcMs(times[0]) - nowMs);
    for (let i = 1; i < times.length; i++) {
        const diff = Math.abs(parseUtcMs(times[i]) - nowMs);
        if (diff < bestDiff) {
            bestDiff = diff;
            bestIndex = i;
        }
    }
    return bestIndex;
}
