import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { buildFanChartData, buildFanChartOptions } from '../charts/fan-chart';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';
import uPlot from 'uplot';

export interface SolarPanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/** UV severity color based on index value. */
function uvColor(value: number): string {
    if (value <= 2) return '#4ade80';   // Low (green)
    if (value <= 5) return '#facc15';   // Moderate (yellow)
    if (value <= 7) return '#fb923c';   // High (orange)
    if (value <= 10) return '#ef4444';  // Very High (red)
    return '#a855f7';                   // Extreme (purple)
}

/**
 * Builds uPlot options for the UV index chart with color-coded severity.
 */
function buildUvChartOptions(syncKey: string): uPlot.Options {
    return {
        width: 800,
        height: 200,
        series: [
            {},
            {
                label: 'UV Index',
                show: true,
                stroke: 'rgba(168, 85, 247, 0.9)',
                width: 2,
                fill: 'rgba(168, 85, 247, 0.1)',
                points: { show: false },
            },
        ],
        axes: [
            {
                stroke: '#666',
                grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
            },
            {
                label: 'UV Index',
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
                    // Draw color-coded background bands for UV severity levels
                    const ctx = u.ctx;
                    ctx.save();
                    ctx.globalAlpha = 0.05;

                    const bands = [
                        { min: 0, max: 2, color: '#4ade80' },
                        { min: 2, max: 5, color: '#facc15' },
                        { min: 5, max: 7, color: '#fb923c' },
                        { min: 7, max: 10, color: '#ef4444' },
                        { min: 10, max: 15, color: '#a855f7' },
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
 * Solar Panel.
 * Displays UV index chart (color-coded severity), shortwave radiation fan chart,
 * and sun altitude background overlay for day/night periods.
 *
 * Validates: Requirements 12.1, 12.2, 12.3, 12.4
 */
export function SolarPanel({ forecast, units: _units, overlays: _overlays, zoom }: SolarPanelProps) {
    const { ensemble, uv, astronomy } = forecast;
    const radiationStats = ensemble.statistics.shortwave_radiation;

    const hasSomething = uv || radiationStats;
    if (!hasSomething) return null;

    const ensembleTimes = ensemble.times.map(t => Math.floor(new Date(t).getTime() / 1000));
    const identity = (v: number) => v;

    // UV index chart
    let uvData: uPlot.AlignedData | null = null;
    let uvOptions: uPlot.Options | null = null;

    if (uv) {
        const uvTimes = uv.times.map(t => Math.floor(new Date(t).getTime() / 1000));
        uvData = [uvTimes, uv.uv_index] as uPlot.AlignedData;
        uvOptions = buildUvChartOptions(CHART_SYNC_KEY);
    }

    // Shortwave radiation fan chart with sun altitude background
    let radiationData: ReturnType<typeof buildFanChartData> | null = null;
    let radiationOptions: ReturnType<typeof buildFanChartOptions> | null = null;

    if (radiationStats) {
        const radiationConfig = {
            times: ensembleTimes,
            stats: radiationStats,
            sunAltitude: astronomy?.sun_altitude,
            unitConverter: identity,
            label: 'Solar Radiation (W/m²)',
            syncKey: CHART_SYNC_KEY,
            zoomLevel: zoom,
        };
        radiationData = buildFanChartData(radiationConfig);
        radiationOptions = buildFanChartOptions(radiationConfig);
    }

    return (
        <div class="panel">
            <div class="panel__header">Solar</div>
            <div class="panel__body">
                {uvOptions && uvData && (
                    <ChartWrapper options={uvOptions} data={uvData} />
                )}
                {radiationOptions && radiationData && (
                    <ChartWrapper options={radiationOptions} data={radiationData} />
                )}
            </div>
        </div>
    );
}

// Export for potential use in tooltips
export { uvColor };
