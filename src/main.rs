mod api;
mod instrument;
mod monitor;

use alloy::providers::ProviderBuilder;
use axum::{
    routing::{get, post},
    Router,
};
use axum_tracing_opentelemetry::middleware::{OtelAxumLayer, OtelInResponseLayer};
use clap::Parser;
use eyre::Result;
use monitor::AppState;
use std::{
    sync::{Arc, Mutex},
    time::SystemTime,
};
use tokio::time::{self, Duration, Instant};

/// Monitor an Ethereum node RPC endpoint
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct MonitorArgs {
    /// Listen address for the API
    #[clap(long, default_value = "127.0.0.1:8080")]
    listen: String,
    /// JSON-RPC URL of the Ethereum node
    #[clap(long, default_value = "http://localhost:8545")]
    rpc_url: String,
    /// Block frequency that is to be expected from the Ethereum node
    #[clap(long, default_value = "12")]
    block_frequency: u64,
    /// Enable OpenTelemetry tracing
    #[clap(long, default_value = "false")]
    tracing: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = MonitorArgs::parse();

    // Initialize tracing
    instrument::init(args.tracing);

    tracing::info!(
        rpc_url = args.rpc_url,
        block_frequency = args.block_frequency,
        "Starting Ethereum node monitor"
    );

    // Shared state used across the application
    let app_state = Arc::new(Mutex::new(AppState::new(args.block_frequency)));

    // Create the API
    let last_block_api_state = app_state.clone();
    let toggle_fail_api_state = app_state.clone();
    let health_api_state = app_state.clone();
    let app = Router::new()
        .route("/lastBlock", get(move || api::last_block_handler(last_block_api_state)))
        .route(
            "/toggleFail",
            post(move || api::toggle_fail_handler(toggle_fail_api_state)),
        )
        .route("/health", get(move || api::health_handler(health_api_state)))
        .layer(OtelInResponseLayer::default())
        .layer(OtelAxumLayer::default());
        // .with_state(shared_state);

    // Spawn a task to run the API
    tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(args.listen).await.unwrap();
        axum::serve(listener, app).await.unwrap();
    });

    // Create a provider
    let rpc_url = args.rpc_url.parse()?;
    let provider = ProviderBuilder::new().on_reqwest_http(rpc_url)?;

    let block_frequency_millis = args.block_frequency * 1_000;

    loop {
        AppState::poll_and_update_block(provider.clone(), app_state.clone()).await;

        let now_since_epoch = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Time went backwards")
            .as_millis() as u64;
    
        let delay_millis = block_frequency_millis - (now_since_epoch % block_frequency_millis);
        let next_tick = Instant::now() + Duration::from_millis(delay_millis) + Duration::from_secs(1);
    
        // Sleep until the next calculated multiple of block_frequency
        time::sleep_until(next_tick).await;
    }
}
