use detta_consensus::{FinalityCertificate, Vote};
use detta_core::{
    Block, BlockError, ChainId, DeTTaState, MempoolError, Transaction, ValidatorNode,
};
use detta_network::{Envelope, InMemoryTransport, NetworkError, NetworkMessage};
use detta_protocol::{
    build_snapshot_chunks, SignatureError, SignedValidatorMessage, SnapshotChunkRequest,
    SnapshotSyncError, ValidatorPublicKey, ValidatorSigningKey,
};
use detta_rpc::{RpcError, RpcService};
use detta_storage::{FileStorage, StorageError};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub const DEFAULT_NODE_NETWORK_ID: &str = "detta-localnet";

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeError {
    Rpc(RpcError),
    Storage(StorageError),
    Block(BlockError),
    Mempool(MempoolError),
    Network(NetworkError),
    ValidatorKeyNotFound(String),
    Signature(SignatureError),
    SnapshotSync(SnapshotSyncError),
    SnapshotRootNotFound {
        requested: String,
        available: String,
    },
    SigningKeyMismatch {
        expected: String,
        actual: String,
    },
    BlockProposerMismatch {
        expected: String,
        actual: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkIngestOutcome {
    TransactionAccepted,
    BlockImported,
    VoteReceived,
    FinalityCertificateReceived,
    IgnoredControlMessage,
}

pub struct PersistentValidatorNode {
    validator_id: String,
    network_id: String,
    chain_id: ChainId,
    validator_keys: BTreeMap<String, ValidatorPublicKey>,
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
        let chain_id = state.chain_id().clone();
        storage
            .commit_snapshot(&state.snapshot())
            .map_err(NodeError::Storage)?;
        storage.commit_mempool(&[]).map_err(NodeError::Storage)?;

        Ok(Self {
            rpc: RpcService::new(ValidatorNode::new(validator_id.clone(), state)),
            storage,
            validator_id,
            network_id: DEFAULT_NODE_NETWORK_ID.into(),
            chain_id,
            validator_keys: BTreeMap::new(),
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
        let chain_id = state.chain_id().clone();
        let pending = storage.load_mempool().map_err(NodeError::Storage)?;
        let node = ValidatorNode::with_pending_transactions(validator_id.clone(), state, pending)
            .map_err(NodeError::Mempool)?;

        Ok(Self {
            rpc: RpcService::new(node),
            storage,
            validator_id,
            network_id: DEFAULT_NODE_NETWORK_ID.into(),
            chain_id,
            validator_keys: BTreeMap::new(),
        })
    }

    pub fn validator_id(&self) -> &str {
        &self.validator_id
    }

    pub fn rpc(&self) -> &RpcService {
        &self.rpc
    }

    pub fn network_id(&self) -> &str {
        &self.network_id
    }

    pub fn chain_id(&self) -> &ChainId {
        &self.chain_id
    }

    pub fn set_network_id(&mut self, network_id: impl Into<String>) {
        self.network_id = network_id.into();
    }

    pub fn trust_validator_key(&mut self, public_key: ValidatorPublicKey) {
        self.validator_keys
            .insert(public_key.validator_id.clone(), public_key);
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

    pub fn gossip_vote(
        &self,
        vote: Vote,
        transport: &mut InMemoryTransport,
    ) -> Result<usize, NodeError> {
        transport
            .broadcast(self.validator_id.clone(), NetworkMessage::Vote(vote))
            .map_err(NodeError::Network)
    }

    pub fn gossip_finality_certificate(
        &self,
        certificate: FinalityCertificate,
        transport: &mut InMemoryTransport,
    ) -> Result<usize, NodeError> {
        transport
            .broadcast(
                self.validator_id.clone(),
                NetworkMessage::FinalityCertificate(certificate),
            )
            .map_err(NodeError::Network)
    }

    pub fn sign_validator_message(
        &self,
        signing_key: &ValidatorSigningKey,
        message: NetworkMessage,
    ) -> Result<NetworkMessage, NodeError> {
        if signing_key.validator_id() != self.validator_id {
            return Err(NodeError::SigningKeyMismatch {
                expected: self.validator_id.clone(),
                actual: signing_key.validator_id().to_string(),
            });
        }
        let signed = signing_key
            .sign_message(&self.network_id, self.chain_id.clone(), message)
            .map_err(NodeError::Signature)?;
        Ok(NetworkMessage::SignedValidator(Box::new(signed)))
    }

    pub fn gossip_signed_block_proposal(
        &self,
        block: Block,
        signing_key: &ValidatorSigningKey,
        transport: &mut InMemoryTransport,
    ) -> Result<usize, NodeError> {
        if block.header.proposer != self.validator_id {
            return Err(NodeError::BlockProposerMismatch {
                expected: self.validator_id.clone(),
                actual: block.header.proposer,
            });
        }
        let signed =
            self.sign_validator_message(signing_key, NetworkMessage::Block(Box::new(block)))?;
        transport
            .broadcast(self.validator_id.clone(), signed)
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
            NetworkMessage::Vote(_) => Ok(NetworkIngestOutcome::VoteReceived),
            NetworkMessage::FinalityCertificate(_) => {
                Ok(NetworkIngestOutcome::FinalityCertificateReceived)
            }
            NetworkMessage::SignedValidator(signed) => {
                self.verify_signed_validator_message(signed)?;
                let signed_envelope = Envelope {
                    from: envelope.from.clone(),
                    to: envelope.to.clone(),
                    message: signed.message.as_ref().clone(),
                };
                self.ingest_network_envelope(&signed_envelope)
            }
            NetworkMessage::ValidatorSetUpdate(_)
            | NetworkMessage::EquivocationEvidence(_)
            | NetworkMessage::StateSnapshot(_)
            | NetworkMessage::PeerHello(_)
            | NetworkMessage::SnapshotChunkRequest(_)
            | NetworkMessage::SnapshotChunkManifest(_)
            | NetworkMessage::SnapshotChunk(_) => Ok(NetworkIngestOutcome::IgnoredControlMessage),
        }
    }

    pub fn load_block(&self, height: u64) -> Result<Block, NodeError> {
        self.storage.load_block(height).map_err(NodeError::Storage)
    }

    pub fn serve_snapshot_chunk_request(
        &self,
        request: &SnapshotChunkRequest,
        max_chunk_bytes: usize,
    ) -> Result<Vec<NetworkMessage>, NodeError> {
        let snapshot = self.storage.load_snapshot().map_err(NodeError::Storage)?;
        if request.snapshot_root != snapshot.global_state_root {
            return Err(NodeError::SnapshotRootNotFound {
                requested: request.snapshot_root.clone(),
                available: snapshot.global_state_root,
            });
        }

        let chunk_set =
            build_snapshot_chunks(&snapshot, max_chunk_bytes).map_err(NodeError::SnapshotSync)?;
        let start = request.start_index as usize;
        let limit = request.max_chunks as usize;
        let mut messages = vec![NetworkMessage::SnapshotChunkManifest(
            chunk_set.manifest.clone(),
        )];
        messages.extend(
            chunk_set
                .chunks
                .into_iter()
                .skip(start)
                .take(limit)
                .map(NetworkMessage::SnapshotChunk),
        );
        Ok(messages)
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

    fn verify_signed_validator_message(
        &self,
        signed: &SignedValidatorMessage,
    ) -> Result<(), NodeError> {
        let public_key = self
            .validator_keys
            .get(&signed.signer)
            .ok_or_else(|| NodeError::ValidatorKeyNotFound(signed.signer.clone()))?;
        signed
            .verify(&self.network_id, &self.chain_id, public_key)
            .map_err(NodeError::Signature)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Argument, Method};
    use detta_network::{InMemoryTransport, TcpProtocolStream};
    use detta_protocol::{
        ProtocolMessage, SignatureError, SnapshotChunkRequest, SnapshotChunkSet,
        ValidatorSigningKey,
    };
    use std::fs;
    use std::net::TcpListener;
    use std::thread;
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

    fn validator_key(validator_id: &str, seed_byte: u8) -> ValidatorSigningKey {
        ValidatorSigningKey::from_seed(validator_id, "consensus-key-1", [seed_byte; 32])
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
    fn persistent_node_gossips_consensus_vote_and_certificate() {
        let validator_dir = temp_dir("gossip-consensus-validator");
        let peer_dir = temp_dir("gossip-consensus-peer");
        let validator =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &validator_dir)
                .unwrap();
        let mut peer =
            PersistentValidatorNode::bootstrap("validator-2", seeded_state(), &peer_dir).unwrap();
        let mut transport =
            InMemoryTransport::new(["validator-1".into(), "validator-2".into()]).unwrap();

        let vote = Vote {
            validator_id: "validator-1".into(),
            height: 1,
            block_hash: "block-a".into(),
        };
        let certificate = FinalityCertificate {
            height: 1,
            block_hash: "block-a".into(),
            signers: vec!["validator-1".into(), "validator-2".into()],
        };

        assert_eq!(
            validator.gossip_vote(vote.clone(), &mut transport).unwrap(),
            1
        );
        assert_eq!(
            validator
                .gossip_finality_certificate(certificate.clone(), &mut transport)
                .unwrap(),
            1
        );

        let envelopes = transport.drain_peer("validator-2").unwrap();
        assert_eq!(envelopes[0].message, NetworkMessage::Vote(vote));
        assert_eq!(
            peer.ingest_network_envelope(&envelopes[0]).unwrap(),
            NetworkIngestOutcome::VoteReceived
        );
        assert_eq!(
            envelopes[1].message,
            NetworkMessage::FinalityCertificate(certificate)
        );
        assert_eq!(
            peer.ingest_network_envelope(&envelopes[1]).unwrap(),
            NetworkIngestOutcome::FinalityCertificateReceived
        );

        fs::remove_dir_all(validator_dir).unwrap();
        fs::remove_dir_all(peer_dir).unwrap();
    }

    #[test]
    fn persistent_node_gossips_signed_block_proposal_to_peer() {
        let proposer_dir = temp_dir("signed-proposal");
        let peer_dir = temp_dir("signed-proposal-peer");
        let proposer_key = validator_key("validator-1", 7);
        let mut proposer =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &proposer_dir)
                .unwrap();
        proposer.set_network_id("detta-testnet");
        proposer.submit_transaction(transfer_tx()).unwrap();
        let block = proposer.produce_block(1, 1_000).unwrap();

        let mut peer =
            PersistentValidatorNode::bootstrap("validator-2", seeded_state(), &peer_dir).unwrap();
        peer.set_network_id("detta-testnet");
        peer.trust_validator_key(proposer_key.public_key());
        let mut transport =
            InMemoryTransport::new(["validator-1".into(), "validator-2".into()]).unwrap();

        assert_eq!(
            proposer
                .gossip_signed_block_proposal(block.clone(), &proposer_key, &mut transport)
                .unwrap(),
            1
        );
        let envelope = transport.drain_peer("validator-2").unwrap().pop().unwrap();

        assert!(matches!(
            envelope.message,
            NetworkMessage::SignedValidator(_)
        ));
        assert_eq!(
            peer.ingest_network_envelope(&envelope).unwrap(),
            NetworkIngestOutcome::BlockImported
        );
        assert_eq!(peer.load_block(1).unwrap().block_hash(), block.block_hash());

        fs::remove_dir_all(proposer_dir).unwrap();
        fs::remove_dir_all(peer_dir).unwrap();
    }

    #[test]
    fn tcp_signed_block_proposal_round_trips_with_signed_vote() {
        let proposer_dir = temp_dir("tcp-signed-proposer");
        let peer_dir = temp_dir("tcp-signed-peer");
        let proposer_key = validator_key("validator-1", 7);
        let peer_key = validator_key("validator-2", 8);
        let mut proposer =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &proposer_dir)
                .unwrap();
        proposer.set_network_id("detta-testnet");
        proposer.trust_validator_key(peer_key.public_key());
        proposer.submit_transaction(transfer_tx()).unwrap();
        let block = proposer.produce_block(1, 1_000).unwrap();
        let block_hash = block.block_hash();
        let signed_block = proposer
            .sign_validator_message(
                &proposer_key,
                NetworkMessage::Block(Box::new(block.clone())),
            )
            .unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server_block = block.clone();
        let server_proposer_key = proposer_key.public_key();
        let server = thread::spawn(move || {
            let mut peer =
                PersistentValidatorNode::bootstrap("validator-2", seeded_state(), &peer_dir)
                    .unwrap();
            peer.set_network_id("detta-testnet");
            peer.trust_validator_key(server_proposer_key);

            let (stream, _) = listener.accept().unwrap();
            let mut tcp = TcpProtocolStream::from_stream(stream);
            let message = tcp.receive().unwrap();
            let envelope = Envelope {
                from: "validator-1".into(),
                to: "validator-2".into(),
                message,
            };

            assert_eq!(
                peer.ingest_network_envelope(&envelope).unwrap(),
                NetworkIngestOutcome::BlockImported
            );

            let vote = Vote {
                validator_id: "validator-2".into(),
                height: server_block.header.height,
                block_hash: server_block.block_hash(),
            };
            let signed_vote = peer
                .sign_validator_message(&peer_key, NetworkMessage::Vote(vote))
                .unwrap();
            tcp.send(&signed_vote).unwrap();
            fs::remove_dir_all(peer_dir).unwrap();
        });

        let mut tcp = TcpProtocolStream::connect(addr).unwrap();
        tcp.send(&signed_block).unwrap();
        let response = tcp.receive().unwrap();
        let response_envelope = Envelope {
            from: "validator-2".into(),
            to: "validator-1".into(),
            message: response,
        };

        assert_eq!(
            proposer
                .ingest_network_envelope(&response_envelope)
                .unwrap(),
            NetworkIngestOutcome::VoteReceived
        );
        server.join().unwrap();
        assert_eq!(proposer.load_block(1).unwrap().block_hash(), block_hash);

        fs::remove_dir_all(proposer_dir).unwrap();
    }

    #[test]
    fn persistent_node_verifies_signed_validator_envelope_before_ingest() {
        let peer_dir = temp_dir("signed-peer");
        let key = validator_key("validator-1", 7);
        let mut peer =
            PersistentValidatorNode::bootstrap("validator-2", seeded_state(), &peer_dir).unwrap();
        peer.set_network_id("detta-testnet");
        peer.trust_validator_key(key.public_key());

        let vote = Vote {
            validator_id: "validator-1".into(),
            height: 1,
            block_hash: "block-a".into(),
        };
        let signed = key
            .sign_message(
                "detta-testnet",
                peer.chain_id().clone(),
                ProtocolMessage::Vote(vote),
            )
            .unwrap();
        let envelope = Envelope {
            from: "validator-1".into(),
            to: "validator-2".into(),
            message: NetworkMessage::SignedValidator(Box::new(signed)),
        };

        assert_eq!(
            peer.ingest_network_envelope(&envelope).unwrap(),
            NetworkIngestOutcome::VoteReceived
        );

        fs::remove_dir_all(peer_dir).unwrap();
    }

    #[test]
    fn persistent_node_rejects_untrusted_or_wrong_domain_signed_envelopes() {
        let peer_dir = temp_dir("signed-peer-rejects");
        let key = validator_key("validator-1", 7);
        let mut peer =
            PersistentValidatorNode::bootstrap("validator-2", seeded_state(), &peer_dir).unwrap();
        peer.set_network_id("detta-testnet");

        let vote = Vote {
            validator_id: "validator-1".into(),
            height: 1,
            block_hash: "block-a".into(),
        };
        let signed = key
            .sign_message(
                "detta-testnet",
                peer.chain_id().clone(),
                ProtocolMessage::Vote(vote.clone()),
            )
            .unwrap();
        let envelope = Envelope {
            from: "validator-1".into(),
            to: "validator-2".into(),
            message: NetworkMessage::SignedValidator(Box::new(signed)),
        };

        assert_eq!(
            peer.ingest_network_envelope(&envelope).unwrap_err(),
            NodeError::ValidatorKeyNotFound("validator-1".into())
        );

        peer.trust_validator_key(key.public_key());
        let wrong_network = key
            .sign_message(
                "wrong-net",
                peer.chain_id().clone(),
                ProtocolMessage::Vote(vote),
            )
            .unwrap();
        let wrong_network_envelope = Envelope {
            from: "validator-1".into(),
            to: "validator-2".into(),
            message: NetworkMessage::SignedValidator(Box::new(wrong_network)),
        };

        assert_eq!(
            peer.ingest_network_envelope(&wrong_network_envelope)
                .unwrap_err(),
            NodeError::Signature(SignatureError::NetworkMismatch {
                expected: "detta-testnet".into(),
                actual: "wrong-net".into(),
            })
        );

        fs::remove_dir_all(peer_dir).unwrap();
    }

    #[test]
    fn persistent_node_serves_snapshot_chunks_from_storage() {
        let node_dir = temp_dir("state-sync-source");
        let node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &node_dir).unwrap();
        let snapshot_root = node.rpc().get_state_root();
        let mut all_chunks = Vec::new();
        let mut manifest = None;
        let mut start_index = 0;

        loop {
            let response = node
                .serve_snapshot_chunk_request(
                    &SnapshotChunkRequest {
                        snapshot_root: snapshot_root.clone(),
                        start_index,
                        max_chunks: 2,
                    },
                    64,
                )
                .unwrap();

            match &response[0] {
                NetworkMessage::SnapshotChunkManifest(next_manifest) => {
                    if let Some(manifest) = &manifest {
                        assert_eq!(manifest, next_manifest);
                    } else {
                        manifest = Some(next_manifest.clone());
                    }
                }
                message => panic!("expected snapshot manifest, got {message:?}"),
            }

            for message in response.into_iter().skip(1) {
                match message {
                    NetworkMessage::SnapshotChunk(chunk) => all_chunks.push(chunk),
                    message => panic!("expected snapshot chunk, got {message:?}"),
                }
            }

            let manifest_ref = manifest.as_ref().unwrap();
            if all_chunks.len() >= manifest_ref.chunk_count as usize {
                break;
            }
            start_index += 2;
        }

        let chunk_set = SnapshotChunkSet {
            manifest: manifest.unwrap(),
            chunks: all_chunks,
        };
        let reconstructed = chunk_set.reconstruct_snapshot().unwrap();
        assert_eq!(reconstructed.global_state_root, snapshot_root);

        fs::remove_dir_all(node_dir).unwrap();
    }

    #[test]
    fn persistent_node_rejects_unknown_snapshot_root_request() {
        let node_dir = temp_dir("state-sync-missing-root");
        let node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &node_dir).unwrap();

        let error = node
            .serve_snapshot_chunk_request(
                &SnapshotChunkRequest {
                    snapshot_root: "missing-root".into(),
                    start_index: 0,
                    max_chunks: 1,
                },
                64,
            )
            .unwrap_err();

        assert_eq!(
            error,
            NodeError::SnapshotRootNotFound {
                requested: "missing-root".into(),
                available: node.rpc().get_state_root(),
            }
        );

        fs::remove_dir_all(node_dir).unwrap();
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
