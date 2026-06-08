use detta_core::{Block, BlockError, DeTTaState, MempoolError, Transaction, ValidatorNode};
use detta_network::{Envelope, InMemoryTransport, NetworkError, NetworkMessage};
use detta_rpc::{RpcError, RpcService};
use detta_storage::{FileStorage, StorageError};
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeError {
    Rpc(RpcError),
    Storage(StorageError),
    Block(BlockError),
    Mempool(MempoolError),
    Network(NetworkError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkIngestOutcome {
    TransactionAccepted,
    BlockImported,
    IgnoredControlMessage,
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
        storage.commit_mempool(&[]).map_err(NodeError::Storage)?;

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
        let pending = storage.load_mempool().map_err(NodeError::Storage)?;
        let node = ValidatorNode::with_pending_transactions(validator_id.clone(), state, pending)
            .map_err(NodeError::Mempool)?;

        Ok(Self {
            rpc: RpcService::new(node),
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

    pub fn pending_len(&self) -> usize {
        self.rpc.node().pending_len()
    }

    pub fn submit_transaction(&mut self, tx: Transaction) -> Result<(), NodeError> {
        self.rpc.submit_transaction(tx).map_err(NodeError::Rpc)?;
        self.persist_mempool()
    }

    pub fn submit_and_gossip_transaction(
        &mut self,
        tx: Transaction,
        transport: &mut InMemoryTransport,
    ) -> Result<usize, NodeError> {
        self.submit_transaction(tx.clone())?;
        transport
            .broadcast(self.validator_id.clone(), NetworkMessage::Transaction(tx))
            .map_err(NodeError::Network)
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

    pub fn ingest_network_envelope(
        &mut self,
        envelope: &Envelope,
    ) -> Result<NetworkIngestOutcome, NodeError> {
        match &envelope.message {
            NetworkMessage::Transaction(tx) => {
                self.submit_transaction(tx.clone())?;
                Ok(NetworkIngestOutcome::TransactionAccepted)
            }
            NetworkMessage::Block(block) => {
                self.import_block(block)?;
                Ok(NetworkIngestOutcome::BlockImported)
            }
            NetworkMessage::Vote(_)
            | NetworkMessage::FinalityCertificate(_)
            | NetworkMessage::ValidatorSetUpdate(_)
            | NetworkMessage::EquivocationEvidence(_)
            | NetworkMessage::StateSnapshot(_)
            | NetworkMessage::PeerHello(_) => Ok(NetworkIngestOutcome::IgnoredControlMessage),
        }
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
        self.persist_mempool()?;
        Ok(())
    }

    fn persist_mempool(&self) -> Result<(), NodeError> {
        self.storage
            .commit_mempool(self.rpc.node().pending_transactions())
            .map_err(NodeError::Storage)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Argument, Method};
    use detta_network::InMemoryTransport;
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

    fn seeded_defi_state() -> DeTTaState {
        let mut state = seeded_state();
        state.deploy_amm_pool("PoolA", "USDC", "ETH").unwrap();
        state
    }

    fn transfer_tx() -> Transaction {
        tx_to(
            "TokenA",
            "tx1",
            "Alice",
            1,
            Method::Transfer,
            vec![
                Argument::Principal("Bob".into()),
                Argument::Asset("USDC".into()),
                Argument::Amount(10),
            ],
        )
    }

    fn tx_to(
        target: &str,
        tx_hash: &str,
        sender: &str,
        nonce: u64,
        method: Method,
        args: Vec<Argument>,
    ) -> Transaction {
        Transaction {
            chain_id: "detta-local".into(),
            tx_hash: tx_hash.into(),
            sender: sender.into(),
            nonce,
            target: target.into(),
            method,
            args,
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
    fn persistent_validator_restores_pending_mempool_after_restart() {
        let dir = temp_dir("mempool-restart");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();

        node.submit_transaction(transfer_tx()).unwrap();
        assert_eq!(node.pending_len(), 1);

        let mut restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        assert_eq!(restarted.pending_len(), 1);

        let block = restarted.produce_block(1, 1_000).unwrap();
        assert_eq!(block.transactions.len(), 1);
        assert_eq!(restarted.pending_len(), 0);

        let reloaded = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        assert_eq!(reloaded.pending_len(), 0);
        assert_eq!(
            reloaded.rpc().call_balance_view("TokenA", "Bob", "USDC"),
            60
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_ingests_gossiped_transaction_envelope() {
        let dir = temp_dir("network-tx");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        let mut transport =
            InMemoryTransport::new(["client-1".into(), "validator-1".into()]).unwrap();

        transport
            .send(
                "client-1",
                "validator-1",
                NetworkMessage::Transaction(transfer_tx()),
            )
            .unwrap();
        let envelope = transport.drain_peer("validator-1").unwrap().pop().unwrap();
        let outcome = node.ingest_network_envelope(&envelope).unwrap();

        assert_eq!(outcome, NetworkIngestOutcome::TransactionAccepted);
        assert_eq!(node.pending_len(), 1);
        let reloaded = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        assert_eq!(reloaded.pending_len(), 1);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_gossips_admitted_transaction_to_peer() {
        let validator_dir = temp_dir("gossip-validator");
        let peer_dir = temp_dir("gossip-peer");
        let mut validator =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &validator_dir)
                .unwrap();
        let mut peer =
            PersistentValidatorNode::bootstrap("validator-2", seeded_state(), &peer_dir).unwrap();
        let mut transport =
            InMemoryTransport::new(["validator-1".into(), "validator-2".into()]).unwrap();

        let sent = validator
            .submit_and_gossip_transaction(transfer_tx(), &mut transport)
            .unwrap();

        assert_eq!(sent, 1);
        assert_eq!(validator.pending_len(), 1);
        let envelope = transport.drain_peer("validator-2").unwrap().pop().unwrap();
        assert_eq!(
            peer.ingest_network_envelope(&envelope).unwrap(),
            NetworkIngestOutcome::TransactionAccepted
        );
        assert_eq!(peer.pending_len(), 1);
        let reloaded_peer = PersistentValidatorNode::restart("validator-2", &peer_dir).unwrap();
        assert_eq!(reloaded_peer.pending_len(), 1);

        fs::remove_dir_all(validator_dir).unwrap();
        fs::remove_dir_all(peer_dir).unwrap();
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

    #[test]
    fn persistent_node_ingests_block_envelope() {
        let validator_dir = temp_dir("network-block-validator");
        let full_node_dir = temp_dir("network-block-full-node");
        let mut validator =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &validator_dir)
                .unwrap();
        validator.submit_transaction(transfer_tx()).unwrap();
        let block = validator.produce_block(1, 1_000).unwrap();
        let mut full_node =
            PersistentValidatorNode::bootstrap("full-node-1", seeded_state(), &full_node_dir)
                .unwrap();
        let mut transport =
            InMemoryTransport::new(["validator-1".into(), "full-node-1".into()]).unwrap();

        transport
            .send(
                "validator-1",
                "full-node-1",
                NetworkMessage::Block(Box::new(block.clone())),
            )
            .unwrap();
        let envelope = transport.drain_peer("full-node-1").unwrap().pop().unwrap();
        let outcome = full_node.ingest_network_envelope(&envelope).unwrap();

        assert_eq!(outcome, NetworkIngestOutcome::BlockImported);
        assert_eq!(
            full_node.load_block(1).unwrap().header.global_state_root,
            block.header.global_state_root
        );
        let reloaded = PersistentValidatorNode::restart("full-node-1", &full_node_dir).unwrap();
        assert_eq!(
            reloaded.rpc().call_balance_view("TokenA", "Bob", "USDC"),
            60
        );

        fs::remove_dir_all(validator_dir).unwrap();
        fs::remove_dir_all(full_node_dir).unwrap();
    }

    #[test]
    fn three_independent_nodes_replay_token_and_amm_blocks() {
        let validator_dir = temp_dir("mvp-validator");
        let full_node_dirs = [
            temp_dir("mvp-full-node-1"),
            temp_dir("mvp-full-node-2"),
            temp_dir("mvp-full-node-3"),
        ];
        let mut validator =
            PersistentValidatorNode::bootstrap("validator-1", seeded_defi_state(), &validator_dir)
                .unwrap();

        validator.submit_transaction(transfer_tx()).unwrap();
        validator
            .submit_transaction(tx_to(
                "PoolA",
                "tx2",
                "Alice",
                2,
                Method::AddLiquidity,
                vec![Argument::Amount(100), Argument::Amount(50)],
            ))
            .unwrap();
        let block_1 = validator.produce_block(1, 1_000).unwrap();

        validator
            .submit_transaction(tx_to(
                "PoolA",
                "tx3",
                "Bob",
                1,
                Method::Swap,
                vec![
                    Argument::Asset("USDC".into()),
                    Argument::Amount(10),
                    Argument::Amount(4),
                ],
            ))
            .unwrap();
        let block_2 = validator.produce_block(2, 2_000).unwrap();
        let expected_root = block_2.header.global_state_root.clone();

        for (index, dir) in full_node_dirs.iter().enumerate() {
            let mut full_node = PersistentValidatorNode::bootstrap(
                format!("full-node-{}", index + 1),
                seeded_defi_state(),
                dir,
            )
            .unwrap();
            full_node.import_block(&block_1).unwrap();
            full_node.import_block(&block_2).unwrap();
        }

        let replayed_roots: Vec<_> = full_node_dirs
            .iter()
            .enumerate()
            .map(|(index, dir)| {
                PersistentValidatorNode::restart(format!("full-node-{}", index + 1), dir)
                    .unwrap()
                    .rpc()
                    .get_state_root()
            })
            .collect();

        assert_eq!(replayed_roots, vec![expected_root.clone(); 3]);

        let reloaded = PersistentValidatorNode::restart("full-node-1", &full_node_dirs[0]).unwrap();
        let state = reloaded.rpc().node().state();
        assert_eq!(state.balance("TokenA", "Bob", "USDC"), 60);
        assert_eq!(state.reserve("PoolA", "USDC"), 110);
        assert_eq!(state.reserve("PoolA", "ETH"), 46);
        assert_eq!(state.lp_supply("PoolA"), 150);
        assert_eq!(state.lp_balance("PoolA", "Alice"), 150);

        fs::remove_dir_all(validator_dir).unwrap();
        for dir in full_node_dirs {
            fs::remove_dir_all(dir).unwrap();
        }
    }
}
