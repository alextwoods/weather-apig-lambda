import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { convertWave } from '../units/converter';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';
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

/**
 * Builds uPlot options for a simple line chart (wave period, direction, tide).
 */
function buildLineChartOptions(label: string, syncKey: string): uPlot.Options {
    return {
        width: 800,
        height: 150,
        series: [
            {},
            {
                label,
                show: true,
                stroke: 'rgba(59, 130, 246, 0.9)',
                width: 1.5,
                points: { show: false },
            },
        ],
        axes: [
            {
                stroke: '#666',
                grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
            },
            {
                label,
                stroke: '#666',
                grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
                size: 60,
            },
        ],
        scales: {
            x: { time: true },
            y: { auto: true },
        },
        cursor: {
            sync: { key: syncKey },
        },
        legend: { show: false },
    };
}

/**
 * Builds uPlot options for the wave height chart with unit conversion.
 */
function buildWaveHeightOptions(label: string, syncKey: string): uPlot.Options {
    return {
        width: 800,
        height: 150,
        series: [
            {},
            {
                label,
                show: true,
                stroke: 'rgba(14, 165, 233, 0.9)',
                width: 2,
                fill: 'rgba(14, 165, 233, 0.1)',
                points: { show: false },
            },
        ],
        axes: [
            {
                stroke: '#666',
                grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
            },
            {
                label,
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
 * Builds uPlot options for the tide chart.
 */
function buildTideChartOptions(syncKey: string): uPlot.Options {
    return {
        width: 800,
        height: 150,
        series: [
            {},
            {
                label: 'Tide Height (m)',
                show: true,
                stroke: 'rgba(20, 184, 166, 0.9)',
                width: 2,
                fill: 'rgba(20, 184, 166, 0.15)',
                points: { show: false },
            },
        ],
        axes: [
            {
                stroke: '#666',
                grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
            },
            {
                label: 'Tide Height (m)',
                stroke: '#666',
                grid: { stroke: 'rgba(0, 0, 0, 0.06)' },
                size: 60,
            },
        ],
        scales: {
            x: { time: true },
            y: { auto: true },
        },
        cursor: {
            sync: { key: syncKey },
        },
        legend: { show: false },
    };
}

/**
 * Marine Panel.
 * Displays wave height, wave period, wave direction charts,
 * tide chart showing predicted water levels, and SST when present.
 * Hides entirely when marine data is absent.
 *
 * Validates: Requirements 15.1, 15.2, 15.3, 15.4, 15.5
 */
export function MarinePanel({ forecast, units, overlays: _overlays, zoom: _zoom }: MarinePanelProps) {
    const { marine, tides, water_temperature } = forecast;

    // Hide entire panel when marine data is absent
    if (!marine && !tides) return null;

    const unitConvert = (v: number) => convertWave(v, units.wave);

    // Wave height chart
    let waveHeightData: uPlot.AlignedData | null = null;
    let waveHeightOptions: uPlot.Options | null = null;

    if (marine) {
        const marineTimes = marine.times.map(t => Math.floor(new Date(t).getTime() / 1000));
        const convertedHeight = marine.wave_height.map(v => v != null ? unitConvert(v) : null);

        waveHeightData = [marineTimes, convertedHeight] as uPlot.AlignedData;
        waveHeightOptions = buildWaveHeightOptions(
            `Wave Height (${waveUnitLabel(units.wave)})`,
            CHART_SYNC_KEY,
        );
    }

    // Wave period chart
    let wavePeriodData: uPlot.AlignedData | null = null;
    let wavePeriodOptions: uPlot.Options | null = null;

    if (marine) {
        const marineTimes = marine.times.map(t => Math.floor(new Date(t).getTime() / 1000));
        wavePeriodData = [marineTimes, marine.wave_period] as uPlot.AlignedData;
        wavePeriodOptions = buildLineChartOptions('Wave Period (s)', CHART_SYNC_KEY);
    }

    // Wave direction chart
    let waveDirectionData: uPlot.AlignedData | null = null;
    let waveDirectionOptions: uPlot.Options | null = null;

    if (marine) {
        const marineTimes = marine.times.map(t => Math.floor(new Date(t).getTime() / 1000));
        waveDirectionData = [marineTimes, marine.wave_direction] as uPlot.AlignedData;
        waveDirectionOptions = buildLineChartOptions('Wave Direction (°)', CHART_SYNC_KEY);
    }

    // Tide chart
    let tideData: uPlot.AlignedData | null = null;
    let tideOptions: uPlot.Options | null = null;

    if (tides && tides.predictions.length > 0) {
        const tideTimes = tides.predictions.map(p => Math.floor(new Date(p.time).getTime() / 1000));
        const tideHeights = tides.predictions.map(p => p.height_m);
        tideData = [tideTimes, tideHeights] as uPlot.AlignedData;
        tideOptions = buildTideChartOptions(CHART_SYNC_KEY);
    }

    // SST display
    const sst = water_temperature?.temperature_celsius ?? null;

    return (
        <div class="panel">
            <div class="panel__header">
                Marine
                {sst != null && (
                    <span class="panel__header-badge">
                        {' '}SST: {sst.toFixed(1)}°C
                    </span>
                )}
            </div>
            <div class="panel__body">
                {waveHeightOptions && waveHeightData && (
                    <ChartWrapper options={waveHeightOptions} data={waveHeightData} />
                )}
                {wavePeriodOptions && wavePeriodData && (
                    <ChartWrapper options={wavePeriodOptions} data={wavePeriodData} />
                )}
                {waveDirectionOptions && waveDirectionData && (
                    <ChartWrapper options={waveDirectionOptions} data={waveDirectionData} />
                )}
                {tideOptions && tideData && (
                    <ChartWrapper options={tideOptions} data={tideData} />
                )}
            </div>
        </div>
    );
}
