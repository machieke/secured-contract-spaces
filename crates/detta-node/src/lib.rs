use detta_consensus::{
    quorum_for, validate_da_slashing_evidence, ConsensusCluster, ConsensusError,
    EquivocationEvidence, FinalityCertificate, SlashingEvidence, SlashingRecord, Vote,
};
use detta_core::{
    transaction_resource_units, Argument, AspectModuleRecord, AspectModuleRoots, Block, BlockError,
    ChainId, DataAvailabilityCommitment, DeTTaState, MempoolError, Method, StateSnapshot,
    Transaction, TxStatus, ValidatorNode,
};
use detta_da::{
    derive_application_sample_schedule, derive_sample_schedule, prove_application_namespace,
    prove_application_share_inclusion, prove_namespace, prove_share_inclusion,
    validate_production_block_payload, verify_application_light_client_samples,
    verify_light_client_samples, verify_share_against_manifest, ApplicationDaNamespaceSection,
    ApplicationDaPayload, ApplicationDaSampleProofBundle, ApplicationDaShareSet,
    DaApplicationProfile, DaAvailabilityCertificate, DaAvailabilityVote, DaChallengeEvidence,
    DaChallengeRecord, DaCodingFraudProof, DaError, DaManifest, DaNamespace, DaNamespaceSection,
    DaPayload, DaProductionProfile, DaRecord, DaSampleProof, DaSampleProofBundle, DaShare,
    DaShareChallenge, DaShareChallengeResponse, DaShareSet,
};
use detta_network::{Envelope, InMemoryTransport, NetworkError, NetworkMessage, TcpProtocolStream};
use detta_protocol::{
    build_snapshot_chunks_with_metadata_roots, DaShareRequest, ProtocolMessageKind, SignatureError,
    SignedValidatorMessage, SnapshotChunk, SnapshotChunkManifest, SnapshotChunkRequest,
    SnapshotChunkSet, SnapshotSyncError, ValidatorPublicKey, ValidatorSetMetadata,
    ValidatorSetMetadataUpdate, ValidatorSignatureDomain, ValidatorSigningKey,
    SNAPSHOT_METADATA_REQUIRED_METADATA_ROOTS_ROOT,
    SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_CONFIG_ROOT,
    SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_ROOT, SNAPSHOT_METADATA_STATE_SYNC_CLIENT_METRICS_ROOT,
    SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT,
};
use detta_rpc::{
    json_rpc_response_for_request, ApplicationDaStatusReport, BlockPage, DaCodingFraudReport,
    DaRepairStatusReport, DaStatusReport, JsonRpcHandler, NodeHealthReport, OperatorAlert,
    OperatorAlertPolicy, OperatorAlertReport, OperatorAlertSeverity, OperatorMetricsReport,
    PersistentNodeSnapshotRoots, RequiredSnapshotMetadataRootsReport, RpcError, RpcErrorBody,
    RpcRequest, RpcResponse, RpcResult, RpcService, RpcTransportError, SnapshotMetadataRootStatus,
    SnapshotSyncClientMetricsReport, ValidatorSetMetadataUpdateStatus, DEFAULT_MAX_BLOCK_PAGE_SIZE,
};
use detta_storage::{
    ConsensusSigningRecord, DaRetentionAuditReport, DaRetentionPolicyConfig,
    DaRetentionPrunePlanReport, FileStorage, SnapshotImportAuditConfig, SnapshotImportAuditRecord,
    StorageError, ValidatorSetMetadataAuditOutcome, ValidatorSetMetadataAuditRecord,
};
use serde::{Deserialize, Serialize};
use std::collections::{btree_map::Entry, BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::Instant;

pub const DEFAULT_NODE_NETWORK_ID: &str = "detta-localnet";
pub const DEFAULT_MAX_PENDING_VALIDATOR_SET_METADATA_UPDATES: usize = 128;
pub const DEFAULT_MAX_PENDING_VALIDATOR_SET_METADATA_AUTHORIZATIONS_PER_VALIDATOR: usize = 32;
pub const DEFAULT_MAX_VALIDATOR_SET_METADATA_AUDIT_RECORDS: usize = 4_096;
pub const DEFAULT_MAX_VALIDATOR_SET_METADATA_AUDIT_PAGE_SIZE: usize = 100;
pub const DEFAULT_MAX_SNAPSHOT_IMPORT_AUDIT_RECORDS: usize = 4_096;
pub const DEFAULT_MAX_SNAPSHOT_IMPORT_AUDIT_PAGE_SIZE: usize = 100;
pub const DEFAULT_MAX_DA_SHARES_PER_REQUEST: u32 = 128;
pub const DEFAULT_MAX_DA_SHARE_REQUESTS_PER_PEER: u32 = 64;

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
    DataAvailability(DaError),
    SnapshotRootNotFound {
        requested: String,
        available: String,
    },
    SigningKeyMismatch {
        expected: String,
        actual: String,
    },
    ConsensusMessageSignerMismatch {
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
    ConsensusSigningConflict {
        validator_id: String,
        domain: ValidatorSignatureDomain,
        height: u64,
        existing_block_hash: String,
        attempted_block_hash: String,
    },
    BlockProposalRejected(String),
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
    DataAvailabilityStored,
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
    max_snapshot_import_audit_records: usize,
    max_snapshot_import_audit_page_size: usize,
    observed_peer_ids: BTreeSet<String>,
    last_block_execution_micros: Option<u64>,
    last_proof_serving_micros: Option<u64>,
    rpc_error_count: u64,
    operator_alert_policy: OperatorAlertPolicy,
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

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaShareSyncClientMetrics {
    pub peer_attempts: u32,
    pub peer_failures: u32,
    pub requests_sent: u32,
    pub manifests_received: u32,
    pub shares_received: u32,
    pub duplicate_shares: u32,
    pub invalid_responses: u32,
    pub payload_reconstructable: bool,
    pub peer_scores: Vec<DaShareSyncPeerScore>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaShareSyncPeerScore {
    pub peer_index: usize,
    pub score: i64,
    pub valid_responses: u32,
    pub invalid_responses: u32,
    pub failures: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaShareRequestRateLimiter {
    max_requests_per_peer: u32,
    requests_by_peer: BTreeMap<String, u32>,
}

impl DaShareRequestRateLimiter {
    pub fn new(max_requests_per_peer: u32) -> Result<Self, DaError> {
        if max_requests_per_peer == 0 {
            return Err(DaError::InvalidPayload(
                "DA share request rate limit must be positive".into(),
            ));
        }
        Ok(Self {
            max_requests_per_peer,
            requests_by_peer: BTreeMap::new(),
        })
    }

    pub fn admit(&mut self, peer_id: &str) -> Result<(), DaError> {
        if peer_id.is_empty() {
            return Err(DaError::InvalidPayload(
                "DA share request peer ID must be nonempty".into(),
            ));
        }
        let requests = self
            .requests_by_peer
            .entry(peer_id.to_string())
            .or_default();
        if *requests >= self.max_requests_per_peer {
            return Err(DaError::InvalidPayload(format!(
                "DA share request rate limit exceeded for peer {peer_id}"
            )));
        }
        *requests = requests.saturating_add(1);
        Ok(())
    }

    pub fn request_count(&self, peer_id: &str) -> u32 {
        self.requests_by_peer.get(peer_id).copied().unwrap_or(0)
    }
}

impl Default for DaShareRequestRateLimiter {
    fn default() -> Self {
        Self::new(DEFAULT_MAX_DA_SHARE_REQUESTS_PER_PEER)
            .expect("default DA share request limit is positive")
    }
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

fn record_da_peer_valid_response(metrics: &mut DaShareSyncClientMetrics, peer_index: usize) {
    let peer_score = da_peer_score_mut(metrics, peer_index);
    peer_score.score = peer_score.score.saturating_add(1);
    peer_score.valid_responses = peer_score.valid_responses.saturating_add(1);
}

fn record_da_peer_invalid_response(metrics: &mut DaShareSyncClientMetrics, peer_index: usize) {
    let peer_score = da_peer_score_mut(metrics, peer_index);
    peer_score.score = peer_score.score.saturating_sub(1);
    peer_score.invalid_responses = peer_score.invalid_responses.saturating_add(1);
}

fn record_da_peer_failure(metrics: &mut DaShareSyncClientMetrics, peer_index: usize) {
    let peer_score = da_peer_score_mut(metrics, peer_index);
    peer_score.score = peer_score.score.saturating_sub(1);
    peer_score.failures = peer_score.failures.saturating_add(1);
}

fn da_peer_score_mut(
    metrics: &mut DaShareSyncClientMetrics,
    peer_index: usize,
) -> &mut DaShareSyncPeerScore {
    if let Some(position) = metrics
        .peer_scores
        .iter()
        .position(|score| score.peer_index == peer_index)
    {
        return &mut metrics.peer_scores[position];
    }
    metrics.peer_scores.push(DaShareSyncPeerScore {
        peer_index,
        ..DaShareSyncPeerScore::default()
    });
    metrics
        .peer_scores
        .last_mut()
        .expect("peer score was just inserted")
}

fn ensure_production_da_retention_policy(storage: &FileStorage) -> Result<(), NodeError> {
    if storage
        .maybe_load_da_retention_policy()
        .map_err(NodeError::Storage)?
        .is_none()
    {
        storage
            .commit_da_retention_policy(&DaRetentionPolicyConfig::production_default())
            .map_err(NodeError::Storage)?;
    }
    Ok(())
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

pub fn fetch_da_share_set_from_tcp_peers(
    mut connect_peer: impl FnMut(usize) -> Result<TcpProtocolStream, NetworkError>,
    peer_count: usize,
    manifest_hash: impl Into<String>,
    max_shares_per_request: u32,
) -> Result<(DaShareSet, DaShareSyncClientMetrics), NodeError> {
    if peer_count == 0 {
        return Err(NodeError::DataAvailability(DaError::InsufficientShares {
            required: 1,
            actual: 0,
        }));
    }
    if max_shares_per_request == 0 {
        return Err(NodeError::DataAvailability(DaError::InvalidChunkSize));
    }
    if max_shares_per_request > DEFAULT_MAX_DA_SHARES_PER_REQUEST {
        return Err(NodeError::DataAvailability(DaError::InvalidPayload(
            "DA share request exceeds maximum share count".into(),
        )));
    }

    let manifest_hash = manifest_hash.into();
    let mut manifest = None;
    let mut shares = BTreeMap::new();
    let mut metrics = DaShareSyncClientMetrics::default();

    for peer_index in 0..peer_count {
        metrics.peer_attempts = metrics.peer_attempts.saturating_add(1);
        da_peer_score_mut(&mut metrics, peer_index);
        let mut stream = match connect_peer(peer_index) {
            Ok(stream) => stream,
            Err(_) => {
                metrics.peer_failures = metrics.peer_failures.saturating_add(1);
                record_da_peer_failure(&mut metrics, peer_index);
                continue;
            }
        };
        let start_index = first_missing_da_share_index(manifest.as_ref(), &shares).unwrap_or(0);
        let request = DaShareRequest {
            manifest_hash: manifest_hash.clone(),
            start_index,
            max_shares: max_shares_per_request,
        };
        metrics.requests_sent = metrics.requests_sent.saturating_add(1);
        if stream
            .send(&NetworkMessage::DaShareRequest(request))
            .is_err()
        {
            metrics.peer_failures = metrics.peer_failures.saturating_add(1);
            record_da_peer_failure(&mut metrics, peer_index);
            continue;
        }

        let peer_manifest = match stream.receive() {
            Ok(NetworkMessage::DaManifest(peer_manifest)) => *peer_manifest,
            Ok(_) => {
                metrics.invalid_responses = metrics.invalid_responses.saturating_add(1);
                record_da_peer_invalid_response(&mut metrics, peer_index);
                continue;
            }
            Err(_) => {
                metrics.peer_failures = metrics.peer_failures.saturating_add(1);
                record_da_peer_failure(&mut metrics, peer_index);
                continue;
            }
        };
        let peer_manifest_hash = peer_manifest
            .manifest_hash()
            .map_err(NodeError::DataAvailability)?;
        if peer_manifest_hash != manifest_hash {
            metrics.invalid_responses = metrics.invalid_responses.saturating_add(1);
            record_da_peer_invalid_response(&mut metrics, peer_index);
            continue;
        }
        if let Some(expected_manifest) = &manifest {
            if expected_manifest != &peer_manifest {
                metrics.invalid_responses = metrics.invalid_responses.saturating_add(1);
                record_da_peer_invalid_response(&mut metrics, peer_index);
                continue;
            }
        } else {
            manifest = Some(peer_manifest);
        }
        metrics.manifests_received = metrics.manifests_received.saturating_add(1);

        let manifest_ref = manifest.as_ref().expect("manifest set above");
        let expected_shares = manifest_ref
            .encoded_share_count
            .saturating_sub(start_index)
            .min(max_shares_per_request);
        let mut peer_valid = true;
        for _ in 0..expected_shares {
            let share = match stream.receive() {
                Ok(NetworkMessage::DaShare(share)) => share,
                Ok(_) => {
                    metrics.invalid_responses = metrics.invalid_responses.saturating_add(1);
                    record_da_peer_invalid_response(&mut metrics, peer_index);
                    peer_valid = false;
                    break;
                }
                Err(_) => {
                    metrics.peer_failures = metrics.peer_failures.saturating_add(1);
                    record_da_peer_failure(&mut metrics, peer_index);
                    peer_valid = false;
                    break;
                }
            };
            if verify_share_against_manifest(manifest_ref, &share).is_err() {
                metrics.invalid_responses = metrics.invalid_responses.saturating_add(1);
                record_da_peer_invalid_response(&mut metrics, peer_index);
                peer_valid = false;
                break;
            }
            record_da_peer_valid_response(&mut metrics, peer_index);
            match shares.entry(share.index) {
                Entry::Vacant(entry) => {
                    metrics.shares_received = metrics.shares_received.saturating_add(1);
                    entry.insert(share);
                }
                Entry::Occupied(_) => {
                    metrics.duplicate_shares = metrics.duplicate_shares.saturating_add(1);
                }
            }
        }
        if !peer_valid {
            continue;
        }
        if let Some(share_set) = reconstructable_da_share_set(manifest_ref, &shares)? {
            metrics.payload_reconstructable = true;
            return Ok((share_set, metrics));
        }
    }

    let required = manifest
        .as_ref()
        .map_or(1, |manifest| manifest.reconstruction_threshold);
    Err(NodeError::DataAvailability(DaError::InsufficientShares {
        required,
        actual: shares.len() as u32,
    }))
}

impl PersistentValidatorNode {
    pub fn bootstrap(
        validator_id: impl Into<String>,
        state: DeTTaState,
        storage_root: impl Into<PathBuf>,
    ) -> Result<Self, NodeError> {
        let validator_id = validator_id.into();
        let storage = FileStorage::open(storage_root).map_err(NodeError::Storage)?;
        ensure_production_da_retention_policy(&storage)?;
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
            max_snapshot_import_audit_records: DEFAULT_MAX_SNAPSHOT_IMPORT_AUDIT_RECORDS,
            max_snapshot_import_audit_page_size: DEFAULT_MAX_SNAPSHOT_IMPORT_AUDIT_PAGE_SIZE,
            observed_peer_ids: BTreeSet::new(),
            last_block_execution_micros: None,
            last_proof_serving_micros: None,
            rpc_error_count: 0,
            operator_alert_policy: OperatorAlertPolicy::default(),
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
        ensure_production_da_retention_policy(&storage)?;
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
            max_snapshot_import_audit_records: DEFAULT_MAX_SNAPSHOT_IMPORT_AUDIT_RECORDS,
            max_snapshot_import_audit_page_size: DEFAULT_MAX_SNAPSHOT_IMPORT_AUDIT_PAGE_SIZE,
            observed_peer_ids: BTreeSet::new(),
            last_block_execution_micros: None,
            last_proof_serving_micros: None,
            rpc_error_count: 0,
            operator_alert_policy: OperatorAlertPolicy::default(),
        };
        if let Some(metadata) = node
            .storage
            .maybe_load_validator_set_metadata()
            .map_err(NodeError::Storage)?
        {
            node.apply_validator_set_metadata(metadata)?;
        }
        if let Some(config) = node
            .storage
            .maybe_load_snapshot_import_audit_config()
            .map_err(NodeError::Storage)?
        {
            node.max_snapshot_import_audit_records = config.max_records;
            node.max_snapshot_import_audit_page_size = config.max_page_size;
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
        let snapshot_import_audit_records = self.load_snapshot_import_audit_records()?;
        let required_snapshot_metadata_roots = self.load_required_snapshot_metadata_roots()?;
        let required_snapshot_metadata_roots_root = self.required_snapshot_metadata_roots_root()?;
        let snapshot_sync_client_metrics = self
            .load_snapshot_sync_client_metrics()?
            .map(snapshot_sync_client_metrics_report);
        let snapshot_import_audit_config_root = self
            .storage
            .snapshot_import_audit_config_root()
            .map_err(NodeError::Storage)?;
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
            snapshot_import_audit_record_count: snapshot_import_audit_records.len(),
            snapshot_import_audit_max_records: self.max_snapshot_import_audit_records,
            snapshot_import_audit_max_page_size: self.max_snapshot_import_audit_page_size,
            snapshot_import_audit_config_root,
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
        let local_snapshot_import_audit_config_root = self
            .storage
            .snapshot_import_audit_config_root()
            .map_err(NodeError::Storage)?;
        let persisted_snapshot_import_audit_config_root = persisted_metadata_roots
            .get(SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_CONFIG_ROOT)
            .cloned();
        let snapshot_import_audit_config_root = local_snapshot_import_audit_config_root
            .clone()
            .or_else(|| persisted_snapshot_import_audit_config_root.clone());
        let snapshot_import_audit_records = self.load_snapshot_import_audit_records()?;
        let local_snapshot_import_audit_root = if snapshot_import_audit_records.is_empty() {
            None
        } else {
            Some(
                FileStorage::snapshot_import_audit_root_for(&snapshot_import_audit_records)
                    .map_err(NodeError::Storage)?,
            )
        };
        let persisted_snapshot_import_audit_root = persisted_metadata_roots
            .get(SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_ROOT)
            .cloned();
        let snapshot_import_audit_root = local_snapshot_import_audit_root
            .clone()
            .or_else(|| persisted_snapshot_import_audit_root.clone());
        let required_snapshot_metadata_roots = self.load_required_snapshot_metadata_roots()?;
        let local_required_snapshot_metadata_roots_root =
            if required_snapshot_metadata_roots.is_empty() {
                None
            } else {
                Some(
                    FileStorage::required_snapshot_metadata_roots_root_for(
                        &required_snapshot_metadata_roots,
                    )
                    .map_err(NodeError::Storage)?,
                )
            };
        let persisted_required_snapshot_metadata_roots_root = persisted_metadata_roots
            .get(SNAPSHOT_METADATA_REQUIRED_METADATA_ROOTS_ROOT)
            .cloned();
        let required_snapshot_metadata_roots_root = local_required_snapshot_metadata_roots_root
            .clone()
            .or_else(|| persisted_required_snapshot_metadata_roots_root.clone());
        let local_snapshot_sync_client_metrics_root = self
            .storage
            .snapshot_sync_client_metrics_root::<SnapshotSyncClientMetrics>()
            .map_err(NodeError::Storage)?;
        let persisted_snapshot_sync_client_metrics_root = persisted_metadata_roots
            .get(SNAPSHOT_METADATA_STATE_SYNC_CLIENT_METRICS_ROOT)
            .cloned();
        let snapshot_sync_client_metrics_root = local_snapshot_sync_client_metrics_root
            .clone()
            .or_else(|| persisted_snapshot_sync_client_metrics_root.clone());
        let persisted_matches_local_validator_set_metadata_audit_root =
            persisted_validator_set_metadata_audit_root
                .as_ref()
                .is_some_and(|root| root == &local_validator_set_metadata_audit_root);
        let using_imported_validator_set_metadata_audit_root =
            persisted_validator_set_metadata_audit_root
                .as_ref()
                .is_some_and(|root| root != &local_validator_set_metadata_audit_root);
        let persisted_matches_local_snapshot_import_audit_config_root =
            persisted_snapshot_import_audit_config_root
                .as_ref()
                .is_some_and(|root| Some(root) == local_snapshot_import_audit_config_root.as_ref());
        let using_imported_snapshot_import_audit_config_root =
            local_snapshot_import_audit_config_root.is_none()
                && persisted_snapshot_import_audit_config_root.is_some();
        let persisted_matches_local_snapshot_import_audit_root =
            persisted_snapshot_import_audit_root
                .as_ref()
                .is_some_and(|root| Some(root) == local_snapshot_import_audit_root.as_ref());
        let using_imported_snapshot_import_audit_root = persisted_snapshot_import_audit_root
            .is_some()
            && local_snapshot_import_audit_root.is_none();
        let persisted_matches_local_required_snapshot_metadata_roots_root =
            persisted_required_snapshot_metadata_roots_root
                .as_ref()
                .is_some_and(|root| {
                    Some(root) == local_required_snapshot_metadata_roots_root.as_ref()
                });
        let using_imported_required_snapshot_metadata_roots_root =
            local_required_snapshot_metadata_roots_root.is_none()
                && persisted_required_snapshot_metadata_roots_root.is_some();
        let persisted_matches_local_snapshot_sync_client_metrics_root =
            persisted_snapshot_sync_client_metrics_root
                .as_ref()
                .is_some_and(|root| Some(root) == local_snapshot_sync_client_metrics_root.as_ref());
        let using_imported_snapshot_sync_client_metrics_root =
            local_snapshot_sync_client_metrics_root.is_none()
                && persisted_snapshot_sync_client_metrics_root.is_some();
        Ok(SnapshotMetadataRootStatus {
            validator_set_metadata_audit_root,
            local_validator_set_metadata_audit_root,
            persisted_validator_set_metadata_audit_root,
            using_imported_validator_set_metadata_audit_root,
            persisted_matches_local_validator_set_metadata_audit_root,
            snapshot_import_audit_config_root,
            local_snapshot_import_audit_config_root,
            persisted_snapshot_import_audit_config_root,
            using_imported_snapshot_import_audit_config_root,
            persisted_matches_local_snapshot_import_audit_config_root,
            snapshot_import_audit_root,
            local_snapshot_import_audit_root,
            persisted_snapshot_import_audit_root,
            using_imported_snapshot_import_audit_root,
            persisted_matches_local_snapshot_import_audit_root,
            required_snapshot_metadata_roots_root,
            local_required_snapshot_metadata_roots_root,
            persisted_required_snapshot_metadata_roots_root,
            using_imported_required_snapshot_metadata_roots_root,
            persisted_matches_local_required_snapshot_metadata_roots_root,
            snapshot_sync_client_metrics_root,
            local_snapshot_sync_client_metrics_root,
            persisted_snapshot_sync_client_metrics_root,
            using_imported_snapshot_sync_client_metrics_root,
            persisted_matches_local_snapshot_sync_client_metrics_root,
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
        if let Some(snapshot_import_audit_config) = self
            .storage
            .maybe_load_snapshot_import_audit_config()
            .map_err(NodeError::Storage)?
        {
            metadata_roots.insert(
                SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_CONFIG_ROOT.into(),
                FileStorage::snapshot_import_audit_config_root_for(&snapshot_import_audit_config)
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
        let records_proof_latency = records_proof_latency(&request);
        let started = Instant::now();
        if let Err(error) = request.validate_bounds() {
            let response = Err(error).into();
            if records_proof_latency {
                self.last_proof_serving_micros = Some(elapsed_micros(started));
            }
            self.rpc_error_count = self.rpc_error_count.saturating_add(1);
            return response;
        }

        let response = match request {
            RpcRequest::SubmitTransaction { transaction } => self
                .submit_transaction(transaction)
                .map(|()| RpcResult::Submitted)
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::SubmitSignedTransaction { signed } => signed
                .into_authorized_transaction(self.rpc.node().state())
                .map_err(NodeError::Mempool)
                .and_then(|transaction| self.submit_transaction(transaction))
                .map(|()| RpcResult::Submitted)
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::ProduceBlock { height, timestamp } => self
                .produce_block(height, timestamp)
                .map(|block| RpcResult::Block(Box::new(block)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::ProduceDaBlock {
                height,
                timestamp,
                share_size_bytes,
            } => self
                .produce_block_with_data_availability(height, timestamp, share_size_bytes as usize)
                .map(|block| RpcResult::Block(Box::new(block)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::ImportBlock { block } => self
                .import_block(&block)
                .map(|()| RpcResult::Imported)
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetBlock { height } => match self.storage.maybe_load_block(height) {
                Ok(Some(block)) => RpcResponse::Ok(RpcResult::Block(Box::new(block))),
                Ok(None) => Err(RpcError::BlockNotFound).into(),
                Err(error) => node_rpc_error_response(NodeError::Storage(error)),
            },
            RpcRequest::GetBlocksPage {
                start_height,
                limit,
            } => match self.storage.load_blocks() {
                Ok(blocks) => {
                    let effective_limit = limit.min(DEFAULT_MAX_BLOCK_PAGE_SIZE);
                    let highest_height = blocks
                        .iter()
                        .map(|block| block.header.height)
                        .max()
                        .unwrap_or(0);
                    let page_blocks = blocks
                        .into_iter()
                        .filter(|block| block.header.height >= start_height)
                        .take(effective_limit)
                        .collect();
                    RpcResponse::Ok(RpcResult::BlocksPage(BlockPage {
                        blocks: page_blocks,
                        start_height,
                        limit: effective_limit,
                        highest_height,
                    }))
                }
                Err(error) => node_rpc_error_response(NodeError::Storage(error)),
            },
            RpcRequest::GetTransaction { tx_hash } => {
                match self.storage.find_transaction(&tx_hash) {
                    Ok(Some(transaction)) => {
                        RpcResponse::Ok(RpcResult::Transaction(Box::new(transaction)))
                    }
                    Ok(None) => Err(RpcError::TransactionNotFound).into(),
                    Err(error) => node_rpc_error_response(NodeError::Storage(error)),
                }
            }
            RpcRequest::GetReceipt { tx_hash } => match self.storage.find_receipt(&tx_hash) {
                Ok(Some(receipt)) => RpcResponse::Ok(RpcResult::Receipt(Box::new(receipt))),
                Ok(None) => Err(RpcError::ReceiptNotFound).into(),
                Err(error) => node_rpc_error_response(NodeError::Storage(error)),
            },
            RpcRequest::GetReceiptProof { height, index } => {
                match self.storage.maybe_load_block(height) {
                    Ok(Some(block)) => match block.receipt_proof(index) {
                        Some(proof) => RpcResponse::Ok(RpcResult::ReceiptProof(Box::new(proof))),
                        None => Err(RpcError::ProofNotFound).into(),
                    },
                    Ok(None) => Err(RpcError::BlockNotFound).into(),
                    Err(error) => node_rpc_error_response(NodeError::Storage(error)),
                }
            }
            RpcRequest::GetFinalityCertificate { height } => {
                match self.storage.maybe_load_finality_certificate(height) {
                    Ok(Some(certificate)) => {
                        RpcResponse::Ok(RpcResult::FinalityCertificate(Box::new(certificate)))
                    }
                    Ok(None) => Err(RpcError::CertificateNotFound).into(),
                    Err(error) => node_rpc_error_response(NodeError::Storage(error)),
                }
            }
            RpcRequest::GetSlashingRecord { validator_id } => {
                match self.storage.maybe_load_slashing_record(&validator_id) {
                    Ok(Some(record)) => {
                        RpcResponse::Ok(RpcResult::SlashingRecord(Box::new(record)))
                    }
                    Ok(None) => Err(RpcError::SlashingRecordNotFound).into(),
                    Err(error) => node_rpc_error_response(NodeError::Storage(error)),
                }
            }
            RpcRequest::GetDaManifest { manifest_hash } => {
                match self.storage.maybe_load_da_manifest(&manifest_hash) {
                    Ok(Some(manifest)) => {
                        RpcResponse::Ok(RpcResult::DaManifest(Box::new(manifest)))
                    }
                    Ok(None) => Err(RpcError::DaManifestNotFound).into(),
                    Err(error) => node_rpc_error_response(NodeError::Storage(error)),
                }
            }
            RpcRequest::GetDaShare {
                manifest_hash,
                index,
            } => match self.storage.maybe_load_da_share(&manifest_hash, index) {
                Ok(Some(share)) => RpcResponse::Ok(RpcResult::DaShare(Box::new(share))),
                Ok(None) => Err(RpcError::DaShareNotFound).into(),
                Err(error) => node_rpc_error_response(NodeError::Storage(error)),
            },
            RpcRequest::GetDaCertificate { certificate_hash } => {
                match self.storage.maybe_load_da_certificate(&certificate_hash) {
                    Ok(Some(certificate)) => {
                        RpcResponse::Ok(RpcResult::DaAvailabilityCertificate(Box::new(certificate)))
                    }
                    Ok(None) => Err(RpcError::DaCertificateNotFound).into(),
                    Err(error) => node_rpc_error_response(NodeError::Storage(error)),
                }
            }
            RpcRequest::GetDaChallengeRecord { challenge_id } => {
                match self.storage.maybe_load_da_challenge_record(&challenge_id) {
                    Ok(Some(record)) => {
                        RpcResponse::Ok(RpcResult::DaChallengeRecord(Box::new(record)))
                    }
                    Ok(None) => Err(RpcError::DaChallengeRecordNotFound).into(),
                    Err(error) => node_rpc_error_response(NodeError::Storage(error)),
                }
            }
            RpcRequest::GetDaPayload { manifest_hash } => self
                .load_da_payload(&manifest_hash)
                .map(|payload| RpcResult::DaPayload(Box::new(payload)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetDaNamespace {
                manifest_hash,
                namespace,
            } => match self.load_da_namespace(&manifest_hash, &namespace) {
                Ok(Some(section)) => RpcResponse::Ok(RpcResult::DaNamespace(Box::new(section))),
                Ok(None) => Err(RpcError::DaNamespaceNotFound).into(),
                Err(error) => node_rpc_error_response(error),
            },
            RpcRequest::GetDaSampleProofs {
                manifest_hash,
                client_randomness,
                sample_count,
                namespaces,
            } => self
                .da_sample_proof_bundle(
                    &manifest_hash,
                    &client_randomness,
                    sample_count,
                    &namespaces,
                )
                .map(|bundle| RpcResult::DaSampleProofs(Box::new(bundle)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| match error {
                    NodeError::Rpc(error) => Err(error).into(),
                    error => node_rpc_error_response(error),
                }),
            RpcRequest::GetDaStatus { manifest_hash } => self
                .da_status(&manifest_hash)
                .map(|status| RpcResult::DaStatus(Box::new(status)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetDaRepairStatus { manifest_hash } => self
                .da_repair_status(&manifest_hash)
                .map(|status| RpcResult::DaRepairStatus(Box::new(status)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetDaCodingFraudProof { manifest_hash } => self
                .da_coding_fraud_proof(&manifest_hash)
                .map(|report| RpcResult::DaCodingFraud(Box::new(report)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetDaStorageStats => self
                .storage
                .da_storage_stats()
                .map(RpcResult::DaStorageStats)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetDaRetentionAudit => self
                .da_retention_audit()
                .map(|report| RpcResult::DaRetentionAudit(Box::new(report)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetDaRetentionPrunePlan => self
                .da_retention_prune_plan()
                .map(|report| RpcResult::DaRetentionPrunePlan(Box::new(report)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetDaManifestIndexByHeight { height } => self
                .storage
                .load_da_manifest_index_by_height(height)
                .map(RpcResult::DaManifestIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetDaManifestIndexByBlockHash { block_hash } => self
                .storage
                .load_da_manifest_index_by_block_hash(&block_hash)
                .map(RpcResult::DaManifestIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetDaManifestIndexByNamespace { namespace } => self
                .storage
                .load_da_manifest_index_by_namespace(&namespace)
                .map(RpcResult::DaManifestIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetDaManifestIndexByRetentionClass { class } => self
                .storage
                .load_da_manifest_index_by_retention_class(class)
                .map(RpcResult::DaManifestIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetDaCertificateIndexByManifest { manifest_hash } => self
                .storage
                .load_da_certificate_index_by_manifest_hash(&manifest_hash)
                .map(RpcResult::DaCertificateIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetDaCertificateIndexByHeight { height } => self
                .storage
                .load_da_certificate_index_by_height(height)
                .map(RpcResult::DaCertificateIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetDaCertificateIndexByBlockHash { block_hash } => self
                .storage
                .load_da_certificate_index_by_block_hash(&block_hash)
                .map(RpcResult::DaCertificateIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaProfile { profile_id } => self
                .storage
                .maybe_load_application_da_profile_registration(&profile_id)
                .map(|registration| match registration {
                    Some(registration) => {
                        RpcResponse::Ok(RpcResult::ApplicationDaProfile(Box::new(registration)))
                    }
                    None => Err(RpcError::ApplicationDaProfileNotFound).into(),
                })
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaProfileIndexByApplicationId { application_id } => self
                .storage
                .load_application_da_profile_index_by_application_id(&application_id)
                .map(RpcResult::ApplicationDaProfileIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaProfileIndexByApplicationVersion {
                application_id,
                profile_version,
            } => self
                .storage
                .load_application_da_profile_index_by_application_version(
                    &application_id,
                    profile_version,
                )
                .map(RpcResult::ApplicationDaProfileIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaManifest { manifest_hash } => self
                .storage
                .maybe_load_application_da_manifest(&manifest_hash)
                .map(|manifest| match manifest {
                    Some(manifest) => {
                        RpcResponse::Ok(RpcResult::ApplicationDaManifest(Box::new(manifest)))
                    }
                    None => Err(RpcError::ApplicationDaManifestNotFound).into(),
                })
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaShare {
                manifest_hash,
                index,
            } => self
                .storage
                .maybe_load_application_da_share(&manifest_hash, index)
                .map(|share| match share {
                    Some(share) => RpcResponse::Ok(RpcResult::ApplicationDaShare(Box::new(share))),
                    None => Err(RpcError::ApplicationDaShareNotFound).into(),
                })
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaCertificate { certificate_hash } => self
                .storage
                .maybe_load_application_da_certificate(&certificate_hash)
                .map(|certificate| match certificate {
                    Some(certificate) => RpcResponse::Ok(
                        RpcResult::ApplicationDaAvailabilityCertificate(Box::new(certificate)),
                    ),
                    None => Err(RpcError::ApplicationDaCertificateNotFound).into(),
                })
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaPayload { manifest_hash }
            | RpcRequest::GetApplicationDaReconstructedPayload { manifest_hash } => self
                .load_application_da_payload(&manifest_hash)
                .map(|payload| RpcResult::ApplicationDaPayload(Box::new(payload)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| match error {
                    NodeError::Rpc(error) => Err(error).into(),
                    error => node_rpc_error_response(error),
                }),
            RpcRequest::GetApplicationDaNamespace {
                manifest_hash,
                namespace,
            } => match self.load_application_da_namespace(&manifest_hash, &namespace) {
                Ok(Some(section)) => {
                    RpcResponse::Ok(RpcResult::ApplicationDaNamespace(Box::new(section)))
                }
                Ok(None) => Err(RpcError::ApplicationDaNamespaceNotFound).into(),
                Err(error) => node_rpc_error_response(error),
            },
            RpcRequest::GetApplicationDaSampleProofs {
                manifest_hash,
                client_randomness,
                sample_count,
                namespaces,
            } => self
                .application_da_sample_proof_bundle(
                    &manifest_hash,
                    &client_randomness,
                    sample_count,
                    &namespaces,
                )
                .map(|bundle| RpcResult::ApplicationDaSampleProofs(Box::new(bundle)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| match error {
                    NodeError::Rpc(error) => Err(error).into(),
                    error => node_rpc_error_response(error),
                }),
            RpcRequest::GetApplicationDaStatus { manifest_hash } => self
                .application_da_status(&manifest_hash)
                .map(|status| RpcResult::ApplicationDaStatus(Box::new(status)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetApplicationDaManifestIndexByApplicationId { application_id } => self
                .storage
                .load_application_da_manifest_index_by_application_id(&application_id)
                .map(RpcResult::ApplicationDaManifestIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaManifestIndexByProfileId { profile_id } => self
                .storage
                .load_application_da_manifest_index_by_profile_id(&profile_id)
                .map(RpcResult::ApplicationDaManifestIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaManifestIndexByCoordinate { coordinate } => self
                .storage
                .load_application_da_manifest_index_by_coordinate(&coordinate)
                .map(RpcResult::ApplicationDaManifestIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaManifestIndexByNamespace { namespace } => self
                .storage
                .load_application_da_manifest_index_by_namespace(&namespace)
                .map(RpcResult::ApplicationDaManifestIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaManifestIndexByRetentionClass { class } => self
                .storage
                .load_application_da_manifest_index_by_retention_class(&class)
                .map(RpcResult::ApplicationDaManifestIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaManifestIndexByApplicationRoot { application_root } => self
                .storage
                .load_application_da_manifest_index_by_application_root(&application_root)
                .map(RpcResult::ApplicationDaManifestIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaCertificateIndexByManifest { manifest_hash } => self
                .storage
                .load_application_da_certificate_index_by_manifest_hash(&manifest_hash)
                .map(RpcResult::ApplicationDaCertificateIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaCertificateIndexByApplicationId { application_id } => self
                .storage
                .load_application_da_certificate_index_by_application_id(&application_id)
                .map(RpcResult::ApplicationDaCertificateIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaCertificateIndexByProfileId { profile_id } => self
                .storage
                .load_application_da_certificate_index_by_profile_id(&profile_id)
                .map(RpcResult::ApplicationDaCertificateIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetApplicationDaCertificateIndexByCoordinate { coordinate } => self
                .storage
                .load_application_da_certificate_index_by_coordinate(&coordinate)
                .map(RpcResult::ApplicationDaCertificateIndex)
                .map(RpcResponse::Ok)
                .unwrap_or_else(|error| node_rpc_error_response(NodeError::Storage(error))),
            RpcRequest::GetNodeHealth => RpcResponse::Ok(RpcResult::NodeHealth(Box::new(
                self.persistent_node_health(),
            ))),
            RpcRequest::GetOperatorMetrics => self
                .operator_metrics()
                .map(|metrics| RpcResult::OperatorMetrics(Box::new(metrics)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetOperatorAlerts => self
                .operator_alerts()
                .map(|alerts| RpcResult::OperatorAlerts(Box::new(alerts)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
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
            RpcRequest::GetSnapshotImportAuditConfigRoot => self
                .storage
                .snapshot_import_audit_config_root()
                .map_err(NodeError::Storage)
                .map(RpcResult::SnapshotImportAuditConfigRoot)
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetSnapshotImportAuditConfig => RpcResponse::Ok(
                RpcResult::SnapshotImportAuditConfig(self.snapshot_import_audit_config()),
            ),
            RpcRequest::GetPersistentNodeSnapshotRoots => self
                .node_snapshot_roots()
                .map(|snapshot| RpcResult::PersistentNodeSnapshotRoots(Box::new(snapshot)))
                .map(RpcResponse::Ok)
                .unwrap_or_else(node_rpc_error_response),
            RpcRequest::GetSnapshotMetadataRootStatus => self
                .snapshot_metadata_root_status()
                .map(|status| RpcResult::SnapshotMetadataRootStatus(Box::new(status)))
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
        };
        if records_proof_latency {
            self.last_proof_serving_micros = Some(elapsed_micros(started));
        }
        if matches!(response, RpcResponse::Error(_)) {
            self.rpc_error_count = self.rpc_error_count.saturating_add(1);
        }
        response
    }

    pub fn persistent_node_health(&self) -> NodeHealthReport {
        let mut health = self.rpc.node_health();
        health.network_id = Some(self.network_id.clone());
        health.validator_id = Some(self.validator_id.clone());
        health.da_production_profile = Some(DaProductionProfile::v1());
        health.trusted_validator_keys = Some(self.validator_keys.len());
        health.pending_validator_set_metadata_updates =
            Some(self.pending_validator_set_metadata_authorizations.len());
        health
    }

    pub fn operator_metrics(&self) -> Result<OperatorMetricsReport, NodeError> {
        let height = self.current_height();
        let highest_finalized_height = self.highest_finalized_height()?;
        let da_stats = self
            .storage
            .da_storage_stats()
            .map_err(NodeError::Storage)?;
        let da_challenge_records = self
            .storage
            .load_da_challenge_records()
            .map_err(NodeError::Storage)?;
        let da_repair_records = self
            .storage
            .load_da_repair_records()
            .map_err(NodeError::Storage)?;
        let slashing_records = self
            .storage
            .load_slashing_records()
            .map_err(NodeError::Storage)?;
        let da_pending_repair_record_count = da_repair_records
            .iter()
            .filter(|record| !record.completed)
            .count() as u64;
        let da_oldest_pending_repair_age_blocks = da_repair_records
            .iter()
            .filter(|record| !record.completed)
            .map(|record| height.saturating_sub(record.recorded_at_height))
            .max();
        Ok(OperatorMetricsReport {
            network_id: Some(self.network_id.clone()),
            validator_id: Some(self.validator_id.clone()),
            peer_count: Some(self.observed_peer_ids.len()),
            da_production_profile: Some(DaProductionProfile::v1()),
            mempool_size: self.pending_len(),
            consensus_height: height,
            highest_finalized_height,
            finality_lag: highest_finalized_height
                .map(|finalized_height| height.saturating_sub(finalized_height)),
            last_block_execution_micros: self.last_block_execution_micros,
            last_proof_serving_micros: self.last_proof_serving_micros,
            storage_bytes: Some(self.storage.storage_bytes().map_err(NodeError::Storage)?),
            rpc_error_count: self.rpc_error_count,
            da_manifest_count: da_stats.manifest_count,
            da_missing_share_count: da_stats.missing_share_count,
            da_payload_count: da_stats.payload_count,
            da_challenge_record_count: da_challenge_records.len() as u64,
            da_challenge_evidence_count: da_challenge_records
                .iter()
                .filter(|record| record.evidence.is_some())
                .count() as u64,
            da_custody_failure_count: slashing_records
                .iter()
                .filter(|record| matches!(record.evidence, SlashingEvidence::DataAvailability(_)))
                .count() as u64,
            da_repair_record_count: da_stats.repair_record_count,
            da_pending_repair_record_count,
            da_oldest_pending_repair_age_blocks,
            da_total_bytes: da_stats.total_bytes,
        })
    }

    pub fn operator_alerts(&self) -> Result<OperatorAlertReport, NodeError> {
        let metrics = self.operator_metrics()?;
        let root_mismatch = self.snapshot_root_mismatch()?;
        let slashing_record_count = self
            .storage
            .load_slashing_records()
            .map_err(NodeError::Storage)?
            .len();
        let (latest_block_failure_count, latest_block_receipt_count) =
            self.latest_block_failure_counts()?;
        let alerts = operator_alerts_for_state(
            &self.operator_alert_policy,
            &metrics,
            root_mismatch,
            slashing_record_count,
            latest_block_failure_count,
            latest_block_receipt_count,
        );

        Ok(OperatorAlertReport {
            policy: self.operator_alert_policy.clone(),
            metrics,
            alerts,
            root_mismatch,
            slashing_record_count,
            latest_block_failure_count,
            latest_block_receipt_count,
        })
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

    fn checkpoint_previous_block_hash(&self) -> Result<String, NodeError> {
        let latest_block_hash = self
            .storage
            .load_blocks()
            .map_err(NodeError::Storage)?
            .into_iter()
            .max_by_key(|block| block.header.height)
            .map(|block| block.block_hash());
        Ok(latest_block_hash.unwrap_or_else(|| self.rpc.snapshot().global_state_root))
    }

    pub fn set_operator_alert_policy(&mut self, policy: OperatorAlertPolicy) {
        self.operator_alert_policy = policy;
    }

    fn highest_finalized_height(&self) -> Result<Option<u64>, NodeError> {
        let height = self.current_height();
        for candidate in (1..=height).rev() {
            if self
                .storage
                .maybe_load_finality_certificate(candidate)
                .map_err(NodeError::Storage)?
                .is_some()
            {
                return Ok(Some(candidate));
            }
        }
        Ok(None)
    }

    fn latest_block_failure_counts(&self) -> Result<(usize, usize), NodeError> {
        let height = self.current_height();
        if height == 0 {
            return Ok((0, 0));
        }
        let Some(block) = self
            .storage
            .maybe_load_block(height)
            .map_err(NodeError::Storage)?
        else {
            return Ok((0, 0));
        };
        let failure_count = block
            .receipts
            .iter()
            .filter(|receipt| receipt.status != TxStatus::Committed)
            .count();
        Ok((failure_count, block.receipts.len()))
    }

    fn snapshot_root_mismatch(&self) -> Result<bool, NodeError> {
        let status = self.snapshot_metadata_root_status()?;
        Ok(
            (status.persisted_validator_set_metadata_audit_root.is_some()
                && !status.persisted_matches_local_validator_set_metadata_audit_root)
                || (status.persisted_snapshot_import_audit_config_root.is_some()
                    && !status.persisted_matches_local_snapshot_import_audit_config_root)
                || (status.persisted_snapshot_import_audit_root.is_some()
                    && !status.persisted_matches_local_snapshot_import_audit_root)
                || (status
                    .persisted_required_snapshot_metadata_roots_root
                    .is_some()
                    && !status.persisted_matches_local_required_snapshot_metadata_roots_root)
                || (status.persisted_snapshot_sync_client_metrics_root.is_some()
                    && !status.persisted_matches_local_snapshot_sync_client_metrics_root),
        )
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

    pub fn set_snapshot_import_audit_limits(
        &mut self,
        max_records: usize,
        max_page_size: usize,
    ) -> Result<(), NodeError> {
        self.max_snapshot_import_audit_records = max_records;
        self.max_snapshot_import_audit_page_size = max_page_size;
        self.storage
            .commit_snapshot_import_audit_config(&SnapshotImportAuditConfig {
                max_records,
                max_page_size,
            })
            .map_err(NodeError::Storage)
    }

    pub fn snapshot_import_audit_config(&self) -> SnapshotImportAuditConfig {
        SnapshotImportAuditConfig {
            max_records: self.max_snapshot_import_audit_records,
            max_page_size: self.max_snapshot_import_audit_page_size,
        }
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

    pub fn gossip_data_availability_for_block(
        &self,
        block: &Block,
        transport: &mut InMemoryTransport,
    ) -> Result<usize, NodeError> {
        let commitment = block.header.data_availability.as_ref().ok_or_else(|| {
            NodeError::DataAvailability(DaError::InvalidManifest(
                "block has no data availability commitment".into(),
            ))
        })?;
        let share_set = self.load_da_share_set(&commitment.manifest_hash)?;
        let manifest_hash = share_set
            .manifest
            .manifest_hash()
            .map_err(NodeError::DataAvailability)?;
        if manifest_hash != commitment.manifest_hash {
            return Err(NodeError::DataAvailability(DaError::ManifestHashMismatch {
                expected: commitment.manifest_hash.clone(),
                actual: manifest_hash,
            }));
        }
        if share_set.manifest.payload_hash != commitment.payload_root {
            return Err(NodeError::DataAvailability(DaError::PayloadHashMismatch {
                expected: commitment.payload_root.clone(),
                actual: share_set.manifest.payload_hash.clone(),
            }));
        }
        if share_set.manifest.share_root != commitment.share_root {
            return Err(NodeError::DataAvailability(DaError::ShareRootMismatch {
                expected: commitment.share_root.clone(),
                actual: share_set.manifest.share_root.clone(),
            }));
        }

        let mut sent = transport
            .broadcast(
                self.validator_id.clone(),
                NetworkMessage::DaManifest(Box::new(share_set.manifest.clone())),
            )
            .map_err(NodeError::Network)?;
        for share in share_set.shares {
            sent = sent.saturating_add(
                transport
                    .broadcast(self.validator_id.clone(), NetworkMessage::DaShare(share))
                    .map_err(NodeError::Network)?,
            );
        }
        Ok(sent)
    }

    pub fn persist_finality_certificate(
        &mut self,
        certificate: &FinalityCertificate,
    ) -> Result<(), NodeError> {
        self.storage
            .commit_finality_certificate(certificate)
            .map_err(NodeError::Storage)?;
        self.rpc.publish_finality_certificate(certificate.clone());
        Ok(())
    }

    pub fn load_finality_certificate(&self, height: u64) -> Result<FinalityCertificate, NodeError> {
        self.storage
            .load_finality_certificate(height)
            .map_err(NodeError::Storage)
    }

    pub fn persist_and_gossip_signed_finality_certificate(
        &mut self,
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
            evidence: SlashingEvidence::Equivocation(evidence),
        };
        self.storage
            .commit_slashing_record(&record)
            .map_err(NodeError::Storage)?;
        Ok(record)
    }

    pub fn persist_da_share_challenge(
        &self,
        challenge: DaShareChallenge,
    ) -> Result<String, NodeError> {
        let record = DaChallengeRecord {
            challenge,
            response: None,
            evidence: None,
        };
        self.storage
            .commit_da_challenge_record(&record)
            .map_err(NodeError::Storage)
    }

    pub fn persist_da_share_challenge_response(
        &self,
        response: DaShareChallengeResponse,
    ) -> Result<String, NodeError> {
        let mut record = self
            .storage
            .load_da_challenge_record(&response.challenge_hash)
            .map_err(NodeError::Storage)?;
        response
            .validate(&record.challenge)
            .map_err(NodeError::DataAvailability)?;
        record.response = Some(response);
        self.storage
            .commit_da_challenge_record(&record)
            .map_err(NodeError::Storage)
    }

    pub fn persist_da_challenge_evidence(
        &self,
        evidence: DaChallengeEvidence,
    ) -> Result<SlashingRecord, NodeError> {
        evidence.validate().map_err(NodeError::DataAvailability)?;
        validate_da_slashing_evidence(self.rpc.node().state().da_slashing_policy(), &evidence)
            .map_err(|error| {
                NodeError::DataAvailability(DaError::InvalidChallenge(format!("{error:?}")))
            })?;
        if let Some(mut record) = self
            .storage
            .maybe_load_da_challenge_record(&evidence.challenge_hash)
            .map_err(NodeError::Storage)?
        {
            record.evidence = Some(evidence.clone());
            self.storage
                .commit_da_challenge_record(&record)
                .map_err(NodeError::Storage)?;
        }
        let record = SlashingRecord {
            validator_id: evidence.challenged_validator_id.clone(),
            slashed_at_height: evidence.observed_at_height,
            evidence: SlashingEvidence::DataAvailability(evidence),
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

    pub fn load_da_challenge_record(
        &self,
        challenge_id: &str,
    ) -> Result<DaChallengeRecord, NodeError> {
        self.storage
            .load_da_challenge_record(challenge_id)
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
            .load_snapshot_import_audit_records_page(
                offset,
                limit.min(self.max_snapshot_import_audit_page_size),
            )
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
        self.reserve_consensus_signing_record(&message)?;
        let signed = signing_key
            .sign_message(&self.network_id, self.chain_id.clone(), message)
            .map_err(NodeError::Signature)?;
        Ok(NetworkMessage::SignedValidator(Box::new(signed)))
    }

    pub fn sign_da_availability_vote(
        &self,
        signing_key: &ValidatorSigningKey,
        manifest: &DaManifest,
        shares: &[DaShare],
        custody_share_count: u32,
    ) -> Result<NetworkMessage, NodeError> {
        let vote = DaAvailabilityVote::from_verified_custody(
            manifest,
            shares,
            self.validator_id.clone(),
            custody_share_count,
        )
        .map_err(NodeError::DataAvailability)?;
        self.sign_validator_message(signing_key, NetworkMessage::DaAvailabilityVote(vote))
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
        let started = Instant::now();
        let block = self
            .rpc
            .produce_block(height, timestamp)
            .map_err(NodeError::Rpc)?;
        self.last_block_execution_micros = Some(elapsed_micros(started));
        self.persist_committed_block(&block)?;
        Ok(block)
    }

    /// Build a block proposal from the pending mempool **without committing it**:
    /// no state mutation, no mempool drain, no storage write. The block is
    /// computed on a clone of the current state, so its roots are final, but the
    /// node stays at its current height until the proposal is actually finalized
    /// and applied via [`Self::import_block`]. This is the producer half of a
    /// commit-after-finality (two-phase) consensus flow: a proposer can build and
    /// broadcast a block and only commit it once a quorum has voted, so an
    /// abandoned proposal can never fork the chain.
    pub fn build_proposal(&self, height: u64, timestamp: u64) -> Block {
        let transactions = self.pending_transactions_for_next_block();
        let consensus_certificate = format!("bft-cert:{}:{}", self.validator_id, height);
        let (block, _working_state) = self.rpc.node().state().build_block(
            height,
            transactions,
            timestamp,
            self.validator_id.clone(),
            consensus_certificate,
        );
        block
    }

    /// Verify the signature on a signed validator message and return the inner
    /// message **without applying it** (no import, no commit). Used by a voter to
    /// authenticate a proposal before validating and voting on it in a
    /// commit-after-finality flow. Errors if the message is not signed or the
    /// signer is not trusted.
    pub fn verify_signed_message(
        &self,
        message: &NetworkMessage,
    ) -> Result<NetworkMessage, NodeError> {
        let NetworkMessage::SignedValidator(signed) = message else {
            return Err(NodeError::BlockProposalRejected(
                "consensus message is not signed".into(),
            ));
        };
        self.verified_signed_validator_message(signed)
    }

    /// Verify a proposed block against the local chain tip **without committing
    /// it**: re-execute its transactions on a clone of the current state and
    /// confirm the recomputed block is identical (same roots and hash). A
    /// validator can call this to decide whether to vote before it commits, which
    /// only happens once the block is finalized. Leaves state, mempool, and
    /// storage untouched.
    pub fn verify_block_without_commit(&self, block: &Block) -> Result<(), NodeError> {
        let expected_height = self.current_height() + 1;
        if block.header.height != expected_height {
            return Err(NodeError::BlockProposalRejected(format!(
                "proposed height {} does not build on current height {} (expected {expected_height})",
                block.header.height,
                self.current_height()
            )));
        }
        let (rebuilt, _working_state) = self.rpc.node().state().build_block(
            block.header.height,
            block.transactions.clone(),
            block.header.timestamp,
            block.header.proposer.clone(),
            block.header.consensus_certificate.clone(),
        );
        if rebuilt.block_hash() != block.block_hash() {
            return Err(NodeError::BlockProposalRejected(format!(
                "re-executed block at height {} does not match the proposal",
                block.header.height
            )));
        }
        Ok(())
    }

    pub fn produce_block_with_data_availability(
        &mut self,
        height: u64,
        timestamp: u64,
        share_size_bytes: usize,
    ) -> Result<Block, NodeError> {
        let started = Instant::now();
        let transactions = self.pending_transactions_for_next_block();
        let consensus_certificate = format!("sim-cert:{}:{}", self.validator_id, height);
        let (mut block, _) = self.rpc.node().state().build_block(
            height,
            transactions,
            timestamp,
            self.validator_id.clone(),
            consensus_certificate,
        );

        let execution_block_hash = block.block_hash();
        let payload = da_payload_for_block(&block)?;
        let share_set = DaShareSet::from_payload_reed_solomon_with_target_share_size(
            &payload,
            &execution_block_hash,
            share_size_bytes,
        )
        .map_err(NodeError::DataAvailability)?;
        let manifest_hash = share_set
            .manifest
            .manifest_hash()
            .map_err(NodeError::DataAvailability)?;
        block.header.data_availability = Some(DataAvailabilityCommitment {
            payload_root: share_set.manifest.payload_hash.clone(),
            manifest_hash: manifest_hash.clone(),
            share_root: share_set.manifest.share_root.clone(),
            certificate_hash: None,
        });
        self.storage
            .commit_da_share_set(&share_set)
            .map_err(NodeError::Storage)?;
        self.rpc.import_block(&block).map_err(NodeError::Rpc)?;
        self.last_block_execution_micros = Some(elapsed_micros(started));
        self.persist_committed_block(&block)?;
        Ok(block)
    }

    pub fn import_block(&mut self, block: &Block) -> Result<(), NodeError> {
        let started = Instant::now();
        self.rpc.import_block(block).map_err(NodeError::Rpc)?;
        self.last_block_execution_micros = Some(elapsed_micros(started));
        self.persist_committed_block(block)
    }

    pub fn ingest_network_envelope(
        &mut self,
        envelope: &Envelope,
    ) -> Result<NetworkIngestOutcome, NodeError> {
        if envelope.from != self.validator_id {
            self.observed_peer_ids.insert(envelope.from.clone());
        }
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
            NetworkMessage::DaManifest(manifest) => {
                self.storage
                    .commit_da_manifest(manifest)
                    .map_err(NodeError::Storage)?;
                Ok(NetworkIngestOutcome::DataAvailabilityStored)
            }
            NetworkMessage::DaShare(share) => {
                self.storage
                    .commit_da_share(share)
                    .map_err(NodeError::Storage)?;
                Ok(NetworkIngestOutcome::DataAvailabilityStored)
            }
            NetworkMessage::DaAvailabilityCertificate(certificate) => {
                self.storage
                    .commit_da_certificate(certificate)
                    .map_err(NodeError::Storage)?;
                Ok(NetworkIngestOutcome::DataAvailabilityStored)
            }
            NetworkMessage::DaShareChallenge(challenge) => {
                self.persist_da_share_challenge(challenge.clone())?;
                Ok(NetworkIngestOutcome::DataAvailabilityStored)
            }
            NetworkMessage::DaShareChallengeResponse(response) => {
                self.persist_da_share_challenge_response(response.clone())?;
                Ok(NetworkIngestOutcome::DataAvailabilityStored)
            }
            NetworkMessage::DaChallengeEvidence(evidence) => {
                self.persist_da_challenge_evidence((**evidence).clone())?;
                Ok(NetworkIngestOutcome::DataAvailabilityStored)
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
            | NetworkMessage::SnapshotChunk(_)
            | NetworkMessage::DaShareRequest(_)
            | NetworkMessage::DaAvailabilityVote(_) => {
                Ok(NetworkIngestOutcome::IgnoredControlMessage)
            }
        }
    }

    pub fn load_block(&self, height: u64) -> Result<Block, NodeError> {
        self.storage.load_block(height).map_err(NodeError::Storage)
    }

    pub fn load_da_manifest(&self, manifest_hash: &str) -> Result<DaManifest, NodeError> {
        self.storage
            .load_da_manifest(manifest_hash)
            .map_err(NodeError::Storage)
    }

    pub fn load_da_share(&self, manifest_hash: &str, index: u32) -> Result<DaShare, NodeError> {
        self.storage
            .load_da_share(manifest_hash, index)
            .map_err(NodeError::Storage)
    }

    pub fn serve_da_share_request(
        &self,
        request: &DaShareRequest,
    ) -> Result<Vec<NetworkMessage>, NodeError> {
        if request.max_shares == 0 {
            return Err(NodeError::DataAvailability(DaError::InvalidChunkSize));
        }
        if request.max_shares > DEFAULT_MAX_DA_SHARES_PER_REQUEST {
            return Err(NodeError::DataAvailability(DaError::InvalidPayload(
                "DA share request exceeds maximum share count".into(),
            )));
        }
        let manifest = self.load_da_manifest(&request.manifest_hash)?;
        let end_index = request
            .start_index
            .saturating_add(request.max_shares)
            .min(manifest.encoded_share_count);
        let mut messages = vec![NetworkMessage::DaManifest(Box::new(manifest))];
        for index in request.start_index..end_index {
            if let Some(share) = self
                .storage
                .maybe_load_da_share(&request.manifest_hash, index)
                .map_err(NodeError::Storage)?
            {
                messages.push(NetworkMessage::DaShare(share));
            }
        }
        Ok(messages)
    }

    pub fn serve_da_share_request_with_rate_limit(
        &self,
        peer_id: &str,
        request: &DaShareRequest,
        rate_limiter: &mut DaShareRequestRateLimiter,
    ) -> Result<Vec<NetworkMessage>, NodeError> {
        rate_limiter
            .admit(peer_id)
            .map_err(NodeError::DataAvailability)?;
        self.serve_da_share_request(request)
    }

    pub fn persist_da_certificate(
        &self,
        certificate: &DaAvailabilityCertificate,
    ) -> Result<String, NodeError> {
        self.storage
            .commit_da_certificate(certificate)
            .map_err(NodeError::Storage)
    }

    pub fn load_da_certificate(
        &self,
        certificate_hash: &str,
    ) -> Result<DaAvailabilityCertificate, NodeError> {
        self.storage
            .load_da_certificate(certificate_hash)
            .map_err(NodeError::Storage)
    }

    pub fn load_da_share_set(&self, manifest_hash: &str) -> Result<DaShareSet, NodeError> {
        self.storage
            .load_da_share_set(manifest_hash)
            .map_err(NodeError::Storage)
    }

    pub fn load_da_payload(&self, manifest_hash: &str) -> Result<DaPayload, NodeError> {
        if let Some(payload) = self
            .storage
            .maybe_load_da_payload(manifest_hash)
            .map_err(NodeError::Storage)?
        {
            return Ok(payload);
        }
        let payload = self
            .load_da_share_set(manifest_hash)?
            .reconstruct_payload()
            .map_err(NodeError::DataAvailability)?;
        self.storage
            .commit_da_payload(manifest_hash, &payload)
            .map_err(NodeError::Storage)?;
        Ok(payload)
    }

    pub fn load_application_da_payload(
        &self,
        manifest_hash: &str,
    ) -> Result<ApplicationDaPayload, NodeError> {
        if let Some(payload) = self
            .storage
            .maybe_load_application_da_payload(manifest_hash)
            .map_err(NodeError::Storage)?
        {
            return Ok(payload);
        }

        let manifest = self
            .storage
            .maybe_load_application_da_manifest(manifest_hash)
            .map_err(NodeError::Storage)?
            .ok_or(NodeError::Rpc(RpcError::ApplicationDaManifestNotFound))?;
        let registration = self
            .storage
            .load_application_da_profile_registration(&manifest.profile_id)
            .map_err(NodeError::Storage)?;
        let shares =
            self.local_application_da_shares(manifest_hash, manifest.encoded_share_count)?;
        let share_set = ApplicationDaShareSet { manifest, shares };
        let payload = share_set
            .reconstruct_payload(&registration.profile)
            .map_err(NodeError::DataAvailability)?;
        self.storage
            .commit_application_da_payload(manifest_hash, &payload)
            .map_err(NodeError::Storage)?;
        Ok(payload)
    }

    fn local_application_da_shares(
        &self,
        manifest_hash: &str,
        encoded_share_count: u32,
    ) -> Result<Vec<DaShare>, NodeError> {
        let mut shares = Vec::new();
        for index in 0..encoded_share_count {
            if let Some(share) = self
                .storage
                .maybe_load_application_da_share(manifest_hash, index)
                .map_err(NodeError::Storage)?
            {
                shares.push(share);
            }
        }
        Ok(shares)
    }

    pub fn build_snapshot_da_share_set(
        &self,
        max_snapshot_chunk_bytes: usize,
        data_share_count: u32,
        parity_share_count: u32,
    ) -> Result<(SnapshotChunkSet, DaShareSet), NodeError> {
        let snapshot = self.storage.load_snapshot().map_err(NodeError::Storage)?;
        let metadata_roots = self.effective_snapshot_metadata_roots()?;
        let chunk_set = build_snapshot_chunks_with_metadata_roots(
            &snapshot,
            max_snapshot_chunk_bytes,
            metadata_roots,
        )
        .map_err(NodeError::SnapshotSync)?;
        let payload = da_payload_for_snapshot_chunk_set(
            self.chain_id.clone(),
            self.current_height(),
            self.checkpoint_previous_block_hash()?,
            &chunk_set,
        )?;
        let checkpoint_hash = snapshot_checkpoint_block_hash(&chunk_set)?;
        let share_set = DaShareSet::from_payload_reed_solomon(
            &payload,
            checkpoint_hash,
            data_share_count,
            parity_share_count,
        )
        .map_err(NodeError::DataAvailability)?;
        Ok((chunk_set, share_set))
    }

    pub fn persist_snapshot_da_share_set(
        &self,
        max_snapshot_chunk_bytes: usize,
        data_share_count: u32,
        parity_share_count: u32,
    ) -> Result<(SnapshotChunkSet, String), NodeError> {
        let (chunk_set, share_set) = self.build_snapshot_da_share_set(
            max_snapshot_chunk_bytes,
            data_share_count,
            parity_share_count,
        )?;
        let manifest_hash = self
            .storage
            .commit_da_share_set(&share_set)
            .map_err(NodeError::Storage)?;
        Ok((chunk_set, manifest_hash))
    }

    pub fn import_snapshot_from_da_share_set(
        &self,
        share_set: &DaShareSet,
        required_metadata_roots: &BTreeMap<String, String>,
    ) -> Result<StateSnapshot, NodeError> {
        self.import_snapshot_from_da_share_set_with_certificate(
            share_set,
            None,
            required_metadata_roots,
        )
    }

    pub fn import_snapshot_from_da_share_set_with_certificate(
        &self,
        share_set: &DaShareSet,
        certificate: Option<&DaAvailabilityCertificate>,
        required_metadata_roots: &BTreeMap<String, String>,
    ) -> Result<StateSnapshot, NodeError> {
        let da_manifest_hash = share_set
            .manifest
            .manifest_hash()
            .map_err(NodeError::DataAvailability)?;
        let da_certificate_hash = match certificate {
            Some(certificate) => {
                validate_da_certificate_for_share_set(certificate, share_set)?;
                Some(
                    certificate
                        .certificate_hash()
                        .map_err(NodeError::DataAvailability)?,
                )
            }
            None => None,
        };
        let payload = share_set
            .reconstruct_payload()
            .map_err(NodeError::DataAvailability)?;
        let chunk_set = snapshot_chunk_set_from_da_payload(&payload)?;
        self.import_snapshot_chunk_set_with_da_audit(
            &chunk_set,
            required_metadata_roots,
            Some(da_manifest_hash),
            da_certificate_hash,
        )
    }

    pub fn import_da_certified_block_after_checkpoint(
        &mut self,
        share_set: &DaShareSet,
        certificate: &DaAvailabilityCertificate,
    ) -> Result<Block, NodeError> {
        validate_da_certificate_for_share_set(certificate, share_set)?;
        let payload = share_set
            .reconstruct_payload()
            .map_err(NodeError::DataAvailability)?;
        let mut block = block_from_da_payload(&payload)?;
        let execution_block_hash = block.block_hash();
        if execution_block_hash != share_set.manifest.block_hash {
            return Err(NodeError::DataAvailability(DaError::InvalidPayload(
                "DA block payload does not match manifest block hash".into(),
            )));
        }
        let manifest_hash = share_set
            .manifest
            .manifest_hash()
            .map_err(NodeError::DataAvailability)?;
        block.header.data_availability = Some(DataAvailabilityCommitment {
            payload_root: share_set.manifest.payload_hash.clone(),
            manifest_hash,
            share_root: share_set.manifest.share_root.clone(),
            certificate_hash: Some(
                certificate
                    .certificate_hash()
                    .map_err(NodeError::DataAvailability)?,
            ),
        });
        self.import_block(&block)?;
        Ok(block)
    }

    pub fn load_da_namespace(
        &self,
        manifest_hash: &str,
        namespace: &str,
    ) -> Result<Option<DaNamespaceSection>, NodeError> {
        let namespace = DaNamespace::new(namespace).map_err(NodeError::DataAvailability)?;
        let payload = self.load_da_payload(manifest_hash)?;
        Ok(payload
            .namespaces
            .into_iter()
            .find(|section| section.namespace == namespace))
    }

    pub fn load_application_da_namespace(
        &self,
        manifest_hash: &str,
        namespace: &str,
    ) -> Result<Option<ApplicationDaNamespaceSection>, NodeError> {
        let namespace = DaNamespace::new(namespace).map_err(NodeError::DataAvailability)?;
        let payload = self.load_application_da_payload(manifest_hash)?;
        Ok(payload
            .namespaces
            .into_iter()
            .find(|section| section.namespace == namespace))
    }

    pub fn da_sample_proof_bundle(
        &self,
        manifest_hash: &str,
        client_randomness: &str,
        sample_count: u32,
        namespaces: &[String],
    ) -> Result<DaSampleProofBundle, NodeError> {
        let manifest = self
            .storage
            .maybe_load_da_manifest(manifest_hash)
            .map_err(NodeError::Storage)?
            .ok_or(NodeError::Rpc(RpcError::DaManifestNotFound))?;
        let schedule =
            derive_sample_schedule(&manifest, client_randomness.as_bytes(), sample_count)
                .map_err(NodeError::DataAvailability)?;
        let mut sample_proofs = Vec::with_capacity(schedule.share_indices.len());
        for index in &schedule.share_indices {
            let share = self
                .storage
                .maybe_load_da_share(manifest_hash, *index)
                .map_err(NodeError::Storage)?
                .ok_or(NodeError::Rpc(RpcError::DaShareNotFound))?;
            sample_proofs.push(DaSampleProof {
                share,
                inclusion_proof: prove_share_inclusion(&manifest, *index)
                    .map_err(NodeError::DataAvailability)?,
            });
        }

        let namespace_proofs = namespaces
            .iter()
            .map(|namespace| {
                let namespace =
                    DaNamespace::new(namespace.clone()).map_err(NodeError::DataAvailability)?;
                prove_namespace(&manifest, &namespace).map_err(NodeError::DataAvailability)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let verification = verify_light_client_samples(
            &manifest,
            client_randomness.as_bytes(),
            sample_count,
            &sample_proofs,
            &namespace_proofs,
        )
        .map_err(NodeError::DataAvailability)?;

        Ok(DaSampleProofBundle {
            schedule,
            sample_proofs,
            namespace_proofs,
            verification,
        })
    }

    pub fn application_da_sample_proof_bundle(
        &self,
        manifest_hash: &str,
        client_randomness: &str,
        sample_count: u32,
        namespaces: &[String],
    ) -> Result<ApplicationDaSampleProofBundle, NodeError> {
        let manifest = self
            .storage
            .maybe_load_application_da_manifest(manifest_hash)
            .map_err(NodeError::Storage)?
            .ok_or(NodeError::Rpc(RpcError::ApplicationDaManifestNotFound))?;
        let registration = self
            .storage
            .load_application_da_profile_registration(&manifest.profile_id)
            .map_err(NodeError::Storage)?;
        let profile: DaApplicationProfile = registration.profile;
        let schedule = derive_application_sample_schedule(
            &manifest,
            &profile,
            client_randomness.as_bytes(),
            sample_count,
        )
        .map_err(NodeError::DataAvailability)?;
        let mut sample_proofs = Vec::with_capacity(schedule.share_indices.len());
        for index in &schedule.share_indices {
            let share = self
                .storage
                .maybe_load_application_da_share(manifest_hash, *index)
                .map_err(NodeError::Storage)?
                .ok_or(NodeError::Rpc(RpcError::ApplicationDaShareNotFound))?;
            sample_proofs.push(DaSampleProof {
                share,
                inclusion_proof: prove_application_share_inclusion(&manifest, &profile, *index)
                    .map_err(NodeError::DataAvailability)?,
            });
        }

        let namespace_proofs = namespaces
            .iter()
            .map(|namespace| {
                let namespace =
                    DaNamespace::new(namespace.clone()).map_err(NodeError::DataAvailability)?;
                prove_application_namespace(&manifest, &profile, &namespace)
                    .map_err(NodeError::DataAvailability)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let verification = verify_application_light_client_samples(
            &manifest,
            &profile,
            client_randomness.as_bytes(),
            sample_count,
            &sample_proofs,
            &namespace_proofs,
        )
        .map_err(NodeError::DataAvailability)?;

        Ok(ApplicationDaSampleProofBundle {
            schedule,
            sample_proofs,
            namespace_proofs,
            verification,
        })
    }

    pub fn da_status(&self, manifest_hash: &str) -> Result<DaStatusReport, NodeError> {
        let certificate_hash = self.da_certificate_hash_for_manifest(manifest_hash)?;
        let certificate_available = match &certificate_hash {
            Some(hash) => self
                .storage
                .maybe_load_da_certificate(hash)
                .map_err(NodeError::Storage)?
                .is_some(),
            None => false,
        };

        let Some(manifest) = self
            .storage
            .maybe_load_da_manifest(manifest_hash)
            .map_err(NodeError::Storage)?
        else {
            return Ok(DaStatusReport {
                manifest_hash: manifest_hash.into(),
                manifest_available: false,
                certificate_hash,
                certificate_available,
                expected_share_count: 0,
                stored_share_count: 0,
                missing_share_indices: Vec::new(),
                payload_reconstructable: false,
                payload_bytes: None,
                namespace_count: None,
                reconstruction_error: Some("manifest not found".into()),
            });
        };

        let mut stored_share_count = 0_u32;
        let mut missing_share_indices = Vec::new();
        for index in 0..manifest.encoded_share_count {
            match self
                .storage
                .maybe_load_da_share(manifest_hash, index)
                .map_err(NodeError::Storage)?
            {
                Some(_) => stored_share_count = stored_share_count.saturating_add(1),
                None => missing_share_indices.push(index),
            }
        }

        let reconstruction = self.load_da_payload(manifest_hash);
        Ok(DaStatusReport {
            manifest_hash: manifest_hash.into(),
            manifest_available: true,
            certificate_hash,
            certificate_available,
            expected_share_count: manifest.encoded_share_count,
            stored_share_count,
            missing_share_indices,
            payload_reconstructable: reconstruction.is_ok(),
            payload_bytes: Some(manifest.payload_bytes),
            namespace_count: Some(manifest.namespace_ranges.len()),
            reconstruction_error: reconstruction.err().map(|error| format!("{error:?}")),
        })
    }

    pub fn application_da_status(
        &self,
        manifest_hash: &str,
    ) -> Result<ApplicationDaStatusReport, NodeError> {
        let certificate_hash = self.application_da_certificate_hash_for_manifest(manifest_hash)?;
        let certificate_available = match &certificate_hash {
            Some(hash) => self
                .storage
                .maybe_load_application_da_certificate(hash)
                .map_err(NodeError::Storage)?
                .is_some(),
            None => false,
        };

        let Some(manifest) = self
            .storage
            .maybe_load_application_da_manifest(manifest_hash)
            .map_err(NodeError::Storage)?
        else {
            return Ok(ApplicationDaStatusReport {
                manifest_hash: manifest_hash.into(),
                manifest_available: false,
                application_id: None,
                profile_id: None,
                coordinate: None,
                certificate_hash,
                certificate_available,
                expected_share_count: 0,
                stored_share_count: 0,
                missing_share_indices: Vec::new(),
                payload_reconstructable: false,
                payload_bytes: None,
                namespace_count: None,
                reconstruction_error: Some("manifest not found".into()),
            });
        };

        let mut stored_share_count = 0_u32;
        let mut missing_share_indices = Vec::new();
        for index in 0..manifest.encoded_share_count {
            match self
                .storage
                .maybe_load_application_da_share(manifest_hash, index)
                .map_err(NodeError::Storage)?
            {
                Some(_) => stored_share_count = stored_share_count.saturating_add(1),
                None => missing_share_indices.push(index),
            }
        }

        let reconstruction = self.load_application_da_payload(manifest_hash);
        Ok(ApplicationDaStatusReport {
            manifest_hash: manifest_hash.into(),
            manifest_available: true,
            application_id: Some(manifest.application_id),
            profile_id: Some(manifest.profile_id),
            coordinate: Some(manifest.coordinate),
            certificate_hash,
            certificate_available,
            expected_share_count: manifest.encoded_share_count,
            stored_share_count,
            missing_share_indices,
            payload_reconstructable: reconstruction.is_ok(),
            payload_bytes: Some(manifest.payload_bytes),
            namespace_count: Some(manifest.namespace_ranges.len()),
            reconstruction_error: reconstruction.err().map(|error| format!("{error:?}")),
        })
    }

    pub fn da_repair_status(&self, manifest_hash: &str) -> Result<DaRepairStatusReport, NodeError> {
        let status = self.da_status(manifest_hash)?;
        Ok(DaRepairStatusReport {
            manifest_hash: status.manifest_hash,
            repair_needed: !status.missing_share_indices.is_empty()
                || !status.payload_reconstructable,
            pending_repair_count: status.missing_share_indices.len(),
            missing_share_indices: status.missing_share_indices,
            payload_reconstructable: status.payload_reconstructable,
            reconstruction_error: status.reconstruction_error,
        })
    }

    /// Evaluate whether a locally stored manifest's committed shares are a valid
    /// erasure encoding of its committed payload, building a transferable,
    /// slashable coding-fraud proof from the proposer's own committed data shares
    /// when they are not. Requires the manifest and all `original_share_count`
    /// data shares to be locally present; coding correctness cannot be judged
    /// from a partial share set.
    pub fn da_coding_fraud_proof(
        &self,
        manifest_hash: &str,
    ) -> Result<DaCodingFraudReport, NodeError> {
        let Some(manifest) = self
            .storage
            .maybe_load_da_manifest(manifest_hash)
            .map_err(NodeError::Storage)?
        else {
            return Ok(DaCodingFraudReport {
                manifest_hash: manifest_hash.into(),
                manifest_available: false,
                expected_data_share_count: 0,
                available_data_share_count: 0,
                fault_detected: false,
                fault: None,
                proof: None,
                detail: Some("manifest not found".into()),
            });
        };

        let mut data_shares = Vec::new();
        for index in 0..manifest.original_share_count {
            if let Some(share) = self
                .storage
                .maybe_load_da_share(manifest_hash, index)
                .map_err(NodeError::Storage)?
            {
                data_shares.push(share);
            }
        }
        let available_data_share_count = data_shares.len() as u32;

        if available_data_share_count < manifest.original_share_count {
            return Ok(DaCodingFraudReport {
                manifest_hash: manifest_hash.into(),
                manifest_available: true,
                expected_data_share_count: manifest.original_share_count,
                available_data_share_count,
                fault_detected: false,
                fault: None,
                proof: None,
                detail: Some(format!(
                    "{available_data_share_count} of {} data shares available; cannot evaluate coding correctness",
                    manifest.original_share_count
                )),
            });
        }

        let reporter_id = self.validator_id.clone();
        match DaCodingFraudProof::from_committed_data_shares(&manifest, &data_shares, reporter_id) {
            Ok(proof) => Ok(DaCodingFraudReport {
                manifest_hash: manifest_hash.into(),
                manifest_available: true,
                expected_data_share_count: manifest.original_share_count,
                available_data_share_count,
                fault_detected: true,
                fault: Some(proof.fault.clone()),
                proof: Some(proof),
                detail: Some("manifest does not commit a valid encoding of its payload".into()),
            }),
            Err(DaError::InvalidCodingFraudProof(_)) => Ok(DaCodingFraudReport {
                manifest_hash: manifest_hash.into(),
                manifest_available: true,
                expected_data_share_count: manifest.original_share_count,
                available_data_share_count,
                fault_detected: false,
                fault: None,
                proof: None,
                detail: Some("manifest commits a valid encoding of its payload".into()),
            }),
            Err(error) => Ok(DaCodingFraudReport {
                manifest_hash: manifest_hash.into(),
                manifest_available: true,
                expected_data_share_count: manifest.original_share_count,
                available_data_share_count,
                fault_detected: false,
                fault: None,
                proof: None,
                detail: Some(format!(
                    "could not evaluate coding correctness from local shares: {error:?}"
                )),
            }),
        }
    }

    pub fn da_retention_audit(&self) -> Result<DaRetentionAuditReport, NodeError> {
        self.storage
            .da_retention_audit(self.current_height())
            .map_err(NodeError::Storage)
    }

    pub fn da_retention_prune_plan(&self) -> Result<DaRetentionPrunePlanReport, NodeError> {
        self.storage
            .da_retention_prune_plan(self.current_height())
            .map_err(NodeError::Storage)
    }

    fn da_certificate_hash_for_manifest(
        &self,
        manifest_hash: &str,
    ) -> Result<Option<String>, NodeError> {
        let committed_certificate_hash = self
            .storage
            .load_blocks()
            .map_err(NodeError::Storage)?
            .into_iter()
            .find_map(|block| {
                let commitment = block.header.data_availability?;
                if commitment.manifest_hash == manifest_hash {
                    commitment.certificate_hash
                } else {
                    None
                }
            });
        if committed_certificate_hash.is_some() {
            return Ok(committed_certificate_hash);
        }

        Ok(self
            .storage
            .load_da_certificate_index_by_manifest_hash(manifest_hash)
            .map_err(NodeError::Storage)?
            .first()
            .map(|entry| entry.certificate_hash.clone()))
    }

    fn application_da_certificate_hash_for_manifest(
        &self,
        manifest_hash: &str,
    ) -> Result<Option<String>, NodeError> {
        Ok(self
            .storage
            .load_application_da_certificate_index_by_manifest_hash(manifest_hash)
            .map_err(NodeError::Storage)?
            .first()
            .map(|entry| entry.certificate_hash.clone()))
    }

    fn pending_transactions_for_next_block(&self) -> Vec<Transaction> {
        let mut transactions = self.rpc.node().pending_transactions().to_vec();
        transactions.sort_by(|left, right| {
            (left.sender.as_str(), left.nonce, left.tx_hash.as_str()).cmp(&(
                right.sender.as_str(),
                right.nonce,
                right.tx_hash.as_str(),
            ))
        });

        let mut selected = Vec::new();
        let mut used_units = 0_u64;
        for tx in transactions {
            let tx_units = transaction_resource_units(&tx);
            let next_units = used_units.saturating_add(tx_units);
            if next_units <= self.rpc.node().block_resource_limit() {
                used_units = next_units;
                selected.push(tx);
            }
        }
        selected
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
        self.import_snapshot_chunk_set_with_da_audit(chunk_set, required_metadata_roots, None, None)
    }

    fn import_snapshot_chunk_set_with_da_audit(
        &self,
        chunk_set: &SnapshotChunkSet,
        required_metadata_roots: &BTreeMap<String, String>,
        da_manifest_hash: Option<String>,
        da_certificate_hash: Option<String>,
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
            .append_snapshot_import_audit_record_with_retention(
                SnapshotImportAuditRecord {
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
                    da_manifest_hash,
                    da_certificate_hash,
                },
                self.max_snapshot_import_audit_records,
            )
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

    pub fn verified_da_availability_vote(
        &self,
        message: &NetworkMessage,
    ) -> Result<DaAvailabilityVote, NodeError> {
        let NetworkMessage::SignedValidator(signed) = message else {
            return Err(NodeError::UnexpectedSignedMessage {
                expected: ProtocolMessageKind::DaAvailabilityVote,
                actual: message.kind(),
            });
        };
        match self.verified_signed_validator_message(signed)? {
            NetworkMessage::DaAvailabilityVote(vote) => Ok(vote),
            other => Err(NodeError::UnexpectedSignedMessage {
                expected: ProtocolMessageKind::DaAvailabilityVote,
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

    pub fn collect_da_availability_certificate(
        &self,
        manifest: &DaManifest,
        messages: &[NetworkMessage],
        quorum: usize,
    ) -> Result<DaAvailabilityCertificate, NodeError> {
        let votes = messages
            .iter()
            .map(|message| self.verified_da_availability_vote(message))
            .collect::<Result<Vec<_>, _>>()?;
        ConsensusCluster::data_availability_certificate_from_votes(manifest, votes, quorum)
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

    fn reserve_consensus_signing_record(&self, message: &NetworkMessage) -> Result<(), NodeError> {
        let Some(commitment) = consensus_signing_commitment(&self.validator_id, message)? else {
            return Ok(());
        };
        let record = ConsensusSigningRecord {
            validator_id: self.validator_id.clone(),
            domain: commitment.domain,
            height: commitment.height,
            block_hash: commitment.block_hash,
        };
        if let Some(existing) = self
            .storage
            .commit_consensus_signing_record_if_absent(&record)
            .map_err(NodeError::Storage)?
        {
            if existing.block_hash != record.block_hash {
                return Err(NodeError::ConsensusSigningConflict {
                    validator_id: record.validator_id,
                    domain: record.domain,
                    height: record.height,
                    existing_block_hash: existing.block_hash,
                    attempted_block_hash: record.block_hash,
                });
            }
        }
        Ok(())
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct ConsensusSigningCommitment {
    domain: ValidatorSignatureDomain,
    height: u64,
    block_hash: String,
}

fn consensus_signing_commitment(
    validator_id: &str,
    message: &NetworkMessage,
) -> Result<Option<ConsensusSigningCommitment>, NodeError> {
    match message {
        NetworkMessage::Block(block) => {
            if block.header.proposer != validator_id {
                return Err(NodeError::ConsensusMessageSignerMismatch {
                    expected: validator_id.to_string(),
                    actual: block.header.proposer.clone(),
                });
            }
            Ok(Some(ConsensusSigningCommitment {
                domain: ValidatorSignatureDomain::BlockProposal,
                height: block.header.height,
                block_hash: block.block_hash(),
            }))
        }
        NetworkMessage::Vote(vote) => {
            if vote.validator_id != validator_id {
                return Err(NodeError::ConsensusMessageSignerMismatch {
                    expected: validator_id.to_string(),
                    actual: vote.validator_id.clone(),
                });
            }
            Ok(Some(ConsensusSigningCommitment {
                domain: ValidatorSignatureDomain::Vote,
                height: vote.height,
                block_hash: vote.block_hash.clone(),
            }))
        }
        NetworkMessage::DaAvailabilityVote(vote) => {
            if vote.validator_id != validator_id {
                return Err(NodeError::ConsensusMessageSignerMismatch {
                    expected: validator_id.to_string(),
                    actual: vote.validator_id.clone(),
                });
            }
            Ok(Some(ConsensusSigningCommitment {
                domain: ValidatorSignatureDomain::DaAvailabilityVote,
                height: vote.height,
                block_hash: da_vote_signing_commitment(vote),
            }))
        }
        NetworkMessage::FinalityCertificate(certificate) => Ok(Some(ConsensusSigningCommitment {
            domain: ValidatorSignatureDomain::FinalityCertificate,
            height: certificate.height,
            block_hash: certificate.block_hash.clone(),
        })),
        NetworkMessage::DaAvailabilityCertificate(certificate) => {
            Ok(Some(ConsensusSigningCommitment {
                domain: ValidatorSignatureDomain::DaAvailabilityCertificate,
                height: certificate.height,
                block_hash: format!("{}:{}", certificate.block_hash, certificate.manifest_hash),
            }))
        }
        _ => Ok(None),
    }
}

fn da_vote_signing_commitment(vote: &DaAvailabilityVote) -> String {
    format!("{}:{}", vote.block_hash, vote.manifest_hash)
}

fn da_payload_for_block(block: &Block) -> Result<DaPayload, NodeError> {
    let mut sections = vec![DaNamespaceSection::new(
        DaNamespace::new("detta.block").map_err(NodeError::DataAvailability)?,
        vec![DaRecord::BlockHeader(Box::new(block.header.clone()))],
    )
    .map_err(NodeError::DataAvailability)?];

    if !block.transactions.is_empty() {
        sections.push(
            DaNamespaceSection::new(
                DaNamespace::new("detta.tx").map_err(NodeError::DataAvailability)?,
                block
                    .transactions
                    .iter()
                    .cloned()
                    .map(DaRecord::SignedTransaction)
                    .collect(),
            )
            .map_err(NodeError::DataAvailability)?,
        );
    }
    if !block.receipts.is_empty() {
        sections.push(
            DaNamespaceSection::new(
                DaNamespace::new("detta.receipt").map_err(NodeError::DataAvailability)?,
                block
                    .receipts
                    .iter()
                    .cloned()
                    .map(DaRecord::Receipt)
                    .collect(),
            )
            .map_err(NodeError::DataAvailability)?,
        );
    }
    push_da_section(
        &mut sections,
        "detta.aspect",
        aspect_da_records_for_block(block)?,
    )?;
    push_da_section(
        &mut sections,
        "detta.bridge",
        method_payload_da_records_for_block(
            block,
            |method| {
                matches!(
                    method,
                    Method::QueueBridgeMessage | Method::RedeemBridgeMessage
                )
            },
            |tx, payload| DaRecord::BridgeProof {
                message_id: text_arg(tx, 2)
                    .or_else(|| text_arg(tx, 0))
                    .unwrap_or(tx.tx_hash.as_str())
                    .to_string(),
                proof: payload,
            },
        )?,
    )?;
    push_da_section(
        &mut sections,
        "detta.governance",
        method_payload_da_records_for_block(
            block,
            |method| {
                matches!(
                    method,
                    Method::PauseContract
                        | Method::UnpauseContract
                        | Method::ScheduleUpgrade
                        | Method::ExecuteUpgrade
                        | Method::SchedulePolicyUpdate
                        | Method::ExecutePolicyUpdate
                        | Method::ScheduleDaSlashingPolicyUpdate
                        | Method::ExecuteDaSlashingPolicyUpdate
                )
            },
            |tx, payload| DaRecord::GovernancePayload {
                proposal_id: text_arg(tx, 0).unwrap_or(tx.tx_hash.as_str()).to_string(),
                payload,
            },
        )?,
    )?;
    push_da_section(
        &mut sections,
        "detta.oracle",
        method_payload_da_records_for_block(
            block,
            |method| matches!(method, Method::SubmitPrice),
            |tx, payload| DaRecord::OracleEvidence {
                asset: asset_arg(tx, 0).unwrap_or(tx.target.as_str()).to_string(),
                evidence: payload,
            },
        )?,
    )?;

    let payload = DaPayload::new(
        block.header.chain_id.clone(),
        block.header.height,
        block.header.previous_block_hash.clone(),
        sections,
    )
    .map_err(NodeError::DataAvailability)?;
    validate_production_block_payload(&payload, &DaProductionProfile::v1())
        .map_err(NodeError::DataAvailability)?;
    Ok(payload)
}

fn push_da_section(
    sections: &mut Vec<DaNamespaceSection>,
    namespace: &str,
    records: Vec<DaRecord>,
) -> Result<(), NodeError> {
    if records.is_empty() {
        return Ok(());
    }
    sections.push(
        DaNamespaceSection::new(
            DaNamespace::new(namespace).map_err(NodeError::DataAvailability)?,
            records,
        )
        .map_err(NodeError::DataAvailability)?,
    );
    Ok(())
}

fn committed_da_transactions(block: &Block) -> impl Iterator<Item = &Transaction> {
    block
        .transactions
        .iter()
        .zip(block.receipts.iter())
        .filter_map(|(transaction, receipt)| {
            if receipt.tx_hash == transaction.tx_hash && receipt.status == TxStatus::Committed {
                Some(transaction)
            } else {
                None
            }
        })
}

fn aspect_da_records_for_block(block: &Block) -> Result<Vec<DaRecord>, NodeError> {
    committed_da_transactions(block)
        .filter(|transaction| transaction.method == Method::SubmitAspectModule)
        .map(aspect_da_record_for_transaction)
        .collect()
}

fn aspect_da_record_for_transaction(transaction: &Transaction) -> Result<DaRecord, NodeError> {
    let module = match transaction.args.as_slice() {
        [Argument::Text(module_id), Argument::Text(source)] => {
            AspectModuleRecord::from_verified_source(module_id.clone(), source).map_err(
                |error| {
                    NodeError::DataAvailability(DaError::InvalidPayload(format!(
                        "failed to reconstruct DA aspect artifact from source: {error:?}"
                    )))
                },
            )?
        }
        [Argument::Text(module_id), Argument::Text(taxonomy_version), Argument::Text(source_root), Argument::Text(ir_root), Argument::Text(abi_root), Argument::Text(policy_root), Argument::Text(storage_schema_root), Argument::Text(registry_schema_root), Argument::Text(invariant_root)] => {
            AspectModuleRecord::new(
                module_id.clone(),
                taxonomy_version.clone(),
                AspectModuleRoots {
                    source_root: source_root.clone(),
                    ir_root: ir_root.clone(),
                    abi_root: abi_root.clone(),
                    policy_root: policy_root.clone(),
                    storage_schema_root: storage_schema_root.clone(),
                    registry_schema_root: registry_schema_root.clone(),
                    invariant_root: invariant_root.clone(),
                },
            )
        }
        _ => {
            return Err(NodeError::DataAvailability(DaError::InvalidPayload(
                "committed SubmitAspectModule transaction has unsupported DA argument shape".into(),
            )));
        }
    };
    Ok(DaRecord::AspectArtifact(Box::new(module)))
}

fn method_payload_da_records_for_block(
    block: &Block,
    include_method: impl Fn(&Method) -> bool,
    build_record: impl Fn(&Transaction, String) -> DaRecord,
) -> Result<Vec<DaRecord>, NodeError> {
    committed_da_transactions(block)
        .filter(|transaction| include_method(&transaction.method))
        .map(|transaction| {
            canonical_transaction_payload(transaction)
                .map(|payload| build_record(transaction, payload))
        })
        .collect()
}

fn canonical_transaction_payload(transaction: &Transaction) -> Result<String, NodeError> {
    serde_json::to_string(transaction).map_err(|error| {
        NodeError::DataAvailability(DaError::InvalidPayload(format!(
            "failed to encode DA transaction evidence: {error}"
        )))
    })
}

fn text_arg(transaction: &Transaction, index: usize) -> Option<&str> {
    match transaction.args.get(index) {
        Some(Argument::Text(value)) => Some(value),
        _ => None,
    }
}

fn asset_arg(transaction: &Transaction, index: usize) -> Option<&str> {
    match transaction.args.get(index) {
        Some(Argument::Asset(value)) => Some(value),
        _ => None,
    }
}

fn da_payload_for_snapshot_chunk_set(
    chain_id: impl Into<ChainId>,
    height: u64,
    previous_block_hash: impl Into<String>,
    chunk_set: &SnapshotChunkSet,
) -> Result<DaPayload, NodeError> {
    chunk_set.verify().map_err(NodeError::SnapshotSync)?;
    let manifest_hash = chunk_set
        .manifest
        .manifest_hash()
        .map_err(NodeError::SnapshotSync)?;
    let mut records = vec![DaRecord::SnapshotChunkManifest {
        snapshot_root: chunk_set.manifest.snapshot_root.clone(),
        snapshot_hash: chunk_set.manifest.snapshot_hash.clone(),
        metadata_roots: chunk_set.manifest.metadata_roots.clone(),
        chunk_size: chunk_set.manifest.chunk_size,
        total_bytes: chunk_set.manifest.total_bytes,
        chunk_count: chunk_set.manifest.chunk_count,
        chunk_hashes: chunk_set.manifest.chunk_hashes.clone(),
        chunk_root: chunk_set.manifest.chunk_root.clone(),
    }];
    records.extend(
        chunk_set
            .chunks
            .iter()
            .map(|chunk| DaRecord::SnapshotChunk {
                snapshot_root: chunk_set.manifest.snapshot_root.clone(),
                manifest_hash: manifest_hash.clone(),
                chunk_index: chunk.index,
                chunk_hash: chunk.chunk_hash.clone(),
                bytes: chunk.bytes.clone(),
            }),
    );

    DaPayload::new(
        chain_id,
        height,
        previous_block_hash,
        vec![DaNamespaceSection::new(
            DaNamespace::new("detta.snapshot").map_err(NodeError::DataAvailability)?,
            records,
        )
        .map_err(NodeError::DataAvailability)?],
    )
    .map_err(NodeError::DataAvailability)
}

fn snapshot_chunk_set_from_da_payload(payload: &DaPayload) -> Result<SnapshotChunkSet, NodeError> {
    payload.validate().map_err(NodeError::DataAvailability)?;
    let snapshot_namespace =
        DaNamespace::new("detta.snapshot").map_err(NodeError::DataAvailability)?;
    let mut manifest = None;
    let mut chunks = Vec::new();
    let mut chunk_snapshot_roots = BTreeMap::new();

    for section in &payload.namespaces {
        if section.namespace != snapshot_namespace {
            continue;
        }
        for record in &section.records {
            match record {
                DaRecord::SnapshotChunkManifest {
                    snapshot_root,
                    snapshot_hash,
                    metadata_roots,
                    chunk_size,
                    total_bytes,
                    chunk_count,
                    chunk_hashes,
                    chunk_root,
                } => {
                    if manifest.is_some() {
                        return Err(NodeError::DataAvailability(DaError::InvalidPayload(
                            "DA snapshot payload contains multiple manifests".into(),
                        )));
                    }
                    manifest = Some(SnapshotChunkManifest {
                        snapshot_root: snapshot_root.clone(),
                        snapshot_hash: snapshot_hash.clone(),
                        metadata_roots: metadata_roots.clone(),
                        chunk_size: *chunk_size,
                        total_bytes: *total_bytes,
                        chunk_count: *chunk_count,
                        chunk_hashes: chunk_hashes.clone(),
                        chunk_root: chunk_root.clone(),
                    });
                }
                DaRecord::SnapshotChunk {
                    snapshot_root,
                    manifest_hash,
                    chunk_index,
                    chunk_hash,
                    bytes,
                } => {
                    chunk_snapshot_roots.insert(*chunk_index, snapshot_root.clone());
                    chunks.push(SnapshotChunk {
                        manifest_hash: manifest_hash.clone(),
                        index: *chunk_index,
                        bytes: bytes.clone(),
                        chunk_hash: chunk_hash.clone(),
                    });
                }
                _ => {
                    return Err(NodeError::DataAvailability(DaError::InvalidPayload(
                        "DA snapshot namespace contains a non-snapshot record".into(),
                    )));
                }
            }
        }
    }

    let manifest = manifest.ok_or_else(|| {
        NodeError::DataAvailability(DaError::InvalidPayload(
            "DA snapshot payload has no manifest".into(),
        ))
    })?;
    for (chunk_index, snapshot_root) in chunk_snapshot_roots {
        if snapshot_root != manifest.snapshot_root {
            return Err(NodeError::DataAvailability(DaError::InvalidPayload(
                format!("DA snapshot chunk {chunk_index} root does not match manifest root"),
            )));
        }
    }

    let chunk_set = SnapshotChunkSet { manifest, chunks };
    chunk_set.verify().map_err(NodeError::SnapshotSync)?;
    Ok(chunk_set)
}

fn snapshot_checkpoint_block_hash(chunk_set: &SnapshotChunkSet) -> Result<String, NodeError> {
    Ok(format!(
        "snapshot-checkpoint:{}",
        chunk_set
            .manifest
            .manifest_hash()
            .map_err(NodeError::SnapshotSync)?
    ))
}

fn block_from_da_payload(payload: &DaPayload) -> Result<Block, NodeError> {
    payload.validate().map_err(NodeError::DataAvailability)?;
    validate_production_block_payload(payload, &DaProductionProfile::v1())
        .map_err(NodeError::DataAvailability)?;
    let block_namespace = DaNamespace::new("detta.block").map_err(NodeError::DataAvailability)?;
    let tx_namespace = DaNamespace::new("detta.tx").map_err(NodeError::DataAvailability)?;
    let receipt_namespace =
        DaNamespace::new("detta.receipt").map_err(NodeError::DataAvailability)?;
    let mut header = None;
    let mut transactions = Vec::new();
    let mut receipts = Vec::new();

    for section in &payload.namespaces {
        if section.namespace == block_namespace {
            for record in &section.records {
                let DaRecord::BlockHeader(block_header) = record else {
                    return Err(NodeError::DataAvailability(DaError::InvalidPayload(
                        "DA block namespace contains a non-header record".into(),
                    )));
                };
                if header.is_some() {
                    return Err(NodeError::DataAvailability(DaError::InvalidPayload(
                        "DA block payload contains multiple block headers".into(),
                    )));
                }
                header = Some((**block_header).clone());
            }
        } else if section.namespace == tx_namespace {
            for record in &section.records {
                let DaRecord::SignedTransaction(transaction) = record else {
                    return Err(NodeError::DataAvailability(DaError::InvalidPayload(
                        "DA transaction namespace contains a non-transaction record".into(),
                    )));
                };
                transactions.push(transaction.clone());
            }
        } else if section.namespace == receipt_namespace {
            for record in &section.records {
                let DaRecord::Receipt(receipt) = record else {
                    return Err(NodeError::DataAvailability(DaError::InvalidPayload(
                        "DA receipt namespace contains a non-receipt record".into(),
                    )));
                };
                receipts.push(receipt.clone());
            }
        }
    }

    let header = header.ok_or_else(|| {
        NodeError::DataAvailability(DaError::InvalidPayload(
            "DA block payload has no block header".into(),
        ))
    })?;
    if payload.chain_id != header.chain_id
        || payload.height != header.height
        || payload.previous_block_hash != header.previous_block_hash
    {
        return Err(NodeError::DataAvailability(DaError::InvalidPayload(
            "DA block payload metadata does not match block header".into(),
        )));
    }
    Ok(Block {
        header,
        transactions,
        receipts,
    })
}

fn validate_da_certificate_for_share_set(
    certificate: &DaAvailabilityCertificate,
    share_set: &DaShareSet,
) -> Result<(), NodeError> {
    certificate
        .validate()
        .map_err(NodeError::DataAvailability)?;
    let manifest_hash = share_set
        .manifest
        .manifest_hash()
        .map_err(NodeError::DataAvailability)?;
    if certificate.chain_id != share_set.manifest.chain_id
        || certificate.height != share_set.manifest.height
        || certificate.block_hash != share_set.manifest.block_hash
        || certificate.manifest_hash != manifest_hash
        || certificate.share_root != share_set.manifest.share_root
    {
        return Err(NodeError::DataAvailability(
            DaError::InvalidAvailabilityCertificate(
                "certificate does not match DA share set".into(),
            ),
        ));
    }
    Ok(())
}

fn first_missing_da_share_index(
    manifest: Option<&DaManifest>,
    shares: &BTreeMap<u32, DaShare>,
) -> Option<u32> {
    let manifest = manifest?;
    (0..manifest.encoded_share_count).find(|index| !shares.contains_key(index))
}

fn reconstructable_da_share_set(
    manifest: &DaManifest,
    shares: &BTreeMap<u32, DaShare>,
) -> Result<Option<DaShareSet>, NodeError> {
    if shares.len() < manifest.reconstruction_threshold as usize {
        return Ok(None);
    }
    let share_set = DaShareSet {
        manifest: manifest.clone(),
        shares: shares.values().cloned().collect(),
    };
    match share_set.verify() {
        Ok(()) => Ok(Some(share_set)),
        Err(DaError::InsufficientShares { .. }) => Ok(None),
        Err(error) => Err(NodeError::DataAvailability(error)),
    }
}

fn records_proof_latency(request: &RpcRequest) -> bool {
    matches!(
        request,
        RpcRequest::GetReceiptProof { .. }
            | RpcRequest::GetAspectModuleProof { .. }
            | RpcRequest::GetStorageProof { .. }
            | RpcRequest::GetStorageNonInclusionProof { .. }
            | RpcRequest::GetRegistryProof { .. }
            | RpcRequest::GetRegistryNonInclusionProof { .. }
            | RpcRequest::GetOutboxMessageProof { .. }
            | RpcRequest::GetEventProof { .. }
    )
}

fn operator_alerts_for_state(
    policy: &OperatorAlertPolicy,
    metrics: &OperatorMetricsReport,
    root_mismatch: bool,
    slashing_record_count: usize,
    latest_block_failure_count: usize,
    latest_block_receipt_count: usize,
) -> Vec<OperatorAlert> {
    let mut alerts = Vec::new();
    if metrics
        .peer_count
        .is_some_and(|peers| peers < policy.min_peer_count)
    {
        alerts.push(operator_alert(
            "operator.peer_isolation",
            OperatorAlertSeverity::Critical,
            "observed peer count is below policy minimum",
        ));
    }
    if metrics.consensus_height > 0
        && metrics
            .finality_lag
            .is_none_or(|lag| lag > policy.max_finality_lag)
    {
        alerts.push(operator_alert(
            "operator.stalled_consensus",
            OperatorAlertSeverity::Critical,
            "finality lag exceeds policy threshold",
        ));
    }
    if root_mismatch {
        alerts.push(operator_alert(
            "operator.root_mismatch",
            OperatorAlertSeverity::Critical,
            "persisted metadata roots do not match local roots",
        ));
    }
    if latest_block_receipt_count > 0 {
        let failure_ratio = (latest_block_failure_count * 1_000) / latest_block_receipt_count;
        if failure_ratio > usize::from(policy.max_latest_block_failure_ratio_per_mille) {
            alerts.push(operator_alert(
                "operator.excessive_reverts",
                OperatorAlertSeverity::Warning,
                "latest block failure ratio exceeds policy threshold",
            ));
        }
    }
    if slashing_record_count > 0 {
        alerts.push(operator_alert(
            "operator.slashing_evidence",
            OperatorAlertSeverity::Critical,
            "durable slashing evidence is present",
        ));
    }
    if metrics
        .storage_bytes
        .is_some_and(|bytes| bytes > policy.max_storage_bytes)
    {
        alerts.push(operator_alert(
            "operator.disk_pressure",
            OperatorAlertSeverity::Warning,
            "storage byte count exceeds policy threshold",
        ));
    }
    if metrics.rpc_error_count > policy.max_rpc_error_count {
        alerts.push(operator_alert(
            "operator.rpc_overload",
            OperatorAlertSeverity::Warning,
            "RPC error count exceeds policy threshold",
        ));
    }
    if metrics.da_missing_share_count > 0 {
        alerts.push(operator_alert(
            "operator.da_missing_shares",
            OperatorAlertSeverity::Critical,
            "data availability store has missing shares",
        ));
    }
    if metrics.da_repair_record_count > 0 {
        alerts.push(operator_alert(
            "operator.da_repair_pending",
            OperatorAlertSeverity::Warning,
            "data availability repair records are pending",
        ));
    }
    if metrics.da_custody_failure_count > 0 {
        alerts.push(operator_alert(
            "operator.da_custody_failure",
            OperatorAlertSeverity::Critical,
            "data availability custody failure slashing evidence is present",
        ));
    }
    if metrics
        .da_oldest_pending_repair_age_blocks
        .is_some_and(|age| age > policy.max_da_repair_lag_blocks)
    {
        alerts.push(operator_alert(
            "operator.da_repair_lag",
            OperatorAlertSeverity::Warning,
            "data availability repair lag exceeds policy threshold",
        ));
    }
    if metrics.da_challenge_evidence_count > 0 {
        alerts.push(operator_alert(
            "operator.da_challenge_failure",
            OperatorAlertSeverity::Critical,
            "data availability challenge failure evidence is present",
        ));
    }
    if metrics.mempool_size > policy.max_mempool_size {
        alerts.push(operator_alert(
            "operator.mempool_saturation",
            OperatorAlertSeverity::Warning,
            "mempool size exceeds policy threshold",
        ));
    }
    alerts
}

fn operator_alert(
    code: impl Into<String>,
    severity: OperatorAlertSeverity,
    message: impl Into<String>,
) -> OperatorAlert {
    OperatorAlert {
        code: code.into(),
        severity,
        message: message.into(),
    }
}

fn elapsed_micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros())
        .unwrap_or(u64::MAX)
        .max(1)
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
        NodeError::ConsensusSigningConflict { .. } => "node.consensus_signing_conflict",
        NodeError::ConsensusMessageSignerMismatch { .. } => {
            "node.consensus_message_signer_mismatch"
        }
        NodeError::Signature(_) => "node.validator_signature_error",
        NodeError::ValidatorKeyNotFound(_) => "node.validator_key_not_found",
        NodeError::Consensus(ConsensusError::QuorumNotReached { .. }) => {
            "node.validator_quorum_not_reached"
        }
        NodeError::DataAvailability(_) => "node.data_availability_error",
        NodeError::Mempool(MempoolError::ChainMismatch) => "mempool.chain_mismatch",
        NodeError::Mempool(MempoolError::InvalidSignature) => "mempool.invalid_signature",
        NodeError::Mempool(MempoolError::UnauthorizedSigner { .. }) => {
            "mempool.unauthorized_signer"
        }
        NodeError::Mempool(MempoolError::TransactionExpired { .. }) => {
            "mempool.transaction_expired"
        }
        NodeError::Mempool(MempoolError::DuplicateTransaction) => "mempool.duplicate_transaction",
        NodeError::Mempool(MempoolError::NonceAlreadyUsed) => "mempool.nonce_already_used",
        NodeError::Mempool(MempoolError::InsufficientBudget) => "mempool.insufficient_budget",
        NodeError::Mempool(MempoolError::TransactionTooLarge { .. }) => {
            "mempool.transaction_too_large"
        }
        NodeError::Mempool(MempoolError::PoolFull { .. }) => "mempool.pool_full",
        NodeError::Mempool(MempoolError::SenderPendingLimitExceeded { .. }) => {
            "mempool.sender_pending_limit_exceeded"
        }
        _ => "node.error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{
        Amount, Argument, Method, PolicyEffect, ScheduledPolicyUpdate, ScheduledUpgrade, TxStatus,
        DA_SLASHING_POLICY_SCOPE, DEFAULT_BLOCK_RESOURCE_LIMIT, DEFAULT_MEMPOOL_MAX_PENDING,
        DEFAULT_MEMPOOL_MAX_PENDING_PER_SENDER, DEFAULT_MEMPOOL_MAX_TRANSACTION_BYTES,
    };
    use detta_network::{InMemoryTransport, TcpProtocolStream};
    use detta_protocol::{
        ProtocolMessage, SignatureError, SnapshotChunkRequest, SnapshotChunkSet,
        ValidatorSetMetadataUpdate, ValidatorSigningKey,
    };
    use detta_rpc::{JsonRpcServer, SubscriptionNotification, SubscriptionTopic};
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    const ASPECT_SOURCE: &str =
        include_str!("../../../models/aspects/stdlib/minimal-transfer-token.metta");

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

    fn state_rejecting_invalid_da_slashing_evidence() -> DeTTaState {
        let mut state = seeded_state();
        state
            .deploy_governance_with_timelock("GovDA", DA_SLASHING_POLICY_SCOPE, "Admin", 1)
            .unwrap();
        let schedule = state.apply_transaction(tx_to(
            "GovDA",
            "tx-da-policy-schedule",
            "Admin",
            1,
            Method::ScheduleDaSlashingPolicyUpdate,
            vec![
                Argument::Text("da-policy-1".into()),
                Argument::Text("true".into()),
                Argument::Text("false".into()),
                Argument::Amount(0),
                Argument::Amount(0),
            ],
        ));
        assert_eq!(schedule.status, TxStatus::Committed);
        let (block, state) = state.build_block(
            1,
            vec![tx_to(
                "GovDA",
                "tx-da-policy-execute",
                "Admin",
                2,
                Method::ExecuteDaSlashingPolicyUpdate,
                vec![Argument::Text("da-policy-1".into())],
            )],
            1_000,
            "validator-1",
            "cert-1",
        );
        assert_eq!(block.receipts[0].status, TxStatus::Committed);
        assert!(!state.da_slashing_policy().slash_invalid_response);
        state
    }

    fn seeded_defi_state() -> DeTTaState {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_token(
                "TokenA",
                "USDC",
                vec![("Alice".into(), 200), ("Bob".into(), 50)],
            )
            .unwrap();
        state
            .deploy_token("TokenETH", "ETH", vec![("Alice".into(), 100)])
            .unwrap();
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
            snapshot_import_audit_record_count: 0,
            snapshot_import_audit_max_records: DEFAULT_MAX_SNAPSHOT_IMPORT_AUDIT_RECORDS,
            snapshot_import_audit_max_page_size: DEFAULT_MAX_SNAPSHOT_IMPORT_AUDIT_PAGE_SIZE,
            snapshot_import_audit_config_root: None,
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
            RpcResponse::Ok(RpcResult::SnapshotMetadataRootStatus(status)) => *status,
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
            valid_until_height: None,
            target: target.into(),
            method,
            args,
            signature_ok: true,
            budget: 1_000_000,
        }
    }

    fn text(value: impl Into<String>) -> Argument {
        Argument::Text(value.into())
    }

    fn asset(value: impl Into<String>) -> Argument {
        Argument::Asset(value.into())
    }

    fn amount(value: Amount) -> Argument {
        Argument::Amount(value)
    }

    fn state_with_factory() -> DeTTaState {
        let mut state = seeded_state();
        state.deploy_factory("Factory").unwrap();
        state
    }

    fn defi_da_state() -> DeTTaState {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_token(
                "TokenUSDC",
                "USDC",
                vec![
                    ("Alice".into(), 1_000),
                    ("VaultA".into(), 1_000),
                    ("Liquidator".into(), 1_000),
                ],
            )
            .unwrap();
        state
            .deploy_token(
                "TokenATOM",
                "ATOM",
                vec![("Alice".into(), 1_000), ("Liquidator".into(), 1_000)],
            )
            .unwrap();
        state.deploy_amm_pool("PoolAB", "USDC", "ATOM").unwrap();
        state
            .deploy_oracle("OracleA", "ATOM", "OracleBot", 10)
            .unwrap();
        state
            .deploy_lending_vault("VaultA", "ATOM", "USDC", "OracleA", 5_000, 10)
            .unwrap();
        state.deploy_staking("StakeA", "ATOM").unwrap();
        state
    }

    fn large_aspect_submission(index: u64) -> Transaction {
        let root_suffix = format!("{}-{index}", "x".repeat(512));
        tx_to(
            "Factory",
            &format!("tx-aspect-load-{index}"),
            "Issuer",
            index,
            Method::SubmitAspectModule,
            vec![
                text(format!("LoadAspect{index}")),
                text("NormalizedBalanceFirst.v1"),
                text(format!("source-root-{root_suffix}")),
                text(format!("ir-root-{root_suffix}")),
                text(format!("abi-root-{root_suffix}")),
                text(format!("policy-root-{root_suffix}")),
                text(format!("storage-schema-root-{root_suffix}")),
                text(format!("registry-schema-root-{root_suffix}")),
                text(format!("invariant-root-{root_suffix}")),
            ],
        )
    }

    #[test]
    fn persistent_validator_produces_block_and_survives_restart() {
        let dir = temp_dir("restart");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        node.submit_transaction(transfer_tx()).unwrap();

        let block = node.produce_block(1, 1_000).unwrap();
        let expected_transaction = block.transactions[0].clone();
        let expected_receipt = block.receipts[0].clone();
        let mut restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        let expected_receipt_proof = block.receipt_proof(0).unwrap();
        let expected_event_proof = restarted.rpc().node().state().event_proof(0).unwrap();

        assert_eq!(restarted.validator_id(), "validator-1");
        assert_eq!(
            restarted.load_block(1).unwrap().block_hash(),
            block.block_hash()
        );
        assert_eq!(
            restarted.rpc().call_balance_view("TokenA", "Bob", "USDC"),
            60
        );
        assert_eq!(
            restarted.handle_rpc_request(RpcRequest::GetBlock { height: 1 }),
            RpcResponse::Ok(RpcResult::Block(Box::new(block.clone())))
        );
        assert_eq!(
            restarted.handle_rpc_request(RpcRequest::GetTransaction {
                tx_hash: "tx1".into(),
            }),
            RpcResponse::Ok(RpcResult::Transaction(Box::new(expected_transaction)))
        );
        assert_eq!(
            restarted.handle_rpc_request(RpcRequest::GetReceipt {
                tx_hash: "tx1".into(),
            }),
            RpcResponse::Ok(RpcResult::Receipt(Box::new(expected_receipt)))
        );
        assert_eq!(
            restarted.handle_rpc_request(RpcRequest::GetReceiptProof {
                height: 1,
                index: 0,
            }),
            RpcResponse::Ok(RpcResult::ReceiptProof(Box::new(expected_receipt_proof)))
        );
        assert_eq!(
            restarted.handle_rpc_request(RpcRequest::GetEventProof { index: 0 }),
            RpcResponse::Ok(RpcResult::EventProof(Box::new(expected_event_proof)))
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_validator_produces_da_committed_block_and_survives_restart() {
        let dir = temp_dir("da-restart");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        node.submit_transaction(transfer_tx()).unwrap();

        let block = node
            .produce_block_with_data_availability(1, 1_000, 64)
            .unwrap();
        let commitment = block.header.data_availability.clone().unwrap();
        let share_set = node.load_da_share_set(&commitment.manifest_hash).unwrap();
        let certificate = DaAvailabilityCertificate::from_manifest(
            &share_set.manifest,
            vec!["validator-1".into(), "validator-2".into()],
        )
        .unwrap();
        let certificate_hash = node.persist_da_certificate(&certificate).unwrap();
        let payload = share_set.reconstruct_payload().unwrap();
        let namespaces = payload
            .namespaces
            .iter()
            .map(|section| section.namespace.0.as_str())
            .collect::<Vec<_>>();

        assert_eq!(share_set.manifest.payload_hash, commitment.payload_root);
        assert_eq!(share_set.manifest.share_root, commitment.share_root);
        assert_eq!(
            share_set.manifest.erasure_scheme,
            detta_da::ErasureScheme::ReedSolomonV1
        );
        assert_eq!(
            share_set.manifest.encoded_share_count,
            share_set.manifest.original_share_count * 2
        );
        assert_eq!(
            share_set.manifest.reconstruction_threshold,
            share_set.manifest.original_share_count
        );
        assert_eq!(share_set.manifest.height, 1);
        assert_eq!(share_set.manifest.chain_id, "detta-local");
        assert_eq!(share_set.manifest.block_hash, {
            let mut execution_block = block.clone();
            execution_block.header.data_availability = None;
            execution_block.block_hash()
        });
        assert_eq!(
            payload.previous_block_hash,
            block.header.previous_block_hash
        );
        assert_eq!(namespaces, vec!["detta.block", "detta.receipt", "detta.tx"]);
        assert!(matches!(
            &payload.namespaces[0].records[0],
            DaRecord::BlockHeader(_)
        ));
        assert_eq!(node.pending_len(), 0);
        assert_eq!(
            node.load_block(1).unwrap().header.data_availability,
            Some(commitment.clone())
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetDaManifest {
                manifest_hash: commitment.manifest_hash.clone(),
            }),
            RpcResponse::Ok(RpcResult::DaManifest(Box::new(share_set.manifest.clone())))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetDaShare {
                manifest_hash: commitment.manifest_hash.clone(),
                index: 0,
            }),
            RpcResponse::Ok(RpcResult::DaShare(Box::new(share_set.shares[0].clone())))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetDaCertificate {
                certificate_hash: certificate_hash.clone(),
            }),
            RpcResponse::Ok(RpcResult::DaAvailabilityCertificate(Box::new(
                certificate.clone()
            )))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetDaPayload {
                manifest_hash: commitment.manifest_hash.clone(),
            }),
            RpcResponse::Ok(RpcResult::DaPayload(Box::new(payload.clone())))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetDaNamespace {
                manifest_hash: commitment.manifest_hash.clone(),
                namespace: "detta.tx".into(),
            }),
            RpcResponse::Ok(RpcResult::DaNamespace(Box::new(
                payload.namespaces[2].clone()
            )))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetDaNamespace {
                manifest_hash: commitment.manifest_hash.clone(),
                namespace: "detta.oracle".into(),
            }),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.da_namespace_not_found".into(),
                message: "DA namespace was not found".into(),
            })
        );
        let sample_response = node.handle_rpc_request(RpcRequest::GetDaSampleProofs {
            manifest_hash: commitment.manifest_hash.clone(),
            client_randomness: "node-test-randomness".into(),
            sample_count: 2,
            namespaces: vec!["detta.tx".into(), "detta.oracle".into()],
        });
        let RpcResponse::Ok(RpcResult::DaSampleProofs(sample_bundle)) = sample_response else {
            panic!("expected DA sample proof bundle, got {sample_response:?}");
        };
        assert!(sample_bundle.verification.valid);
        assert_eq!(sample_bundle.schedule.requested_sample_count, 2);
        assert_eq!(sample_bundle.sample_proofs.len(), 2);
        assert_eq!(sample_bundle.namespace_proofs.len(), 2);
        assert!(sample_bundle.namespace_proofs[0].range.is_some());
        assert!(sample_bundle.namespace_proofs[1].range.is_none());
        assert_eq!(
            verify_light_client_samples(
                &share_set.manifest,
                b"node-test-randomness",
                2,
                &sample_bundle.sample_proofs,
                &sample_bundle.namespace_proofs,
            )
            .unwrap(),
            sample_bundle.verification
        );
        let stats_response = node.handle_rpc_request(RpcRequest::GetDaStorageStats);
        let RpcResponse::Ok(RpcResult::DaStorageStats(stats)) = stats_response else {
            panic!("expected DA storage stats, got {stats_response:?}");
        };
        assert_eq!(stats.manifest_count, 1);
        assert_eq!(
            stats.expected_share_count,
            share_set.manifest.encoded_share_count as u64
        );
        assert_eq!(stats.stored_share_count, share_set.shares.len() as u64);
        assert_eq!(stats.missing_share_count, 0);
        assert_eq!(stats.certificate_count, 1);
        assert_eq!(
            stats.retention_policy,
            Some(DaRetentionPolicyConfig::production_default())
        );
        assert!(stats.retention_policy_root.is_some());
        assert!(stats.retention_policy_bytes > 0);
        let retention_audit_response = node.handle_rpc_request(RpcRequest::GetDaRetentionAudit);
        let RpcResponse::Ok(RpcResult::DaRetentionAudit(retention_audit)) =
            retention_audit_response
        else {
            panic!("expected DA retention audit, got {retention_audit_response:?}");
        };
        assert_eq!(retention_audit.current_height, 1);
        assert_eq!(retention_audit.manifest_count, 1);
        assert_eq!(retention_audit.active_manifest_count, 1);
        assert_eq!(retention_audit.expired_manifest_count, 0);
        assert_eq!(retention_audit.missing_policy_class_count, 0);
        assert_eq!(retention_audit.unsatisfied_manifest_count, 0);
        assert_eq!(
            retention_audit.policy,
            Some(DaRetentionPolicyConfig::production_default())
        );
        assert_eq!(retention_audit.entries.len(), 1);
        let retention_entry = &retention_audit.entries[0];
        assert_eq!(retention_entry.manifest_hash, commitment.manifest_hash);
        assert_eq!(retention_entry.class, detta_storage::DaRetentionClass::Hot);
        assert!(retention_entry.payload_present);
        assert_eq!(
            retention_entry.stored_share_count,
            retention_entry.expected_share_count
        );
        assert!(retention_entry.retention_satisfied);
        let prune_plan_response = node.handle_rpc_request(RpcRequest::GetDaRetentionPrunePlan);
        let RpcResponse::Ok(RpcResult::DaRetentionPrunePlan(prune_plan)) = prune_plan_response
        else {
            panic!("expected DA retention prune plan, got {prune_plan_response:?}");
        };
        assert_eq!(prune_plan.current_height, 1);
        assert_eq!(prune_plan.manifest_count, 1);
        assert_eq!(prune_plan.candidate_manifest_count, 0);
        assert_eq!(prune_plan.prunable_payload_count, 0);
        assert_eq!(prune_plan.prunable_share_count, 0);
        assert_eq!(
            prune_plan.policy,
            Some(DaRetentionPolicyConfig::production_default())
        );
        assert!(prune_plan.entries.is_empty());
        let height_index_response =
            node.handle_rpc_request(RpcRequest::GetDaManifestIndexByHeight { height: 1 });
        let RpcResponse::Ok(RpcResult::DaManifestIndex(height_index)) = height_index_response
        else {
            panic!("expected DA height index, got {height_index_response:?}");
        };
        assert_eq!(height_index.len(), 1);
        assert_eq!(height_index[0].manifest_hash, commitment.manifest_hash);
        let block_hash_index_response =
            node.handle_rpc_request(RpcRequest::GetDaManifestIndexByBlockHash {
                block_hash: share_set.manifest.block_hash.clone(),
            });
        let RpcResponse::Ok(RpcResult::DaManifestIndex(block_hash_index)) =
            block_hash_index_response
        else {
            panic!("expected DA block-hash index, got {block_hash_index_response:?}");
        };
        assert_eq!(block_hash_index.len(), 1);
        assert_eq!(block_hash_index[0].manifest_hash, commitment.manifest_hash);
        let namespace_index_response =
            node.handle_rpc_request(RpcRequest::GetDaManifestIndexByNamespace {
                namespace: "detta.tx".into(),
            });
        let RpcResponse::Ok(RpcResult::DaManifestIndex(namespace_index)) = namespace_index_response
        else {
            panic!("expected DA namespace index, got {namespace_index_response:?}");
        };
        assert_eq!(namespace_index.len(), 1);
        assert_eq!(namespace_index[0].manifest_hash, commitment.manifest_hash);
        let retention_index_response =
            node.handle_rpc_request(RpcRequest::GetDaManifestIndexByRetentionClass {
                class: detta_storage::DaRetentionClass::Hot,
            });
        let RpcResponse::Ok(RpcResult::DaManifestIndex(retention_index)) = retention_index_response
        else {
            panic!("expected DA retention-class index, got {retention_index_response:?}");
        };
        assert_eq!(retention_index.len(), 1);
        assert_eq!(retention_index[0].manifest_hash, commitment.manifest_hash);
        let certificate_manifest_index_response =
            node.handle_rpc_request(RpcRequest::GetDaCertificateIndexByManifest {
                manifest_hash: commitment.manifest_hash.clone(),
            });
        let RpcResponse::Ok(RpcResult::DaCertificateIndex(certificate_manifest_index)) =
            certificate_manifest_index_response
        else {
            panic!(
                "expected DA certificate manifest index, got {certificate_manifest_index_response:?}"
            );
        };
        assert_eq!(certificate_manifest_index.len(), 1);
        assert_eq!(
            certificate_manifest_index[0].certificate_hash,
            certificate_hash
        );
        let certificate_height_index_response =
            node.handle_rpc_request(RpcRequest::GetDaCertificateIndexByHeight { height: 1 });
        let RpcResponse::Ok(RpcResult::DaCertificateIndex(certificate_height_index)) =
            certificate_height_index_response
        else {
            panic!(
                "expected DA certificate height index, got {certificate_height_index_response:?}"
            );
        };
        assert_eq!(certificate_height_index.len(), 1);
        assert_eq!(
            certificate_height_index[0].certificate_hash,
            certificate_hash
        );
        let certificate_block_hash_index_response =
            node.handle_rpc_request(RpcRequest::GetDaCertificateIndexByBlockHash {
                block_hash: share_set.manifest.block_hash.clone(),
            });
        let RpcResponse::Ok(RpcResult::DaCertificateIndex(certificate_block_hash_index)) =
            certificate_block_hash_index_response
        else {
            panic!(
                "expected DA certificate block-hash index, got {certificate_block_hash_index_response:?}"
            );
        };
        assert_eq!(certificate_block_hash_index.len(), 1);
        assert_eq!(
            certificate_block_hash_index[0].certificate_hash,
            certificate_hash
        );
        let status_response = node.handle_rpc_request(RpcRequest::GetDaStatus {
            manifest_hash: commitment.manifest_hash.clone(),
        });
        let RpcResponse::Ok(RpcResult::DaStatus(status)) = status_response else {
            panic!("expected DA status, got {status_response:?}");
        };
        assert!(status.manifest_available);
        assert_eq!(
            status.expected_share_count,
            share_set.manifest.encoded_share_count
        );
        assert_eq!(status.stored_share_count, share_set.shares.len() as u32);
        assert!(status.missing_share_indices.is_empty());
        assert!(status.payload_reconstructable);
        assert_eq!(status.certificate_hash, Some(certificate_hash.clone()));
        assert!(status.certificate_available);
        let repair_response = node.handle_rpc_request(RpcRequest::GetDaRepairStatus {
            manifest_hash: commitment.manifest_hash.clone(),
        });
        let RpcResponse::Ok(RpcResult::DaRepairStatus(repair_status)) = repair_response else {
            panic!("expected DA repair status, got {repair_response:?}");
        };
        assert!(!repair_status.repair_needed);
        assert_eq!(repair_status.pending_repair_count, 0);
        assert!(repair_status.missing_share_indices.is_empty());

        let coding_response = node.handle_rpc_request(RpcRequest::GetDaCodingFraudProof {
            manifest_hash: commitment.manifest_hash.clone(),
        });
        let RpcResponse::Ok(RpcResult::DaCodingFraud(coding)) = coding_response else {
            panic!("expected DA coding fraud report, got {coding_response:?}");
        };
        assert!(coding.manifest_available);
        assert!(!coding.fault_detected);
        assert!(coding.proof.is_none());
        assert_eq!(
            coding.available_data_share_count,
            share_set.manifest.original_share_count
        );
        assert_eq!(
            coding.detail.as_deref(),
            Some("manifest commits a valid encoding of its payload")
        );

        let restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        assert_eq!(
            restarted.load_block(1).unwrap().header.data_availability,
            Some(commitment.clone())
        );
        assert_eq!(
            restarted
                .load_da_share_set(&commitment.manifest_hash)
                .unwrap()
                .reconstruct_payload()
                .unwrap(),
            payload
        );
        assert_eq!(
            restarted.load_da_certificate(&certificate_hash).unwrap(),
            certificate
        );
        let restarted_status = restarted.da_status(&commitment.manifest_hash).unwrap();
        assert_eq!(
            restarted_status.certificate_hash,
            Some(certificate_hash.clone())
        );
        assert!(restarted_status.certificate_available);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_json_rpc_serves_application_da_storage() {
        let dir = temp_dir("application-da-rpc");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        let profile = detta_da::DaApplicationProfile::social_demo_v1();
        let profile_id = profile.profile_id().unwrap();
        let coordinate = detta_da::DaApplicationCoordinate {
            application_id: detta_da::DaApplicationId::new("social.demo").unwrap(),
            stream_id: "main".into(),
            sequence: 1,
            epoch: Some(1),
            parent_hash: None,
            subject_hash: None,
        };
        let post = detta_da::DaRecordEnvelope::new(
            "social.post",
            1,
            "application/json",
            detta_da::DaRecordEncoding::CanonicalJson,
            br#"{"author":"alice","post_id":"post-1","text":"rpc"}"#.to_vec(),
            Some("alice".into()),
            Some("signature-1".into()),
        )
        .unwrap();
        let payload = detta_da::ApplicationDaPayload::new(
            &profile,
            coordinate.clone(),
            detta_da::DaPayloadKind::Batch,
            None,
            vec![
                detta_da::DaApplicationRoot::new("social.event.log.root", "11".repeat(32)).unwrap(),
            ],
            vec![detta_da::ApplicationDaNamespaceSection::new(
                DaNamespace::new("social.feed").unwrap(),
                vec![post],
            )
            .unwrap()],
        )
        .unwrap();
        let share_set =
            detta_da::ApplicationDaShareSet::from_payload_reed_solomon(&payload, &profile, 4, 2)
                .unwrap();
        let manifest_hash = node
            .storage
            .commit_application_da_share_set(&share_set, &profile)
            .unwrap();
        let certificate = detta_da::ApplicationDaAvailabilityCertificate::from_manifest(
            &share_set.manifest,
            &profile,
            vec!["validator-1".into(), "validator-2".into()],
        )
        .unwrap();
        let certificate_hash = node
            .storage
            .commit_application_da_certificate(&certificate)
            .unwrap();
        let registration = node
            .storage
            .load_application_da_profile_registration(&profile_id)
            .unwrap();
        let application_root = share_set.manifest.application_root.clone().unwrap();

        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetApplicationDaProfile {
                profile_id: profile_id.clone(),
            }),
            RpcResponse::Ok(RpcResult::ApplicationDaProfile(Box::new(
                registration.clone()
            )))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetApplicationDaManifest {
                manifest_hash: manifest_hash.clone(),
            }),
            RpcResponse::Ok(RpcResult::ApplicationDaManifest(Box::new(
                share_set.manifest.clone()
            )))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetApplicationDaPayload {
                manifest_hash: manifest_hash.clone(),
            }),
            RpcResponse::Ok(RpcResult::ApplicationDaPayload(Box::new(
                payload.canonicalized()
            )))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetApplicationDaReconstructedPayload {
                manifest_hash: manifest_hash.clone(),
            }),
            RpcResponse::Ok(RpcResult::ApplicationDaPayload(Box::new(
                payload.canonicalized()
            )))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetApplicationDaNamespace {
                manifest_hash: manifest_hash.clone(),
                namespace: "social.feed".into(),
            }),
            RpcResponse::Ok(RpcResult::ApplicationDaNamespace(Box::new(
                payload.canonicalized().namespaces[0].clone()
            )))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetApplicationDaShare {
                manifest_hash: manifest_hash.clone(),
                index: 0,
            }),
            RpcResponse::Ok(RpcResult::ApplicationDaShare(Box::new(
                share_set.shares[0].clone()
            )))
        );
        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetApplicationDaCertificate {
                certificate_hash: certificate_hash.clone(),
            }),
            RpcResponse::Ok(RpcResult::ApplicationDaAvailabilityCertificate(Box::new(
                certificate.clone()
            )))
        );

        let sample_response = node.handle_rpc_request(RpcRequest::GetApplicationDaSampleProofs {
            manifest_hash: manifest_hash.clone(),
            client_randomness: "application-da-rpc-randomness".into(),
            sample_count: 2,
            namespaces: vec!["social.feed".into()],
        });
        let RpcResponse::Ok(RpcResult::ApplicationDaSampleProofs(sample_bundle)) = sample_response
        else {
            panic!("expected application DA sample proof bundle, got {sample_response:?}");
        };
        assert_eq!(sample_bundle.schedule.manifest_hash, manifest_hash);
        assert_eq!(sample_bundle.sample_proofs.len(), 2);
        assert_eq!(sample_bundle.namespace_proofs.len(), 1);
        assert!(sample_bundle.verification.valid);

        let status_response = node.handle_rpc_request(RpcRequest::GetApplicationDaStatus {
            manifest_hash: manifest_hash.clone(),
        });
        let RpcResponse::Ok(RpcResult::ApplicationDaStatus(status)) = status_response else {
            panic!("expected application DA status, got {status_response:?}");
        };
        assert!(status.manifest_available);
        assert_eq!(
            status.application_id,
            Some(coordinate.application_id.clone())
        );
        assert_eq!(status.profile_id, Some(registration.profile_id.clone()));
        assert_eq!(status.coordinate, Some(coordinate.clone()));
        assert_eq!(status.certificate_hash, Some(certificate_hash.clone()));
        assert!(status.certificate_available);
        assert_eq!(
            status.expected_share_count,
            share_set.manifest.encoded_share_count
        );
        assert_eq!(
            status.stored_share_count,
            share_set.manifest.encoded_share_count
        );
        assert!(status.missing_share_indices.is_empty());
        assert!(status.payload_reconstructable);
        assert_eq!(status.payload_bytes, Some(share_set.manifest.payload_bytes));
        assert_eq!(
            status.namespace_count,
            Some(share_set.manifest.namespace_ranges.len())
        );
        assert!(status.reconstruction_error.is_none());

        let profile_index_response =
            node.handle_rpc_request(RpcRequest::GetApplicationDaProfileIndexByApplicationId {
                application_id: "social.demo".into(),
            });
        let RpcResponse::Ok(RpcResult::ApplicationDaProfileIndex(profile_index)) =
            profile_index_response
        else {
            panic!("expected application DA profile index, got {profile_index_response:?}");
        };
        assert_eq!(profile_index.len(), 1);
        assert_eq!(profile_index[0].profile_id, profile_id);

        let version_index_response = node.handle_rpc_request(
            RpcRequest::GetApplicationDaProfileIndexByApplicationVersion {
                application_id: "social.demo".into(),
                profile_version: 1,
            },
        );
        let RpcResponse::Ok(RpcResult::ApplicationDaProfileIndex(version_index)) =
            version_index_response
        else {
            panic!("expected application DA profile version index, got {version_index_response:?}");
        };
        assert_eq!(version_index.len(), 1);

        let manifest_index_response =
            node.handle_rpc_request(RpcRequest::GetApplicationDaManifestIndexByCoordinate {
                coordinate: coordinate.clone(),
            });
        let RpcResponse::Ok(RpcResult::ApplicationDaManifestIndex(manifest_index)) =
            manifest_index_response
        else {
            panic!("expected application DA manifest coordinate index, got {manifest_index_response:?}");
        };
        assert_eq!(manifest_index.len(), 1);
        assert_eq!(manifest_index[0].manifest_hash, manifest_hash);

        for request in [
            RpcRequest::GetApplicationDaManifestIndexByApplicationId {
                application_id: "social.demo".into(),
            },
            RpcRequest::GetApplicationDaManifestIndexByNamespace {
                namespace: "social.feed".into(),
            },
            RpcRequest::GetApplicationDaManifestIndexByRetentionClass {
                class: detta_da::DaApplicationRetentionClass::Warm,
            },
            RpcRequest::GetApplicationDaManifestIndexByApplicationRoot {
                application_root: application_root.clone(),
            },
        ] {
            let response = node.handle_rpc_request(request);
            let RpcResponse::Ok(RpcResult::ApplicationDaManifestIndex(index)) = response else {
                panic!("expected application DA manifest index, got {response:?}");
            };
            assert_eq!(index.len(), 1);
            assert_eq!(index[0].manifest_hash, manifest_hash);
        }

        for request in [
            RpcRequest::GetApplicationDaCertificateIndexByManifest {
                manifest_hash: manifest_hash.clone(),
            },
            RpcRequest::GetApplicationDaCertificateIndexByApplicationId {
                application_id: "social.demo".into(),
            },
            RpcRequest::GetApplicationDaCertificateIndexByProfileId {
                profile_id: registration.profile_id.clone(),
            },
            RpcRequest::GetApplicationDaCertificateIndexByCoordinate { coordinate },
        ] {
            let response = node.handle_rpc_request(request);
            let RpcResponse::Ok(RpcResult::ApplicationDaCertificateIndex(index)) = response else {
                panic!("expected application DA certificate index, got {response:?}");
            };
            assert_eq!(index.len(), 1);
            assert_eq!(index[0].certificate_hash, certificate_hash);
        }

        assert_eq!(
            node.handle_rpc_request(RpcRequest::GetApplicationDaProfile {
                profile_id: "22".repeat(32),
            }),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.application_da_profile_not_found".into(),
                message: "application DA profile was not found".into(),
            })
        );

        let mut restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        assert_eq!(
            restarted
                .handle_rpc_request(RpcRequest::GetApplicationDaCertificate { certificate_hash }),
            RpcResponse::Ok(RpcResult::ApplicationDaAvailabilityCertificate(Box::new(
                certificate
            )))
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn da_payload_includes_specialized_defi_evidence_namespaces() {
        fn records<'a>(payload: &'a DaPayload, namespace: &str) -> &'a [DaRecord] {
            payload
                .namespaces
                .iter()
                .find(|section| section.namespace.0 == namespace)
                .unwrap_or_else(|| panic!("missing DA namespace {namespace}"))
                .records
                .as_slice()
        }

        let mut state = seeded_state();
        state.deploy_factory("Factory").unwrap();
        state
            .deploy_oracle("OracleA", "ATOM", "OracleBot", 10)
            .unwrap();
        state.deploy_bridge("BridgeA", "ShardA").unwrap();
        state
            .deploy_governance_with_timelock("GovA", "TokenA", "Admin", 1)
            .unwrap();

        let (block, _) = state.build_block(
            1,
            vec![
                tx_to(
                    "Factory",
                    "tx-da-aspect",
                    "Issuer",
                    1,
                    Method::SubmitAspectModule,
                    vec![
                        Argument::Text("AspectDA".into()),
                        Argument::Text("NormalizedBalanceFirst.v1".into()),
                        Argument::Text("source-root-da".into()),
                        Argument::Text("ir-root-da".into()),
                        Argument::Text("abi-root-da".into()),
                        Argument::Text("policy-root-da".into()),
                        Argument::Text("storage-schema-root-da".into()),
                        Argument::Text("registry-schema-root-da".into()),
                        Argument::Text("invariant-root-da".into()),
                    ],
                ),
                tx_to(
                    "BridgeA",
                    "tx-da-bridge",
                    "Alice",
                    1,
                    Method::QueueBridgeMessage,
                    vec![
                        Argument::Text("ShardB".into()),
                        Argument::Text("BridgeB".into()),
                        Argument::Text("bridge-msg-da".into()),
                        Argument::Principal("Bob".into()),
                        Argument::Asset("USDC".into()),
                        Argument::Amount(1),
                    ],
                ),
                tx_to(
                    "GovA",
                    "tx-da-governance",
                    "Admin",
                    1,
                    Method::ScheduleUpgrade,
                    vec![
                        Argument::Text("upgrade-da".into()),
                        Argument::Text("code-root-da".into()),
                    ],
                ),
                tx_to(
                    "OracleA",
                    "tx-da-oracle",
                    "OracleBot",
                    1,
                    Method::SubmitPrice,
                    vec![
                        Argument::Asset("ATOM".into()),
                        Argument::Amount(12),
                        Argument::Amount(1),
                    ],
                ),
            ],
            1_000,
            "validator-1",
            "cert-1",
        );
        assert!(block
            .receipts
            .iter()
            .all(|receipt| receipt.status == TxStatus::Committed));

        let payload = da_payload_for_block(&block).unwrap();
        let namespaces = payload
            .namespaces
            .iter()
            .map(|section| section.namespace.0.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            namespaces,
            vec![
                "detta.aspect",
                "detta.block",
                "detta.bridge",
                "detta.governance",
                "detta.oracle",
                "detta.receipt",
                "detta.tx",
            ]
        );

        match &records(&payload, "detta.aspect")[0] {
            DaRecord::AspectArtifact(module) => {
                assert_eq!(module.module_id, "AspectDA");
                assert_eq!(module.source_root, "source-root-da");
            }
            record => panic!("expected aspect artifact DA record, got {record:?}"),
        }
        match &records(&payload, "detta.bridge")[0] {
            DaRecord::BridgeProof { message_id, proof } => {
                assert_eq!(message_id, "bridge-msg-da");
                assert!(proof.contains("QueueBridgeMessage"));
            }
            record => panic!("expected bridge proof DA record, got {record:?}"),
        }
        match &records(&payload, "detta.governance")[0] {
            DaRecord::GovernancePayload {
                proposal_id,
                payload,
            } => {
                assert_eq!(proposal_id, "upgrade-da");
                assert!(payload.contains("ScheduleUpgrade"));
            }
            record => panic!("expected governance DA record, got {record:?}"),
        }
        match &records(&payload, "detta.oracle")[0] {
            DaRecord::OracleEvidence { asset, evidence } => {
                assert_eq!(asset, "ATOM");
                assert!(evidence.contains("SubmitPrice"));
            }
            record => panic!("expected oracle evidence DA record, got {record:?}"),
        }
    }

    #[test]
    fn build_proposal_does_not_commit_and_verifies_without_mutation() {
        let dir = temp_dir("bft-build-proposal");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        node.submit_transaction(transfer_tx()).unwrap();

        let height_before = node.current_height();
        let pending_before = node.pending_len();
        let root_before = node.rpc().get_state_root();
        assert!(pending_before > 0);

        // Building a proposal commits nothing: height, mempool, and state are
        // unchanged.
        let proposal = node.build_proposal(height_before + 1, 1_000);
        assert_eq!(proposal.header.height, height_before + 1);
        assert_eq!(node.current_height(), height_before);
        assert_eq!(node.pending_len(), pending_before);
        assert_eq!(node.rpc().get_state_root(), root_before);

        // A faithful proposal verifies, and verification is also non-mutating.
        node.verify_block_without_commit(&proposal).unwrap();
        assert_eq!(node.current_height(), height_before);
        assert_eq!(node.pending_len(), pending_before);
        assert_eq!(node.rpc().get_state_root(), root_before);

        // A proposal whose committed roots were tampered is rejected (the
        // re-execution produces the correct roots, which no longer match).
        let mut forged_root = proposal.clone();
        forged_root.header.global_state_root = "00".repeat(32);
        assert!(matches!(
            node.verify_block_without_commit(&forged_root),
            Err(NodeError::BlockProposalRejected(_))
        ));

        // A proposal that does not build on the current tip is rejected.
        let wrong_height = node.build_proposal(height_before + 2, 1_000);
        assert!(matches!(
            node.verify_block_without_commit(&wrong_height),
            Err(NodeError::BlockProposalRejected(_))
        ));

        // Committing the verified proposal advances the chain and drains the
        // now-committed transaction from the mempool.
        node.import_block(&proposal).unwrap();
        assert_eq!(node.current_height(), height_before + 1);
        assert!(node.pending_len() < pending_before);
        assert_eq!(
            node.rpc().get_state_root(),
            proposal.header.global_state_root
        );
    }

    #[test]
    fn validator_cannot_sign_two_blocks_at_one_height() {
        // The safety foundation for proposer rotation: an honest validator signs
        // at most one block per height, durably. With quorum overlap this means
        // at most one block per height can gather a finality certificate, so no
        // two conflicting blocks can be finalized — no fork — even when multiple
        // proposers propose for the same height during a view change.
        let dir = temp_dir("bft-anti-equivocation");
        let node = PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        let key = ValidatorSigningKey::from_seed("validator-1", "consensus-key-1", [1u8; 32]);
        let vote = |hash: &str| {
            NetworkMessage::Vote(Vote {
                validator_id: "validator-1".into(),
                height: 1,
                block_hash: hash.into(),
            })
        };

        // First vote at height 1 is accepted; re-signing the same block is
        // idempotent; a different block at the same height is rejected.
        node.sign_validator_message(&key, vote("block-aaa"))
            .unwrap();
        node.sign_validator_message(&key, vote("block-aaa"))
            .unwrap();
        assert!(matches!(
            node.sign_validator_message(&key, vote("block-bbb")),
            Err(NodeError::ConsensusSigningConflict { height: 1, .. })
        ));

        // The lock survives a restart (it is persisted, not just in memory).
        drop(node);
        let restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        assert!(matches!(
            restarted.sign_validator_message(&key, vote("block-bbb")),
            Err(NodeError::ConsensusSigningConflict { height: 1, .. })
        ));
        // A different height is unaffected.
        restarted
            .sign_validator_message(
                &key,
                NetworkMessage::Vote(Vote {
                    validator_id: "validator-1".into(),
                    height: 2,
                    block_hash: "block-ccc".into(),
                }),
            )
            .unwrap();
    }

    #[test]
    fn da_coding_fraud_proof_rpc_detects_inconsistent_manifest() {
        let dir = temp_dir("da-coding-fraud");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        node.submit_transaction(transfer_tx()).unwrap();
        let block = node
            .produce_block_with_data_availability(1, 1_000, 64)
            .unwrap();
        let commitment = block.header.data_availability.clone().unwrap();
        let honest = node.load_da_share_set(&commitment.manifest_hash).unwrap();

        // Honest manifest: the RPC reports no coding fault and yields no proof.
        let honest_response = node.handle_rpc_request(RpcRequest::GetDaCodingFraudProof {
            manifest_hash: commitment.manifest_hash.clone(),
        });
        let RpcResponse::Ok(RpcResult::DaCodingFraud(honest_report)) = honest_response else {
            panic!("expected DA coding fraud report, got {honest_response:?}");
        };
        assert!(honest_report.manifest_available);
        assert!(!honest_report.fault_detected);
        assert!(honest_report.proof.is_none());

        // Forge a payload-hash-inconsistent manifest. The share commitment stays
        // internally consistent (share_root is over share_hashes only), so it
        // persists, but its data shares no longer decode to the committed payload.
        let mut forged = honest.manifest.clone();
        forged.payload_hash = "11".repeat(32);
        let forged_hash = node.storage.commit_da_manifest(&forged).unwrap();
        for share in honest
            .shares
            .iter()
            .filter(|share| share.index < forged.original_share_count)
        {
            let mut rebound = share.clone();
            rebound.manifest_hash = forged_hash.clone();
            node.storage.commit_da_share(&rebound).unwrap();
        }

        let response = node.handle_rpc_request(RpcRequest::GetDaCodingFraudProof {
            manifest_hash: forged_hash.clone(),
        });
        let RpcResponse::Ok(RpcResult::DaCodingFraud(report)) = response else {
            panic!("expected DA coding fraud report, got {response:?}");
        };
        assert!(report.manifest_available);
        assert_eq!(
            report.available_data_share_count,
            forged.original_share_count
        );
        assert!(report.fault_detected);
        assert!(matches!(
            report.fault,
            Some(detta_da::DaCodingFault::PayloadHashMismatch { .. })
        ));
        let proof = report.proof.expect("coding fraud proof present");
        proof.validate(&forged).unwrap();
    }

    #[test]
    fn persistent_node_imports_snapshot_from_da_reed_solomon_checkpoint() {
        let source_dir = temp_dir("da-snapshot-source");
        let sink_dir = temp_dir("da-snapshot-sink");
        let mut source =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &source_dir).unwrap();
        let sink = PersistentValidatorNode::bootstrap(
            "validator-2",
            DeTTaState::new("detta-local"),
            &sink_dir,
        )
        .unwrap();
        source.submit_transaction(transfer_tx()).unwrap();
        source.produce_block(1, 1_000).unwrap();

        let (chunk_set, share_set) = source.build_snapshot_da_share_set(64, 3, 2).unwrap();
        let da_manifest_hash = share_set.manifest.manifest_hash().unwrap();
        let certificate = DaAvailabilityCertificate::from_manifest(
            &share_set.manifest,
            vec!["validator-1".into(), "validator-2".into()],
        )
        .unwrap();
        let da_certificate_hash = certificate.certificate_hash().unwrap();
        let required_metadata_roots = chunk_set.manifest.metadata_roots.clone();
        let threshold_share_indices = [0_u32, 2, 4];
        let threshold_share_set = DaShareSet {
            manifest: share_set.manifest.clone(),
            shares: threshold_share_indices
                .into_iter()
                .map(|index| {
                    share_set
                        .shares
                        .iter()
                        .find(|share| share.index == index)
                        .unwrap()
                        .clone()
                })
                .collect(),
        };

        assert_eq!(
            threshold_share_set.shares.len() as u32,
            threshold_share_set.manifest.reconstruction_threshold
        );
        threshold_share_set.verify().unwrap();

        let mut tampered_share_set = threshold_share_set.clone();
        tampered_share_set.shares[0].bytes[0] ^= 0xff;
        assert!(matches!(
            sink.import_snapshot_from_da_share_set(&tampered_share_set, &required_metadata_roots,),
            Err(NodeError::DataAvailability(
                DaError::ShareHashMismatch { .. }
            ))
        ));

        let mut wrong_certificate = certificate.clone();
        wrong_certificate.share_root = "wrong-share-root".into();
        assert!(matches!(
            sink.import_snapshot_from_da_share_set_with_certificate(
                &threshold_share_set,
                Some(&wrong_certificate),
                &required_metadata_roots,
            ),
            Err(NodeError::DataAvailability(
                DaError::InvalidAvailabilityCertificate(_)
            ))
        ));

        let imported = sink
            .import_snapshot_from_da_share_set_with_certificate(
                &threshold_share_set,
                Some(&certificate),
                &required_metadata_roots,
            )
            .unwrap();
        assert_eq!(
            imported.global_state_root,
            chunk_set.manifest.snapshot_root.clone()
        );
        assert_eq!(
            sink.load_required_snapshot_metadata_roots().unwrap(),
            required_metadata_roots
        );
        assert_eq!(
            sink.load_snapshot_import_audit_records().unwrap(),
            vec![SnapshotImportAuditRecord {
                snapshot_root: chunk_set.manifest.snapshot_root.clone(),
                manifest_hash: chunk_set.manifest.manifest_hash().unwrap(),
                required_metadata_roots_root:
                    FileStorage::required_snapshot_metadata_roots_root_for(
                        &chunk_set.manifest.metadata_roots,
                    )
                    .unwrap(),
                required_metadata_roots_count: chunk_set.manifest.metadata_roots.len(),
                manifest_metadata_roots_count: chunk_set.manifest.metadata_roots.len(),
                chunk_count: chunk_set.manifest.chunk_count,
                metadata_roots_verified: true,
                da_manifest_hash: Some(da_manifest_hash),
                da_certificate_hash: Some(da_certificate_hash),
            }]
        );

        fs::remove_dir_all(source_dir).unwrap();
        fs::remove_dir_all(sink_dir).unwrap();
    }

    #[test]
    fn fetches_da_snapshot_shares_from_multiple_tcp_peers_and_skips_invalid_share() {
        let source_dir = temp_dir("da-snapshot-multi-peer-source");
        let mut source =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &source_dir).unwrap();
        source.submit_transaction(transfer_tx()).unwrap();
        source.produce_block(1, 1_000).unwrap();
        let (_, share_set) = source.build_snapshot_da_share_set(64, 3, 2).unwrap();
        let manifest_hash = share_set.manifest.manifest_hash().unwrap();

        let mut addrs = Vec::new();
        let mut handles = Vec::new();

        let bad_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        addrs.push(bad_listener.local_addr().unwrap());
        let bad_manifest = share_set.manifest.clone();
        let mut bad_share = share_set.shares[0].clone();
        bad_share.bytes[0] ^= 0xff;
        handles.push(thread::spawn(move || {
            let (stream, _) = bad_listener.accept().unwrap();
            let mut tcp = TcpProtocolStream::from_stream(stream);
            match tcp.receive().unwrap() {
                NetworkMessage::DaShareRequest(_) => {
                    tcp.send(&NetworkMessage::DaManifest(Box::new(bad_manifest)))
                        .unwrap();
                    tcp.send(&NetworkMessage::DaShare(bad_share)).unwrap();
                }
                message => panic!("expected DA share request, got {message:?}"),
            }
        }));

        for peer_index in 0..3 {
            let peer_dir = temp_dir(&format!("da-snapshot-share-peer-{peer_index}"));
            let peer = PersistentValidatorNode::bootstrap(
                format!("validator-{}", peer_index + 2),
                seeded_state(),
                &peer_dir,
            )
            .unwrap();
            peer.storage.commit_da_share_set(&share_set).unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            addrs.push(listener.local_addr().unwrap());
            handles.push(thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                let mut tcp = TcpProtocolStream::from_stream(stream);
                match tcp.receive().unwrap() {
                    NetworkMessage::DaShareRequest(request) => {
                        for response in peer.serve_da_share_request(&request).unwrap() {
                            tcp.send(&response).unwrap();
                        }
                    }
                    message => panic!("expected DA share request, got {message:?}"),
                }
                fs::remove_dir_all(peer_dir).unwrap();
            }));
        }

        let (fetched, metrics) = fetch_da_share_set_from_tcp_peers(
            |peer_index| TcpProtocolStream::connect(addrs[peer_index]),
            addrs.len(),
            manifest_hash,
            1,
        )
        .unwrap();

        assert_eq!(fetched.manifest, share_set.manifest);
        assert_eq!(
            fetched.shares.len() as u32,
            fetched.manifest.reconstruction_threshold
        );
        assert_eq!(
            fetched.reconstruct_payload().unwrap(),
            share_set.reconstruct_payload().unwrap()
        );
        assert_eq!(metrics.peer_attempts, 4);
        assert_eq!(metrics.peer_failures, 0);
        assert_eq!(metrics.invalid_responses, 1);
        assert_eq!(metrics.shares_received, 3);
        assert!(metrics.payload_reconstructable);
        assert_eq!(metrics.peer_scores.len(), 4);
        assert_eq!(
            metrics.peer_scores[0],
            DaShareSyncPeerScore {
                peer_index: 0,
                score: -1,
                valid_responses: 0,
                invalid_responses: 1,
                failures: 0,
            }
        );
        for peer_score in &metrics.peer_scores[1..] {
            assert_eq!(peer_score.score, 1);
            assert_eq!(peer_score.valid_responses, 1);
            assert_eq!(peer_score.invalid_responses, 0);
            assert_eq!(peer_score.failures, 0);
        }

        for handle in handles {
            handle.join().unwrap();
        }
        fs::remove_dir_all(source_dir).unwrap();
    }

    #[test]
    fn da_share_request_rate_limiter_blocks_excessive_peer_requests() {
        let source_dir = temp_dir("da-share-rate-limit-source");
        let mut source =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &source_dir).unwrap();
        source.submit_transaction(transfer_tx()).unwrap();
        let block = source
            .produce_block_with_data_availability(1, 1_000, 64)
            .unwrap();
        let manifest_hash = block
            .header
            .data_availability
            .as_ref()
            .unwrap()
            .manifest_hash
            .clone();
        let request = DaShareRequest {
            manifest_hash,
            start_index: 0,
            max_shares: 1,
        };
        let mut limiter = DaShareRequestRateLimiter::new(1).unwrap();

        let first_response = source
            .serve_da_share_request_with_rate_limit("peer-1", &request, &mut limiter)
            .unwrap();
        assert_eq!(limiter.request_count("peer-1"), 1);
        assert!(matches!(first_response[0], NetworkMessage::DaManifest(_)));
        assert!(matches!(first_response[1], NetworkMessage::DaShare(_)));

        assert!(matches!(
            source.serve_da_share_request_with_rate_limit("peer-1", &request, &mut limiter),
            Err(NodeError::DataAvailability(DaError::InvalidPayload(_)))
        ));
        assert_eq!(limiter.request_count("peer-1"), 1);

        source
            .serve_da_share_request_with_rate_limit("peer-2", &request, &mut limiter)
            .unwrap();
        assert_eq!(limiter.request_count("peer-2"), 1);

        fs::remove_dir_all(source_dir).unwrap();
    }

    #[test]
    fn persistent_node_replays_da_certified_block_after_checkpoint_import() {
        let source_dir = temp_dir("da-checkpoint-replay-source");
        let sink_dir = temp_dir("da-checkpoint-replay-sink");
        let mut source =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &source_dir).unwrap();
        let sink = PersistentValidatorNode::bootstrap(
            "validator-2",
            DeTTaState::new("detta-local"),
            &sink_dir,
        )
        .unwrap();
        source.submit_transaction(transfer_tx()).unwrap();
        source.produce_block(1, 1_000).unwrap();
        let (checkpoint_chunk_set, checkpoint_share_set) =
            source.build_snapshot_da_share_set(64, 3, 2).unwrap();
        let checkpoint_certificate = DaAvailabilityCertificate::from_manifest(
            &checkpoint_share_set.manifest,
            vec!["validator-1".into(), "validator-2".into()],
        )
        .unwrap();
        sink.import_snapshot_from_da_share_set_with_certificate(
            &checkpoint_share_set,
            Some(&checkpoint_certificate),
            &checkpoint_chunk_set.manifest.metadata_roots,
        )
        .unwrap();
        let mut sink = PersistentValidatorNode::restart("validator-2", &sink_dir).unwrap();
        assert_eq!(sink.current_height(), 1);

        source
            .submit_transaction(tx_to(
                "TokenA",
                "tx2",
                "Alice",
                2,
                Method::Transfer,
                vec![
                    Argument::Principal("Bob".into()),
                    Argument::Asset("USDC".into()),
                    Argument::Amount(10),
                ],
            ))
            .unwrap();
        let source_block = source
            .produce_block_with_data_availability(2, 2_000, 64)
            .unwrap();
        let commitment = source_block.header.data_availability.as_ref().unwrap();
        let block_share_set = source.load_da_share_set(&commitment.manifest_hash).unwrap();
        let block_certificate = DaAvailabilityCertificate::from_manifest(
            &block_share_set.manifest,
            vec!["validator-1".into(), "validator-2".into()],
        )
        .unwrap();

        let imported_block = sink
            .import_da_certified_block_after_checkpoint(&block_share_set, &block_certificate)
            .unwrap();
        let mut expected_block = source_block.clone();
        expected_block
            .header
            .data_availability
            .as_mut()
            .unwrap()
            .certificate_hash = Some(block_certificate.certificate_hash().unwrap());
        assert_eq!(imported_block, expected_block);
        assert_eq!(sink.current_height(), 2);
        assert_eq!(sink.rpc().call_balance_view("TokenA", "Bob", "USDC"), 70);

        let mut wrong_certificate = block_certificate.clone();
        wrong_certificate.block_hash = "wrong-block-hash".into();
        assert!(matches!(
            sink.import_da_certified_block_after_checkpoint(&block_share_set, &wrong_certificate),
            Err(NodeError::DataAvailability(
                DaError::InvalidAvailabilityCertificate(_)
            ))
        ));

        fs::remove_dir_all(source_dir).unwrap();
        fs::remove_dir_all(sink_dir).unwrap();
    }

    #[test]
    fn persistent_node_stores_da_manifest_and_share_gossip() {
        let source_dir = temp_dir("da-gossip-source");
        let sink_dir = temp_dir("da-gossip-sink");
        let mut source =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &source_dir).unwrap();
        let mut sink =
            PersistentValidatorNode::bootstrap("validator-2", seeded_state(), &sink_dir).unwrap();
        source.submit_transaction(transfer_tx()).unwrap();
        let block = source
            .produce_block_with_data_availability(1, 1_000, 64)
            .unwrap();
        let commitment = block.header.data_availability.as_ref().unwrap();
        let share_set = source.load_da_share_set(&commitment.manifest_hash).unwrap();
        let certificate = DaAvailabilityCertificate::from_manifest(
            &share_set.manifest,
            vec!["validator-1".into(), "validator-2".into()],
        )
        .unwrap();
        let certificate_hash = certificate.certificate_hash().unwrap();
        let mut transport =
            InMemoryTransport::new(["validator-1".into(), "validator-2".into()]).unwrap();

        assert_eq!(
            source
                .gossip_data_availability_for_block(&block, &mut transport)
                .unwrap(),
            share_set.shares.len() + 1
        );
        let da_envelopes = transport.drain_peer("validator-2").unwrap();
        assert_eq!(da_envelopes.len(), share_set.shares.len() + 1);
        assert!(matches!(
            da_envelopes[0].message,
            NetworkMessage::DaManifest(_)
        ));
        for envelope in da_envelopes {
            assert_eq!(
                sink.ingest_network_envelope(&envelope).unwrap(),
                NetworkIngestOutcome::DataAvailabilityStored
            );
        }
        assert_eq!(
            sink.ingest_network_envelope(&Envelope {
                from: "validator-1".into(),
                to: "validator-2".into(),
                message: NetworkMessage::DaAvailabilityCertificate(Box::new(certificate.clone())),
            })
            .unwrap(),
            NetworkIngestOutcome::DataAvailabilityStored
        );

        assert_eq!(
            sink.load_da_share_set(&commitment.manifest_hash)
                .unwrap()
                .reconstruct_payload()
                .unwrap(),
            share_set.reconstruct_payload().unwrap()
        );
        assert_eq!(
            sink.load_da_certificate(&certificate_hash).unwrap(),
            certificate
        );
        assert!(matches!(
            sink.serve_da_share_request(&DaShareRequest {
                manifest_hash: commitment.manifest_hash.clone(),
                start_index: 0,
                max_shares: DEFAULT_MAX_DA_SHARES_PER_REQUEST + 1,
            }),
            Err(NodeError::DataAvailability(DaError::InvalidPayload(_)))
        ));

        fs::remove_dir_all(source_dir).unwrap();
        fs::remove_dir_all(sink_dir).unwrap();
    }

    #[test]
    fn da_gossip_load_handles_large_aspect_module_deployments() {
        let source_dir = temp_dir("da-gossip-large-aspect-source");
        let sink_dirs = (2..=4)
            .map(|validator| temp_dir(&format!("da-gossip-large-aspect-sink-{validator}")))
            .collect::<Vec<_>>();
        let mut source =
            PersistentValidatorNode::bootstrap("validator-1", state_with_factory(), &source_dir)
                .unwrap();
        let mut sinks = sink_dirs
            .iter()
            .enumerate()
            .map(|(index, dir)| {
                PersistentValidatorNode::bootstrap(
                    format!("validator-{}", index + 2),
                    state_with_factory(),
                    dir,
                )
                .unwrap()
            })
            .collect::<Vec<_>>();

        source
            .submit_transaction(tx_to(
                "Factory",
                "tx-aspect-load-source",
                "Issuer",
                1,
                Method::SubmitAspectModule,
                vec![text("ERC20ConformantToken"), text(ASPECT_SOURCE)],
            ))
            .unwrap();
        for index in 2..=9 {
            source
                .submit_transaction(large_aspect_submission(index))
                .unwrap();
        }
        let block = source
            .produce_block_with_data_availability(1, 1_000, 4096)
            .unwrap();
        let commitment = block.header.data_availability.as_ref().unwrap().clone();
        let share_set = source.load_da_share_set(&commitment.manifest_hash).unwrap();
        let expected_message_count = share_set.shares.len() + 1;
        let mut transport = InMemoryTransport::new([
            "validator-1".into(),
            "validator-2".into(),
            "validator-3".into(),
            "validator-4".into(),
        ])
        .unwrap();

        assert!(share_set.manifest.payload_bytes > 24 * 1024);
        assert_eq!(
            share_set.manifest.erasure_scheme,
            detta_da::ErasureScheme::ReedSolomonV1
        );
        assert!(share_set.manifest.share_size_bytes <= 4096);
        assert_eq!(block.receipts.len(), 9);
        assert!(block
            .receipts
            .iter()
            .all(|receipt| receipt.status == TxStatus::Committed));
        assert_eq!(
            source
                .gossip_data_availability_for_block(&block, &mut transport)
                .unwrap(),
            expected_message_count * sinks.len()
        );

        for sink in &mut sinks {
            let envelopes = transport.drain_peer(sink.validator_id()).unwrap();
            assert_eq!(envelopes.len(), expected_message_count);
            for envelope in envelopes {
                assert_eq!(
                    sink.ingest_network_envelope(&envelope).unwrap(),
                    NetworkIngestOutcome::DataAvailabilityStored
                );
            }
            let payload = sink.load_da_payload(&commitment.manifest_hash).unwrap();
            assert_eq!(payload.height, 1);
            assert_eq!(
                payload.namespace_root().unwrap(),
                share_set.manifest.namespace_root
            );
            let status = sink.da_status(&commitment.manifest_hash).unwrap();
            assert!(status.payload_reconstructable);
            assert!(status.missing_share_indices.is_empty());
        }

        fs::remove_dir_all(source_dir).unwrap();
        for dir in sink_dirs {
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn da_certified_blocks_sustain_defi_traffic_and_certificates() {
        let dir = temp_dir("da-sustained-defi");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", defi_da_state(), &dir).unwrap();
        let workloads = vec![
            vec![
                tx_to(
                    "OracleA",
                    "tx-da-defi-oracle-1",
                    "OracleBot",
                    1,
                    Method::SubmitPrice,
                    vec![asset("ATOM"), amount(2), amount(0)],
                ),
                tx_to(
                    "PoolAB",
                    "tx-da-defi-liquidity-1",
                    "Alice",
                    1,
                    Method::AddLiquidity,
                    vec![amount(100), amount(100)],
                ),
                tx_to(
                    "VaultA",
                    "tx-da-defi-deposit-1",
                    "Alice",
                    2,
                    Method::DepositCollateral,
                    vec![asset("ATOM"), amount(100)],
                ),
                tx_to(
                    "StakeA",
                    "tx-da-defi-stake-1",
                    "Alice",
                    3,
                    Method::Stake,
                    vec![asset("ATOM"), amount(50)],
                ),
            ],
            vec![
                tx_to(
                    "VaultA",
                    "tx-da-defi-borrow-1",
                    "Alice",
                    4,
                    Method::Borrow,
                    vec![asset("USDC"), amount(50)],
                ),
                tx_to(
                    "PoolAB",
                    "tx-da-defi-swap-1",
                    "Alice",
                    5,
                    Method::Swap,
                    vec![asset("USDC"), amount(10), amount(1)],
                ),
            ],
            vec![
                tx_to(
                    "OracleA",
                    "tx-da-defi-oracle-2",
                    "OracleBot",
                    2,
                    Method::SubmitPrice,
                    vec![asset("ATOM"), amount(3), amount(2_000)],
                ),
                tx_to(
                    "PoolAB",
                    "tx-da-defi-liquidity-2",
                    "Alice",
                    6,
                    Method::AddLiquidity,
                    vec![amount(25), amount(25)],
                ),
                tx_to(
                    "StakeA",
                    "tx-da-defi-stake-2",
                    "Alice",
                    7,
                    Method::Stake,
                    vec![asset("ATOM"), amount(25)],
                ),
            ],
        ];

        for (height, transactions) in workloads.into_iter().enumerate() {
            for transaction in transactions {
                node.submit_transaction(transaction).unwrap();
            }
            let height = height as u64 + 1;
            let block = node
                .produce_block_with_data_availability(height, height * 1_000, 96)
                .unwrap();
            assert!(block
                .receipts
                .iter()
                .all(|receipt| receipt.status == TxStatus::Committed));
            let manifest_hash = block
                .header
                .data_availability
                .as_ref()
                .unwrap()
                .manifest_hash
                .clone();
            let share_set = node.load_da_share_set(&manifest_hash).unwrap();
            let certificate = DaAvailabilityCertificate::from_manifest(
                &share_set.manifest,
                vec![
                    "validator-1".into(),
                    "validator-2".into(),
                    "validator-3".into(),
                ],
            )
            .unwrap();
            let certificate_hash = node.persist_da_certificate(&certificate).unwrap();
            assert_eq!(
                node.load_da_certificate(&certificate_hash).unwrap(),
                certificate
            );
            let status = node.da_status(&manifest_hash).unwrap();
            assert!(status.payload_reconstructable);
            assert_eq!(status.stored_share_count, status.expected_share_count);
        }

        assert_eq!(
            node.rpc().call_balance_view("TokenATOM", "Alice", "ATOM"),
            708
        );
        assert_eq!(
            node.rpc().call_balance_view("TokenUSDC", "Alice", "USDC"),
            915
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn long_running_multi_validator_retention_simulation_survives_restart() {
        let validator_ids = ["validator-1", "validator-2", "validator-3", "validator-4"];
        let dirs = validator_ids
            .iter()
            .map(|validator| temp_dir(&format!("da-retention-{validator}")))
            .collect::<Vec<_>>();
        let keys = validator_ids
            .iter()
            .enumerate()
            .map(|(index, validator)| validator_key(validator, index as u8 + 20))
            .collect::<Vec<_>>();
        let public_keys = keys.iter().map(|key| key.public_key()).collect::<Vec<_>>();
        let mut validators = validator_ids
            .iter()
            .zip(dirs.iter())
            .map(|(validator, dir)| {
                PersistentValidatorNode::bootstrap_with_validator_set(
                    *validator,
                    seeded_state(),
                    dir,
                    "detta-testnet",
                    public_keys.clone(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();

        let retention_policy = detta_storage::DaRetentionPolicyConfig {
            policies: vec![
                detta_storage::DaRetentionPolicy {
                    class: detta_storage::DaRetentionClass::Hot,
                    retain_payloads: true,
                    retain_all_shares: true,
                    min_retention_blocks: 16,
                    max_payload_bytes: Some(1024 * 1024),
                },
                detta_storage::DaRetentionPolicy {
                    class: detta_storage::DaRetentionClass::Checkpoint,
                    retain_payloads: true,
                    retain_all_shares: true,
                    min_retention_blocks: 128,
                    max_payload_bytes: None,
                },
            ],
        };
        for validator in &validators {
            validator
                .storage
                .commit_da_retention_policy(&retention_policy)
                .unwrap();
        }

        let mut manifest_hashes = Vec::new();
        for height in 1..=12_u64 {
            validators[0]
                .submit_transaction(tx_to(
                    "TokenA",
                    &format!("tx-da-retention-{height}"),
                    "Alice",
                    height,
                    Method::Transfer,
                    vec![Argument::Principal("Bob".into()), asset("USDC"), amount(1)],
                ))
                .unwrap();
            let block = validators[0]
                .produce_block_with_data_availability(height, height * 1_000, 64)
                .unwrap();
            let manifest_hash = block
                .header
                .data_availability
                .as_ref()
                .unwrap()
                .manifest_hash
                .clone();
            let share_set = validators[0].load_da_share_set(&manifest_hash).unwrap();
            for validator in validators.iter().skip(1) {
                validator.storage.commit_da_share_set(&share_set).unwrap();
            }
            for (validator, key) in validators.iter().zip(keys.iter()) {
                assert!(matches!(
                    validator
                        .sign_da_availability_vote(key, &share_set.manifest, &share_set.shares, 2)
                        .unwrap(),
                    NetworkMessage::SignedValidator(_)
                ));
            }
            manifest_hashes.push(manifest_hash);
        }

        drop(validators);
        let restarted = validator_ids
            .iter()
            .zip(dirs.iter())
            .map(|(validator, dir)| PersistentValidatorNode::restart(*validator, dir).unwrap())
            .collect::<Vec<_>>();
        for node in &restarted {
            assert_eq!(
                node.storage.maybe_load_da_retention_policy().unwrap(),
                Some(retention_policy.clone())
            );
            for manifest_hash in &manifest_hashes {
                assert!(node.load_da_share_set(manifest_hash).is_ok());
                assert!(
                    node.da_status(manifest_hash)
                        .unwrap()
                        .payload_reconstructable
                );
            }
            assert!(node
                .storage
                .maybe_load_consensus_signing_record(
                    node.validator_id(),
                    ValidatorSignatureDomain::DaAvailabilityVote,
                    12,
                )
                .unwrap()
                .is_some());
        }
        let conflicting_vote = DaAvailabilityVote {
            chain_id: "detta-local".into(),
            height: 12,
            block_hash: "conflicting-block".into(),
            manifest_hash: "conflicting-manifest".into(),
            share_root: "conflicting-share-root".into(),
            validator_id: "validator-1".into(),
            custody_share_indices: Vec::new(),
            sampled_share_indices: Vec::new(),
        };
        assert!(matches!(
            restarted[0].sign_validator_message(
                &keys[0],
                NetworkMessage::DaAvailabilityVote(conflicting_vote),
            ),
            Err(NodeError::ConsensusSigningConflict { .. })
        ));

        for dir in dirs {
            fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn da_retrieval_recovers_with_offline_validator_minority() {
        let source_dir = temp_dir("da-offline-retrieval-source");
        let mut source =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &source_dir).unwrap();
        source.submit_transaction(transfer_tx()).unwrap();
        source.produce_block(1, 1_000).unwrap();
        let (_, share_set) = source.build_snapshot_da_share_set(64, 3, 2).unwrap();
        let manifest_hash = share_set.manifest.manifest_hash().unwrap();

        let offline_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let offline_addr = offline_listener.local_addr().unwrap();
        drop(offline_listener);
        let mut addrs = vec![offline_addr];
        let mut handles = Vec::new();
        for peer_index in 0..3 {
            let peer_dir = temp_dir(&format!("da-offline-retrieval-peer-{peer_index}"));
            let peer = PersistentValidatorNode::bootstrap(
                format!("validator-{}", peer_index + 2),
                seeded_state(),
                &peer_dir,
            )
            .unwrap();
            peer.storage.commit_da_share_set(&share_set).unwrap();
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            addrs.push(listener.local_addr().unwrap());
            handles.push(thread::spawn(move || {
                let (stream, _) = listener.accept().unwrap();
                let mut tcp = TcpProtocolStream::from_stream(stream);
                match tcp.receive().unwrap() {
                    NetworkMessage::DaShareRequest(request) => {
                        for response in peer.serve_da_share_request(&request).unwrap() {
                            tcp.send(&response).unwrap();
                        }
                    }
                    message => panic!("expected DA share request, got {message:?}"),
                }
                fs::remove_dir_all(peer_dir).unwrap();
            }));
        }

        let (fetched, metrics) = fetch_da_share_set_from_tcp_peers(
            |peer_index| TcpProtocolStream::connect(addrs[peer_index]),
            addrs.len(),
            manifest_hash,
            1,
        )
        .unwrap();

        assert_eq!(metrics.peer_attempts, 4);
        assert_eq!(metrics.peer_failures, 1);
        assert_eq!(metrics.shares_received, 3);
        assert!(metrics.payload_reconstructable);
        assert_eq!(
            fetched.reconstruct_payload().unwrap(),
            share_set.reconstruct_payload().unwrap()
        );
        for handle in handles {
            handle.join().unwrap();
        }
        fs::remove_dir_all(source_dir).unwrap();
    }

    #[test]
    fn archive_node_reconstructs_historical_da_payloads_after_restart() {
        let source_dir = temp_dir("da-archive-source");
        let archive_dir = temp_dir("da-archive-node");
        let mut source =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &source_dir).unwrap();
        let mut archive =
            PersistentValidatorNode::bootstrap("archive-1", seeded_state(), &archive_dir).unwrap();
        let mut historical = Vec::new();

        for height in 1..=5_u64 {
            source
                .submit_transaction(tx_to(
                    "TokenA",
                    &format!("tx-da-archive-{height}"),
                    "Alice",
                    height,
                    Method::Transfer,
                    vec![Argument::Principal("Bob".into()), asset("USDC"), amount(1)],
                ))
                .unwrap();
            let block = source
                .produce_block_with_data_availability(height, height * 1_000, 64)
                .unwrap();
            let manifest_hash = block
                .header
                .data_availability
                .as_ref()
                .unwrap()
                .manifest_hash
                .clone();
            let share_set = source.load_da_share_set(&manifest_hash).unwrap();
            let certificate = DaAvailabilityCertificate::from_manifest(
                &share_set.manifest,
                vec!["validator-1".into(), "archive-1".into()],
            )
            .unwrap();
            archive.storage.commit_da_share_set(&share_set).unwrap();
            archive.persist_da_certificate(&certificate).unwrap();
            let payload = share_set.reconstruct_payload().unwrap();
            let mut reconstructed_block = block_from_da_payload(&payload).unwrap();
            reconstructed_block.header.data_availability = Some(DataAvailabilityCommitment {
                payload_root: share_set.manifest.payload_hash.clone(),
                manifest_hash: share_set.manifest.manifest_hash().unwrap(),
                share_root: share_set.manifest.share_root.clone(),
                certificate_hash: None,
            });
            archive.import_block(&reconstructed_block).unwrap();
            assert_eq!(reconstructed_block.block_hash(), block.block_hash());
            historical.push((height, manifest_hash, block));
        }

        drop(archive);
        let archive = PersistentValidatorNode::restart("archive-1", &archive_dir).unwrap();
        for (height, manifest_hash, expected_block) in historical {
            let payload = archive.load_da_payload(&manifest_hash).unwrap();
            assert_eq!(payload.height, height);
            assert_eq!(
                archive.load_block(height).unwrap().block_hash(),
                expected_block.block_hash()
            );
            let tx_hashes = payload
                .namespaces
                .iter()
                .flat_map(|section| section.records.iter())
                .filter_map(|record| match record {
                    DaRecord::SignedTransaction(transaction) => Some(transaction.tx_hash.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(tx_hashes, vec![format!("tx-da-archive-{height}")]);
        }

        fs::remove_dir_all(source_dir).unwrap();
        fs::remove_dir_all(archive_dir).unwrap();
    }

    #[test]
    fn persistent_node_restores_from_backup_and_verifies_finalized_roots() {
        let dir = temp_dir("backup-node-source");
        let backup_dir = temp_dir("backup-node-copy");
        let restore_dir = temp_dir("backup-node-restore");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        node.submit_transaction(transfer_tx()).unwrap();
        let block = node.produce_block(1, 1_000).unwrap();
        let certificate = FinalityCertificate {
            height: 1,
            block_hash: block.block_hash(),
            signers: vec!["validator-1".into()],
        };
        node.persist_finality_certificate(&certificate).unwrap();
        let expected_root = block.header.global_state_root.clone();

        let manifest = node.storage.backup_to(&backup_dir).unwrap();
        FileStorage::restore_from_backup(&backup_dir, &restore_dir).unwrap();
        let mut restored = PersistentValidatorNode::restart("validator-1", &restore_dir).unwrap();

        assert_eq!(manifest.highest_block_height, Some(1));
        assert_eq!(manifest.highest_finality_certificate_height, Some(1));
        assert_eq!(
            restored.load_block(1).unwrap().header.global_state_root,
            expected_root
        );
        assert_eq!(restored.load_finality_certificate(1).unwrap(), certificate);
        assert_eq!(
            restored.handle_rpc_request(RpcRequest::GetBlock { height: 1 }),
            RpcResponse::Ok(RpcResult::Block(Box::new(block.clone())))
        );
        assert_eq!(
            restored.handle_rpc_request(RpcRequest::GetFinalityCertificate { height: 1 }),
            RpcResponse::Ok(RpcResult::FinalityCertificate(Box::new(certificate)))
        );

        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(backup_dir).unwrap();
        fs::remove_dir_all(restore_dir).unwrap();
    }

    #[test]
    fn persistent_node_reports_operational_health() {
        let dir = temp_dir("node-health");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        node.trust_validator_key(validator_key("validator-2", 8).public_key());
        node.submit_transaction(transfer_tx()).unwrap();

        let response = node.handle_rpc_request(RpcRequest::GetNodeHealth);

        let RpcResponse::Ok(RpcResult::NodeHealth(health)) = response else {
            panic!("expected node health response");
        };
        assert_eq!(health.network_id.as_deref(), Some(DEFAULT_NODE_NETWORK_ID));
        assert_eq!(health.validator_id.as_deref(), Some("validator-1"));
        assert_eq!(health.chain_id, "detta-local");
        assert_eq!(health.height, 0);
        assert_eq!(
            health.da_production_profile,
            Some(DaProductionProfile::v1())
        );
        assert_eq!(health.pending_mempool_transactions, 1);
        assert_eq!(health.trusted_validator_keys, Some(1));
        assert_eq!(health.pending_validator_set_metadata_updates, Some(0));
        assert_eq!(
            health.global_state_root,
            node.rpc().node().state().global_state_root()
        );
        assert_eq!(
            health.storage_root,
            node.rpc().node().state().storage_root()
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_installs_production_da_retention_policy_by_default() {
        let dir = temp_dir("node-da-retention-production-default");
        let node = PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        let production_policy = DaRetentionPolicyConfig::production_default();

        assert_eq!(
            node.storage.maybe_load_da_retention_policy().unwrap(),
            Some(production_policy.clone())
        );

        drop(node);
        let restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        assert_eq!(
            restarted.storage.maybe_load_da_retention_policy().unwrap(),
            Some(production_policy)
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_preserves_existing_da_retention_policy() {
        let dir = temp_dir("node-da-retention-custom-policy");
        let custom_policy = DaRetentionPolicyConfig {
            policies: vec![detta_storage::DaRetentionPolicy {
                class: detta_storage::DaRetentionClass::Hot,
                retain_payloads: true,
                retain_all_shares: true,
                min_retention_blocks: 32,
                max_payload_bytes: Some(1024 * 1024),
            }],
        };
        FileStorage::open(&dir)
            .unwrap()
            .commit_da_retention_policy(&custom_policy)
            .unwrap();

        let node = PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        assert_eq!(
            node.storage.maybe_load_da_retention_policy().unwrap(),
            Some(custom_policy.clone())
        );

        drop(node);
        let restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        assert_eq!(
            restarted.storage.maybe_load_da_retention_policy().unwrap(),
            Some(custom_policy)
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_reports_operator_metrics() {
        let dir = temp_dir("operator-metrics");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        node.submit_transaction(transfer_tx()).unwrap();
        let block = node.produce_block(1, 1_000).unwrap();
        node.persist_finality_certificate(&FinalityCertificate {
            height: 1,
            block_hash: block.block_hash(),
            signers: vec!["validator-1".into()],
        })
        .unwrap();
        node.ingest_network_envelope(&Envelope {
            from: "validator-2".into(),
            to: "validator-1".into(),
            message: NetworkMessage::FinalityCertificate(FinalityCertificate {
                height: 1,
                block_hash: block.block_hash(),
                signers: vec!["validator-1".into(), "validator-2".into()],
            }),
        })
        .unwrap();

        assert!(matches!(
            node.handle_rpc_request(RpcRequest::GetReceiptProof {
                height: 1,
                index: 0
            }),
            RpcResponse::Ok(RpcResult::ReceiptProof(_))
        ));
        assert!(matches!(
            node.handle_rpc_request(RpcRequest::GetBlock { height: 99 }),
            RpcResponse::Error(_)
        ));

        let response = node.handle_rpc_request(RpcRequest::GetOperatorMetrics);
        let RpcResponse::Ok(RpcResult::OperatorMetrics(metrics)) = response else {
            panic!("expected operator metrics response");
        };
        assert_eq!(metrics.network_id.as_deref(), Some(DEFAULT_NODE_NETWORK_ID));
        assert_eq!(metrics.validator_id.as_deref(), Some("validator-1"));
        assert_eq!(metrics.peer_count, Some(1));
        assert_eq!(
            metrics.da_production_profile,
            Some(DaProductionProfile::v1())
        );
        assert_eq!(metrics.mempool_size, 0);
        assert_eq!(metrics.consensus_height, 1);
        assert_eq!(metrics.highest_finalized_height, Some(1));
        assert_eq!(metrics.finality_lag, Some(0));
        assert!(metrics.last_block_execution_micros.is_some());
        assert!(metrics.last_proof_serving_micros.is_some());
        assert!(metrics.storage_bytes.unwrap() > 0);
        assert_eq!(metrics.rpc_error_count, 1);
        assert_eq!(metrics.da_manifest_count, 0);
        assert_eq!(metrics.da_missing_share_count, 0);
        assert_eq!(metrics.da_payload_count, 0);
        assert_eq!(metrics.da_challenge_record_count, 0);
        assert_eq!(metrics.da_challenge_evidence_count, 0);
        assert_eq!(metrics.da_custody_failure_count, 0);
        assert_eq!(metrics.da_repair_record_count, 0);
        assert_eq!(metrics.da_pending_repair_record_count, 0);
        assert_eq!(metrics.da_oldest_pending_repair_age_blocks, None);
        assert_eq!(
            metrics.da_total_bytes,
            node.storage.da_storage_stats().unwrap().total_bytes
        );
        assert!(metrics.da_total_bytes > 0);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_reports_operator_alerts() {
        let dir = temp_dir("operator-alerts");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        node.set_operator_alert_policy(OperatorAlertPolicy {
            min_peer_count: 1,
            max_finality_lag: 0,
            max_storage_bytes: 1,
            max_rpc_error_count: 0,
            max_mempool_size: 0,
            max_latest_block_failure_ratio_per_mille: 0,
            max_da_repair_lag_blocks: 0,
        });
        let failing_tx = tx_to(
            "TokenA",
            "tx-fail",
            "Alice",
            1,
            Method::Transfer,
            vec![
                Argument::Principal("Bob".into()),
                Argument::Asset("USDC".into()),
                Argument::Amount(1_000),
            ],
        );
        node.submit_transaction(failing_tx).unwrap();
        let block = node.produce_block(1, 1_000).unwrap();
        assert_eq!(block.receipts[0].status, TxStatus::Reverted);
        let mut persisted_metadata_roots = BTreeMap::new();
        persisted_metadata_roots.insert(
            SNAPSHOT_METADATA_VALIDATOR_SET_AUDIT_ROOT.into(),
            "stale-validator-set-audit-root".into(),
        );
        node.storage
            .commit_snapshot_metadata_roots(&persisted_metadata_roots)
            .unwrap();
        node.persist_equivocation_evidence(EquivocationEvidence {
            validator_id: "validator-2".into(),
            height: 1,
            first_block_hash: "block-a".into(),
            second_block_hash: "block-b".into(),
        })
        .unwrap();
        let da_payload = DaPayload::new(
            "detta-local",
            1,
            "previous-block",
            vec![DaNamespaceSection::new(
                DaNamespace::new("detta.tx").unwrap(),
                vec![DaRecord::SignedTransaction(transfer_tx())],
            )
            .unwrap()],
        )
        .unwrap();
        let da_share_set = DaShareSet::from_payload(&da_payload, "block-da", 64).unwrap();
        let da_manifest_hash = node
            .storage
            .commit_da_manifest(&da_share_set.manifest)
            .unwrap();
        let da_vote = DaAvailabilityVote::from_manifest_with_custody(
            &da_share_set.manifest,
            "validator-1",
            [0],
            [],
        )
        .unwrap();
        let da_challenge =
            DaShareChallenge::from_availability_vote(&da_vote, "validator-3", 0, 3).unwrap();
        node.persist_da_share_challenge(da_challenge.clone())
            .unwrap();
        let mut invalid_da_share = da_share_set.shares[0].clone();
        invalid_da_share.bytes[0] ^= 0x01;
        let da_response =
            DaShareChallengeResponse::from_share(&da_challenge, invalid_da_share).unwrap();
        node.persist_da_share_challenge_response(da_response.clone())
            .unwrap();
        let da_evidence = DaChallengeEvidence::invalid_response(
            &da_challenge,
            &da_response,
            &da_share_set.manifest,
            "validator-3",
            2,
        )
        .unwrap();
        node.persist_da_challenge_evidence(da_evidence).unwrap();
        node.storage
            .commit_da_repair_record(&detta_storage::DaRepairRecord {
                manifest_hash: da_manifest_hash.clone(),
                missing_share_indices: vec![0],
                recorded_at_height: 0,
                reason: "operator alert test".into(),
                completed: false,
            })
            .unwrap();

        assert!(matches!(
            node.handle_rpc_request(RpcRequest::GetBlock { height: 99 }),
            RpcResponse::Error(_)
        ));

        let response = node.handle_rpc_request(RpcRequest::GetOperatorAlerts);
        let RpcResponse::Ok(RpcResult::OperatorAlerts(report)) = response else {
            panic!("expected operator alert response, got {response:?}");
        };
        let codes: Vec<_> = report
            .alerts
            .iter()
            .map(|alert| alert.code.as_str())
            .collect();
        assert_eq!(
            codes,
            vec![
                "operator.peer_isolation",
                "operator.stalled_consensus",
                "operator.root_mismatch",
                "operator.excessive_reverts",
                "operator.slashing_evidence",
                "operator.disk_pressure",
                "operator.rpc_overload",
                "operator.da_missing_shares",
                "operator.da_repair_pending",
                "operator.da_custody_failure",
                "operator.da_repair_lag",
                "operator.da_challenge_failure",
            ]
        );
        assert!(report.root_mismatch);
        assert_eq!(report.slashing_record_count, 2);
        assert_eq!(report.latest_block_failure_count, 1);
        assert_eq!(report.latest_block_receipt_count, 1);
        assert_eq!(report.metrics.consensus_height, 1);
        assert_eq!(report.metrics.highest_finalized_height, None);
        assert_eq!(report.metrics.finality_lag, None);
        assert_eq!(report.metrics.peer_count, Some(0));
        assert_eq!(report.metrics.rpc_error_count, 1);
        assert_eq!(report.metrics.da_manifest_count, 1);
        assert_eq!(
            report.metrics.da_missing_share_count,
            da_share_set.manifest.encoded_share_count as u64
        );
        assert_eq!(report.metrics.da_payload_count, 0);
        assert_eq!(report.metrics.da_challenge_record_count, 1);
        assert_eq!(report.metrics.da_challenge_evidence_count, 1);
        assert_eq!(report.metrics.da_custody_failure_count, 1);
        assert_eq!(report.metrics.da_repair_record_count, 1);
        assert_eq!(report.metrics.da_pending_repair_record_count, 1);
        assert_eq!(report.metrics.da_oldest_pending_repair_age_blocks, Some(1));
        assert!(report.metrics.da_total_bytes > 0);

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
        let RpcResponse::Ok(RpcResult::MempoolStatus(status)) =
            restarted.handle_rpc_request(RpcRequest::GetMempoolStatus)
        else {
            panic!("expected mempool status response");
        };
        assert_eq!(status.pending_transactions, 1);
        assert_eq!(status.pending_by_sender.get("Alice").copied(), Some(1));
        assert_eq!(status.max_pending, DEFAULT_MEMPOOL_MAX_PENDING);
        assert_eq!(
            status.max_pending_per_sender,
            DEFAULT_MEMPOOL_MAX_PENDING_PER_SENDER
        );
        assert_eq!(
            status.max_transaction_bytes,
            DEFAULT_MEMPOOL_MAX_TRANSACTION_BYTES
        );
        assert_eq!(status.block_resource_limit, DEFAULT_BLOCK_RESOURCE_LIMIT);

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
    fn persistent_validator_refuses_conflicting_vote_after_restart() {
        let dir = temp_dir("consensus-signing-vote-lock");
        let key = validator_key("validator-1", 7);
        let node = PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        let vote_a = Vote {
            validator_id: "validator-1".into(),
            height: 7,
            block_hash: "block-a".into(),
        };
        let vote_b = Vote {
            block_hash: "block-b".into(),
            ..vote_a.clone()
        };

        assert!(matches!(
            node.sign_validator_message(&key, NetworkMessage::Vote(vote_a.clone()))
                .unwrap(),
            NetworkMessage::SignedValidator(_)
        ));
        assert_eq!(
            node.storage
                .maybe_load_consensus_signing_record(
                    "validator-1",
                    ValidatorSignatureDomain::Vote,
                    7,
                )
                .unwrap()
                .unwrap()
                .block_hash,
            "block-a"
        );

        let restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        assert!(matches!(
            restarted
                .sign_validator_message(&key, NetworkMessage::Vote(vote_a))
                .unwrap(),
            NetworkMessage::SignedValidator(_)
        ));
        assert_eq!(
            restarted
                .sign_validator_message(&key, NetworkMessage::Vote(vote_b))
                .unwrap_err(),
            NodeError::ConsensusSigningConflict {
                validator_id: "validator-1".into(),
                domain: ValidatorSignatureDomain::Vote,
                height: 7,
                existing_block_hash: "block-a".into(),
                attempted_block_hash: "block-b".into(),
            }
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_node_collects_signed_da_availability_certificate() {
        let proposer_dir = temp_dir("signed-da-vote-proposer");
        let peer_dir = temp_dir("signed-da-vote-peer");
        let proposer_key = validator_key("validator-1", 7);
        let peer_key = validator_key("validator-2", 8);
        let mut proposer = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-1",
            seeded_state(),
            &proposer_dir,
            "detta-testnet",
            vec![proposer_key.public_key(), peer_key.public_key()],
        )
        .unwrap();
        let peer = PersistentValidatorNode::bootstrap_with_validator_set(
            "validator-2",
            seeded_state(),
            &peer_dir,
            "detta-testnet",
            vec![proposer_key.public_key(), peer_key.public_key()],
        )
        .unwrap();
        let block = proposer
            .produce_block_with_data_availability(1, 1_000, 64)
            .unwrap();
        let manifest_hash = block
            .header
            .data_availability
            .as_ref()
            .unwrap()
            .manifest_hash
            .clone();
        let manifest = proposer.load_da_manifest(&manifest_hash).unwrap();
        let share_set = proposer.load_da_share_set(&manifest_hash).unwrap();
        let signed_votes = vec![
            proposer
                .sign_da_availability_vote(&proposer_key, &manifest, &share_set.shares, 2)
                .unwrap(),
            peer.sign_da_availability_vote(&peer_key, &manifest, &share_set.shares, 2)
                .unwrap(),
        ];

        let certificate = proposer
            .collect_da_availability_certificate(&manifest, &signed_votes, 2)
            .unwrap();

        assert_eq!(certificate.signers, vec!["validator-1", "validator-2"]);
        assert_eq!(certificate.manifest_hash, manifest_hash);
        assert!(matches!(
            peer.sign_da_availability_vote(&peer_key, &manifest, &[], 2),
            Err(NodeError::DataAvailability(DaError::MissingShare { .. }))
        ));

        fs::remove_dir_all(proposer_dir).unwrap();
        fs::remove_dir_all(peer_dir).unwrap();
    }

    #[test]
    fn persistent_validator_refuses_conflicting_da_vote_after_restart() {
        let dir = temp_dir("da-signing-vote-lock");
        let key = validator_key("validator-1", 7);
        let node = PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        let vote_a = DaAvailabilityVote {
            chain_id: "detta-local".into(),
            height: 7,
            block_hash: "block-a".into(),
            manifest_hash: "manifest-a".into(),
            share_root: "share-root-a".into(),
            validator_id: "validator-1".into(),
            custody_share_indices: Vec::new(),
            sampled_share_indices: Vec::new(),
        };
        let vote_b = DaAvailabilityVote {
            manifest_hash: "manifest-b".into(),
            ..vote_a.clone()
        };

        assert!(matches!(
            node.sign_validator_message(&key, NetworkMessage::DaAvailabilityVote(vote_a.clone()))
                .unwrap(),
            NetworkMessage::SignedValidator(_)
        ));
        assert_eq!(
            node.storage
                .maybe_load_consensus_signing_record(
                    "validator-1",
                    ValidatorSignatureDomain::DaAvailabilityVote,
                    7,
                )
                .unwrap()
                .unwrap()
                .block_hash,
            "block-a:manifest-a"
        );

        let restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        assert!(matches!(
            restarted
                .sign_validator_message(&key, NetworkMessage::DaAvailabilityVote(vote_a))
                .unwrap(),
            NetworkMessage::SignedValidator(_)
        ));
        assert_eq!(
            restarted
                .sign_validator_message(&key, NetworkMessage::DaAvailabilityVote(vote_b))
                .unwrap_err(),
            NodeError::ConsensusSigningConflict {
                validator_id: "validator-1".into(),
                domain: ValidatorSignatureDomain::DaAvailabilityVote,
                height: 7,
                existing_block_hash: "block-a:manifest-a".into(),
                attempted_block_hash: "block-a:manifest-b".into(),
            }
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_validator_refuses_conflicting_block_proposal_after_partition_restart() {
        let dir = temp_dir("consensus-signing-block-lock");
        let key = validator_key("validator-1", 7);
        let state = seeded_state();
        let (block_a, _) =
            state.build_block(1, vec![transfer_tx()], 1_000, "validator-1", "cert-a");
        let (block_b, _) =
            state.build_block(1, vec![transfer_tx()], 2_000, "validator-1", "cert-b");
        let block_a_hash = block_a.block_hash();
        let block_b_hash = block_b.block_hash();
        assert_ne!(block_a_hash, block_b_hash);
        let node = PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();

        assert!(matches!(
            node.sign_validator_message(&key, NetworkMessage::Block(Box::new(block_a)))
                .unwrap(),
            NetworkMessage::SignedValidator(_)
        ));

        let restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        assert_eq!(
            restarted
                .sign_validator_message(&key, NetworkMessage::Block(Box::new(block_b)))
                .unwrap_err(),
            NodeError::ConsensusSigningConflict {
                validator_id: "validator-1".into(),
                domain: ValidatorSignatureDomain::BlockProposal,
                height: 1,
                existing_block_hash: block_a_hash,
                attempted_block_hash: block_b_hash,
            }
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persistent_validator_rejects_mismatched_consensus_message_signer() {
        let dir = temp_dir("consensus-signing-signer-mismatch");
        let key = validator_key("validator-1", 7);
        let node = PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        let vote = Vote {
            validator_id: "validator-2".into(),
            height: 7,
            block_hash: "block-a".into(),
        };

        assert_eq!(
            node.sign_validator_message(&key, NetworkMessage::Vote(vote))
                .unwrap_err(),
            NodeError::ConsensusMessageSignerMismatch {
                expected: "validator-1".into(),
                actual: "validator-2".into(),
            }
        );
        assert_eq!(
            node.storage
                .maybe_load_consensus_signing_record(
                    "validator-1",
                    ValidatorSignatureDomain::Vote,
                    7,
                )
                .unwrap(),
            None
        );

        fs::remove_dir_all(dir).unwrap();
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
        let subscription = match proposer.handle_rpc_request(RpcRequest::Subscribe {
            topics: vec![SubscriptionTopic::Finality],
        }) {
            RpcResponse::Ok(RpcResult::SubscriptionStatus(subscription)) => subscription,
            response => panic!("expected finality subscription, got {response:?}"),
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
        let page = match proposer.handle_rpc_request(RpcRequest::GetSubscriptionEvents {
            subscription_id: subscription.subscription_id.clone(),
            from_sequence: subscription.next_sequence,
            limit: 10,
        }) {
            RpcResponse::Ok(RpcResult::SubscriptionEvents(page)) => page,
            response => panic!("expected finality subscription page, got {response:?}"),
        };
        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].topic, SubscriptionTopic::Finality);
        assert!(matches!(
            &page.events[0].notification,
            SubscriptionNotification::FinalityCertificate(observed) if **observed == certificate
        ));

        assert_eq!(
            proposer.load_finality_certificate(3).unwrap(),
            certificate.clone()
        );
        let mut reloaded = PersistentValidatorNode::restart("validator-1", &proposer_dir).unwrap();
        assert_eq!(
            reloaded.load_finality_certificate(3).unwrap(),
            certificate.clone()
        );
        assert_eq!(
            reloaded.handle_rpc_request(RpcRequest::GetFinalityCertificate { height: 3 }),
            RpcResponse::Ok(RpcResult::FinalityCertificate(Box::new(
                certificate.clone()
            )))
        );
        assert_eq!(
            reloaded.handle_rpc_request(RpcRequest::GetFinalityCertificate { height: 4 }),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.certificate_not_found".into(),
                message: "finality certificate was not found".into(),
            })
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
            SlashingEvidence::Equivocation(evidence.clone())
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
                evidence: SlashingEvidence::Equivocation(EquivocationEvidence {
                    validator_id: "validator-1".into(),
                    height: 11,
                    first_block_hash: "block-a".into(),
                    second_block_hash: "block-b".into(),
                }),
            }
        );

        let mut reloaded_peer = PersistentValidatorNode::restart("validator-3", &peer_dir).unwrap();
        assert_eq!(
            reloaded_peer
                .load_slashing_record("validator-1")
                .unwrap()
                .slashed_at_height,
            11
        );
        assert_eq!(
            reloaded_peer.handle_rpc_request(RpcRequest::GetSlashingRecord {
                validator_id: "validator-1".into(),
            }),
            RpcResponse::Ok(RpcResult::SlashingRecord(Box::new(SlashingRecord {
                validator_id: "validator-1".into(),
                slashed_at_height: 11,
                evidence: SlashingEvidence::Equivocation(EquivocationEvidence {
                    validator_id: "validator-1".into(),
                    height: 11,
                    first_block_hash: "block-a".into(),
                    second_block_hash: "block-b".into(),
                }),
            })))
        );
        assert_eq!(
            reloaded_peer.handle_rpc_request(RpcRequest::GetSlashingRecord {
                validator_id: "validator-9".into(),
            }),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.slashing_record_not_found".into(),
                message: "slashing record was not found".into(),
            })
        );

        fs::remove_dir_all(reporter_dir).unwrap();
        fs::remove_dir_all(peer_dir).unwrap();
    }

    #[test]
    fn node_ingests_signed_da_challenge_flow_and_persists_slashing_evidence() {
        let peer_dir = temp_dir("signed-da-challenge-peer");
        let challenged_key = validator_key("validator-1", 7);
        let reporter_key = validator_key("validator-2", 8);
        let mut peer =
            PersistentValidatorNode::bootstrap("validator-3", seeded_state(), &peer_dir).unwrap();
        peer.set_network_id("detta-testnet");
        peer.trust_validator_key(challenged_key.public_key());
        peer.trust_validator_key(reporter_key.public_key());

        let payload = DaPayload::new(
            "detta-local",
            7,
            "prev-block",
            vec![DaNamespaceSection::new(
                DaNamespace::new("detta.tx").unwrap(),
                vec![DaRecord::SignedTransaction(transfer_tx())],
            )
            .unwrap()],
        )
        .unwrap();
        let share_set = DaShareSet::from_payload(&payload, "block-7", 64).unwrap();
        let vote = DaAvailabilityVote::from_manifest_with_custody(
            &share_set.manifest,
            "validator-1",
            [0],
            [],
        )
        .unwrap();
        let challenge =
            DaShareChallenge::from_availability_vote(&vote, "validator-2", 0, 11).unwrap();
        let challenge_id = challenge.challenge_hash().unwrap();
        let mut invalid_share = share_set.shares[0].clone();
        invalid_share.bytes[0] ^= 0x01;
        let response = DaShareChallengeResponse::from_share(&challenge, invalid_share).unwrap();
        let evidence = DaChallengeEvidence::invalid_response(
            &challenge,
            &response,
            &share_set.manifest,
            "validator-2",
            10,
        )
        .unwrap();

        let signed_challenge = reporter_key
            .sign_message(
                "detta-testnet",
                "detta-local",
                NetworkMessage::DaShareChallenge(challenge.clone()),
            )
            .unwrap();
        assert_eq!(
            peer.ingest_network_envelope(&Envelope {
                from: "validator-2".into(),
                to: "validator-3".into(),
                message: NetworkMessage::SignedValidator(Box::new(signed_challenge)),
            })
            .unwrap(),
            NetworkIngestOutcome::DataAvailabilityStored
        );

        let signed_response = challenged_key
            .sign_message(
                "detta-testnet",
                "detta-local",
                NetworkMessage::DaShareChallengeResponse(response.clone()),
            )
            .unwrap();
        assert_eq!(
            peer.ingest_network_envelope(&Envelope {
                from: "validator-1".into(),
                to: "validator-3".into(),
                message: NetworkMessage::SignedValidator(Box::new(signed_response)),
            })
            .unwrap(),
            NetworkIngestOutcome::DataAvailabilityStored
        );

        let signed_evidence = reporter_key
            .sign_message(
                "detta-testnet",
                "detta-local",
                NetworkMessage::DaChallengeEvidence(Box::new(evidence.clone())),
            )
            .unwrap();
        assert_eq!(
            peer.ingest_network_envelope(&Envelope {
                from: "validator-2".into(),
                to: "validator-3".into(),
                message: NetworkMessage::SignedValidator(Box::new(signed_evidence)),
            })
            .unwrap(),
            NetworkIngestOutcome::DataAvailabilityStored
        );

        let record = peer.load_da_challenge_record(&challenge_id).unwrap();
        assert_eq!(record.challenge, challenge);
        assert_eq!(record.response, Some(response));
        assert_eq!(record.evidence, Some(evidence.clone()));
        assert_eq!(
            peer.load_slashing_record("validator-1").unwrap(),
            SlashingRecord {
                validator_id: "validator-1".into(),
                slashed_at_height: 10,
                evidence: SlashingEvidence::DataAvailability(evidence.clone()),
            }
        );

        let mut restarted = PersistentValidatorNode::restart("validator-3", &peer_dir).unwrap();
        assert_eq!(
            restarted.handle_rpc_request(RpcRequest::GetDaChallengeRecord {
                challenge_id: challenge_id.clone(),
            }),
            RpcResponse::Ok(RpcResult::DaChallengeRecord(Box::new(record)))
        );
        assert_eq!(
            restarted.handle_rpc_request(RpcRequest::GetDaChallengeRecord {
                challenge_id: "missing-challenge".into(),
            }),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.da_challenge_record_not_found".into(),
                message: "DA challenge record was not found".into(),
            })
        );

        fs::remove_dir_all(peer_dir).unwrap();
    }

    #[test]
    fn node_da_slashing_policy_rejects_disabled_invalid_response_fault() {
        let peer_dir = temp_dir("da-slashing-policy-peer");
        let peer = PersistentValidatorNode::bootstrap(
            "validator-3",
            state_rejecting_invalid_da_slashing_evidence(),
            &peer_dir,
        )
        .unwrap();
        let payload = DaPayload::new(
            "detta-local",
            7,
            "prev-block",
            vec![DaNamespaceSection::new(
                DaNamespace::new("detta.tx").unwrap(),
                vec![DaRecord::SignedTransaction(transfer_tx())],
            )
            .unwrap()],
        )
        .unwrap();
        let share_set = DaShareSet::from_payload(&payload, "block-7", 64).unwrap();
        let vote = DaAvailabilityVote::from_manifest_with_custody(
            &share_set.manifest,
            "validator-1",
            [0],
            [],
        )
        .unwrap();
        let challenge =
            DaShareChallenge::from_availability_vote(&vote, "validator-2", 0, 11).unwrap();
        let mut invalid_share = share_set.shares[0].clone();
        invalid_share.bytes[0] ^= 0x01;
        let response = DaShareChallengeResponse::from_share(&challenge, invalid_share).unwrap();
        let evidence = DaChallengeEvidence::invalid_response(
            &challenge,
            &response,
            &share_set.manifest,
            "validator-2",
            10,
        )
        .unwrap();

        assert!(matches!(
            peer.persist_da_challenge_evidence(evidence),
            Err(NodeError::DataAvailability(DaError::InvalidChallenge(_)))
        ));
        assert_eq!(
            peer.storage
                .maybe_load_slashing_record("validator-1")
                .unwrap(),
            None
        );

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
            RpcResponse::Ok(RpcResult::SnapshotMetadataRootStatus(Box::new(
                SnapshotMetadataRootStatus {
                    validator_set_metadata_audit_root: retained_root.clone(),
                    local_validator_set_metadata_audit_root: retained_root.clone(),
                    persisted_validator_set_metadata_audit_root: Some(retained_root.clone()),
                    using_imported_validator_set_metadata_audit_root: false,
                    persisted_matches_local_validator_set_metadata_audit_root: true,
                    snapshot_import_audit_config_root: None,
                    local_snapshot_import_audit_config_root: None,
                    persisted_snapshot_import_audit_config_root: None,
                    using_imported_snapshot_import_audit_config_root: false,
                    persisted_matches_local_snapshot_import_audit_config_root: false,
                    snapshot_import_audit_root: None,
                    local_snapshot_import_audit_root: None,
                    persisted_snapshot_import_audit_root: None,
                    using_imported_snapshot_import_audit_root: false,
                    persisted_matches_local_snapshot_import_audit_root: false,
                    required_snapshot_metadata_roots_root: None,
                    local_required_snapshot_metadata_roots_root: None,
                    persisted_required_snapshot_metadata_roots_root: None,
                    using_imported_required_snapshot_metadata_roots_root: false,
                    persisted_matches_local_required_snapshot_metadata_roots_root: false,
                    snapshot_sync_client_metrics_root: None,
                    local_snapshot_sync_client_metrics_root: None,
                    persisted_snapshot_sync_client_metrics_root: None,
                    using_imported_snapshot_sync_client_metrics_root: false,
                    persisted_matches_local_snapshot_sync_client_metrics_root: false,
                },
            )))
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
        let expected_sync_metrics_root =
            FileStorage::snapshot_sync_client_metrics_root_for(&sync_metrics).unwrap();
        let expected_sync_report = snapshot_sync_client_metrics_report(sync_metrics.clone());
        let snapshot_import_audit_record = SnapshotImportAuditRecord {
            snapshot_root: "snapshot-root-1".into(),
            manifest_hash: "manifest-hash-1".into(),
            required_metadata_roots_root: expected_required_metadata_roots_root.clone(),
            required_metadata_roots_count: expected_required_metadata_roots.len(),
            manifest_metadata_roots_count: expected_required_metadata_roots.len() + 1,
            chunk_count: 4,
            metadata_roots_verified: true,
            da_manifest_hash: None,
            da_certificate_hash: None,
        };
        let expected_snapshot_import_audit_records = vec![snapshot_import_audit_record.clone()];
        let expected_snapshot_import_audit_root =
            FileStorage::snapshot_import_audit_root_for(&expected_snapshot_import_audit_records)
                .unwrap();
        let expected_snapshot_import_audit_config = SnapshotImportAuditConfig {
            max_records: 7,
            max_page_size: 3,
        };
        let expected_snapshot_import_audit_config_root =
            FileStorage::snapshot_import_audit_config_root_for(
                &expected_snapshot_import_audit_config,
            )
            .unwrap();
        let server_snapshot_import_audit_config = expected_snapshot_import_audit_config.clone();
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
            node.set_snapshot_import_audit_limits(
                server_snapshot_import_audit_config.max_records,
                server_snapshot_import_audit_config.max_page_size,
            )
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
        assert_eq!(initial_snapshot.snapshot_import_audit_record_count, 1);
        assert_eq!(
            initial_snapshot.snapshot_import_audit_max_records,
            expected_snapshot_import_audit_config.max_records
        );
        assert_eq!(
            initial_snapshot.snapshot_import_audit_max_page_size,
            expected_snapshot_import_audit_config.max_page_size
        );
        assert_eq!(
            initial_snapshot.snapshot_import_audit_config_root,
            Some(expected_snapshot_import_audit_config_root.clone())
        );
        write_rpc_request(&mut stream, &RpcRequest::GetSnapshotMetadataRootStatus);
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::SnapshotMetadataRootStatus(Box::new(
                SnapshotMetadataRootStatus {
                    validator_set_metadata_audit_root: initial_snapshot
                        .validator_set_metadata_audit_root
                        .clone(),
                    local_validator_set_metadata_audit_root: initial_snapshot
                        .validator_set_metadata_audit_root
                        .clone(),
                    persisted_validator_set_metadata_audit_root: None,
                    using_imported_validator_set_metadata_audit_root: false,
                    persisted_matches_local_validator_set_metadata_audit_root: false,
                    snapshot_import_audit_config_root: Some(
                        expected_snapshot_import_audit_config_root.clone(),
                    ),
                    local_snapshot_import_audit_config_root: Some(
                        expected_snapshot_import_audit_config_root.clone(),
                    ),
                    persisted_snapshot_import_audit_config_root: None,
                    using_imported_snapshot_import_audit_config_root: false,
                    persisted_matches_local_snapshot_import_audit_config_root: false,
                    snapshot_import_audit_root: Some(expected_snapshot_import_audit_root.clone()),
                    local_snapshot_import_audit_root: Some(
                        expected_snapshot_import_audit_root.clone(),
                    ),
                    persisted_snapshot_import_audit_root: None,
                    using_imported_snapshot_import_audit_root: false,
                    persisted_matches_local_snapshot_import_audit_root: false,
                    required_snapshot_metadata_roots_root: Some(
                        expected_required_metadata_roots_root.clone(),
                    ),
                    local_required_snapshot_metadata_roots_root: Some(
                        expected_required_metadata_roots_root.clone(),
                    ),
                    persisted_required_snapshot_metadata_roots_root: None,
                    using_imported_required_snapshot_metadata_roots_root: false,
                    persisted_matches_local_required_snapshot_metadata_roots_root: false,
                    snapshot_sync_client_metrics_root: Some(expected_sync_metrics_root.clone()),
                    local_snapshot_sync_client_metrics_root: Some(
                        expected_sync_metrics_root.clone(),
                    ),
                    persisted_snapshot_sync_client_metrics_root: None,
                    using_imported_snapshot_sync_client_metrics_root: false,
                    persisted_matches_local_snapshot_sync_client_metrics_root: false,
                },
            )))
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
        write_rpc_request(&mut stream, &RpcRequest::GetSnapshotImportAuditConfigRoot);
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::SnapshotImportAuditConfigRoot(Some(
                expected_snapshot_import_audit_config_root
            )))
        );
        write_rpc_request(&mut stream, &RpcRequest::GetSnapshotImportAuditConfig);
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::SnapshotImportAuditConfig(
                expected_snapshot_import_audit_config
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
    fn persistent_node_json_rpc_tcp_serves_upgrade_rehearsal_reports() {
        let dir = temp_dir("upgrade-rehearsal-json-rpc");
        let mut state = seeded_state();
        state
            .deploy_governance_with_timelock("GovA", "TokenA", "Admin", 2)
            .unwrap();
        let old_code_hash = state.code_hash("TokenA").unwrap().to_string();
        let schedule = state.apply_transaction(Transaction {
            chain_id: "detta-local".into(),
            tx_hash: "tx-schedule-upgrade".into(),
            sender: "Admin".into(),
            nonce: 1,
            valid_until_height: None,
            target: "GovA".into(),
            method: Method::ScheduleUpgrade,
            args: vec![
                Argument::Text("upgrade-1".into()),
                Argument::Text("token-code-v2".into()),
            ],
            signature_ok: true,
            budget: 1_000_000,
        });
        assert_eq!(schedule.status, TxStatus::Committed);
        let policy_schedule = state.apply_transaction(Transaction {
            chain_id: "detta-local".into(),
            tx_hash: "tx-schedule-policy".into(),
            sender: "Admin".into(),
            nonce: 2,
            valid_until_height: None,
            target: "GovA".into(),
            method: Method::SchedulePolicyUpdate,
            args: vec![
                Argument::Text("policy-update-1".into()),
                Argument::Text("transfer".into()),
                Argument::Text("registryWrite".into()),
            ],
            signature_ok: true,
            budget: 1_000_000,
        });
        assert_eq!(policy_schedule.status, TxStatus::Committed);
        let expected_scheduled_upgrades = vec![ScheduledUpgrade {
            upgrade_id: "upgrade-1".into(),
            governance_contract: "GovA".into(),
            target_contract: "TokenA".into(),
            new_code_hash: "token-code-v2".into(),
            execute_after_height: 2,
            executed: false,
        }];
        let expected_scheduled_policy_updates = vec![ScheduledPolicyUpdate {
            update_id: "policy-update-1".into(),
            governance_contract: "GovA".into(),
            target_contract: "TokenA".into(),
            method: Method::Transfer,
            effect: PolicyEffect::RegistryWrite,
            execute_after_height: 2,
            executed: false,
        }];
        let expected_report = state.rehearse_scheduled_upgrade("upgrade-1").unwrap();
        let expected_report_root = expected_report.report_root();
        let server = JsonRpcServer::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let mut node = PersistentValidatorNode::bootstrap("validator-1", state, &dir).unwrap();
            server
                .serve_next_connection_with_handler(&mut node)
                .unwrap();
            assert_eq!(
                node.rpc.node().state().code_hash("TokenA"),
                Some(old_code_hash.as_str())
            );
            fs::remove_dir_all(dir).unwrap();
        });

        let mut stream = TcpStream::connect(addr).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        write_rpc_request(&mut stream, &RpcRequest::GetScheduledUpgrades);
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::ScheduledUpgrades(expected_scheduled_upgrades))
        );
        write_rpc_request(&mut stream, &RpcRequest::GetScheduledPolicyUpdates);
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::ScheduledPolicyUpdates(
                expected_scheduled_policy_updates
            ))
        );
        write_rpc_request(
            &mut stream,
            &RpcRequest::GetUpgradeRehearsalReport {
                upgrade_id: "upgrade-1".into(),
            },
        );

        let RpcResponse::Ok(RpcResult::UpgradeRehearsalReport(report)) =
            read_rpc_response(&mut reader)
        else {
            panic!("expected upgrade rehearsal report");
        };
        assert_eq!(*report, expected_report);
        assert_eq!(report.report_root(), expected_report_root);
        assert_eq!(report.report_root().len(), 64);

        stream.shutdown(Shutdown::Write).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn persistent_node_json_rpc_tcp_serves_durable_history_after_restart() {
        let dir = temp_dir("durable-history-json-rpc");
        let mut node =
            PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        node.submit_transaction(transfer_tx()).unwrap();
        let block = node.produce_block(1, 1_000).unwrap();
        let expected_transaction = block.transactions[0].clone();
        let expected_receipt = block.receipts[0].clone();
        let restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        let expected_receipt_proof = block.receipt_proof(0).unwrap();
        let expected_event_proof = restarted.rpc().node().state().event_proof(0).unwrap();
        let server = JsonRpcServer::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let mut node = restarted;
            server
                .serve_next_connection_with_handler(&mut node)
                .unwrap();
            fs::remove_dir_all(dir).unwrap();
        });

        let mut stream = TcpStream::connect(addr).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        write_rpc_request(&mut stream, &RpcRequest::GetBlock { height: 1 });
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::Block(Box::new(block.clone())))
        );
        write_rpc_request(
            &mut stream,
            &RpcRequest::GetBlocksPage {
                start_height: 1,
                limit: 10,
            },
        );
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::BlocksPage(BlockPage {
                blocks: vec![block],
                start_height: 1,
                limit: 10,
                highest_height: 1,
            }))
        );
        write_rpc_request(
            &mut stream,
            &RpcRequest::GetTransaction {
                tx_hash: "tx1".into(),
            },
        );
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::Transaction(Box::new(expected_transaction)))
        );
        write_rpc_request(
            &mut stream,
            &RpcRequest::GetReceipt {
                tx_hash: "tx1".into(),
            },
        );
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::Receipt(Box::new(expected_receipt)))
        );
        write_rpc_request(
            &mut stream,
            &RpcRequest::GetReceiptProof {
                height: 1,
                index: 0,
            },
        );
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::ReceiptProof(Box::new(expected_receipt_proof)))
        );
        write_rpc_request(&mut stream, &RpcRequest::GetEventProof { index: 0 });
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::EventProof(Box::new(expected_event_proof)))
        );

        stream.shutdown(Shutdown::Write).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn persistent_node_json_rpc_tcp_serves_finality_certificates_after_restart() {
        let dir = temp_dir("finality-certificate-json-rpc");
        let node = PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        let certificate = FinalityCertificate {
            height: 7,
            block_hash: "block-hash-7".into(),
            signers: vec![
                "validator-1".into(),
                "validator-2".into(),
                "validator-3".into(),
            ],
        };
        node.storage
            .commit_finality_certificate(&certificate)
            .unwrap();
        let restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        let expected_certificate = certificate.clone();
        let server = JsonRpcServer::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let mut node = restarted;
            server
                .serve_next_connection_with_handler(&mut node)
                .unwrap();
            fs::remove_dir_all(dir).unwrap();
        });

        let mut stream = TcpStream::connect(addr).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        write_rpc_request(
            &mut stream,
            &RpcRequest::GetFinalityCertificate { height: 7 },
        );
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::FinalityCertificate(Box::new(
                expected_certificate
            )))
        );
        write_rpc_request(
            &mut stream,
            &RpcRequest::GetFinalityCertificate { height: 8 },
        );
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.certificate_not_found".into(),
                message: "finality certificate was not found".into(),
            })
        );

        stream.shutdown(Shutdown::Write).unwrap();
        handle.join().unwrap();
    }

    #[test]
    fn persistent_node_json_rpc_tcp_serves_slashing_records_after_restart() {
        let dir = temp_dir("slashing-record-json-rpc");
        let node = PersistentValidatorNode::bootstrap("validator-1", seeded_state(), &dir).unwrap();
        let record = SlashingRecord {
            validator_id: "validator-1".into(),
            slashed_at_height: 11,
            evidence: SlashingEvidence::Equivocation(EquivocationEvidence {
                validator_id: "validator-1".into(),
                height: 11,
                first_block_hash: "block-a".into(),
                second_block_hash: "block-b".into(),
            }),
        };
        node.storage.commit_slashing_record(&record).unwrap();
        let restarted = PersistentValidatorNode::restart("validator-1", &dir).unwrap();
        let expected_record = record.clone();
        let server = JsonRpcServer::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let handle = thread::spawn(move || {
            let mut node = restarted;
            server
                .serve_next_connection_with_handler(&mut node)
                .unwrap();
            fs::remove_dir_all(dir).unwrap();
        });

        let mut stream = TcpStream::connect(addr).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        write_rpc_request(
            &mut stream,
            &RpcRequest::GetSlashingRecord {
                validator_id: "validator-1".into(),
            },
        );
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Ok(RpcResult::SlashingRecord(Box::new(expected_record)))
        );
        write_rpc_request(
            &mut stream,
            &RpcRequest::GetSlashingRecord {
                validator_id: "validator-9".into(),
            },
        );
        assert_eq!(
            read_rpc_response(&mut reader),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.slashing_record_not_found".into(),
                message: "slashing record was not found".into(),
            })
        );

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
        let required_metadata_roots_root =
            FileStorage::required_snapshot_metadata_roots_root_for(&required_metadata_roots)
                .unwrap();
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
        let sink_snapshot_import_audit_root = sink.snapshot_import_audit_root().unwrap();
        assert_eq!(
            sink.handle_rpc_request(RpcRequest::GetSnapshotMetadataRootStatus),
            RpcResponse::Ok(RpcResult::SnapshotMetadataRootStatus(Box::new(
                SnapshotMetadataRootStatus {
                    validator_set_metadata_audit_root: expected_audit_root.clone(),
                    local_validator_set_metadata_audit_root: sink_local_audit_root,
                    persisted_validator_set_metadata_audit_root: Some(expected_audit_root.clone()),
                    using_imported_validator_set_metadata_audit_root: true,
                    persisted_matches_local_validator_set_metadata_audit_root: false,
                    snapshot_import_audit_config_root: None,
                    local_snapshot_import_audit_config_root: None,
                    persisted_snapshot_import_audit_config_root: None,
                    using_imported_snapshot_import_audit_config_root: false,
                    persisted_matches_local_snapshot_import_audit_config_root: false,
                    snapshot_import_audit_root: Some(sink_snapshot_import_audit_root.clone()),
                    local_snapshot_import_audit_root: Some(sink_snapshot_import_audit_root),
                    persisted_snapshot_import_audit_root: None,
                    using_imported_snapshot_import_audit_root: false,
                    persisted_matches_local_snapshot_import_audit_root: false,
                    required_snapshot_metadata_roots_root: Some(
                        required_metadata_roots_root.clone(),
                    ),
                    local_required_snapshot_metadata_roots_root: Some(required_metadata_roots_root,),
                    persisted_required_snapshot_metadata_roots_root: None,
                    using_imported_required_snapshot_metadata_roots_root: false,
                    persisted_matches_local_required_snapshot_metadata_roots_root: false,
                    snapshot_sync_client_metrics_root: None,
                    local_snapshot_sync_client_metrics_root: None,
                    persisted_snapshot_sync_client_metrics_root: None,
                    using_imported_snapshot_sync_client_metrics_root: false,
                    persisted_matches_local_snapshot_sync_client_metrics_root: false,
                },
            )))
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
        assert_eq!(
            status.snapshot_sync_client_metrics_root,
            Some(expected_metrics_root.clone())
        );
        assert_eq!(
            status.local_snapshot_sync_client_metrics_root,
            Some(expected_metrics_root.clone())
        );
        assert_eq!(status.persisted_snapshot_sync_client_metrics_root, None);
        assert!(!status.using_imported_snapshot_sync_client_metrics_root);
        assert!(!status.persisted_matches_local_snapshot_sync_client_metrics_root);

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
        sink.set_snapshot_import_audit_limits(1, 1).unwrap();
        let expected_snapshot_import_audit_config = SnapshotImportAuditConfig {
            max_records: 1,
            max_page_size: 1,
        };
        let snapshot_import_audit_config_root = FileStorage::snapshot_import_audit_config_root_for(
            &expected_snapshot_import_audit_config,
        )
        .unwrap();
        assert_eq!(
            sink.handle_rpc_request(RpcRequest::GetSnapshotImportAuditConfig),
            RpcResponse::Ok(RpcResult::SnapshotImportAuditConfig(
                expected_snapshot_import_audit_config.clone()
            ))
        );
        assert_eq!(
            sink.handle_rpc_request(RpcRequest::GetSnapshotImportAuditConfigRoot),
            RpcResponse::Ok(RpcResult::SnapshotImportAuditConfigRoot(Some(
                snapshot_import_audit_config_root.clone()
            )))
        );
        let sink_metadata_status = sink.snapshot_metadata_root_status().unwrap();
        assert_eq!(
            sink_metadata_status.snapshot_import_audit_config_root,
            Some(snapshot_import_audit_config_root.clone())
        );
        assert_eq!(
            sink_metadata_status.local_snapshot_import_audit_config_root,
            Some(snapshot_import_audit_config_root.clone())
        );
        assert_eq!(
            sink_metadata_status.persisted_snapshot_import_audit_config_root,
            None
        );
        assert!(!sink_metadata_status.using_imported_snapshot_import_audit_config_root);
        assert!(!sink_metadata_status.persisted_matches_local_snapshot_import_audit_config_root);
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
            da_manifest_hash: None,
            da_certificate_hash: None,
        };
        assert_eq!(
            sink.load_snapshot_import_audit_records().unwrap(),
            vec![expected_import_audit_record.clone()]
        );
        let retained_import = sink
            .import_snapshot_chunk_set(&chunk_set, &required_metadata_roots)
            .unwrap();
        assert_eq!(retained_import.global_state_root, snapshot_root);
        assert_eq!(
            sink.load_snapshot_import_audit_records().unwrap(),
            vec![expected_import_audit_record.clone()]
        );
        assert_eq!(
            sink.load_snapshot_import_audit_records_page(0, 10).unwrap(),
            vec![expected_import_audit_record.clone()]
        );
        let snapshot_import_audit_root = sink.snapshot_import_audit_root().unwrap();
        let sink_import_audit_status = sink.snapshot_metadata_root_status().unwrap();
        assert_eq!(
            sink_import_audit_status.snapshot_import_audit_root,
            Some(snapshot_import_audit_root.clone())
        );
        assert_eq!(
            sink_import_audit_status.local_snapshot_import_audit_root,
            Some(snapshot_import_audit_root.clone())
        );
        assert_eq!(
            sink_import_audit_status.persisted_snapshot_import_audit_root,
            None
        );
        assert!(!sink_import_audit_status.using_imported_snapshot_import_audit_root);
        assert!(!sink_import_audit_status.persisted_matches_local_snapshot_import_audit_root);
        assert_eq!(
            sink_import_audit_status.required_snapshot_metadata_roots_root,
            Some(required_metadata_roots_root.clone())
        );
        assert_eq!(
            sink_import_audit_status.local_required_snapshot_metadata_roots_root,
            Some(required_metadata_roots_root.clone())
        );
        assert_eq!(
            sink_import_audit_status.persisted_required_snapshot_metadata_roots_root,
            None
        );
        assert!(!sink_import_audit_status.using_imported_required_snapshot_metadata_roots_root);
        assert!(
            !sink_import_audit_status.persisted_matches_local_required_snapshot_metadata_roots_root
        );
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
                actual: Some(expected_metrics_root.clone()),
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
        let sink_imported_metrics_status = sink.snapshot_metadata_root_status().unwrap();
        assert_eq!(
            sink_imported_metrics_status.snapshot_sync_client_metrics_root,
            Some(expected_metrics_root.clone())
        );
        assert_eq!(
            sink_imported_metrics_status.local_snapshot_sync_client_metrics_root,
            None
        );
        assert_eq!(
            sink_imported_metrics_status.persisted_snapshot_sync_client_metrics_root,
            Some(expected_metrics_root.clone())
        );
        assert!(sink_imported_metrics_status.using_imported_snapshot_sync_client_metrics_root);
        assert!(
            !sink_imported_metrics_status.persisted_matches_local_snapshot_sync_client_metrics_root
        );
        let source_metrics_root = expected_metrics_root.clone();
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
        let sink_local_metrics_status = sink.snapshot_metadata_root_status().unwrap();
        assert_eq!(
            sink_local_metrics_status.snapshot_sync_client_metrics_root,
            Some(expected_metrics_root.clone())
        );
        assert_eq!(
            sink_local_metrics_status.local_snapshot_sync_client_metrics_root,
            Some(expected_metrics_root.clone())
        );
        assert_eq!(
            sink_local_metrics_status.persisted_snapshot_sync_client_metrics_root,
            Some(source_metrics_root)
        );
        assert!(!sink_local_metrics_status.using_imported_snapshot_sync_client_metrics_root);
        assert!(
            !sink_local_metrics_status.persisted_matches_local_snapshot_sync_client_metrics_root
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
        assert_eq!(sink_roots.snapshot_import_audit_record_count, 1);
        assert_eq!(sink_roots.snapshot_import_audit_max_records, 1);
        assert_eq!(sink_roots.snapshot_import_audit_max_page_size, 1);
        assert_eq!(
            sink_roots.snapshot_import_audit_config_root,
            Some(snapshot_import_audit_config_root.clone())
        );
        let response = sink
            .serve_snapshot_chunk_request(
                &SnapshotChunkRequest {
                    snapshot_root: snapshot_root.clone(),
                    start_index: 0,
                    max_chunks: 1024,
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
                assert_eq!(
                    manifest
                        .metadata_roots
                        .get(SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_CONFIG_ROOT),
                    Some(&snapshot_import_audit_config_root)
                );
            }
            message => panic!("expected snapshot manifest, got {message:?}"),
        }
        let mut response_iter = response.into_iter();
        let downstream_manifest = match response_iter.next().unwrap() {
            NetworkMessage::SnapshotChunkManifest(manifest) => manifest,
            message => panic!("expected snapshot manifest, got {message:?}"),
        };
        let downstream_chunks = response_iter
            .map(|message| match message {
                NetworkMessage::SnapshotChunk(chunk) => chunk,
                message => panic!("expected snapshot chunk, got {message:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            downstream_chunks.len(),
            downstream_manifest.chunk_count as usize
        );
        let downstream_chunk_set = SnapshotChunkSet {
            manifest: downstream_manifest,
            chunks: downstream_chunks,
        };
        let mut downstream_required_metadata_roots = BTreeMap::new();
        downstream_required_metadata_roots.insert(
            SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_ROOT.into(),
            snapshot_import_audit_root.clone(),
        );
        downstream_required_metadata_roots.insert(
            SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_CONFIG_ROOT.into(),
            snapshot_import_audit_config_root.clone(),
        );
        let downstream_required_metadata_roots_root =
            FileStorage::required_snapshot_metadata_roots_root_for(
                &downstream_required_metadata_roots,
            )
            .unwrap();
        let downstream_dir = temp_dir("state-sync-downstream-import-audit");
        let downstream = PersistentValidatorNode::bootstrap(
            "validator-3",
            DeTTaState::new("detta-local"),
            &downstream_dir,
        )
        .unwrap();
        let downstream_import = downstream
            .import_snapshot_chunk_set(&downstream_chunk_set, &downstream_required_metadata_roots)
            .unwrap();
        assert_eq!(downstream_import.global_state_root, snapshot_root);
        let downstream_metadata_status = downstream.snapshot_metadata_root_status().unwrap();
        let downstream_local_import_audit_root = downstream.snapshot_import_audit_root().unwrap();
        assert_eq!(
            downstream_metadata_status.snapshot_import_audit_config_root,
            Some(snapshot_import_audit_config_root.clone())
        );
        assert_eq!(
            downstream_metadata_status.local_snapshot_import_audit_config_root,
            None
        );
        assert_eq!(
            downstream_metadata_status.persisted_snapshot_import_audit_config_root,
            Some(snapshot_import_audit_config_root.clone())
        );
        assert!(downstream_metadata_status.using_imported_snapshot_import_audit_config_root);
        assert!(
            !downstream_metadata_status.persisted_matches_local_snapshot_import_audit_config_root
        );
        assert_eq!(
            downstream_metadata_status.snapshot_import_audit_root,
            Some(downstream_local_import_audit_root.clone())
        );
        assert_eq!(
            downstream_metadata_status.local_snapshot_import_audit_root,
            Some(downstream_local_import_audit_root)
        );
        assert_eq!(
            downstream_metadata_status.persisted_snapshot_import_audit_root,
            Some(snapshot_import_audit_root.clone())
        );
        assert!(!downstream_metadata_status.using_imported_snapshot_import_audit_root);
        assert!(!downstream_metadata_status.persisted_matches_local_snapshot_import_audit_root);
        assert_eq!(
            downstream_metadata_status.required_snapshot_metadata_roots_root,
            Some(downstream_required_metadata_roots_root.clone())
        );
        assert_eq!(
            downstream_metadata_status.local_required_snapshot_metadata_roots_root,
            Some(downstream_required_metadata_roots_root)
        );
        assert_eq!(
            downstream_metadata_status.persisted_required_snapshot_metadata_roots_root,
            Some(required_metadata_roots_root.clone())
        );
        assert!(!downstream_metadata_status.using_imported_required_snapshot_metadata_roots_root);
        assert!(
            !downstream_metadata_status
                .persisted_matches_local_required_snapshot_metadata_roots_root
        );
        assert_eq!(
            downstream_metadata_status.snapshot_sync_client_metrics_root,
            Some(expected_metrics_root.clone())
        );
        assert_eq!(
            downstream_metadata_status.local_snapshot_sync_client_metrics_root,
            None
        );
        assert_eq!(
            downstream_metadata_status.persisted_snapshot_sync_client_metrics_root,
            Some(expected_metrics_root.clone())
        );
        assert!(downstream_metadata_status.using_imported_snapshot_sync_client_metrics_root);
        assert!(
            !downstream_metadata_status.persisted_matches_local_snapshot_sync_client_metrics_root
        );
        let mut wrong_downstream_roots = downstream_required_metadata_roots.clone();
        wrong_downstream_roots.insert(
            SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_ROOT.into(),
            "wrong-import-audit-root".into(),
        );
        assert_eq!(
            downstream
                .import_snapshot_chunk_set(&downstream_chunk_set, &wrong_downstream_roots)
                .unwrap_err(),
            NodeError::SnapshotSync(SnapshotSyncError::MetadataRootMismatch {
                key: SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_ROOT.into(),
                expected: "wrong-import-audit-root".into(),
                actual: Some(snapshot_import_audit_root.clone()),
            })
        );
        let mut wrong_downstream_config_roots = downstream_required_metadata_roots;
        wrong_downstream_config_roots.insert(
            SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_CONFIG_ROOT.into(),
            "wrong-import-audit-config-root".into(),
        );
        assert_eq!(
            downstream
                .import_snapshot_chunk_set(&downstream_chunk_set, &wrong_downstream_config_roots)
                .unwrap_err(),
            NodeError::SnapshotSync(SnapshotSyncError::MetadataRootMismatch {
                key: SNAPSHOT_METADATA_SNAPSHOT_IMPORT_AUDIT_CONFIG_ROOT.into(),
                expected: "wrong-import-audit-config-root".into(),
                actual: Some(snapshot_import_audit_config_root.clone()),
            })
        );
        fs::remove_dir_all(downstream_dir).unwrap();

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
        let restarted_roots = restarted_sink.node_snapshot_roots().unwrap();
        assert_eq!(restarted_roots.snapshot_import_audit_record_count, 1);
        assert_eq!(restarted_roots.snapshot_import_audit_max_records, 1);
        assert_eq!(restarted_roots.snapshot_import_audit_max_page_size, 1);
        assert_eq!(
            restarted_roots.snapshot_import_audit_config_root,
            Some(snapshot_import_audit_config_root)
        );
        assert_eq!(
            restarted_sink.handle_rpc_request(RpcRequest::GetSnapshotImportAuditConfig),
            RpcResponse::Ok(RpcResult::SnapshotImportAuditConfig(
                expected_snapshot_import_audit_config
            ))
        );
        assert_eq!(
            restarted_sink.handle_rpc_request(RpcRequest::GetSnapshotImportAuditConfigRoot),
            RpcResponse::Ok(RpcResult::SnapshotImportAuditConfigRoot(Some(
                restarted_roots
                    .snapshot_import_audit_config_root
                    .clone()
                    .unwrap()
            )))
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
        assert_eq!(state.balance("TokenA", "Bob", "USDC"), 50);
        assert_eq!(state.balance("TokenETH", "Bob", "ETH"), 4);
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
