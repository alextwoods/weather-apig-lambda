import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { convertTemp } from '../units/converter';
import { buildFanChartData, buildFanChartOptions } from '../charts/fan-chart';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';

export interface TemperaturePanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/**
 * Temperature Panel.
 * Displays a fan chart for temperature_2m ensemble statistics,
 * with optional HRRR overlay line and observation point markers.
 *
 * Validates: Requirements 8.1, 8.2, 8.3, 8.4
 */
export function TemperaturePanel({ forecast, units, overlays, zoom }: TemperaturePanelProps) {
    const { ensemble, hrrr, observations } = forecast;
    const stats = ensemble.statistics.temperature_2m;

    if (!stats) return null;

    const times = ensemble.times.map(t => Math.floor(new Date(t).getTime() / 1000));
    const unitConverter = (v: number) => convertTemp(v, units.temperature);

    // Build HRRR overlay data if enabled and available
    const hrrrData = overlays.has('hrrr') && hrrr?.temperature_2m
        ? hrrr.temperature_2m
        : undefined;

    // Build observation markers if enabled and available
    const obsMarkers = overlays.has('obs') && observations?.entries
        ? observations.entries
            .filter(e => e.temperature_celsius != null)
            .map(e => ({
                time: Math.floor(new Date(e.timestamp).getTime() / 1000),
                value: e.temperature_celsius!,
            }))
        : undefined;

    const unitLabel = units.temperature === 'C' ? 'Temperature (°C)' : 'Temperature (°F)';

    const config = {
        times,
        stats,
        hrrr: hrrrData,
        observations: obsMarkers,
        unitConverter,
        label: unitLabel,
        syncKey: CHART_SYNC_KEY,
        zoomLevel: zoom,
    };

    const data = buildFanChartData(config);
    const options = buildFanChartOptions(config);

    return (
        <div class="panel">
            <div class="panel__header">Temperature</div>
            <div class="panel__body">
                <ChartWrapper options={options} data={data} />
            </div>
        </div>
    );
}
