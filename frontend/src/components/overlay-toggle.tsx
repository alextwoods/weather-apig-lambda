import type { OverlayType } from '../state/url-state';
import { OVERLAY_COLORS } from '../charts/colors';

/** Overlay definitions with display labels and colors */
const OVERLAYS: ReadonlyArray<{ id: OverlayType; label: string; color: string }> = [
    { id: 'hrrr', label: 'HRRR', color: OVERLAY_COLORS.hrrr },
    { id: 'obs', label: 'Observed', color: OVERLAY_COLORS.obs },
    { id: 'extended', label: 'Extended', color: OVERLAY_COLORS.extended },
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
    stationName?: string;
    stationDistanceKm?: number;
}

/**
 * Format distance for display per iOS spec:
 * < 1 km: shown in meters (e.g., "850 m")
 * 1-10 km: one decimal (e.g., "3.2 km")
 * > 10 km: integer (e.g., "15 km")
 */
function formatDistance(km: number): string {
    if (km < 1) return `${Math.round(km * 1000)} m`;
    if (km < 10) return `${km.toFixed(1)} km`;
    return `${Math.round(km)} km`;
}

/**
 * Overlay Toggle Bar component.
 * Displays capsule-shaped toggle buttons for HRRR, Observations, and Extended overlays.
 * Each button uses its specific color.
 * Enabled: filled capsule at color opacity 0.8, black text.
 * Disabled: transparent capsule with color stroke at 0.6 opacity, colored text, 0.4 opacity.
 * Shows station attribution below when observations are available.
 */
export function OverlayToggle({
    activeOverlays,
    onOverlaysChange,
    hrrrAvailable,
    observationsAvailable,
    stationName,
    stationDistanceKm,
}: OverlayToggleProps) {
    function isHidden(id: OverlayType): boolean {
        if (id === 'hrrr') return !hrrrAvailable;
        if (id === 'obs') return !observationsAvailable;
        return false;
    }

    function handleToggle(id: OverlayType): void {
        if (isHidden(id)) return;
        onOverlaysChange(toggleOverlay(activeOverlays, id));
    }

    return (
        <div class="overlay-toggle-container">
            <div class="toggle-bar" role="group" aria-label="Overlay toggles">
                {OVERLAYS.map(({ id, label, color }) => {
                    if (isHidden(id)) return null;
                    const isActive = activeOverlays.has(id);
                    return (
                        <button
                            key={id}
                            type="button"
                            class="toggle-bar__capsule"
                            aria-pressed={isActive}
                            onClick={() => handleToggle(id)}
                            style={isActive
                                ? { backgroundColor: color, color: '#000', borderColor: color, opacity: 1 }
                                : { backgroundColor: 'transparent', color: color, borderColor: color, opacity: 0.4 }
                            }
                        >
                            {label}
                        </button>
                    );
                })}
            </div>
            {/* Station attribution */}
            {observationsAvailable && stationName && (
                <div class="overlay-toggle__attribution">
                    {stationName}
                    {stationDistanceKm != null && ` — ${formatDistance(stationDistanceKm)}`}
                </div>
            )}
        </div>
    );
}
