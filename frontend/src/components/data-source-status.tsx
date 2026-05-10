import type { CacheInfo } from '../api/types';

export interface DataSourceStatusProps {
    cache: Record<string, CacheInfo>;
    errors: Record<string, string | null>;
}

/**
 * Formats a cache age in seconds into a human-readable string.
 * Examples: "2m ago", "1h ago", "30s ago", "2h ago"
 */
export function formatCacheAge(ageSeconds: number): string {
    if (ageSeconds < 60) {
        return `${Math.round(ageSeconds)}s ago`;
    }
    if (ageSeconds < 3600) {
        return `${Math.round(ageSeconds / 60)}m ago`;
    }
    if (ageSeconds < 86400) {
        return `${Math.round(ageSeconds / 3600)}h ago`;
    }
    return `${Math.round(ageSeconds / 86400)}d ago`;
}

/**
 * Data Source Status component.
 * Displays each data source with its cache age, freshness indicator, and error state.
 * Uses CSS classes from panels.css (.panel__status, .panel__status-dot, etc.)
 */
export function DataSourceStatus({ cache, errors }: DataSourceStatusProps) {
    const sourceNames = Object.keys(cache);

    if (sourceNames.length === 0) {
        return null;
    }

    return (
        <div class="data-source-status">
            {sourceNames.map((source) => {
                const info = cache[source];
                const error = errors[source] ?? null;
                const hasError = error !== null && error !== undefined;

                let dotClass = 'panel__status-dot';
                if (hasError) {
                    dotClass += ' panel__status-dot--error';
                } else if (info.is_fresh) {
                    dotClass += ' panel__status-dot--fresh';
                } else {
                    dotClass += ' panel__status-dot--stale';
                }

                return (
                    <div key={source} class="panel__status">
                        <span class={dotClass} aria-hidden="true" />
                        <span class="data-source-status__name">{source}</span>
                        <span class="data-source-status__age">{formatCacheAge(info.age_seconds)}</span>
                        {hasError && (
                            <span class="data-source-status__error">{error}</span>
                        )}
                    </div>
                );
            })}
        </div>
    );
}
