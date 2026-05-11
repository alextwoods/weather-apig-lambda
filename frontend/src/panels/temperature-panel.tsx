import type { ForecastResponse } from '../api/types';
import type { UnitPreferences } from '../units/types';
import type { ZoomLevel, OverlayType } from '../state/url-state';
import { convertTemp } from '../units/converter';
import { buildMultiFanChartData, buildMultiFanChartOptions, type MultiFanChartConfig } from '../charts/fan-chart';
import { ChartWrapper } from '../charts/chart-wrapper';
import { CHART_SYNC_KEY } from '../charts/sync';
import { VARIABLE_COLORS } from '../charts/colors';
import { timesToUnixSeconds, parseUtcMs } from '../api/time-utils';

export interface TemperaturePanelProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
    overlays: Set<OverlayType>;
    zoom: ZoomLevel;
}

/**
 * Temperature Panel.
 * Displays a multi-series fan chart with temperature, apparent temperature (feels like),
 * and dew point overlaid on a single chart. Each variable has its own colored bands.
 * Includes optional HRRR overlay line and observation point markers.
 *
 * Colors: Temperature=orange, Feels Like=purple, Dew Point=cyan
 * HRRR: dashed orange line at 0.5 opacity
 * Observations: yellow dots
 */
export function TemperaturePanel({ forecast, units, overlays, zoom }: TemperaturePanelProps) {
    const { ensemble, hrrr, observations, astronomy } = forecast;
    const tempStats = ensemble.statistics.temperature_2m;

    if (!tempStats) return null;

    const times = timesToUnixSeconds(ensemble.times);
    const unitConverter = (v: number) => convertTemp(v, units.temperature);
    const unitLabel = units.temperature === 'C' ? '°C' : '°F';

    // Build multi-series config
    const seriesConfigs: MultiFanChartConfig['series'] = [
        { stats: tempStats, color: 'temperature', label: 'Temp' },
    ];

    // Add apparent temperature if available
    const feelsLikeStats = ensemble.statistics.apparent_temperature;
    if (feelsLikeStats) {
        seriesConfigs.push({ stats: feelsLikeStats, color: 'apparentTemp', label: 'Feels Like' });
    }

    // Add dew point if available
    const dewPointStats = ensemble.statistics.dew_point_2m;
    if (dewPointStats) {
        seriesConfigs.push({ stats: dewPointStats, color: 'dewPoint', label: 'Dew Pt' });
    }

    // HRRR overlay
    const hrrrLines: MultiFanChartConfig['hrrr'] = [];
    if (overlays.has('hrrr') && hrrr?.temperature_2m) {
        hrrrLines.push({
            data: hrrr.temperature_2m,
            color: 'rgba(251, 146, 60, 0.5)',
            label: 'HRRR Temp',
        });
    }

    // Observation markers
    const obsMarkers = overlays.has('obs') && observations?.entries
        ? observations.entries
            .filter(e => e.temperature_celsius != null)
            .map(e => ({
                time: parseUtcMs(e.timestamp) / 1000 | 0,
                value: e.temperature_celsius!,
            }))
        : undefined;

    const config: MultiFanChartConfig = {
        times,
        series: seriesConfigs,
        hrrr: hrrrLines.length > 0 ? hrrrLines : undefined,
        observations: obsMarkers,
        sunAltitude: astronomy?.sun_altitude,
        unitConverter,
        axisLabel: `Temp (${unitLabel})`,
        syncKey: CHART_SYNC_KEY,
        zoomLevel: zoom,
        height: 220,
    };

    const data = buildMultiFanChartData(config);
    const options = buildMultiFanChartOptions(config);

    // Current values for HUD row
    const now = new Date();
    const nowIdx = findNearestIndex(ensemble.times, now);
    const currentTemp = tempStats.median[nowIdx];
    const currentP10 = tempStats.p10[nowIdx];
    const currentP90 = tempStats.p90[nowIdx];

    // Observation value (within 2 hours)
    let obsTemp: number | null = null;
    if (overlays.has('obs') && observations?.entries) {
        const recentObs = observations.entries.find(e => {
            const diff = Math.abs(now.getTime() - parseUtcMs(e.timestamp));
            return diff < 2 * 3600 * 1000 && e.temperature_celsius != null;
        });
        if (recentObs) obsTemp = recentObs.temperature_celsius;
    }

    // HRRR value (within 1 hour)
    let hrrrTemp: number | null = null;
    if (overlays.has('hrrr') && hrrr?.temperature_2m && hrrr.times) {
        const hrrrIdx = findNearestIndex(hrrr.times, now);
        const diff = Math.abs(now.getTime() - parseUtcMs(hrrr.times[hrrrIdx]));
        if (diff < 3600 * 1000) {
            hrrrTemp = hrrr.temperature_2m[hrrrIdx] ?? null;
        }
    }

    return (
        <div class="panel">
            <div class="panel__header">
                <span class="panel__title">Temp ({unitLabel})</span>
                <div class="panel__legend">
                    <span class="panel__legend-item">
                        <span class="panel__legend-dot" style={{ backgroundColor: VARIABLE_COLORS.temperature.stroke }} />
                        Temp
                    </span>
                    {feelsLikeStats && (
                        <span class="panel__legend-item">
                            <span class="panel__legend-dot" style={{ backgroundColor: VARIABLE_COLORS.apparentTemp.stroke }} />
                            Feels Like
                        </span>
                    )}
                    {dewPointStats && (
                        <span class="panel__legend-item">
                            <span class="panel__legend-dot" style={{ backgroundColor: VARIABLE_COLORS.dewPoint.stroke }} />
                            Dew Pt
                        </span>
                    )}
                </div>
            </div>
            {/* HUD row with current values */}
            <div class="panel__hud-row">
                {obsTemp != null && (
                    <span class="panel__hud-chip panel__hud-chip--obs">
                        <span class="panel__hud-chip-label">Obs:</span>
                        {Math.round(unitConverter(obsTemp))}{unitLabel}
                    </span>
                )}
                {hrrrTemp != null && (
                    <span class="panel__hud-chip panel__hud-chip--hrrr">
                        <span class="panel__hud-chip-label">HRRR:</span>
                        {Math.round(unitConverter(hrrrTemp))}{unitLabel}
                    </span>
                )}
                {currentTemp != null && (
                    <span class="panel__hud-chip panel__hud-chip--ens-temp">
                        <span class="panel__hud-chip-label">Ens:</span>
                        {Math.round(unitConverter(currentTemp))}
                        {currentP10 != null && currentP90 != null && (
                            <span class="panel__hud-chip-range">
                                ({Math.round(unitConverter(currentP10))}–{Math.round(unitConverter(currentP90))})
                            </span>
                        )}
                        {unitLabel}
                    </span>
                )}
            </div>
            <div class="panel__body">
                <ChartWrapper options={options} data={data} height={220} />
            </div>
        </div>
    );
}

/** Find the index in `times` whose ISO timestamp is closest to `now`. */
function findNearestIndex(times: string[], now: Date): number {
    if (times.length === 0) return 0;
    const nowMs = now.getTime();
    let bestIndex = 0;
    let bestDiff = Math.abs(parseUtcMs(times[0]) - nowMs);
    for (let i = 1; i < times.length; i++) {
        const diff = Math.abs(parseUtcMs(times[i]) - nowMs);
        if (diff < bestDiff) {
            bestDiff = diff;
            bestIndex = i;
        }
    }
    return bestIndex;
}
