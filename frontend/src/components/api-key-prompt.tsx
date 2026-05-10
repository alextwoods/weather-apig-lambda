import { useRef, useEffect, useState } from 'preact/hooks';

export interface ApiKeyPromptProps {
    isOpen: boolean;
    onSubmit: (apiKey: string) => void;
    onClose: () => void;
}

/**
 * API Key Prompt modal component.
 * Displays a modal overlay for entering or updating the API key.
 * Triggered on HTTP 403 or when no API key is stored.
 * Focuses the input when opened.
 */
export function ApiKeyPrompt({ isOpen, onSubmit, onClose }: ApiKeyPromptProps) {
    const inputRef = useRef<HTMLInputElement>(null);
    const [value, setValue] = useState('');

    useEffect(() => {
        if (isOpen && inputRef.current) {
            inputRef.current.focus();
        }
    }, [isOpen]);

    if (!isOpen) {
        return null;
    }

    function handleSubmit(e: Event): void {
        e.preventDefault();
        const trimmed = value.trim();
        if (trimmed.length > 0) {
            onSubmit(trimmed);
            setValue('');
        }
    }

    function handleKeyDown(e: KeyboardEvent): void {
        if (e.key === 'Escape') {
            onClose();
        }
    }

    return (
        <div
            class="modal-overlay"
            role="dialog"
            aria-modal="true"
            aria-labelledby="api-key-prompt-title"
            onKeyDown={handleKeyDown}
        >
            <div class="modal-card">
                <h2 id="api-key-prompt-title" class="modal-card__title">
                    API Key Required
                </h2>
                <p class="modal-card__description">
                    Enter your API key to access weather forecast data.
                </p>
                <form onSubmit={handleSubmit} class="modal-card__form">
                    <input
                        ref={inputRef}
                        type="text"
                        class="modal-card__input"
                        placeholder="Enter API key"
                        value={value}
                        onInput={(e) => setValue((e.target as HTMLInputElement).value)}
                        aria-label="API key"
                        autocomplete="off"
                    />
                    <div class="modal-card__actions">
                        <button
                            type="button"
                            class="modal-card__btn modal-card__btn--secondary"
                            onClick={onClose}
                        >
                            Cancel
                        </button>
                        <button
                            type="submit"
                            class="modal-card__btn modal-card__btn--primary"
                            disabled={value.trim().length === 0}
                        >
                            Submit
                        </button>
                    </div>
                </form>
            </div>
        </div>
    );
}
