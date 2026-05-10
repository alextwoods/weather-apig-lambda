/**
 * EPA AQI category mapping.
 *
 * Maps an integer AQI value [0–500] to its EPA category name and color.
 * Boundaries are mutually exclusive and collectively exhaustive over the range.
 *
 * Validates: Requirements 5.5, 13.1, 13.3
 */

export interface AqiCategory {
    category: string;
    color: string;
}

/**
 * Map an integer AQI value (0–500) to its EPA category and color.
 *
 * EPA breakpoints:
 *   0–50:   Good (green, #00e400)
 *   51–100: Moderate (yellow, #ffff00)
 *   101–150: Unhealthy for Sensitive Groups (orange, #ff7e00)
 *   151–200: Unhealthy (red, #ff0000)
 *   201–300: Very Unhealthy (purple, #8f3f97)
 *   301–500: Hazardous (maroon, #7e0023)
 *
 * @throws RangeError if aqi is outside [0, 500] or not an integer
 */
export function getAqiCategory(aqi: number): AqiCategory {
    if (!Number.isInteger(aqi) || aqi < 0 || aqi > 500) {
        throw new RangeError(`AQI value must be an integer in [0, 500], got ${aqi}`);
    }

    if (aqi <= 50) {
        return { category: 'Good', color: '#00e400' };
    }
    if (aqi <= 100) {
        return { category: 'Moderate', color: '#ffff00' };
    }
    if (aqi <= 150) {
        return { category: 'Unhealthy for Sensitive Groups', color: '#ff7e00' };
    }
    if (aqi <= 200) {
        return { category: 'Unhealthy', color: '#ff0000' };
    }
    if (aqi <= 300) {
        return { category: 'Very Unhealthy', color: '#8f3f97' };
    }
    // 301–500
    return { category: 'Hazardous', color: '#7e0023' };
}
