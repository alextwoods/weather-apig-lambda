import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { convertPrecip } from '../units/converter';
import { buildFanChartData, buildFanChartOptions } from '../charts/fan-chart';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';

export interface PrecipitationPanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/**
 * Precipitation Panel.
 * Displays a fan chart for precipitation ensemble statistics,
 * with optional HRRR overlay line.
 *
 * Validates: Requirements 11.1, 11.2, 11.3
 */
export function PrecipitationPanel({ forecast, units, overlays, zoom }: PrecipitationPanelProps) {
    const { ensemble, hrrr } = forecast;
    const stats = ensemble.statistics.precipitation;

    if (!stats) return null;

    const times = ensemble.times.map(t => Math.floor(new Date(t).getTime() / 1000));
    const unitConverter = (v: number) => convertPrecip(v, units.precipitation);

    const hrrrData = overlays.has('hrrr') && hrrr?.precipitation
        ? hrrr.precipitation
        : undefined;

    const unitLabel = units.precipitation === 'mm' ? 'Precipitation (mm)' : 'Precipitation (in)';

    const config = {
        times,
        stats,
        hrrr: hrrrData,
        unitConverter,
        label: unitLabel,
        syncKey: CHART_SYNC_KEY,
        zoomLevel: zoom,
    };

    const data = buildFanChartData(config);
    const options = buildFanChartOptions(config);

    return (
        <div class="panel">
            <div class="panel__header">Precipitation</div>
            <div class="panel__body">
                <ChartWrapper options={options} data={data} />
            </div>
        </div>
    );
}
