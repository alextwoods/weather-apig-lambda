import uPlot from 'uplot';

/**
 * Axis/scale configuration for each weather variable type.
 *
 * Each config provides a uPlot scale definition and axis definition
 * suitable for the variable's expected value range and display needs.
 */

export interface VariableScaleConfig {
    /** uPlot scale configuration for the y-axis */
    scale: uPlot.Scale;
    /** uPlot axis configuration for the y-axis */
    axis: Partial<uPlot.Axis>;
}

/**
 * Temperature scale — auto-ranging to fit data.
 * No fixed min/max since temperatures vary widely by location and season.
 */
export const temperatureScale: VariableScaleConfig = {
    scale: {
        auto: true,
    },
    axis: {
        label: 'Temperature',
        size: 60,
        stroke: '#666',
        grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
        values: (_u: uPlot, vals: number[]) => vals.map(v => `${v.toFixed(0)}°`),
    },
};

/**
 * Wind speed scale — minimum 0, auto upper bound.
 * Wind speed cannot be negative.
 */
export const windSpeedScale: VariableScaleConfig = {
    scale: {
        auto: true,
        range: (_u: uPlot, min: number, max: number): uPlot.Range.MinMax => [
            Math.min(0, min),
            max,
        ],
    },
    axis: {
        label: 'Wind Speed',
        size: 60,
        stroke: '#666',
        grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
    },
};

/**
 * Pressure scale — tight auto-ranging.
 * Pressure values cluster in a narrow range (typically 980–1040 hPa),
 * so we use auto range with a small padding to show detail.
 */
export const pressureScale: VariableScaleConfig = {
    scale: {
        auto: true,
        range: (_u: uPlot, min: number, max: number): uPlot.Range.MinMax => {
            // Add 2% padding on each side for a tight but readable range
            const padding = (max - min) * 0.02 || 1;
            return [min - padding, max + padding];
        },
    },
    axis: {
        label: 'Pressure',
        size: 70,
        stroke: '#666',
        grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
    },
};

/**
 * Percentage scale — fixed 0–100 range.
 * Used for cloud cover, humidity, precipitation probability.
 */
export const percentageScale: VariableScaleConfig = {
    scale: {
        auto: false,
        range: (): uPlot.Range.MinMax => [0, 100],
    },
    axis: {
        label: '%',
        size: 50,
        stroke: '#666',
        grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
        values: (_u: uPlot, vals: number[]) => vals.map(v => `${v.toFixed(0)}%`),
    },
};

/**
 * Precipitation scale — minimum 0, auto upper bound.
 * Precipitation cannot be negative.
 */
export const precipitationScale: VariableScaleConfig = {
    scale: {
        auto: true,
        range: (_u: uPlot, min: number, max: number): uPlot.Range.MinMax => [
            0,
            Math.max(max, 0.1), // Ensure some visible range even with zero precip
        ],
    },
    axis: {
        label: 'Precipitation',
        size: 60,
        stroke: '#666',
        grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
    },
};

/**
 * UV index scale — minimum 0, auto upper bound.
 * UV index is always non-negative.
 */
export const uvIndexScale: VariableScaleConfig = {
    scale: {
        auto: true,
        range: (_u: uPlot, _min: number, max: number): uPlot.Range.MinMax => [
            0,
            Math.max(max, 1), // Ensure some visible range
        ],
    },
    axis: {
        label: 'UV Index',
        size: 50,
        stroke: '#666',
        grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
        values: (_u: uPlot, vals: number[]) => vals.map(v => v.toFixed(0)),
    },
};

/**
 * Wave height scale — minimum 0, auto upper bound.
 * Wave height is always non-negative.
 */
export const waveHeightScale: VariableScaleConfig = {
    scale: {
        auto: true,
        range: (_u: uPlot, _min: number, max: number): uPlot.Range.MinMax => [
            0,
            Math.max(max, 0.1), // Ensure some visible range
        ],
    },
    axis: {
        label: 'Wave Height',
        size: 60,
        stroke: '#666',
        grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
    },
};
