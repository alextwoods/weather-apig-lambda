import { useRef, useCallback, useState } from 'preact/hooks';

/** All available ensemble models with their display names */
const MODELS: ReadonlyArray<{ id: string; label: string }> = [
    { id: 'ecmwf', label: 'ECMWF' },
    { id: 'gfs', label: 'GFS' },
    { id: 'icon', label: 'ICON' },
    { id: 'gem', label: 'GEM' },
    { id: 'bom', label: 'BOM' },
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
 * Displays toggle buttons for each of the 5 ensemble models.
 * Debounces changes by 300ms before calling onModelsChange.
 * Prevents disabling all models (rejects toggle if it would leave zero enabled).
 */
export function ModelToggle({ enabledModels, onModelsChange }: ModelToggleProps) {
    const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    // Track the pending (optimistic) state for immediate visual feedback
    const [pendingModels, setPendingModels] = useState<Set<string> | null>(null);

    const displayModels = pendingModels ?? enabledModels;

    const handleToggle = useCallback(
        (modelId: string) => {
            const current = pendingModels ?? enabledModels;
            const result = toggleModel(current, modelId);

            // Reject toggle if it would leave zero models enabled
            if (result === null) {
                return;
            }

            // Update optimistic state immediately for visual feedback
            setPendingModels(result);

            // Clear any existing debounce timer
            if (debounceRef.current !== null) {
                clearTimeout(debounceRef.current);
            }

            // Debounce the actual callback by 300ms
            debounceRef.current = setTimeout(() => {
                debounceRef.current = null;
                setPendingModels(null);
                onModelsChange(result);
            }, 300);
        },
        [enabledModels, pendingModels, onModelsChange],
    );

    return (
        <div class="toggle-bar" role="group" aria-label="Ensemble model toggles">
            {MODELS.map(({ id, label }) => {
                const isActive = displayModels.has(id);
                return (
                    <button
                        key={id}
                        type="button"
                        class={`toggle-bar__button${isActive ? ' toggle-bar__button--active' : ''}`}
                        aria-pressed={isActive}
                        onClick={() => handleToggle(id)}
                    >
                        {label}
                    </button>
                );
            })}
        </div>
    );
}
