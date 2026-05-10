import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { WeatherApiClient, ApiError } from '../../src/api/client';
import {
    forecastUrl,
    forecastMembersUrl,
    geocodeUrl,
    observationStationsUrl,
    marineStationsUrl,
} from '../../src/api/endpoints';

// --- Endpoint URL construction tests ---

describe('endpoint URL builders', () => {
    describe('forecastUrl', () => {
        it('builds URL with required lat/lon only', () => {
            const url = forecastUrl({ lat: 47.61, lon: -122.33 });
            expect(url).toBe('/forecast?lat=47.61&lon=-122.33');
        });

        it('includes marine_lat and marine_lon when provided', () => {
            const url = forecastUrl({ lat: 47.61, lon: -122.33, marine_lat: 47.5, marine_lon: -122.4 });
            expect(url).toContain('marine_lat=47.5');
            expect(url).toContain('marine_lon=-122.4');
        });

        it('includes station_id when provided', () => {
            const url = forecastUrl({ lat: 47.61, lon: -122.33, station_id: 'KBFI' });
            expect(url).toContain('station_id=KBFI');
        });

        it('joins models with commas', () => {
            const url = forecastUrl({ lat: 47.61, lon: -122.33, models: ['ecmwf_ifs025_ensemble', 'ncep_gefs_seamless'] });
            expect(url).toContain('models=ecmwf_ifs025_ensemble%2Cncep_gefs_seamless');
        });

        it('maps short model names to API suffixes', () => {
            const url = forecastUrl({ lat: 47.61, lon: -122.33, models: ['ecmwf', 'gfs'] });
            expect(url).toContain('models=ecmwf_ifs025_ensemble%2Cncep_gefs_seamless');
        });

        it('includes force_refresh=true when set', () => {
            const url = forecastUrl({ lat: 47.61, lon: -122.33, force_refresh: true });
            expect(url).toContain('force_refresh=true');
        });

        it('omits force_refresh when false', () => {
            const url = forecastUrl({ lat: 47.61, lon: -122.33, force_refresh: false });
            expect(url).not.toContain('force_refresh');
        });

        it('includes refresh_source when provided', () => {
            const url = forecastUrl({ lat: 47.61, lon: -122.33, refresh_source: 'ensemble' });
            expect(url).toContain('refresh_source=ensemble');
        });

        it('omits optional params when undefined', () => {
            const url = forecastUrl({ lat: 0, lon: 0 });
            expect(url).toBe('/forecast?lat=0&lon=0');
        });
    });

    describe('forecastMembersUrl', () => {
        it('builds URL with required params', () => {
            const url = forecastMembersUrl({ variable: 'temperature_2m', lat: 47.61, lon: -122.33 });
            expect(url).toBe('/forecast/members?variable=temperature_2m&lat=47.61&lon=-122.33');
        });

        it('includes models when provided', () => {
            const url = forecastMembersUrl({ variable: 'wind_speed_10m', lat: 47.61, lon: -122.33, models: ['ecmwf_ifs025_ensemble'] });
            expect(url).toContain('models=ecmwf_ifs025_ensemble');
        });
    });

    describe('geocodeUrl', () => {
        it('builds URL with query string', () => {
            const url = geocodeUrl('Seattle');
            expect(url).toBe('/geocode?q=Seattle');
        });

        it('encodes special characters in query', () => {
            const url = geocodeUrl('New York City');
            expect(url).toBe('/geocode?q=New%20York%20City');
        });
    });

    describe('observationStationsUrl', () => {
        it('builds URL with lat/lon', () => {
            const url = observationStationsUrl(47.61, -122.33);
            expect(url).toBe('/stations/observations?lat=47.61&lon=-122.33');
        });
    });

    describe('marineStationsUrl', () => {
        it('builds URL with lat/lon', () => {
            const url = marineStationsUrl(47.61, -122.33);
            expect(url).toBe('/stations/marine?lat=47.61&lon=-122.33');
        });
    });
});

// --- WeatherApiClient tests ---

describe('WeatherApiClient', () => {
    let client: WeatherApiClient;
    let mockFetch: ReturnType<typeof vi.fn>;

    beforeEach(() => {
        mockFetch = vi.fn();
        vi.stubGlobal('fetch', mockFetch);
        client = new WeatherApiClient('test-api-key-123');
    });

    afterEach(() => {
        vi.unstubAllGlobals();
    });

    describe('x-api-key header injection', () => {
        it('includes x-api-key header when API key is set', async () => {
            mockFetch.mockResolvedValue({
                ok: true,
                json: () => Promise.resolve([]),
            });

            await client.geocode('Seattle');

            expect(mockFetch).toHaveBeenCalledWith(
                '/geocode?q=Seattle',
                { headers: { 'x-api-key': 'test-api-key-123' } },
            );
        });

        it('does NOT include x-api-key header when API key is null', async () => {
            const noKeyClient = new WeatherApiClient(null);
            mockFetch.mockResolvedValue({
                ok: true,
                json: () => Promise.resolve([]),
            });

            await noKeyClient.geocode('Seattle');

            expect(mockFetch).toHaveBeenCalledWith(
                '/geocode?q=Seattle',
                { headers: {} },
            );
        });

        it('uses updated key after setApiKey is called', async () => {
            client.setApiKey('new-key-456');
            mockFetch.mockResolvedValue({
                ok: true,
                json: () => Promise.resolve([]),
            });

            await client.geocode('Portland');

            expect(mockFetch).toHaveBeenCalledWith(
                '/geocode?q=Portland',
                { headers: { 'x-api-key': 'new-key-456' } },
            );
        });
    });

    describe('403 error detection', () => {
        it('throws ApiError with status 403 on forbidden response', async () => {
            mockFetch.mockResolvedValue({
                ok: false,
                status: 403,
                text: () => Promise.resolve('Forbidden'),
            });

            await expect(client.geocode('Seattle')).rejects.toThrow(ApiError);
            await expect(client.geocode('Seattle')).rejects.toMatchObject({
                status: 403,
                message: 'Forbidden',
            });
        });

        it('includes response body in error message', async () => {
            mockFetch.mockResolvedValue({
                ok: false,
                status: 403,
                text: () => Promise.resolve('Invalid API key'),
            });

            await expect(client.forecast({ lat: 47, lon: -122 })).rejects.toMatchObject({
                status: 403,
                message: 'Invalid API key',
            });
        });

        it('uses fallback message when response body is empty', async () => {
            mockFetch.mockResolvedValue({
                ok: false,
                status: 403,
                text: () => Promise.resolve(''),
            });

            await expect(client.geocode('test')).rejects.toMatchObject({
                status: 403,
                message: 'HTTP 403',
            });
        });
    });

    describe('network error handling', () => {
        it('wraps TypeError (network failure) in ApiError with status 0', async () => {
            mockFetch.mockRejectedValue(new TypeError('Failed to fetch'));

            await expect(client.geocode('Seattle')).rejects.toThrow(ApiError);
            await expect(client.geocode('Seattle')).rejects.toMatchObject({
                status: 0,
                message: 'Failed to fetch',
            });
        });

        it('wraps generic Error in ApiError with status 0', async () => {
            mockFetch.mockRejectedValue(new Error('Network timeout'));

            await expect(client.forecast({ lat: 47, lon: -122 })).rejects.toMatchObject({
                status: 0,
                message: 'Network timeout',
            });
        });

        it('uses fallback message for non-Error thrown values', async () => {
            mockFetch.mockRejectedValue('something weird');

            await expect(client.geocode('test')).rejects.toMatchObject({
                status: 0,
                message: 'Network error',
            });
        });
    });

    describe('successful responses', () => {
        it('returns parsed JSON for successful forecast response', async () => {
            const mockData = { ensemble: { times: ['2024-01-01T00:00:00Z'] } };
            mockFetch.mockResolvedValue({
                ok: true,
                json: () => Promise.resolve(mockData),
            });

            const result = await client.forecast({ lat: 47.61, lon: -122.33 });
            expect(result).toEqual(mockData);
        });

        it('returns parsed JSON for geocode response', async () => {
            const mockData = [{ name: 'Seattle', latitude: 47.61, longitude: -122.33, country: 'US' }];
            mockFetch.mockResolvedValue({
                ok: true,
                json: () => Promise.resolve({ results: mockData }),
            });

            const result = await client.geocode('Seattle');
            expect(result).toEqual(mockData);
        });

        it('calls correct URL for observationStations', async () => {
            mockFetch.mockResolvedValue({
                ok: true,
                json: () => Promise.resolve([]),
            });

            await client.observationStations(47.61, -122.33);
            expect(mockFetch).toHaveBeenCalledWith(
                '/stations/observations?lat=47.61&lon=-122.33',
                expect.objectContaining({ headers: { 'x-api-key': 'test-api-key-123' } }),
            );
        });

        it('calls correct URL for marineStations', async () => {
            mockFetch.mockResolvedValue({
                ok: true,
                json: () => Promise.resolve([]),
            });

            await client.marineStations(47.61, -122.33);
            expect(mockFetch).toHaveBeenCalledWith(
                '/stations/marine?lat=47.61&lon=-122.33',
                expect.objectContaining({ headers: { 'x-api-key': 'test-api-key-123' } }),
            );
        });

        it('calls correct URL for forecastMembers', async () => {
            mockFetch.mockResolvedValue({
                ok: true,
                json: () => Promise.resolve({ times: [], statistics: {}, members_by_model: {} }),
            });

            await client.forecastMembers({ variable: 'temperature_2m', lat: 47.61, lon: -122.33 });
            expect(mockFetch).toHaveBeenCalledWith(
                '/forecast/members?variable=temperature_2m&lat=47.61&lon=-122.33',
                expect.objectContaining({ headers: { 'x-api-key': 'test-api-key-123' } }),
            );
        });
    });

    describe('other HTTP errors', () => {
        it('throws ApiError with correct status for 500 response', async () => {
            mockFetch.mockResolvedValue({
                ok: false,
                status: 500,
                text: () => Promise.resolve('Internal Server Error'),
            });

            await expect(client.forecast({ lat: 47, lon: -122 })).rejects.toMatchObject({
                status: 500,
                message: 'Internal Server Error',
            });
        });

        it('throws ApiError with correct status for 404 response', async () => {
            mockFetch.mockResolvedValue({
                ok: false,
                status: 404,
                text: () => Promise.resolve('Not Found'),
            });

            await expect(client.geocode('nowhere')).rejects.toMatchObject({
                status: 404,
                message: 'Not Found',
            });
        });
    });
});
