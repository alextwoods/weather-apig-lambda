mod cache;
mod compute;
mod fetcher;
mod models;
mod response;
mod router;
mod sources;

use std::sync::Arc;

use lambda_http::{run, service_fn, Error, Request};
use models::{AppConfig, AppState};

#[tokio::main]
async fn main() -> Result<(), Error> {
    // Load AWS SDK config from environment (region, credentials, etc.)
    let aws_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;

    // Create AWS SDK clients
    let s3_client = aws_sdk_s3::Client::new(&aws_config);
    let ddb_client = aws_sdk_dynamodb::Client::new(&aws_config);

    // Read configuration from environment variables
    let cache_bucket = std::env::var("CACHE_BUCKET").unwrap_or_default();
    let cache_table = std::env::var("CACHE_TABLE").unwrap_or_default();

    // Create a shared reqwest::Client with connection pooling and default timeouts
    let http_client = reqwest::Client::builder()
        .pool_max_idle_per_host(10)
        .build()
        .expect("failed to build reqwest client");

    // Build shared application state wrapped in Arc for sharing across requests
    let state = Arc::new(AppState {
        http_client,
        s3_client,
        ddb_client,
        config: AppConfig {
            cache_bucket,
            cache_table,
            ..AppConfig::default()
        },
    });

    // Run the Lambda runtime, routing each request through the router
    run(service_fn(move |event: Request| {
        let state = Arc::clone(&state);
        async move { router::route(&event, &state).await }
    }))
    .await
}
