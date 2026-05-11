import type { ForecastResponse, DailySection } from '../api/types';
import type { UnitPreferences } from '../units/types';
import { convertTemp, convertWind, convertPressure, convertPrecip } from '../units/converter';

export interface DataTableProps {
    forecast: ForecastResponse;
    units: UnitPreferences;
}

/** Row definition for the data table. */
interface VariableRow {
    label: string;
    unit: string;
    values: (string | null)[];
}

/**
 * Formats a time string as a short hour label (e.g. "14:00").
 */
function formatTime(isoTime: string): string {
    const d = new Date(isoTime + (isoTime.includes('Z') || isoTime.includes('+') ? '' : 'Z'));
    return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', hour12: false });
}

/**
 * Formats a number to a fixed number of decimal places, or returns '—' for null.
 */
function fmt(value: number | null | undefined, decimals = 1): string | null {
    if (value === null || value === undefined) return null;
    return value.toFixed(decimals);
}

/**
 * Builds the variable rows from the forecast ensemble statistics.
 */
function buildRows(forecast: ForecastResponse, units: UnitPreferences): VariableRow[] {
    const { statistics } = forecast.ensemble;
    const rows: VariableRow[] = [];

    // Temperature
    const tempStats = statistics.temperature_2m;
    if (tempStats) {
        const unitLabel = units.temperature === 'C' ? '°C' : '°F';
        rows.push({
            label: 'Temperature',
            unit: unitLabel,
            values: tempStats.median.map(v => fmt(v !== null ? convertTemp(v, units.temperature) : null)),
        });
    }

    // Wind Speed
    const windStats = statistics.wind_speed_10m;
    if (windStats) {
        const unitLabel = units.wind;
        rows.push({
            label: 'Wind Speed',
            unit: unitLabel,
            values: windStats.median.map(v => fmt(v !== null ? convertWind(v, units.wind) : null)),
        });
    }

    // Wind Gusts
    const gustStats = statistics.wind_gusts_10m;
    if (gustStats) {
        const unitLabel = units.wind;
        rows.push({
            label: 'Wind Gusts',
            unit: unitLabel,
            values: gustStats.median.map(v => fmt(v !== null ? convertWind(v, units.wind) : null)),
        });
    }

    // Wind Direction
    const dirStats = statistics.wind_direction_10m;
    if (dirStats) {
        rows.push({
            label: 'Wind Direction',
            unit: '°',
            values: dirStats.median.map(v => fmt(v, 0)),
        });
    }

    // Cloud Cover
    const cloudStats = statistics.cloud_cover;
    if (cloudStats) {
        rows.push({
            label: 'Cloud Cover',
            unit: '%',
            values: cloudStats.median.map(v => fmt(v, 0)),
        });
    }

    // Humidity
    const humidityStats = statistics.relative_humidity_2m;
    if (humidityStats) {
        rows.push({
            label: 'Humidity',
            unit: '%',
            values: humidityStats.median.map(v => fmt(v, 0)),
        });
    }

    // Precipitation
    const precipStats = statistics.precipitation;
    if (precipStats) {
        const unitLabel = units.precipitation;
        rows.push({
            label: 'Precipitation',
            unit: unitLabel,
            values: precipStats.median.map(v => fmt(v !== null ? convertPrecip(v, units.precipitation) : null, 2)),
        });
    }

    // Pressure
    const pressureStats = statistics.pressure_msl;
    if (pressureStats) {
        const unitLabel = units.pressure;
        rows.push({
            label: 'Pressure',
            unit: unitLabel,
            values: pressureStats.median.map(v => fmt(v !== null ? convertPressure(v, units.pressure) : null)),
        });
    }

    // UV Index (from uv source, not ensemble statistics)
    if (forecast.uv?.uv_index) {
        rows.push({
            label: 'UV Index',
            unit: '',
            values: forecast.uv.uv_index.map(v => fmt(v, 0)),
        });
    }

    return rows;
}

/**
 * Data Table View.
 * Displays forecast data in a scrollable table with time steps as columns
 * and weather variables as rows. Rows are grouped by daily sections.
 *
 * Validates: Requirements 16.1, 16.2, 16.3, 16.4, 16.5
 */
export function DataTable({ forecast, units }: DataTableProps) {
    const { ensemble } = forecast;
    const { times, daily_sections } = ensemble;
    const rows = buildRows(forecast, units);

    if (rows.length === 0 || times.length === 0) {
        return (
            <div class="panel">
                <div class="panel__header">Data Table</div>
                <div class="panel__body">No data available</div>
            </div>
        );
    }

    return (
        <div class="panel">
            <div class="panel__header">
                <span class="panel__title">Data Table</span>
            </div>
            <div class="data-table">
                <table>
                    <thead>
                        {renderSectionedHeader(times, daily_sections)}
                    </thead>
                    <tbody>
                        {renderSectionedBody(rows, daily_sections)}
                    </tbody>
                </table>
            </div>
        </div>
    );
}

/**
 * Renders the table header with time columns grouped by daily sections.
 * Each daily section gets a section header row spanning its columns.
 */
function renderSectionedHeader(times: string[], sections: DailySection[]) {
    // Date header row
    const dateRow = (
        <tr class="data-table__section-header">
            <th>Variable</th>
            {sections.map(section => (
                <th
                    key={section.date}
                    colSpan={section.end_index - section.start_index + 1}
                >
                    {section.date}
                </th>
            ))}
        </tr>
    );

    // Time header row
    const timeRow = (
        <tr>
            <th>Unit</th>
            {times.map((t, i) => (
                <td key={i}>{formatTime(t)}</td>
            ))}
        </tr>
    );

    return (
        <>
            {dateRow}
            {timeRow}
        </>
    );
}

/**
 * Renders the table body with variable rows.
 */
function renderSectionedBody(rows: VariableRow[], _sections: DailySection[]) {
    return rows.map(row => (
        <tr key={row.label}>
            <th>{row.label} ({row.unit})</th>
            {row.values.map((v, i) => (
                <td key={i}>{v ?? '—'}</td>
            ))}
        </tr>
    ));
}
