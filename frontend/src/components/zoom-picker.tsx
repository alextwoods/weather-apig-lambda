import type { ZoomLevel } from '../state/url-state';

/** Available zoom levels with their display labels */
const ZOOM_OPTIONS: ReadonlyArray<{ value: ZoomLevel; label: string }> = [
    { value: '3d', label: '3d' },
    { value: '5d', label: '5d' },
    { value: '7d', label: '7d' },
    { value: '10d', label: '10d' },
];

export interface ZoomPickerProps {
    zoom: ZoomLevel;
    onZoomChange: (zoom: ZoomLevel) => void;
}

/**
 * Zoom Picker component.
 * Displays 4 toggle buttons for selecting the chart time axis zoom level
 * (2h, 6h, 12h, 24h per screen width). Defaults to 6h on initial load.
 * Selection persists across panel navigation via URL state.
 */
export function ZoomPicker({ zoom, onZoomChange }: ZoomPickerProps) {
    return (
        <div class="toggle-bar" role="group" aria-label="Chart zoom level">
            {ZOOM_OPTIONS.map(({ value, label }) => {
                const isActive = zoom === value;
                return (
                    <button
                        key={value}
                        type="button"
                        class={`toggle-bar__button${isActive ? ' toggle-bar__button--active' : ''}`}
                        aria-pressed={isActive}
                        onClick={() => {
                            if (!isActive) {
                                onZoomChange(value);
                            }
                        }}
                    >
                        {label}
                    </button>
                );
            })}
        </div>
    );
}
