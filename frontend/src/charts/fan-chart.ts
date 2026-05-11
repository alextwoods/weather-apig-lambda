import uPlot from 'uplot';
import type { ZoomLevel } from '../state/url-state';
import { ZOOM_DURATION_SECONDS } from './zoom';
import type { VariableColorKey } from './colors';
import { VARIABLE_COLORS, CURRENT_TIME_STROKE, CURRENT_TIME_WIDTH, DAY_SHADE_FILL, DAY_SEPARATOR_STROKE, DAY_SEPARATOR_WIDTH, DAY_SEPARATOR_DASH } from './colors';

// --- Fan Chart Configuration ---

export interface FanChartConfig {
    times: number[];           // Unix timestamps (x-axis)
    stats: {
        p10: (number | null)[];
        p25: (number | null)[];
        median: (number | null)[];
        p75: (number | null)[];
        p90: (number | null)[];
    };
    color: VariableColorKey;   // Variable color key for band/line coloring
    hrrr?: (number | null)[];  // Optional HRRR overlay
    hrrrColor?: string;        // HRRR line color override (defaults to variable color at 0.5 opacity)
    observations?: { time: number; value: number }[];  // Optional obs overlay
    sunAltitude?: number[];    // For day/night shading
    unitConverter: (v: number) => number;  // Applied to all values
    label: string;             // Y-axis label
    syncKey: string;           // uPlot.sync() group key
    zoomLevel: ZoomLevel;
    height?: number;           // Chart height (default 200)
    yMin?: number;             // Fixed Y-axis minimum
    yMax?: number;             // Fixed Y-axis maximum
}

/**
 * Multi-series fan chart configuration for overlaying multiple variables
 * on a single chart (e.g., temperature + feels like + dew point).
 */
export interface MultiFanChartConfig {
    times: number[];
    series: Array<{
        stats: {
            p10: (number | null)[];
            p25: (number | null)[];
            median: (number | null)[];
            p75: (number | null)[];
            p90: (number | null)[];
        };
        color: VariableColorKey;
        label: string;
    }>;
    hrrr?: Array<{
        data: (number | null)[];
        color: string;
        label: string;
    }>;
    observations?: { time: number; value: number }[];
    sunAltitude?: number[];
    unitConverter: (v: number) => number;
    axisLabel: string;
    syncKey: string;
    zoomLevel: ZoomLevel;
    height?: number;
    yMin?: number;
    yMax?: number;
}

/**
 * Applies the unit converter to a data array, preserving nulls.
 */
function convertArray(
    values: (number | null)[],
    converter: (v: number) => number,
): (number | null | undefined)[] {
    return values.map(v => (v === null ? null : converter(v)));
}

/**
 * Builds the uPlot AlignedData array for a single-variable fan chart.
 *
 * Data layout:
 *   data[0] = timestamps (seconds since epoch)
 *   data[1] = p90 values (hidden series, upper band boundary)
 *   data[2] = p75 values (hidden series, inner band boundary)
 *   data[3] = median values (visible line)
 *   data[4] = p25 values (hidden series, inner band boundary)
 *   data[5] = p10 values (hidden series, lower band boundary)
 *   data[6] = HRRR values (optional, visible line)
 */
export function buildFanChartData(config: FanChartConfig): uPlot.AlignedData {
    const { times, stats, hrrr, unitConverter } = config;

    const data: uPlot.AlignedData = [
        times,
        convertArray(stats.p90, unitConverter),
        convertArray(stats.p75, unitConverter),
        convertArray(stats.median, unitConverter),
        convertArray(stats.p25, unitConverter),
        convertArray(stats.p10, unitConverter),
    ] as uPlot.AlignedData;

    if (hrrr) {
        (data as (number | null | undefined)[][]).push(convertArray(hrrr, unitConverter));
    }

    return data;
}

/**
 * Builds the uPlot Options object for a single-variable fan chart.
 *
 * Configures:
 * - Hidden series for band boundaries (p10, p25, p75, p90)
 * - Visible median line series with variable-specific color
 * - Three bands: p75→p90 (outer), p25→p75 (inner), p10→p25 (outer)
 * - Optional HRRR overlay series (dashed line)
 * - Optional observation point markers (rendered via hooks)
 * - Current time red vertical line
 * - Night shading from sun altitude data
 * - Sync key for crosshair coordination
 */
export function buildFanChartOptions(config: FanChartConfig): uPlot.Options {
    const { color, label, syncKey, hrrr, observations, unitConverter, sunAltitude, height, yMin, yMax } = config;

    const colors = VARIABLE_COLORS[color];

    // --- Series configuration ---
    const series: uPlot.Series[] = [
        // Series 0: time axis (implicit, empty config)
        {},
        // Series 1: p90 (hidden, upper outer band boundary)
        {
            label: 'p90',
            show: true,
            stroke: 'transparent',
            width: 0,
            points: { show: false },
        },
        // Series 2: p75 (hidden, upper inner band boundary)
        {
            label: 'p75',
            show: true,
            stroke: 'transparent',
            width: 0,
            points: { show: false },
        },
        // Series 3: median (visible line)
        {
            label: 'Median',
            show: true,
            stroke: colors.stroke,
            width: 2,
            points: { show: false },
        },
        // Series 4: p25 (hidden, lower inner band boundary)
        {
            label: 'p25',
            show: true,
            stroke: 'transparent',
            width: 0,
            points: { show: false },
        },
        // Series 5: p10 (hidden, lower outer band boundary)
        {
            label: 'p10',
            show: true,
            stroke: 'transparent',
            width: 0,
            points: { show: false },
        },
    ];

    // Series 6: HRRR overlay (optional)
    if (hrrr) {
        const hrrrStroke = config.hrrrColor ?? colors.stroke.replace(', 1)', ', 0.5)');
        series.push({
            label: 'HRRR',
            show: true,
            stroke: hrrrStroke,
            width: 1.5,
            dash: [4, 3],
            points: { show: false },
        });
    }

    // --- Bands configuration ---
    const bands: uPlot.Band[] = [
        // p75→p90 (outer band, lighter fill)
        { series: [1, 2], fill: colors.outer },
        // p25→p75 (inner band, darker fill)
        { series: [2, 4], fill: colors.inner },
        // p10→p25 (outer band, lighter fill)
        { series: [4, 5], fill: colors.outer },
    ];

    // --- Axes configuration ---
    const axes: uPlot.Axis[] = [
        // X-axis (time)
        {
            stroke: '#666',
            grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
        },
        // Y-axis (values)
        {
            label,
            stroke: '#666',
            grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
            size: 60,
        },
    ];

    // --- Hooks for overlays ---
    const drawHooks: ((u: uPlot) => void)[] = [];

    // Night shading
    if (sunAltitude && sunAltitude.length > 0) {
        drawHooks.push(buildNightShadingHook(config.times, sunAltitude));
    }

    // Current time red vertical line
    drawHooks.push(buildCurrentTimeHook());

    // Observation point markers
    if (observations && observations.length > 0) {
        drawHooks.push(buildObservationsHook(observations, unitConverter));
    }

    const hooks: uPlot.Hooks.Arrays = {};
    if (drawHooks.length > 0) {
        hooks.draw = drawHooks;
    }

    // --- Scale configuration ---
    const yScale: uPlot.Scale = { auto: true };
    if (yMin !== undefined) (yScale as any).min = yMin;
    if (yMax !== undefined) (yScale as any).max = yMax;

    // X-axis scale: apply zoom level to constrain visible window
    const zoomDuration = ZOOM_DURATION_SECONDS[config.zoomLevel];
    const nowSec = Date.now() / 1000;
    const xMin = nowSec - zoomDuration * 0.1; // Show a little before now
    const xMax = xMin + zoomDuration;

    // --- Build options ---
    const options: uPlot.Options = {
        width: 800,
        height: height ?? 200,
        series,
        bands,
        axes,
        hooks,
        cursor: {
            sync: {
                key: syncKey,
            },
        },
        scales: {
            x: { time: true, min: xMin, max: xMax },
            y: yScale,
        },
        legend: { show: false },
    };

    return options;
}

/**
 * Builds uPlot data for a multi-series fan chart (multiple variables overlaid).
 *
 * Data layout:
 *   data[0] = timestamps
 *   For each series (5 values each): p90, p75, median, p25, p10
 *   Then HRRR lines (if any)
 */
export function buildMultiFanChartData(config: MultiFanChartConfig): uPlot.AlignedData {
    const { times, series: seriesConfigs, hrrr, unitConverter } = config;

    const data: (number | null | undefined)[][] = [times as any];

    for (const s of seriesConfigs) {
        data.push(convertArray(s.stats.p90, unitConverter));
        data.push(convertArray(s.stats.p75, unitConverter));
        data.push(convertArray(s.stats.median, unitConverter));
        data.push(convertArray(s.stats.p25, unitConverter));
        data.push(convertArray(s.stats.p10, unitConverter));
    }

    if (hrrr) {
        for (const h of hrrr) {
            data.push(convertArray(h.data, unitConverter));
        }
    }

    return data as uPlot.AlignedData;
}

/**
 * Builds uPlot options for a multi-series fan chart.
 * Each variable gets its own colored bands and median line.
 */
export function buildMultiFanChartOptions(config: MultiFanChartConfig): uPlot.Options {
    const { series: seriesConfigs, hrrr, observations, unitConverter, axisLabel, syncKey, sunAltitude, height, yMin, yMax } = config;

    const uplotSeries: uPlot.Series[] = [{}]; // Series 0: time
    const uplotBands: uPlot.Band[] = [];

    let seriesIndex = 1;

    for (const s of seriesConfigs) {
        const colors = VARIABLE_COLORS[s.color];
        const p90Idx = seriesIndex;
        const p75Idx = seriesIndex + 1;
        const medianIdx = seriesIndex + 2;
        const p25Idx = seriesIndex + 3;
        const p10Idx = seriesIndex + 4;

        // p90
        uplotSeries.push({ label: `${s.label} p90`, show: true, stroke: 'transparent', width: 0, points: { show: false } });
        // p75
        uplotSeries.push({ label: `${s.label} p75`, show: true, stroke: 'transparent', width: 0, points: { show: false } });
        // median
        uplotSeries.push({ label: s.label, show: true, stroke: colors.stroke, width: 2, points: { show: false } });
        // p25
        uplotSeries.push({ label: `${s.label} p25`, show: true, stroke: 'transparent', width: 0, points: { show: false } });
        // p10
        uplotSeries.push({ label: `${s.label} p10`, show: true, stroke: 'transparent', width: 0, points: { show: false } });

        // Bands
        uplotBands.push({ series: [p90Idx, p75Idx], fill: colors.outer });
        uplotBands.push({ series: [p75Idx, p25Idx], fill: colors.inner });
        uplotBands.push({ series: [p25Idx, p10Idx], fill: colors.outer });

        seriesIndex += 5;
    }

    // HRRR overlay lines
    if (hrrr) {
        for (const h of hrrr) {
            uplotSeries.push({
                label: h.label,
                show: true,
                stroke: h.color,
                width: 1.5,
                dash: [4, 3],
                points: { show: false },
            });
        }
    }

    // --- Axes ---
    const axes: uPlot.Axis[] = [
        { stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' } },
        { label: axisLabel, stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' }, size: 60 },
    ];

    // --- Hooks ---
    const drawHooks: ((u: uPlot) => void)[] = [];

    if (sunAltitude && sunAltitude.length > 0) {
        drawHooks.push(buildNightShadingHook(config.times, sunAltitude));
    }

    drawHooks.push(buildCurrentTimeHook());

    if (observations && observations.length > 0) {
        drawHooks.push(buildObservationsHook(observations, unitConverter));
    }

    const hooks: uPlot.Hooks.Arrays = {};
    if (drawHooks.length > 0) {
        hooks.draw = drawHooks;
    }

    // --- Scale ---
    const yScale: uPlot.Scale = { auto: true };
    if (yMin !== undefined) (yScale as any).min = yMin;
    if (yMax !== undefined) (yScale as any).max = yMax;

    // X-axis scale: apply zoom level to constrain visible window
    const zoomDuration = ZOOM_DURATION_SECONDS[config.zoomLevel];
    const nowSec = Date.now() / 1000;
    const xMin = nowSec - zoomDuration * 0.1;
    const xMax = xMin + zoomDuration;

    return {
        width: 800,
        height: height ?? 220,
        series: uplotSeries,
        bands: uplotBands,
        axes,
        hooks,
        cursor: { sync: { key: syncKey } },
        scales: { x: { time: true, min: xMin, max: xMax }, y: yScale },
        legend: { show: false },
    };
}

// --- Draw hook builders ---

/**
 * Builds a draw hook that renders a solid red vertical line at the current time.
 */
function buildCurrentTimeHook(): (u: uPlot) => void {
    return (u: uPlot) => {
        const now = Date.now() / 1000;
        const cx = u.valToPos(now, 'x', true);

        // Only draw if within the visible plot area
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
    };
}

/**
 * Builds a draw hook that renders day shading (lighter background when sun altitude >= 0).
 * Night areas remain unshaded (darker), day areas get a subtle lighter overlay.
 */
function buildNightShadingHook(times: number[], sunAltitude: number[]): (u: uPlot) => void {
    return (u: uPlot) => {
        const ctx = u.ctx;
        ctx.save();
        ctx.fillStyle = DAY_SHADE_FILL;

        // Find contiguous day regions (sun altitude >= 0)
        let inDay = false;
        let dayStart = 0;

        for (let i = 0; i < sunAltitude.length; i++) {
            const isDay = sunAltitude[i] >= 0;

            if (isDay && !inDay) {
                // Interpolate start (transition from night to day)
                if (i > 0 && sunAltitude[i - 1] < 0) {
                    const frac = -sunAltitude[i - 1] / (sunAltitude[i] - sunAltitude[i - 1]);
                    const interpTime = times[i - 1] + frac * (times[i] - times[i - 1]);
                    dayStart = u.valToPos(interpTime, 'x', true);
                } else {
                    dayStart = u.valToPos(times[i], 'x', true);
                }
                inDay = true;
            } else if (!isDay && inDay) {
                // Interpolate end (transition from day to night)
                let dayEnd: number;
                if (i > 0 && sunAltitude[i - 1] >= 0) {
                    const frac = sunAltitude[i - 1] / (sunAltitude[i - 1] - sunAltitude[i]);
                    const interpTime = times[i - 1] + frac * (times[i] - times[i - 1]);
                    dayEnd = u.valToPos(interpTime, 'x', true);
                } else {
                    dayEnd = u.valToPos(times[i], 'x', true);
                }

                // Clamp to plot area and draw
                const x1 = Math.max(dayStart, u.bbox.left);
                const x2 = Math.min(dayEnd, u.bbox.left + u.bbox.width);
                if (x2 > x1) {
                    ctx.fillRect(x1, u.bbox.top, x2 - x1, u.bbox.height);
                }
                inDay = false;
            }
        }

        // If still in day at end of data
        if (inDay) {
            const dayEnd = u.bbox.left + u.bbox.width;
            const x1 = Math.max(dayStart, u.bbox.left);
            if (dayEnd > x1) {
                ctx.fillRect(x1, u.bbox.top, dayEnd - x1, u.bbox.height);
            }
        }

        ctx.restore();
    };
}

/**
 * Builds a draw hook that renders observation point markers as yellow dots.
 */
function buildObservationsHook(
    observations: { time: number; value: number }[],
    unitConverter: (v: number) => number,
): (u: uPlot) => void {
    return (u: uPlot) => {
        const ctx = u.ctx;
        ctx.save();
        ctx.fillStyle = 'rgba(250, 204, 21, 1)'; // Yellow per iOS spec

        for (const obs of observations) {
            const cx = u.valToPos(obs.time, 'x', true);
            // Find the first visible y-scale series (median is typically series 3)
            const scale = u.series[3]?.scale ?? u.series[1]?.scale ?? 'y';
            const cy = u.valToPos(unitConverter(obs.value), scale, true);

            // Only draw if within the visible plot area
            if (cx >= u.bbox.left && cx <= u.bbox.left + u.bbox.width &&
                cy >= u.bbox.top && cy <= u.bbox.top + u.bbox.height) {
                ctx.beginPath();
                ctx.arc(cx, cy, 3 * devicePixelRatio, 0, Math.PI * 2);
                ctx.fill();
            }
        }

        ctx.restore();
    };
}
