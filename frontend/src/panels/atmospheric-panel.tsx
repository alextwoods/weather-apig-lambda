import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { buildFanChartData, buildFanChartOptions, type FanChartConfig } from '../charts/fan-chart';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';
import uPlot from 'uplot';

export interface AtmosphericPanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/**
 * Builds uPlot options for a precipitation probability multi-line chart.
 * Shows three lines: any, moderate, and heavy probability thresholds.
 */
function buildPrecipProbOptions(syncKey: string): uPlot.Options {
    return {
        width: 800,
        height: 200,
        series: [
            {},
            {
                label: 'Any',
                show: true,
                stroke: 'rgba(59, 130, 246, 0.9)',
                width: 2,
                points: { show: false },
            },
            {
                label: 'Moderate',
                show: true,
                stroke: 'rgba(245, 158, 11, 0.9)',
                width: 2,
                points: { show: false },
            },
            {
                label: 'Heavy',
                show: true,
                stroke: 'rgba(239, 68, 68, 0.9)',
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
                label: 'Probability (%)',
                stroke: '#666',
                grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
                size: 60,
            },
        ],
        scales: {
            x: { time: true },
            y: { min: 0, max: 100 },
        },
        cursor: {
            sync: { key: syncKey },
        },
        legend: { show: true },
    };
}

/**
 * Atmospheric Panel.
 * Displays fan charts for cloud_cover and relative_humidity_2m,
 * plus a precipitation probability multi-line chart.
 *
 * Validates: Requirements 10.1, 10.2, 10.3, 10.4
 */
export function AtmosphericPanel({ forecast, units: _units, overlays: _overlays, zoom }: AtmosphericPanelProps) {
    const { ensemble } = forecast;
    const cloudStats = ensemble.statistics.cloud_cover;
    const humidityStats = ensemble.statistics.relative_humidity_2m;
    const precipProb = ensemble.precipitation_probability;

    const times = ensemble.times.map(t => Math.floor(new Date(t).getTime() / 1000));
    const identity = (v: number) => v;

    // Cloud cover fan chart (percentage, 0-100%)
    let cloudData: ReturnType<typeof buildFanChartData> | null = null;
    let cloudOptions: ReturnType<typeof buildFanChartOptions> | null = null;

    if (cloudStats) {
        const cloudConfig: FanChartConfig = {
            times,
            stats: cloudStats,
            unitConverter: identity,
            label: 'Cloud Cover (%)',
            syncKey: CHART_SYNC_KEY,
            zoomLevel: zoom,
        };
        cloudData = buildFanChartData(cloudConfig);
        cloudOptions = buildFanChartOptions(cloudConfig);
    }

    // Relative humidity fan chart (percentage, 0-100%)
    let humidityData: ReturnType<typeof buildFanChartData> | null = null;
    let humidityOptions: ReturnType<typeof buildFanChartOptions> | null = null;

    if (humidityStats) {
        const humidityConfig: FanChartConfig = {
            times,
            stats: humidityStats,
            unitConverter: identity,
            label: 'Relative Humidity (%)',
            syncKey: CHART_SYNC_KEY,
            zoomLevel: zoom,
        };
        humidityData = buildFanChartData(humidityConfig);
        humidityOptions = buildFanChartOptions(humidityConfig);
    }

    // Precipitation probability multi-line chart
    let precipProbData: uPlot.AlignedData | null = null;
    let precipProbOptions: uPlot.Options | null = null;

    if (precipProb) {
        precipProbData = [
            times,
            precipProb.any,
            precipProb.moderate,
            precipProb.heavy,
        ] as uPlot.AlignedData;
        precipProbOptions = buildPrecipProbOptions(CHART_SYNC_KEY);
    }

    if (!cloudStats && !humidityStats && !precipProb) return null;

    return (
        <div class="panel">
            <div class="panel__header">Atmospheric</div>
            <div class="panel__body">
                {cloudOptions && cloudData && (
                    <ChartWrapper options={cloudOptions} data={cloudData} />
                )}
                {humidityOptions && humidityData && (
                    <ChartWrapper options={humidityOptions} data={humidityData} />
                )}
                {precipProbOptions && precipProbData && (
                    <ChartWrapper options={precipProbOptions} data={precipProbData} />
                )}
            </div>
        </div>
    );
}
