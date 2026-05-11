import { useRef, useCallback, useState } from 'preact/hooks';
import { MODEL_COLORS } from '../charts/colors';

/** All available ensemble models with their display names, colors, member counts, and descriptions */
const MODELS: ReadonlyArray<{ id: string; label: string; color: string; members: number; description: string }> = [
    { id: 'ecmwf', label: 'ECMWF IFS', color: MODEL_COLORS.ecmwf, members: 51, description: 'European Centre for Medium-Range Weather Forecasts — Integrated Forecasting System ensemble' },
    { id: 'gfs', label: 'GFS Ensemble', color: MODEL_COLORS.gfs, members: 31, description: 'NOAA/NCEP Global Forecast System ensemble (GEFS)' },
    { id: 'icon', label: 'ICON Ensemble', color: MODEL_COLORS.icon, members: 40, description: 'Deutscher Wetterdienst ICOsahedral Non-hydrostatic model ensemble' },
    { id: 'gem', label: 'GEM Global', color: MODEL_COLORS.gem, members: 21, description: 'Environment and Climate Change Canada Global Environmental Multiscale model ensemble' },
    { id: 'bom', label: 'BOM ACCESS', color: MODEL_COLORS.bom, members: 18, description: 'Australian Bureau of Meteorology ACCESS Global ensemble' },
];

/**
 * Pure function that computes the result of toggling a model in the enabled set.
 * Returns the new set with the model toggled, or null if the toggle would
 * result in an empty set (i.e., disabling the last enabled model).
 */
export function toggleModel(current: Set<string>, model: string): Set<string> | null {
    const next = new Set(current);
    if (next.has(model)) {
        next.delete(model);
    } else {
        next.add(model);
    }
    if (next.size === 0) {
        return null;
    }
    return next;
}

export interface ModelToggleProps {
    enabledModels: Set<string>;
    onModelsChange: (models: Set<string>) => void;
}

/**
 * Model Toggle Bar component.
 * Displays capsule-shaped toggle buttons for each of the 5 ensemble models.
 * Each button uses the model's specific color.
 * Enabled: filled capsule with model color, white text.
 * Disabled: transparent capsule with model color border, model color text, 0.4 opacity.
 * Debounces changes by 300ms before calling onModelsChange.
 * Prevents disabling all models (rejects toggle if it would leave zero enabled).
 */
export function ModelToggle({ enabledModels, onModelsChange }: ModelToggleProps) {
    const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const [pendingModels, setPendingModels] = useState<Set<string> | null>(null);
    const [showInfo, setShowInfo] = useState(false);

    const displayModels = pendingModels ?? enabledModels;

    const handleToggle = useCallback(
        (modelId: string) => {
            const current = pendingModels ?? enabledModels;
            const result = toggleModel(current, modelId);

            if (result === null) return;

            setPendingModels(result);

            if (debounceRef.current !== null) {
                clearTimeout(debounceRef.current);
            }

            debounceRef.current = setTimeout(() => {
                debounceRef.current = null;
                setPendingModels(null);
                onModelsChange(result);
            }, 300);
        },
        [enabledModels, pendingModels, onModelsChange],
    );

    return (
        <>
            <div class="toggle-bar" role="group" aria-label="Ensemble model toggles">
                {MODELS.map(({ id, label, color }) => {
                    const isActive = displayModels.has(id);
                    return (
                        <button
                            key={id}
                            type="button"
                            class="toggle-bar__capsule"
                            aria-pressed={isActive}
                            onClick={() => handleToggle(id)}
                            style={isActive
                                ? { backgroundColor: color, color: '#fff', borderColor: color, opacity: 1 }
                                : { backgroundColor: 'transparent', color: color, borderColor: color, opacity: 0.4 }
                            }
                        >
                            {label}
                        </button>
                    );
                })}
                <button
                    type="button"
                    class="toggle-bar__info-btn"
                    onClick={() => setShowInfo(true)}
                    aria-label="Model information"
                >
                    ⓘ
                </button>
            </div>

            {/* Model Info Sheet */}
            {showInfo && (
                <div class="modal-overlay" onClick={() => setShowInfo(false)}>
                    <div class="modal-card" onClick={(e) => e.stopPropagation()}>
                        <div class="modal-card__title">Ensemble Models</div>
                        <div class="model-info-list">
                            {MODELS.map(({ id, label, color, members, description }) => (
                                <div key={id} class="model-info-item">
                                    <span class="model-info-item__dot" style={{ backgroundColor: color }} />
                                    <div class="model-info-item__text">
                                        <span class="model-info-item__name">{label}</span>
                                        <span class="model-info-item__members">{members} members</span>
                                        <span class="model-info-item__desc">{description}</span>
                                    </div>
                                </div>
                            ))}
                        </div>
                        <div class="modal-card__actions">
                            <button
                                type="button"
                                class="modal-card__btn modal-card__btn--primary"
                                onClick={() => setShowInfo(false)}
                            >
                                Done
                            </button>
                        </div>
                    </div>
                </div>
            )}
        </>
    );
}
