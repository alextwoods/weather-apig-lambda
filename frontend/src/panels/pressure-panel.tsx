import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { convertPressure } from '../units/converter';
import { buildFanChartData, buildFanChartOptions } from '../charts/fan-chart';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';
import { VARIABLE_COLORS } from '../charts/colors';
import { timesToUnixSeconds, parseUtcMs } from '../api/time-utils';

export interface PressurePanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/** Pressure unit label for axis display. */
function pressureUnitLabel(unit: UnitPreferences['pressure']): string {
    switch (unit) {
        case 'hPa': return 'hPa';
        case 'inHg': return 'inHg';
        case 'mmHg': return 'mmHg';
    }
}

/**
 * Pressure Panel.
 * Displays a fan chart for pressure_msl ensemble statistics in blue,
 * with optional HRRR surface pressure overlay line (dashed blue)
 * and observation point markers (yellow dots).
 *
 * Color: Blue (matching iOS spec Section 18)
 */
export function PressurePanel({ forecast, units, overlays, zoom }: PressurePanelProps) {
    const { ensemble, hrrr, observations, astronomy } = forecast;
    const stats = ensemble.statistics.pressure_msl;

    if (!stats) return null;

    const times = timesToUnixSeconds(ensemble.times);
    const unitConverter = (v: number) => convertPressure(v, units.pressure);

    const hrrrData = overlays.has('hrrr') && hrrr?.surface_pressure
        ? hrrr.surface_pressure
        : undefined;

    // Observation markers for pressure
    const obsMarkers = overlays.has('obs') && observations?.entries
        ? observations.entries
            .filter(e => e.pressure_hpa != null)
            .map(e => ({
                time: parseUtcMs(e.timestamp) / 1000 | 0,
                value: e.pressure_hpa!,
            }))
        : undefined;

    const label = `Pressure (${pressureUnitLabel(units.pressure)})`;

    const config = {
        times,
        stats,
        color: 'pressure' as const,
        hrrr: hrrrData,
        hrrrColor: 'rgba(96, 165, 250, 0.5)',
        observations: obsMarkers,
        sunAltitude: astronomy?.sun_altitude,
        unitConverter,
        label,
        syncKey: CHART_SYNC_KEY,
        zoomLevel: zoom,
        height: 160,
    };

    const data = buildFanChartData(config);
    const options = buildFanChartOptions(config);

    return (
        <div class="panel">
            <div class="panel__header">
                <span class="panel__title">Pressure ({pressureUnitLabel(units.pressure)})</span>
                <div class="panel__legend">
                    <span class="panel__legend-item">
                        <span class="panel__legend-dot" style={{ backgroundColor: VARIABLE_COLORS.pressure.stroke }} />
                        {pressureUnitLabel(units.pressure)}
                    </span>
                </div>
            </div>
            <div class="panel__body">
                <ChartWrapper options={options} data={data} height={160} />
            </div>
        </div>
    );
}
