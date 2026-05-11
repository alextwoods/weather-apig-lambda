import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';
import { getAqiCategory } from '../units/aqi';
import { CURRENT_TIME_STROKE, CURRENT_TIME_WIDTH } from '../charts/colors';
import { timesToUnixSeconds, parseUtcMs } from '../api/time-utils';
import { ZOOM_DURATION_SECONDS } from '../charts/zoom';
import { createCrosshairTooltipHook } from '../charts/hooks';
import uPlot from 'uplot';

export interface AirQualityPanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/** Current time draw hook. */
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

/**
 * Builds uPlot options for the combined AQI chart.
 * Shows all three lines (AQI=blue, PM2.5=teal, PM10=gray) on one chart
 * with EPA color bands as background.
 * Matches iOS spec Section 17.3.
 */
function buildAqiChartOptions(syncKey: string, zoomLevel?: ZoomLevel): uPlot.Options {
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
        height: 200,
        series: [
            {},
            // US AQI — Blue, 2pt solid
            {
                label: 'US AQI',
                show: true,
                stroke: 'rgba(96, 165, 250, 1)',
                width: 2,
                points: { show: false },
            },
            // PM2.5 — Teal, 1.5pt solid
            {
                label: 'PM2.5',
                show: true,
                stroke: 'rgba(45, 212, 191, 1)',
                width: 1.5,
                points: { show: false },
            },
            // PM10 — Gray, 1.5pt solid
            {
                label: 'PM10',
                show: true,
                stroke: 'rgba(156, 163, 175, 1)',
                width: 1.5,
                points: { show: false },
            },
        ],
        axes: [
            { stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' } },
            { label: 'AQI / µg/m³', stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' }, size: 60 },
        ],
        scales: {
            x: xScale,
            y: { min: 0, max: 500 },
        },
        cursor: { sync: { key: syncKey } },
        legend: { show: false },
        hooks: {
            draw: [
                // EPA color bands
                (u: uPlot) => {
                    const ctx = u.ctx;
                    ctx.save();

                    const bands = [
                        { min: 0, max: 50, color: 'rgba(0, 228, 0, 0.15)' },       // Good - green
                        { min: 50, max: 100, color: 'rgba(255, 255, 0, 0.15)' },    // Moderate - yellow
                        { min: 100, max: 150, color: 'rgba(255, 126, 0, 0.15)' },   // USG - orange
                        { min: 150, max: 200, color: 'rgba(255, 0, 0, 0.15)' },     // Unhealthy - red
                        { min: 200, max: 300, color: 'rgba(143, 63, 151, 0.15)' },  // Very Unhealthy - purple
                        { min: 300, max: 500, color: 'rgba(128, 0, 0, 0.15)' },     // Hazardous - dark red
                    ];

                    for (const band of bands) {
                        const yTop = u.valToPos(band.max, 'y', true);
                        const yBot = u.valToPos(band.min, 'y', true);
                        if (isFinite(yTop) && isFinite(yBot)) {
                            ctx.fillStyle = band.color;
                            ctx.fillRect(
                                u.bbox.left,
                                Math.max(yTop, u.bbox.top),
                                u.bbox.width,
                                Math.min(yBot - yTop, u.bbox.height),
                            );
                        }
                    }

                    ctx.restore();
                },
                // Current time indicator
                currentTimeHook(),
            ],
            setCursor: [createCrosshairTooltipHook()],
        },
    };
}

/**
 * Air Quality Panel.
 * Displays a single combined chart with US AQI (blue), PM2.5 (teal), and PM10 (gray)
 * lines overlaid on EPA color bands.
 *
 * Matches iOS spec Section 17.
 */
export function AirQualityPanel({ forecast, units: _units, overlays: _overlays, zoom }: AirQualityPanelProps) {
    const { air_quality } = forecast;

    if (!air_quality) return null;

    const times = timesToUnixSeconds(air_quality.times);

    // Current AQI value and category for HUD row
    const now = new Date();
    const nowMs = now.getTime();
    let bestIdx = 0;
    let bestDiff = Math.abs(parseUtcMs(air_quality.times[0]) - nowMs);
    for (let i = 1; i < air_quality.times.length; i++) {
        const diff = Math.abs(parseUtcMs(air_quality.times[i]) - nowMs);
        if (diff < bestDiff) { bestDiff = diff; bestIdx = i; }
    }
    const currentAqi = air_quality.us_aqi[bestIdx];
    const aqiInt = currentAqi != null ? Math.round(currentAqi) : null;
    const aqiCat = aqiInt != null && aqiInt >= 0 && aqiInt <= 500
        ? getAqiCategory(aqiInt)
        : null;

    // Combined chart with all three series
    const aqiData: uPlot.AlignedData = [
        times,
        air_quality.us_aqi,
        air_quality.pm2_5,
        air_quality.pm10,
    ] as uPlot.AlignedData;
    const aqiOptions = buildAqiChartOptions(CHART_SYNC_KEY, zoom);

    return (
        <div class="panel">
            <div class="panel__header">
                <span class="panel__title">Air Quality (AQI)</span>
                <div class="panel__legend">
                    <span class="panel__legend-item">
                        <span class="panel__legend-dot" style={{ backgroundColor: 'rgba(96, 165, 250, 1)' }} />
                        US AQI
                    </span>
                    <span class="panel__legend-item">
                        <span class="panel__legend-dot" style={{ backgroundColor: 'rgba(45, 212, 191, 1)' }} />
                        PM2.5
                    </span>
                    <span class="panel__legend-item">
                        <span class="panel__legend-dot" style={{ backgroundColor: 'rgba(156, 163, 175, 1)' }} />
                        PM10
                    </span>
                </div>
            </div>
            {/* HUD row */}
            {aqiInt != null && aqiCat && (
                <div class="panel__hud-row">
                    <span class="panel__hud-chip" style={{ color: aqiCat.color }}>
                        <span class="panel__hud-chip-label">AQI:</span>
                        {aqiInt}-{aqiCat.category}
                    </span>
                </div>
            )}
            <div class="panel__body">
                <ChartWrapper options={aqiOptions} data={aqiData} height={200} />
            </div>
        </div>
    );
}
