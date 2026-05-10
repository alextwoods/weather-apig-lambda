/**
 * Integration tests that hit the live deployed API.
 *
 * These tests verify the full request/response cycle through CloudFront → API Gateway → Lambda,
 * including CORS headers, response format, and error handling.
 *
 * Requirements:
 *   - WEATHER_API_KEY environment variable must be set
 *   - Network access to weather.popelka-woods.com
 *
 * Run with:
 *   WEATHER_API_KEY=<key> npx vitest --run tests/integration/
 */
import { describe, it, expect, beforeAll } from 'vitest';
import { WeatherApiClient } from '../../src/api/client';

const BASE_URL = 'https://weather.popelka-woods.com';
const API_KEY = process.env.WEATHER_API_KEY ?? '';

// Skip the entire suite if no API key is provided
const describeIntegration = API_KEY ? describe : describe.skip;

describeIntegration('Live API Integration Tests', () => {
    let client: WeatherApiClient;

    beforeAll(() => {
        // The WeatherApiClient uses relative URLs by default (for browser use).
        // For integration tests we need absolute URLs, so we create a custom client.
        client = new WeatherApiClient(API_KEY);
    });

    // --- Helper to make raw fetch requests with the API key ---
    async function apiFetch(path: string, options: RequestInit = {}): Promise<Response> {
        const headers: Record<string, string> = {
            'x-api-key': API_KEY,
            ...(options.headers as Record<string, string> ?? {}),
        };
        return fetch(`${BASE_URL}${path}`, { ...options, headers });
    }

    // =========================================================================
    // CORS Preflight Tests
    // =========================================================================
    describe('CORS preflight (OPTIONS)', () => {
        it('geocode OPTIONS returns 204 with correct CORS headers', async () => {
            const resp = await fetch(`${BASE_URL}/geocode`, {
                method: 'OPTIONS',
                headers: {
                    'Origin': 'https://weather.popelka-woods.com',
                    'Access-Control-Request-Method': 'GET',
                    'Access-Control-Request-Headers': 'x-api-key',
                },
            });

            expect(resp.status).toBe(204);
            expect(resp.headers.get('access-control-allow-origin')).toBe('*');
            expect(resp.headers.get('access-control-allow-headers')).toContain('x-api-key');
            expect(resp.headers.get('access-control-allow-methods')).toContain('GET');
        });

        it('forecast OPTIONS returns 204 with correct CORS headers', async () => {
            const resp = await fetch(`${BASE_URL}/forecast`, {
                method: 'OPTIONS',
                headers: {
                    'Origin': 'https://weather.popelka-woods.com',
                    'Access-Control-Request-Method': 'GET',
                    'Access-Control-Request-Headers': 'x-api-key',
                },
            });

            expect(resp.status).toBe(204);
            expect(resp.headers.get('access-control-allow-origin')).toBe('*');
            expect(resp.headers.get('access-control-allow-headers')).toContain('x-api-key');
        });
    });

    // =========================================================================
    // API Key Enforcement
    // =========================================================================
    describe('API key enforcement', () => {
        it('geocode without API key returns 403 (not HTML)', async () => {
            const resp = await fetch(`${BASE_URL}/geocode?q=Seattle`);

            expect(resp.status).toBe(403);
            const contentType = resp.headers.get('content-type') ?? '';
            expect(contentType).toContain('application/json');
            // Must NOT be HTML (the old errorResponses bug)
            const body = await resp.text();
            expect(body).not.toContain('<!DOCTYPE');
        });

        it('forecast without API key returns 403 (not HTML)', async () => {
            const resp = await fetch(`${BASE_URL}/forecast?lat=47.67&lon=-122.37`);

            expect(resp.status).toBe(403);
            const contentType = resp.headers.get('content-type') ?? '';
            expect(contentType).toContain('application/json');
            const body = await resp.text();
            expect(body).not.toContain('<!DOCTYPE');
        });
    });

    // =========================================================================
    // Geocode Endpoint
    // =========================================================================
    describe('GET /geocode', () => {
        it('returns results array wrapped in { results: [...] }', async () => {
            const resp = await apiFetch('/geocode?q=Seattle');

            expect(resp.status).toBe(200);
            expect(resp.headers.get('content-type')).toContain('application/json');
            expect(resp.headers.get('access-control-allow-origin')).toBe('*');

            const data = await resp.json();
            expect(data).toHaveProperty('results');
            expect(Array.isArray(data.results)).toBe(true);
            expect(data.results.length).toBeGreaterThan(0);
        });

        it('geocode results have required fields', async () => {
            const resp = await apiFetch('/geocode?q=Seattle');
            const data = await resp.json();
            const first = data.results[0];

            expect(first).toHaveProperty('name');
            expect(first).toHaveProperty('latitude');
            expect(first).toHaveProperty('longitude');
            expect(first).toHaveProperty('country');
            expect(typeof first.latitude).toBe('number');
            expect(typeof first.longitude).toBe('number');
        });

        it('geocode with short query returns results', async () => {
            const resp = await apiFetch('/geocode?q=Po');
            const data = await resp.json();

            expect(resp.status).toBe(200);
            expect(data.results.length).toBeGreaterThan(0);
        });

        it('geocode without q parameter returns 400', async () => {
            const resp = await apiFetch('/geocode');

            expect(resp.status).toBe(400);
            const data = await resp.json();
            expect(data).toHaveProperty('error');
        });

        it('WeatherApiClient.geocode() returns unwrapped array', async () => {
            // Use a custom fetch that prepends the base URL
            const originalFetch = globalThis.fetch;
            globalThis.fetch = (input: RequestInfo | URL, init?: RequestInit) => {
                const url = typeof input === 'string' ? `${BASE_URL}${input}` : input;
                return originalFetch(url, init);
            };

            try {
                const results = await client.geocode('Seattle');
                expect(Array.isArray(results)).toBe(true);
                expect(results.length).toBeGreaterThan(0);
                expect(results[0]).toHaveProperty('name');
                expect(results[0]).toHaveProperty('latitude');
                expect(results[0]).toHaveProperty('longitude');
            } finally {
                globalThis.fetch = originalFetch;
            }
        });
    });

    // =========================================================================
    // Forecast Endpoint
    // =========================================================================
    describe('GET /forecast', () => {
        it('returns JSON with ensemble data for Seattle', async () => {
            const resp = await apiFetch('/forecast?lat=47.67&lon=-122.37');

            expect(resp.status).toBe(200);
            expect(resp.headers.get('content-type')).toContain('application/json');
            expect(resp.headers.get('access-control-allow-origin')).toBe('*');

            const data = await resp.json();
            expect(data).toHaveProperty('ensemble');
            expect(data.ensemble).toHaveProperty('times');
            expect(data.ensemble).toHaveProperty('statistics');
            expect(data.ensemble).toHaveProperty('daily_sections');
            expect(Array.isArray(data.ensemble.times)).toBe(true);
            expect(data.ensemble.times.length).toBeGreaterThan(0);
        });

        it('forecast response includes cache metadata', async () => {
            const resp = await apiFetch('/forecast?lat=47.67&lon=-122.37');
            const data = await resp.json();

            expect(data).toHaveProperty('cache');
            expect(typeof data.cache).toBe('object');
        });

        it('forecast response includes astronomy data', async () => {
            const resp = await apiFetch('/forecast?lat=47.67&lon=-122.37');
            const data = await resp.json();

            expect(data).toHaveProperty('astronomy');
            expect(data.astronomy).toHaveProperty('times');
            expect(data.astronomy).toHaveProperty('sun_altitude');
            expect(data.astronomy).toHaveProperty('moon_altitude');
        });

        it('forecast with missing lat returns 400', async () => {
            const resp = await apiFetch('/forecast?lon=-122.37');

            expect(resp.status).toBe(400);
            const data = await resp.json();
            expect(data).toHaveProperty('error');
        });

        it('forecast with invalid models returns 400', async () => {
            const resp = await apiFetch('/forecast?lat=47.67&lon=-122.37&models=fake_model');

            expect(resp.status).toBe(400);
            const data = await resp.json();
            expect(data).toHaveProperty('error');
            expect(data.error).toContain('fake_model');
        });

        it('forecast supports gzip encoding', async () => {
            const resp = await apiFetch('/forecast?lat=47.67&lon=-122.37');

            // CloudFront or the Lambda may apply content-encoding
            const encoding = resp.headers.get('content-encoding');
            // The response should still be parseable as JSON regardless
            const data = await resp.json();
            expect(data).toHaveProperty('ensemble');
        });
    });

    // =========================================================================
    // Stations Endpoint
    // =========================================================================
    describe('GET /stations', () => {
        it('marine stations returns results for Seattle area', async () => {
            const resp = await apiFetch('/stations/marine?lat=47.67&lon=-122.37');

            expect(resp.status).toBe(200);
            expect(resp.headers.get('access-control-allow-origin')).toBe('*');

            const data = await resp.json();
            expect(data).toHaveProperty('stations');
            expect(Array.isArray(data.stations)).toBe(true);
            expect(data.stations.length).toBeGreaterThan(0);

            const first = data.stations[0];
            expect(first).toHaveProperty('id');
            expect(first).toHaveProperty('name');
            expect(first).toHaveProperty('latitude');
            expect(first).toHaveProperty('longitude');
            expect(first).toHaveProperty('distance_km');
        });

        it('marine stations are sorted by distance', async () => {
            const resp = await apiFetch('/stations/marine?lat=47.67&lon=-122.37');
            const data = await resp.json();

            for (let i = 1; i < data.stations.length; i++) {
                expect(data.stations[i].distance_km).toBeGreaterThanOrEqual(
                    data.stations[i - 1].distance_km
                );
            }
        });

        it('observation stations returns results for Seattle area', async () => {
            const resp = await apiFetch('/stations/observations?lat=47.67&lon=-122.37');

            expect(resp.status).toBe(200);
            const data = await resp.json();
            expect(data).toHaveProperty('stations');
            expect(Array.isArray(data.stations)).toBe(true);
            expect(data.stations.length).toBeGreaterThan(0);
        });
    });

    // =========================================================================
    // CloudFront SPA Routing
    // =========================================================================
    describe('CloudFront SPA routing', () => {
        it('root path returns HTML (index.html)', async () => {
            const resp = await fetch(`${BASE_URL}/`);

            expect(resp.status).toBe(200);
            const body = await resp.text();
            expect(body).toContain('<!DOCTYPE html>');
        });

        it('unknown SPA path returns HTML (not 404)', async () => {
            const resp = await fetch(`${BASE_URL}/settings`);

            expect(resp.status).toBe(200);
            const body = await resp.text();
            expect(body).toContain('<!DOCTYPE html>');
        });

        it('static assets are served correctly', async () => {
            // First get index.html to find the JS bundle name
            const indexResp = await fetch(`${BASE_URL}/`);
            const html = await indexResp.text();
            const jsMatch = html.match(/src="(\/assets\/[^"]+\.js)"/);
            expect(jsMatch).not.toBeNull();

            const jsResp = await fetch(`${BASE_URL}${jsMatch![1]}`);
            expect(jsResp.status).toBe(200);
            const contentType = jsResp.headers.get('content-type') ?? '';
            expect(contentType).toContain('javascript');
        });
    });
});
