import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';
import { getAqiCategory } from '../units/aqi';
import uPlot from 'uplot';

export interface AirQualityPanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/**
 * Builds uPlot options for the AQI chart with EPA color bands.
 */
function buildAqiChartOptions(syncKey: string): uPlot.Options {
    return {
        width: 800,
        height: 200,
        series: [
            {},
            {
                label: 'US AQI',
                show: true,
                stroke: 'rgba(107, 114, 128, 0.9)',
                width: 2,
                points: { show: false },
            },
        ],
        axes: [
            {
                stroke: '#666',
                grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
            },
            {
                label: 'AQI',
                stroke: '#666',
                grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
                size: 60,
            },
        ],
        scales: {
            x: { time: true },
            y: { min: 0 },
        },
        cursor: {
            sync: { key: syncKey },
        },
        legend: { show: false },
        hooks: {
            draw: [
                (u: uPlot) => {
                    // Draw EPA color bands as background
                    const ctx = u.ctx;
                    ctx.save();
                    ctx.globalAlpha = 0.08;

                    const bands = [
                        { min: 0, max: 50, color: '#00e400' },
                        { min: 50, max: 100, color: '#ffff00' },
                        { min: 100, max: 150, color: '#ff7e00' },
                        { min: 150, max: 200, color: '#ff0000' },
                        { min: 200, max: 300, color: '#8f3f97' },
                        { min: 300, max: 500, color: '#7e0023' },
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
            ],
        },
    };
}

/**
 * Builds uPlot options for PM2.5 / PM10 charts.
 */
function buildPmChartOptions(label: string, syncKey: string): uPlot.Options {
    return {
        width: 800,
        height: 150,
        series: [
            {},
            {
                label,
                show: true,
                stroke: 'rgba(107, 114, 128, 0.9)',
                width: 1.5,
                fill: 'rgba(107, 114, 128, 0.1)',
                points: { show: false },
            },
        ],
        axes: [
            {
                stroke: '#666',
                grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
            },
            {
                label: `${label} (µg/m³)`,
                stroke: '#666',
                grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
                size: 60,
            },
        ],
        scales: {
            x: { time: true },
            y: { min: 0 },
        },
        cursor: {
            sync: { key: syncKey },
        },
        legend: { show: false },
    };
}

/**
 * Air Quality Panel.
 * Displays AQI chart with EPA color bands, PM2.5 and PM10 charts.
 * Shows AQI as integers with EPA category label.
 *
 * Validates: Requirements 13.1, 13.2, 13.3
 */
export function AirQualityPanel({ forecast, units: _units, overlays: _overlays, zoom: _zoom }: AirQualityPanelProps) {
    const { air_quality } = forecast;

    if (!air_quality) return null;

    const times = air_quality.times.map(t => Math.floor(new Date(t).getTime() / 1000));

    // Current AQI value and category for header display
    const currentAqi = air_quality.us_aqi[0];
    const aqiInt = currentAqi != null ? Math.round(currentAqi) : null;
    const aqiCat = aqiInt != null && aqiInt >= 0 && aqiInt <= 500
        ? getAqiCategory(aqiInt)
        : null;

    // AQI chart
    const aqiData: uPlot.AlignedData = [times, air_quality.us_aqi] as uPlot.AlignedData;
    const aqiOptions = buildAqiChartOptions(CHART_SYNC_KEY);

    // PM2.5 chart
    const pm25Data: uPlot.AlignedData = [times, air_quality.pm2_5] as uPlot.AlignedData;
    const pm25Options = buildPmChartOptions('PM2.5', CHART_SYNC_KEY);

    // PM10 chart
    const pm10Data: uPlot.AlignedData = [times, air_quality.pm10] as uPlot.AlignedData;
    const pm10Options = buildPmChartOptions('PM10', CHART_SYNC_KEY);

    return (
        <div class="panel">
            <div class="panel__header">
                Air Quality
                {aqiInt != null && aqiCat && (
                    <span class="panel__header-badge" style={{ color: aqiCat.color }}>
                        {' '}{aqiInt} — {aqiCat.category}
                    </span>
                )}
            </div>
            <div class="panel__body">
                <ChartWrapper options={aqiOptions} data={aqiData} />
                <ChartWrapper options={pm25Options} data={pm25Data} />
                <ChartWrapper options={pm10Options} data={pm10Data} />
            </div>
        </div>
    );
}
