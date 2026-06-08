use detta_consensus::{
    quorum_for, ConsensusCluster, ConsensusError, EquivocationEvidence, FinalityCertificate,
    SlashingRecord, Vote,
};
use detta_core::{
    Block, BlockError, ChainId, DeTTaState, MempoolError, StateSnapshot, Transaction, ValidatorNode,
};
use detta_network::{Envelope, InMemoryTransport, NetworkError, NetworkMessage, TcpProtocolStream};
use detta_protocol::{
    build_snapshot_chunks_with_metadata_roots, ProtocolMessageKind, SignatureError,
    SignedValidatorMessage, SnapshotChunkManifest, SnapshotChunkRequest, SnapshotChunkSet,
    SnapshotSyncError, ValidatorPublicKey, ValidatorSetMetadata, ValidatorSetMetadataUpdate,
    ValidatorSigningKey, SNAPSHOT_METADATA_REQUIRED_METADATA_ROOTS_ROOT,
    SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_ROOT, SNAPSHOT_METADATA_STATE_SYNC_CLIENT_METRICS_ROOT,
    SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT,
};
use detta_rpc::{
    json_rpc_response_for_request, JsonRpcHandler, PersistentNodeSnapshotRoots,
    RequiredSnapshotMetadataRootsReport, RpcError, RpcErrorBody, RpcRequest, RpcResponse,
    RpcResult, RpcService, RpcTransportError, SnapshotMetadataRootStatus,
    SnapshotSyncClientMetricsReport, ValidatorSetMetadataUpdateStatus,
};
use detta_storage::{
    FileStorage, SnapshotImportAuditRecord, StorageError, ValidatorSetMetadataAuditOutcome,
    ValidatorSetMetadataAuditRecord,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub const DEFAULT_NODE_NETWORK_ID: &str = "detta-localnet";
pub const DEFAULT_MAX_PENDING_VALIDATOR_SET_METADATA_UPDATES: usize = 128;
pub const DEFAULT_MAX_PENDING_VALIDATOR_SET_METADATA_AUTHORIZATIONS_PER_VALIDATOR: usize = 32;
pub const DEFAULT_MAX_VALIDATOR_SET_METADATA_AUDIT_RECORDS: usize = 4_096;
pub const DEFAULT_MAX_VALIDATOR_SET_METADATA_AUDIT_PAGE_SIZE: usize = 100;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NodeError {
    Rpc(RpcError),
    Storage(StorageError),
    Block(BlockError),
    Mempool(MempoolError),
    Network(NetworkError),
    Consensus(ConsensusError),
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
    UnexpectedSignedMessage {
        expected: ProtocolMessageKind,
        actual: ProtocolMessageKind,
    },
    UnexpectedSnapshotSyncMessage {
        expected: ProtocolMessageKind,
        actual: ProtocolMessageKind,
    },
    DuplicateValidatorKey(String),
    ValidatorMetadataKeyNotFound(String),
    ValidatorSetMetadataUpdateAlreadyApplied(String),
    ValidatorSetMetadataUpdateMismatch,
    ValidatorSetMetadataUpdateNotFound(String),
    ValidatorSetMetadataUpdateExpired {
        update_id: String,
        expires_at_height: u64,
        current_height: u64,
    },
    ValidatorSetMetadataAuthorizationPoolFull {
        pending_updates: usize,
        max_pending_updates: usize,
    },
    ValidatorSetMetadataAuthorizationRateLimited {
        signer: String,
        pending_authorizations: usize,
        max_pending_authorizations: usize,
    },
    ValidatorSetChainMismatch {
        expected: String,
        actual: String,
    },
    UnsignedValidatorSetMetadataUpdate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkIngestOutcome {
    TransactionAccepted,
    BlockImported,
    VoteReceived,
    FinalityCertificateReceived,
    EquivocationEvidencePersisted,
    ValidatorSetMetadataUpdated,
    ValidatorSetMetadataAuthorizationStored,
    IgnoredControlMessage,
}

pub struct PersistentValidatorNode {
    validator_id: String,
    network_id: String,
    chain_id: ChainId,
    validator_keys: BTreeMap<String, ValidatorPublicKey>,
    applied_validator_set_updates: BTreeSet<String>,
    pending_validator_set_metadata_authorizations:
        BTreeMap<String, BTreeMap<String, SignedValidatorMessage>>,
    max_pending_validator_set_metadata_updates: usize,
    max_pending_validator_set_metadata_authorizations_per_validator: usize,
    max_validator_set_metadata_audit_records: usize,
    max_validator_set_metadata_audit_page_size: usize,
    rpc: RpcService,
    storage: FileStorage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PersistentNodeSnapshot {
    pub state_snapshot: StateSnapshot,
    pub validator_set_metadata_audit_root: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotSyncClientMetrics {
    pub retry_attempts: u32,
    pub stream_failures: u32,
    pub requests_sent: u32,
    pub manifests_received: u32,
    pub chunks_received: u32,
    pub resume_requests: u32,
    pub metadata_roots_verified: bool,
    pub required_metadata_roots_root: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SnapshotSyncRetryPolicy {
    pub max_attempts: u32,
}

impl Default for SnapshotSyncRetryPolicy {
    fn default() -> Self {
        Self { max_attempts: 3 }
    }
}

impl SnapshotSyncClientMetrics {
    fn include_attempt(&mut self, attempt: SnapshotSyncClientMetrics) {
        self.requests_sent += attempt.requests_sent;
        self.manifests_received += attempt.manifests_received;
        self.chunks_received += attempt.chunks_received;
        self.resume_requests += attempt.resume_requests;
        self.metadata_roots_verified |= attempt.metadata_roots_verified;
        if attempt.required_metadata_roots_root.is_some() {
            self.required_metadata_roots_root = attempt.required_metadata_roots_root;
        }
    }
}

fn snapshot_sync_client_metrics_report(
    metrics: SnapshotSyncClientMetrics,
) -> SnapshotSyncClientMetricsReport {
    SnapshotSyncClientMetricsReport {
        retry_attempts: metrics.retry_attempts,
        stream_failures: metrics.stream_failures,
        requests_sent: metrics.requests_sent,
        manifests_received: metrics.manifests_received,
        chunks_received: metrics.chunks_received,
        resume_requests: metrics.resume_requests,
        metadata_roots_verified: metrics.metadata_roots_verified,
        required_metadata_roots_root: metrics.required_metadata_roots_root,
    }
}

pub fn fetch_verified_snapshot_chunk_set_over_tcp(
    stream: &mut TcpProtocolStream,
    snapshot_root: impl Into<String>,
    max_chunks_per_request: u32,
    required_metadata_roots: &BTreeMap<String, String>,
) -> Result<SnapshotChunkSet, NodeError> {
    fetch_verified_snapshot_chunk_set_over_tcp_with_metrics(
        stream,
        snapshot_root,
        max_chunks_per_request,
        required_metadata_roots,
    )
    .map(|(chunk_set, _metrics)| chunk_set)
}

pub fn fetch_verified_snapshot_chunk_set_over_tcp_with_retries(
    mut connect: impl FnMut() -> Result<TcpProtocolStream, NetworkError>,
    snapshot_root: impl Into<String>,
    max_chunks_per_request: u32,
    required_metadata_roots: &BTreeMap<String, String>,
    retry_policy: SnapshotSyncRetryPolicy,
) -> Result<(SnapshotChunkSet, SnapshotSyncClientMetrics), NodeError> {
    let snapshot_root = snapshot_root.into();
    let mut metrics = SnapshotSyncClientMetrics::default();
    let mut last_error = "no attempts configured".to_string();

    for attempt in 0..retry_policy.max_attempts {
        metrics.retry_attempts += 1;
        let mut stream = match connect() {
            Ok(stream) => stream,
            Err(error) => {
                metrics.stream_failures += 1;
                last_error = format!("{error:?}");
                continue;
            }
        };

        match fetch_verified_snapshot_chunk_set_over_tcp_with_metrics(
            &mut stream,
            snapshot_root.clone(),
            max_chunks_per_request,
            required_metadata_roots,
        ) {
            Ok((chunk_set, attempt_metrics)) => {
                metrics.include_attempt(attempt_metrics);
                return Ok((chunk_set, metrics));
            }
            Err(NodeError::Network(error)) if attempt + 1 < retry_policy.max_attempts => {
                metrics.stream_failures += 1;
                last_error = format!("{error:?}");
            }
            Err(error) => return Err(error),
        }
    }

    Err(NodeError::Network(NetworkError::RetryExhausted {
        attempts: retry_policy.max_attempts as usize,
        last_error,
    }))
}

pub fn fetch_verified_snapshot_chunk_set_over_tcp_with_metrics(
    stream: &mut TcpProtocolStream,
    snapshot_root: impl Into<String>,
    max_chunks_per_request: u32,
    required_metadata_roots: &BTreeMap<String, String>,
) -> Result<(SnapshotChunkSet, SnapshotSyncClientMetrics), NodeError> {
    if max_chunks_per_request == 0 {
        return Err(NodeError::SnapshotSync(SnapshotSyncError::InvalidChunkSize));
    }

    let snapshot_root = snapshot_root.into();
    let required_metadata_roots_root =
        FileStorage::required_snapshot_metadata_roots_root_for(required_metadata_roots)
            .map_err(NodeError::Storage)?;
    let mut manifest: Option<SnapshotChunkManifest> = None;
    let mut chunks = Vec::new();
    let mut start_index = 0;
    let mut metrics = SnapshotSyncClientMetrics::default();

    loop {
        metrics.requests_sent += 1;
        if start_index > 0 {
            metrics.resume_requests += 1;
        }
        stream
            .send(&NetworkMessage::SnapshotChunkRequest(
                SnapshotChunkRequest {
                    snapshot_root: snapshot_root.clone(),
                    start_index,
                    max_chunks: max_chunks_per_request,
                },
            ))
            .map_err(NodeError::Network)?;

        let manifest_message = stream.receive().map_err(NodeError::Network)?;
        let NetworkMessage::SnapshotChunkManifest(next_manifest) = manifest_message else {
            return Err(NodeError::UnexpectedSnapshotSyncMessage {
                expected: ProtocolMessageKind::SnapshotChunkManifest,
                actual: manifest_message.kind(),
            });
        };
        metrics.manifests_received += 1;
        if let Some(manifest) = &manifest {
            if manifest != &next_manifest {
                let expected = manifest.manifest_hash().map_err(NodeError::SnapshotSync)?;
                let actual = next_manifest
                    .manifest_hash()
                    .map_err(NodeError::SnapshotSync)?;
                return Err(NodeError::SnapshotSync(
                    SnapshotSyncError::ManifestHashMismatch { expected, actual },
                ));
            }
        } else {
            manifest = Some(next_manifest);
        }

        let manifest_ref = manifest.as_ref().unwrap();
        let remaining = manifest_ref.chunk_count.saturating_sub(start_index);
        let expected_chunks = remaining.min(max_chunks_per_request);
        for _ in 0..expected_chunks {
            let chunk_message = stream.receive().map_err(NodeError::Network)?;
            match chunk_message {
                NetworkMessage::SnapshotChunk(chunk) => {
                    metrics.chunks_received += 1;
                    chunks.push(chunk);
                }
                message => {
                    return Err(NodeError::UnexpectedSnapshotSyncMessage {
                        expected: ProtocolMessageKind::SnapshotChunk,
                        actual: message.kind(),
                    });
                }
            }
        }

        if chunks.len() >= manifest_ref.chunk_count as usize {
            let chunk_set = SnapshotChunkSet {
                manifest: manifest.unwrap(),
                chunks,
            };
            chunk_set
                .verify_with_metadata_roots(required_metadata_roots)
                .map_err(NodeError::SnapshotSync)?;
            metrics.metadata_roots_verified = true;
            metrics.required_metadata_roots_root = Some(required_metadata_roots_root);
            return Ok((chunk_set, metrics));
        }
        start_index += expected_chunks;
    }
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
            applied_validator_set_updates: BTreeSet::new(),
            pending_validator_set_metadata_authorizations: BTreeMap::new(),
            max_pending_validator_set_metadata_updates:
                DEFAULT_MAX_PENDING_VALIDATOR_SET_METADATA_UPDATES,
            max_pending_validator_set_metadata_authorizations_per_validator:
                DEFAULT_MAX_PENDING_VALIDATOR_SET_METADATA_AUTHORIZATIONS_PER_VALIDATOR,
            max_validator_set_metadata_audit_records:
                DEFAULT_MAX_VALIDATOR_SET_METADATA_AUDIT_RECORDS,
            max_validator_set_metadata_audit_page_size:
                DEFAULT_MAX_VALIDATOR_SET_METADATA_AUDIT_PAGE_SIZE,
        })
    }

    pub fn bootstrap_with_validator_set(
        validator_id: impl Into<String>,
        state: DeTTaState,
        storage_root: impl Into<PathBuf>,
        network_id: impl Into<String>,
        validator_keys: Vec<ValidatorPublicKey>,
    ) -> Result<Self, NodeError> {
        let mut node = Self::bootstrap(validator_id, state, storage_root)?;
        node.persist_validator_set_metadata(network_id, validator_keys)?;
        Ok(node)
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

        let mut node = Self {
            rpc: RpcService::new(node),
            storage,
            validator_id,
            network_id: DEFAULT_NODE_NETWORK_ID.into(),
            chain_id,
            validator_keys: BTreeMap::new(),
            applied_validator_set_updates: BTreeSet::new(),
            pending_validator_set_metadata_authorizations: BTreeMap::new(),
            max_pending_validator_set_metadata_updates:
                DEFAULT_MAX_PENDING_VALIDATOR_SET_METADATA_UPDATES,
            max_pending_validator_set_metadata_authorizations_per_validator:
                DEFAULT_MAX_PENDING_VALIDATOR_SET_METADATA_AUTHORIZATIONS_PER_VALIDATOR,
            max_validator_set_metadata_audit_records:
                DEFAULT_MAX_VALIDATOR_SET_METADATA_AUDIT_RECORDS,
            max_validator_set_metadata_audit_page_size:
                DEFAULT_MAX_VALIDATOR_SET_METADATA_AUDIT_PAGE_SIZE,
        };
        if let Some(metadata) = node
            .storage
            .maybe_load_validator_set_metadata()
            .map_err(NodeError::Storage)?
        {
            node.apply_validator_set_metadata(metadata)?;
        }
        node.load_pending_validator_set_metadata_authorizations()?;
        Ok(node)
    }

    pub fn validator_id(&self) -> &str {
        &self.validator_id
    }

    pub fn rpc(&self) -> &RpcService {
        &self.rpc
    }

    pub fn node_snapshot(&self) -> Result<PersistentNodeSnapshot, NodeError> {
        let metadata_roots = self.effective_snapshot_metadata_roots()?;
        Ok(PersistentNodeSnapshot {
            state_snapshot: self.rpc.snapshot(),
            validator_set_metadata_audit_root: metadata_roots
                .get(SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT)
                .cloned()
                .unwrap_or_default(),
        })
    }

    pub fn node_snapshot_roots(&self) -> Result<PersistentNodeSnapshotRoots, NodeError> {
        let snapshot = self.node_snapshot()?;
        let required_snapshot_metadata_roots = self.load_required_snapshot_metadata_roots()?;
        let required_snapshot_metadata_roots_root = self.required_snapshot_metadata_roots_root()?;
        let snapshot_sync_client_metrics = self
            .load_snapshot_sync_client_metrics()?
            .map(snapshot_sync_client_metrics_report);
        Ok(PersistentNodeSnapshotRoots {
            storage_root: snapshot.state_snapshot.storage_root,
            registry_root: snapshot.state_snapshot.registry_root,
            policy_root: snapshot.state_snapshot.policy_root,
            event_root: snapshot.state_snapshot.event_root,
            nonce_root: snapshot.state_snapshot.nonce_root,
            outbox_root: snapshot.state_snapshot.outbox_root,
            global_state_root: snapshot.state_snapshot.global_state_root,
            validator_set_metadata_audit_root: snapshot.validator_set_metadata_audit_root,
            snapshot_import_audit_root: self.snapshot_import_audit_root()?,
            required_snapshot_metadata_roots,
            required_snapshot_metadata_roots_root,
            snapshot_sync_client_metrics,
        })
    }

    pub fn snapshot_metadata_root_status(&self) -> Result<SnapshotMetadataRootStatus, NodeError> {
        let persisted_metadata_roots = self
            .storage
            .load_snapshot_metadata_roots()
            .map_err(NodeError::Storage)?;
        let local_validator_set_metadata_audit_root = self
            .storage
            .validator_set_metadata_audit_root()
            .map_err(NodeError::Storage)?;
        let persisted_validator_set_metadata_audit_root = persisted_metadata_roots
            .get(SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT)
            .cloned();
        let validator_set_metadata_audit_root = persisted_validator_set_metadata_audit_root
            .clone()
            .unwrap_or_else(|| local_validator_set_metadata_audit_root.clone());
        let persisted_matches_local_validator_set_metadata_audit_root =
            persisted_validator_set_metadata_audit_root
                .as_ref()
                .is_some_and(|root| root == &local_validator_set_metadata_audit_root);
        let using_imported_validator_set_metadata_audit_root =
            persisted_validator_set_metadata_audit_root
                .as_ref()
                .is_some_and(|root| root != &local_validator_set_metadata_audit_root);
        Ok(SnapshotMetadataRootStatus {
            validator_set_metadata_audit_root,
            local_validator_set_metadata_audit_root,
            persisted_validator_set_metadata_audit_root,
            using_imported_validator_set_metadata_audit_root,
            persisted_matches_local_validator_set_metadata_audit_root,
        })
    }

    pub fn persist_snapshot_sync_client_metrics(
        &self,
        metrics: &SnapshotSyncClientMetrics,
    ) -> Result<(), NodeError> {
        self.storage
            .commit_snapshot_sync_client_metrics(metrics)
            .map_err(NodeError::Storage)
    }

    pub fn load_snapshot_sync_client_metrics(
        &self,
    ) -> Result<Option<SnapshotSyncClientMetrics>, NodeError> {
        self.storage
            .load_snapshot_sync_client_metrics()
            .map_err(NodeError::Storage)
    }

    pub fn snapshot_metadata_roots(&self) -> Result<BTreeMap<String, String>, NodeError> {
        self.effective_snapshot_metadata_roots()
    }

    pub fn load_required_snapshot_metadata_roots(
        &self,
    ) -> Result<BTreeMap<String, String>, NodeError> {
        self.storage
            .load_required_snapshot_metadata_roots()
            .map_err(NodeError::Storage)
    }

    pub fn required_snapshot_metadata_roots_root(&self) -> Result<String, NodeError> {
        self.storage
            .required_snapshot_metadata_roots_root()
            .map_err(NodeError::Storage)
    }

    pub fn required_snapshot_metadata_roots_report(
        &self,
    ) -> Result<RequiredSnapshotMetadataRootsReport, NodeError> {
        Ok(RequiredSnapshotMetadataRootsReport {
            roots: self.load_required_snapshot_metadata_roots()?,
            root: self.required_snapshot_metadata_roots_root()?,
        })
    }

    fn effective_snapshot_metadata_roots(&self) -> Result<BTreeMap<String, String>, NodeError> {
        let mut metadata_roots = self
            .storage
            .load_snapshot_metadata_roots()
            .map_err(NodeError::Storage)?;
        if !metadata_roots.contains_key(SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT) {
            metadata_roots.insert(
                SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT.into(),
                self.storage
                    .validator_set_metadata_audit_root()
                    .map_err(NodeError::Storage)?,
            );
        }
        if let Some(metrics_root) = self
            .storage
            .snapshot_sync_client_metrics_root::<SnapshotSyncClientMetrics>()
            .map_err(NodeError::Storage)?
        {
            metadata_roots.insert(
                SNAPSHOT_METADATA_STATE_SYNC_CLIENT_METRICS_ROOT.into(),
                metrics_root,
            );
        }
        let required_metadata_roots = self.load_required_snapshot_metadata_roots()?;
        if !required_metadata_roots.is_empty() {
            metadata_roots.insert(
                SNAPSHOT_METADATA_REQUIRED_METADATA_ROOTS_ROOT.into(),
                FileStorage::required_snapshot_metadata_roots_root_for(&required_metadata_roots)
                    .map_err(NodeError::Storage)?,
            );
        }
        let snapshot_import_audit_records = self.load_snapshot_import_audit_records()?;
        if !snapshot_import_audit_records.is_empty() {
            metadata_roots.insert(
                SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_ROOT.into(),
                FileStorage::snapshot_import_audit_root_for(&snapshot_import_audit_records)
                    .map_err(NodeError::Storage)?,
            );
        }
        Ok(metadata_roots)
    }

    fn persist_current_snapshot_metadata_roots(&self) -> Result<(), NodeError> {
        let mut metadata_roots = self
            .storage
            .load_snapshot_metadata_roots()
            .map_err(NodeError::Storage)?;
        metadata_roots.insert(
            SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT.into(),
            self.storage
                .validator_set_metadata_audit_root()
                .map_err(NodeError::Storage)?,
        );
        self.storage
            .commit_snapshot_metadata_roots(&metadata_roots)
            .map_err(NodeError::Storage)
    }

    pub fn handle_rpc_request(&mut self, request: RpcRequest) -> RpcResponse {
        match request {
            RpcRequest::ProposeValidatorSetMetadataUpdate { authorization } => self
                .propose_validator_set_metadata_update_authorization(authorization)
                .map(RpcResult::ValidatorSetMetadataUpdateStatus)
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetValidatorSetMetadataUpdateStatus { update_id } => {
                RpcResponse::Ok(RpcResult::ValidatorSetMetadataUpdateStatus(
                    self.validator_set_metadata_update_status(update_id),
                ))
            }
            RpcRequest::GetValidatorSetMetadataAuditRecords { offset, limit } => self
                .load_validator_set_metadata_audit_records_page(offset, limit)
                .map(RpcResult::ValidatorSetMetadataAuditRecords)
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetSnapshotImportAuditRecords { offset, limit } => self
                .load_snapshot_import_audit_records_page(offset, limit)
                .map(RpcResult::SnapshotImportAuditRecords)
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetSnapshotImportAuditRoot => self
                .snapshot_import_audit_root()
                .map(RpcResult::SnapshotImportAuditRoot)
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetPersistentNodeSnapshotRoots => self
                .node_snapshot_roots()
                .map(|snapshot| RpcResult::PersistentNodeSnapshotRoots(Box::new(snapshot)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetSnapshotMetadataRootStatus => self
                .snapshot_metadata_root_status()
                .map(RpcResult::SnapshotMetadataRootStatus)
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetSnapshotSyncClientMetrics => self
                .load_snapshot_sync_client_metrics()
                .map(|metrics| {
                    RpcResult::SnapshotSyncClientMetrics(
                        metrics.map(snapshot_sync_client_metrics_report),
                    )
                })
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetRequiredSnapshotMetadataRoots => self
                .required_snapshot_metadata_roots_report()
                .map(RpcResult::RequiredSnapshotMetadataRoots)
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            request => self.rpc.handle_request(request),
        }
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

    pub fn validator_key_count(&self) -> usize {
        self.validator_keys.len()
    }

    pub fn trusted_validator_key(&self, validator_id: &str) -> Option<&ValidatorPublicKey> {
        self.validator_keys.get(validator_id)
    }

    pub fn has_applied_validator_set_update(&self, update_id: &str) -> bool {
        self.applied_validator_set_updates.contains(update_id)
    }

    pub fn validator_set_metadata_update_status(
        &self,
        update_id: impl Into<String>,
    ) -> ValidatorSetMetadataUpdateStatus {
        let update_id = update_id.into();
        ValidatorSetMetadataUpdateStatus {
            pending_authorizations: self
                .pending_validator_set_metadata_authorizations
                .get(&update_id)
                .map_or(0, BTreeMap::len),
            required_quorum: self.validator_set_metadata_quorum(),
            applied: self.has_applied_validator_set_update(&update_id),
            update_id,
        }
    }

    pub fn current_height(&self) -> u64 {
        self.rpc.node().state().height()
    }

    pub fn set_validator_set_metadata_authorization_limits(
        &mut self,
        max_pending_updates: usize,
        max_pending_authorizations_per_validator: usize,
    ) {
        self.max_pending_validator_set_metadata_updates = max_pending_updates;
        self.max_pending_validator_set_metadata_authorizations_per_validator =
            max_pending_authorizations_per_validator;
    }

    pub fn set_validator_set_metadata_audit_limits(
        &mut self,
        max_records: usize,
        max_page_size: usize,
    ) {
        self.max_validator_set_metadata_audit_records = max_records;
        self.max_validator_set_metadata_audit_page_size = max_page_size;
    }

    pub fn validator_set_metadata_quorum(&self) -> usize {
        quorum_for(self.validator_keys.len()).max(1)
    }

    pub fn persist_validator_set_metadata(
        &mut self,
        network_id: impl Into<String>,
        validator_keys: Vec<ValidatorPublicKey>,
    ) -> Result<(), NodeError> {
        let metadata = ValidatorSetMetadata {
            network_id: network_id.into(),
            chain_id: self.chain_id.clone(),
            validators: validator_keys,
            applied_updates: Vec::new(),
        };
        let keyring = keyring_from_metadata(&metadata)?;
        self.storage
            .commit_validator_set_metadata(&metadata)
            .map_err(NodeError::Storage)?;
        self.network_id = metadata.network_id;
        self.validator_keys = keyring;
        self.applied_validator_set_updates = metadata.applied_updates.into_iter().collect();
        Ok(())
    }

    pub fn verified_validator_set_metadata_update(
        &self,
        signed: &SignedValidatorMessage,
    ) -> Result<ValidatorSetMetadataUpdate, NodeError> {
        match self.verified_signed_validator_message(signed)? {
            NetworkMessage::ValidatorSetMetadataUpdate(update) => Ok(update),
            other => Err(NodeError::UnexpectedSignedMessage {
                expected: ProtocolMessageKind::ValidatorSetMetadataUpdate,
                actual: other.kind(),
            }),
        }
    }

    pub fn apply_quorum_authorized_validator_set_metadata_update(
        &mut self,
        messages: &[NetworkMessage],
    ) -> Result<(), NodeError> {
        self.apply_quorum_authorized_validator_set_metadata_update_with_quorum(
            messages,
            self.validator_set_metadata_quorum(),
        )
    }

    pub fn apply_quorum_authorized_validator_set_metadata_update_with_quorum(
        &mut self,
        messages: &[NetworkMessage],
        quorum: usize,
    ) -> Result<(), NodeError> {
        let mut authorized_update = None;
        let mut authorizers = BTreeSet::new();

        for message in messages {
            let NetworkMessage::SignedValidator(signed) = message else {
                return Err(NodeError::UnexpectedSignedMessage {
                    expected: ProtocolMessageKind::ValidatorSetMetadataUpdate,
                    actual: message.kind(),
                });
            };
            let update = self.verified_validator_set_metadata_update(signed)?;
            if self.has_applied_validator_set_update(&update.update_id) {
                return Err(NodeError::ValidatorSetMetadataUpdateAlreadyApplied(
                    update.update_id,
                ));
            }
            if let Some(expected) = &authorized_update {
                if expected != &update {
                    return Err(NodeError::ValidatorSetMetadataUpdateMismatch);
                }
            } else {
                authorized_update = Some(update);
            }
            authorizers.insert(signed.signer.clone());
        }

        if authorizers.len() < quorum {
            return Err(NodeError::Consensus(ConsensusError::QuorumNotReached {
                accepted: authorizers.len(),
                required: quorum,
            }));
        }

        let update =
            authorized_update.ok_or(NodeError::Consensus(ConsensusError::QuorumNotReached {
                accepted: 0,
                required: quorum,
            }))?;
        self.apply_validator_set_metadata_update(update)
    }

    pub fn record_pending_validator_set_metadata_authorization(
        &mut self,
        message: NetworkMessage,
    ) -> Result<usize, NodeError> {
        let signed = match message {
            NetworkMessage::SignedValidator(signed) => signed,
            other => {
                return Err(NodeError::UnexpectedSignedMessage {
                    expected: ProtocolMessageKind::ValidatorSetMetadataUpdate,
                    actual: other.kind(),
                });
            }
        };
        let signer = signed.signer.clone();
        let verified_update = self.verified_validator_set_metadata_update(&signed).ok();
        let count = match self.insert_pending_validator_set_metadata_authorization(*signed) {
            Ok(count) => count,
            Err(error) => {
                if let Some(update) = verified_update {
                    self.record_validator_set_metadata_audit(
                        update.update_id,
                        ValidatorSetMetadataAuditOutcome::Rejected,
                        vec![signer],
                        node_rpc_error_code(&error).to_string(),
                    )?;
                }
                return Err(error);
            }
        };
        self.persist_pending_validator_set_metadata_authorizations()?;
        Ok(count)
    }

    pub fn propose_validator_set_metadata_update_authorization(
        &mut self,
        authorization: SignedValidatorMessage,
    ) -> Result<ValidatorSetMetadataUpdateStatus, NodeError> {
        let update = self.verified_validator_set_metadata_update(&authorization)?;
        let update_id = update.update_id;
        let authorization_count = self.record_pending_validator_set_metadata_authorization(
            NetworkMessage::SignedValidator(Box::new(authorization)),
        )?;
        if authorization_count >= self.validator_set_metadata_quorum() {
            self.apply_pending_validator_set_metadata_update(&update_id)?;
        }
        Ok(self.validator_set_metadata_update_status(update_id))
    }

    fn insert_pending_validator_set_metadata_authorization(
        &mut self,
        signed: SignedValidatorMessage,
    ) -> Result<usize, NodeError> {
        let update = self.verified_validator_set_metadata_update(&signed)?;
        self.reject_expired_validator_set_metadata_update(&update)?;
        if self.has_applied_validator_set_update(&update.update_id) {
            return Err(NodeError::ValidatorSetMetadataUpdateAlreadyApplied(
                update.update_id,
            ));
        }
        let signer = signed.signer.clone();
        let existing_update_authorizations = self
            .pending_validator_set_metadata_authorizations
            .get(&update.update_id);
        let replacing_existing_authorization = existing_update_authorizations
            .is_some_and(|authorizations| authorizations.contains_key(&signer));
        if existing_update_authorizations.is_none()
            && self.pending_validator_set_metadata_authorizations.len()
                >= self.max_pending_validator_set_metadata_updates
        {
            return Err(NodeError::ValidatorSetMetadataAuthorizationPoolFull {
                pending_updates: self.pending_validator_set_metadata_authorizations.len(),
                max_pending_updates: self.max_pending_validator_set_metadata_updates,
            });
        }
        let pending_authorizations_for_signer = self
            .pending_validator_set_metadata_authorizations
            .values()
            .filter(|authorizations| authorizations.contains_key(&signer))
            .count();
        if !replacing_existing_authorization
            && pending_authorizations_for_signer
                >= self.max_pending_validator_set_metadata_authorizations_per_validator
        {
            return Err(NodeError::ValidatorSetMetadataAuthorizationRateLimited {
                signer,
                pending_authorizations: pending_authorizations_for_signer,
                max_pending_authorizations: self
                    .max_pending_validator_set_metadata_authorizations_per_validator,
            });
        }
        if let Some(existing_authorizations) = self
            .pending_validator_set_metadata_authorizations
            .get(&update.update_id)
        {
            if let Some(existing) = existing_authorizations.values().next() {
                let existing_update = self.verified_validator_set_metadata_update(existing)?;
                if existing_update != update {
                    return Err(NodeError::ValidatorSetMetadataUpdateMismatch);
                }
            }
        }

        let entry = self
            .pending_validator_set_metadata_authorizations
            .entry(update.update_id)
            .or_default();
        entry.insert(signed.signer.clone(), signed);
        Ok(entry.len())
    }

    pub fn prune_validator_set_metadata_authorizations(&mut self) -> Result<usize, NodeError> {
        let current_height = self.current_height();
        let mut pruned_updates = Vec::new();
        for (update_id, authorizations) in &self.pending_validator_set_metadata_authorizations {
            let signers = authorizations.keys().cloned().collect::<Vec<_>>();
            let Some(signed) = authorizations.values().next() else {
                pruned_updates.push((
                    update_id.clone(),
                    signers,
                    "empty_authorization_set".to_string(),
                ));
                continue;
            };
            let update = self.verified_validator_set_metadata_update(signed)?;
            if self.has_applied_validator_set_update(update_id) {
                pruned_updates.push((update_id.clone(), signers, "already_applied".to_string()));
            } else if validator_set_metadata_update_expired(&update, current_height) {
                pruned_updates.push((update_id.clone(), signers, "expired".to_string()));
            }
        }

        let pruned = pruned_updates.len();
        if pruned > 0 {
            for (update_id, signers, reason) in pruned_updates {
                self.pending_validator_set_metadata_authorizations
                    .remove(&update_id);
                self.record_validator_set_metadata_audit(
                    update_id,
                    ValidatorSetMetadataAuditOutcome::Pruned,
                    signers,
                    reason,
                )?;
            }
            self.persist_pending_validator_set_metadata_authorizations()?;
        }
        Ok(pruned)
    }

    fn persist_pending_validator_set_metadata_authorizations(&self) -> Result<(), NodeError> {
        let authorizations = self
            .pending_validator_set_metadata_authorizations
            .values()
            .flat_map(|authorizations| authorizations.values().cloned())
            .collect::<Vec<_>>();
        self.storage
            .commit_pending_validator_set_metadata_authorizations(&authorizations)
            .map_err(NodeError::Storage)
    }

    fn load_pending_validator_set_metadata_authorizations(&mut self) -> Result<(), NodeError> {
        let authorizations = self
            .storage
            .load_pending_validator_set_metadata_authorizations()
            .map_err(NodeError::Storage)?;
        self.pending_validator_set_metadata_authorizations.clear();
        for signed in authorizations {
            let update = self.verified_validator_set_metadata_update(&signed)?;
            if self.has_applied_validator_set_update(&update.update_id)
                || validator_set_metadata_update_expired(&update, self.current_height())
            {
                continue;
            }
            self.insert_pending_validator_set_metadata_authorization(signed)?;
        }
        self.persist_pending_validator_set_metadata_authorizations()
    }

    pub fn pending_validator_set_metadata_authorization_count(&self, update_id: &str) -> usize {
        self.pending_validator_set_metadata_authorizations
            .get(update_id)
            .map_or(0, BTreeMap::len)
    }

    pub fn apply_pending_validator_set_metadata_update(
        &mut self,
        update_id: &str,
    ) -> Result<(), NodeError> {
        let authorizations = self
            .pending_validator_set_metadata_authorizations
            .get(update_id)
            .ok_or_else(|| NodeError::ValidatorSetMetadataUpdateNotFound(update_id.to_string()))?;
        let signers = authorizations.keys().cloned().collect::<Vec<_>>();
        let messages = authorizations
            .values()
            .cloned()
            .map(|signed| NetworkMessage::SignedValidator(Box::new(signed)))
            .collect::<Vec<_>>();
        if let Err(error) = self.apply_quorum_authorized_validator_set_metadata_update(&messages) {
            self.record_validator_set_metadata_audit(
                update_id.to_string(),
                ValidatorSetMetadataAuditOutcome::Rejected,
                signers,
                node_rpc_error_code(&error).to_string(),
            )?;
            return Err(error);
        }
        self.record_validator_set_metadata_audit(
            update_id.to_string(),
            ValidatorSetMetadataAuditOutcome::Applied,
            signers,
            "applied".into(),
        )?;
        self.pending_validator_set_metadata_authorizations
            .remove(update_id);
        self.persist_pending_validator_set_metadata_authorizations()
    }

    fn apply_validator_set_metadata_update(
        &mut self,
        update: ValidatorSetMetadataUpdate,
    ) -> Result<(), NodeError> {
        if self
            .applied_validator_set_updates
            .contains(&update.update_id)
        {
            return Err(NodeError::ValidatorSetMetadataUpdateAlreadyApplied(
                update.update_id,
            ));
        }

        let mut next_keyring = self.validator_keys.clone();
        for validator_id in &update.remove_validators {
            if next_keyring.remove(validator_id).is_none() {
                return Err(NodeError::ValidatorMetadataKeyNotFound(
                    validator_id.clone(),
                ));
            }
        }
        for public_key in update.add_validators {
            if next_keyring.contains_key(&public_key.validator_id) {
                return Err(NodeError::DuplicateValidatorKey(public_key.validator_id));
            }
            next_keyring.insert(public_key.validator_id.clone(), public_key);
        }

        let mut applied_updates = self.applied_validator_set_updates.clone();
        applied_updates.insert(update.update_id);
        let metadata = ValidatorSetMetadata {
            network_id: self.network_id.clone(),
            chain_id: self.chain_id.clone(),
            validators: next_keyring.values().cloned().collect(),
            applied_updates: applied_updates.iter().cloned().collect(),
        };
        self.storage
            .commit_validator_set_metadata(&metadata)
            .map_err(NodeError::Storage)?;
        self.validator_keys = next_keyring;
        self.applied_validator_set_updates = applied_updates;
        Ok(())
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

    pub fn persist_finality_certificate(
        &self,
        certificate: &FinalityCertificate,
    ) -> Result<(), NodeError> {
        self.storage
            .commit_finality_certificate(certificate)
            .map_err(NodeError::Storage)
    }

    pub fn load_finality_certificate(&self, height: u64) -> Result<FinalityCertificate, NodeError> {
        self.storage
            .load_finality_certificate(height)
            .map_err(NodeError::Storage)
    }

    pub fn persist_and_gossip_signed_finality_certificate(
        &self,
        certificate: FinalityCertificate,
        signing_key: &ValidatorSigningKey,
        transport: &mut InMemoryTransport,
    ) -> Result<usize, NodeError> {
        self.persist_finality_certificate(&certificate)?;
        let signed = self.sign_validator_message(
            signing_key,
            NetworkMessage::FinalityCertificate(certificate),
        )?;
        transport
            .broadcast(self.validator_id.clone(), signed)
            .map_err(NodeError::Network)
    }

    pub fn persist_equivocation_evidence(
        &self,
        evidence: EquivocationEvidence,
    ) -> Result<SlashingRecord, NodeError> {
        let record = SlashingRecord {
            validator_id: evidence.validator_id.clone(),
            slashed_at_height: evidence.height,
            evidence,
        };
        self.storage
            .commit_slashing_record(&record)
            .map_err(NodeError::Storage)?;
        Ok(record)
    }

    pub fn load_slashing_record(&self, validator_id: &str) -> Result<SlashingRecord, NodeError> {
        self.storage
            .load_slashing_record(validator_id)
            .map_err(NodeError::Storage)
    }

    pub fn load_validator_set_metadata_audit_records(
        &self,
    ) -> Result<Vec<ValidatorSetMetadataAuditRecord>, NodeError> {
        self.storage
            .load_validator_set_metadata_audit_records()
            .map_err(NodeError::Storage)
    }

    pub fn load_validator_set_metadata_audit_records_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<ValidatorSetMetadataAuditRecord>, NodeError> {
        self.storage
            .load_validator_set_metadata_audit_records_page(
                offset,
                limit.min(self.max_validator_set_metadata_audit_page_size),
            )
            .map_err(NodeError::Storage)
    }

    pub fn load_snapshot_import_audit_records(
        &self,
    ) -> Result<Vec<SnapshotImportAuditRecord>, NodeError> {
        self.storage
            .load_snapshot_import_audit_records()
            .map_err(NodeError::Storage)
    }

    pub fn load_snapshot_import_audit_records_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<SnapshotImportAuditRecord>, NodeError> {
        self.storage
            .load_snapshot_import_audit_records_page(offset, limit)
            .map_err(NodeError::Storage)
    }

    pub fn snapshot_import_audit_root(&self) -> Result<String, NodeError> {
        self.storage
            .snapshot_import_audit_root()
            .map_err(NodeError::Storage)
    }

    fn record_validator_set_metadata_audit(
        &self,
        update_id: String,
        outcome: ValidatorSetMetadataAuditOutcome,
        signers: Vec<String>,
        reason: String,
    ) -> Result<(), NodeError> {
        self.storage
            .append_validator_set_metadata_audit_record_with_retention(
                ValidatorSetMetadataAuditRecord {
                    update_id,
                    outcome,
                    height: self.current_height(),
                    signers,
                    reason,
                },
                self.max_validator_set_metadata_audit_records,
            )
            .map_err(NodeError::Storage)?;
        self.persist_current_snapshot_metadata_roots()
    }

    pub fn persist_and_gossip_signed_equivocation_evidence(
        &self,
        evidence: EquivocationEvidence,
        signing_key: &ValidatorSigningKey,
        transport: &mut InMemoryTransport,
    ) -> Result<usize, NodeError> {
        self.persist_equivocation_evidence(evidence.clone())?;
        let signed = self
            .sign_validator_message(signing_key, NetworkMessage::EquivocationEvidence(evidence))?;
        transport
            .broadcast(self.validator_id.clone(), signed)
            .map_err(NodeError::Network)
    }

    pub fn sign_validator_set_metadata_update_authorization(
        &self,
        update: ValidatorSetMetadataUpdate,
        signing_key: &ValidatorSigningKey,
    ) -> Result<NetworkMessage, NodeError> {
        self.sign_validator_message(
            signing_key,
            NetworkMessage::ValidatorSetMetadataUpdate(update),
        )
    }

    pub fn gossip_signed_validator_set_metadata_update_authorization(
        &self,
        update: ValidatorSetMetadataUpdate,
        signing_key: &ValidatorSigningKey,
        transport: &mut InMemoryTransport,
    ) -> Result<usize, NodeError> {
        let signed = self.sign_validator_set_metadata_update_authorization(update, signing_key)?;
        transport
            .broadcast(self.validator_id.clone(), signed)
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
            NetworkMessage::EquivocationEvidence(evidence) => {
                self.persist_equivocation_evidence(evidence.clone())?;
                Ok(NetworkIngestOutcome::EquivocationEvidencePersisted)
            }
            NetworkMessage::ValidatorSetMetadataUpdate(_) => {
                Err(NodeError::UnsignedValidatorSetMetadataUpdate)
            }
            NetworkMessage::SignedValidator(signed) => {
                let message = self.verified_signed_validator_message(signed)?;
                if let NetworkMessage::ValidatorSetMetadataUpdate(update) = message {
                    let update_id = update.update_id;
                    let authorization_count = self
                        .record_pending_validator_set_metadata_authorization(
                            envelope.message.clone(),
                        )?;
                    if authorization_count >= self.validator_set_metadata_quorum() {
                        self.apply_pending_validator_set_metadata_update(&update_id)?;
                        return Ok(NetworkIngestOutcome::ValidatorSetMetadataUpdated);
                    }
                    return Ok(NetworkIngestOutcome::ValidatorSetMetadataAuthorizationStored);
                }
                let signed_envelope = Envelope {
                    from: envelope.from.clone(),
                    to: envelope.to.clone(),
                    message,
                };
                self.ingest_network_envelope(&signed_envelope)
            }
            NetworkMessage::ValidatorSetUpdate(_)
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

        let metadata_roots = self.effective_snapshot_metadata_roots()?;
        let chunk_set =
            build_snapshot_chunks_with_metadata_roots(&snapshot, max_chunk_bytes, metadata_roots)
                .map_err(NodeError::SnapshotSync)?;
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

    pub fn import_snapshot_chunk_set(
        &self,
        chunk_set: &SnapshotChunkSet,
        required_metadata_roots: &BTreeMap<String, String>,
    ) -> Result<StateSnapshot, NodeError> {
        let snapshot = chunk_set
            .reconstruct_snapshot_with_metadata_roots(required_metadata_roots)
            .map_err(NodeError::SnapshotSync)?;
        self.storage
            .commit_snapshot(&snapshot)
            .map_err(NodeError::Storage)?;
        self.storage
            .commit_snapshot_metadata_roots(&chunk_set.manifest.metadata_roots)
            .map_err(NodeError::Storage)?;
        self.storage
            .commit_required_snapshot_metadata_roots(required_metadata_roots)
            .map_err(NodeError::Storage)?;
        let required_metadata_roots_root =
            FileStorage::required_snapshot_metadata_roots_root_for(required_metadata_roots)
                .map_err(NodeError::Storage)?;
        self.storage
            .append_snapshot_import_audit_record(SnapshotImportAuditRecord {
                snapshot_root: snapshot.global_state_root.clone(),
                manifest_hash: chunk_set
                    .manifest
                    .manifest_hash()
                    .map_err(NodeError::SnapshotSync)?,
                required_metadata_roots_root,
                required_metadata_roots_count: required_metadata_roots.len(),
                manifest_metadata_roots_count: chunk_set.manifest.metadata_roots.len(),
                chunk_count: chunk_set.manifest.chunk_count,
                metadata_roots_verified: true,
            })
            .map_err(NodeError::Storage)?;
        Ok(snapshot)
    }

    pub fn verified_consensus_vote(&self, message: &NetworkMessage) -> Result<Vote, NodeError> {
        let NetworkMessage::SignedValidator(signed) = message else {
            return Err(NodeError::UnexpectedSignedMessage {
                expected: ProtocolMessageKind::Vote,
                actual: message.kind(),
            });
        };
        match self.verified_signed_validator_message(signed)? {
            NetworkMessage::Vote(vote) => Ok(vote),
            other => Err(NodeError::UnexpectedSignedMessage {
                expected: ProtocolMessageKind::Vote,
                actual: other.kind(),
            }),
        }
    }

    pub fn collect_finality_certificate(
        &self,
        height: u64,
        block_hash: &str,
        messages: &[NetworkMessage],
        quorum: usize,
    ) -> Result<FinalityCertificate, NodeError> {
        let votes = messages
            .iter()
            .map(|message| self.verified_consensus_vote(message))
            .collect::<Result<Vec<_>, _>>()?;
        ConsensusCluster::certificate_from_votes(height, block_hash, votes, quorum)
            .map_err(NodeError::Consensus)
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

    fn verified_signed_validator_message(
        &self,
        signed: &SignedValidatorMessage,
    ) -> Result<NetworkMessage, NodeError> {
        self.verify_signed_validator_message(signed)?;
        Ok(signed.message.as_ref().clone())
    }

    fn apply_validator_set_metadata(
        &mut self,
        metadata: ValidatorSetMetadata,
    ) -> Result<(), NodeError> {
        if metadata.chain_id != self.chain_id {
            return Err(NodeError::ValidatorSetChainMismatch {
                expected: self.chain_id.clone(),
                actual: metadata.chain_id,
            });
        }
        let keyring = keyring_from_metadata(&metadata)?;
        self.network_id = metadata.network_id;
        self.validator_keys = keyring;
        self.applied_validator_set_updates = metadata.applied_updates.into_iter().collect();
        Ok(())
    }
}

impl JsonRpcHandler for PersistentValidatorNode {
    fn handle_json_request(&mut self, request: &[u8]) -> Result<Vec<u8>, RpcTransportError> {
        json_rpc_response_for_request(request, |request| self.handle_rpc_request(request))
    }
}

impl PersistentValidatorNode {
    fn reject_expired_validator_set_metadata_update(
        &self,
        update: &ValidatorSetMetadataUpdate,
    ) -> Result<(), NodeError> {
        let current_height = self.current_height();
        if let Some(expires_at_height) = update.expires_at_height {
            if current_height >= expires_at_height {
                return Err(NodeError::ValidatorSetMetadataUpdateExpired {
                    update_id: update.update_id.clone(),
                    expires_at_height,
                    current_height,
                });
            }
        }
        Ok(())
    }
}

fn validator_set_metadata_update_expired(
    update: &ValidatorSetMetadataUpdate,
    current_height: u64,
) -> bool {
    update
        .expires_at_height
        .is_some_and(|expires_at_height| current_height >= expires_at_height)
}

fn keyring_from_metadata(
    metadata: &ValidatorSetMetadata,
) -> Result<BTreeMap<String, ValidatorPublicKey>, NodeError> {
    let mut keyring = BTreeMap::new();
    for public_key in &metadata.validators {
        if keyring
            .insert(public_key.validator_id.clone(), public_key.clone())
            .is_some()
        {
            return Err(NodeError::DuplicateValidatorKey(
                public_key.validator_id.clone(),
            ));
        }
    }
    Ok(keyring)
}

fn node_rpc_error_response(error: NodeError) -> RpcResponse {
    RpcResponse::Error(RpcErrorBody {
        code: node_rpc_error_code(&error).into(),
        message: format!("{error:?}"),
    })
}

fn node_rpc_error_code(error: &NodeError) -> &'static str {
    match error {
        NodeError::ValidatorSetMetadataUpdateAlreadyApplied(_) => {
            "node.validator_set_metadata_update_already_applied"
        }
        NodeError::ValidatorSetMetadataUpdateMismatch => {
            "node.validator_set_metadata_update_mismatch"
        }
        NodeError::ValidatorSetMetadataUpdateNotFound(_) => {
            "node.validator_set_metadata_update_not_found"
        }
        NodeError::ValidatorSetMetadataUpdateExpired { .. } => {
            "node.validator_set_metadata_update_expired"
        }
        NodeError::ValidatorSetMetadataAuthorizationPoolFull { .. } => {
            "node.validator_set_metadata_authorization_pool_full"
        }
        NodeError::ValidatorSetMetadataAuthorizationRateLimited { .. } => {
            "node.validator_set_metadata_authorization_rate_limited"
        }
        NodeError::UnsignedValidatorSetMetadataUpdate => {
            "node.unsigned_validator_set_metadata_update"
        }
        NodeError::Signature(_) => "node.validator_signature_error",
        NodeError::ValidatorKeyNotFound(_) => "node.validator_key_not_found",
        NodeError::Consensus(ConsensusError::QuorumNotReached { .. }) => {
            "node.validator_quorum_not_reached"
        }
        _ => "node.error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Argument, Method};
    use detta_network::{InMemoryTransport, TcpProtocolStream};
    use detta_protocol::{
        ProtocolMessage, SignatureError, SnapshotChunkRequest, SnapshotChunkSet,
        ValidatorSetMetadataUpdate, ValidatorSigningKey,
    };
    use detta_rpc::JsonRpcServer;
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
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

    fn write_rpc_request(stream: &mut TcpStream, request: &RpcRequest) {
        serde_json::to_writer(&mut *stream, request).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();
    }

    fn read_rpc_response(reader: &mut BufReader<TcpStream>) -> RpcResponse {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(!line.is_empty());
        serde_json::from_str(&line).unwrap()
    }

    fn persistent_node_snapshot_roots(
        snapshot: &PersistentNodeSnapshot,
    ) -> PersistentNodeSnapshotRoots {
        PersistentNodeSnapshotRoots {
            storage_root: snapshot.state_snapshot.storage_root.clone(),
            registry_root: snapshot.state_snapshot.registry_root.clone(),
            policy_root: snapshot.state_snapshot.policy_root.clone(),
            event_root: snapshot.state_snapshot.event_root.clone(),
            nonce_root: snapshot.state_snapshot.nonce_root.clone(),
            outbox_root: snapshot.state_snapshot.outbox_root.clone(),
            global_state_root: snapshot.state_snapshot.global_state_root.clone(),
            validator_set_metadata_audit_root: snapshot.validator_set_metadata_audit_root.clone(),
            snapshot_import_audit_root: FileStorage::snapshot_import_audit_root_for(&[]).unwrap(),
            required_snapshot_metadata_roots: BTreeMap::new(),
            required_snapshot_metadata_roots_root:
                FileStorage::required_snapshot_metadata_roots_root_for(&BTreeMap::new()).unwrap(),
            snapshot_sync_client_metrics: None,
        }
    }

    fn expect_persistent_node_snapshot_roots(response: RpcResponse) -> PersistentNodeSnapshotRoots {
        match response {
            RpcResponse::Ok(RpcResult::PersistentNodeSnapshotRoots(snapshot)) => *snapshot,
            response => panic!("expected persistent node snapshot roots, got {response:?}"),
        }
    }

    fn expect_snapshot_metadata_root_status(response: RpcResponse) -> SnapshotMetadataRootStatus {
        match response {
            RpcResponse::Ok(RpcResult::SnapshotMetadataRootStatus(status)) => status,
            response => panic!("expected snapshot metadata root status, got {response:?}"),
        }
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
    fn persistent_node_gossips_signed_validator_set_metadata_update_authorization() {
        let validator_dir = temp_dir("gossip-validator-set-update-validator");
        let peer_dir = temp_dir("gossip-validator-set-update-peer");
        let validator_signing_key = validator_key("validator-1", 7);
        let peer_key = validator_key("validator-2", 8);
        let added_key = validator_key("validator-3", 9);
        let validator = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-1",
            seeded_state(),
            &validator_dir,
            "detta-testnet",
            vec![validator_signing_key.public_key(), peer_key.public_key()],
        )
        .unwrap();
        let peer = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-2",
            seeded_state(),
            &peer_dir,
            "detta-testnet",
            vec![validator_signing_key.public_key(), peer_key.public_key()],
        )
        .unwrap();
        let mut transport =
            InMemoryTransport::new(["validator-1".into(), "validator-2".into()]).unwrap();
        let update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-1".into(),
            add_validators: vec![added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: None,
        };

        assert_eq!(
            validator
                .gossip_signed_validator_set_metadata_update_authorization(
                    update.clone(),
                    &validator_signing_key,
                    &mut transport,
                )
                .unwrap(),
            1
        );

        let envelope = transport.drain_peer("validator-2").unwrap().pop().unwrap();
        let NetworkMessage::SignedValidator(signed) = &envelope.message else {
            panic!("expected signed validator metadata update authorization");
        };
        assert_eq!(
            peer.verified_validator_set_metadata_update(signed).unwrap(),
            update
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
    fn tcp_signed_votes_assemble_finality_certificate_at_quorum() {
        let proposer_dir = temp_dir("tcp-finality-proposer");
        let proposer_key = validator_key("validator-1", 7);
        let peer2_key = validator_key("validator-2", 8);
        let peer3_key = validator_key("validator-3", 9);
        let mut proposer =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &proposer_dir)
                .unwrap();
        proposer.set_network_id("detta-testnet");
        proposer.trust_validator_key(proposer_key.public_key());
        proposer.trust_validator_key(peer2_key.public_key());
        proposer.trust_validator_key(peer3_key.public_key());
        proposer.submit_transaction(transfer_tx()).unwrap();
        let block = proposer.produce_block(1, 1_000).unwrap();
        let block_hash = block.block_hash();
        let signed_block = proposer
            .sign_validator_message(
                &proposer_key,
                NetworkMessage::Block(Box::new(block.clone())),
            )
            .unwrap();

        let mut peers = Vec::new();
        for (peer_id, peer_key) in [
            ("validator-2".to_string(), peer2_key),
            ("validator-3".to_string(), peer3_key),
        ] {
            let peer_dir = temp_dir(&format!("tcp-finality-{peer_id}"));
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let proposer_public_key = proposer_key.public_key();
            let peer_block = block.clone();
            let handle = thread::spawn(move || {
                let mut peer =
                    PersistentValidatorNode::bootstrap(&peer_id, seeded_state(), &peer_dir)
                        .unwrap();
                peer.set_network_id("detta-testnet");
                peer.trust_validator_key(proposer_public_key);

                let (stream, _) = listener.accept().unwrap();
                let mut tcp = TcpProtocolStream::from_stream(stream);
                let envelope = Envelope {
                    from: "validator-1".into(),
                    to: peer_id.clone(),
                    message: tcp.receive().unwrap(),
                };
                assert_eq!(
                    peer.ingest_network_envelope(&envelope).unwrap(),
                    NetworkIngestOutcome::BlockImported
                );

                let vote = Vote {
                    validator_id: peer_id.clone(),
                    height: peer_block.header.height,
                    block_hash: peer_block.block_hash(),
                };
                let signed_vote = peer
                    .sign_validator_message(&peer_key, NetworkMessage::Vote(vote))
                    .unwrap();
                tcp.send(&signed_vote).unwrap();
                fs::remove_dir_all(peer_dir).unwrap();
            });
            peers.push((addr, handle));
        }

        let mut signed_votes = vec![proposer
            .sign_validator_message(
                &proposer_key,
                NetworkMessage::Vote(Vote {
                    validator_id: "validator-1".into(),
                    height: block.header.height,
                    block_hash: block_hash.clone(),
                }),
            )
            .unwrap()];

        for (addr, handle) in peers {
            let mut tcp = TcpProtocolStream::connect(addr).unwrap();
            tcp.send(&signed_block).unwrap();
            signed_votes.push(tcp.receive().unwrap());
            handle.join().unwrap();
        }

        let certificate = proposer
            .collect_finality_certificate(block.header.height, &block_hash, &signed_votes, 3)
            .unwrap();

        assert_eq!(certificate.height, block.header.height);
        assert_eq!(certificate.block_hash, block_hash);
        assert_eq!(
            certificate.signers,
            vec!["validator-1", "validator-2", "validator-3"]
        );

        fs::remove_dir_all(proposer_dir).unwrap();
    }

    #[test]
    fn tcp_validator_set_metadata_update_authorizations_apply_at_quorum() {
        let coordinator_dir = temp_dir("tcp-validator-set-update-coordinator");
        let coordinator_key = validator_key("validator-1", 7);
        let peer2_key = validator_key("validator-2", 8);
        let peer3_key = validator_key("validator-3", 9);
        let added_key = validator_key("validator-4", 10);
        let current_validator_keys = vec![
            coordinator_key.public_key(),
            peer2_key.public_key(),
            peer3_key.public_key(),
        ];
        let mut coordinator = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-1",
            seeded_state(),
            &coordinator_dir,
            "detta-testnet",
            current_validator_keys.clone(),
        )
        .unwrap();
        let update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-1".into(),
            add_validators: vec![added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: None,
        };
        let coordinator_authorization = coordinator
            .sign_validator_set_metadata_update_authorization(update.clone(), &coordinator_key)
            .unwrap();

        let mut peers = Vec::new();
        for (peer_id, peer_key) in [
            ("validator-2".to_string(), peer2_key),
            ("validator-3".to_string(), peer3_key),
        ] {
            let peer_dir = temp_dir(&format!("tcp-validator-set-update-{peer_id}"));
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let addr = listener.local_addr().unwrap();
            let trusted_validator_keys = current_validator_keys.clone();
            let expected_update = update.clone();
            let handle = thread::spawn(move || {
                let peer = PersistentValidatorNode::bootstrap_with_validator_set(
                    &peer_id,
                    seeded_state(),
                    &peer_dir,
                    "detta-testnet",
                    trusted_validator_keys,
                )
                .unwrap();

                let (stream, _) = listener.accept().unwrap();
                let mut tcp = TcpProtocolStream::from_stream(stream);
                let message = tcp.receive().unwrap();
                let NetworkMessage::SignedValidator(signed) = &message else {
                    panic!("expected signed validator metadata update authorization");
                };
                let received_update = peer.verified_validator_set_metadata_update(signed).unwrap();
                assert_eq!(received_update, expected_update);

                let signed_response = peer
                    .sign_validator_set_metadata_update_authorization(received_update, &peer_key)
                    .unwrap();
                tcp.send(&signed_response).unwrap();
                fs::remove_dir_all(peer_dir).unwrap();
            });
            peers.push((addr, handle));
        }

        let mut signed_updates = vec![coordinator_authorization.clone()];
        for (addr, handle) in peers {
            let mut tcp = TcpProtocolStream::connect(addr).unwrap();
            tcp.send(&coordinator_authorization).unwrap();
            signed_updates.push(tcp.receive().unwrap());
            handle.join().unwrap();
        }

        assert_eq!(coordinator.validator_set_metadata_quorum(), 3);
        coordinator
            .apply_quorum_authorized_validator_set_metadata_update(&signed_updates)
            .unwrap();
        assert!(coordinator.has_applied_validator_set_update("validator-set-update-1"));
        assert!(coordinator.trusted_validator_key("validator-4").is_some());

        let restarted = PersistentValidatorNode::restart("validator-1", &coordinator_dir).unwrap();
        assert!(restarted.has_applied_validator_set_update("validator-set-update-1"));
        assert!(restarted.trusted_validator_key("validator-4").is_some());

        fs::remove_dir_all(coordinator_dir).unwrap();
    }

    #[test]
    fn proposer_persists_and_gossips_signed_finality_certificate() {
        let proposer_dir = temp_dir("signed-certificate-proposer");
        let peer_dir = temp_dir("signed-certificate-peer");
        let proposer_key = validator_key("validator-1", 7);
        let mut proposer =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &proposer_dir)
                .unwrap();
        proposer.set_network_id("detta-testnet");
        let mut peer =
            PersistentValidatorNode::bootstrap("validator-2", seeded_state(), &peer_dir).unwrap();
        peer.set_network_id("detta-testnet");
        peer.trust_validator_key(proposer_key.public_key());
        let mut transport =
            InMemoryTransport::new(["validator-1".into(), "validator-2".into()]).unwrap();
        let certificate = FinalityCertificate {
            height: 3,
            block_hash: "block-hash-3".into(),
            signers: vec![
                "validator-1".into(),
                "validator-2".into(),
                "validator-3".into(),
            ],
        };

        assert_eq!(
            proposer
                .persist_and_gossip_signed_finality_certificate(
                    certificate.clone(),
                    &proposer_key,
                    &mut transport,
                )
                .unwrap(),
            1
        );

        assert_eq!(
            proposer.load_finality_certificate(3).unwrap(),
            certificate.clone()
        );
        let reloaded = PersistentValidatorNode::restart("validator-1", &proposer_dir).unwrap();
        assert_eq!(
            reloaded.load_finality_certificate(3).unwrap(),
            certificate.clone()
        );

        let envelope = transport.drain_peer("validator-2").unwrap().pop().unwrap();
        assert!(matches!(
            envelope.message,
            NetworkMessage::SignedValidator(_)
        ));
        assert_eq!(
            peer.ingest_network_envelope(&envelope).unwrap(),
            NetworkIngestOutcome::FinalityCertificateReceived
        );

        fs::remove_dir_all(proposer_dir).unwrap();
        fs::remove_dir_all(peer_dir).unwrap();
    }

    #[test]
    fn node_persists_and_gossips_signed_equivocation_evidence() {
        let reporter_dir = temp_dir("signed-evidence-reporter");
        let peer_dir = temp_dir("signed-evidence-peer");
        let reporter_key = validator_key("validator-2", 8);
        let mut reporter =
            PersistentValidatorNode::bootstrap("validator-2", seeded_state(), &reporter_dir)
                .unwrap();
        reporter.set_network_id("detta-testnet");
        let mut peer =
            PersistentValidatorNode::bootstrap("validator-3", seeded_state(), &peer_dir).unwrap();
        peer.set_network_id("detta-testnet");
        peer.trust_validator_key(reporter_key.public_key());
        let mut transport =
            InMemoryTransport::new(["validator-2".into(), "validator-3".into()]).unwrap();
        let evidence = EquivocationEvidence {
            validator_id: "validator-1".into(),
            height: 11,
            first_block_hash: "block-a".into(),
            second_block_hash: "block-b".into(),
        };

        assert_eq!(
            reporter
                .persist_and_gossip_signed_equivocation_evidence(
                    evidence.clone(),
                    &reporter_key,
                    &mut transport,
                )
                .unwrap(),
            1
        );
        assert_eq!(
            reporter
                .load_slashing_record("validator-1")
                .unwrap()
                .evidence,
            evidence
        );

        let envelope = transport.drain_peer("validator-3").unwrap().pop().unwrap();
        assert_eq!(
            peer.ingest_network_envelope(&envelope).unwrap(),
            NetworkIngestOutcome::EquivocationEvidencePersisted
        );
        assert_eq!(
            peer.load_slashing_record("validator-1").unwrap(),
            SlashingRecord {
                validator_id: "validator-1".into(),
                slashed_at_height: 11,
                evidence: EquivocationEvidence {
                    validator_id: "validator-1".into(),
                    height: 11,
                    first_block_hash: "block-a".into(),
                    second_block_hash: "block-b".into(),
                },
            }
        );

        let reloaded_peer = PersistentValidatorNode::restart("validator-3", &peer_dir).unwrap();
        assert_eq!(
            reloaded_peer
                .load_slashing_record("validator-1")
                .unwrap()
                .slashed_at_height,
            11
        );

        fs::remove_dir_all(reporter_dir).unwrap();
        fs::remove_dir_all(peer_dir).unwrap();
    }

    #[test]
    fn persistent_node_reloads_validator_set_keyring_on_restart() {
        let peer_dir = temp_dir("validator-set-restart");
        let key = validator_key("validator-1", 7);
        let node = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-2",
            seeded_state(),
            &peer_dir,
            "detta-testnet",
            vec![key.public_key()],
        )
        .unwrap();
        assert_eq!(node.network_id(), "detta-testnet");
        assert_eq!(node.validator_key_count(), 1);
        assert!(node.trusted_validator_key("validator-1").is_some());

        let mut restarted = PersistentValidatorNode::restart("validator-2", &peer_dir).unwrap();
        assert_eq!(restarted.network_id(), "detta-testnet");
        assert_eq!(restarted.validator_key_count(), 1);
        assert!(restarted.trusted_validator_key("validator-1").is_some());

        let vote = Vote {
            validator_id: "validator-1".into(),
            height: 1,
            block_hash: "block-a".into(),
        };
        let signed = key
            .sign_message(
                "detta-testnet",
                restarted.chain_id().clone(),
                NetworkMessage::Vote(vote),
            )
            .unwrap();
        let envelope = Envelope {
            from: "validator-1".into(),
            to: "validator-2".into(),
            message: NetworkMessage::SignedValidator(Box::new(signed)),
        };

        assert_eq!(
            restarted.ingest_network_envelope(&envelope).unwrap(),
            NetworkIngestOutcome::VoteReceived
        );

        fs::remove_dir_all(peer_dir).unwrap();
    }

    #[test]
    fn persistent_node_rejects_duplicate_validator_key_metadata() {
        let dir = temp_dir("validator-set-duplicate");
        let key = validator_key("validator-1", 7).public_key();
        let mut node =
            PersistentValidatorNode::bootstrap("validator-2", seeded_state(), &dir).unwrap();

        assert_eq!(
            node.persist_validator_set_metadata("detta-testnet", vec![key.clone(), key.clone()],)
                .unwrap_err(),
            NodeError::DuplicateValidatorKey("validator-1".into())
        );
        assert_eq!(node.validator_key_count(), 0);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_applies_quorum_authorized_validator_set_metadata_update_and_reloads() {
        let dir = temp_dir("validator-set-update");
        let signer_key = validator_key("validator-1", 7);
        let removed_key = validator_key("validator-2", 8);
        let added_key = validator_key("validator-3", 9);
        let mut node = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-2",
            seeded_state(),
            &dir,
            "detta-testnet",
            vec![signer_key.public_key(), removed_key.public_key()],
        )
        .unwrap();
        let update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-1".into(),
            add_validators: vec![added_key.public_key()],
            remove_validators: vec!["validator-2".into()],
            expires_at_height: None,
        };
        let signer_update = signer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(update.clone()),
            )
            .unwrap();
        let removed_validator_update = removed_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(update),
            )
            .unwrap();
        let signed_updates = vec![
            NetworkMessage::SignedValidator(Box::new(signer_update)),
            NetworkMessage::SignedValidator(Box::new(removed_validator_update)),
        ];

        assert_eq!(node.validator_set_metadata_quorum(), 2);
        node.apply_quorum_authorized_validator_set_metadata_update(&signed_updates)
            .unwrap();
        assert!(node.has_applied_validator_set_update("validator-set-update-1"));
        assert!(node.trusted_validator_key("validator-1").is_some());
        assert!(node.trusted_validator_key("validator-2").is_none());
        assert!(node.trusted_validator_key("validator-3").is_some());

        let restarted = PersistentValidatorNode::restart("validator-2", &dir).unwrap();
        assert_eq!(restarted.network_id(), "detta-testnet");
        assert!(restarted.has_applied_validator_set_update("validator-set-update-1"));
        assert!(restarted.trusted_validator_key("validator-2").is_none());
        assert!(restarted.trusted_validator_key("validator-3").is_some());

        assert_eq!(
            node.apply_quorum_authorized_validator_set_metadata_update(&signed_updates)
                .unwrap_err(),
            NodeError::ValidatorSetMetadataUpdateAlreadyApplied("validator-set-update-1".into())
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_stores_single_validator_set_metadata_update_without_quorum() {
        let dir = temp_dir("validator-set-single-update");
        let signer_key = validator_key("validator-1", 7);
        let peer_key = validator_key("validator-2", 8);
        let added_key = validator_key("validator-3", 9);
        let mut node = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-2",
            seeded_state(),
            &dir,
            "detta-testnet",
            vec![signer_key.public_key(), peer_key.public_key()],
        )
        .unwrap();
        let signed_update = signer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(ValidatorSetMetadataUpdate {
                    update_id: "validator-set-update-1".into(),
                    add_validators: vec![added_key.public_key()],
                    remove_validators: vec![],
                    expires_at_height: None,
                }),
            )
            .unwrap();
        let envelope = Envelope {
            from: "validator-1".into(),
            to: "validator-2".into(),
            message: NetworkMessage::SignedValidator(Box::new(signed_update)),
        };

        assert_eq!(
            node.ingest_network_envelope(&envelope).unwrap(),
            NetworkIngestOutcome::ValidatorSetMetadataAuthorizationStored
        );
        assert_eq!(
            node.pending_validator_set_metadata_authorization_count("validator-set-update-1"),
            1
        );
        assert!(!node.has_applied_validator_set_update("validator-set-update-1"));
        assert!(node.trusted_validator_key("validator-3").is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_reloads_pending_validator_set_metadata_authorizations() {
        let dir = temp_dir("validator-set-pending-reload");
        let signer_key = validator_key("validator-1", 7);
        let peer_key = validator_key("validator-2", 8);
        let added_key = validator_key("validator-3", 9);
        let mut node = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-2",
            seeded_state(),
            &dir,
            "detta-testnet",
            vec![signer_key.public_key(), peer_key.public_key()],
        )
        .unwrap();
        let update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-1".into(),
            add_validators: vec![added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: None,
        };
        let signer_authorization = signer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(update.clone()),
            )
            .unwrap();
        let signer_envelope = Envelope {
            from: "validator-1".into(),
            to: "validator-2".into(),
            message: NetworkMessage::SignedValidator(Box::new(signer_authorization)),
        };

        assert_eq!(
            node.ingest_network_envelope(&signer_envelope).unwrap(),
            NetworkIngestOutcome::ValidatorSetMetadataAuthorizationStored
        );

        let mut restarted = PersistentValidatorNode::restart("validator-2", &dir).unwrap();
        assert_eq!(
            restarted.pending_validator_set_metadata_authorization_count("validator-set-update-1"),
            1
        );
        assert!(restarted.trusted_validator_key("validator-3").is_none());

        let peer_authorization = peer_key
            .sign_message(
                "detta-testnet",
                restarted.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(update),
            )
            .unwrap();
        let peer_envelope = Envelope {
            from: "validator-2".into(),
            to: "validator-2".into(),
            message: NetworkMessage::SignedValidator(Box::new(peer_authorization)),
        };

        assert_eq!(
            restarted.ingest_network_envelope(&peer_envelope).unwrap(),
            NetworkIngestOutcome::ValidatorSetMetadataUpdated
        );
        assert_eq!(
            restarted.pending_validator_set_metadata_authorization_count("validator-set-update-1"),
            0
        );
        assert!(restarted.has_applied_validator_set_update("validator-set-update-1"));
        assert!(restarted.trusted_validator_key("validator-3").is_some());

        let reloaded = PersistentValidatorNode::restart("validator-2", &dir).unwrap();
        assert_eq!(
            reloaded.pending_validator_set_metadata_authorization_count("validator-set-update-1"),
            0
        );
        assert!(reloaded.has_applied_validator_set_update("validator-set-update-1"));
        assert!(reloaded.trusted_validator_key("validator-3").is_some());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_prunes_expired_validator_set_metadata_authorizations() {
        let dir = temp_dir("validator-set-expiry");
        let signer_key = validator_key("validator-1", 7);
        let peer_key = validator_key("validator-2", 8);
        let added_key = validator_key("validator-3", 9);
        let mut node = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-2",
            seeded_state(),
            &dir,
            "detta-testnet",
            vec![signer_key.public_key(), peer_key.public_key()],
        )
        .unwrap();
        let update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-1".into(),
            add_validators: vec![added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: Some(1),
        };
        let signer_authorization = signer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(update.clone()),
            )
            .unwrap();
        let signer_envelope = Envelope {
            from: "validator-1".into(),
            to: "validator-2".into(),
            message: NetworkMessage::SignedValidator(Box::new(signer_authorization)),
        };

        assert_eq!(
            node.ingest_network_envelope(&signer_envelope).unwrap(),
            NetworkIngestOutcome::ValidatorSetMetadataAuthorizationStored
        );
        assert_eq!(
            node.pending_validator_set_metadata_authorization_count("validator-set-update-1"),
            1
        );

        node.produce_block(1, 1_000).unwrap();
        assert_eq!(node.current_height(), 1);
        assert_eq!(
            node.prune_validator_set_metadata_authorizations().unwrap(),
            1
        );
        assert_eq!(
            node.pending_validator_set_metadata_authorization_count("validator-set-update-1"),
            0
        );

        let peer_authorization = peer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(update),
            )
            .unwrap();
        let peer_envelope = Envelope {
            from: "validator-2".into(),
            to: "validator-2".into(),
            message: NetworkMessage::SignedValidator(Box::new(peer_authorization)),
        };
        assert_eq!(
            node.ingest_network_envelope(&peer_envelope).unwrap_err(),
            NodeError::ValidatorSetMetadataUpdateExpired {
                update_id: "validator-set-update-1".into(),
                expires_at_height: 1,
                current_height: 1,
            }
        );
        assert!(node.trusted_validator_key("validator-3").is_none());

        let restarted = PersistentValidatorNode::restart("validator-2", &dir).unwrap();
        assert_eq!(
            restarted.pending_validator_set_metadata_authorization_count("validator-set-update-1"),
            0
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_bounds_pending_validator_set_metadata_update_pool() {
        let dir = temp_dir("validator-set-pool-limit");
        let signer_key = validator_key("validator-1", 7);
        let peer_key = validator_key("validator-2", 8);
        let added_key = validator_key("validator-3", 9);
        let other_added_key = validator_key("validator-4", 10);
        let mut node = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-2",
            seeded_state(),
            &dir,
            "detta-testnet",
            vec![signer_key.public_key(), peer_key.public_key()],
        )
        .unwrap();
        node.set_validator_set_metadata_authorization_limits(1, 10);
        let first_update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-1".into(),
            add_validators: vec![added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: None,
        };
        let second_update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-2".into(),
            add_validators: vec![other_added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: None,
        };
        let first_authorization = signer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(first_update),
            )
            .unwrap();
        let second_authorization = peer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(second_update),
            )
            .unwrap();

        assert_eq!(
            node.record_pending_validator_set_metadata_authorization(
                NetworkMessage::SignedValidator(Box::new(first_authorization)),
            )
            .unwrap(),
            1
        );
        assert_eq!(
            node.record_pending_validator_set_metadata_authorization(
                NetworkMessage::SignedValidator(Box::new(second_authorization)),
            )
            .unwrap_err(),
            NodeError::ValidatorSetMetadataAuthorizationPoolFull {
                pending_updates: 1,
                max_pending_updates: 1,
            }
        );
        assert_eq!(
            node.pending_validator_set_metadata_authorization_count("validator-set-update-2"),
            0
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_rate_limits_pending_validator_set_metadata_authorizations_by_signer() {
        let dir = temp_dir("validator-set-rate-limit");
        let signer_key = validator_key("validator-1", 7);
        let peer_key = validator_key("validator-2", 8);
        let added_key = validator_key("validator-3", 9);
        let other_added_key = validator_key("validator-4", 10);
        let mut node = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-2",
            seeded_state(),
            &dir,
            "detta-testnet",
            vec![signer_key.public_key(), peer_key.public_key()],
        )
        .unwrap();
        node.set_validator_set_metadata_authorization_limits(10, 1);
        let first_update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-1".into(),
            add_validators: vec![added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: None,
        };
        let second_update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-2".into(),
            add_validators: vec![other_added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: None,
        };
        let first_authorization = signer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(first_update.clone()),
            )
            .unwrap();
        let replacement_authorization = signer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(first_update),
            )
            .unwrap();
        let second_authorization = signer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(second_update),
            )
            .unwrap();

        assert_eq!(
            node.record_pending_validator_set_metadata_authorization(
                NetworkMessage::SignedValidator(Box::new(first_authorization)),
            )
            .unwrap(),
            1
        );
        assert_eq!(
            node.record_pending_validator_set_metadata_authorization(
                NetworkMessage::SignedValidator(Box::new(replacement_authorization)),
            )
            .unwrap(),
            1
        );
        assert_eq!(
            node.record_pending_validator_set_metadata_authorization(
                NetworkMessage::SignedValidator(Box::new(second_authorization)),
            )
            .unwrap_err(),
            NodeError::ValidatorSetMetadataAuthorizationRateLimited {
                signer: "validator-1".into(),
                pending_authorizations: 1,
                max_pending_authorizations: 1,
            }
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_records_validator_set_metadata_update_audit_records() {
        let dir = temp_dir("validator-set-audit");
        let signer_key = validator_key("validator-1", 7);
        let peer_key = validator_key("validator-2", 8);
        let added_key = validator_key("validator-3", 9);
        let pruned_added_key = validator_key("validator-4", 10);
        let rejected_added_key = validator_key("validator-5", 11);
        let mut node = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-2",
            seeded_state(),
            &dir,
            "detta-testnet",
            vec![signer_key.public_key(), peer_key.public_key()],
        )
        .unwrap();
        let applied_update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-applied".into(),
            add_validators: vec![added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: None,
        };
        for key in [&signer_key, &peer_key] {
            let authorization = key
                .sign_message(
                    "detta-testnet",
                    node.chain_id().clone(),
                    NetworkMessage::ValidatorSetMetadataUpdate(applied_update.clone()),
                )
                .unwrap();
            node.ingest_network_envelope(&Envelope {
                from: key.validator_id().into(),
                to: "validator-2".into(),
                message: NetworkMessage::SignedValidator(Box::new(authorization)),
            })
            .unwrap();
        }

        let pruned_update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-pruned".into(),
            add_validators: vec![pruned_added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: Some(1),
        };
        let pruned_authorization = signer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(pruned_update),
            )
            .unwrap();
        node.ingest_network_envelope(&Envelope {
            from: "validator-1".into(),
            to: "validator-2".into(),
            message: NetworkMessage::SignedValidator(Box::new(pruned_authorization)),
        })
        .unwrap();

        node.produce_block(1, 1_000).unwrap();
        assert_eq!(
            node.prune_validator_set_metadata_authorizations().unwrap(),
            1
        );

        let rejected_update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-rejected".into(),
            add_validators: vec![rejected_added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: Some(1),
        };
        let rejected_authorization = signer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(rejected_update),
            )
            .unwrap();
        assert!(matches!(
            node.ingest_network_envelope(&Envelope {
                from: "validator-1".into(),
                to: "validator-2".into(),
                message: NetworkMessage::SignedValidator(Box::new(rejected_authorization)),
            }),
            Err(NodeError::ValidatorSetMetadataUpdateExpired { .. })
        ));

        let expected_records = vec![
            ValidatorSetMetadataAuditRecord {
                update_id: "validator-set-update-applied".into(),
                outcome: ValidatorSetMetadataAuditOutcome::Applied,
                height: 0,
                signers: vec!["validator-1".into(), "validator-2".into()],
                reason: "applied".into(),
            },
            ValidatorSetMetadataAuditRecord {
                update_id: "validator-set-update-pruned".into(),
                outcome: ValidatorSetMetadataAuditOutcome::Pruned,
                height: 1,
                signers: vec!["validator-1".into()],
                reason: "expired".into(),
            },
            ValidatorSetMetadataAuditRecord {
                update_id: "validator-set-update-rejected".into(),
                outcome: ValidatorSetMetadataAuditOutcome::Rejected,
                height: 1,
                signers: vec!["validator-1".into()],
                reason: "node.validator_set_metadata_update_expired".into(),
            },
        ];

        assert_eq!(
            node.load_validator_set_metadata_audit_records().unwrap(),
            expected_records
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetValidatorSetMetadataAuditRecords {
                offset: 0,
                limit: 10,
            }),
            RpcResponse::Ok(RpcResult::ValidatorSetMetadataAuditRecords(
                expected_records
            ))
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_limits_validator_set_metadata_audit_retention_and_pages() {
        let dir = temp_dir("validator-set-audit-limits");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        node.set_validator_set_metadata_audit_limits(2, 1);
        let mut imported_roots = BTreeMap::new();
        imported_roots.insert(
            SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT.into(),
            "imported-audit-root".into(),
        );
        node.storage
            .commit_snapshot_metadata_roots(&imported_roots)
            .unwrap();
        assert_eq!(
            node.node_snapshot_roots()
                .unwrap()
                .validator_set_metadata_audit_root,
            "imported-audit-root"
        );

        for (update_id, outcome) in [
            (
                "validator-set-update-1",
                ValidatorSetMetadataAuditOutcome::Applied,
            ),
            (
                "validator-set-update-2",
                ValidatorSetMetadataAuditOutcome::Pruned,
            ),
            (
                "validator-set-update-3",
                ValidatorSetMetadataAuditOutcome::Rejected,
            ),
        ] {
            node.record_validator_set_metadata_audit(
                update_id.into(),
                outcome,
                vec!["validator-1".into()],
                "test".into(),
            )
            .unwrap();
        }

        let retained = vec![
            ValidatorSetMetadataAuditRecord {
                update_id: "validator-set-update-2".into(),
                outcome: ValidatorSetMetadataAuditOutcome::Pruned,
                height: 0,
                signers: vec!["validator-1".into()],
                reason: "test".into(),
            },
            ValidatorSetMetadataAuditRecord {
                update_id: "validator-set-update-3".into(),
                outcome: ValidatorSetMetadataAuditOutcome::Rejected,
                height: 0,
                signers: vec!["validator-1".into()],
                reason: "test".into(),
            },
        ];
        assert_eq!(
            node.load_validator_set_metadata_audit_records().unwrap(),
            retained
        );
        let retained_root = node.storage.validator_set_metadata_audit_root().unwrap();
        assert_ne!(retained_root, "imported-audit-root");
        assert_eq!(
            node.node_snapshot_roots()
                .unwrap()
                .validator_set_metadata_audit_root,
            retained_root
        );
        assert_eq!(
            node.storage
                .load_snapshot_metadata_roots()
                .unwrap()
                .get(SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT),
            Some(&retained_root)
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetSnapshotMetadataRootStatus),
            RpcResponse::Ok(RpcResult::SnapshotMetadataRootStatus(
                SnapshotMetadataRootStatus {
                    validator_set_metadata_audit_root: retained_root.clone(),
                    local_validator_set_metadata_audit_root: retained_root.clone(),
                    persisted_validator_set_metadata_audit_root: Some(retained_root.clone()),
                    using_imported_validator_set_metadata_audit_root: false,
                    persisted_matches_local_validator_set_metadata_audit_root: true,
                },
            ))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetValidatorSetMetadataAuditRecords {
                offset: 0,
                limit: 10,
            }),
            RpcResponse::Ok(RpcResult::ValidatorSetMetadataAuditRecords(vec![retained
                [0]
            .clone()]))
        );
        assert_eq!(
            node.load_validator_set_metadata_audit_records_page(1, 10)
                .unwrap(),
            vec![retained[1].clone()]
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_snapshot_includes_validator_set_metadata_audit_root() {
        let dir = temp_dir("validator-set-audit-root");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        let empty_snapshot = node.node_snapshot().unwrap();

        node.record_validator_set_metadata_audit(
            "validator-set-update-1".into(),
            ValidatorSetMetadataAuditOutcome::Applied,
            vec!["validator-1".into()],
            "applied".into(),
        )
        .unwrap();
        let populated_snapshot = node.node_snapshot().unwrap();

        assert_eq!(
            populated_snapshot.state_snapshot.global_state_root,
            empty_snapshot.state_snapshot.global_state_root
        );
        assert_ne!(
            populated_snapshot.validator_set_metadata_audit_root,
            empty_snapshot.validator_set_metadata_audit_root
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetPersistentNodeSnapshotRoots),
            RpcResponse::Ok(RpcResult::PersistentNodeSnapshotRoots(Box::new(
                persistent_node_snapshot_roots(&populated_snapshot)
            )))
        );

        let restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        assert_eq!(
            restarted
                .node_snapshot()
                .unwrap()
                .validator_set_metadata_audit_root,
            populated_snapshot.validator_set_metadata_audit_root
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_rpc_proposes_validator_set_metadata_update_and_reports_status() {
        let dir = temp_dir("validator-set-rpc");
        let signer_key = validator_key("validator-1", 7);
        let peer_key = validator_key("validator-2", 8);
        let added_key = validator_key("validator-3", 9);
        let mut node = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-2",
            seeded_state(),
            &dir,
            "detta-testnet",
            vec![signer_key.public_key(), peer_key.public_key()],
        )
        .unwrap();
        let update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-1".into(),
            add_validators: vec![added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: None,
        };
        let signer_authorization = signer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(update.clone()),
            )
            .unwrap();

        assert_eq!(
            node.handle_rpc_request(RpcRequest::ProposeValidatorSetMetadataUpdate {
                authorization: signer_authorization,
            }),
            RpcResponse::Ok(RpcResult::ValidatorSetMetadataUpdateStatus(
                ValidatorSetMetadataUpdateStatus {
                    update_id: "validator-set-update-1".into(),
                    pending_authorizations: 1,
                    required_quorum: 2,
                    applied: false,
                },
            ))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetValidatorSetMetadataUpdateStatus {
                update_id: "validator-set-update-1".into(),
            }),
            RpcResponse::Ok(RpcResult::ValidatorSetMetadataUpdateStatus(
                ValidatorSetMetadataUpdateStatus {
                    update_id: "validator-set-update-1".into(),
                    pending_authorizations: 1,
                    required_quorum: 2,
                    applied: false,
                },
            ))
        );

        let peer_authorization = peer_key
            .sign_message(
                "detta-testnet",
                node.chain_id().clone(),
                NetworkMessage::ValidatorSetMetadataUpdate(update),
            )
            .unwrap();
        assert_eq!(
            node.handle_rpc_request(RpcRequest::ProposeValidatorSetMetadataUpdate {
                authorization: peer_authorization,
            }),
            RpcResponse::Ok(RpcResult::ValidatorSetMetadataUpdateStatus(
                ValidatorSetMetadataUpdateStatus {
                    update_id: "validator-set-update-1".into(),
                    pending_authorizations: 0,
                    required_quorum: 3,
                    applied: true,
                },
            ))
        );
        assert!(node.trusted_validator_key("validator-3").is_some());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_json_rpc_tcp_serves_validator_set_metadata_methods() {
        let dir = temp_dir("validator-set-json-rpc");
        let signer_key = validator_key("validator-1", 7);
        let peer_key = validator_key("validator-2", 8);
        let added_key = validator_key("validator-3", 9);
        let update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-1".into(),
            add_validators: vec![added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: None,
        };
        let signer_authorization = signer_key
            .sign_message(
                "detta-testnet",
                "detta-local".to_string(),
                NetworkMessage::ValidatorSetMetadataUpdate(update.clone()),
            )
            .unwrap();
        let peer_authorization = peer_key
            .sign_message(
                "detta-testnet",
                "detta-local".to_string(),
                NetworkMessage::ValidatorSetMetadataUpdate(update),
            )
            .unwrap();
        let server = JsonRpcServer::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let mut sync_metrics = SnapshotSyncClientMetrics {
            retry_attempts: 1,
            stream_failures: 0,
            requests_sent: 3,
            manifests_received: 3,
            chunks_received: 5,
            resume_requests: 2,
            metadata_roots_verified: true,
            required_metadata_roots_root: None,
        };
        let mut required_metadata_roots = BTreeMap::new();
        required_metadata_roots.insert(
            SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT.into(),
            "required-validator-audit-root".into(),
        );
        required_metadata_roots.insert(
            SNAPSHOT_METADATA_STATE_SYNC_CLIENT_METRICS_ROOT.into(),
            "required-sync-metrics-root".into(),
        );
        let expected_required_metadata_roots = required_metadata_roots.clone();
        let expected_required_metadata_roots_root =
            FileStorage::required_snapshot_metadata_roots_root_for(
                &expected_required_metadata_roots,
            )
            .unwrap();
        sync_metrics.required_metadata_roots_root =
            Some(expected_required_metadata_roots_root.clone());
        let expected_sync_report = snapshot_sync_client_metrics_report(sync_metrics.clone());
        let snapshot_import_audit_record = SnapshotImportAuditRecord {
            snapshot_root: "snapshot-root-1".into(),
            manifest_hash: "manifest-hash-1".into(),
            required_metadata_roots_root: expected_required_metadata_roots_root.clone(),
            required_metadata_roots_count: expected_required_metadata_roots.len(),
            manifest_metadata_roots_count: expected_required_metadata_roots.len() + 1,
            chunk_count: 4,
            metadata_roots_verified: true,
        };
        let expected_snapshot_import_audit_records = vec![snapshot_import_audit_record.clone()];
        let expected_snapshot_import_audit_root =
            FileStorage::snapshot_import_audit_root_for(&expected_snapshot_import_audit_records)
                .unwrap();
        let handle = thread::spawn(move || {
            let mut node = PersistentValidatorNode::bootstrap_with_validator_set(
                "validator-2",
                seeded_state(),
                &dir,
                "detta-testnet",
                vec![signer_key.public_key(), peer_key.public_key()],
            )
            .unwrap();
            node.persist_snapshot_sync_client_metrics(&sync_metrics)
                .unwrap();
            node.storage
                .commit_required_snapshot_metadata_roots(&required_metadata_roots)
                .unwrap();
            node.storage
                .append_snapshot_import_audit_record(snapshot_import_audit_record)
                .unwrap();
            server
                .serve_next_connection_with_handler(&mut node)
                .unwrap();
            assert!(node.trusted_validator_key("validator-3").is_some());
            fs::remove_dir_all(dir).unwrap();
        });

        let mut stream = TcpStream::connect(addr).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        write_rpc_request(&mut stream, &RpcRequest::GetPersistentNodeSnapshotRoots);
        let initial_snapshot =
            expect_persistent_node_snapshot_roots(read_rpc_response(&mut reader));
        assert_eq!(
            initial_snapshot.required_snapshot_metadata_roots,
            expected_required_metadata_roots
        );
        assert_eq!(
            initial_snapshot.required_snapshot_metadata_roots_root,
            expected_required_metadata_roots_root
        );
        assert_eq!(
            initial_snapshot.snapshot_import_audit_root,
            expected_snapshot_import_audit_root
        );
        write_rpc_request(&mut stream, &RpcRequest::GetSnapshotSyncClientMetrics);
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::SnapshotSyncClientMetrics(Some(
                expected_sync_report
            )))
        );
        write_rpc_request(&mut stream, &RpcRequest::GetRequiredSnapshotMetadataRoots);
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::RequiredSnapshotMetadataRoots(
                RequiredSnapshotMetadataRootsReport {
                    roots: initial_snapshot.required_snapshot_metadata_roots.clone(),
                    root: initial_snapshot
                        .required_snapshot_metadata_roots_root
                        .clone(),
                }
            ))
        );
        write_rpc_request(
            &mut stream,
            &RpcRequest::GetSnapshotImportAuditRecords {
                offset: 0,
                limit: 10,
            },
        );
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::SnapshotImportAuditRecords(
                expected_snapshot_import_audit_records
            ))
        );
        write_rpc_request(&mut stream, &RpcRequest::GetSnapshotImportAuditRoot);
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::SnapshotImportAuditRoot(
                expected_snapshot_import_audit_root.clone()
            ))
        );

        write_rpc_request(
            &mut stream,
            &RpcRequest::ProposeValidatorSetMetadataUpdate {
                authorization: signer_authorization,
            },
        );
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::ValidatorSetMetadataUpdateStatus(
                ValidatorSetMetadataUpdateStatus {
                    update_id: "validator-set-update-1".into(),
                    pending_authorizations: 1,
                    required_quorum: 2,
                    applied: false,
                },
            ))
        );

        write_rpc_request(
            &mut stream,
            &RpcRequest::GetValidatorSetMetadataUpdateStatus {
                update_id: "validator-set-update-1".into(),
            },
        );
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::ValidatorSetMetadataUpdateStatus(
                ValidatorSetMetadataUpdateStatus {
                    update_id: "validator-set-update-1".into(),
                    pending_authorizations: 1,
                    required_quorum: 2,
                    applied: false,
                },
            ))
        );

        write_rpc_request(
            &mut stream,
            &RpcRequest::ProposeValidatorSetMetadataUpdate {
                authorization: peer_authorization,
            },
        );
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::ValidatorSetMetadataUpdateStatus(
                ValidatorSetMetadataUpdateStatus {
                    update_id: "validator-set-update-1".into(),
                    pending_authorizations: 0,
                    required_quorum: 3,
                    applied: true,
                },
            ))
        );

        write_rpc_request(&mut stream, &RpcRequest::GetPersistentNodeSnapshotRoots);
        let updated_snapshot =
            expect_persistent_node_snapshot_roots(read_rpc_response(&mut reader));
        assert_eq!(
            updated_snapshot.global_state_root,
            initial_snapshot.global_state_root
        );
        assert_ne!(
            updated_snapshot.validator_set_metadata_audit_root,
            initial_snapshot.validator_set_metadata_audit_root
        );
        write_rpc_request(&mut stream, &RpcRequest::GetSnapshotMetadataRootStatus);
        let status = expect_snapshot_metadata_root_status(read_rpc_response(&mut reader));
        assert_eq!(
            status.validator_set_metadata_audit_root,
            updated_snapshot.validator_set_metadata_audit_root
        );
        assert_eq!(
            status.persisted_validator_set_metadata_audit_root,
            Some(updated_snapshot.validator_set_metadata_audit_root)
        );
        assert!(!status.using_imported_validator_set_metadata_audit_root);
        assert!(status.persisted_matches_local_validator_set_metadata_audit_root);

        stream.shutdown(Shutdown::Write).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn persistent_node_rejects_mismatched_validator_set_metadata_update_quorum() {
        let dir = temp_dir("validator-set-mismatch");
        let signer_key = validator_key("validator-1", 7);
        let peer_key = validator_key("validator-2", 8);
        let added_key = validator_key("validator-3", 9);
        let other_added_key = validator_key("validator-4", 10);
        let mut node = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-2",
            seeded_state(),
            &dir,
            "detta-testnet",
            vec![signer_key.public_key(), peer_key.public_key()],
        )
        .unwrap();
        let first_update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-1".into(),
            add_validators: vec![added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: None,
        };
        let conflicting_update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-1".into(),
            add_validators: vec![other_added_key.public_key()],
            remove_validators: vec![],
            expires_at_height: None,
        };
        let signed_updates = vec![
            NetworkMessage::SignedValidator(Box::new(
                signer_key
                    .sign_message(
                        "detta-testnet",
                        node.chain_id().clone(),
                        NetworkMessage::ValidatorSetMetadataUpdate(first_update),
                    )
                    .unwrap(),
            )),
            NetworkMessage::SignedValidator(Box::new(
                peer_key
                    .sign_message(
                        "detta-testnet",
                        node.chain_id().clone(),
                        NetworkMessage::ValidatorSetMetadataUpdate(conflicting_update),
                    )
                    .unwrap(),
            )),
        ];

        assert_eq!(
            node.apply_quorum_authorized_validator_set_metadata_update(&signed_updates)
                .unwrap_err(),
            NodeError::ValidatorSetMetadataUpdateMismatch
        );
        assert!(!node.has_applied_validator_set_update("validator-set-update-1"));
        assert!(node.trusted_validator_key("validator-3").is_none());
        assert!(node.trusted_validator_key("validator-4").is_none());

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_rejects_unsigned_validator_set_metadata_update() {
        let dir = temp_dir("validator-set-unsigned-update");
        let added_key = validator_key("validator-3", 9);
        let mut node =
            PersistentValidatorNode::bootstrap("validator-2", seeded_state(), &dir).unwrap();
        let envelope = Envelope {
            from: "validator-1".into(),
            to: "validator-2".into(),
            message: NetworkMessage::ValidatorSetMetadataUpdate(ValidatorSetMetadataUpdate {
                update_id: "validator-set-update-1".into(),
                add_validators: vec![added_key.public_key()],
                remove_validators: vec![],
                expires_at_height: None,
            }),
        };

        assert_eq!(
            node.ingest_network_envelope(&envelope).unwrap_err(),
            NodeError::UnsignedValidatorSetMetadataUpdate
        );
        assert!(node.trusted_validator_key("validator-3").is_none());

        fs::remove_dir_all(dir).unwrap();
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
        node.record_validator_set_metadata_audit(
            "validator-set-update-1".into(),
            ValidatorSetMetadataAuditOutcome::Applied,
            vec!["validator-1".into()],
            "applied".into(),
        )
        .unwrap();
        let snapshot_root = node.rpc().get_state_root();
        let expected_audit_root = node
            .node_snapshot()
            .unwrap()
            .validator_set_metadata_audit_root;
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
                    assert_eq!(
                        next_manifest
                            .metadata_roots
                            .get(SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT),
                        Some(&expected_audit_root)
                    );
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

        let mut required_metadata_roots = BTreeMap::new();
        required_metadata_roots.insert(
            SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT.into(),
            expected_audit_root.clone(),
        );
        let sink_dir = temp_dir("state-sync-sink");
        let mut sink = PersistentValidatorNode::bootstrap(
            "validator-2",
            DeTTaState::new("detta-local"),
            &sink_dir,
        )
        .unwrap();
        let imported = sink
            .import_snapshot_chunk_set(&chunk_set, &required_metadata_roots)
            .unwrap();
        assert_eq!(imported.global_state_root, snapshot_root);
        let sink_local_audit_root = sink.storage.validator_set_metadata_audit_root().unwrap();
        assert_eq!(
            sink.handle_rpc_request(RpcRequest::GetSnapshotMetadataRootStatus),
            RpcResponse::Ok(RpcResult::SnapshotMetadataRootStatus(
                SnapshotMetadataRootStatus {
                    validator_set_metadata_audit_root: expected_audit_root.clone(),
                    local_validator_set_metadata_audit_root: sink_local_audit_root,
                    persisted_validator_set_metadata_audit_root: Some(expected_audit_root.clone()),
                    using_imported_validator_set_metadata_audit_root: true,
                    persisted_matches_local_validator_set_metadata_audit_root: false,
                },
            ))
        );
        assert_eq!(
            sink.node_snapshot_roots()
                .unwrap()
                .validator_set_metadata_audit_root,
            expected_audit_root
        );

        let restarted_sink = PersistentValidatorNode::restart("validator-2", &sink_dir).unwrap();
        assert_eq!(
            restarted_sink
                .node_snapshot_roots()
                .unwrap()
                .validator_set_metadata_audit_root,
            expected_audit_root
        );
        let response = restarted_sink
            .serve_snapshot_chunk_request(
                &SnapshotChunkRequest {
                    snapshot_root: snapshot_root.clone(),
                    start_index: 0,
                    max_chunks: 1,
                },
                64,
            )
            .unwrap();
        match &response[0] {
            NetworkMessage::SnapshotChunkManifest(manifest) => {
                assert_eq!(
                    manifest
                        .metadata_roots
                        .get(SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT),
                    Some(&expected_audit_root)
                );
            }
            message => panic!("expected snapshot manifest, got {message:?}"),
        }

        let mut wrong_metadata_roots = required_metadata_roots;
        wrong_metadata_roots.insert(
            SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT.into(),
            "wrong-audit-root".into(),
        );
        assert_eq!(
            sink.import_snapshot_chunk_set(&chunk_set, &wrong_metadata_roots)
                .unwrap_err(),
            NodeError::SnapshotSync(SnapshotSyncError::MetadataRootMismatch {
                key: SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT.into(),
                expected: "wrong-audit-root".into(),
                actual: Some(expected_audit_root),
            })
        );

        fs::remove_dir_all(sink_dir).unwrap();
        fs::remove_dir_all(node_dir).unwrap();
    }

    #[test]
    fn tcp_state_sync_client_requires_snapshot_metadata_root_status() {
        let source_dir = temp_dir("tcp-state-sync-source");
        let source =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &source_dir).unwrap();
        source
            .record_validator_set_metadata_audit(
                "validator-set-update-1".into(),
                ValidatorSetMetadataAuditOutcome::Applied,
                vec!["validator-1".into()],
                "applied".into(),
            )
            .unwrap();
        let source_metrics = SnapshotSyncClientMetrics {
            retry_attempts: 1,
            stream_failures: 0,
            requests_sent: 2,
            manifests_received: 2,
            chunks_received: 4,
            resume_requests: 1,
            metadata_roots_verified: true,
            required_metadata_roots_root: None,
        };
        source
            .persist_snapshot_sync_client_metrics(&source_metrics)
            .unwrap();
        let snapshot_root = source.rpc().get_state_root();
        let status = source.snapshot_metadata_root_status().unwrap();
        let required_metadata_roots = source.snapshot_metadata_roots().unwrap();
        let expected_required_metadata_roots_root =
            FileStorage::required_snapshot_metadata_roots_root_for(&required_metadata_roots)
                .unwrap();
        let expected_metrics_root = required_metadata_roots
            .get(SNAPSHOT_METADATA_STATE_SYNC_CLIENT_METRICS_ROOT)
            .cloned()
            .unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut tcp = TcpProtocolStream::from_stream(stream);
            while let Ok(message) = tcp.receive() {
                match message {
                    NetworkMessage::SnapshotChunkRequest(request) => {
                        for response in source.serve_snapshot_chunk_request(&request, 64).unwrap() {
                            tcp.send(&response).unwrap();
                        }
                    }
                    message => panic!("expected snapshot chunk request, got {message:?}"),
                }
            }
            fs::remove_dir_all(source_dir).unwrap();
        });

        let mut client = TcpProtocolStream::connect(addr).unwrap();
        let (chunk_set, metrics) = fetch_verified_snapshot_chunk_set_over_tcp_with_metrics(
            &mut client,
            snapshot_root.clone(),
            2,
            &required_metadata_roots,
        )
        .unwrap();
        assert_eq!(
            chunk_set
                .manifest
                .metadata_roots
                .get(SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT),
            Some(&status.validator_set_metadata_audit_root)
        );
        assert_eq!(
            chunk_set
                .manifest
                .metadata_roots
                .get(SNAPSHOT_METADATA_STATE_SYNC_CLIENT_METRICS_ROOT),
            Some(&expected_metrics_root)
        );
        assert_eq!(metrics.chunks_received, chunk_set.chunks.len() as u32);
        assert_eq!(metrics.manifests_received, metrics.requests_sent);
        assert_eq!(
            metrics.resume_requests,
            metrics.requests_sent.saturating_sub(1)
        );
        assert!(metrics.metadata_roots_verified);
        assert_eq!(
            metrics.required_metadata_roots_root.as_deref(),
            Some(expected_required_metadata_roots_root.as_str())
        );

        let sink_dir = temp_dir("tcp-state-sync-sink");
        let mut sink = PersistentValidatorNode::bootstrap(
            "validator-2",
            DeTTaState::new("detta-local"),
            &sink_dir,
        )
        .unwrap();
        let imported = sink
            .import_snapshot_chunk_set(&chunk_set, &required_metadata_roots)
            .unwrap();
        assert_eq!(imported.global_state_root, snapshot_root);
        assert_eq!(
            sink.load_required_snapshot_metadata_roots().unwrap(),
            required_metadata_roots
        );
        let required_metadata_roots_root = sink.required_snapshot_metadata_roots_root().unwrap();
        assert_eq!(
            required_metadata_roots_root,
            expected_required_metadata_roots_root
        );
        let expected_import_audit_record = SnapshotImportAuditRecord {
            snapshot_root: snapshot_root.clone(),
            manifest_hash: chunk_set.manifest.manifest_hash().unwrap(),
            required_metadata_roots_root: required_metadata_roots_root.clone(),
            required_metadata_roots_count: required_metadata_roots.len(),
            manifest_metadata_roots_count: chunk_set.manifest.metadata_roots.len(),
            chunk_count: chunk_set.manifest.chunk_count,
            metadata_roots_verified: true,
        };
        assert_eq!(
            sink.load_snapshot_import_audit_records().unwrap(),
            vec![expected_import_audit_record.clone()]
        );
        let snapshot_import_audit_root = sink.snapshot_import_audit_root().unwrap();
        assert_eq!(
            sink.handle_rpc_request(RpcRequest::GetSnapshotImportAuditRecords {
                offset: 0,
                limit: 10,
            }),
            RpcResponse::Ok(RpcResult::SnapshotImportAuditRecords(vec![
                expected_import_audit_record.clone()
            ]))
        );
        assert_eq!(
            sink.handle_rpc_request(RpcRequest::GetSnapshotImportAuditRoot),
            RpcResponse::Ok(RpcResult::SnapshotImportAuditRoot(
                snapshot_import_audit_root.clone()
            ))
        );
        assert_eq!(
            sink.handle_rpc_request(RpcRequest::GetRequiredSnapshotMetadataRoots),
            RpcResponse::Ok(RpcResult::RequiredSnapshotMetadataRoots(
                RequiredSnapshotMetadataRootsReport {
                    roots: required_metadata_roots.clone(),
                    root: required_metadata_roots_root.clone(),
                }
            ))
        );
        let mut wrong_diagnostics_roots = required_metadata_roots.clone();
        wrong_diagnostics_roots.insert(
            SNAPSHOT_METADATA_STATE_SYNC_CLIENT_METRICS_ROOT.into(),
            "wrong-diagnostics-root".into(),
        );
        assert_eq!(
            sink.import_snapshot_chunk_set(&chunk_set, &wrong_diagnostics_roots)
                .unwrap_err(),
            NodeError::SnapshotSync(SnapshotSyncError::MetadataRootMismatch {
                key: SNAPSHOT_METADATA_STATE_SYNC_CLIENT_METRICS_ROOT.into(),
                expected: "wrong-diagnostics-root".into(),
                actual: Some(expected_metrics_root),
            })
        );
        assert_eq!(
            sink.load_snapshot_import_audit_records().unwrap(),
            vec![expected_import_audit_record.clone()]
        );
        assert_eq!(
            sink.snapshot_import_audit_root().unwrap(),
            snapshot_import_audit_root
        );
        assert_eq!(
            sink.handle_rpc_request(RpcRequest::GetSnapshotSyncClientMetrics),
            RpcResponse::Ok(RpcResult::SnapshotSyncClientMetrics(None))
        );
        let expected_metrics_report = snapshot_sync_client_metrics_report(metrics.clone());
        sink.persist_snapshot_sync_client_metrics(&metrics).unwrap();
        let expected_metrics_root = sink
            .storage
            .snapshot_sync_client_metrics_root::<SnapshotSyncClientMetrics>()
            .unwrap()
            .unwrap();
        assert_eq!(
            sink.load_snapshot_sync_client_metrics().unwrap(),
            Some(metrics.clone())
        );
        assert_eq!(
            sink.handle_rpc_request(RpcRequest::GetSnapshotSyncClientMetrics),
            RpcResponse::Ok(RpcResult::SnapshotSyncClientMetrics(Some(
                expected_metrics_report.clone()
            )))
        );
        let sink_roots = sink.node_snapshot_roots().unwrap();
        assert_eq!(
            sink_roots.validator_set_metadata_audit_root,
            status.validator_set_metadata_audit_root
        );
        assert_eq!(
            sink_roots.snapshot_sync_client_metrics,
            Some(expected_metrics_report.clone())
        );
        assert_eq!(
            sink_roots.required_snapshot_metadata_roots,
            required_metadata_roots
        );
        assert_eq!(
            sink_roots.required_snapshot_metadata_roots_root,
            required_metadata_roots_root
        );
        assert_eq!(
            sink_roots.snapshot_import_audit_root,
            snapshot_import_audit_root
        );
        let response = sink
            .serve_snapshot_chunk_request(
                &SnapshotChunkRequest {
                    snapshot_root: snapshot_root.clone(),
                    start_index: 0,
                    max_chunks: 1,
                },
                64,
            )
            .unwrap();
        match &response[0] {
            NetworkMessage::SnapshotChunkManifest(manifest) => {
                assert_eq!(
                    manifest
                        .metadata_roots
                        .get(SNAPSHOT_METADATA_STATE_SYNC_CLIENT_METRICS_ROOT),
                    Some(&expected_metrics_root)
                );
                assert_eq!(
                    manifest
                        .metadata_roots
                        .get(SNAPSHOT_METADATA_REQUIRED_METADATA_ROOTS_ROOT),
                    Some(&required_metadata_roots_root)
                );
                assert_eq!(
                    manifest
                        .metadata_roots
                        .get(SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_ROOT),
                    Some(&snapshot_import_audit_root)
                );
            }
            message => panic!("expected snapshot manifest, got {message:?}"),
        }

        let mut restarted_sink =
            PersistentValidatorNode::restart("validator-2", &sink_dir).unwrap();
        assert_eq!(
            restarted_sink.load_snapshot_sync_client_metrics().unwrap(),
            Some(metrics.clone())
        );
        assert_eq!(
            restarted_sink.load_snapshot_import_audit_records().unwrap(),
            vec![expected_import_audit_record]
        );
        assert_eq!(
            restarted_sink.handle_rpc_request(RpcRequest::GetSnapshotImportAuditRoot),
            RpcResponse::Ok(RpcResult::SnapshotImportAuditRoot(
                snapshot_import_audit_root.clone()
            ))
        );
        assert_eq!(
            restarted_sink
                .node_snapshot_roots()
                .unwrap()
                .snapshot_import_audit_root,
            snapshot_import_audit_root
        );
        assert_eq!(
            restarted_sink
                .load_required_snapshot_metadata_roots()
                .unwrap(),
            required_metadata_roots
        );
        assert_eq!(
            restarted_sink.handle_rpc_request(RpcRequest::GetRequiredSnapshotMetadataRoots),
            RpcResponse::Ok(RpcResult::RequiredSnapshotMetadataRoots(
                RequiredSnapshotMetadataRootsReport {
                    roots: required_metadata_roots.clone(),
                    root: required_metadata_roots_root.clone(),
                }
            ))
        );
        assert_eq!(
            restarted_sink.handle_rpc_request(RpcRequest::GetSnapshotSyncClientMetrics),
            RpcResponse::Ok(RpcResult::SnapshotSyncClientMetrics(Some(
                expected_metrics_report.clone()
            )))
        );
        assert_eq!(
            restarted_sink
                .node_snapshot_roots()
                .unwrap()
                .snapshot_sync_client_metrics,
            Some(expected_metrics_report)
        );
        assert_eq!(
            restarted_sink
                .node_snapshot_roots()
                .unwrap()
                .required_snapshot_metadata_roots,
            required_metadata_roots
        );
        assert_eq!(
            restarted_sink
                .node_snapshot_roots()
                .unwrap()
                .required_snapshot_metadata_roots_root,
            required_metadata_roots_root
        );

        drop(client);
        handle.join().unwrap();
        fs::remove_dir_all(sink_dir).unwrap();
    }

    #[test]
    fn tcp_state_sync_client_retries_transient_chunk_stream_failure() {
        fn audited_source(name: &str) -> (PathBuf, PersistentValidatorNode, String, String) {
            let dir = temp_dir(name);
            let node =
                PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
            node.record_validator_set_metadata_audit(
                "validator-set-update-1".into(),
                ValidatorSetMetadataAuditOutcome::Applied,
                vec!["validator-1".into()],
                "applied".into(),
            )
            .unwrap();
            let snapshot_root = node.rpc().get_state_root();
            let audit_root = node
                .snapshot_metadata_root_status()
                .unwrap()
                .validator_set_metadata_audit_root;
            (dir, node, snapshot_root, audit_root)
        }

        let (failing_dir, failing_source, snapshot_root, audit_root) =
            audited_source("tcp-state-sync-failing-source");
        let (healthy_dir, healthy_source, healthy_snapshot_root, healthy_audit_root) =
            audited_source("tcp-state-sync-healthy-source");
        assert_eq!(healthy_snapshot_root, snapshot_root);
        assert_eq!(healthy_audit_root, audit_root);
        let mut required_metadata_roots = BTreeMap::new();
        required_metadata_roots.insert(
            SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT.into(),
            audit_root.clone(),
        );
        let required_metadata_roots_root =
            FileStorage::required_snapshot_metadata_roots_root_for(&required_metadata_roots)
                .unwrap();

        let failing_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let failing_addr = failing_listener.local_addr().unwrap();
        let failing_handle = thread::spawn(move || {
            let (stream, _) = failing_listener.accept().unwrap();
            let mut tcp = TcpProtocolStream::from_stream(stream);
            match tcp.receive().unwrap() {
                NetworkMessage::SnapshotChunkRequest(request) => {
                    let responses = failing_source
                        .serve_snapshot_chunk_request(&request, 64)
                        .unwrap();
                    tcp.send(&responses[0]).unwrap();
                }
                message => panic!("expected snapshot chunk request, got {message:?}"),
            }
            fs::remove_dir_all(failing_dir).unwrap();
        });

        let healthy_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let healthy_addr = healthy_listener.local_addr().unwrap();
        let healthy_handle = thread::spawn(move || {
            let (stream, _) = healthy_listener.accept().unwrap();
            let mut tcp = TcpProtocolStream::from_stream(stream);
            while let Ok(message) = tcp.receive() {
                match message {
                    NetworkMessage::SnapshotChunkRequest(request) => {
                        for response in healthy_source
                            .serve_snapshot_chunk_request(&request, 64)
                            .unwrap()
                        {
                            tcp.send(&response).unwrap();
                        }
                    }
                    message => panic!("expected snapshot chunk request, got {message:?}"),
                }
            }
            fs::remove_dir_all(healthy_dir).unwrap();
        });

        let mut addrs = [failing_addr, healthy_addr].into_iter();
        let (chunk_set, metrics) = fetch_verified_snapshot_chunk_set_over_tcp_with_retries(
            || TcpProtocolStream::connect(addrs.next().unwrap()),
            snapshot_root.clone(),
            2,
            &required_metadata_roots,
            SnapshotSyncRetryPolicy { max_attempts: 2 },
        )
        .unwrap();
        assert_eq!(chunk_set.manifest.snapshot_root, snapshot_root);
        assert_eq!(metrics.retry_attempts, 2);
        assert_eq!(metrics.stream_failures, 1);
        assert!(metrics.requests_sent > 0);
        assert!(metrics.metadata_roots_verified);
        assert_eq!(
            metrics.required_metadata_roots_root.as_deref(),
            Some(required_metadata_roots_root.as_str())
        );

        failing_handle.join().unwrap();
        healthy_handle.join().unwrap();
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
