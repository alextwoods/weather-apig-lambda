export interface LoadingIndicatorProps {
    isLoading: boolean;
    message?: string;
}

/**
 * Loading Indicator component.
 * Displays a simple CSS spinner with an optional message.
 * Only renders when `isLoading` is true.
 * Uses `aria-live="polite"` for accessibility so screen readers
 * announce loading state changes.
 */
export function LoadingIndicator({ isLoading, message = 'Loading...' }: LoadingIndicatorProps) {
    if (!isLoading) {
        return null;
    }

    return (
        <div class="loading-indicator" aria-live="polite" role="status">
            <div class="loading-indicator__spinner" aria-hidden="true" />
            <span class="loading-indicator__message">{message}</span>
        </div>
    );
}
