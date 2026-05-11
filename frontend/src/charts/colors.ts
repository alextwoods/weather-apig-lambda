/**
 * Color palette for weather chart rendering.
 *
 * Organized by usage context: variable-specific colors, model toggle colors,
 * overlay colors, AQI categories, and UV severity levels.
 *
 * Colors match the iOS EnsembleWeather app specification.
 */

// --- Variable-specific colors (matching iOS app Section 11.2) ---

export const VARIABLE_COLORS = {
    temperature: { stroke: 'rgba(251, 146, 60, 1)', outer: 'rgba(251, 146, 60, 0.15)', inner: 'rgba(251, 146, 60, 0.30)' },
    apparentTemp: { stroke: 'rgba(168, 85, 247, 1)', outer: 'rgba(168, 85, 247, 0.15)', inner: 'rgba(168, 85, 247, 0.30)' },
    dewPoint: { stroke: 'rgba(34, 211, 238, 1)', outer: 'rgba(34, 211, 238, 0.15)', inner: 'rgba(34, 211, 238, 0.30)' },
    windSpeed: { stroke: 'rgba(74, 222, 128, 1)', outer: 'rgba(74, 222, 128, 0.15)', inner: 'rgba(74, 222, 128, 0.30)' },
    windGusts: { stroke: 'rgba(248, 113, 113, 1)', outer: 'rgba(248, 113, 113, 0.15)', inner: 'rgba(248, 113, 113, 0.30)' },
    cloudCover: { stroke: 'rgba(156, 163, 175, 1)', outer: 'rgba(156, 163, 175, 0.15)', inner: 'rgba(156, 163, 175, 0.30)' },
    humidity: { stroke: 'rgba(34, 211, 238, 1)', outer: 'rgba(34, 211, 238, 0.15)', inner: 'rgba(34, 211, 238, 0.30)' },
    precipitation: { stroke: 'rgba(45, 212, 191, 1)', outer: 'rgba(45, 212, 191, 0.15)', inner: 'rgba(45, 212, 191, 0.30)' },
    pressure: { stroke: 'rgba(96, 165, 250, 1)', outer: 'rgba(96, 165, 250, 0.15)', inner: 'rgba(96, 165, 250, 0.30)' },
    solarIrradiance: { stroke: 'rgba(250, 204, 21, 1)', outer: 'rgba(250, 204, 21, 0.15)', inner: 'rgba(250, 204, 21, 0.30)' },
} as const;

export type VariableColorKey = keyof typeof VARIABLE_COLORS;

// --- Band fill colors (legacy, kept for backward compat) ---

/** Outer band fill (p10–p25 and p75–p90) — lighter, wider uncertainty */
export const BAND_OUTER_FILL = 'rgba(59, 130, 246, 0.1)';

/** Inner band fill (p25–p75) — darker, core uncertainty range */
export const BAND_INNER_FILL = 'rgba(59, 130, 246, 0.25)';

// --- Model toggle colors (matching iOS app Section 7.2) ---

export const MODEL_COLORS = {
    ecmwf: 'rgba(59, 130, 246, 1)',     // Blue
    gfs: 'rgba(74, 222, 128, 1)',       // Green
    icon: 'rgba(251, 146, 60, 1)',      // Orange
    gem: 'rgba(168, 85, 247, 1)',       // Purple
    bom: 'rgba(248, 113, 113, 1)',      // Red
} as const;

/** Ordered array of model colors for indexed access */
export const MODEL_COLOR_LIST = [
    MODEL_COLORS.ecmwf,
    MODEL_COLORS.gfs,
    MODEL_COLORS.icon,
    MODEL_COLORS.gem,
    MODEL_COLORS.bom,
] as const;

// --- Overlay toggle colors (matching iOS app Section 8.2) ---

export const OVERLAY_COLORS = {
    hrrr: 'rgba(140, 140, 140, 1)',     // Gray (Color(white: 0.55))
    obs: 'rgba(250, 204, 21, 1)',       // Yellow
    extended: 'rgba(34, 211, 238, 1)',  // Cyan
} as const;

// --- Overlay line colors ---

/** HRRR overlay line color — uses variable-specific color with reduced opacity */
export const HRRR_STROKE = 'rgba(251, 146, 60, 0.5)';

/** Observation point marker color — yellow per iOS spec */
export const OBSERVATIONS_STROKE = 'rgba(250, 204, 21, 1)';

/** Median line color (legacy default) */
export const MEDIAN_STROKE = 'rgba(59, 130, 246, 1)';

// --- Current time indicator ---
export const CURRENT_TIME_STROKE = 'rgba(239, 68, 68, 1)';
export const CURRENT_TIME_WIDTH = 1.5;

// --- Day/Night shading ---
// Day areas get a lighter overlay on the dark background; night areas remain unshaded.
export const NIGHT_SHADE_FILL = 'rgba(156, 163, 175, 0.10)';
export const DAY_SHADE_FILL = 'rgba(156, 163, 175, 0.10)';

// --- Day separator ---
export const DAY_SEPARATOR_STROKE = 'rgba(156, 163, 175, 0.6)';
export const DAY_SEPARATOR_WIDTH = 0.8;
export const DAY_SEPARATOR_DASH = [4, 4];

// --- AQI (Air Quality Index) EPA category band colors ---

export const AQI_COLORS = {
    /** Good (0–50) */
    good: 'rgba(0, 153, 102, 0.3)',
    /** Moderate (51–100) */
    moderate: 'rgba(255, 222, 51, 0.3)',
    /** Unhealthy for Sensitive Groups (101–150) */
    unhealthySensitive: 'rgba(255, 153, 51, 0.3)',
    /** Unhealthy (151–200) */
    unhealthy: 'rgba(204, 0, 51, 0.3)',
    /** Very Unhealthy (201–300) */
    veryUnhealthy: 'rgba(102, 0, 153, 0.3)',
    /** Hazardous (301–500) */
    hazardous: 'rgba(126, 0, 35, 0.3)',
} as const;

/** AQI category boundaries for drawing horizontal bands on charts */
export const AQI_BANDS = [
    { min: 0, max: 50, fill: AQI_COLORS.good, label: 'Good' },
    { min: 51, max: 100, fill: AQI_COLORS.moderate, label: 'Moderate' },
    { min: 101, max: 150, fill: AQI_COLORS.unhealthySensitive, label: 'USG' },
    { min: 151, max: 200, fill: AQI_COLORS.unhealthy, label: 'Unhealthy' },
    { min: 201, max: 300, fill: AQI_COLORS.veryUnhealthy, label: 'Very Unhealthy' },
    { min: 301, max: 500, fill: AQI_COLORS.hazardous, label: 'Hazardous' },
] as const;

// --- UV severity colors ---

export const UV_COLORS = {
    /** Low (0–2) */
    low: 'rgba(76, 175, 80, 0.8)',
    /** Moderate (3–5) */
    moderate: 'rgba(255, 235, 59, 0.8)',
    /** High (6–7) */
    high: 'rgba(255, 152, 0, 0.8)',
    /** Very High (8–10) */
    veryHigh: 'rgba(244, 67, 54, 0.8)',
    /** Extreme (11+) */
    extreme: 'rgba(156, 39, 176, 0.8)',
} as const;

/** UV index severity boundaries for color coding */
export const UV_BANDS = [
    { min: 0, max: 2, fill: UV_COLORS.low, label: 'Low' },
    { min: 3, max: 5, fill: UV_COLORS.moderate, label: 'Moderate' },
    { min: 6, max: 7, fill: UV_COLORS.high, label: 'High' },
    { min: 8, max: 10, fill: UV_COLORS.veryHigh, label: 'Very High' },
    { min: 11, max: Infinity, fill: UV_COLORS.extreme, label: 'Extreme' },
] as const;
