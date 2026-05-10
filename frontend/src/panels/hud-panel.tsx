import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import { convertTemp, convertWind, convertPressure, convertWave } from '../units/converter';
import { getAqiCategory } from '../units/aqi';

export interface HudPanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
}

/**
 * Find the index in `times` whose ISO timestamp is closest to `now`.
 */
export function findNearestTimeIndex(times: string[], now: Date): number {
    if (times.length === 0) return 0;

    const nowMs = now.getTime();
    let bestIndex = 0;
    let bestDiff = Math.abs(new Date(times[0]).getTime() - nowMs);

    for (let i = 1; i < times.length; i++) {
        const diff = Math.abs(new Date(times[i]).getTime() - nowMs);
        if (diff < bestDiff) {
            bestDiff = diff;
            bestIndex = i;
        }
    }

    return bestIndex;
}

/**
 * Find the nearest tide prediction to `now`.
 */
function findNearestTidePrediction(
    predictions: { time: string; height_m: number }[],
    now: Date
): { time: string; height_m: number } | null {
    if (predictions.length === 0) return null;

    const nowMs = now.getTime();
    let bestIndex = 0;
    let bestDiff = Math.abs(new Date(predictions[0].time).getTime() - nowMs);

    for (let i = 1; i < predictions.length; i++) {
        const diff = Math.abs(new Date(predictions[i].time).getTime() - nowMs);
        if (diff < bestDiff) {
            bestDiff = diff;
            bestIndex = i;
        }
    }

    return predictions[bestIndex];
}

/** Format a number for display, rounding to one decimal place. */
function fmt(value: number | null | undefined): string {
    if (value == null) return '—';
    return value.toFixed(1);
}

/** Format an integer value for display. */
function fmtInt(value: number | null | undefined): string {
    if (value == null) return '—';
    return Math.round(value).toString();
}

/** Get the unit label for temperature. */
function tempUnitLabel(unit: UnitPreferences['temperature']): string {
    return unit === 'C' ? '°C' : '°F';
}

/** Get the unit label for wind speed. */
function windUnitLabel(unit: UnitPreferences['wind']): string {
    switch (unit) {
        case 'kmh': return 'km/h';
        case 'mph': return 'mph';
        case 'kts': return 'kts';
        case 'ms': return 'm/s';
    }
}

/** Get the unit label for pressure. */
function pressureUnitLabel(unit: UnitPreferences['pressure']): string {
    switch (unit) {
        case 'hPa': return 'hPa';
        case 'inHg': return 'inHg';
        case 'mmHg': return 'mmHg';
    }
}

/** Get the unit label for wave height. */
function waveUnitLabel(unit: UnitPreferences['wave']): string {
    return unit === 'm' ? 'm' : 'ft';
}

/**
 * HUD Panel component.
 * Displays current conditions at a glance: temperature, wind, humidity,
 * cloud cover, pressure, UV index, shortwave radiation, AQI, and tide height.
 *
 * Validates: Requirements 5.1, 5.2, 5.3, 5.4, 5.5, 5.6, 5.7
 */
export function HudPanel({ forecast, units }: HudPanelProps) {
    const now = new Date();
    const timeIndex = findNearestTimeIndex(forecast.ensemble.times, now);
    const stats = forecast.ensemble.statistics;

    // Extract median values at the nearest time step
    const temperature = stats.temperature_2m?.median[timeIndex] ?? null;
    const windSpeed = stats.wind_speed_10m?.median[timeIndex] ?? null;
    const windDirection = stats.wind_direction_10m?.median[timeIndex] ?? null;
    const humidity = stats.relative_humidity_2m?.median[timeIndex] ?? null;
    const cloudCover = stats.cloud_cover?.median[timeIndex] ?? null;
    const pressure = stats.pressure_msl?.median[timeIndex] ?? null;
    const shortwaveRadiation = stats.shortwave_radiation?.median[timeIndex] ?? null;

    // UV index from separate source (find nearest time in UV array)
    let uvIndex: number | null = null;
    if (forecast.uv) {
        const uvTimeIndex = findNearestTimeIndex(forecast.uv.times, now);
        uvIndex = forecast.uv.uv_index[uvTimeIndex] ?? null;
    }

    // AQI from separate source (find nearest time in air quality array)
    let aqi: number | null = null;
    if (forecast.air_quality) {
        const aqiTimeIndex = findNearestTimeIndex(forecast.air_quality.times, now);
        aqi = forecast.air_quality.us_aqi[aqiTimeIndex] ?? null;
    }

    // Tide height from predictions
    let tideHeight: number | null = null;
    if (forecast.tides) {
        const nearestTide = findNearestTidePrediction(forecast.tides.predictions, now);
        tideHeight = nearestTide?.height_m ?? null;
    }

    // Apply unit conversions
    const displayTemp = temperature != null ? convertTemp(temperature, units.temperature) : null;
    const displayWind = windSpeed != null ? convertWind(windSpeed, units.wind) : null;
    const displayPressure = pressure != null ? convertPressure(pressure, units.pressure) : null;
    const displayTide = tideHeight != null ? convertWave(tideHeight, units.wave) : null;

    // AQI category for coloring
    const aqiCategory = aqi != null ? getAqiCategory(Math.round(aqi)) : null;

    return (
        <div class="hud-panel">
            <div class="hud-panel__stat">
                <span class="hud-panel__stat-label">Temperature</span>
                <span class="hud-panel__stat-value">
                    {fmt(displayTemp)}
                    <span class="hud-panel__stat-unit">{tempUnitLabel(units.temperature)}</span>
                </span>
            </div>

            <div class="hud-panel__stat">
                <span class="hud-panel__stat-label">Wind Speed</span>
                <span class="hud-panel__stat-value">
                    {fmt(displayWind)}
                    <span class="hud-panel__stat-unit">{windUnitLabel(units.wind)}</span>
                </span>
            </div>

            <div class="hud-panel__stat">
                <span class="hud-panel__stat-label">Wind Direction</span>
                <span class="hud-panel__stat-value">
                    {fmtInt(windDirection)}
                    <span class="hud-panel__stat-unit">°</span>
                </span>
            </div>

            <div class="hud-panel__stat">
                <span class="hud-panel__stat-label">Humidity</span>
                <span class="hud-panel__stat-value">
                    {fmtInt(humidity)}
                    <span class="hud-panel__stat-unit">%</span>
                </span>
            </div>

            <div class="hud-panel__stat">
                <span class="hud-panel__stat-label">Cloud Cover</span>
                <span class="hud-panel__stat-value">
                    {fmtInt(cloudCover)}
                    <span class="hud-panel__stat-unit">%</span>
                </span>
            </div>

            <div class="hud-panel__stat">
                <span class="hud-panel__stat-label">Pressure</span>
                <span class="hud-panel__stat-value">
                    {fmt(displayPressure)}
                    <span class="hud-panel__stat-unit">{pressureUnitLabel(units.pressure)}</span>
                </span>
            </div>

            <div class="hud-panel__stat">
                <span class="hud-panel__stat-label">UV Index</span>
                <span class="hud-panel__stat-value">
                    {fmt(uvIndex)}
                </span>
            </div>

            <div class="hud-panel__stat">
                <span class="hud-panel__stat-label">Solar Radiation</span>
                <span class="hud-panel__stat-value">
                    {fmtInt(shortwaveRadiation)}
                    <span class="hud-panel__stat-unit">W/m²</span>
                </span>
            </div>

            {aqi != null && aqiCategory && (
                <div class="hud-panel__stat" style={{ borderLeft: `4px solid ${aqiCategory.color}` }}>
                    <span class="hud-panel__stat-label">Air Quality</span>
                    <span class="hud-panel__stat-value" style={{ color: aqiCategory.color }}>
                        {fmtInt(aqi)}
                    </span>
                    <span class="hud-panel__stat-unit">{aqiCategory.category}</span>
                </div>
            )}

            {tideHeight != null && (
                <div class="hud-panel__stat">
                    <span class="hud-panel__stat-label">Tide Height</span>
                    <span class="hud-panel__stat-value">
                        {fmt(displayTide)}
                        <span class="hud-panel__stat-unit">{waveUnitLabel(units.wave)}</span>
                    </span>
                </div>
            )}
        </div>
    );
}
