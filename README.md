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

## Deploying with CDK

### First-time setup

```bash
npm install
cdk bootstrap  # only needed once per account/region
```

### Build, then deploy

```bash
make              # build + package Rust Lambdas
cdk synth         # preview the CloudFormation template
cdk diff          # review changes before deploying
cdk deploy
```

### What the stack creates

- API Gateway (REST) at `weather.popelka-woods.com` with CORS enabled
- 4 Lambda functions (forecast, geocode, metadata, stations) on ARM64 / AL2023
- S3 bucket (`weather-cache`) with 1-day lifecycle expiration
- DynamoDB table (`weather-cache`) with TTL, pay-per-request billing
- ACM certificate with DNS validation
- Route 53 A record pointing to the API Gateway
- CloudWatch access logging for the API

## Project Structure

```
weather-apig-lambda/
├── bin/                    # CDK app entry point
├── lib/                    # CDK stack definition
├── crates/                 # Rust Lambda source code
│   ├── forecast/           #   Forecast aggregation + caching
│   ├── geocode/            #   Location search proxy
│   ├── metadata/           #   Model metadata proxy
│   └── stations/           #   NWS + NOAA station lookups
├── data/
│   └── noaa_stations.json  # Bundled NOAA station registry
├── Cargo.toml              # Rust workspace root
├── Makefile                # Build + package commands
├── cdk.json                # CDK app configuration
└── package.json            # CDK Node.js dependencies
```
