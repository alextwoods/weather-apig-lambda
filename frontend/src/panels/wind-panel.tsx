import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { convertWind } from '../units/converter';
import { buildFanChartData, buildFanChartOptions } from '../charts/fan-chart';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';

export interface WindPanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/** Wind unit label for axis display. */
function windUnitLabel(unit: UnitPreferences['wind']): string {
    switch (unit) {
        case 'kmh': return 'km/h';
        case 'mph': return 'mph';
        case 'kts': return 'kts';
        case 'ms': return 'm/s';
    }
}

/**
 * Wind Panel.
 * Displays fan charts for wind_speed_10m and wind_gusts_10m,
 * wind direction indicators, and optional HRRR overlay lines.
 *
 * Validates: Requirements 9.1, 9.2, 9.3, 9.4, 9.5
 */
export function WindPanel({ forecast, units, overlays, zoom }: WindPanelProps) {
    const { ensemble, hrrr } = forecast;
    const speedStats = ensemble.statistics.wind_speed_10m;
    const gustStats = ensemble.statistics.wind_gusts_10m;
    const directionStats = ensemble.statistics.wind_direction_10m;

    if (!speedStats) return null;

    const times = ensemble.times.map(t => Math.floor(new Date(t).getTime() / 1000));
    const unitConverter = (v: number) => convertWind(v, units.wind);
    const label = `Wind Speed (${windUnitLabel(units.wind)})`;

    // Wind speed fan chart
    const speedHrrr = overlays.has('hrrr') && hrrr?.wind_speed_10m
        ? hrrr.wind_speed_10m
        : undefined;

    const speedConfig = {
        times,
        stats: speedStats,
        hrrr: speedHrrr,
        unitConverter,
        label,
        syncKey: CHART_SYNC_KEY,
        zoomLevel: zoom,
    };

    const speedData = buildFanChartData(speedConfig);
    const speedOptions = buildFanChartOptions(speedConfig);

    // Wind gusts fan chart
    let gustData: ReturnType<typeof buildFanChartData> | null = null;
    let gustOptions: ReturnType<typeof buildFanChartOptions> | null = null;

    if (gustStats) {
        const gustHrrr = overlays.has('hrrr') && hrrr?.wind_gusts_10m
            ? hrrr.wind_gusts_10m
            : undefined;

        const gustConfig = {
            times,
            stats: gustStats,
            hrrr: gustHrrr,
            unitConverter,
            label: `Wind Gusts (${windUnitLabel(units.wind)})`,
            syncKey: CHART_SYNC_KEY,
            zoomLevel: zoom,
        };

        gustData = buildFanChartData(gustConfig);
        gustOptions = buildFanChartOptions(gustConfig);
    }

    // Wind direction median values for indicators
    const directionMedian = directionStats?.median ?? null;

    return (
        <div class="panel">
            <div class="panel__header">Wind</div>
            <div class="panel__body">
                <ChartWrapper options={speedOptions} data={speedData} />
                {gustOptions && gustData && (
                    <ChartWrapper options={gustOptions} data={gustData} />
                )}
                {directionMedian && (
                    <div class="panel__wind-direction">
                        <span class="panel__wind-direction-label">Direction (median):</span>
                        {directionMedian.slice(0, 12).map((deg, i) => (
                            <span
                                key={i}
                                class="panel__wind-arrow"
                                style={{ transform: `rotate(${deg ?? 0}deg)` }}
                                title={`${deg ?? '—'}°`}
                            >
                                ↓
                            </span>
                        ))}
                    </div>
                )}
            </div>
        </div>
    );
}
