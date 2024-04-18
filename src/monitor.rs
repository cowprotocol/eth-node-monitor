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
    /// Set as an option to allow for the initial state to be None
    pub latest_block: Option<Block>,
    /// Initial state is false
    pub fail_intentional: bool,
    /// Block frequency that is to be expected from the Ethereum node
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
    #[tracing::instrument(skip(app_state))]
    pub(crate) async fn poll_and_update_block<
        T: Transport + Clone,
        P: Provider<T, N> + std::fmt::Debug,
        N: Network,
    >(
        provider: P,
        app_state: Arc<Mutex<AppState>>,
    ) {
        let block = provider
            .get_block_by_number(BlockNumberOrTag::Latest, true)
            .await;
        let mut state = app_state.lock().unwrap();
        match block {
            Ok(Some(block)) => {
                state.latest_block = Some(block);
                let block = state.latest_block.as_ref().unwrap();
                tracing::debug!(
                    block.number = block.header.number.map(|num| num.to::<u64>()),
                    block.timestamp = block.header.timestamp.to::<u64>(),
                    block.hash = block.header.hash.unwrap_or_default().to_string(),
                    "Updated latest block"
                );
            }
            Ok(None) => tracing::error!("No block found"),
            Err(e) => tracing::error!("Failed to get latest block: {:?}", e),
        }
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
