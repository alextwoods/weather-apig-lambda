import uPlot from 'uplot';
import type { ZoomLevel } from '../state/url-state';

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
    hrrr?: (number | null)[];  // Optional HRRR overlay
    observations?: { time: number; value: number }[];  // Optional obs overlay
    sunAltitude?: number[];    // For day/night shading
    unitConverter: (v: number) => number;  // Applied to all values
    label: string;             // Y-axis label
    syncKey: string;           // uPlot.sync() group key
    zoomLevel: ZoomLevel;
}

// --- Band fill colors ---

const OUTER_BAND_FILL = 'rgba(59, 130, 246, 0.1)';
const INNER_BAND_FILL = 'rgba(59, 130, 246, 0.25)';

// --- Series colors ---

const MEDIAN_STROKE = 'rgba(59, 130, 246, 1)';
const HRRR_STROKE = 'rgba(239, 68, 68, 0.9)';
const OBS_STROKE = 'rgba(34, 197, 94, 1)';

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
 * Builds the uPlot AlignedData array for a fan chart.
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
 * Builds the uPlot Options object for a fan chart.
 *
 * Configures:
 * - Hidden series for band boundaries (p10, p25, p75, p90)
 * - Visible median line series
 * - Three bands: p75→p90 (outer), p25→p75 (inner), p10→p25 (outer)
 * - Optional HRRR overlay series
 * - Optional observation point markers (rendered via hooks)
 * - Sync key for crosshair coordination
 */
export function buildFanChartOptions(config: FanChartConfig): uPlot.Options {
    const { label, syncKey, hrrr, observations, unitConverter } = config;

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
            stroke: MEDIAN_STROKE,
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
        series.push({
            label: 'HRRR',
            show: true,
            stroke: HRRR_STROKE,
            width: 1.5,
            dash: [4, 2],
            points: { show: false },
        });
    }

    // --- Bands configuration ---
    // Bands fill between series pairs. The series indices reference the series array.
    const bands: uPlot.Band[] = [
        // p75→p90 (outer band, lighter fill)
        { series: [1, 2], fill: OUTER_BAND_FILL },
        // p25→p75 (inner band, darker fill)
        { series: [2, 4], fill: INNER_BAND_FILL },
        // p10→p25 (outer band, lighter fill)
        { series: [4, 5], fill: OUTER_BAND_FILL },
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

    // --- Hooks for observation point markers ---
    const hooks: uPlot.Hooks.Arrays = {};

    if (observations && observations.length > 0) {
        hooks.draw = [
            (u: uPlot) => {
                const ctx = u.ctx;
                ctx.save();
                ctx.fillStyle = OBS_STROKE;

                for (const obs of observations!) {
                    const cx = u.valToPos(obs.time, 'x', true);
                    const cy = u.valToPos(unitConverter(obs.value), u.series[3].scale!, true);

                    // Only draw if within the visible plot area
                    if (cx >= u.bbox.left && cx <= u.bbox.left + u.bbox.width &&
                        cy >= u.bbox.top && cy <= u.bbox.top + u.bbox.height) {
                        ctx.beginPath();
                        ctx.arc(cx, cy, 3 * devicePixelRatio, 0, Math.PI * 2);
                        ctx.fill();
                    }
                }

                ctx.restore();
            },
        ];
    }

    // --- Build options ---
    const options: uPlot.Options = {
        width: 800,
        height: 200,
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
            x: { time: true },
            y: { auto: true },
        },
        legend: { show: false },
    };

    return options;
}
