import type { TempUnit, WindUnit, PressureUnit, PrecipUnit, WaveUnit } from './types';

// --- Forward conversions (metric → target unit) ---

/**
 * Convert temperature from Celsius to the target unit.
 * Formula: F = C × 9/5 + 32
 *
 * Validates: Requirements 22.1
 */
export function convertTemp(celsius: number, to: TempUnit): number {
    if (to === 'C') return celsius;
    return celsius * 9 / 5 + 32;
}

/**
 * Convert wind speed from km/h to the target unit.
 * Formulas: mph = kmh ÷ 1.609344, knots = kmh ÷ 1.852, m/s = kmh ÷ 3.6
 *
 * Validates: Requirements 22.2
 */
export function convertWind(kmh: number, to: WindUnit): number {
    switch (to) {
        case 'kmh': return kmh;
        case 'mph': return kmh / 1.609344;
        case 'kts': return kmh / 1.852;
        case 'ms': return kmh / 3.6;
    }
}

/**
 * Convert pressure from hPa to the target unit.
 * Formulas: inHg = hPa ÷ 33.8639, mmHg = hPa × 0.750062
 *
 * Validates: Requirements 22.3
 */
export function convertPressure(hpa: number, to: PressureUnit): number {
    switch (to) {
        case 'hPa': return hpa;
        case 'inHg': return hpa / 33.8639;
        case 'mmHg': return hpa * 0.750062;
    }
}

/**
 * Convert precipitation from mm to the target unit.
 * Formula: inches = mm ÷ 25.4
 *
 * Validates: Requirements 22.4
 */
export function convertPrecip(mm: number, to: PrecipUnit): number {
    if (to === 'mm') return mm;
    return mm / 25.4;
}

/**
 * Convert wave height from meters to the target unit.
 * Formula: feet = meters × 3.28084
 *
 * Validates: Requirements 22.5
 */
export function convertWave(meters: number, to: WaveUnit): number {
    if (to === 'm') return meters;
    return meters * 3.28084;
}

// --- Inverse conversions (target unit → metric) ---

/**
 * Convert temperature from the given unit back to Celsius.
 * Inverse of: F = C × 9/5 + 32 → C = (F - 32) × 5/9
 *
 * Validates: Requirements 22.6
 */
export function convertTempInverse(value: number, from: TempUnit): number {
    if (from === 'C') return value;
    return (value - 32) * 5 / 9;
}

/**
 * Convert wind speed from the given unit back to km/h.
 * Inverse of the forward formulas.
 *
 * Validates: Requirements 22.6
 */
export function convertWindInverse(value: number, from: WindUnit): number {
    switch (from) {
        case 'kmh': return value;
        case 'mph': return value * 1.609344;
        case 'kts': return value * 1.852;
        case 'ms': return value * 3.6;
    }
}

/**
 * Convert pressure from the given unit back to hPa.
 * Inverse of the forward formulas.
 *
 * Validates: Requirements 22.6
 */
export function convertPressureInverse(value: number, from: PressureUnit): number {
    switch (from) {
        case 'hPa': return value;
        case 'inHg': return value * 33.8639;
        case 'mmHg': return value / 0.750062;
    }
}

/**
 * Convert precipitation from the given unit back to mm.
 * Inverse of: inches = mm ÷ 25.4 → mm = inches × 25.4
 *
 * Validates: Requirements 22.6
 */
export function convertPrecipInverse(value: number, from: PrecipUnit): number {
    if (from === 'mm') return value;
    return value * 25.4;
}

/**
 * Convert wave height from the given unit back to meters.
 * Inverse of: feet = meters × 3.28084 → meters = feet ÷ 3.28084
 *
 * Validates: Requirements 22.6
 */
export function convertWaveInverse(value: number, from: WaveUnit): number {
    if (from === 'm') return value;
    return value / 3.28084;
}
