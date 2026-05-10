export interface RefreshControlsProps {
    onFullRefresh: () => void;
    onSourceRefresh: (source: string) => void;
    isLoading: boolean;
    sources: string[];
}

/**
 * Refresh Controls component.
 * Displays a "Refresh All" button and per-source refresh buttons.
 * Shows a loading indicator (spinner text) when isLoading is true.
 * Disables all buttons during loading to prevent duplicate requests.
 */
export function RefreshControls({ onFullRefresh, onSourceRefresh, isLoading, sources }: RefreshControlsProps) {
    return (
        <div class="refresh-controls" role="group" aria-label="Refresh controls">
            <button
                type="button"
                class="refresh-controls__btn refresh-controls__btn--full"
                onClick={onFullRefresh}
                disabled={isLoading}
                aria-busy={isLoading}
            >
                {isLoading ? (
                    <span class="refresh-controls__spinner" aria-hidden="true" />
                ) : null}
                Refresh All
            </button>

            {sources.length > 0 && (
                <div class="refresh-controls__sources">
                    {sources.map((source) => (
                        <button
                            key={source}
                            type="button"
                            class="refresh-controls__btn refresh-controls__btn--source"
                            onClick={() => onSourceRefresh(source)}
                            disabled={isLoading}
                        >
                            {source}
                        </button>
                    ))}
                </div>
            )}
        </div>
    );
}
