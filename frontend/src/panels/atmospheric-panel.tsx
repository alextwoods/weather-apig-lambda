import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { buildMultiFanChartData, buildMultiFanChartOptions, type MultiFanChartConfig } from '../charts/fan-chart';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';
import { VARIABLE_COLORS } from '../charts/colors';
import { timesToUnixSeconds, parseUtcMs } from '../api/time-utils';
import { ZOOM_DURATION_SECONDS } from '../charts/zoom';
import uPlot from 'uplot';
import { CURRENT_TIME_STROKE, CURRENT_TIME_WIDTH, DAY_SHADE_FILL } from '../charts/colors';

export interface AtmosphericPanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/** Green color for precipitation probability lines */
const PRECIP_GREEN = 'rgba(74, 222, 128, 1)';

/**
 * Builds uPlot options for the precipitation probability chart.
 * Shows three lines: any (solid green), moderate (dashed green 0.7), heavy (dotted green 0.5).
 * Also shows area fills for moderate and heavy thresholds.
 * Includes current time indicator and night shading.
 */
function buildPrecipProbOptions(
    syncKey: string,
    times: number[],
    sunAltitude?: number[],
    hrrrAvailable?: boolean,
    zoomLevel?: ZoomLevel,
): uPlot.Options {
    const series: uPlot.Series[] = [
        {},
        // Any (>0.1mm) — solid green, 2pt
        {
            label: 'Any',
            show: true,
            stroke: PRECIP_GREEN,
            width: 2,
            points: { show: false },
        },
        // Moderate (>2.5mm) — dashed green at 0.7 opacity
        {
            label: '>2.5mm',
            show: true,
            stroke: 'rgba(74, 222, 128, 0.7)',
            width: 1.5,
            dash: [4, 3],
            fill: 'rgba(74, 222, 128, 0.25)',
            points: { show: false },
        },
        // Heavy (>7.5mm) — dotted green at 0.5 opacity
        {
            label: '>7.5mm',
            show: true,
            stroke: 'rgba(74, 222, 128, 0.5)',
            width: 1.5,
            dash: [2, 2],
            fill: 'rgba(74, 222, 128, 0.35)',
            points: { show: false },
        },
    ];

    // Add HRRR series if available
    if (hrrrAvailable) {
        series.push({
            label: 'HRRR',
            show: true,
            stroke: 'rgba(74, 222, 128, 0.5)',
            width: 1.5,
            dash: [4, 3],
            points: { show: false },
        });
    }

    // Draw hooks for current time and night shading
    const drawHooks: ((u: uPlot) => void)[] = [];

    if (sunAltitude && sunAltitude.length > 0) {
        drawHooks.push(buildNightShadingHookInline(times, sunAltitude));
    }
    drawHooks.push(buildCurrentTimeHookInline());

    // X-axis scale: apply zoom level if provided
    let xScale: uPlot.Scale = { time: true };
    if (zoomLevel) {
        const zoomDuration = ZOOM_DURATION_SECONDS[zoomLevel];
        const nowSec = Date.now() / 1000;
        const xMin = nowSec - zoomDuration * 0.1;
        const xMax = xMin + zoomDuration;
        xScale = { time: true, min: xMin, max: xMax } as uPlot.Scale;
    }

    return {
        width: 800,
        height: 180,
        series,
        axes: [
            { stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' } },
            { label: 'Probability (%)', stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' }, size: 60 },
        ],
        scales: {
            x: xScale,
            y: { min: 0, max: 100 },
        },
        cursor: { sync: { key: syncKey } },
        legend: { show: false },
        hooks: drawHooks.length > 0 ? { draw: drawHooks } : {},
    };
}

/** Inline current time hook (avoids circular import) */
function buildCurrentTimeHookInline(): (u: uPlot) => void {
    return (u: uPlot) => {
        const now = Date.now() / 1000;
        const cx = u.valToPos(now, 'x', true);
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

/** Inline day shading hook (shades day areas lighter) */
function buildNightShadingHookInline(times: number[], sunAltitude: number[]): (u: uPlot) => void {
    return (u: uPlot) => {
        const ctx = u.ctx;
        ctx.save();
        ctx.fillStyle = DAY_SHADE_FILL;
        let inDay = false;
        let dayStart = 0;
        for (let i = 0; i < sunAltitude.length; i++) {
            const isDay = sunAltitude[i] >= 0;
            if (isDay && !inDay) {
                if (i > 0 && sunAltitude[i - 1] < 0) {
                    const frac = -sunAltitude[i - 1] / (sunAltitude[i] - sunAltitude[i - 1]);
                    const interpTime = times[i - 1] + frac * (times[i] - times[i - 1]);
                    dayStart = u.valToPos(interpTime, 'x', true);
                } else {
                    dayStart = u.valToPos(times[i], 'x', true);
                }
                inDay = true;
            } else if (!isDay && inDay) {
                let dayEnd: number;
                if (i > 0 && sunAltitude[i - 1] >= 0) {
                    const frac = sunAltitude[i - 1] / (sunAltitude[i - 1] - sunAltitude[i]);
                    const interpTime = times[i - 1] + frac * (times[i] - times[i - 1]);
                    dayEnd = u.valToPos(interpTime, 'x', true);
                } else {
                    dayEnd = u.valToPos(times[i], 'x', true);
                }
                const x1 = Math.max(dayStart, u.bbox.left);
                const x2 = Math.min(dayEnd, u.bbox.left + u.bbox.width);
                if (x2 > x1) ctx.fillRect(x1, u.bbox.top, x2 - x1, u.bbox.height);
                inDay = false;
            }
        }
        if (inDay) {
            const dayEnd = u.bbox.left + u.bbox.width;
            const x1 = Math.max(dayStart, u.bbox.left);
            if (dayEnd > x1) ctx.fillRect(x1, u.bbox.top, dayEnd - x1, u.bbox.height);
        }
        ctx.restore();
    };
}

/**
 * Atmospheric Panel.
 * Displays:
 * 1. Cloud cover (gray) + Humidity (cyan) overlaid as fan charts
 * 2. Precipitation probability multi-threshold chart (green)
 *
 * Colors: Cloud=gray, Humidity=cyan, Precip=green
 * Matches iOS spec Section 14.
 */
export function AtmosphericPanel({ forecast, units: _units, overlays, zoom }: AtmosphericPanelProps) {
    const { ensemble, hrrr, astronomy } = forecast;
    const cloudStats = ensemble.statistics.cloud_cover;
    const humidityStats = ensemble.statistics.relative_humidity_2m;
    const precipProb = ensemble.precipitation_probability;

    if (!cloudStats && !humidityStats && !precipProb) return null;

    const times = timesToUnixSeconds(ensemble.times);
    const identity = (v: number) => v;

    // Cloud + Humidity overlaid fan chart
    let cloudHumidityData: uPlot.AlignedData | null = null;
    let cloudHumidityOptions: uPlot.Options | null = null;

    if (cloudStats || humidityStats) {
        const seriesConfigs: MultiFanChartConfig['series'] = [];
        if (cloudStats) {
            seriesConfigs.push({ stats: cloudStats, color: 'cloudCover', label: 'Cloud' });
        }
        if (humidityStats) {
            seriesConfigs.push({ stats: humidityStats, color: 'humidity', label: 'Humidity' });
        }

        const config: MultiFanChartConfig = {
            times,
            series: seriesConfigs,
            sunAltitude: astronomy?.sun_altitude,
            unitConverter: identity,
            axisLabel: '%',
            syncKey: CHART_SYNC_KEY,
            zoomLevel: zoom,
            height: 180,
            yMin: 0,
            yMax: 100,
        };

        cloudHumidityData = buildMultiFanChartData(config);
        cloudHumidityOptions = buildMultiFanChartOptions(config);
    }

    // Precipitation probability chart
    let precipProbData: uPlot.AlignedData | null = null;
    let precipProbOptions: uPlot.Options | null = null;

    if (precipProb) {
        const hrrrPrecipAvailable = overlays.has('hrrr') && !!hrrr?.precipitation_probability;
        precipProbOptions = buildPrecipProbOptions(
            CHART_SYNC_KEY,
            times,
            astronomy?.sun_altitude,
            hrrrPrecipAvailable,
            zoom,
        );

        const dataArrays: (number | null | undefined)[][] = [
            times as any,
            precipProb.any,
            precipProb.moderate,
            precipProb.heavy,
        ];

        if (hrrrPrecipAvailable && hrrr?.precipitation_probability) {
            // Align HRRR data to ensemble times (simple pass-through since they may differ)
            dataArrays.push(hrrr.precipitation_probability);
        }

        precipProbData = dataArrays as uPlot.AlignedData;
    }

    // Current values for HUD row
    const now = new Date();
    const nowMs = now.getTime();
    let nowIdx = 0;
    let bestDiff = Math.abs(parseUtcMs(ensemble.times[0]) - nowMs);
    for (let i = 1; i < ensemble.times.length; i++) {
        const diff = Math.abs(parseUtcMs(ensemble.times[i]) - nowMs);
        if (diff < bestDiff) { bestDiff = diff; nowIdx = i; }
    }

    const currentCloud = cloudStats?.median[nowIdx] ?? null;
    const currentCloudP10 = cloudStats?.p10[nowIdx] ?? null;
    const currentCloudP90 = cloudStats?.p90[nowIdx] ?? null;
    const currentHumidity = humidityStats?.median[nowIdx] ?? null;
    const currentHumidityP10 = humidityStats?.p10[nowIdx] ?? null;
    const currentHumidityP90 = humidityStats?.p90[nowIdx] ?? null;
    const currentPrecipAny = precipProb?.any[nowIdx] ?? null;

    // HRRR precip value (within 1 hour)
    let hrrrPrecip: number | null = null;
    if (overlays.has('hrrr') && hrrr?.precipitation_probability && hrrr.times) {
        let hrrrIdx = 0;
        let hrrrBestDiff = Math.abs(parseUtcMs(hrrr.times[0]) - nowMs);
        for (let i = 1; i < hrrr.times.length; i++) {
            const diff = Math.abs(parseUtcMs(hrrr.times[i]) - nowMs);
            if (diff < hrrrBestDiff) { hrrrBestDiff = diff; hrrrIdx = i; }
        }
        if (hrrrBestDiff < 3600 * 1000) {
            hrrrPrecip = hrrr.precipitation_probability[hrrrIdx] ?? null;
        }
    }

    return (
        <div class="panel">
            <div class="panel__header">
                <span class="panel__title">Atms (%)</span>
                <div class="panel__legend">
                    {cloudStats && (
                        <span class="panel__legend-item">
                            <span class="panel__legend-dot" style={{ backgroundColor: VARIABLE_COLORS.cloudCover.stroke }} />
                            Cloud
                        </span>
                    )}
                    {humidityStats && (
                        <span class="panel__legend-item">
                            <span class="panel__legend-dot" style={{ backgroundColor: VARIABLE_COLORS.humidity.stroke }} />
                            Humidity
                        </span>
                    )}
                    {precipProb && (
                        <>
                            <span class="panel__legend-item">
                                <span class="panel__legend-dot" style={{ backgroundColor: PRECIP_GREEN }} />
                                Any
                            </span>
                            <span class="panel__legend-item">
                                <span class="panel__legend-dot" style={{ backgroundColor: 'rgba(74, 222, 128, 0.7)' }} />
                                &gt;2.5
                            </span>
                            <span class="panel__legend-item">
                                <span class="panel__legend-dot" style={{ backgroundColor: 'rgba(74, 222, 128, 0.5)' }} />
                                &gt;7.5
                            </span>
                        </>
                    )}
                </div>
            </div>
            {/* HUD row */}
            <div class="panel__hud-row">
                {currentCloud != null && (
                    <span class="panel__hud-chip" style={{ color: VARIABLE_COLORS.cloudCover.stroke }}>
                        <span class="panel__hud-chip-label">Cloud:</span>
                        {Math.round(currentCloud)}%
                        {currentCloudP10 != null && currentCloudP90 != null && (
                            <span class="panel__hud-chip-range">
                                ({Math.round(currentCloudP10)}–{Math.round(currentCloudP90)})
                            </span>
                        )}
                    </span>
                )}
                {currentHumidity != null && (
                    <span class="panel__hud-chip" style={{ color: VARIABLE_COLORS.humidity.stroke }}>
                        <span class="panel__hud-chip-label">Hum:</span>
                        {Math.round(currentHumidity)}%
                        {currentHumidityP10 != null && currentHumidityP90 != null && (
                            <span class="panel__hud-chip-range">
                                ({Math.round(currentHumidityP10)}–{Math.round(currentHumidityP90)})
                            </span>
                        )}
                    </span>
                )}
                {hrrrPrecip != null && (
                    <span class="panel__hud-chip" style={{ color: 'rgba(74, 222, 128, 0.7)' }}>
                        <span class="panel__hud-chip-label">HRRR Precip:</span>
                        {Math.round(hrrrPrecip)}%
                    </span>
                )}
                {currentPrecipAny != null && (
                    <span class="panel__hud-chip" style={{ color: PRECIP_GREEN }}>
                        <span class="panel__hud-chip-label">Precip:</span>
                        {Math.round(currentPrecipAny)}%
                    </span>
                )}
            </div>
            <div class="panel__body">
                {cloudHumidityOptions && cloudHumidityData && (
                    <ChartWrapper options={cloudHumidityOptions} data={cloudHumidityData} height={180} />
                )}
                {precipProbOptions && precipProbData && (
                    <ChartWrapper options={precipProbOptions} data={precipProbData} height={180} />
                )}
            </div>
        </div>
    );
}
