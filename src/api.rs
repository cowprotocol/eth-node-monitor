use std::sync::{Arc, Mutex};

use alloy::rpc::types::eth::Block;
use axum::{http::StatusCode, Json};
use serde_json::{json, Value};

use crate::AppState;

/// Handler for the `/health` route
/// Checks the health of the service by comparing the timestamp of the last block
/// with the current timestamp. This endpoint will return a `SERVICE_UNAVAILABLE` status code
/// if:
/// - The difference between the current timestamp and the last block timestamp is greater than the block frequency
/// - The last block is not available
/// - The `fail_intentional` flag is set
/// Otherwise, it will return an `OK` status code.
/// This health check will in all likelihood result in some false positives, as the block frequency
/// is not guaranteed to be exact due to network latency and other factors. For this reason, when
/// monitoring a production system, it is recommended to poll the health endpoint at a frequency
/// that is less than the block frequency and require multiple consecutive failures before
/// considering the service legitimately unhealthy.
#[tracing::instrument(skip(state))]
pub async fn health_handler(
    state: Arc<Mutex<AppState>>,
) -> (StatusCode, axum::Json<serde_json::Value>) {
    let state = state.lock().unwrap();

    // Check if the service is healthy or not
    if state.is_healthy() {
        healthy_response()
    } else {
        unhealthy_response("block is stale or intentionally failing", &state.latest_block)
    }
}

/// Handler for the `/lastBlock` route
/// Return the last block data as provided by target RPC node
#[tracing::instrument]
pub async fn last_block_handler(state: Arc<Mutex<AppState>>) -> Json<Value> {
    let state = state.lock().unwrap();

    match state.latest_block.as_ref() {
        Some(block) => Json(json!({"lastBlock": block})),
        None => Json(json!({"lastBlock": null})),
    }
}

#[tracing::instrument]
/// Handler for the `/toggleFail` route
/// Sets the fail_intentional flag to the opposite of its current value
pub async fn toggle_fail_handler(state: Arc<Mutex<AppState>>) -> Json<Value> {
    let mut state = state.lock().unwrap();
    state.fail_intentional = !state.fail_intentional;

    Json(json!({"fail_intentional": state.fail_intentional}))
}

fn unhealthy_response(reason: &str, block: &Option<Block>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(json!({"status": "unhealthy", "reason": reason, "lastBlock": block})),
    )
}

fn healthy_response() -> (StatusCode, Json<Value>) {
    (StatusCode::OK, Json(json!({"status": "healthy"})))
}
