/**
 * Utility for parsing API time strings as UTC.
 *
 * The backend returns ISO 8601 time strings WITHOUT a timezone suffix
 * (e.g., "2025-05-10T22:00") which are meant to be UTC. However,
 * JavaScript's `new Date("2025-05-10T22:00")` interprets strings without
 * a timezone as LOCAL time, causing incorrect time lookups.
 *
 * This module provides a helper that ensures all API times are parsed as UTC.
 */

/**
 * Parse an API time string as UTC, returning Unix seconds.
 * Appends 'Z' if the string doesn't already have timezone info.
 */
export function parseUtcSeconds(isoTime: string): number {
    return Math.floor(parseUtcMs(isoTime) / 1000);
}

/**
 * Parse an API time string as UTC, returning milliseconds since epoch.
 * Appends 'Z' if the string doesn't already have timezone info.
 */
export function parseUtcMs(isoTime: string): number {
    // If the string already has timezone info (Z, +, or - after the time), use as-is
    if (isoTime.endsWith('Z') || isoTime.endsWith('z') ||
        /[+-]\d{2}:\d{2}$/.test(isoTime) || /[+-]\d{4}$/.test(isoTime)) {
        return new Date(isoTime).getTime();
    }
    // Otherwise append Z to force UTC interpretation
    return new Date(isoTime + 'Z').getTime();
}

/**
 * Convert an array of API time strings to Unix seconds (UTC).
 */
export function timesToUnixSeconds(times: string[]): number[] {
    return times.map(parseUtcSeconds);
}
