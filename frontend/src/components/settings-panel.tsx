import type { UnitPreferences, TempUnit, WindUnit, PressureUnit, PrecipUnit, WaveUnit } from '../units/types';

/** Unit option definitions for each preference category */
const TEMP_OPTIONS: ReadonlyArray<{ value: TempUnit; label: string }> = [
    { value: 'C', label: '°C' },
    { value: 'F', label: '°F' },
];

const WIND_OPTIONS: ReadonlyArray<{ value: WindUnit; label: string }> = [
    { value: 'kmh', label: 'km/h' },
    { value: 'mph', label: 'mph' },
    { value: 'kts', label: 'knots' },
    { value: 'ms', label: 'm/s' },
];

const PRESSURE_OPTIONS: ReadonlyArray<{ value: PressureUnit; label: string }> = [
    { value: 'hPa', label: 'hPa' },
    { value: 'inHg', label: 'inHg' },
    { value: 'mmHg', label: 'mmHg' },
];

const PRECIP_OPTIONS: ReadonlyArray<{ value: PrecipUnit; label: string }> = [
    { value: 'mm', label: 'mm' },
    { value: 'in', label: 'in' },
];

const WAVE_OPTIONS: ReadonlyArray<{ value: WaveUnit; label: string }> = [
    { value: 'm', label: 'm' },
    { value: 'ft', label: 'ft' },
];

export interface SettingsPanelProps {
    isOpen: boolean;
    onClose: () => void;
    units: UnitPreferences;
    onUnitsChange: (units: UnitPreferences) => void;
}

/**
 * Settings Panel component.
 * Displays a slide-out panel with unit preference controls.
 * Each unit type has a group of radio buttons.
 * On change: the parent is notified via onUnitsChange to persist to local storage,
 * update the URL, and re-render all values without re-fetching.
 */
export function SettingsPanel({ isOpen, onClose, units, onUnitsChange }: SettingsPanelProps) {
    if (!isOpen) {
        return null;
    }

    function handleKeyDown(e: KeyboardEvent): void {
        if (e.key === 'Escape') {
            onClose();
        }
    }

    function handleOverlayClick(e: MouseEvent): void {
        if ((e.target as HTMLElement).classList.contains('settings-overlay')) {
            onClose();
        }
    }

    function updateUnit<K extends keyof UnitPreferences>(key: K, value: UnitPreferences[K]): void {
        onUnitsChange({ ...units, [key]: value });
    }

    return (
        <div
            class="settings-overlay"
            role="dialog"
            aria-modal="true"
            aria-labelledby="settings-panel-title"
            onKeyDown={handleKeyDown}
            onClick={handleOverlayClick}
        >
            <div class="settings-panel">
                <div class="settings-panel__header">
                    <h2 id="settings-panel-title" class="settings-panel__title">
                        Settings
                    </h2>
                    <button
                        type="button"
                        class="settings-panel__close"
                        onClick={onClose}
                        aria-label="Close settings"
                    >
                        ✕
                    </button>
                </div>
                <div class="settings-panel__body">
                    <UnitGroup
                        label="Temperature"
                        name="temperature"
                        options={TEMP_OPTIONS}
                        value={units.temperature}
                        onChange={(v) => updateUnit('temperature', v as TempUnit)}
                    />
                    <UnitGroup
                        label="Wind Speed"
                        name="wind"
                        options={WIND_OPTIONS}
                        value={units.wind}
                        onChange={(v) => updateUnit('wind', v as WindUnit)}
                    />
                    <UnitGroup
                        label="Pressure"
                        name="pressure"
                        options={PRESSURE_OPTIONS}
                        value={units.pressure}
                        onChange={(v) => updateUnit('pressure', v as PressureUnit)}
                    />
                    <UnitGroup
                        label="Precipitation"
                        name="precipitation"
                        options={PRECIP_OPTIONS}
                        value={units.precipitation}
                        onChange={(v) => updateUnit('precipitation', v as PrecipUnit)}
                    />
                    <UnitGroup
                        label="Wave Height"
                        name="wave"
                        options={WAVE_OPTIONS}
                        value={units.wave}
                        onChange={(v) => updateUnit('wave', v as WaveUnit)}
                    />
                </div>
            </div>
        </div>
    );
}

/** Props for the UnitGroup sub-component */
interface UnitGroupProps {
    label: string;
    name: string;
    options: ReadonlyArray<{ value: string; label: string }>;
    value: string;
    onChange: (value: string) => void;
}

/**
 * A group of radio buttons for selecting a unit preference.
 */
function UnitGroup({ label, name, options, value, onChange }: UnitGroupProps) {
    return (
        <fieldset class="settings-panel__group" role="radiogroup" aria-label={label}>
            <legend class="settings-panel__group-label">{label}</legend>
            <div class="settings-panel__options">
                {options.map((option) => (
                    <label
                        key={option.value}
                        class={`settings-panel__option${option.value === value ? ' settings-panel__option--selected' : ''}`}
                    >
                        <input
                            type="radio"
                            name={name}
                            value={option.value}
                            checked={option.value === value}
                            onChange={() => onChange(option.value)}
                            class="settings-panel__radio"
                        />
                        <span class="settings-panel__option-label">{option.label}</span>
                    </label>
                ))}
            </div>
        </fieldset>
    );
}
