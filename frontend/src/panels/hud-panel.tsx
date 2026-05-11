import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { OverlayType } from '../state/url-state';
import { convertTemp, convertWind, convertPressure, convertWave } from '../units/converter';
import { getAqiCategory } from '../units/aqi';
import { parseUtcMs } from '../api/time-utils';

export interface HudPanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays?: Set<OverlayType>;
}

/**
 * Find the index in `times` whose ISO timestamp is closest to `now`.
 */
export function findNearestTimeIndex(times: string[], now: Date): number {
    if (times.length === 0) return 0;

    const nowMs = now.getTime();
    let bestIndex = 0;
    let bestDiff = Math.abs(parseUtcMs(times[0]) - nowMs);

    for (let i = 1; i < times.length; i++) {
        const diff = Math.abs(parseUtcMs(times[i]) - nowMs);
        if (diff < bestDiff) {
            bestDiff = diff;
            bestIndex = i;
        }
    }

    return bestIndex;
}

/** Check if a timestamp is within `maxHours` of now. */
function isRecent(timestamp: string, now: Date, maxHours: number): boolean {
    return Math.abs(now.getTime() - parseUtcMs(timestamp)) < maxHours * 3600 * 1000;
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
        case 'kts': return 'kn';
        case 'ms': return 'm/s';
    }
}

/** Get the unit label for wave height. */
function waveUnitLabel(unit: UnitPreferences['wave']): string {
    return unit === 'm' ? 'm' : 'ft';
}

/**
 * HUD Panel component — compact multi-row chip-based summary.
 * Matches the iOS app's Combined HUD (Section 9).
 *
 * Rows:
 * 1. Temp: Obs (yellow), HRRR (orange 0.7), Ens median (p10-p90) (orange)
 * 2. Wind: Obs (yellow), HRRR (green 0.7), Ens median (p10-p90) (green)
 * 3. Atms: HRRR Precip (green 0.7), Ens Precip (green), Cloud (gray)
 * 4. Solar: UV (purple), UV-Clear (purple 0.6) — only if UV data
 * 5. AQI: value-category colored by EPA — only if AQI data
 * 6. Tide: arrow + height (teal), next extreme — only if tide data
 */
export function HudPanel({ forecast, units, overlays }: HudPanelProps) {
    const now = new Date();
    const timeIndex = findNearestTimeIndex(forecast.ensemble.times, now);
    const stats = forecast.ensemble.statistics;
    const showHrrr = !overlays || overlays.has('hrrr');
    const showObs = !overlays || overlays.has('obs');

    // --- Row 1: Temperature ---
    const temperature = stats.temperature_2m?.median[timeIndex] ?? null;
    const tempP10 = stats.temperature_2m?.p10[timeIndex] ?? null;
    const tempP90 = stats.temperature_2m?.p90[timeIndex] ?? null;

    let obsTemp: number | null = null;
    if (showObs && forecast.observations?.entries) {
        const recent = forecast.observations.entries.find(e =>
            isRecent(e.timestamp, now, 2) && e.temperature_celsius != null
        );
        if (recent) obsTemp = recent.temperature_celsius;
    }

    let hrrrTemp: number | null = null;
    if (showHrrr && forecast.hrrr?.temperature_2m && forecast.hrrr.times) {
        const idx = findNearestTimeIndex(forecast.hrrr.times, now);
        if (isRecent(forecast.hrrr.times[idx], now, 1)) {
            hrrrTemp = forecast.hrrr.temperature_2m[idx] ?? null;
        }
    }

    // --- Row 2: Wind ---
    const windSpeed = stats.wind_speed_10m?.median[timeIndex] ?? null;
    const windP10 = stats.wind_speed_10m?.p10[timeIndex] ?? null;
    const windP90 = stats.wind_speed_10m?.p90[timeIndex] ?? null;

    let obsWind: number | null = null;
    if (showObs && forecast.observations?.entries) {
        const recent = forecast.observations.entries.find(e =>
            isRecent(e.timestamp, now, 2) && e.wind_speed_kmh != null
        );
        if (recent) obsWind = recent.wind_speed_kmh;
    }

    let hrrrWind: number | null = null;
    if (showHrrr && forecast.hrrr?.wind_speed_10m && forecast.hrrr.times) {
        const idx = findNearestTimeIndex(forecast.hrrr.times, now);
        if (isRecent(forecast.hrrr.times[idx], now, 1)) {
            hrrrWind = forecast.hrrr.wind_speed_10m[idx] ?? null;
        }
    }

    // --- Row 3: Atmospheric ---
    const precipProbAny = forecast.ensemble.precipitation_probability?.any[timeIndex] ?? null;
    const cloudCover = stats.cloud_cover?.median[timeIndex] ?? null;
    const cloudP10 = stats.cloud_cover?.p10[timeIndex] ?? null;
    const cloudP90 = stats.cloud_cover?.p90[timeIndex] ?? null;

    let hrrrPrecipProb: number | null = null;
    if (showHrrr && forecast.hrrr?.precipitation_probability && forecast.hrrr.times) {
        const idx = findNearestTimeIndex(forecast.hrrr.times, now);
        if (isRecent(forecast.hrrr.times[idx], now, 1)) {
            hrrrPrecipProb = forecast.hrrr.precipitation_probability[idx] ?? null;
        }
    }

    // --- Row 4: Solar/UV ---
    let uvIndex: number | null = null;
    let uvClearSky: number | null = null;
    if (forecast.uv) {
        const uvIdx = findNearestTimeIndex(forecast.uv.times, now);
        uvIndex = forecast.uv.uv_index[uvIdx] ?? null;
        uvClearSky = forecast.uv.uv_index_clear_sky[uvIdx] ?? null;
    }

    // --- Row 5: AQI ---
    let aqi: number | null = null;
    if (forecast.air_quality) {
        const aqiIdx = findNearestTimeIndex(forecast.air_quality.times, now);
        aqi = forecast.air_quality.us_aqi[aqiIdx] ?? null;
    }
    const aqiCategory = aqi != null ? getAqiCategory(Math.round(aqi)) : null;

    // --- Row 6: Tide ---
    let tideHeight: number | null = null;
    let tideRising = false;
    let nextExtreme: { type: 'High' | 'Low'; time: string } | null = null;
    if (forecast.tides && forecast.tides.predictions.length > 0) {
        const preds = forecast.tides.predictions;
        const nowMs = now.getTime();

        // Find nearest prediction
        let nearestIdx = 0;
        let nearestDiff = Math.abs(parseUtcMs(preds[0].time) - nowMs);
        for (let i = 1; i < preds.length; i++) {
            const diff = Math.abs(parseUtcMs(preds[i].time) - nowMs);
            if (diff < nearestDiff) {
                nearestDiff = diff;
                nearestIdx = i;
            }
        }
        tideHeight = preds[nearestIdx].height_m;

        // Determine if rising
        if (nearestIdx < preds.length - 1) {
            tideRising = preds[nearestIdx + 1].height_m > preds[nearestIdx].height_m;
        }

        // Find next extreme (local max or min after now)
        for (let i = nearestIdx + 1; i < preds.length - 1; i++) {
            const prev = preds[i - 1].height_m;
            const curr = preds[i].height_m;
            const next = preds[i + 1].height_m;
            if (curr > prev && curr > next) {
                nextExtreme = { type: 'High', time: preds[i].time };
                break;
            }
            if (curr < prev && curr < next) {
                nextExtreme = { type: 'Low', time: preds[i].time };
                break;
            }
        }
    }

    const tUnit = tempUnitLabel(units.temperature);
    const wUnit = windUnitLabel(units.wind);
    const wvUnit = waveUnitLabel(units.wave);

    return (
        <div class="hud-panel">
            {/* Row 1: Temperature */}
            <div class="hud-panel__row">
                <span class="hud-panel__row-label">Temp</span>
                <div class="hud-panel__chips">
                    {obsTemp != null && (
                        <span class="hud-chip hud-chip--obs">
                            <span class="hud-chip__label">Obs:</span>
                            <span class="hud-chip__value">{fmtInt(convertTemp(obsTemp, units.temperature))}{tUnit}</span>
                        </span>
                    )}
                    {hrrrTemp != null && (
                        <span class="hud-chip hud-chip--hrrr-temp">
                            <span class="hud-chip__label">HRRR:</span>
                            <span class="hud-chip__value">{fmtInt(convertTemp(hrrrTemp, units.temperature))}{tUnit}</span>
                        </span>
                    )}
                    {temperature != null && (
                        <span class="hud-chip hud-chip--ens-temp">
                            <span class="hud-chip__label">Ens:</span>
                            <span class="hud-chip__value">
                                {fmtInt(convertTemp(temperature, units.temperature))}
                                {tempP10 != null && tempP90 != null && (
                                    <span class="hud-chip__range">
                                        ({fmtInt(convertTemp(tempP10, units.temperature))}–{fmtInt(convertTemp(tempP90, units.temperature))})
                                    </span>
                                )}
                                {tUnit}
                            </span>
                        </span>
                    )}
                </div>
            </div>

            {/* Row 2: Wind */}
            <div class="hud-panel__row">
                <span class="hud-panel__row-label">Wind</span>
                <div class="hud-panel__chips">
                    {obsWind != null && (
                        <span class="hud-chip hud-chip--obs">
                            <span class="hud-chip__label">Obs:</span>
                            <span class="hud-chip__value">{fmtInt(convertWind(obsWind, units.wind))} {wUnit}</span>
                        </span>
                    )}
                    {hrrrWind != null && (
                        <span class="hud-chip hud-chip--hrrr-wind">
                            <span class="hud-chip__label">HRRR:</span>
                            <span class="hud-chip__value">{fmtInt(convertWind(hrrrWind, units.wind))} {wUnit}</span>
                        </span>
                    )}
                    {windSpeed != null && (
                        <span class="hud-chip hud-chip--ens-wind">
                            <span class="hud-chip__label">Ens:</span>
                            <span class="hud-chip__value">
                                {fmtInt(convertWind(windSpeed, units.wind))}
                                {windP10 != null && windP90 != null && (
                                    <span class="hud-chip__range">
                                        ({fmtInt(convertWind(windP10, units.wind))}–{fmtInt(convertWind(windP90, units.wind))})
                                    </span>
                                )}
                                {' '}{wUnit}
                            </span>
                        </span>
                    )}
                </div>
            </div>

            {/* Row 3: Atmospheric */}
            <div class="hud-panel__row">
                <span class="hud-panel__row-label">Atms</span>
                <div class="hud-panel__chips">
                    {hrrrPrecipProb != null && (
                        <span class="hud-chip hud-chip--hrrr-precip">
                            <span class="hud-chip__label">HRRR Precip:</span>
                            <span class="hud-chip__value">{fmtInt(hrrrPrecipProb)}%</span>
                        </span>
                    )}
                    {precipProbAny != null && (
                        <span class="hud-chip hud-chip--ens-precip">
                            <span class="hud-chip__label">Ens Precip:</span>
                            <span class="hud-chip__value">{fmtInt(precipProbAny)}%</span>
                        </span>
                    )}
                    {cloudCover != null && (
                        <span class="hud-chip hud-chip--cloud">
                            <span class="hud-chip__label">Cloud:</span>
                            <span class="hud-chip__value">
                                {fmtInt(cloudCover)}%
                                {cloudP10 != null && cloudP90 != null && (
                                    <span class="hud-chip__range">
                                        ({fmtInt(cloudP10)}–{fmtInt(cloudP90)})
                                    </span>
                                )}
                            </span>
                        </span>
                    )}
                </div>
            </div>

            {/* Row 4: Solar/UV (only if UV data available) */}
            {uvIndex != null && (
                <div class="hud-panel__row">
                    <span class="hud-panel__row-label">Solar</span>
                    <div class="hud-panel__chips">
                        <span class="hud-chip hud-chip--uv">
                            <span class="hud-chip__label">UV:</span>
                            <span class="hud-chip__value">{fmt(uvIndex)}</span>
                        </span>
                        {uvClearSky != null && (
                            <span class="hud-chip hud-chip--uv-clear">
                                <span class="hud-chip__label">UV-Clear:</span>
                                <span class="hud-chip__value">{fmt(uvClearSky)}</span>
                            </span>
                        )}
                    </div>
                </div>
            )}

            {/* Row 5: AQI (only if air quality data available) */}
            {aqi != null && aqiCategory && (
                <div class="hud-panel__row">
                    <span class="hud-panel__row-label">AQI</span>
                    <div class="hud-panel__chips">
                        <span class="hud-chip" style={{ color: aqiCategory.color }}>
                            <span class="hud-chip__value">{fmtInt(aqi)}-{aqiCategory.category}</span>
                        </span>
                    </div>
                </div>
            )}

            {/* Row 6: Tide (only if tide data available) */}
            {tideHeight != null && (
                <div class="hud-panel__row">
                    <span class="hud-panel__row-label">Tide</span>
                    <div class="hud-panel__chips">
                        <span class="hud-chip hud-chip--tide">
                            <span class="hud-chip__value">
                                {tideRising ? '↑' : '↓'} {fmt(convertWave(tideHeight, units.wave))} {wvUnit}
                            </span>
                        </span>
                        {nextExtreme && (
                            <span class="hud-chip hud-chip--tide-next">
                                <span class="hud-chip__value">
                                    {nextExtreme.type} {new Date(parseUtcMs(nextExtreme.time)).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false })}
                                </span>
                            </span>
                        )}
                    </div>
                </div>
            )}
        </div>
    );
}
