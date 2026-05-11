# Weather APIG Lambda

Backend API for the EnsembleWeather app. Aggregates data from Open-Meteo (ensemble forecasts, marine, HRRR, UV, air quality), NOAA (tides, observations), and ECCC (CIOPS-Salish Sea SST), then returns pre-computed percentile statistics and precipitation probabilities. Runs on API Gateway + Lambda (Rust, ARM64) with DynamoDB and S3 caching.

## Architecture

Four Lambda functions behind a single API Gateway at `weather.popelka-woods.com`:

| Endpoint | Lambda | Description |
|----------|--------|-------------|
| `GET /forecast` | forecast | Fetches upstream weather sources, computes percentile stats, caches results |
| `GET /geocode` | geocode | Proxies Open-Meteo Geocoding API for location search |
| `GET /models/metadata` | metadata | Returns model initialization times and update intervals |
| `GET /stations/observations` | stations | NWS observation station discovery and recent observations |
| `GET /stations/marine` | stations | Nearby NOAA marine/tide station search |

The forecast Lambda uses an S3 bucket and DynamoDB table for caching upstream API responses. All Lambdas are compiled as standalone Rust binaries targeting `aarch64-unknown-linux-musl` (ARM64 + static linking).

## Prerequisites

- Rust toolchain with the `aarch64-unknown-linux-musl` target
- [Zig](https://ziglang.org/) and [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) (for cross-compilation)
- Node.js 18+
- AWS CDK CLI (`npm install -g aws-cdk`)
- AWS credentials configured
- `zip` utility (for packaging Lambda artifacts)

### Install the Rust cross-compilation target

```bash
rustup target add aarch64-unknown-linux-musl
```

### Install Zig and cargo-zigbuild

The build uses `cargo-zigbuild` with Zig as the cross-linker, which avoids needing a GCC-based musl cross-compiler toolchain.

```bash
brew install zig
cargo install cargo-zigbuild
```

## Building

### Install CDK dependencies

```bash
npm install
```

### Build and package the Lambda functions

```bash
make
```

This runs two steps:

1. `make build` — compiles all four crates in release mode for `aarch64-unknown-linux-musl`
2. `make package` — copies each binary to `bootstrap` and zips it into `target/lambda/{crate}.zip`

The resulting zip files are what CDK deploys as Lambda code assets.

### Run tests

Rust tests (unit + property-based via proptest):

```bash
make test
```

CDK stack tests:

```bash
npm test
```



## Frontend (Web UI)

A Preact + uPlot + TypeScript single-page application that replicates the EnsembleWeather iOS app as a web view. Served from S3 via CloudFront on the same domain as the API.

### Prerequisites

- Node.js 18+

### Development

```bash
cd frontend
npm install
npm run dev       # starts Vite dev server with HMR
```

The dev server proxies API requests to the production backend by default (same-origin requests to `/forecast`, `/geocode`, etc.).

### Running tests

```bash
cd frontend
npm test          # runs vitest (50 tests, including property-based tests)
```

### Building for production

```bash
cd frontend
npm run build     # outputs to frontend/dist/
```

This produces content-hashed JS and CSS bundles in `frontend/dist/`. The CDK stack deploys this directory to S3 and invalidates the CloudFront cache.

## Build and deploy it all - one shot
```
make && cd frontend && npm run build && cd ../ && cdk deploy
```

### Architecture

The frontend is served via CloudFront with dual-origin routing:

- **Default behavior** → S3 bucket (static assets: HTML, CSS, JS)
- **API paths** (`/forecast*`, `/geocode*`, `/models/*`, `/stations/*`) → API Gateway origin

CloudFront handles SPA routing by returning `index.html` for 403/404 responses from S3. The `x-api-key` header is passed through to the API Gateway origin.

### Key technologies

| Component | Choice | Size |
|-----------|--------|------|
| UI Framework | Preact | ~4 KB gzipped |
| Charting | uPlot | ~30 KB gzipped |
| Bundler | Vite | — |
| Language | TypeScript | — |
| Testing | Vitest + fast-check | — |

## Deploying with CDK

### First-time setup

```bash
npm install
cdk bootstrap  # only needed once per account/region
```

### Build everything, then deploy

```bash
make                      # build + package Rust Lambdas
cd frontend && npm run build && cd ..   # build frontend
cdk synth                 # preview the CloudFormation template
cdk diff                  # review changes before deploying
cdk deploy
```

### What the stack creates

- CloudFront distribution at `weather.popelka-woods.com` with dual-origin routing
- S3 bucket for frontend static assets (deployed from `frontend/dist/`)
- API Gateway (REST) with CORS enabled
- 5 Lambda functions (forecast, geocode, metadata, stations, cache_warmer) on ARM64 / AL2023
- S3 bucket (`weather-cache`) with 1-day lifecycle expiration
- DynamoDB tables (`weather-cache`, `weather-location-tracker`) with TTL, pay-per-request billing
- ACM certificates (one for API Gateway, one in us-east-1 for CloudFront)
- Route 53 A record pointing to CloudFront
- EventBridge rule triggering cache warmer every 30 minutes
- CloudWatch dashboard with latency, cache hit, and warmer metrics

## Project Structure

```
weather-apig-lambda/
├── bin/                    # CDK app entry point
├── lib/                    # CDK stack definition
├── crates/                 # Rust Lambda source code
│   ├── cache_warmer/       #   Background cache warming
│   ├── forecast/           #   Forecast aggregation + caching
│   ├── geocode/            #   Location search proxy
│   ├── metadata/           #   Model metadata proxy
│   └── stations/           #   NWS + NOAA station lookups
├── frontend/               # Web frontend (Preact + uPlot + TypeScript)
│   ├── src/                #   Application source
│   │   ├── api/            #     API client and types
│   │   ├── charts/         #     uPlot chart infrastructure
│   │   ├── components/     #     UI control components
│   │   ├── panels/         #     Chart panel components
│   │   ├── state/          #     URL state, local storage, app store
│   │   ├── styles/         #     CSS (global, layout, panels)
│   │   └── units/          #     Unit conversion module
│   ├── tests/              #   Vitest tests (property-based + unit)
│   ├── dist/               #   Production build output (deployed to S3)
│   ├── package.json        #   Frontend dependencies
│   └── vite.config.ts      #   Vite + Vitest configuration
├── data/
│   └── noaa_stations.json  # Bundled NOAA station registry
├── Cargo.toml              # Rust workspace root
├── Makefile                # Build + package commands
├── cdk.json                # CDK app configuration
└── package.json            # CDK Node.js dependencies
```
