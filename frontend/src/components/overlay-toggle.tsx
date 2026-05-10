import type { OverlayType } from '../state/url-state';

/** Overlay definitions with display labels */
const OVERLAYS: ReadonlyArray<{ id: OverlayType; label: string }> = [
    { id: 'hrrr', label: 'HRRR' },
    { id: 'obs', label: 'Observations' },
    { id: 'extended', label: 'Extended' },
];

/**
 * Pure function that computes the result of toggling an overlay in the active set.
 * Returns a new set with the overlay added or removed.
 */
export function toggleOverlay(current: Set<OverlayType>, overlay: OverlayType): Set<OverlayType> {
    const next = new Set(current);
    if (next.has(overlay)) {
        next.delete(overlay);
    } else {
        next.add(overlay);
    }
    return next;
}

export interface OverlayToggleProps {
    activeOverlays: Set<OverlayType>;
    onOverlaysChange: (overlays: Set<OverlayType>) => void;
    hrrrAvailable: boolean;
    observationsAvailable: boolean;
}

/**
 * Overlay Toggle Bar component.
 * Displays toggle buttons for HRRR, Observations, and Extended overlays.
 * Disables toggles when corresponding data is not present in the response.
 */
export function OverlayToggle({ activeOverlays, onOverlaysChange, hrrrAvailable, observationsAvailable }: OverlayToggleProps) {
    function isDisabled(id: OverlayType): boolean {
        if (id === 'hrrr') return !hrrrAvailable;
        if (id === 'obs') return !observationsAvailable;
        return false;
    }

    function handleToggle(id: OverlayType): void {
        if (isDisabled(id)) return;
        onOverlaysChange(toggleOverlay(activeOverlays, id));
    }

    return (
        <div class="toggle-bar" role="group" aria-label="Overlay toggles">
            {OVERLAYS.map(({ id, label }) => {
                const isActive = activeOverlays.has(id);
                const disabled = isDisabled(id);
                return (
                    <button
                        key={id}
                        type="button"
                        class={`toggle-bar__button${isActive ? ' toggle-bar__button--active' : ''}`}
                        aria-pressed={isActive}
                        disabled={disabled}
                        onClick={() => handleToggle(id)}
                    >
                        {label}
                    </button>
                );
            })}
        </div>
    );
}
