use detta_core::{Block, BlockError, DeTTaState, Transaction, ValidatorNode};
use detta_rpc::{RpcError, RpcService};
use detta_storage::{FileStorage, StorageError};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeError {
    Rpc(RpcError),
    Storage(StorageError),
    Block(BlockError),
}

pub struct PersistentValidatorNode {
    validator_id: String,
    rpc: RpcService,
    storage: FileStorage,
}

impl PersistentValidatorNode {
    pub fn bootstrap(
        validator_id: impl Into<String>,
        state: DeTTaState,
        storage_root: impl Into<PathBuf>,
    ) -> Result<Self, NodeError> {
        let validator_id = validator_id.into();
        let storage = FileStorage::open(storage_root).map_err(NodeError::Storage)?;
        storage
            .commit_snapshot(&state.snapshot())
            .map_err(NodeError::Storage)?;

        Ok(Self {
            rpc: RpcService::new(ValidatorNode::new(validator_id.clone(), state)),
            storage,
            validator_id,
        })
    }

    pub fn restart(
        validator_id: impl Into<String>,
        storage_root: impl Into<PathBuf>,
    ) -> Result<Self, NodeError> {
        let validator_id = validator_id.into();
        let storage = FileStorage::open(storage_root).map_err(NodeError::Storage)?;
        let snapshot = storage.load_snapshot().map_err(NodeError::Storage)?;
        let state = DeTTaState::from_snapshot(snapshot)
            .map_err(|error| NodeError::Storage(StorageError::InvalidSnapshot(error)))?;

        Ok(Self {
            rpc: RpcService::new(ValidatorNode::new(validator_id.clone(), state)),
            storage,
            validator_id,
        })
    }

    pub fn validator_id(&self) -> &str {
        &self.validator_id
    }

    pub fn rpc(&self) -> &RpcService {
        &self.rpc
    }

    pub fn submit_transaction(&mut self, tx: Transaction) -> Result<(), NodeError> {
        self.rpc.submit_transaction(tx).map_err(NodeError::Rpc)
    }

    pub fn produce_block(&mut self, height: u64, timestamp: u64) -> Result<Block, NodeError> {
        let block = self
            .rpc
            .produce_block(height, timestamp)
            .map_err(NodeError::Rpc)?;
        self.persist_committed_block(&block)?;
        Ok(block)
    }

    pub fn import_block(&mut self, block: &Block) -> Result<(), NodeError> {
        self.rpc.import_block(block).map_err(NodeError::Rpc)?;
        self.persist_committed_block(block)
    }

    pub fn load_block(&self, height: u64) -> Result<Block, NodeError> {
        self.storage.load_block(height).map_err(NodeError::Storage)
    }

    fn persist_committed_block(&self, block: &Block) -> Result<(), NodeError> {
        self.storage
            .commit_block(block)
            .map_err(NodeError::Storage)?;
        self.storage
            .commit_snapshot(&self.rpc.snapshot())
            .map_err(NodeError::Storage)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Argument, Method};
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("detta-node-{name}-{nonce}"))
    }

    fn seeded_state() -> DeTTaState {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_token(
                "TokenA",
                "USDC",
                vec![("Alice".into(), 100), ("Bob".into(), 50)],
            )
            .unwrap();
        state
    }

    fn transfer_tx() -> Transaction {
        Transaction {
            chain_id: "detta-local".into(),
            tx_hash: "tx1".into(),
            sender: "Alice".into(),
            nonce: 1,
            target: "TokenA".into(),
            method: Method::Transfer,
            args: vec![
                Argument::Principal("Bob".into()),
                Argument::Asset("USDC".into()),
                Argument::Amount(10),
            ],
            signature_ok: true,
            budget: 1_000_000,
        }
    }

    #[test]
    fn persistent_validator_produces_block_and_survives_restart() {
        let dir = temp_dir("restart");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        node.submit_transaction(transfer_tx()).unwrap();

        let block = node.produce_block(1, 1_000).unwrap();
        let restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();

        assert_eq!(restarted.validator_id(), "validator-1");
        assert_eq!(
            restarted.load_block(1).unwrap().block_hash(),
            block.block_hash()
        );
        assert_eq!(
            restarted.rpc().call_balance_view("TokenA", "Bob", "USDC"),
            60
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn full_node_imports_and_persists_block() {
        let validator_dir = temp_dir("validator");
        let full_node_dir = temp_dir("full-node");
        let mut validator =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &validator_dir)
                .unwrap();
        validator.submit_transaction(transfer_tx()).unwrap();
        let block = validator.produce_block(1, 1_000).unwrap();

        let mut full_node =
            PersistentValidatorNode::bootstrap("full-node-1", seeded_state(), &full_node_dir)
                .unwrap();
        full_node.import_block(&block).unwrap();
        let reloaded_full_node =
            PersistentValidatorNode::restart("full-node-1", &full_node_dir).unwrap();

        assert_eq!(
            reloaded_full_node
                .rpc()
                .call_balance_view("TokenA", "Alice", "USDC"),
            90
        );
        assert_eq!(
            reloaded_full_node
                .load_block(1)
                .unwrap()
                .header
                .global_state_root,
            block.header.global_state_root
        );

        fs::remove_dir_all(validator_dir).unwrap();
        fs::remove_dir_all(full_node_dir).unwrap();
    }
}
