import { useState, useRef, useCallback, useEffect } from 'preact/hooks';
import type { WeatherApiClient } from '../api/client';
import type { GeocodeResult } from '../api/types';

export interface LocationSearchProps {
    onLocationSelect: (location: { lat: number; lon: number; name: string }) => void;
    apiClient: WeatherApiClient;
}

/**
 * Location Search component.
 * Displays a text input that queries the geocode API with debounce,
 * shows matching results in a dropdown, and calls onLocationSelect
 * when the user picks a result.
 */
export function LocationSearch({ onLocationSelect, apiClient }: LocationSearchProps) {
    const [query, setQuery] = useState('');
    const [results, setResults] = useState<GeocodeResult[]>([]);
    const [isLoading, setIsLoading] = useState(false);
    const [error, setError] = useState<string | null>(null);
    const [isOpen, setIsOpen] = useState(false);

    const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);
    const containerRef = useRef<HTMLDivElement>(null);

    // Close dropdown when clicking outside
    useEffect(() => {
        function handleClickOutside(e: MouseEvent) {
            if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
                setIsOpen(false);
            }
        }
        document.addEventListener('mousedown', handleClickOutside);
        return () => document.removeEventListener('mousedown', handleClickOutside);
    }, []);

    const handleInput = useCallback(
        (e: Event) => {
            const value = (e.target as HTMLInputElement).value;
            setQuery(value);

            // Clear previous debounce
            if (debounceRef.current !== null) {
                clearTimeout(debounceRef.current);
            }

            // Clear results if query is too short
            if (value.trim().length < 2) {
                setResults([]);
                setIsOpen(false);
                setError(null);
                return;
            }

            // Debounce the API call by 300ms
            debounceRef.current = setTimeout(async () => {
                debounceRef.current = null;
                setIsLoading(true);
                setError(null);

                try {
                    const geocodeResults = await apiClient.geocode(value.trim());
                    setResults(geocodeResults);
                    setIsOpen(geocodeResults.length > 0);
                } catch (err) {
                    setError('Search failed');
                    setResults([]);
                    setIsOpen(true);
                } finally {
                    setIsLoading(false);
                }
            }, 300);
        },
        [apiClient],
    );

    const handleSelect = useCallback(
        (result: GeocodeResult) => {
            onLocationSelect({
                lat: result.latitude,
                lon: result.longitude,
                name: result.name,
            });
            setQuery(result.name);
            setIsOpen(false);
            setResults([]);
        },
        [onLocationSelect],
    );

    return (
        <div class="location-search" ref={containerRef}>
            <input
                type="text"
                class="location-search__input"
                placeholder="Search location..."
                value={query}
                onInput={handleInput}
                onFocus={() => {
                    if (results.length > 0) setIsOpen(true);
                }}
                aria-label="Search for a location"
                aria-expanded={isOpen}
                aria-autocomplete="list"
                role="combobox"
            />
            {isLoading && (
                <span class="location-search__loading" aria-live="polite">
                    Searching...
                </span>
            )}
            {isOpen && (
                <ul class="location-search__results" role="listbox">
                    {error ? (
                        <li class="location-search__error" role="alert">
                            {error}
                        </li>
                    ) : (
                        results.map((result, index) => (
                            <li key={index} role="option">
                                <button
                                    type="button"
                                    class="location-search__result"
                                    onClick={() => handleSelect(result)}
                                >
                                    <span class="location-search__result-name">
                                        {result.name}
                                    </span>
                                    {(result.admin1 || result.country) && (
                                        <span class="location-search__result-detail">
                                            {[result.admin1, result.country]
                                                .filter(Boolean)
                                                .join(', ')}
                                        </span>
                                    )}
                                </button>
                            </li>
                        ))
                    )}
                </ul>
            )}
        </div>
    );
}
