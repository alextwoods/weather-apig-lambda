/**
 * Color palette for weather chart rendering.
 *
 * Organized by usage context: band fills, model lines, overlays,
 * AQI categories, and UV severity levels.
 */

// --- Band fill colors (ensemble percentile bands) ---

/** Outer band fill (p10–p25 and p75–p90) — lighter, wider uncertainty */
export const BAND_OUTER_FILL = 'rgba(59, 130, 246, 0.1)';

/** Inner band fill (p25–p75) — darker, core uncertainty range */
export const BAND_INNER_FILL = 'rgba(59, 130, 246, 0.25)';

// --- Model colors (5 distinct colors for individual model lines in detail view) ---

export const MODEL_COLORS = {
    ecmwf: 'rgba(59, 130, 246, 0.8)',   // Blue
    gfs: 'rgba(239, 68, 68, 0.8)',      // Red
    icon: 'rgba(34, 197, 94, 0.8)',     // Green
    gem: 'rgba(168, 85, 247, 0.8)',     // Purple
    bom: 'rgba(245, 158, 11, 0.8)',     // Amber
} as const;

/** Ordered array of model colors for indexed access */
export const MODEL_COLOR_LIST = [
    MODEL_COLORS.ecmwf,
    MODEL_COLORS.gfs,
    MODEL_COLORS.icon,
    MODEL_COLORS.gem,
    MODEL_COLORS.bom,
] as const;

// --- Overlay colors ---

/** HRRR overlay line color */
export const HRRR_STROKE = 'rgba(239, 68, 68, 0.9)';

/** Observation point marker color */
export const OBSERVATIONS_STROKE = 'rgba(34, 197, 94, 1)';

/** Median line color */
export const MEDIAN_STROKE = 'rgba(59, 130, 246, 1)';

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
