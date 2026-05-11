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
 * Callers can check `error.status` to handle specific HTTP errors.
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
 * Thin wrapper around `fetch` that provides typed methods for each API endpoint.
 * The API key is injected by CloudFront at the origin level, so the frontend
 * does not need to send it. This client simply makes same-origin requests.
 */
export class WeatherApiClient {
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
        const data = await this.request<{ stations: StationsResponse }>(observationStationsUrl(lat, lon));
        return data.stations;
    }

    async marineStations(lat: number, lon: number): Promise<StationsResponse> {
        const data = await this.request<{ stations: StationsResponse }>(marineStationsUrl(lat, lon));
        return data.stations;
    }

    private async request<T>(url: string): Promise<T> {
        let response: Response;
        try {
            response = await fetch(url);
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
