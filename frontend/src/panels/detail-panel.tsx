import { useState, useEffect } from 'preact/hooks';
import type { MembersResponse } from '../api/types';
import type { WeatherApiClient } from '../api/client';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel } from '../state/url-state';
import { timesToUnixSeconds } from '../api/time-utils';
import { convertTemp, convertWind, convertPressure, convertPrecip } from '../units/converter';
import { ChartWrapper } from '../charts/chart-wrapper';
import { MODEL_COLORS, BAND_OUTER_FILL, BAND_INNER_FILL } from '../charts/colors';
import { CHART_SYNC_KEY } from '../charts/sync';
import uPlot from 'uplot';

export interface DetailPanelProps {
    variable: string;
    lat: number;
    lon: number;
    models: string[];
    apiClient: WeatherApiClient;
    units: UnitPreferences;
    zoom: ZoomLevel;
}

/**
 * Returns the appropriate unit converter function for a given variable name.
 */
function getUnitConverter(variable: string, units: UnitPreferences): (v: number) => number {
    if (variable.includes('temperature') || variable.includes('apparent_temperature')) {
        return (v: number) => convertTemp(v, units.temperature);
    }
    if (variable.includes('wind_speed') || variable.includes('wind_gusts')) {
        return (v: number) => convertWind(v, units.wind);
    }
    if (variable.includes('pressure')) {
        return (v: number) => convertPressure(v, units.pressure);
    }
    if (variable === 'precipitation') {
        return (v: number) => convertPrecip(v, units.precipitation);
    }
    // Default: no conversion (percentages, UV index, etc.)
    return (v: number) => v;
}

/**
 * Builds uPlot data array for the detail panel.
 * Layout:
 *   data[0] = timestamps
 *   data[1..5] = percentile band boundaries (p90, p75, median, p25, p10)
 *   data[6..N] = individual member lines
 */
function buildDetailData(
    response: MembersResponse,
    unitConverter: (v: number) => number,
): uPlot.AlignedData {
    const times = timesToUnixSeconds(response.times);
    const { statistics, members_by_model } = response;

    const convertArray = (arr: (number | null)[]): (number | null | undefined)[] =>
        arr.map(v => (v === null ? null : unitConverter(v)));

    const data: (number | null | undefined)[][] = [
        times,
        convertArray(statistics.p90),
        convertArray(statistics.p75),
        convertArray(statistics.median),
        convertArray(statistics.p25),
        convertArray(statistics.p10),
    ];

    // Add individual member lines
    for (const modelName of Object.keys(members_by_model)) {
        const members = members_by_model[modelName];
        for (const memberData of members) {
            data.push(memberData.map(v => (v === null || v === undefined ? null : unitConverter(v))));
        }
    }

    return data as uPlot.AlignedData;
}

/**
 * Builds uPlot options for the detail panel.
 * Configures percentile bands as reference and individual member lines
 * grouped by model with distinct colors.
 */
function buildDetailOptions(
    response: MembersResponse,
    syncKey: string,
    label: string,
): uPlot.Options {
    const { members_by_model } = response;

    // --- Series configuration ---
    const series: uPlot.Series[] = [
        // Series 0: time axis
        {},
        // Series 1: p90 (hidden band boundary)
        { label: 'p90', show: true, stroke: 'transparent', width: 0, points: { show: false } },
        // Series 2: p75 (hidden band boundary)
        { label: 'p75', show: true, stroke: 'transparent', width: 0, points: { show: false } },
        // Series 3: median (reference line)
        { label: 'Median', show: true, stroke: 'rgba(59, 130, 246, 0.5)', width: 1.5, dash: [4, 2], points: { show: false } },
        // Series 4: p25 (hidden band boundary)
        { label: 'p25', show: true, stroke: 'transparent', width: 0, points: { show: false } },
        // Series 5: p10 (hidden band boundary)
        { label: 'p10', show: true, stroke: 'transparent', width: 0, points: { show: false } },
    ];

    // Add member line series grouped by model
    const modelNames = Object.keys(members_by_model);
    for (const modelName of modelNames) {
        const color = MODEL_COLORS[modelName as keyof typeof MODEL_COLORS] ?? 'rgba(128, 128, 128, 0.5)';
        const members = members_by_model[modelName];
        for (let i = 0; i < members.length; i++) {
            series.push({
                label: i === 0 ? modelName : undefined,
                show: true,
                stroke: color,
                width: 0.5,
                points: { show: false },
            });
        }
    }

    // --- Bands configuration (percentile reference bands) ---
    const bands: uPlot.Band[] = [
        { series: [1, 2], fill: BAND_OUTER_FILL },
        { series: [2, 4], fill: BAND_INNER_FILL },
        { series: [4, 5], fill: BAND_OUTER_FILL },
    ];

    // --- Axes ---
    const axes: uPlot.Axis[] = [
        { stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' } },
        { label, stroke: '#666', grid: { stroke: 'rgba(0, 0, 0, 0.06)' }, size: 60 },
    ];

    return {
        width: 800,
        height: 300,
        series,
        bands,
        axes,
        cursor: { sync: { key: syncKey } },
        scales: { x: { time: true }, y: { auto: true } },
        legend: { show: false },
    };
}

/**
 * Detail Panel (Model Drill-Down).
 * Fetches individual ensemble member data for a selected variable and renders
 * all member lines grouped by model with distinct colors, plus percentile
 * statistics as reference bands.
 *
 * Lazy-loads data only when the component is mounted (detail view activated).
 *
 * Validates: Requirements 17.1, 17.2, 17.3, 17.4, 17.5
 */
export function DetailPanel({ variable, lat, lon, models, apiClient, units, zoom }: DetailPanelProps) {
    const [data, setData] = useState<MembersResponse | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    // Fetch member data when mounted or when variable/models change
    useEffect(() => {
        let cancelled = false;

        async function fetchMembers() {
            setLoading(true);
            setError(null);

            try {
                const response = await apiClient.forecastMembers({
                    variable,
                    lat,
                    lon,
                    models: models.length > 0 ? models : undefined,
                });

                if (!cancelled) {
                    setData(response);
                }
            } catch (err) {
                if (!cancelled) {
                    setError(err instanceof Error ? err.message : 'Failed to load member data');
                }
            } finally {
                if (!cancelled) {
                    setLoading(false);
                }
            }
        }

        fetchMembers();

        return () => {
            cancelled = true;
        };
    }, [variable, lat, lon, models.join(','), apiClient]);

    // Loading state
    if (loading) {
        return (
            <div class="panel panel--loading">
                <div class="panel__header">
                    <span class="panel__title">Detail: {variable}</span>
                </div>
                <div class="panel__body">
                    <div class="loading-indicator">
                        <div class="loading-indicator__spinner" />
                        <span class="loading-indicator__message">Loading member data…</span>
                    </div>
                </div>
            </div>
        );
    }

    // Error state
    if (error) {
        return (
            <div class="panel panel--error">
                <div class="panel__header">
                    <span class="panel__title">Detail: {variable}</span>
                </div>
                <div class="panel__body">{error}</div>
            </div>
        );
    }

    // No data
    if (!data) {
        return null;
    }

    const unitConverter = getUnitConverter(variable, units);
    const chartData = buildDetailData(data, unitConverter);
    const chartOptions = buildDetailOptions(data, CHART_SYNC_KEY, variable);

    return (
        <div class="panel">
            <div class="panel__header">
                <span class="panel__title">Detail: {variable}</span>
            </div>
            <div class="panel__body">
                <ChartWrapper options={chartOptions} data={chartData} height={300} />
            </div>
        </div>
    );
}
