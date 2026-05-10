import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { convertPressure } from '../units/converter';
import { buildFanChartData, buildFanChartOptions } from '../charts/fan-chart';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';

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
 * Displays a fan chart for pressure_msl ensemble statistics,
 * with optional HRRR surface pressure overlay line.
 *
 * Validates: Requirements 14.1, 14.2, 14.3
 */
export function PressurePanel({ forecast, units, overlays, zoom }: PressurePanelProps) {
    const { ensemble, hrrr } = forecast;
    const stats = ensemble.statistics.pressure_msl;

    if (!stats) return null;

    const times = ensemble.times.map(t => Math.floor(new Date(t).getTime() / 1000));
    const unitConverter = (v: number) => convertPressure(v, units.pressure);

    const hrrrData = overlays.has('hrrr') && hrrr?.surface_pressure
        ? hrrr.surface_pressure
        : undefined;

    const label = `Pressure (${pressureUnitLabel(units.pressure)})`;

    const config = {
        times,
        stats,
        hrrr: hrrrData,
        unitConverter,
        label,
        syncKey: CHART_SYNC_KEY,
        zoomLevel: zoom,
    };

    const data = buildFanChartData(config);
    const options = buildFanChartOptions(config);

    return (
        <div class="panel">
            <div class="panel__header">Pressure</div>
            <div class="panel__body">
                <ChartWrapper options={options} data={data} />
            </div>
        </div>
    );
}
