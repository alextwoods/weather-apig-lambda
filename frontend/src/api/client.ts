import type {
    ForecastResponse,
    MembersResponse,
    GeocodeResponse,
    StationsResponse,
} from './types';
import {
    forecastUrl,
    forecastMembersUrl,
    geocodeUrl,
    observationStationsUrl,
    marineStationsUrl,
    type ForecastParams,
    type MembersParams,
} from './endpoints';

/**
 * Custom error class that includes the HTTP status code.
 * Callers can check `error.status === 403` to trigger the API key prompt.
 */
export class ApiError extends Error {
    public readonly status: number;

    constructor(message: string, status: number) {
        super(message);
        this.name = 'ApiError';
        this.status = status;
    }
}

/**
 * Thin wrapper around `fetch` that injects the `x-api-key` header
 * and provides typed methods for each API endpoint.
 */
export class WeatherApiClient {
    private apiKey: string | null;

    constructor(apiKey: string | null = null) {
        this.apiKey = apiKey;
    }

    setApiKey(key: string): void {
        this.apiKey = key;
    }

    getApiKey(): string | null {
        return this.apiKey;
    }

    async forecast(params: ForecastParams): Promise<ForecastResponse> {
        return this.request<ForecastResponse>(forecastUrl(params));
    }

    async forecastMembers(params: MembersParams): Promise<MembersResponse> {
        return this.request<MembersResponse>(forecastMembersUrl(params));
    }

    async geocode(query: string): Promise<GeocodeResponse> {
        const data = await this.request<{ results: GeocodeResponse }>(geocodeUrl(query));
        return data.results;
    }

    async observationStations(lat: number, lon: number): Promise<StationsResponse> {
        return this.request<StationsResponse>(observationStationsUrl(lat, lon));
    }

    async marineStations(lat: number, lon: number): Promise<StationsResponse> {
        return this.request<StationsResponse>(marineStationsUrl(lat, lon));
    }

    private async request<T>(url: string): Promise<T> {
        const headers: Record<string, string> = {};
        if (this.apiKey) {
            headers['x-api-key'] = this.apiKey;
        }

        let response: Response;
        try {
            response = await fetch(url, { headers });
        } catch (error) {
            throw new ApiError(
                error instanceof Error ? error.message : 'Network error',
                0
            );
        }

        if (!response.ok) {
            const body = await response.text().catch(() => '');
            const message = body || `HTTP ${response.status}`;
            throw new ApiError(message, response.status);
        }

        return response.json() as Promise<T>;
    }
}
