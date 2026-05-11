import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { buildFanChartData, buildFanChartOptions } from '../charts/fan-chart';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';
import { VARIABLE_COLORS, CURRENT_TIME_STROKE, CURRENT_TIME_WIDTH } from '../charts/colors';
import { timesToUnixSeconds, parseUtcMs } from '../api/time-utils';
import uPlot from 'uplot';

export interface SolarPanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/**
 * UV color gradient (smooth interpolation across 5 stops).
 * Matches iOS spec Section 16.4.
 */
const UV_GRADIENT_STOPS = [
    { value: 0, r: 0, g: 191, b: 179 },     // Teal
    { value: 3, r: 51, g: 115, b: 242 },     // Blue
    { value: 6, r: 89, g: 51, b: 204 },      // Indigo
    { value: 9, r: 204, g: 38, b: 153 },     // Magenta
    { value: 12, r: 230, g: 26, b: 38 },     // Red
];

/** Get UV color by interpolating the gradient. */
export function uvGradientColor(uvValue: number): string {
    const clamped = Math.max(0, Math.min(12, uvValue));

    // Find the two stops to interpolate between
    for (let i = 0; i < UV_GRADIENT_STOPS.length - 1; i++) {
        const low = UV_GRADIENT_STOPS[i];
        const high = UV_GRADIENT_STOPS[i + 1];
        if (clamped >= low.value && clamped <= high.value) {
            const t = (clamped - low.value) / (high.value - low.value);
            const r = Math.round(low.r + t * (high.r - low.r));
            const g = Math.round(low.g + t * (high.g - low.g));
            const b = Math.round(low.b + t * (high.b - low.b));
            return `rgb(${r}, ${g}, ${b})`;
        }
    }

    // Beyond 12: use red
    return 'rgb(230, 26, 38)';
}

/**
 * Builds uPlot options for the combined Solar & UV chart.
 * - Solar irradiance as a yellow fan chart (handled separately)
 * - UV Index as a color-gradient line (drawn via hook)
 * - UV Clear Sky as a dashed line at 0.4 opacity (drawn via hook)
 * - Y-axis domain: 0-1200
 * - UV values scaled by factor of 80 to map onto irradiance Y-axis
 */
function buildSolarUvChartOptions(
    syncKey: string,
    uvTimes?: number[],
    uvIndex?: (number | null)[],
    uvClearSky?: (number | null)[],
): uPlot.Options {
    const drawHooks: ((u: uPlot) => void)[] = [];

    // UV Index as color-gradient segments
    if (uvTimes && uvIndex) {
        drawHooks.push((u: uPlot) => {
            const ctx = u.ctx;
            ctx.save();
            ctx.lineWidth = 2 * devicePixelRatio;
            ctx.lineCap = 'round';

            for (let i = 0; i < uvTimes.length - 1; i++) {
                const v1 = uvIndex[i];
                const v2 = uvIndex[i + 1];
                if (v1 == null || v2 == null) continue;

                const x1 = u.valToPos(uvTimes[i], 'x', true);
                const x2 = u.valToPos(uvTimes[i + 1], 'x', true);
                const y1 = u.valToPos(v1 * 80, 'y', true); // Scale UV by 80
                const y2 = u.valToPos(v2 * 80, 'y', true);

                if (x2 < u.bbox.left || x1 > u.bbox.left + u.bbox.width) continue;

                ctx.strokeStyle = uvGradientColor((v1 + v2) / 2);
                ctx.beginPath();
                ctx.moveTo(x1, y1);
                ctx.lineTo(x2, y2);
                ctx.stroke();
            }
            ctx.restore();
        });
    }

    // UV Clear Sky as dashed line (same color as UV, dotted)
    if (uvTimes && uvClearSky) {
        drawHooks.push((u: uPlot) => {
            const ctx = u.ctx;
            ctx.save();
            ctx.lineWidth = 1.5 * devicePixelRatio;
            ctx.setLineDash([4 * devicePixelRatio, 3 * devicePixelRatio]);
            ctx.lineCap = 'round';

            for (let i = 0; i < uvTimes.length - 1; i++) {
                const v1 = uvClearSky[i];
                const v2 = uvClearSky[i + 1];
                if (v1 == null || v2 == null) continue;

                const x1 = u.valToPos(uvTimes[i], 'x', true);
                const x2 = u.valToPos(uvTimes[i + 1], 'x', true);
                const y1 = u.valToPos(v1 * 80, 'y', true);
                const y2 = u.valToPos(v2 * 80, 'y', true);

                if (x2 < u.bbox.left || x1 > u.bbox.left + u.bbox.width) continue;

                ctx.strokeStyle = uvGradientColor((v1 + v2) / 2);
                ctx.beginPath();
                ctx.moveTo(x1, y1);
                ctx.lineTo(x2, y2);
                ctx.stroke();
            }
            ctx.restore();
        });
    }

    // Current time indicator
    drawHooks.push((u: uPlot) => {
        const now = Date.now() / 1000;
        const cx = u.valToPos(now, 'x', true);
        if (cx < u.bbox.left || cx > u.bbox.left + u.bbox.width) return;
        const ctx = u.ctx;
        ctx.save();
        ctx.strokeStyle = CURRENT_TIME_STROKE;
        ctx.lineWidth = CURRENT_TIME_WIDTH * devicePixelRatio;
        ctx.beginPath();
        ctx.moveTo(cx, u.bbox.top);
        ctx.lineTo(cx, u.bbox.top + u.bbox.height);
        ctx.stroke();
        ctx.restore();
    });

    return {
        width: 800,
        height: 200,
        series: [
            {},
            // Placeholder series for the fan chart data (handled by buildFanChartOptions)
        ],
        axes: [
            { stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' } },
            { label: 'W/m²', stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' }, size: 60 },
        ],
        scales: { x: { time: true }, y: { min: 0, max: 1200 } },
        cursor: { sync: { key: syncKey } },
        legend: { show: false },
        hooks: { draw: drawHooks },
    };
}

/**
 * Solar & UV Panel.
 * Displays:
 * - Solar irradiance fan chart (yellow bands)
 * - UV Index as color-gradient line segments (scaled by 80 to fit irradiance Y-axis)
 * - UV Clear Sky as dashed gradient line at 0.4 opacity
 * - UV scale legend below the chart
 *
 * Matches iOS spec Section 16.
 */
export function SolarPanel({ forecast, units: _units, overlays: _overlays, zoom }: SolarPanelProps) {
    const { ensemble, uv, astronomy } = forecast;
    const radiationStats = ensemble.statistics.shortwave_radiation;

    const hasSomething = uv || radiationStats;
    if (!hasSomething) return null;

    const ensembleTimes = timesToUnixSeconds(ensemble.times);
    const identity = (v: number) => v;

    // Solar irradiance fan chart
    let radiationData: ReturnType<typeof buildFanChartData> | null = null;
    let radiationOptions: ReturnType<typeof buildFanChartOptions> | null = null;

    if (radiationStats) {
        const radiationConfig = {
            times: ensembleTimes,
            stats: radiationStats,
            color: 'solarIrradiance' as const,
            sunAltitude: astronomy?.sun_altitude,
            unitConverter: identity,
            label: 'Solar & UV (W/m²)',
            syncKey: CHART_SYNC_KEY,
            zoomLevel: zoom,
            height: 200,
            yMin: 0,
            yMax: 1200,
        };
        radiationData = buildFanChartData(radiationConfig);
        radiationOptions = buildFanChartOptions(radiationConfig);

        // Add UV overlay hooks to the radiation chart options
        if (uv && radiationOptions.hooks) {
            const uvTimes = timesToUnixSeconds(uv.times);

            // UV Index gradient line
            const uvHook = (u: uPlot) => {
                const ctx = u.ctx;
                ctx.save();
                ctx.lineWidth = 2 * devicePixelRatio;
                ctx.lineCap = 'round';

                for (let i = 0; i < uvTimes.length - 1; i++) {
                    const v1 = uv.uv_index[i];
                    const v2 = uv.uv_index[i + 1];
                    if (v1 == null || v2 == null) continue;

                    const x1 = u.valToPos(uvTimes[i], 'x', true);
                    const x2 = u.valToPos(uvTimes[i + 1], 'x', true);
                    const y1 = u.valToPos(v1 * 80, 'y', true);
                    const y2 = u.valToPos(v2 * 80, 'y', true);

                    if (x2 < u.bbox.left || x1 > u.bbox.left + u.bbox.width) continue;

                    ctx.strokeStyle = uvGradientColor((v1 + v2) / 2);
                    ctx.beginPath();
                    ctx.moveTo(x1, y1);
                    ctx.lineTo(x2, y2);
                    ctx.stroke();
                }
                ctx.restore();
            };

            // UV Clear Sky dashed line (same color as UV line, but dotted)
            const uvClearHook = (u: uPlot) => {
                const ctx = u.ctx;
                ctx.save();
                ctx.lineWidth = 1.5 * devicePixelRatio;
                ctx.setLineDash([4 * devicePixelRatio, 3 * devicePixelRatio]);
                ctx.lineCap = 'round';

                for (let i = 0; i < uvTimes.length - 1; i++) {
                    const v1 = uv.uv_index_clear_sky[i];
                    const v2 = uv.uv_index_clear_sky[i + 1];
                    if (v1 == null || v2 == null) continue;

                    const x1 = u.valToPos(uvTimes[i], 'x', true);
                    const x2 = u.valToPos(uvTimes[i + 1], 'x', true);
                    const y1 = u.valToPos(v1 * 80, 'y', true);
                    const y2 = u.valToPos(v2 * 80, 'y', true);

                    if (x2 < u.bbox.left || x1 > u.bbox.left + u.bbox.width) continue;

                    ctx.strokeStyle = uvGradientColor((v1 + v2) / 2);
                    ctx.beginPath();
                    ctx.moveTo(x1, y1);
                    ctx.lineTo(x2, y2);
                    ctx.stroke();
                }
                ctx.restore();
            };

            (radiationOptions.hooks!.draw as ((u: uPlot) => void)[]).push(uvHook, uvClearHook);
        }
    }

    // Current UV values for HUD
    let currentUv: number | null = null;
    let currentUvClear: number | null = null;
    if (uv) {
        const now = new Date();
        const nowMs = now.getTime();
        let bestIdx = 0;
        let bestDiff = Math.abs(parseUtcMs(uv.times[0]) - nowMs);
        for (let i = 1; i < uv.times.length; i++) {
            const diff = Math.abs(parseUtcMs(uv.times[i]) - nowMs);
            if (diff < bestDiff) { bestDiff = diff; bestIdx = i; }
        }
        currentUv = uv.uv_index[bestIdx] ?? null;
        currentUvClear = uv.uv_index_clear_sky[bestIdx] ?? null;
    }

    return (
        <div class="panel">
            <div class="panel__header">
                <span class="panel__title">Solar & UV (W/m²)</span>
                <div class="panel__legend">
                    <span class="panel__legend-item">
                        <span class="panel__legend-dot" style={{ backgroundColor: VARIABLE_COLORS.solarIrradiance.stroke }} />
                        Irradiance
                    </span>
                    <span class="panel__legend-item">
                        <span class="panel__legend-dot" style={{ background: 'linear-gradient(90deg, rgb(0,191,179), rgb(51,115,242), rgb(89,51,204), rgb(204,38,153), rgb(230,26,38))' }} />
                        UV Index
                    </span>
                    <span class="panel__legend-item" style={{ color: 'var(--color-text-muted)' }}>
                        <span style={{ borderBottom: '1px dashed var(--color-text-muted)', width: '12px', display: 'inline-block' }} />
                        UV Clear
                    </span>
                </div>
            </div>
            {/* HUD row */}
            {(currentUv != null || currentUvClear != null) && (
                <div class="panel__hud-row">
                    {currentUv != null && (
                        <span class="panel__hud-chip" style={{ color: uvGradientColor(currentUv) }}>
                            <span class="panel__hud-chip-label">UV:</span>
                            {currentUv.toFixed(1)}
                        </span>
                    )}
                    {currentUvClear != null && (
                        <span class="panel__hud-chip" style={{ color: uvGradientColor(currentUvClear), opacity: 0.7 }}>
                            <span class="panel__hud-chip-label">UV-Clear:</span>
                            {currentUvClear.toFixed(1)}
                        </span>
                    )}
                </div>
            )}
            <div class="panel__body">
                {radiationOptions && radiationData && (
                    <ChartWrapper options={radiationOptions} data={radiationData} height={200} />
                )}
                {/* UV Scale Legend */}
                {uv && (
                    <div class="panel__uv-scale">
                        {[0, 3, 6, 9, 12].map(v => (
                            <span key={v} style={{ color: uvGradientColor(v), fontSize: '8px' }}>
                                UV {v}
                            </span>
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}
