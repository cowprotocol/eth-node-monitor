mod api;
mod instrument;
mod monitor;

use alloy::{
    providers::{Provider, ProviderBuilder},
    rpc::client::WsConnect,
};
use axum::{
    routing::{get, post},
    Router,
};
use axum_tracing_opentelemetry::middleware::{OtelAxumLayer, OtelInResponseLayer};
use clap::Parser;
use eyre::Result;
use futures_util::StreamExt;
use monitor::AppState;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::signal::unix::{signal, SignalKind};
use url::Url;

const MIN_RECONNECT_DELAY: Duration = Duration::from_secs(1);
const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// Monitor an Ethereum node RPC endpoint
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct MonitorArgs {
    /// Listen address for the API
    #[clap(long, default_value = "127.0.0.1:8080")]
    listen: String,
    /// HTTP URL of the Ethereum node
    #[clap(long, value_parser=parse_url, default_value = "http://localhost:8545")]
    http_rpc_url: Url,
    /// Websockets URL of the Ethereum node
    #[clap(long, value_parser=parse_url, default_value = "ws://localhost:8546")]
    ws_rpc_url: Url,
    /// Block frequency that is to be expected from the Ethereum node
    #[clap(long, default_value = "12")]
    block_frequency: u64,
    /// Enable OpenTelemetry tracing
    #[clap(long, default_value = "false")]
    tracing: bool,
}

fn parse_url(s: &str) -> Result<Url, url::ParseError> {
    Url::parse(s)
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args = MonitorArgs::parse();

    // If the scheme is not http or https, return an error
    if args.http_rpc_url.scheme() != "http" && args.http_rpc_url.scheme() != "https" {
        return Err(eyre::eyre!(
            "Invalid scheme for RPC URL: {}",
            args.http_rpc_url.scheme()
        ));
    }

    // If the scheme is not ws or wss, return an error
    if args.ws_rpc_url.scheme() != "ws" && args.ws_rpc_url.scheme() != "wss" {
        return Err(eyre::eyre!(
            "Invalid scheme for RPC URL: {}",
            args.ws_rpc_url.scheme()
        ));
    }

    // Initialize tracing
    instrument::init(args.tracing);

    tracing::info!(
        http_rpc_url = args.http_rpc_url.to_string(),
        ws_rpc_url = args.ws_rpc_url.to_string(),
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
        .route(
            "/lastBlock",
            get(move || api::last_block_handler(last_block_api_state)),
        )
        .route(
            "/toggleFail",
            post(move || api::toggle_fail_handler(toggle_fail_api_state)),
        )
        .route(
            "/health",
            get(move || api::health_handler(health_api_state)),
        )
        .layer(OtelInResponseLayer::default())
        .layer(OtelAxumLayer::default());

    // Spawn a task to run the API
    let api = tokio::spawn(async move {
        let listener = tokio::net::TcpListener::bind(args.listen).await?;
        axum::serve(listener, app).await?;
        Ok::<_, eyre::Report>(())
    });

    // Create providers
    let http_provider = ProviderBuilder::new().on_reqwest_http(args.http_rpc_url)?;
    let ws = WsConnect::new(args.ws_rpc_url);

    // Follow new blocks, reconnecting whenever the node is unreachable or the
    // subscription ends (e.g. while the node restarts). Exiting instead would
    // make the container crash-loop whenever the node is down, while `/health`
    // already reports the node as unhealthy once the last block goes stale.
    let follow_blocks = async {
        let mut reconnect_delay = MIN_RECONNECT_DELAY;
        loop {
            let result = async {
                let (_ws_provider, sub) = tokio::time::timeout(CONNECT_TIMEOUT, async {
                    let ws_provider = ProviderBuilder::new().on_ws(ws.clone()).await?;
                    let sub = ws_provider.subscribe_blocks().await?;
                    Ok::<_, eyre::Report>((ws_provider, sub))
                })
                .await??;
                tracing::info!("Subscribed to new blocks");

                let mut stream = sub.into_stream();
                while let Some(block) = stream.next().await {
                    reconnect_delay = MIN_RECONNECT_DELAY;
                    monitor::AppState::poll_and_update_block(
                        block,
                        &http_provider,
                        app_state.clone(),
                    )
                    .await;
                }
                Ok::<_, eyre::Report>(())
            }
            .await;

            match result {
                Ok(()) => tracing::warn!("Block subscription ended"),
                Err(err) => tracing::warn!(error = %err, "Failed to subscribe to new blocks"),
            }
            tracing::info!(delay = ?reconnect_delay, "Reconnecting");
            tokio::time::sleep(reconnect_delay).await;
            reconnect_delay = (reconnect_delay * 2).min(MAX_RECONNECT_DELAY);
        }
    };

    let mut sigterm = signal(SignalKind::terminate())?;
    tokio::select! {
        _ = follow_blocks => Ok(()),
        res = api => match res {
            Ok(Ok(())) => Err(eyre::eyre!("API server stopped")),
            Ok(Err(err)) => Err(err.wrap_err("API server failed")),
            Err(err) => Err(eyre::Report::new(err).wrap_err("API server panicked")),
        },
        _ = sigterm.recv() => {
            tracing::info!("Received SIGTERM, shutting down");
            Ok(())
        }
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("Received SIGINT, shutting down");
            Ok(())
        }
    }
}
