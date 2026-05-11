import { timesToUnixSeconds } from '../api/time-utils';
import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { convertPrecip } from '../units/converter';
import { buildFanChartData, buildFanChartOptions } from '../charts/fan-chart';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';
import { VARIABLE_COLORS } from '../charts/colors';

export interface PrecipitationPanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/**
 * Precipitation Panel.
 * Displays a fan chart for precipitation ensemble statistics in teal,
 * with optional HRRR overlay line (dashed teal at 0.5 opacity).
 *
 * Color: Teal (matching iOS spec Section 15)
 */
export function PrecipitationPanel({ forecast, units, overlays, zoom }: PrecipitationPanelProps) {
    const { ensemble, hrrr, astronomy } = forecast;
    const stats = ensemble.statistics.precipitation;

    if (!stats) return null;

    const times = timesToUnixSeconds(ensemble.times);
    const unitConverter = (v: number) => convertPrecip(v, units.precipitation);

    const hrrrData = overlays.has('hrrr') && hrrr?.precipitation
        ? hrrr.precipitation
        : undefined;

    const unitLabel = units.precipitation === 'mm' ? 'mm' : 'in';

    const config = {
        times,
        stats,
        color: 'precipitation' as const,
        hrrr: hrrrData,
        hrrrColor: 'rgba(45, 212, 191, 0.5)',
        sunAltitude: astronomy?.sun_altitude,
        unitConverter,
        label: `Precipitation (${unitLabel})`,
        syncKey: CHART_SYNC_KEY,
        zoomLevel: zoom,
        height: 200,
    };

    const data = buildFanChartData(config);
    const options = buildFanChartOptions(config);

    return (
        <div class="panel">
            <div class="panel__header">
                <span class="panel__title">Precipitation ({unitLabel})</span>
                <div class="panel__legend">
                    <span class="panel__legend-item">
                        <span class="panel__legend-dot" style={{ backgroundColor: VARIABLE_COLORS.precipitation.stroke }} />
                        {unitLabel}
                    </span>
                </div>
            </div>
            <div class="panel__body">
                <ChartWrapper options={options} data={data} height={200} />
            </div>
        </div>
    );
}
