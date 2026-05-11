import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { convertWave, convertTemp } from '../units/converter';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';
import { CURRENT_TIME_STROKE, CURRENT_TIME_WIDTH, DAY_SHADE_FILL } from '../charts/colors';
import { timesToUnixSeconds, parseUtcMs } from '../api/time-utils';
import { ZOOM_DURATION_SECONDS } from '../charts/zoom';
import { createCrosshairTooltipHook } from '../charts/hooks';
import uPlot from 'uplot';

export interface MarinePanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/** Wave height unit label for axis display. */
function waveUnitLabel(unit: UnitPreferences['wave']): string {
    return unit === 'm' ? 'm' : 'ft';
}

/** Convert degrees to a wave direction arrow character. */
function waveArrow(degrees: number): string {
    const adjusted = (degrees + 180) % 360;
    const index = Math.round(adjusted / 45) % 8;
    const arrows = ['↑', '↗', '→', '↘', '↓', '↙', '←', '↖'];
    return arrows[index];
}

/** Build a current time draw hook for non-fan charts. */
function currentTimeHook(): (u: uPlot) => void {
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

/** Day shading hook — shades day areas with a subtle lighter overlay. */
function buildDayShadingHook(times: number[], sunAltitude: number[]): (u: uPlot) => void {
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
 * Builds uPlot options for the wave height chart.
 * Blue line with light fill, wave direction arrows drawn via hook.
 */
function buildWaveHeightOptions(
    label: string,
    syncKey: string,
    waveDirection?: (number | null)[],
    times?: number[],
    waveHeight?: (number | null)[],
): uPlot.Options {
    const drawHooks: ((u: uPlot) => void)[] = [currentTimeHook()];

    // Wave direction arrows every 6 time steps
    if (waveDirection && times && waveHeight) {
        drawHooks.push((u: uPlot) => {
            const ctx = u.ctx;
            ctx.save();
            ctx.fillStyle = 'rgba(96, 165, 250, 0.8)';
            ctx.font = `${11 * devicePixelRatio}px -apple-system, sans-serif`;
            ctx.textAlign = 'center';
            ctx.textBaseline = 'bottom';

            for (let i = 0; i < waveDirection.length; i += 6) {
                const dir = waveDirection[i];
                const h = waveHeight[i];
                if (dir == null || h == null) continue;

                const cx = u.valToPos(times[i], 'x', true);
                const cy = u.valToPos(h, 'y', true);

                if (cx >= u.bbox.left && cx <= u.bbox.left + u.bbox.width &&
                    cy >= u.bbox.top && cy <= u.bbox.top + u.bbox.height) {
                    ctx.fillText(waveArrow(dir), cx, cy - 4 * devicePixelRatio);
                }
            }
            ctx.restore();
        });
    }

    return {
        width: 800,
        height: 180,
        series: [
            {},
            {
                label,
                show: true,
                stroke: 'rgba(96, 165, 250, 0.9)',
                width: 2,
                fill: 'rgba(96, 165, 250, 0.1)',
                points: { show: false },
            },
        ],
        axes: [
            { stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' } },
            { label, stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' }, size: 60 },
        ],
        scales: { x: { time: true }, y: { min: 0, auto: true } },
        cursor: { sync: { key: syncKey } },
        legend: { show: false },
        hooks: { draw: drawHooks, setCursor: [createCrosshairTooltipHook()] },
    };
}

/**
 * Builds uPlot options for the SST chart.
 * Orange line.
 */
function buildSstOptions(label: string, syncKey: string): uPlot.Options {
    return {
        width: 800,
        height: 180,
        series: [
            {},
            {
                label,
                show: true,
                stroke: 'rgba(251, 146, 60, 0.9)',
                width: 2,
                points: { show: false },
            },
        ],
        axes: [
            { stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' } },
            { label, stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' }, size: 60 },
        ],
        scales: { x: { time: true }, y: { auto: true } },
        cursor: { sync: { key: syncKey } },
        legend: { show: false },
        hooks: { draw: [currentTimeHook()], setCursor: [createCrosshairTooltipHook()] },
    };
}

/**
 * Builds uPlot options for the tide chart.
 * Teal line with day/night shading and current time indicator.
 */
function buildTideChartOptions(
    syncKey: string,
    sunAltitude?: number[],
    sunAltitudeTimes?: number[],
): uPlot.Options {
    const series: uPlot.Series[] = [
        {},
        {
            label: 'Tide Height',
            show: true,
            stroke: 'rgba(45, 212, 191, 0.9)',
            width: 2,
            fill: 'rgba(45, 212, 191, 0.1)',
            points: { show: false },
        },
    ];

    // Draw hooks
    const drawHooks: ((u: uPlot) => void)[] = [];

    // Day/night shading
    if (sunAltitude && sunAltitudeTimes && sunAltitude.length > 0) {
        drawHooks.push(buildDayShadingHook(sunAltitudeTimes, sunAltitude));
    }

    // Current time indicator
    drawHooks.push(currentTimeHook());

    return {
        width: 800,
        height: 180,
        series,
        axes: [
            { stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' } },
            { label: 'Tide Height', stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' }, size: 60 },
        ],
        scales: { x: { time: true }, y: { auto: true } },
        cursor: { sync: { key: syncKey } },
        legend: { show: false },
        hooks: { draw: drawHooks, setCursor: [createCrosshairTooltipHook()] },
    };
}

/**
 * Marine Panel.
 * Displays wave height (with direction arrows), SST, and tide charts.
 * Shows unavailable states when marine data is absent.
 * Includes current time red vertical line on all charts.
 *
 * Matches iOS spec Section 19.
 */
export function MarinePanel({ forecast, units, overlays: _overlays, zoom }: MarinePanelProps) {
    const { marine, tides, water_temperature, astronomy } = forecast;

    console.log('[MarinePanel] Entry:', { marine: !!marine, tides: !!tides, water_temperature: !!water_temperature });
    if (marine) {
        console.log('[MarinePanel] marine.times.length:', marine.times.length, 'wave_height[0]:', marine.wave_height[0]);
    }

    // Hide entire panel when marine data is absent
    if (!marine && !tides && !water_temperature) {
        console.log('[MarinePanel] All data absent, returning null');
        return null;
    }

    const unitConvert = (v: number) => convertWave(v, units.wave);
    const tempConvert = (v: number) => convertTemp(v, units.temperature);
    const wUnit = waveUnitLabel(units.wave);
    const tUnit = units.temperature === 'C' ? '°C' : '°F';

    // Compute zoom x-scale
    const zoomDuration = ZOOM_DURATION_SECONDS[zoom];
    const nowSec = Date.now() / 1000;
    const xMin = nowSec - zoomDuration * 0.1;
    const xMax = xMin + zoomDuration;

    /** Apply zoom x-scale to chart options */
    function applyZoomScale(opts: uPlot.Options): uPlot.Options {
        return {
            ...opts,
            scales: { ...opts.scales, x: { time: true, min: xMin, max: xMax } as uPlot.Scale },
        };
    }

    // Wave height chart
    let waveHeightData: uPlot.AlignedData | null = null;
    let waveHeightOptions: uPlot.Options | null = null;

    if (marine) {
        const marineTimes = timesToUnixSeconds(marine.times);
        const convertedHeight = marine.wave_height.map(v => v != null ? unitConvert(v) : null);

        if (marineTimes[0] && !isNaN(marineTimes[0]) && convertedHeight.some(v => v != null)) {
            waveHeightData = [marineTimes, convertedHeight] as uPlot.AlignedData;
            waveHeightOptions = applyZoomScale(buildWaveHeightOptions(
                `Wave Height (${wUnit})`,
                CHART_SYNC_KEY,
                marine.wave_direction,
                marineTimes,
                convertedHeight,
            ));
        }
    }

    // SST chart — only if there are actual non-null SST values
    let sstData: uPlot.AlignedData | null = null;
    let sstOptions: uPlot.Options | null = null;

    if (marine?.sea_surface_temperature) {
        const hasAnySst = marine.sea_surface_temperature.some(v => v != null);
        if (hasAnySst) {
            const marineTimes = timesToUnixSeconds(marine.times);
            const convertedSst = marine.sea_surface_temperature.map(v => v != null ? tempConvert(v) : null);
            sstData = [marineTimes, convertedSst] as uPlot.AlignedData;
            sstOptions = applyZoomScale(buildSstOptions(`Sea Surface Temp (${tUnit})`, CHART_SYNC_KEY));
        }
    }

    // Tide chart with sun/moon altitude
    let tideData: uPlot.AlignedData | null = null;
    let tideOptions: uPlot.Options | null = null;

    if (tides && tides.predictions.length > 0) {
        const tideTimes = tides.predictions.map(p => parseUtcMs(p.time) / 1000 | 0);
        const tideHeights = tides.predictions.map(p => unitConvert(p.height_m));

        const dataArrays: (number | null | undefined)[][] = [tideTimes as any, tideHeights];

        // Sun altitude data for day/night shading
        const sunAlt = astronomy?.sun_altitude;
        const astroTimes = astronomy?.times ? timesToUnixSeconds(astronomy.times) : undefined;

        tideOptions = applyZoomScale(buildTideChartOptions(CHART_SYNC_KEY, sunAlt, astroTimes));
        tideData = [tideTimes, tideHeights] as uPlot.AlignedData;
    }

    // Water temperature display (when no SST chart but NOAA observation exists)
    const waterTemp = water_temperature?.temperature_celsius ?? null;
    const waterTempStation = water_temperature?.station?.name ?? null;

    return (
        <div class="panel">
            <div class="panel__header">
                <span class="panel__title">Marine</span>
                <div class="panel__legend">
                    {marine && (
                        <span class="panel__legend-item">
                            <span class="panel__legend-dot" style={{ backgroundColor: 'rgba(96, 165, 250, 0.9)' }} />
                            Wave
                        </span>
                    )}
                    {(sstData || waterTemp != null) && (
                        <span class="panel__legend-item">
                            <span class="panel__legend-dot" style={{ backgroundColor: 'rgba(251, 146, 60, 0.9)' }} />
                            SST
                        </span>
                    )}
                    {tides && (
                        <>
                            <span class="panel__legend-item">
                                <span class="panel__legend-dot" style={{ backgroundColor: 'rgba(45, 212, 191, 0.9)' }} />
                                Tide
                            </span>
                            <span class="panel__legend-item">
                                <span class="panel__legend-dot" style={{ backgroundColor: 'rgba(250, 204, 21, 0.6)' }} />
                                Sun
                            </span>
                            <span class="panel__legend-item">
                                <span class="panel__legend-dot" style={{ backgroundColor: 'rgba(156, 163, 175, 0.6)' }} />
                                Moon
                            </span>
                        </>
                    )}
                </div>
            </div>
            <div class="panel__body">
                {/* No marine data message */}
                {!marine && !tides && waterTemp == null && (
                    <div class="panel__empty-state">
                        🌊 Marine data is not available for this location.
                    </div>
                )}

                {/* Wave height chart */}
                {waveHeightOptions && waveHeightData && (
                    <ChartWrapper options={waveHeightOptions} data={waveHeightData} height={180} />
                )}

                {/* SST chart */}
                {sstOptions && sstData && (
                    <ChartWrapper options={sstOptions} data={sstData} height={180} />
                )}

                {/* NOAA water temperature (fallback when no SST chart) */}
                {!sstData && waterTemp != null && (
                    <div class="panel__water-temp">
                        🌡️ SST{waterTempStation && ` (${waterTempStation})`}: {tempConvert(waterTemp).toFixed(1)}{tUnit}
                    </div>
                )}

                {/* Tide chart */}
                {tideOptions && tideData && (
                    <ChartWrapper options={tideOptions} data={tideData} height={180} />
                )}
            </div>
        </div>
    );
}
