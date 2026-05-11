import { useState } from 'preact/hooks';
import type { CacheInfo } from '../api/types';

export interface RefreshControlsProps {
    onFullRefresh: () => void;
    onSourceRefresh: (source: string) => void;
    isLoading: boolean;
    sources: string[];
    cacheInfo?: Record<string, CacheInfo>;
    errors?: Record<string, string | null>;
}

/** Format cache age as a human-readable string. */
function formatAge(seconds: number): string {
    if (seconds < 60) return `${Math.round(seconds)}s ago`;
    if (seconds < 3600) return `${Math.round(seconds / 60)}m ago`;
    return `${(seconds / 3600).toFixed(1)}h ago`;
}

/**
 * Sources Info accordion.
 * Shows a compact summary line (collapsed) with source count and freshness.
 * Expands to show per-source cache status, timing, and a "Refresh All" button.
 */
export function RefreshControls({ onFullRefresh, onSourceRefresh, isLoading, sources, cacheInfo, errors }: RefreshControlsProps) {
    const [expanded, setExpanded] = useState(false);

    if (sources.length === 0 && !isLoading) return null;

    // Compute summary
    const freshCount = cacheInfo
        ? Object.values(cacheInfo).filter(c => c.is_fresh).length
        : 0;
    const totalCount = sources.length;
    const oldestAge = cacheInfo
        ? Math.max(...Object.values(cacheInfo).map(c => c.age_seconds))
        : 0;

    return (
        <div class="sources-info">
            {/* Collapsed summary line */}
            <button
                type="button"
                class="sources-info__toggle"
                onClick={() => setExpanded(!expanded)}
                aria-expanded={expanded}
            >
                <span class="sources-info__summary">
                    {isLoading ? (
                        <span class="sources-info__loading">Loading...</span>
                    ) : (
                        <>
                            <span class="sources-info__dot sources-info__dot--fresh" />
                            {freshCount}/{totalCount} fresh
                            {oldestAge > 0 && (
                                <span class="sources-info__age"> · oldest {formatAge(oldestAge)}</span>
                            )}
                        </>
                    )}
                </span>
                <span class="sources-info__chevron">{expanded ? '▾' : '▸'}</span>
            </button>

            {/* Expanded detail */}
            {expanded && (
                <div class="sources-info__detail">
                    <ul class="sources-info__list">
                        {sources.map((source) => {
                            const cache = cacheInfo?.[source];
                            const error = errors?.[source];
                            return (
                                <li key={source} class="sources-info__item">
                                    <span class={`sources-info__dot ${cache?.is_fresh ? 'sources-info__dot--fresh' : 'sources-info__dot--stale'}`} />
                                    <span class="sources-info__source-name">{source}</span>
                                    <span class="sources-info__source-status">
                                        {error ? (
                                            <span class="sources-info__error">error</span>
                                        ) : cache ? (
                                            formatAge(cache.age_seconds)
                                        ) : '—'}
                                    </span>
                                    <button
                                        type="button"
                                        class="sources-info__refresh-btn"
                                        onClick={() => onSourceRefresh(source)}
                                        disabled={isLoading}
                                        aria-label={`Refresh ${source}`}
                                    >
                                        ↻
                                    </button>
                                </li>
                            );
                        })}
                    </ul>
                    <button
                        type="button"
                        class="sources-info__refresh-all"
                        onClick={onFullRefresh}
                        disabled={isLoading}
                    >
                        {isLoading ? 'Refreshing...' : '↻ Refresh All Sources'}
                    </button>
                </div>
            )}
        </div>
    );
}
