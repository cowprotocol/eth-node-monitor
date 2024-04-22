use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use alloy::{
    network::Network,
    providers::Provider,
    rpc::types::eth::{Block, BlockNumberOrTag},
    transports::Transport,
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
        let http_block = provider
            .get_block_by_number(
                BlockNumberOrTag::Number(block.header.number.unwrap().to::<u64>()),
                true,
            )
            .await
            .unwrap();

        match http_block {
            Some(blk) => {
                if block.header.hash != blk.header.hash {
                    tracing::error!("Block hash mismatch between HTTP and WS providers");
                }
            }
            None => {
                tracing::error!("Block not found in HTTP provider");
            }
        }

        tracing::debug!(
            block.number = block.header.number.map(|num| num.to::<u64>()),
            block.timestamp = block.header.timestamp.to::<u64>(),
            block.hash = block.header.hash.unwrap_or_default().to_string(),
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
                let stale_duration = current_timestamp - block_timestamp;
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
