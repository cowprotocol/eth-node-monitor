use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use alloy::{
    network::Network, primitives::FixedBytes, providers::Provider, rpc::types::eth::{Block, BlockNumberOrTag}, transports::Transport
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct AppState {
    // Set as an option to allow for the initial state to be None
    pub latest_block: Option<Block>,
    // Initial state is false
    pub fail_intentional: bool,
    // Block frequency that is to be expected from the Ethereum node
    block_frequency: u64,
}

impl AppState {
    /// Create a new instance of the `AppState` struct
    pub fn new(block_frequency: u64) -> Self {
        Self {
            latest_block: None,
            fail_intentional: false,
            block_frequency,
        }
    }

    /// Poll the Ethereum node for the latest block and update the shared state
    /// with the latest block.
    #[tracing::instrument(skip_all)]
    pub(crate) async fn poll_and_update_block<
        T: Transport + Clone,
        P: Provider<T, N> + std::fmt::Debug,
        N: Network,
    >(
        block: Block,
        provider: &P,
        app_state: Arc<Mutex<AppState>>,
    ) {
        // Before updating the latest block, check to see if it is retrievable from the
        // HTTP provider. If the block is not found in the HTTP provider, log an error
        // and continue to the next block.
        // If the block is found in the HTTP provider, check to see if the block hash
        // matches between the HTTP and WS providers. If the block hash does not match,
        // log an error and continue to the next block.
        let Some(number) = block.header.number else {
            tracing::warn!("Received block header without a number");
            return;
        };
        let http_block = match provider
            .get_block_by_number(BlockNumberOrTag::Number(number.to::<u64>()), true)
            .await
        {
            Ok(http_block) => http_block,
            Err(err) => {
                tracing::warn!(error = %err, "Failed to fetch block from HTTP provider");
                return;
            }
        };

        match http_block {
            Some(ref blk) => {
                let ws_hash = block.header.hash.unwrap_or_default();
                let http_hash = blk.header.hash.unwrap_or_default();
                if ws_hash != http_hash && ws_hash != FixedBytes::ZERO {
                    tracing::error!(
                        ws_block = ?block,
                        http_block = ?http_block,
                        "Block hash mismatch between HTTP and WS providers"
                    );
                    return;
                }
            }
            None => {
                tracing::error!("Block not found in HTTP provider");
                return;
            }
        }

        tracing::debug!(
            block.number = ?block.header.number,
            block.timestamp = ?block.header.timestamp,
            block.hash = ?block.header.hash,
            "Updating to latest block"
        );
        app_state.lock().unwrap().latest_block = Some(block);
    }

    #[tracing::instrument(skip(self))]
    pub(crate) fn is_healthy(&self) -> bool {
        if self.fail_intentional {
            return false;
        }

        let block_data = &self.latest_block;
        match block_data {
            Some(block) => {
                let block_timestamp = block.header.timestamp.to::<u64>();
                let current_timestamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let stale_duration = current_timestamp.saturating_sub(block_timestamp);
                tracing::debug!(
                    blocknumber = block.header.number.map(|num| num.to::<u64>()),
                    blocktimestamp = block_timestamp,
                    currenttimestamp = current_timestamp,
                    blockfrequency = self.block_frequency,
                    calculatedstale = stale_duration,
                    "Checking health"
                );

                if stale_duration > self.block_frequency {
                    return false;
                }
                return true;
            }
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn state_with_block_at(timestamp: u64) -> AppState {
        let zero = |len: usize| format!("0x{}", "0".repeat(len));
        let block = serde_json::from_value(serde_json::json!({
            "parentHash": zero(64),
            "sha3Uncles": zero(64),
            "miner": zero(40),
            "stateRoot": zero(64),
            "transactionsRoot": zero(64),
            "receiptsRoot": zero(64),
            "logsBloom": zero(512),
            "difficulty": "0x0",
            "gasLimit": "0x0",
            "gasUsed": "0x0",
            "timestamp": format!("{timestamp:#x}"),
            "extraData": "0x",
        }))
        .unwrap();
        let mut state = AppState::new(4);
        state.latest_block = Some(block);
        state
    }

    #[test]
    fn block_timestamped_ahead_of_local_clock_is_healthy() {
        assert!(state_with_block_at(now() + 2).is_healthy());
    }

    #[test]
    fn stale_block_is_unhealthy() {
        assert!(!state_with_block_at(now() - 10).is_healthy());
    }
}
