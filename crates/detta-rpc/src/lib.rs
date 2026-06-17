use detta_consensus::{FinalityCertificate, SlashingRecord};
use detta_core::{
    Amount, AspectModuleProof, AspectModuleRecord, AssetId, Block, BlockError, ContractId,
    ContractRecord, DeTTaState, Event, EventProof, ExecutionError, GrantKey, MempoolError,
    OutboxMessageProof, Principal, Receipt, ReceiptProof, RegistryNonInclusionProof, RegistryProof,
    ScheduledPolicyUpdate, ScheduledUpgrade, StateKey, StateSnapshot, StorageNonInclusionProof,
    StorageProof, Transaction, UpgradeRehearsalReport, ValidatorNode,
};
use detta_da::{
    DaAvailabilityCertificate, DaChallengeRecord, DaManifest, DaNamespaceSection, DaPayload,
    DaSampleProofBundle, DaShare,
};
use detta_evaluator::{
    canonical_script_source, parse_restricted_script, restricted_evaluator_fixture_inventory,
    trace_root, EvaluatorError, EvaluatorFixtureInventory, ExecutionReport,
    RestrictedScriptEvaluator,
};
use detta_protocol::SignedValidatorMessage;
use detta_storage::{
    DaStorageStats, SnapshotImportAuditConfig, SnapshotImportAuditRecord,
    ValidatorSetMetadataAuditRecord,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, ToSocketAddrs};

pub const DEFAULT_MAX_RPC_REQUEST_BYTES: usize = 1024 * 1024;
pub const DEFAULT_MAX_HTTP_HEADER_BYTES: usize = 16 * 1024;
pub const DEFAULT_MAX_EVENT_PAGE_SIZE: usize = 1_000;
pub const DEFAULT_MAX_BLOCK_PAGE_SIZE: usize = 100;
pub const DEFAULT_MAX_SUBSCRIPTION_EVENT_PAGE_SIZE: usize = 1_000;
pub const DEFAULT_MAX_SUBSCRIPTION_EVENTS: usize = 10_000;
pub const DEFAULT_MAX_RESTRICTED_EVALUATOR_STEPS: u64 = 10_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RpcError {
    Mempool(MempoolError),
    Block(BlockError),
    BlockNotFound,
    CertificateNotFound,
    SlashingRecordNotFound,
    ReceiptNotFound,
    TransactionNotFound,
    ContractNotFound,
    AspectModuleNotFound,
    DaManifestNotFound,
    DaShareNotFound,
    DaCertificateNotFound,
    DaChallengeRecordNotFound,
    DaNamespaceNotFound,
    ProofNotFound,
    SubscriptionNotFound,
    Execution(ExecutionError),
    EvaluatorParse(String),
    EvaluatorExecution(EvaluatorError),
    EvaluatorStepBudgetTooLarge,
    UnsupportedNodeMethod,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignedClientTransaction {
    pub transaction: Transaction,
    pub public_key_hex: String,
    pub signature_hex: String,
}

#[derive(Serialize)]
struct TransactionSigningPayload<'a> {
    chain_id: &'a str,
    tx_hash: &'a str,
    sender: &'a str,
    nonce: u64,
    valid_until_height: Option<u64>,
    target: &'a str,
    method: &'a detta_core::Method,
    args: &'a [detta_core::Argument],
    budget: u64,
}

impl SignedClientTransaction {
    pub fn sign_with_seed(mut transaction: Transaction, seed: [u8; 32]) -> Self {
        transaction.signature_ok = false;
        let signing_key = SigningKey::from_bytes(&seed);
        let payload = Self::signing_payload(&transaction);
        let signature = signing_key.sign(&payload);
        Self {
            transaction,
            public_key_hex: hex_lower(&signing_key.verifying_key().to_bytes()),
            signature_hex: hex_lower(&signature.to_bytes()),
        }
    }

    pub fn verify(&self) -> bool {
        if self.public_key_hex != self.public_key_hex.to_ascii_lowercase()
            || self.signature_hex != self.signature_hex.to_ascii_lowercase()
        {
            return false;
        }
        let Some(public_key_bytes) = hex_to_array::<32>(&self.public_key_hex) else {
            return false;
        };
        let Some(signature_bytes) = hex_to_array::<64>(&self.signature_hex) else {
            return false;
        };
        let Ok(verifying_key) = VerifyingKey::from_bytes(&public_key_bytes) else {
            return false;
        };
        let signature = Signature::from_bytes(&signature_bytes);
        verifying_key
            .verify(&Self::signing_payload(&self.transaction), &signature)
            .is_ok()
    }

    pub fn into_verified_transaction(mut self) -> Result<Transaction, MempoolError> {
        if !self.verify() {
            return Err(MempoolError::InvalidSignature);
        }
        self.transaction.signature_ok = true;
        Ok(self.transaction)
    }

    pub fn into_authorized_transaction(
        mut self,
        state: &DeTTaState,
    ) -> Result<Transaction, MempoolError> {
        if !self.verify() {
            return Err(MempoolError::InvalidSignature);
        }
        if !state.has_active_account_key(&self.transaction.sender, &self.public_key_hex) {
            return Err(MempoolError::UnauthorizedSigner {
                sender: self.transaction.sender,
                public_key_hex: self.public_key_hex,
            });
        }
        self.transaction.signature_ok = true;
        Ok(self.transaction)
    }

    fn signing_payload(transaction: &Transaction) -> Vec<u8> {
        serde_json::to_vec(&TransactionSigningPayload {
            chain_id: &transaction.chain_id,
            tx_hash: &transaction.tx_hash,
            sender: &transaction.sender,
            nonce: transaction.nonce,
            valid_until_height: transaction.valid_until_height,
            target: &transaction.target,
            method: &transaction.method,
            args: &transaction.args,
            budget: transaction.budget,
        })
        .expect("transaction signing payload should serialize")
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn hex_to_array<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 {
        return None;
    }
    let mut out = [0u8; N];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(chunk[0])?;
        let low = hex_nibble(chunk[1])?;
        out[index] = (high << 4) | low;
    }
    Some(out)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum RpcRequest {
    SubmitTransaction {
        transaction: Transaction,
    },
    SubmitSignedTransaction {
        signed: SignedClientTransaction,
    },
    ProduceBlock {
        height: u64,
        timestamp: u64,
    },
    ImportBlock {
        block: Box<Block>,
    },
    GetTransaction {
        tx_hash: String,
    },
    GetReceipt {
        tx_hash: String,
    },
    GetReceiptProof {
        height: u64,
        index: usize,
    },
    GetBlock {
        height: u64,
    },
    GetBlocksPage {
        start_height: u64,
        limit: usize,
    },
    GetFinalityCertificate {
        height: u64,
    },
    GetSlashingRecord {
        validator_id: String,
    },
    GetDaManifest {
        manifest_hash: String,
    },
    GetDaShare {
        manifest_hash: String,
        index: u32,
    },
    GetDaCertificate {
        certificate_hash: String,
    },
    GetDaChallengeRecord {
        challenge_id: String,
    },
    GetDaPayload {
        manifest_hash: String,
    },
    GetDaNamespace {
        manifest_hash: String,
        namespace: String,
    },
    GetDaSampleProofs {
        manifest_hash: String,
        client_randomness: String,
        sample_count: u32,
        namespaces: Vec<String>,
    },
    GetDaStatus {
        manifest_hash: String,
    },
    GetDaRepairStatus {
        manifest_hash: String,
    },
    GetDaStorageStats,
    GetNodeHealth,
    GetOperatorMetrics,
    GetOperatorAlerts,
    GetMempoolStatus,
    GetStateRoot,
    GetSnapshot,
    GetPersistentNodeSnapshotRoots,
    GetSnapshotMetadataRootStatus,
    GetSnapshotSyncClientMetrics,
    GetRequiredSnapshotMetadataRoots,
    GetBalance {
        contract: ContractId,
        owner: Principal,
        asset: AssetId,
    },
    GetTotalSupply {
        contract: ContractId,
        asset: AssetId,
    },
    GetStorageProof {
        key: StateKey,
    },
    GetStorageNonInclusionProof {
        key: StateKey,
    },
    GetRegistryProof {
        key: GrantKey,
    },
    GetRegistryNonInclusionProof {
        key: GrantKey,
    },
    GetOutboxMessageProof {
        index: usize,
    },
    GetEventProof {
        index: usize,
    },
    GetEvents,
    GetEventsPage {
        offset: usize,
        limit: usize,
    },
    Subscribe {
        topics: Vec<SubscriptionTopic>,
    },
    GetSubscriptionEvents {
        subscription_id: String,
        from_sequence: u64,
        limit: usize,
    },
    GetContract {
        contract: ContractId,
    },
    GetAspectModule {
        module_hash: String,
    },
    GetAspectModuleProof {
        module_hash: String,
    },
    GetAspectModuleArtifacts {
        module_hash: String,
    },
    GetAspectModules,
    GetScheduledUpgrades,
    GetScheduledPolicyUpdates,
    GetUpgradeRehearsalReport {
        upgrade_id: String,
    },
    EvaluateRestrictedScript {
        source: String,
        max_steps: u64,
    },
    GetRestrictedEvaluatorFixtureInventory,
    ProposeValidatorSetMetadataUpdate {
        authorization: SignedValidatorMessage,
    },
    GetValidatorSetMetadataUpdateStatus {
        update_id: String,
    },
    GetValidatorSetMetadataAuditRecords {
        offset: usize,
        limit: usize,
    },
    GetSnapshotImportAuditRecords {
        offset: usize,
        limit: usize,
    },
    GetSnapshotImportAuditRoot,
    GetSnapshotImportAuditConfigRoot,
    GetSnapshotImportAuditConfig,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidatorSetMetadataUpdateStatus {
    pub update_id: String,
    pub pending_authorizations: usize,
    pub required_quorum: usize,
    pub applied: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PersistentNodeSnapshotRoots {
    pub storage_root: String,
    pub registry_root: String,
    pub policy_root: String,
    pub event_root: String,
    pub nonce_root: String,
    pub outbox_root: String,
    pub global_state_root: String,
    pub validator_set_metadata_audit_root: String,
    pub snapshot_import_audit_root: String,
    pub snapshot_import_audit_record_count: usize,
    pub snapshot_import_audit_max_records: usize,
    pub snapshot_import_audit_max_page_size: usize,
    pub snapshot_import_audit_config_root: Option<String>,
    pub required_snapshot_metadata_roots: BTreeMap<String, String>,
    pub required_snapshot_metadata_roots_root: String,
    pub snapshot_sync_client_metrics: Option<SnapshotSyncClientMetricsReport>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaStatusReport {
    pub manifest_hash: String,
    pub manifest_available: bool,
    pub certificate_hash: Option<String>,
    pub certificate_available: bool,
    pub expected_share_count: u32,
    pub stored_share_count: u32,
    pub missing_share_indices: Vec<u32>,
    pub payload_reconstructable: bool,
    pub payload_bytes: Option<u64>,
    pub namespace_count: Option<usize>,
    pub reconstruction_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaRepairStatusReport {
    pub manifest_hash: String,
    pub repair_needed: bool,
    pub missing_share_indices: Vec<u32>,
    pub pending_repair_count: usize,
    pub payload_reconstructable: bool,
    pub reconstruction_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RequiredSnapshotMetadataRootsReport {
    pub roots: BTreeMap<String, String>,
    pub root: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AspectModuleArtifactReport {
    pub module_hash: String,
    pub module_id: String,
    pub taxonomy_version: String,
    pub source_root: String,
    pub ir_root: String,
    pub abi_root: String,
    pub policy_root: String,
    pub storage_schema_root: String,
    pub registry_schema_root: String,
    pub invariant_root: String,
    pub has_canonical_source: bool,
    pub has_ir: bool,
    pub bundle_ids: Vec<String>,
    pub abi: BTreeMap<String, detta_aspects::MethodAbiDef>,
    pub policies: BTreeMap<String, detta_aspects::MethodPolicyIr>,
    pub storage_schema: BTreeMap<String, detta_aspects::StateSchema>,
    pub registry_schema: BTreeMap<String, detta_aspects::RegistrySchema>,
    pub invariants: BTreeMap<String, detta_aspects::InvariantDef>,
}

impl AspectModuleArtifactReport {
    fn from_record(module: &AspectModuleRecord) -> Self {
        let mut report = Self {
            module_hash: module.module_hash.clone(),
            module_id: module.module_id.clone(),
            taxonomy_version: module.taxonomy_version.clone(),
            source_root: module.source_root.clone(),
            ir_root: module.ir_root.clone(),
            abi_root: module.abi_root.clone(),
            policy_root: module.policy_root.clone(),
            storage_schema_root: module.storage_schema_root.clone(),
            registry_schema_root: module.registry_schema_root.clone(),
            invariant_root: module.invariant_root.clone(),
            has_canonical_source: module.canonical_source.is_some(),
            has_ir: module.ir.is_some(),
            bundle_ids: Vec::new(),
            abi: BTreeMap::new(),
            policies: BTreeMap::new(),
            storage_schema: BTreeMap::new(),
            registry_schema: BTreeMap::new(),
            invariants: BTreeMap::new(),
        };

        if let Some(ir) = &module.ir {
            report.bundle_ids = ir.bundles.keys().cloned().collect();
            report.abi = ir.abi.clone();
            report.policies = ir.policies.clone();
            report.storage_schema = ir.storage_schema.clone();
            report.registry_schema = ir.registry_schema.clone();
            report.invariants = ir.invariants.clone();
        }

        report
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotMetadataRootStatus {
    pub validator_set_metadata_audit_root: String,
    pub local_validator_set_metadata_audit_root: String,
    pub persisted_validator_set_metadata_audit_root: Option<String>,
    pub using_imported_validator_set_metadata_audit_root: bool,
    pub persisted_matches_local_validator_set_metadata_audit_root: bool,
    pub snapshot_import_audit_config_root: Option<String>,
    pub local_snapshot_import_audit_config_root: Option<String>,
    pub persisted_snapshot_import_audit_config_root: Option<String>,
    pub using_imported_snapshot_import_audit_config_root: bool,
    pub persisted_matches_local_snapshot_import_audit_config_root: bool,
    pub snapshot_import_audit_root: Option<String>,
    pub local_snapshot_import_audit_root: Option<String>,
    pub persisted_snapshot_import_audit_root: Option<String>,
    pub using_imported_snapshot_import_audit_root: bool,
    pub persisted_matches_local_snapshot_import_audit_root: bool,
    pub required_snapshot_metadata_roots_root: Option<String>,
    pub local_required_snapshot_metadata_roots_root: Option<String>,
    pub persisted_required_snapshot_metadata_roots_root: Option<String>,
    pub using_imported_required_snapshot_metadata_roots_root: bool,
    pub persisted_matches_local_required_snapshot_metadata_roots_root: bool,
    pub snapshot_sync_client_metrics_root: Option<String>,
    pub local_snapshot_sync_client_metrics_root: Option<String>,
    pub persisted_snapshot_sync_client_metrics_root: Option<String>,
    pub using_imported_snapshot_sync_client_metrics_root: bool,
    pub persisted_matches_local_snapshot_sync_client_metrics_root: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotSyncClientMetricsReport {
    pub retry_attempts: u32,
    pub stream_failures: u32,
    pub requests_sent: u32,
    pub manifests_received: u32,
    pub chunks_received: u32,
    pub resume_requests: u32,
    pub metadata_roots_verified: bool,
    pub required_metadata_roots_root: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct NodeHealthReport {
    pub chain_id: String,
    pub network_id: Option<String>,
    pub validator_id: Option<String>,
    pub height: u64,
    pub pending_mempool_transactions: usize,
    pub trusted_validator_keys: Option<usize>,
    pub pending_validator_set_metadata_updates: Option<usize>,
    pub storage_root: String,
    pub registry_root: String,
    pub policy_root: String,
    pub event_root: String,
    pub nonce_root: String,
    pub outbox_root: String,
    pub global_state_root: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperatorMetricsReport {
    pub network_id: Option<String>,
    pub validator_id: Option<String>,
    pub peer_count: Option<usize>,
    pub mempool_size: usize,
    pub consensus_height: u64,
    pub highest_finalized_height: Option<u64>,
    pub finality_lag: Option<u64>,
    pub last_block_execution_micros: Option<u64>,
    pub last_proof_serving_micros: Option<u64>,
    pub storage_bytes: Option<u64>,
    pub rpc_error_count: u64,
    pub da_manifest_count: u64,
    pub da_missing_share_count: u64,
    pub da_payload_count: u64,
    pub da_challenge_record_count: u64,
    pub da_challenge_evidence_count: u64,
    pub da_custody_failure_count: u64,
    pub da_repair_record_count: u64,
    pub da_pending_repair_record_count: u64,
    pub da_oldest_pending_repair_age_blocks: Option<u64>,
    pub da_total_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperatorAlertPolicy {
    pub min_peer_count: usize,
    pub max_finality_lag: u64,
    pub max_storage_bytes: u64,
    pub max_rpc_error_count: u64,
    pub max_mempool_size: usize,
    pub max_latest_block_failure_ratio_per_mille: u16,
    pub max_da_repair_lag_blocks: u64,
}

impl Default for OperatorAlertPolicy {
    fn default() -> Self {
        Self {
            min_peer_count: 1,
            max_finality_lag: 2,
            max_storage_bytes: u64::MAX,
            max_rpc_error_count: 100,
            max_mempool_size: 10_000,
            max_latest_block_failure_ratio_per_mille: 500,
            max_da_repair_lag_blocks: 32,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperatorAlertSeverity {
    Warning,
    Critical,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperatorAlert {
    pub code: String,
    pub severity: OperatorAlertSeverity,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OperatorAlertReport {
    pub policy: OperatorAlertPolicy,
    pub metrics: OperatorMetricsReport,
    pub alerts: Vec<OperatorAlert>,
    pub root_mismatch: bool,
    pub slashing_record_count: usize,
    pub latest_block_failure_count: usize,
    pub latest_block_receipt_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MempoolStatus {
    pub pending_transactions: usize,
    pub max_pending: usize,
    pub max_pending_per_sender: usize,
    pub max_transaction_bytes: usize,
    pub block_resource_limit: u64,
    pub pending_by_sender: BTreeMap<Principal, usize>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventPage {
    pub events: Vec<Event>,
    pub offset: usize,
    pub limit: usize,
    pub total_events: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlockPage {
    pub blocks: Vec<Block>,
    pub start_height: u64,
    pub limit: usize,
    pub highest_height: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionTopic {
    Blocks,
    Receipts,
    Events,
    Finality,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum SubscriptionNotification {
    Block(Box<Block>),
    Receipt(Box<Receipt>),
    Event(Box<Event>),
    FinalityCertificate(Box<FinalityCertificate>),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionEvent {
    pub sequence: u64,
    pub topic: SubscriptionTopic,
    pub notification: SubscriptionNotification,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionStatus {
    pub subscription_id: String,
    pub topics: Vec<SubscriptionTopic>,
    pub next_sequence: u64,
    pub earliest_retained_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SubscriptionEventPage {
    pub subscription_id: String,
    pub events: Vec<SubscriptionEvent>,
    pub from_sequence: u64,
    pub next_sequence: u64,
    pub earliest_retained_sequence: u64,
    pub limit: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RestrictedEvaluationReport {
    pub evaluator: String,
    pub source: String,
    pub canonical_source: String,
    pub max_steps: u64,
    pub report: ExecutionReport,
    pub trace_root: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "result", content = "data", rename_all = "snake_case")]
pub enum RpcResult {
    Submitted,
    Imported,
    Block(Box<Block>),
    BlocksPage(BlockPage),
    FinalityCertificate(Box<FinalityCertificate>),
    SlashingRecord(Box<SlashingRecord>),
    Transaction(Box<Transaction>),
    Receipt(Box<Receipt>),
    NodeHealth(Box<NodeHealthReport>),
    OperatorMetrics(Box<OperatorMetricsReport>),
    OperatorAlerts(Box<OperatorAlertReport>),
    MempoolStatus(MempoolStatus),
    StateRoot(String),
    Snapshot(Box<StateSnapshot>),
    PersistentNodeSnapshotRoots(Box<PersistentNodeSnapshotRoots>),
    SnapshotMetadataRootStatus(Box<SnapshotMetadataRootStatus>),
    SnapshotSyncClientMetrics(Option<SnapshotSyncClientMetricsReport>),
    RequiredSnapshotMetadataRoots(RequiredSnapshotMetadataRootsReport),
    Amount(Amount),
    StorageProof(Box<StorageProof>),
    StorageNonInclusionProof(Box<StorageNonInclusionProof>),
    RegistryProof(Box<RegistryProof>),
    RegistryNonInclusionProof(Box<RegistryNonInclusionProof>),
    OutboxMessageProof(Box<OutboxMessageProof>),
    ReceiptProof(Box<ReceiptProof>),
    EventProof(Box<EventProof>),
    Events(Vec<Event>),
    EventsPage(EventPage),
    SubscriptionStatus(SubscriptionStatus),
    SubscriptionEvents(SubscriptionEventPage),
    Contract(Box<ContractRecord>),
    AspectModule(Box<AspectModuleRecord>),
    AspectModuleProof(Box<AspectModuleProof>),
    AspectModuleArtifacts(Box<AspectModuleArtifactReport>),
    AspectModules(Vec<AspectModuleRecord>),
    ScheduledUpgrades(Vec<ScheduledUpgrade>),
    ScheduledPolicyUpdates(Vec<ScheduledPolicyUpdate>),
    UpgradeRehearsalReport(Box<UpgradeRehearsalReport>),
    RestrictedEvaluation(Box<RestrictedEvaluationReport>),
    RestrictedEvaluatorFixtureInventory(Box<EvaluatorFixtureInventory>),
    ValidatorSetMetadataUpdateStatus(ValidatorSetMetadataUpdateStatus),
    ValidatorSetMetadataAuditRecords(Vec<ValidatorSetMetadataAuditRecord>),
    SnapshotImportAuditRecords(Vec<SnapshotImportAuditRecord>),
    SnapshotImportAuditRoot(String),
    SnapshotImportAuditConfigRoot(Option<String>),
    SnapshotImportAuditConfig(SnapshotImportAuditConfig),
    DaManifest(Box<DaManifest>),
    DaShare(Box<DaShare>),
    DaAvailabilityCertificate(Box<DaAvailabilityCertificate>),
    DaChallengeRecord(Box<DaChallengeRecord>),
    DaPayload(Box<DaPayload>),
    DaNamespace(Box<DaNamespaceSection>),
    DaSampleProofs(Box<DaSampleProofBundle>),
    DaStatus(Box<DaStatusReport>),
    DaRepairStatus(Box<DaRepairStatusReport>),
    DaStorageStats(DaStorageStats),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", content = "body", rename_all = "snake_case")]
pub enum RpcResponse {
    Ok(RpcResult),
    Error(RpcErrorBody),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RpcErrorBody {
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AuthenticatedRpcRequest {
    pub bearer_token: String,
    pub request: RpcRequest,
}

#[derive(Debug)]
pub enum RpcTransportError {
    Io(io::Error),
    Encode(serde_json::Error),
    RequestTooLarge { max_bytes: usize },
}

impl From<io::Error> for RpcTransportError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub struct JsonRpcServer {
    listener: TcpListener,
    max_request_bytes: usize,
}

impl JsonRpcServer {
    pub fn bind(addr: impl ToSocketAddrs) -> Result<Self, RpcTransportError> {
        let listener = TcpListener::bind(addr)?;
        Ok(Self {
            listener,
            max_request_bytes: DEFAULT_MAX_RPC_REQUEST_BYTES,
        })
    }

    pub fn with_max_request_bytes(mut self, max_request_bytes: usize) -> Self {
        self.max_request_bytes = max_request_bytes;
        self
    }

    pub fn local_addr(&self) -> Result<SocketAddr, RpcTransportError> {
        self.listener.local_addr().map_err(RpcTransportError::Io)
    }

    pub fn serve_next_connection(&self, service: &mut RpcService) -> Result<(), RpcTransportError> {
        self.serve_next_connection_with_handler(service)
    }

    pub fn serve_next_connection_with_handler<H: JsonRpcHandler>(
        &self,
        handler: &mut H,
    ) -> Result<(), RpcTransportError> {
        let (stream, _) = self.listener.accept()?;
        serve_json_rpc_handler_connection(handler, stream, self.max_request_bytes)
    }
}

pub struct HttpJsonRpcServer {
    listener: TcpListener,
    max_request_bytes: usize,
}

impl HttpJsonRpcServer {
    pub fn bind(addr: impl ToSocketAddrs) -> Result<Self, RpcTransportError> {
        let listener = TcpListener::bind(addr)?;
        Ok(Self {
            listener,
            max_request_bytes: DEFAULT_MAX_RPC_REQUEST_BYTES,
        })
    }

    pub fn with_max_request_bytes(mut self, max_request_bytes: usize) -> Self {
        self.max_request_bytes = max_request_bytes;
        self
    }

    pub fn local_addr(&self) -> Result<SocketAddr, RpcTransportError> {
        self.listener.local_addr().map_err(RpcTransportError::Io)
    }

    pub fn serve_next_connection(&self, service: &mut RpcService) -> Result<(), RpcTransportError> {
        self.serve_next_connection_with_handler(service)
    }

    pub fn serve_next_connection_with_handler<H: JsonRpcHandler>(
        &self,
        handler: &mut H,
    ) -> Result<(), RpcTransportError> {
        let (stream, _) = self.listener.accept()?;
        serve_http_json_rpc_handler_connection(handler, stream, self.max_request_bytes)
    }
}

pub trait JsonRpcHandler {
    fn handle_json_request(&mut self, request: &[u8]) -> Result<Vec<u8>, RpcTransportError>;
}

pub struct AuthenticatedJsonRpcHandler<'a, H> {
    inner: &'a mut H,
    bearer_token: String,
}

impl<'a, H: JsonRpcHandler> AuthenticatedJsonRpcHandler<'a, H> {
    pub fn new(inner: &'a mut H, bearer_token: impl Into<String>) -> Self {
        Self {
            inner,
            bearer_token: bearer_token.into(),
        }
    }
}

impl<H: JsonRpcHandler> JsonRpcHandler for AuthenticatedJsonRpcHandler<'_, H> {
    fn handle_json_request(&mut self, request: &[u8]) -> Result<Vec<u8>, RpcTransportError> {
        let envelope = match serde_json::from_slice::<AuthenticatedRpcRequest>(request) {
            Ok(envelope) => envelope,
            Err(error) => {
                return if serde_json::from_slice::<serde_json::Value>(request).is_err() {
                    rpc_error_response_bytes("rpc.decode_error", error.to_string())
                } else {
                    rpc_error_response_bytes(
                        "rpc.authentication_required",
                        "operator endpoint requires a valid bearer token",
                    )
                };
            }
        };

        if envelope.bearer_token != self.bearer_token {
            return rpc_error_response_bytes(
                "rpc.authentication_required",
                "operator endpoint requires a valid bearer token",
            );
        }

        let inner_request =
            serde_json::to_vec(&envelope.request).map_err(RpcTransportError::Encode)?;
        self.inner.handle_json_request(&inner_request)
    }
}

pub struct RateLimitedJsonRpcHandler<'a, H> {
    inner: &'a mut H,
    max_requests: usize,
    served_requests: usize,
}

impl<'a, H: JsonRpcHandler> RateLimitedJsonRpcHandler<'a, H> {
    pub fn new(inner: &'a mut H, max_requests: usize) -> Self {
        Self {
            inner,
            max_requests,
            served_requests: 0,
        }
    }
}

impl<H: JsonRpcHandler> JsonRpcHandler for RateLimitedJsonRpcHandler<'_, H> {
    fn handle_json_request(&mut self, request: &[u8]) -> Result<Vec<u8>, RpcTransportError> {
        if self.served_requests >= self.max_requests {
            return rpc_error_response_bytes(
                "rpc.rate_limited",
                "operator endpoint request limit exceeded",
            );
        }
        self.served_requests += 1;
        self.inner.handle_json_request(request)
    }
}

pub struct RpcService {
    node: ValidatorNode,
    subscriptions: BTreeMap<String, Vec<SubscriptionTopic>>,
    subscription_events: Vec<SubscriptionEvent>,
    next_subscription_id: u64,
    next_subscription_sequence: u64,
}

impl RpcService {
    pub fn new(node: ValidatorNode) -> Self {
        Self {
            node,
            subscriptions: BTreeMap::new(),
            subscription_events: Vec::new(),
            next_subscription_id: 1,
            next_subscription_sequence: 1,
        }
    }

    pub fn node(&self) -> &ValidatorNode {
        &self.node
    }

    fn normalize_subscription_topics(mut topics: Vec<SubscriptionTopic>) -> Vec<SubscriptionTopic> {
        if topics.is_empty() {
            topics = vec![
                SubscriptionTopic::Blocks,
                SubscriptionTopic::Receipts,
                SubscriptionTopic::Events,
                SubscriptionTopic::Finality,
            ];
        }
        topics.sort();
        topics.dedup();
        topics
    }

    fn earliest_retained_subscription_sequence(&self) -> u64 {
        self.subscription_events
            .first()
            .map(|event| event.sequence)
            .unwrap_or(self.next_subscription_sequence)
    }

    pub fn subscribe(&mut self, topics: Vec<SubscriptionTopic>) -> SubscriptionStatus {
        let topics = Self::normalize_subscription_topics(topics);
        let subscription_id = format!("sub-{}", self.next_subscription_id);
        self.next_subscription_id += 1;
        self.subscriptions
            .insert(subscription_id.clone(), topics.clone());
        SubscriptionStatus {
            subscription_id,
            topics,
            next_sequence: self.next_subscription_sequence,
            earliest_retained_sequence: self.earliest_retained_subscription_sequence(),
        }
    }

    pub fn get_subscription_events(
        &self,
        subscription_id: &str,
        from_sequence: u64,
        limit: usize,
    ) -> Result<SubscriptionEventPage, RpcError> {
        let topics = self
            .subscriptions
            .get(subscription_id)
            .ok_or(RpcError::SubscriptionNotFound)?;
        let effective_limit = limit.min(DEFAULT_MAX_SUBSCRIPTION_EVENT_PAGE_SIZE);
        let earliest_retained_sequence = self.earliest_retained_subscription_sequence();
        let effective_from_sequence = from_sequence.max(earliest_retained_sequence);
        let mut events = Vec::new();
        if effective_limit > 0 {
            for event in self
                .subscription_events
                .iter()
                .filter(|event| event.sequence >= effective_from_sequence)
            {
                if topics.contains(&event.topic) {
                    events.push(event.clone());
                    if events.len() == effective_limit {
                        break;
                    }
                }
            }
        }
        let next_sequence = events
            .last()
            .map(|event| event.sequence + 1)
            .unwrap_or(self.next_subscription_sequence);

        Ok(SubscriptionEventPage {
            subscription_id: subscription_id.into(),
            events,
            from_sequence: effective_from_sequence,
            next_sequence,
            earliest_retained_sequence,
            limit: effective_limit,
        })
    }

    fn publish_subscription_event(
        &mut self,
        topic: SubscriptionTopic,
        notification: SubscriptionNotification,
    ) {
        self.subscription_events.push(SubscriptionEvent {
            sequence: self.next_subscription_sequence,
            topic,
            notification,
        });
        self.next_subscription_sequence += 1;
        if self.subscription_events.len() > DEFAULT_MAX_SUBSCRIPTION_EVENTS {
            let excess = self.subscription_events.len() - DEFAULT_MAX_SUBSCRIPTION_EVENTS;
            self.subscription_events.drain(0..excess);
        }
    }

    fn publish_block_notifications(&mut self, block: &Block, events: Vec<Event>) {
        self.publish_subscription_event(
            SubscriptionTopic::Blocks,
            SubscriptionNotification::Block(Box::new(block.clone())),
        );
        for receipt in &block.receipts {
            self.publish_subscription_event(
                SubscriptionTopic::Receipts,
                SubscriptionNotification::Receipt(Box::new(receipt.clone())),
            );
        }
        for event in events {
            self.publish_subscription_event(
                SubscriptionTopic::Events,
                SubscriptionNotification::Event(Box::new(event)),
            );
        }
    }

    pub fn publish_finality_certificate(&mut self, certificate: FinalityCertificate) {
        self.publish_subscription_event(
            SubscriptionTopic::Finality,
            SubscriptionNotification::FinalityCertificate(Box::new(certificate)),
        );
    }

    pub fn submit_transaction(&mut self, tx: Transaction) -> Result<(), RpcError> {
        self.node.submit_transaction(tx).map_err(RpcError::Mempool)
    }

    pub fn produce_block(&mut self, height: u64, timestamp: u64) -> Result<Block, RpcError> {
        let event_start = self.node.state().events().len();
        let block = self.node.propose_pending_block(height, timestamp);
        self.node
            .validate_and_apply(&block)
            .map_err(RpcError::Block)?;
        let events = self.node.state().events()[event_start..].to_vec();
        self.publish_block_notifications(&block, events);
        Ok(block)
    }

    pub fn import_block(&mut self, block: &Block) -> Result<(), RpcError> {
        let event_start = self.node.state().events().len();
        self.node
            .validate_and_apply(block)
            .map_err(RpcError::Block)?;
        let events = self.node.state().events()[event_start..].to_vec();
        self.publish_block_notifications(block, events);
        Ok(())
    }

    pub fn get_transaction(&self, tx_hash: &str) -> Result<Transaction, RpcError> {
        self.node
            .get_transaction(tx_hash)
            .cloned()
            .ok_or(RpcError::TransactionNotFound)
    }

    pub fn get_receipt(&self, tx_hash: &str) -> Result<Receipt, RpcError> {
        self.node
            .get_receipt(tx_hash)
            .cloned()
            .ok_or(RpcError::ReceiptNotFound)
    }

    pub fn get_receipt_proof(&self, height: u64, index: usize) -> Result<ReceiptProof, RpcError> {
        self.node
            .get_block(height)
            .ok_or(RpcError::BlockNotFound)?
            .receipt_proof(index)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_block(&self, height: u64) -> Result<Block, RpcError> {
        self.node
            .get_block(height)
            .cloned()
            .ok_or(RpcError::BlockNotFound)
    }

    pub fn get_blocks_page(&self, start_height: u64, limit: usize) -> BlockPage {
        let effective_limit = limit.min(DEFAULT_MAX_BLOCK_PAGE_SIZE);
        BlockPage {
            blocks: self.node.blocks_from(start_height, effective_limit),
            start_height,
            limit: effective_limit,
            highest_height: self.node.highest_block_height(),
        }
    }

    pub fn get_state_root(&self) -> String {
        self.node.state().global_state_root()
    }

    pub fn node_health(&self) -> NodeHealthReport {
        let state = self.node.state();
        NodeHealthReport {
            chain_id: state.chain_id().clone(),
            network_id: None,
            validator_id: None,
            height: state.height(),
            pending_mempool_transactions: self.node.pending_len(),
            trusted_validator_keys: None,
            pending_validator_set_metadata_updates: None,
            storage_root: state.storage_root(),
            registry_root: state.registry_root(),
            policy_root: state.policy_root(),
            event_root: state.event_root(),
            nonce_root: state.nonce_root(),
            outbox_root: state.outbox_root(),
            global_state_root: state.global_state_root(),
        }
    }

    pub fn mempool_status(&self) -> MempoolStatus {
        let policy = self.node.mempool_admission_policy();
        let mut pending_by_sender = BTreeMap::new();
        for tx in self.node.pending_transactions() {
            *pending_by_sender.entry(tx.sender.clone()).or_insert(0) += 1;
        }

        MempoolStatus {
            pending_transactions: self.node.pending_len(),
            max_pending: policy.max_pending,
            max_pending_per_sender: policy.max_pending_per_sender,
            max_transaction_bytes: policy.max_transaction_bytes,
            block_resource_limit: self.node.block_resource_limit(),
            pending_by_sender,
        }
    }

    pub fn snapshot(&self) -> StateSnapshot {
        self.node.state().snapshot()
    }

    pub fn call_balance_view(
        &self,
        contract: impl Into<ContractId>,
        owner: impl Into<Principal>,
        asset: impl Into<AssetId>,
    ) -> Amount {
        self.node.state().balance(contract, owner, asset)
    }

    pub fn call_total_supply_view(
        &self,
        contract: impl Into<ContractId>,
        asset: impl Into<AssetId>,
    ) -> Amount {
        self.node.state().total_supply(contract, asset)
    }

    pub fn get_storage_proof(&self, key: &StateKey) -> Result<StorageProof, RpcError> {
        self.node
            .state()
            .storage_proof(key)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_storage_non_inclusion_proof(
        &self,
        key: &StateKey,
    ) -> Result<StorageNonInclusionProof, RpcError> {
        self.node
            .state()
            .storage_non_inclusion_proof(key)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_registry_proof(&self, key: &GrantKey) -> Result<RegistryProof, RpcError> {
        self.node
            .state()
            .registry_proof(key)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_registry_non_inclusion_proof(
        &self,
        key: &GrantKey,
    ) -> Result<RegistryNonInclusionProof, RpcError> {
        self.node
            .state()
            .registry_non_inclusion_proof(key)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_outbox_message_proof(&self, index: usize) -> Result<OutboxMessageProof, RpcError> {
        self.node
            .state()
            .outbox_message_proof(index)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_event_proof(&self, index: usize) -> Result<EventProof, RpcError> {
        self.node
            .state()
            .event_proof(index)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_events(&self) -> Vec<Event> {
        self.node.state().events().to_vec()
    }

    pub fn get_events_page(&self, offset: usize, limit: usize) -> EventPage {
        let events = self.node.state().events();
        let effective_limit = limit.min(DEFAULT_MAX_EVENT_PAGE_SIZE);
        EventPage {
            events: events
                .iter()
                .skip(offset)
                .take(effective_limit)
                .cloned()
                .collect(),
            offset,
            limit: effective_limit,
            total_events: events.len(),
        }
    }

    pub fn get_contract(
        &self,
        contract: impl Into<ContractId>,
    ) -> Result<ContractRecord, RpcError> {
        self.node
            .state()
            .contract(contract)
            .cloned()
            .ok_or(RpcError::ContractNotFound)
    }

    pub fn get_aspect_module(&self, module_hash: &str) -> Result<AspectModuleRecord, RpcError> {
        self.node
            .state()
            .aspect_module(module_hash)
            .cloned()
            .ok_or(RpcError::AspectModuleNotFound)
    }

    pub fn get_aspect_module_proof(
        &self,
        module_hash: &str,
    ) -> Result<AspectModuleProof, RpcError> {
        self.node
            .state()
            .aspect_module_proof(module_hash)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_aspect_module_artifacts(
        &self,
        module_hash: &str,
    ) -> Result<AspectModuleArtifactReport, RpcError> {
        self.get_aspect_module(module_hash)
            .map(|module| AspectModuleArtifactReport::from_record(&module))
    }

    pub fn get_aspect_modules(&self) -> Vec<AspectModuleRecord> {
        self.node.state().aspect_module_records().cloned().collect()
    }

    pub fn get_scheduled_upgrades(&self) -> Vec<ScheduledUpgrade> {
        self.node.state().scheduled_upgrades().cloned().collect()
    }

    pub fn get_scheduled_policy_updates(&self) -> Vec<ScheduledPolicyUpdate> {
        self.node
            .state()
            .scheduled_policy_updates()
            .cloned()
            .collect()
    }

    pub fn get_upgrade_rehearsal_report(
        &self,
        upgrade_id: &str,
    ) -> Result<UpgradeRehearsalReport, RpcError> {
        self.node
            .state()
            .rehearse_scheduled_upgrade(upgrade_id)
            .map_err(RpcError::Execution)
    }

    pub fn evaluate_restricted_script(
        &self,
        source: String,
        max_steps: u64,
    ) -> Result<RestrictedEvaluationReport, RpcError> {
        if max_steps > DEFAULT_MAX_RESTRICTED_EVALUATOR_STEPS {
            return Err(RpcError::EvaluatorStepBudgetTooLarge);
        }
        let instructions = parse_restricted_script(&source)
            .map_err(|error| RpcError::EvaluatorParse(format!("{error:?}")))?;
        let canonical_source = canonical_script_source(&instructions);
        let evaluator = RestrictedScriptEvaluator::new(max_steps);
        let report = evaluator
            .execute(&instructions)
            .map_err(RpcError::EvaluatorExecution)?;
        let trace_root = trace_root(&report.trace);
        Ok(RestrictedEvaluationReport {
            evaluator: "detta.restricted-script-evaluator".into(),
            source,
            canonical_source,
            max_steps,
            report,
            trace_root,
        })
    }

    pub fn handle_request(&mut self, request: RpcRequest) -> RpcResponse {
        match request {
            RpcRequest::SubmitTransaction { transaction } => self
                .submit_transaction(transaction)
                .map(|()| RpcResult::Submitted)
                .into(),
            RpcRequest::SubmitSignedTransaction { signed } => signed
                .into_authorized_transaction(self.node.state())
                .map_err(RpcError::Mempool)
                .and_then(|transaction| self.submit_transaction(transaction))
                .map(|()| RpcResult::Submitted)
                .into(),
            RpcRequest::ProduceBlock { height, timestamp } => self
                .produce_block(height, timestamp)
                .map(|block| RpcResult::Block(Box::new(block)))
                .into(),
            RpcRequest::ImportBlock { block } => self
                .import_block(&block)
                .map(|()| RpcResult::Imported)
                .into(),
            RpcRequest::GetTransaction { tx_hash } => self
                .get_transaction(&tx_hash)
                .map(|transaction| RpcResult::Transaction(Box::new(transaction)))
                .into(),
            RpcRequest::GetReceipt { tx_hash } => self
                .get_receipt(&tx_hash)
                .map(|receipt| RpcResult::Receipt(Box::new(receipt)))
                .into(),
            RpcRequest::GetReceiptProof { height, index } => self
                .get_receipt_proof(height, index)
                .map(|proof| RpcResult::ReceiptProof(Box::new(proof)))
                .into(),
            RpcRequest::GetBlock { height } => self
                .get_block(height)
                .map(|block| RpcResult::Block(Box::new(block)))
                .into(),
            RpcRequest::GetBlocksPage {
                start_height,
                limit,
            } => RpcResponse::Ok(RpcResult::BlocksPage(
                self.get_blocks_page(start_height, limit),
            )),
            RpcRequest::GetNodeHealth => {
                RpcResponse::Ok(RpcResult::NodeHealth(Box::new(self.node_health())))
            }
            RpcRequest::GetOperatorMetrics => Err(RpcError::UnsupportedNodeMethod).into(),
            RpcRequest::GetOperatorAlerts => Err(RpcError::UnsupportedNodeMethod).into(),
            RpcRequest::GetMempoolStatus => {
                RpcResponse::Ok(RpcResult::MempoolStatus(self.mempool_status()))
            }
            RpcRequest::GetStateRoot => {
                RpcResponse::Ok(RpcResult::StateRoot(self.get_state_root()))
            }
            RpcRequest::GetSnapshot => {
                RpcResponse::Ok(RpcResult::Snapshot(Box::new(self.snapshot())))
            }
            RpcRequest::GetBalance {
                contract,
                owner,
                asset,
            } => RpcResponse::Ok(RpcResult::Amount(
                self.call_balance_view(contract, owner, asset),
            )),
            RpcRequest::GetTotalSupply { contract, asset } => RpcResponse::Ok(RpcResult::Amount(
                self.call_total_supply_view(contract, asset),
            )),
            RpcRequest::GetStorageProof { key } => self
                .get_storage_proof(&key)
                .map(|proof| RpcResult::StorageProof(Box::new(proof)))
                .into(),
            RpcRequest::GetStorageNonInclusionProof { key } => self
                .get_storage_non_inclusion_proof(&key)
                .map(|proof| RpcResult::StorageNonInclusionProof(Box::new(proof)))
                .into(),
            RpcRequest::GetRegistryProof { key } => self
                .get_registry_proof(&key)
                .map(|proof| RpcResult::RegistryProof(Box::new(proof)))
                .into(),
            RpcRequest::GetRegistryNonInclusionProof { key } => self
                .get_registry_non_inclusion_proof(&key)
                .map(|proof| RpcResult::RegistryNonInclusionProof(Box::new(proof)))
                .into(),
            RpcRequest::GetOutboxMessageProof { index } => self
                .get_outbox_message_proof(index)
                .map(|proof| RpcResult::OutboxMessageProof(Box::new(proof)))
                .into(),
            RpcRequest::GetEventProof { index } => self
                .get_event_proof(index)
                .map(|proof| RpcResult::EventProof(Box::new(proof)))
                .into(),
            RpcRequest::GetEvents => RpcResponse::Ok(RpcResult::Events(self.get_events())),
            RpcRequest::GetEventsPage { offset, limit } => {
                RpcResponse::Ok(RpcResult::EventsPage(self.get_events_page(offset, limit)))
            }
            RpcRequest::Subscribe { topics } => {
                RpcResponse::Ok(RpcResult::SubscriptionStatus(self.subscribe(topics)))
            }
            RpcRequest::GetSubscriptionEvents {
                subscription_id,
                from_sequence,
                limit,
            } => self
                .get_subscription_events(&subscription_id, from_sequence, limit)
                .map(RpcResult::SubscriptionEvents)
                .into(),
            RpcRequest::GetContract { contract } => self
                .get_contract(contract)
                .map(|contract| RpcResult::Contract(Box::new(contract)))
                .into(),
            RpcRequest::GetAspectModule { module_hash } => self
                .get_aspect_module(&module_hash)
                .map(|module| RpcResult::AspectModule(Box::new(module)))
                .into(),
            RpcRequest::GetAspectModuleProof { module_hash } => self
                .get_aspect_module_proof(&module_hash)
                .map(|proof| RpcResult::AspectModuleProof(Box::new(proof)))
                .into(),
            RpcRequest::GetAspectModuleArtifacts { module_hash } => self
                .get_aspect_module_artifacts(&module_hash)
                .map(|report| RpcResult::AspectModuleArtifacts(Box::new(report)))
                .into(),
            RpcRequest::GetAspectModules => {
                RpcResponse::Ok(RpcResult::AspectModules(self.get_aspect_modules()))
            }
            RpcRequest::GetScheduledUpgrades => {
                RpcResponse::Ok(RpcResult::ScheduledUpgrades(self.get_scheduled_upgrades()))
            }
            RpcRequest::GetScheduledPolicyUpdates => RpcResponse::Ok(
                RpcResult::ScheduledPolicyUpdates(self.get_scheduled_policy_updates()),
            ),
            RpcRequest::GetUpgradeRehearsalReport { upgrade_id } => self
                .get_upgrade_rehearsal_report(&upgrade_id)
                .map(|report| RpcResult::UpgradeRehearsalReport(Box::new(report)))
                .into(),
            RpcRequest::EvaluateRestrictedScript { source, max_steps } => self
                .evaluate_restricted_script(source, max_steps)
                .map(|report| RpcResult::RestrictedEvaluation(Box::new(report)))
                .into(),
            RpcRequest::GetRestrictedEvaluatorFixtureInventory => {
                RpcResponse::Ok(RpcResult::RestrictedEvaluatorFixtureInventory(Box::new(
                    restricted_evaluator_fixture_inventory(),
                )))
            }
            RpcRequest::ProposeValidatorSetMetadataUpdate { .. }
            | RpcRequest::GetFinalityCertificate { .. }
            | RpcRequest::GetSlashingRecord { .. }
            | RpcRequest::GetDaManifest { .. }
            | RpcRequest::GetDaShare { .. }
            | RpcRequest::GetDaCertificate { .. }
            | RpcRequest::GetDaChallengeRecord { .. }
            | RpcRequest::GetDaPayload { .. }
            | RpcRequest::GetDaNamespace { .. }
            | RpcRequest::GetDaSampleProofs { .. }
            | RpcRequest::GetDaStatus { .. }
            | RpcRequest::GetDaRepairStatus { .. }
            | RpcRequest::GetDaStorageStats
            | RpcRequest::GetValidatorSetMetadataUpdateStatus { .. }
            | RpcRequest::GetValidatorSetMetadataAuditRecords { .. }
            | RpcRequest::GetSnapshotImportAuditRecords { .. }
            | RpcRequest::GetSnapshotImportAuditRoot
            | RpcRequest::GetSnapshotImportAuditConfigRoot
            | RpcRequest::GetSnapshotImportAuditConfig
            | RpcRequest::GetPersistentNodeSnapshotRoots
            | RpcRequest::GetSnapshotMetadataRootStatus
            | RpcRequest::GetSnapshotSyncClientMetrics
            | RpcRequest::GetRequiredSnapshotMetadataRoots => {
                Err(RpcError::UnsupportedNodeMethod).into()
            }
        }
    }

    pub fn handle_json_request(&mut self, request: &[u8]) -> Result<Vec<u8>, RpcTransportError> {
        json_rpc_response_for_request(request, |request| self.handle_request(request))
    }
}

impl JsonRpcHandler for RpcService {
    fn handle_json_request(&mut self, request: &[u8]) -> Result<Vec<u8>, RpcTransportError> {
        RpcService::handle_json_request(self, request)
    }
}

impl From<Result<RpcResult, RpcError>> for RpcResponse {
    fn from(result: Result<RpcResult, RpcError>) -> Self {
        match result {
            Ok(result) => RpcResponse::Ok(result),
            Err(error) => RpcResponse::Error(RpcErrorBody::from(error)),
        }
    }
}

impl From<RpcError> for RpcErrorBody {
    fn from(error: RpcError) -> Self {
        Self {
            code: rpc_error_code(&error).into(),
            message: rpc_error_message(&error).into(),
        }
    }
}

pub fn serve_json_rpc_connection<S: Read + Write>(
    service: &mut RpcService,
    stream: S,
    max_request_bytes: usize,
) -> Result<(), RpcTransportError> {
    serve_json_rpc_handler_connection(service, stream, max_request_bytes)
}

pub fn serve_json_rpc_handler_connection<H: JsonRpcHandler, S: Read + Write>(
    handler: &mut H,
    stream: S,
    max_request_bytes: usize,
) -> Result<(), RpcTransportError> {
    let mut reader = BufReader::new(stream);

    loop {
        let Some(request) = read_bounded_json_line(&mut reader, max_request_bytes)? else {
            return Ok(());
        };

        let response = handler.handle_json_request(&request)?;
        let stream = reader.get_mut();
        stream.write_all(&response)?;
        stream.write_all(b"\n")?;
        stream.flush()?;
    }
}

pub fn serve_http_json_rpc_connection<S: Read + Write>(
    service: &mut RpcService,
    stream: S,
    max_request_bytes: usize,
) -> Result<(), RpcTransportError> {
    serve_http_json_rpc_handler_connection(service, stream, max_request_bytes)
}

pub fn serve_http_json_rpc_handler_connection<H: JsonRpcHandler, S: Read + Write>(
    handler: &mut H,
    stream: S,
    max_request_bytes: usize,
) -> Result<(), RpcTransportError> {
    let mut reader = BufReader::new(stream);
    let headers = read_http_headers(&mut reader, DEFAULT_MAX_HTTP_HEADER_BYTES)?;
    if headers.is_empty() {
        return Ok(());
    }

    let response = match parse_http_json_rpc_request_head(&headers, max_request_bytes) {
        Ok(content_length) => {
            let mut body = vec![0; content_length];
            reader.read_exact(&mut body)?;
            let response = handler.handle_json_request(&body)?;
            http_response_bytes(200, "OK", response)
        }
        Err(response) => response,
    };

    let stream = reader.get_mut();
    stream.write_all(&response)?;
    stream.flush()?;
    Ok(())
}

pub fn json_rpc_response_for_request(
    request: &[u8],
    handle: impl FnOnce(RpcRequest) -> RpcResponse,
) -> Result<Vec<u8>, RpcTransportError> {
    let response = match serde_json::from_slice::<RpcRequest>(request) {
        Ok(request) => handle(request),
        Err(error) => RpcResponse::Error(RpcErrorBody {
            code: "rpc.decode_error".into(),
            message: error.to_string(),
        }),
    };

    serde_json::to_vec(&response).map_err(RpcTransportError::Encode)
}

fn rpc_error_response_bytes(
    code: impl Into<String>,
    message: impl Into<String>,
) -> Result<Vec<u8>, RpcTransportError> {
    serde_json::to_vec(&RpcResponse::Error(RpcErrorBody {
        code: code.into(),
        message: message.into(),
    }))
    .map_err(RpcTransportError::Encode)
}

fn read_http_headers<R: Read>(
    reader: &mut R,
    max_header_bytes: usize,
) -> Result<Vec<u8>, RpcTransportError> {
    let mut headers = Vec::new();
    let mut byte = [0_u8; 1];

    loop {
        let read = reader.read(&mut byte)?;
        if read == 0 {
            return Ok(headers);
        }

        headers.push(byte[0]);
        if headers.len() > max_header_bytes {
            return Err(RpcTransportError::RequestTooLarge {
                max_bytes: max_header_bytes,
            });
        }
        if headers.ends_with(b"\r\n\r\n") {
            return Ok(headers);
        }
    }
}

fn parse_http_json_rpc_request_head(
    headers: &[u8],
    max_request_bytes: usize,
) -> Result<usize, Vec<u8>> {
    let header_text = String::from_utf8_lossy(headers);
    let Some((request_line, header_lines)) = header_text.split_once("\r\n") else {
        return Err(http_rpc_error_response(
            400,
            "Bad Request",
            "rpc.http_bad_request",
            "malformed HTTP request",
        ));
    };

    let mut request_parts = request_line.split_whitespace();
    let method = request_parts.next().unwrap_or_default();
    let path = request_parts.next().unwrap_or_default();
    let version = request_parts.next().unwrap_or_default();
    if request_parts.next().is_some() || !version.starts_with("HTTP/1.") {
        return Err(http_rpc_error_response(
            400,
            "Bad Request",
            "rpc.http_bad_request",
            "malformed HTTP request",
        ));
    }
    if method != "POST" {
        return Err(http_rpc_error_response(
            405,
            "Method Not Allowed",
            "rpc.http_method_not_allowed",
            "JSON RPC HTTP endpoint requires POST",
        ));
    }
    if path != "/" && path != "/rpc" {
        return Err(http_rpc_error_response(
            404,
            "Not Found",
            "rpc.http_not_found",
            "JSON RPC HTTP endpoint was not found",
        ));
    }

    let mut content_length = None;
    for line in header_lines.split("\r\n").filter(|line| !line.is_empty()) {
        let Some((name, value)) = line.split_once(':') else {
            return Err(http_rpc_error_response(
                400,
                "Bad Request",
                "rpc.http_bad_request",
                "malformed HTTP header",
            ));
        };
        if name.trim().eq_ignore_ascii_case("content-length") {
            let Ok(parsed) = value.trim().parse::<usize>() else {
                return Err(http_rpc_error_response(
                    400,
                    "Bad Request",
                    "rpc.http_bad_request",
                    "invalid Content-Length",
                ));
            };
            if content_length.is_some_and(|existing| existing != parsed) {
                return Err(http_rpc_error_response(
                    400,
                    "Bad Request",
                    "rpc.http_bad_request",
                    "conflicting Content-Length",
                ));
            }
            content_length = Some(parsed);
        }
    }

    let Some(content_length) = content_length else {
        return Err(http_rpc_error_response(
            411,
            "Length Required",
            "rpc.http_length_required",
            "JSON RPC HTTP endpoint requires Content-Length",
        ));
    };
    if content_length > max_request_bytes {
        return Err(http_rpc_error_response(
            413,
            "Payload Too Large",
            "rpc.request_too_large",
            "JSON RPC request body exceeded maximum size",
        ));
    }

    Ok(content_length)
}

fn http_rpc_error_response(status_code: u16, reason: &str, code: &str, message: &str) -> Vec<u8> {
    let body = rpc_error_response_bytes(code, message).expect("RPC error body serializes");
    http_response_bytes(status_code, reason, body)
}

fn http_response_bytes(status_code: u16, reason: &str, body: Vec<u8>) -> Vec<u8> {
    let mut response = format!(
        "HTTP/1.1 {status_code} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend(body);
    response
}

fn read_bounded_json_line<R: Read>(
    reader: &mut R,
    max_request_bytes: usize,
) -> Result<Option<Vec<u8>>, RpcTransportError> {
    let mut line = Vec::new();
    let mut byte = [0_u8; 1];

    loop {
        let read = reader.read(&mut byte)?;
        if read == 0 {
            return if line.is_empty() {
                Ok(None)
            } else {
                Ok(Some(line))
            };
        }

        line.push(byte[0]);
        if line.len() > max_request_bytes {
            return Err(RpcTransportError::RequestTooLarge {
                max_bytes: max_request_bytes,
            });
        }
        if byte[0] == b'\n' {
            return Ok(Some(line));
        }
    }
}

fn rpc_error_code(error: &RpcError) -> &'static str {
    match error {
        RpcError::Mempool(MempoolError::ChainMismatch) => "mempool.chain_mismatch",
        RpcError::Mempool(MempoolError::InvalidSignature) => "mempool.invalid_signature",
        RpcError::Mempool(MempoolError::UnauthorizedSigner { .. }) => "mempool.unauthorized_signer",
        RpcError::Mempool(MempoolError::TransactionExpired { .. }) => "mempool.transaction_expired",
        RpcError::Mempool(MempoolError::DuplicateTransaction) => "mempool.duplicate_transaction",
        RpcError::Mempool(MempoolError::NonceAlreadyUsed) => "mempool.nonce_already_used",
        RpcError::Mempool(MempoolError::InsufficientBudget) => "mempool.insufficient_budget",
        RpcError::Mempool(MempoolError::TransactionTooLarge { .. }) => {
            "mempool.transaction_too_large"
        }
        RpcError::Mempool(MempoolError::PoolFull { .. }) => "mempool.pool_full",
        RpcError::Mempool(MempoolError::SenderPendingLimitExceeded { .. }) => {
            "mempool.sender_pending_limit_exceeded"
        }
        RpcError::Block(BlockError::ChainMismatch) => "block.chain_mismatch",
        RpcError::Block(BlockError::PreviousBlockMismatch) => "block.previous_block_mismatch",
        RpcError::Block(BlockError::TxRootMismatch) => "block.tx_root_mismatch",
        RpcError::Block(BlockError::ReceiptRootMismatch) => "block.receipt_root_mismatch",
        RpcError::Block(BlockError::ReceiptMismatch) => "block.receipt_mismatch",
        RpcError::Block(BlockError::StorageRootMismatch) => "block.storage_root_mismatch",
        RpcError::Block(BlockError::RegistryRootMismatch) => "block.registry_root_mismatch",
        RpcError::Block(BlockError::PolicyRootMismatch) => "block.policy_root_mismatch",
        RpcError::Block(BlockError::EventRootMismatch) => "block.event_root_mismatch",
        RpcError::Block(BlockError::NonceRootMismatch) => "block.nonce_root_mismatch",
        RpcError::Block(BlockError::OutboxRootMismatch) => "block.outbox_root_mismatch",
        RpcError::Block(BlockError::GlobalStateRootMismatch) => "block.global_state_root_mismatch",
        RpcError::Block(BlockError::ResourceLimitExceeded { .. }) => {
            "block.resource_limit_exceeded"
        }
        RpcError::BlockNotFound => "rpc.block_not_found",
        RpcError::CertificateNotFound => "rpc.certificate_not_found",
        RpcError::SlashingRecordNotFound => "rpc.slashing_record_not_found",
        RpcError::ReceiptNotFound => "rpc.receipt_not_found",
        RpcError::TransactionNotFound => "rpc.transaction_not_found",
        RpcError::ContractNotFound => "rpc.contract_not_found",
        RpcError::AspectModuleNotFound => "rpc.aspect_module_not_found",
        RpcError::DaManifestNotFound => "rpc.da_manifest_not_found",
        RpcError::DaShareNotFound => "rpc.da_share_not_found",
        RpcError::DaCertificateNotFound => "rpc.da_certificate_not_found",
        RpcError::DaChallengeRecordNotFound => "rpc.da_challenge_record_not_found",
        RpcError::DaNamespaceNotFound => "rpc.da_namespace_not_found",
        RpcError::ProofNotFound => "rpc.proof_not_found",
        RpcError::SubscriptionNotFound => "rpc.subscription_not_found",
        RpcError::Execution(ExecutionError::UpgradeNotFound) => "execution.upgrade_not_found",
        RpcError::Execution(ExecutionError::UpgradeAlreadyExecuted) => {
            "execution.upgrade_already_executed"
        }
        RpcError::Execution(ExecutionError::ContractNotFound) => "execution.contract_not_found",
        RpcError::Execution(_) => "execution.failed",
        RpcError::EvaluatorParse(_) => "evaluator.parse_error",
        RpcError::EvaluatorExecution(EvaluatorError::ForbiddenPrimitive) => {
            "evaluator.forbidden_primitive"
        }
        RpcError::EvaluatorExecution(EvaluatorError::StepBudgetExceeded) => {
            "evaluator.step_budget_exceeded"
        }
        RpcError::EvaluatorExecution(EvaluatorError::ArithmeticOverflow) => {
            "evaluator.arithmetic_overflow"
        }
        RpcError::EvaluatorExecution(EvaluatorError::Aborted) => "evaluator.aborted",
        RpcError::EvaluatorStepBudgetTooLarge => "evaluator.step_budget_too_large",
        RpcError::UnsupportedNodeMethod => "rpc.unsupported_node_method",
    }
}

fn rpc_error_message(error: &RpcError) -> &'static str {
    match error {
        RpcError::Mempool(MempoolError::ChainMismatch) => {
            "transaction chain ID does not match this node"
        }
        RpcError::Mempool(MempoolError::InvalidSignature) => {
            "transaction signature failed admission"
        }
        RpcError::Mempool(MempoolError::UnauthorizedSigner { .. }) => {
            "transaction signer is not registered for the sender account"
        }
        RpcError::Mempool(MempoolError::TransactionExpired { .. }) => {
            "transaction validity window has expired"
        }
        RpcError::Mempool(MempoolError::DuplicateTransaction) => "transaction is already pending",
        RpcError::Mempool(MempoolError::NonceAlreadyUsed) => {
            "transaction nonce has already been committed"
        }
        RpcError::Mempool(MempoolError::InsufficientBudget) => {
            "transaction budget is below deterministic execution cost"
        }
        RpcError::Mempool(MempoolError::TransactionTooLarge { .. }) => {
            "transaction exceeds the configured mempool byte limit"
        }
        RpcError::Mempool(MempoolError::PoolFull { .. }) => {
            "mempool has reached the configured pending transaction limit"
        }
        RpcError::Mempool(MempoolError::SenderPendingLimitExceeded { .. }) => {
            "sender has reached the configured pending transaction limit"
        }
        RpcError::Block(BlockError::ChainMismatch) => "block chain ID does not match this node",
        RpcError::Block(BlockError::PreviousBlockMismatch) => {
            "block does not extend the latest committed block"
        }
        RpcError::Block(BlockError::TxRootMismatch) => "block transaction root is invalid",
        RpcError::Block(BlockError::ReceiptRootMismatch) => "block receipt root is invalid",
        RpcError::Block(BlockError::ReceiptMismatch) => "block receipts are invalid",
        RpcError::Block(BlockError::StorageRootMismatch) => "block storage root is invalid",
        RpcError::Block(BlockError::RegistryRootMismatch) => "block registry root is invalid",
        RpcError::Block(BlockError::PolicyRootMismatch) => "block policy root is invalid",
        RpcError::Block(BlockError::EventRootMismatch) => "block event root is invalid",
        RpcError::Block(BlockError::NonceRootMismatch) => "block nonce root is invalid",
        RpcError::Block(BlockError::OutboxRootMismatch) => "block outbox root is invalid",
        RpcError::Block(BlockError::GlobalStateRootMismatch) => {
            "block global state root is invalid"
        }
        RpcError::Block(BlockError::ResourceLimitExceeded { .. }) => {
            "block resource limit exceeded"
        }
        RpcError::BlockNotFound => "block was not found",
        RpcError::CertificateNotFound => "finality certificate was not found",
        RpcError::SlashingRecordNotFound => "slashing record was not found",
        RpcError::ReceiptNotFound => "receipt was not found",
        RpcError::TransactionNotFound => "transaction was not found",
        RpcError::ContractNotFound => "contract was not found",
        RpcError::AspectModuleNotFound => "aspect module was not found",
        RpcError::DaManifestNotFound => "DA manifest was not found",
        RpcError::DaShareNotFound => "DA share was not found",
        RpcError::DaCertificateNotFound => "DA certificate was not found",
        RpcError::DaChallengeRecordNotFound => "DA challenge record was not found",
        RpcError::DaNamespaceNotFound => "DA namespace was not found",
        RpcError::ProofNotFound => "proof was not found",
        RpcError::SubscriptionNotFound => "subscription was not found",
        RpcError::Execution(ExecutionError::UpgradeNotFound) => "upgrade was not found",
        RpcError::Execution(ExecutionError::UpgradeAlreadyExecuted) => {
            "upgrade has already been executed"
        }
        RpcError::Execution(ExecutionError::ContractNotFound) => "contract was not found",
        RpcError::Execution(_) => "execution failed",
        RpcError::EvaluatorParse(_) => "restricted evaluator source failed to parse",
        RpcError::EvaluatorExecution(EvaluatorError::ForbiddenPrimitive) => {
            "restricted evaluator primitive is forbidden"
        }
        RpcError::EvaluatorExecution(EvaluatorError::StepBudgetExceeded) => {
            "restricted evaluator step budget was exceeded"
        }
        RpcError::EvaluatorExecution(EvaluatorError::ArithmeticOverflow) => {
            "restricted evaluator arithmetic overflowed"
        }
        RpcError::EvaluatorExecution(EvaluatorError::Aborted) => "restricted evaluator aborted",
        RpcError::EvaluatorStepBudgetTooLarge => "restricted evaluator step budget is too large",
        RpcError::UnsupportedNodeMethod => "method must be handled by a persistent validator node",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_consensus::{EquivocationEvidence, SlashingEvidence};
    use detta_core::{
        Argument, AspectModuleRecord, AspectModuleRoots, ContractInvariant, DeTTaState,
        EventPayload, MerkleProof, Method, PolicyEffect, TxStatus, DEFAULT_BLOCK_RESOURCE_LIMIT,
        DEFAULT_MEMPOOL_MAX_PENDING, DEFAULT_MEMPOOL_MAX_PENDING_PER_SENDER,
        DEFAULT_MEMPOOL_MAX_TRANSACTION_BYTES,
    };
    use detta_protocol::{ProtocolMessage, ValidatorSetMetadataUpdate, ValidatorSignatureDomain};
    use std::io::{BufRead, BufReader, Read, Write};
    use std::net::{Shutdown, TcpStream};

    fn seeded_rpc() -> RpcService {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_token(
                "TokenA",
                "USDC",
                vec![("Alice".into(), 100), ("Bob".into(), 50)],
            )
            .unwrap();
        RpcService::new(ValidatorNode::new("validator-1", state))
    }

    fn aspect_module_record() -> AspectModuleRecord {
        AspectModuleRecord::new(
            "MinimalTransferToken",
            "NormalizedBalanceFirst.v1",
            AspectModuleRoots {
                source_root: "source-root".into(),
                ir_root: "ir-root".into(),
                abi_root: "abi-root".into(),
                policy_root: "policy-root".into(),
                storage_schema_root: "storage-schema-root".into(),
                registry_schema_root: "registry-schema-root".into(),
                invariant_root: "invariant-root".into(),
            },
        )
    }

    fn transfer_tx() -> Transaction {
        Transaction {
            chain_id: "detta-local".into(),
            tx_hash: "tx1".into(),
            sender: "Alice".into(),
            nonce: 1,
            valid_until_height: None,
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

    fn write_request(stream: &mut TcpStream, request: &RpcRequest) {
        serde_json::to_writer(&mut *stream, request).unwrap();
        stream.write_all(b"\n").unwrap();
        stream.flush().unwrap();
    }

    fn read_response(reader: &mut BufReader<TcpStream>) -> RpcResponse {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(!line.is_empty());
        serde_json::from_str(&line).unwrap()
    }

    fn http_round_trip(raw_request: Vec<u8>, max_request_bytes: usize) -> (u16, RpcResponse) {
        let server = HttpJsonRpcServer::bind("127.0.0.1:0")
            .unwrap()
            .with_max_request_bytes(max_request_bytes);
        let addr = server.local_addr().unwrap();
        let server_thread = std::thread::spawn(move || {
            let mut rpc = seeded_rpc();
            server.serve_next_connection(&mut rpc).unwrap();
        });

        let mut stream = TcpStream::connect(addr).unwrap();
        stream.write_all(&raw_request).unwrap();
        stream.flush().unwrap();
        stream.shutdown(Shutdown::Write).unwrap();

        let mut response_bytes = Vec::new();
        stream.read_to_end(&mut response_bytes).unwrap();
        server_thread.join().unwrap();
        parse_http_rpc_response(&response_bytes)
    }

    fn http_post_request(path: &str, body: &[u8]) -> Vec<u8> {
        let mut request = format!(
            "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        request.extend(body);
        request
    }

    fn parse_http_rpc_response(response: &[u8]) -> (u16, RpcResponse) {
        let split = response
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("HTTP response has header terminator");
        let headers = String::from_utf8(response[..split].to_vec()).unwrap();
        let body = &response[split + 4..];
        let status_line = headers.lines().next().unwrap();
        let status = status_line
            .split_whitespace()
            .nth(1)
            .unwrap()
            .parse::<u16>()
            .unwrap();
        let content_length = headers
            .lines()
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .map(|(_, value)| value.trim().parse::<usize>().unwrap())
            .unwrap();
        assert_eq!(body.len(), content_length);
        (status, serde_json::from_slice(body).unwrap())
    }

    fn authenticated_request(token: &str, request: RpcRequest) -> Vec<u8> {
        serde_json::to_vec(&AuthenticatedRpcRequest {
            bearer_token: token.into(),
            request,
        })
        .unwrap()
    }

    fn rpc_request_method_tags() -> Vec<&'static str> {
        vec![
            "submit_transaction",
            "submit_signed_transaction",
            "produce_block",
            "import_block",
            "get_transaction",
            "get_receipt",
            "get_receipt_proof",
            "get_block",
            "get_blocks_page",
            "get_finality_certificate",
            "get_slashing_record",
            "get_da_manifest",
            "get_da_share",
            "get_da_certificate",
            "get_da_challenge_record",
            "get_da_payload",
            "get_da_namespace",
            "get_da_sample_proofs",
            "get_da_status",
            "get_da_repair_status",
            "get_da_storage_stats",
            "get_node_health",
            "get_operator_metrics",
            "get_operator_alerts",
            "get_mempool_status",
            "get_state_root",
            "get_snapshot",
            "get_persistent_node_snapshot_roots",
            "get_snapshot_metadata_root_status",
            "get_snapshot_sync_client_metrics",
            "get_required_snapshot_metadata_roots",
            "get_balance",
            "get_total_supply",
            "get_storage_proof",
            "get_storage_non_inclusion_proof",
            "get_registry_proof",
            "get_registry_non_inclusion_proof",
            "get_outbox_message_proof",
            "get_event_proof",
            "get_events",
            "get_events_page",
            "subscribe",
            "get_subscription_events",
            "get_contract",
            "get_aspect_module",
            "get_aspect_module_proof",
            "get_aspect_module_artifacts",
            "get_aspect_modules",
            "get_scheduled_upgrades",
            "get_scheduled_policy_updates",
            "get_upgrade_rehearsal_report",
            "evaluate_restricted_script",
            "get_restricted_evaluator_fixture_inventory",
            "propose_validator_set_metadata_update",
            "get_validator_set_metadata_update_status",
            "get_validator_set_metadata_audit_records",
            "get_snapshot_import_audit_records",
            "get_snapshot_import_audit_root",
            "get_snapshot_import_audit_config_root",
            "get_snapshot_import_audit_config",
        ]
    }

    #[test]
    fn rpc_openapi_schema_covers_request_method_tags() {
        let schema: serde_json::Value =
            serde_json::from_str(include_str!("../../../detta-rpc-openapi.json")).unwrap();
        assert_eq!(schema["openapi"], "3.1.0");
        assert_eq!(schema["x-detta-schema"], "detta.rpc.openapi.v1");
        assert_eq!(schema["x-detta-schema-version"], 1);
        assert_eq!(
            schema["paths"]["/rpc"]["post"]["requestBody"]["content"]["application/json"]["schema"]
                ["$ref"],
            "#/components/schemas/RpcRequest"
        );

        let schema_methods: Vec<_> = schema["components"]["schemas"]["RpcMethod"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert_eq!(schema_methods, rpc_request_method_tags());
    }

    #[test]
    fn authenticated_json_rpc_handler_requires_bearer_token() {
        let mut rpc = seeded_rpc();
        let mut handler = AuthenticatedJsonRpcHandler::new(&mut rpc, "operator-secret");
        let missing_token =
            serde_json::to_vec(&RpcRequest::GetStateRoot).expect("request serializes");

        let missing_response: RpcResponse =
            serde_json::from_slice(&handler.handle_json_request(&missing_token).unwrap()).unwrap();
        assert_eq!(
            missing_response,
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.authentication_required".into(),
                message: "operator endpoint requires a valid bearer token".into(),
            })
        );

        let bad_response: RpcResponse = serde_json::from_slice(
            &handler
                .handle_json_request(&authenticated_request(
                    "wrong-secret",
                    RpcRequest::GetStateRoot,
                ))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            bad_response,
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.authentication_required".into(),
                message: "operator endpoint requires a valid bearer token".into(),
            })
        );

        let ok_response: RpcResponse = serde_json::from_slice(
            &handler
                .handle_json_request(&authenticated_request(
                    "operator-secret",
                    RpcRequest::GetStateRoot,
                ))
                .unwrap(),
        )
        .unwrap();
        match ok_response {
            RpcResponse::Ok(RpcResult::StateRoot(root)) => assert_eq!(root.len(), 64),
            response => panic!("expected authenticated state root, got {response:?}"),
        }
    }

    #[test]
    fn rate_limited_json_rpc_handler_rejects_excess_requests() {
        let mut rpc = seeded_rpc();
        let mut handler = RateLimitedJsonRpcHandler::new(&mut rpc, 1);
        let request = serde_json::to_vec(&RpcRequest::GetStateRoot).unwrap();

        let first: RpcResponse =
            serde_json::from_slice(&handler.handle_json_request(&request).unwrap()).unwrap();
        match first {
            RpcResponse::Ok(RpcResult::StateRoot(root)) => assert_eq!(root.len(), 64),
            response => panic!("expected first state root, got {response:?}"),
        }

        let second: RpcResponse =
            serde_json::from_slice(&handler.handle_json_request(&request).unwrap()).unwrap();
        assert_eq!(
            second,
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.rate_limited".into(),
                message: "operator endpoint request limit exceeded".into(),
            })
        );
    }

    #[test]
    fn snapshot_metadata_root_status_json_fixture_is_stable() {
        let response = RpcResponse::Ok(RpcResult::SnapshotMetadataRootStatus(Box::new(
            SnapshotMetadataRootStatus {
                validator_set_metadata_audit_root: "validator-imported-root".into(),
                local_validator_set_metadata_audit_root: "validator-local-root".into(),
                persisted_validator_set_metadata_audit_root: Some("validator-imported-root".into()),
                using_imported_validator_set_metadata_audit_root: true,
                persisted_matches_local_validator_set_metadata_audit_root: false,
                snapshot_import_audit_config_root: Some("config-imported-root".into()),
                local_snapshot_import_audit_config_root: None,
                persisted_snapshot_import_audit_config_root: Some("config-imported-root".into()),
                using_imported_snapshot_import_audit_config_root: true,
                persisted_matches_local_snapshot_import_audit_config_root: false,
                snapshot_import_audit_root: Some("audit-local-root".into()),
                local_snapshot_import_audit_root: Some("audit-local-root".into()),
                persisted_snapshot_import_audit_root: Some("audit-imported-root".into()),
                using_imported_snapshot_import_audit_root: false,
                persisted_matches_local_snapshot_import_audit_root: false,
                required_snapshot_metadata_roots_root: Some("required-local-root".into()),
                local_required_snapshot_metadata_roots_root: Some("required-local-root".into()),
                persisted_required_snapshot_metadata_roots_root: Some(
                    "required-imported-root".into(),
                ),
                using_imported_required_snapshot_metadata_roots_root: false,
                persisted_matches_local_required_snapshot_metadata_roots_root: false,
                snapshot_sync_client_metrics_root: Some("metrics-local-root".into()),
                local_snapshot_sync_client_metrics_root: Some("metrics-local-root".into()),
                persisted_snapshot_sync_client_metrics_root: Some("metrics-imported-root".into()),
                using_imported_snapshot_sync_client_metrics_root: false,
                persisted_matches_local_snapshot_sync_client_metrics_root: false,
            },
        )));
        let fixture = concat!(
            r#"{"status":"ok","body":{"result":"snapshot_metadata_root_status","data":{"#,
            r#""validator_set_metadata_audit_root":"validator-imported-root","#,
            r#""local_validator_set_metadata_audit_root":"validator-local-root","#,
            r#""persisted_validator_set_metadata_audit_root":"validator-imported-root","#,
            r#""using_imported_validator_set_metadata_audit_root":true,"#,
            r#""persisted_matches_local_validator_set_metadata_audit_root":false,"#,
            r#""snapshot_import_audit_config_root":"config-imported-root","#,
            r#""local_snapshot_import_audit_config_root":null,"#,
            r#""persisted_snapshot_import_audit_config_root":"config-imported-root","#,
            r#""using_imported_snapshot_import_audit_config_root":true,"#,
            r#""persisted_matches_local_snapshot_import_audit_config_root":false,"#,
            r#""snapshot_import_audit_root":"audit-local-root","#,
            r#""local_snapshot_import_audit_root":"audit-local-root","#,
            r#""persisted_snapshot_import_audit_root":"audit-imported-root","#,
            r#""using_imported_snapshot_import_audit_root":false,"#,
            r#""persisted_matches_local_snapshot_import_audit_root":false,"#,
            r#""required_snapshot_metadata_roots_root":"required-local-root","#,
            r#""local_required_snapshot_metadata_roots_root":"required-local-root","#,
            r#""persisted_required_snapshot_metadata_roots_root":"required-imported-root","#,
            r#""using_imported_required_snapshot_metadata_roots_root":false,"#,
            r#""persisted_matches_local_required_snapshot_metadata_roots_root":false,"#,
            r#""snapshot_sync_client_metrics_root":"metrics-local-root","#,
            r#""local_snapshot_sync_client_metrics_root":"metrics-local-root","#,
            r#""persisted_snapshot_sync_client_metrics_root":"metrics-imported-root","#,
            r#""using_imported_snapshot_sync_client_metrics_root":false,"#,
            r#""persisted_matches_local_snapshot_sync_client_metrics_root":false}}}"#,
        );

        assert_eq!(serde_json::to_string(&response).unwrap(), fixture);
        assert_eq!(
            serde_json::from_str::<RpcResponse>(fixture).unwrap(),
            response
        );
    }

    #[test]
    fn node_health_json_fixture_is_stable() {
        let response = RpcResponse::Ok(RpcResult::NodeHealth(Box::new(NodeHealthReport {
            chain_id: "detta-local".into(),
            network_id: Some("detta-localnet".into()),
            validator_id: Some("validator-1".into()),
            height: 7,
            pending_mempool_transactions: 2,
            trusted_validator_keys: Some(4),
            pending_validator_set_metadata_updates: Some(1),
            storage_root: "storage-root".into(),
            registry_root: "registry-root".into(),
            policy_root: "policy-root".into(),
            event_root: "event-root".into(),
            nonce_root: "nonce-root".into(),
            outbox_root: "outbox-root".into(),
            global_state_root: "global-root".into(),
        })));
        let fixture = concat!(
            r#"{"status":"ok","body":{"result":"node_health","data":{"#,
            r#""chain_id":"detta-local","network_id":"detta-localnet","#,
            r#""validator_id":"validator-1","height":7,"#,
            r#""pending_mempool_transactions":2,"trusted_validator_keys":4,"#,
            r#""pending_validator_set_metadata_updates":1,"#,
            r#""storage_root":"storage-root","registry_root":"registry-root","#,
            r#""policy_root":"policy-root","event_root":"event-root","#,
            r#""nonce_root":"nonce-root","outbox_root":"outbox-root","#,
            r#""global_state_root":"global-root"}}}"#,
        );

        assert_eq!(serde_json::to_string(&response).unwrap(), fixture);
        assert_eq!(
            serde_json::from_str::<RpcResponse>(fixture).unwrap(),
            response
        );
    }

    #[test]
    fn operator_metrics_json_fixture_is_stable() {
        let response = RpcResponse::Ok(RpcResult::OperatorMetrics(Box::new(
            OperatorMetricsReport {
                network_id: Some("detta-localnet".into()),
                validator_id: Some("validator-1".into()),
                peer_count: Some(3),
                mempool_size: 5,
                consensus_height: 11,
                highest_finalized_height: Some(10),
                finality_lag: Some(1),
                last_block_execution_micros: Some(120),
                last_proof_serving_micros: Some(40),
                storage_bytes: Some(4096),
                rpc_error_count: 2,
                da_manifest_count: 1,
                da_missing_share_count: 0,
                da_payload_count: 1,
                da_challenge_record_count: 0,
                da_challenge_evidence_count: 0,
                da_custody_failure_count: 0,
                da_repair_record_count: 0,
                da_pending_repair_record_count: 0,
                da_oldest_pending_repair_age_blocks: None,
                da_total_bytes: 512,
            },
        )));
        let fixture = concat!(
            r#"{"status":"ok","body":{"result":"operator_metrics","data":{"#,
            r#""network_id":"detta-localnet","validator_id":"validator-1","#,
            r#""peer_count":3,"mempool_size":5,"consensus_height":11,"#,
            r#""highest_finalized_height":10,"finality_lag":1,"#,
            r#""last_block_execution_micros":120,"last_proof_serving_micros":40,"#,
            r#""storage_bytes":4096,"rpc_error_count":2,"#,
            r#""da_manifest_count":1,"da_missing_share_count":0,"#,
            r#""da_payload_count":1,"da_challenge_record_count":0,"#,
            r#""da_challenge_evidence_count":0,"da_custody_failure_count":0,"#,
            r#""da_repair_record_count":0,"da_pending_repair_record_count":0,"#,
            r#""da_oldest_pending_repair_age_blocks":null,"#,
            r#""da_total_bytes":512}}}"#,
        );

        assert_eq!(serde_json::to_string(&response).unwrap(), fixture);
        assert_eq!(
            serde_json::from_str::<RpcResponse>(fixture).unwrap(),
            response
        );
    }

    #[test]
    fn operator_alerts_json_fixture_is_stable() {
        let policy = OperatorAlertPolicy {
            min_peer_count: 1,
            max_finality_lag: 2,
            max_storage_bytes: 4096,
            max_rpc_error_count: 3,
            max_mempool_size: 10,
            max_latest_block_failure_ratio_per_mille: 500,
            max_da_repair_lag_blocks: 8,
        };
        let metrics = OperatorMetricsReport {
            network_id: Some("detta-localnet".into()),
            validator_id: Some("validator-1".into()),
            peer_count: Some(0),
            mempool_size: 12,
            consensus_height: 11,
            highest_finalized_height: Some(8),
            finality_lag: Some(3),
            last_block_execution_micros: Some(120),
            last_proof_serving_micros: Some(40),
            storage_bytes: Some(8192),
            rpc_error_count: 4,
            da_manifest_count: 1,
            da_missing_share_count: 2,
            da_payload_count: 1,
            da_challenge_record_count: 1,
            da_challenge_evidence_count: 1,
            da_custody_failure_count: 1,
            da_repair_record_count: 1,
            da_pending_repair_record_count: 1,
            da_oldest_pending_repair_age_blocks: Some(9),
            da_total_bytes: 1024,
        };
        let response = RpcResponse::Ok(RpcResult::OperatorAlerts(Box::new(OperatorAlertReport {
            policy,
            metrics,
            alerts: vec![
                OperatorAlert {
                    code: "operator.peer_isolation".into(),
                    severity: OperatorAlertSeverity::Critical,
                    message: "observed peer count is below policy minimum".into(),
                },
                OperatorAlert {
                    code: "operator.rpc_overload".into(),
                    severity: OperatorAlertSeverity::Warning,
                    message: "RPC error count exceeds policy threshold".into(),
                },
            ],
            root_mismatch: false,
            slashing_record_count: 0,
            latest_block_failure_count: 1,
            latest_block_receipt_count: 2,
        })));
        let fixture = concat!(
            r#"{"status":"ok","body":{"result":"operator_alerts","data":{"#,
            r#""policy":{"min_peer_count":1,"max_finality_lag":2,"#,
            r#""max_storage_bytes":4096,"max_rpc_error_count":3,"#,
            r#""max_mempool_size":10,"max_latest_block_failure_ratio_per_mille":500,"#,
            r#""max_da_repair_lag_blocks":8},"#,
            r#""metrics":{"network_id":"detta-localnet","validator_id":"validator-1","#,
            r#""peer_count":0,"mempool_size":12,"consensus_height":11,"#,
            r#""highest_finalized_height":8,"finality_lag":3,"#,
            r#""last_block_execution_micros":120,"last_proof_serving_micros":40,"#,
            r#""storage_bytes":8192,"rpc_error_count":4,"#,
            r#""da_manifest_count":1,"da_missing_share_count":2,"#,
            r#""da_payload_count":1,"da_challenge_record_count":1,"#,
            r#""da_challenge_evidence_count":1,"da_custody_failure_count":1,"#,
            r#""da_repair_record_count":1,"da_pending_repair_record_count":1,"#,
            r#""da_oldest_pending_repair_age_blocks":9,"#,
            r#""da_total_bytes":1024},"#,
            r#""alerts":[{"code":"operator.peer_isolation","severity":"critical","#,
            r#""message":"observed peer count is below policy minimum"},"#,
            r#"{"code":"operator.rpc_overload","severity":"warning","#,
            r#""message":"RPC error count exceeds policy threshold"}],"#,
            r#""root_mismatch":false,"slashing_record_count":0,"#,
            r#""latest_block_failure_count":1,"latest_block_receipt_count":2}}}"#,
        );

        assert_eq!(serde_json::to_string(&response).unwrap(), fixture);
        assert_eq!(
            serde_json::from_str::<RpcResponse>(fixture).unwrap(),
            response
        );
    }

    #[test]
    fn mempool_status_json_fixture_is_stable() {
        let mut pending_by_sender = BTreeMap::new();
        pending_by_sender.insert("Alice".into(), 2);
        pending_by_sender.insert("Bob".into(), 1);
        let response = RpcResponse::Ok(RpcResult::MempoolStatus(MempoolStatus {
            pending_transactions: 3,
            max_pending: 100,
            max_pending_per_sender: 4,
            max_transaction_bytes: 4096,
            block_resource_limit: 10_000,
            pending_by_sender,
        }));
        let fixture = concat!(
            r#"{"status":"ok","body":{"result":"mempool_status","data":{"#,
            r#""pending_transactions":3,"max_pending":100,"#,
            r#""max_pending_per_sender":4,"max_transaction_bytes":4096,"#,
            r#""block_resource_limit":10000,"#,
            r#""pending_by_sender":{"Alice":2,"Bob":1}}}}"#,
        );

        assert_eq!(serde_json::to_string(&response).unwrap(), fixture);
        assert_eq!(
            serde_json::from_str::<RpcResponse>(fixture).unwrap(),
            response
        );
    }

    #[test]
    fn subscription_json_fixtures_are_stable() {
        let subscribe_request = RpcRequest::Subscribe {
            topics: vec![SubscriptionTopic::Events],
        };
        let subscribe_request_fixture = r#"{"method":"subscribe","params":{"topics":["events"]}}"#;
        assert_eq!(
            serde_json::to_string(&subscribe_request).unwrap(),
            subscribe_request_fixture
        );
        assert_eq!(
            serde_json::from_str::<RpcRequest>(subscribe_request_fixture).unwrap(),
            subscribe_request
        );

        let events_request = RpcRequest::GetSubscriptionEvents {
            subscription_id: "sub-1".into(),
            from_sequence: 1,
            limit: 10,
        };
        let events_request_fixture = concat!(
            r#"{"method":"get_subscription_events","params":{"#,
            r#""subscription_id":"sub-1","from_sequence":1,"limit":10}}"#,
        );
        assert_eq!(
            serde_json::to_string(&events_request).unwrap(),
            events_request_fixture
        );
        assert_eq!(
            serde_json::from_str::<RpcRequest>(events_request_fixture).unwrap(),
            events_request
        );

        let status_response = RpcResponse::Ok(RpcResult::SubscriptionStatus(SubscriptionStatus {
            subscription_id: "sub-1".into(),
            topics: vec![SubscriptionTopic::Events],
            next_sequence: 1,
            earliest_retained_sequence: 1,
        }));
        let status_fixture = concat!(
            r#"{"status":"ok","body":{"result":"subscription_status","data":{"#,
            r#""subscription_id":"sub-1","topics":["events"],"next_sequence":1,"#,
            r#""earliest_retained_sequence":1}}}"#,
        );
        assert_eq!(
            serde_json::to_string(&status_response).unwrap(),
            status_fixture
        );
        assert_eq!(
            serde_json::from_str::<RpcResponse>(status_fixture).unwrap(),
            status_response
        );

        let page_response = RpcResponse::Ok(RpcResult::SubscriptionEvents(SubscriptionEventPage {
            subscription_id: "sub-1".into(),
            events: vec![SubscriptionEvent {
                sequence: 1,
                topic: SubscriptionTopic::Events,
                notification: SubscriptionNotification::Event(Box::new(Event {
                    contract: "TokenA".into(),
                    tx_hash: "tx1".into(),
                    index: 0,
                    payload: EventPayload::Transfer {
                        from: "Alice".into(),
                        to: "Bob".into(),
                        asset: "USDC".into(),
                        amount: 10,
                    },
                })),
            }],
            from_sequence: 1,
            next_sequence: 2,
            earliest_retained_sequence: 1,
            limit: 10,
        }));
        let page_fixture = concat!(
            r#"{"status":"ok","body":{"result":"subscription_events","data":{"#,
            r#""subscription_id":"sub-1","events":[{"sequence":1,"topic":"events","#,
            r#""notification":{"kind":"event","data":{"contract":"TokenA","#,
            r#""tx_hash":"tx1","index":0,"payload":{"Transfer":{"from":"Alice","#,
            r#""to":"Bob","asset":"USDC","amount":10}}}}}],"#,
            r#""from_sequence":1,"next_sequence":2,"earliest_retained_sequence":1,"#,
            r#""limit":10}}}"#,
        );
        assert_eq!(serde_json::to_string(&page_response).unwrap(), page_fixture);
        assert_eq!(
            serde_json::from_str::<RpcResponse>(page_fixture).unwrap(),
            page_response
        );

        let finality_page_response =
            RpcResponse::Ok(RpcResult::SubscriptionEvents(SubscriptionEventPage {
                subscription_id: "sub-2".into(),
                events: vec![SubscriptionEvent {
                    sequence: 3,
                    topic: SubscriptionTopic::Finality,
                    notification: SubscriptionNotification::FinalityCertificate(Box::new(
                        FinalityCertificate {
                            height: 7,
                            block_hash: "block-hash-7".into(),
                            signers: vec![
                                "validator-1".into(),
                                "validator-2".into(),
                                "validator-3".into(),
                            ],
                        },
                    )),
                }],
                from_sequence: 3,
                next_sequence: 4,
                earliest_retained_sequence: 1,
                limit: 10,
            }));
        let finality_page_fixture = concat!(
            r#"{"status":"ok","body":{"result":"subscription_events","data":{"#,
            r#""subscription_id":"sub-2","events":[{"sequence":3,"topic":"finality","#,
            r#""notification":{"kind":"finality_certificate","data":{"height":7,"#,
            r#""block_hash":"block-hash-7","signers":["validator-1","validator-2","#,
            r#""validator-3"]}}}],"from_sequence":3,"next_sequence":4,"#,
            r#""earliest_retained_sequence":1,"limit":10}}}"#,
        );
        assert_eq!(
            serde_json::to_string(&finality_page_response).unwrap(),
            finality_page_fixture
        );
        assert_eq!(
            serde_json::from_str::<RpcResponse>(finality_page_fixture).unwrap(),
            finality_page_response
        );

        let missing_response = RpcResponse::Error(RpcErrorBody {
            code: "rpc.subscription_not_found".into(),
            message: "subscription was not found".into(),
        });
        let missing_fixture = concat!(
            r#"{"status":"error","body":{"code":"rpc.subscription_not_found","#,
            r#""message":"subscription was not found"}}"#,
        );
        assert_eq!(
            serde_json::to_string(&missing_response).unwrap(),
            missing_fixture
        );
        assert_eq!(
            serde_json::from_str::<RpcResponse>(missing_fixture).unwrap(),
            missing_response
        );
    }

    #[test]
    fn governance_schedule_json_fixtures_are_stable() {
        let upgrades_response =
            RpcResponse::Ok(RpcResult::ScheduledUpgrades(vec![ScheduledUpgrade {
                upgrade_id: "upgrade-1".into(),
                governance_contract: "GovA".into(),
                target_contract: "TokenA".into(),
                new_code_hash: "token-code-v2".into(),
                execute_after_height: 7,
                executed: false,
            }]));
        let upgrades_fixture = concat!(
            r#"{"status":"ok","body":{"result":"scheduled_upgrades","data":[{"#,
            r#""upgrade_id":"upgrade-1","governance_contract":"GovA","#,
            r#""target_contract":"TokenA","new_code_hash":"token-code-v2","#,
            r#""execute_after_height":7,"executed":false}]}}"#,
        );
        assert_eq!(
            serde_json::to_string(&upgrades_response).unwrap(),
            upgrades_fixture
        );
        assert_eq!(
            serde_json::from_str::<RpcResponse>(upgrades_fixture).unwrap(),
            upgrades_response
        );

        let policy_updates_response = RpcResponse::Ok(RpcResult::ScheduledPolicyUpdates(vec![
            ScheduledPolicyUpdate {
                update_id: "policy-update-1".into(),
                governance_contract: "GovA".into(),
                target_contract: "TokenA".into(),
                method: Method::Transfer,
                effect: PolicyEffect::RegistryWrite,
                execute_after_height: 7,
                executed: false,
            },
        ]));
        let policy_updates_fixture = concat!(
            r#"{"status":"ok","body":{"result":"scheduled_policy_updates","data":[{"#,
            r#""update_id":"policy-update-1","governance_contract":"GovA","#,
            r#""target_contract":"TokenA","method":"Transfer","#,
            r#""effect":"RegistryWrite","execute_after_height":7,"executed":false}]}}"#,
        );
        assert_eq!(
            serde_json::to_string(&policy_updates_response).unwrap(),
            policy_updates_fixture
        );
        assert_eq!(
            serde_json::from_str::<RpcResponse>(policy_updates_fixture).unwrap(),
            policy_updates_response
        );
    }

    #[test]
    fn block_page_json_fixture_is_stable() {
        let response = RpcResponse::Ok(RpcResult::BlocksPage(BlockPage {
            blocks: vec![],
            start_height: 42,
            limit: DEFAULT_MAX_BLOCK_PAGE_SIZE,
            highest_height: 41,
        }));
        let fixture = concat!(
            r#"{"status":"ok","body":{"result":"blocks_page","data":{"#,
            r#""blocks":[],"start_height":42,"limit":100,"highest_height":41}}}"#,
        );

        assert_eq!(serde_json::to_string(&response).unwrap(), fixture);
        assert_eq!(
            serde_json::from_str::<RpcResponse>(fixture).unwrap(),
            response
        );
    }

    #[test]
    fn proof_response_json_fixtures_are_stable() {
        let receipt_response = RpcResponse::Ok(RpcResult::ReceiptProof(Box::new(ReceiptProof {
            receipt: Receipt {
                tx_hash: "tx1".into(),
                status: TxStatus::Committed,
                error: None,
                return_value: None,
                resource_units_used: 7,
                storage_root_after: "storage-root".into(),
                registry_root_after: "registry-root".into(),
                policy_root_after: "policy-root".into(),
                event_root_after: "event-root".into(),
                nonce_root_after: "nonce-root".into(),
                global_state_root_after: "global-root".into(),
            },
            proof: MerkleProof {
                root: "receipt-root".into(),
                leaf_hash: "receipt-leaf".into(),
                index: 0,
                leaf_count: 1,
                path: vec![],
            },
        })));
        let receipt_fixture = concat!(
            r#"{"status":"ok","body":{"result":"receipt_proof","data":{"#,
            r#""receipt":{"tx_hash":"tx1","status":"Committed","error":null,"#,
            r#""return_value":null,"resource_units_used":7,"#,
            r#""storage_root_after":"storage-root","#,
            r#""registry_root_after":"registry-root","#,
            r#""policy_root_after":"policy-root","#,
            r#""event_root_after":"event-root","#,
            r#""nonce_root_after":"nonce-root","#,
            r#""global_state_root_after":"global-root"},"#,
            r#""proof":{"root":"receipt-root","leaf_hash":"receipt-leaf","#,
            r#""index":0,"leaf_count":1,"path":[]}}}}"#,
        );
        assert_eq!(
            serde_json::to_string(&receipt_response).unwrap(),
            receipt_fixture
        );
        assert_eq!(
            serde_json::from_str::<RpcResponse>(receipt_fixture).unwrap(),
            receipt_response
        );

        let event_response = RpcResponse::Ok(RpcResult::EventProof(Box::new(EventProof {
            event: Event {
                contract: "TokenA".into(),
                tx_hash: "tx1".into(),
                index: 0,
                payload: EventPayload::Transfer {
                    from: "Alice".into(),
                    to: "Bob".into(),
                    asset: "USDC".into(),
                    amount: 10,
                },
            },
            proof: MerkleProof {
                root: "event-root".into(),
                leaf_hash: "event-leaf".into(),
                index: 0,
                leaf_count: 1,
                path: vec![],
            },
        })));
        let event_fixture = concat!(
            r#"{"status":"ok","body":{"result":"event_proof","data":{"#,
            r#""event":{"contract":"TokenA","tx_hash":"tx1","index":0,"#,
            r#""payload":{"Transfer":{"from":"Alice","to":"Bob","#,
            r#""asset":"USDC","amount":10}}},"#,
            r#""proof":{"root":"event-root","leaf_hash":"event-leaf","#,
            r#""index":0,"leaf_count":1,"path":[]}}}}"#,
        );
        assert_eq!(
            serde_json::to_string(&event_response).unwrap(),
            event_fixture
        );
        assert_eq!(
            serde_json::from_str::<RpcResponse>(event_fixture).unwrap(),
            event_response
        );
    }

    #[test]
    fn finality_certificate_json_fixture_is_stable() {
        let request = RpcRequest::GetFinalityCertificate { height: 7 };
        let request_fixture = r#"{"method":"get_finality_certificate","params":{"height":7}}"#;
        assert_eq!(serde_json::to_string(&request).unwrap(), request_fixture);
        assert_eq!(
            serde_json::from_str::<RpcRequest>(request_fixture).unwrap(),
            request
        );

        let response = RpcResponse::Ok(RpcResult::FinalityCertificate(Box::new(
            FinalityCertificate {
                height: 7,
                block_hash: "block-hash-7".into(),
                signers: vec![
                    "validator-1".into(),
                    "validator-2".into(),
                    "validator-3".into(),
                ],
            },
        )));
        let response_fixture = concat!(
            r#"{"status":"ok","body":{"result":"finality_certificate","data":{"#,
            r#""height":7,"block_hash":"block-hash-7","#,
            r#""signers":["validator-1","validator-2","validator-3"]}}}"#,
        );
        assert_eq!(serde_json::to_string(&response).unwrap(), response_fixture);
        assert_eq!(
            serde_json::from_str::<RpcResponse>(response_fixture).unwrap(),
            response
        );

        let missing_response = RpcResponse::Error(RpcErrorBody {
            code: "rpc.certificate_not_found".into(),
            message: "finality certificate was not found".into(),
        });
        let missing_fixture = concat!(
            r#"{"status":"error","body":{"code":"rpc.certificate_not_found","#,
            r#""message":"finality certificate was not found"}}"#,
        );
        assert_eq!(
            serde_json::to_string(&missing_response).unwrap(),
            missing_fixture
        );
        assert_eq!(
            serde_json::from_str::<RpcResponse>(missing_fixture).unwrap(),
            missing_response
        );
    }

    #[test]
    fn slashing_record_json_fixture_is_stable() {
        let request = RpcRequest::GetSlashingRecord {
            validator_id: "validator-1".into(),
        };
        let request_fixture =
            r#"{"method":"get_slashing_record","params":{"validator_id":"validator-1"}}"#;
        assert_eq!(serde_json::to_string(&request).unwrap(), request_fixture);
        assert_eq!(
            serde_json::from_str::<RpcRequest>(request_fixture).unwrap(),
            request
        );

        let response = RpcResponse::Ok(RpcResult::SlashingRecord(Box::new(SlashingRecord {
            validator_id: "validator-1".into(),
            slashed_at_height: 11,
            evidence: SlashingEvidence::Equivocation(EquivocationEvidence {
                validator_id: "validator-1".into(),
                height: 11,
                first_block_hash: "block-a".into(),
                second_block_hash: "block-b".into(),
            }),
        })));
        let response_fixture = concat!(
            r#"{"status":"ok","body":{"result":"slashing_record","data":{"#,
            r#""validator_id":"validator-1","slashed_at_height":11,"#,
            r#""evidence":{"type":"equivocation","data":{"validator_id":"validator-1","#,
            r#""height":11,"first_block_hash":"block-a","second_block_hash":"block-b"}}}}}"#,
        );
        assert_eq!(serde_json::to_string(&response).unwrap(), response_fixture);
        assert_eq!(
            serde_json::from_str::<RpcResponse>(response_fixture).unwrap(),
            response
        );

        let missing_response = RpcResponse::Error(RpcErrorBody {
            code: "rpc.slashing_record_not_found".into(),
            message: "slashing record was not found".into(),
        });
        let missing_fixture = concat!(
            r#"{"status":"error","body":{"code":"rpc.slashing_record_not_found","#,
            r#""message":"slashing record was not found"}}"#,
        );
        assert_eq!(
            serde_json::to_string(&missing_response).unwrap(),
            missing_fixture
        );
        assert_eq!(
            serde_json::from_str::<RpcResponse>(missing_fixture).unwrap(),
            missing_response
        );
    }

    #[test]
    fn rpc_submits_transaction_produces_block_and_returns_receipt() {
        let mut rpc = seeded_rpc();
        rpc.submit_transaction(transfer_tx()).unwrap();
        let pending_status = rpc.mempool_status();
        assert_eq!(pending_status.pending_transactions, 1);
        assert_eq!(
            pending_status.pending_by_sender.get("Alice").copied(),
            Some(1)
        );
        assert_eq!(pending_status.max_pending, DEFAULT_MEMPOOL_MAX_PENDING);
        assert_eq!(
            pending_status.max_pending_per_sender,
            DEFAULT_MEMPOOL_MAX_PENDING_PER_SENDER
        );
        assert_eq!(
            pending_status.max_transaction_bytes,
            DEFAULT_MEMPOOL_MAX_TRANSACTION_BYTES
        );
        assert_eq!(
            pending_status.block_resource_limit,
            DEFAULT_BLOCK_RESOURCE_LIMIT
        );

        let block = rpc.produce_block(1, 1_000).unwrap();
        let receipt = rpc.get_receipt("tx1").unwrap();
        let committed_status = rpc.mempool_status();
        let block_page = rpc.get_blocks_page(1, 10);
        let capped_block_page = rpc.get_blocks_page(1, DEFAULT_MAX_BLOCK_PAGE_SIZE + 1);

        assert_eq!(block.transactions.len(), 1);
        assert_eq!(receipt.status, TxStatus::Committed);
        assert_eq!(block_page.blocks, vec![block.clone()]);
        assert_eq!(block_page.start_height, 1);
        assert_eq!(block_page.limit, 10);
        assert_eq!(block_page.highest_height, 1);
        assert_eq!(capped_block_page.limit, DEFAULT_MAX_BLOCK_PAGE_SIZE);
        assert_eq!(committed_status.pending_transactions, 0);
        assert!(committed_status.pending_by_sender.is_empty());
        assert_eq!(rpc.call_balance_view("TokenA", "Alice", "USDC"), 90);
        assert_eq!(rpc.call_balance_view("TokenA", "Bob", "USDC"), 60);
        assert_eq!(rpc.get_transaction("tx1").unwrap().tx_hash, "tx1");
        assert_eq!(rpc.get_block(1).unwrap().header.height, 1);
    }

    #[test]
    fn rpc_subscription_polling_returns_block_receipt_and_event_notifications() {
        let mut rpc = seeded_rpc();
        let all_subscription = rpc.subscribe(vec![]);
        let event_subscription = rpc.subscribe(vec![SubscriptionTopic::Events]);

        assert_eq!(all_subscription.subscription_id, "sub-1");
        assert_eq!(
            all_subscription.topics,
            vec![
                SubscriptionTopic::Blocks,
                SubscriptionTopic::Receipts,
                SubscriptionTopic::Events,
                SubscriptionTopic::Finality,
            ]
        );
        assert_eq!(all_subscription.next_sequence, 1);
        assert_eq!(event_subscription.subscription_id, "sub-2");

        rpc.submit_transaction(transfer_tx()).unwrap();
        rpc.produce_block(1, 1_000).unwrap();

        let all_events = rpc
            .get_subscription_events(
                &all_subscription.subscription_id,
                all_subscription.next_sequence,
                10,
            )
            .unwrap();
        assert_eq!(all_events.events.len(), 3);
        assert_eq!(all_events.next_sequence, 4);
        assert_eq!(
            all_events
                .events
                .iter()
                .map(|event| event.topic)
                .collect::<Vec<_>>(),
            vec![
                SubscriptionTopic::Blocks,
                SubscriptionTopic::Receipts,
                SubscriptionTopic::Events,
            ]
        );
        assert!(matches!(
            &all_events.events[0].notification,
            SubscriptionNotification::Block(_)
        ));
        assert!(matches!(
            &all_events.events[1].notification,
            SubscriptionNotification::Receipt(_)
        ));
        assert!(matches!(
            &all_events.events[2].notification,
            SubscriptionNotification::Event(_)
        ));

        let event_only = rpc
            .get_subscription_events(
                &event_subscription.subscription_id,
                event_subscription.next_sequence,
                10,
            )
            .unwrap();
        assert_eq!(event_only.events.len(), 1);
        assert_eq!(event_only.events[0].topic, SubscriptionTopic::Events);
        assert_eq!(event_only.next_sequence, 4);
        assert_eq!(
            rpc.get_subscription_events("missing-subscription", 1, 10)
                .unwrap_err(),
            RpcError::SubscriptionNotFound
        );
    }

    #[test]
    fn rpc_subscription_polling_returns_finality_notifications() {
        let mut rpc = seeded_rpc();
        let subscription = rpc.subscribe(vec![SubscriptionTopic::Finality]);
        let certificate = FinalityCertificate {
            height: 7,
            block_hash: "block-hash-7".into(),
            signers: vec![
                "validator-1".into(),
                "validator-2".into(),
                "validator-3".into(),
            ],
        };

        rpc.publish_finality_certificate(certificate.clone());
        let page = rpc
            .get_subscription_events(
                &subscription.subscription_id,
                subscription.next_sequence,
                10,
            )
            .unwrap();

        assert_eq!(page.events.len(), 1);
        assert_eq!(page.events[0].topic, SubscriptionTopic::Finality);
        assert_eq!(page.next_sequence, 2);
        assert!(matches!(
            &page.events[0].notification,
            SubscriptionNotification::FinalityCertificate(observed) if **observed == certificate
        ));
    }

    #[test]
    fn rpc_returns_storage_proof_and_contract_descriptor() {
        let mut rpc = seeded_rpc();
        rpc.submit_transaction(transfer_tx()).unwrap();
        let block = rpc.produce_block(1, 1_000).unwrap();

        let key = StateKey::Balance {
            contract: "TokenA".into(),
            owner: "Bob".into(),
            asset: "USDC".into(),
        };
        let proof = rpc.get_storage_proof(&key).unwrap();
        let receipt_proof = rpc.get_receipt_proof(1, 0).unwrap();
        let event_proof = rpc.get_event_proof(0).unwrap();
        let event_page = rpc.get_events_page(0, 10);
        let capped_event_page = rpc.get_events_page(0, DEFAULT_MAX_EVENT_PAGE_SIZE + 1);
        let contract = rpc.get_contract("TokenA").unwrap();

        assert!(proof.verify());
        assert_eq!(proof.proof.root, block.header.storage_root);
        assert!(receipt_proof.verify());
        assert_eq!(receipt_proof.proof.root, block.header.receipt_root);
        assert!(event_proof.verify());
        assert_eq!(event_proof.proof.root, block.header.event_root);
        assert_eq!(event_page.events, rpc.get_events());
        assert_eq!(event_page.offset, 0);
        assert_eq!(event_page.limit, 10);
        assert_eq!(event_page.total_events, 1);
        assert_eq!(capped_event_page.limit, DEFAULT_MAX_EVENT_PAGE_SIZE);
        assert_eq!(contract.contract_id, "TokenA");
        assert!(contract.method_policy(&Method::Transfer).is_some());
        assert!(contract
            .declared_invariants()
            .contains(&ContractInvariant::TokenSupplyMatchesBalances));
        assert_eq!(rpc.get_state_root(), block.header.global_state_root);
    }

    #[test]
    fn rpc_returns_registered_aspect_modules() {
        let mut state = DeTTaState::new("detta-local");
        let module = aspect_module_record();
        let module_hash = module.module_hash.clone();
        state.register_aspect_module(module.clone()).unwrap();
        let mut rpc = RpcService::new(ValidatorNode::new("validator-1", state));

        assert_eq!(
            rpc.handle_request(RpcRequest::GetAspectModule {
                module_hash: module_hash.clone(),
            }),
            RpcResponse::Ok(RpcResult::AspectModule(Box::new(module.clone())))
        );
        let proof_response = rpc.handle_request(RpcRequest::GetAspectModuleProof {
            module_hash: module_hash.clone(),
        });
        let RpcResponse::Ok(RpcResult::AspectModuleProof(proof)) = proof_response else {
            panic!("expected aspect module proof");
        };
        assert!(proof.verify());
        assert_eq!(proof.proof.root, rpc.node().state().aspect_module_root());
        let artifact_response = rpc.handle_request(RpcRequest::GetAspectModuleArtifacts {
            module_hash: module_hash.clone(),
        });
        let RpcResponse::Ok(RpcResult::AspectModuleArtifacts(report)) = artifact_response else {
            panic!("expected aspect module artifact report");
        };
        assert_eq!(report.module_hash, module_hash);
        assert_eq!(report.module_id, "MinimalTransferToken");
        assert!(!report.has_canonical_source);
        assert!(!report.has_ir);
        assert!(report.abi.is_empty());
        assert!(report.policies.is_empty());
        assert!(report.storage_schema.is_empty());
        assert!(report.registry_schema.is_empty());
        assert!(report.invariants.is_empty());
        assert_eq!(
            rpc.handle_request(RpcRequest::GetAspectModules),
            RpcResponse::Ok(RpcResult::AspectModules(vec![module]))
        );
    }

    #[test]
    fn rpc_rejects_missing_aspect_module_query() {
        let mut rpc = seeded_rpc();

        assert_eq!(
            rpc.handle_request(RpcRequest::GetAspectModule {
                module_hash: "missing-module".into(),
            }),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.aspect_module_not_found".into(),
                message: "aspect module was not found".into(),
            })
        );
    }

    #[test]
    fn rpc_returns_upgrade_rehearsal_report() {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_token("TokenA", "USDC", vec![("Alice".into(), 100)])
            .unwrap();
        state
            .deploy_governance_with_timelock("GovA", "TokenA", "Admin", 2)
            .unwrap();
        let old_code_hash = state.code_hash("TokenA").unwrap().to_string();
        let storage_root = state.storage_root();
        let registry_root = state.registry_root();
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

        let mut rpc = RpcService::new(ValidatorNode::new("validator-1", state));
        assert_eq!(
            rpc.handle_request(RpcRequest::GetScheduledUpgrades),
            RpcResponse::Ok(RpcResult::ScheduledUpgrades(vec![ScheduledUpgrade {
                upgrade_id: "upgrade-1".into(),
                governance_contract: "GovA".into(),
                target_contract: "TokenA".into(),
                new_code_hash: "token-code-v2".into(),
                execute_after_height: 2,
                executed: false,
            }]))
        );
        assert_eq!(
            rpc.handle_request(RpcRequest::GetScheduledPolicyUpdates),
            RpcResponse::Ok(RpcResult::ScheduledPolicyUpdates(vec![
                ScheduledPolicyUpdate {
                    update_id: "policy-update-1".into(),
                    governance_contract: "GovA".into(),
                    target_contract: "TokenA".into(),
                    method: Method::Transfer,
                    effect: PolicyEffect::RegistryWrite,
                    execute_after_height: 2,
                    executed: false,
                },
            ]))
        );
        let response = rpc.handle_request(RpcRequest::GetUpgradeRehearsalReport {
            upgrade_id: "upgrade-1".into(),
        });
        let RpcResponse::Ok(RpcResult::UpgradeRehearsalReport(report)) = response else {
            panic!("expected upgrade rehearsal report");
        };

        assert_eq!(report.upgrade_id, "upgrade-1");
        assert_eq!(report.governance_contract, "GovA");
        assert_eq!(report.target_contract, "TokenA");
        assert_eq!(report.old_code_hash, old_code_hash);
        assert_eq!(report.new_code_hash, "token-code-v2");
        assert_eq!(report.execute_after_height, 2);
        assert!(!report.ready_at_height);
        assert_eq!(report.invariant_failures, vec![]);
        assert_eq!(report.storage_root, storage_root);
        assert_eq!(report.registry_root, registry_root);
        assert_eq!(
            rpc.node().state().code_hash("TokenA"),
            Some(old_code_hash.as_str())
        );
        let report_root = report.report_root();
        assert_eq!(report_root.len(), 64);
        assert!(report_root
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()));

        let missing = rpc.handle_request(RpcRequest::GetUpgradeRehearsalReport {
            upgrade_id: "missing-upgrade".into(),
        });
        assert_eq!(
            missing,
            RpcResponse::Error(RpcErrorBody {
                code: "execution.upgrade_not_found".into(),
                message: "upgrade was not found".into(),
            })
        );
    }

    #[test]
    fn json_rpc_dispatch_returns_stable_success_and_error_responses() {
        let mut rpc = seeded_rpc();

        let balance_response = rpc.handle_request(RpcRequest::GetBalance {
            contract: "TokenA".into(),
            owner: "Alice".into(),
            asset: "USDC".into(),
        });
        assert_eq!(balance_response, RpcResponse::Ok(RpcResult::Amount(100)));

        let missing_response = rpc.handle_request(RpcRequest::GetReceipt {
            tx_hash: "missing".into(),
        });
        assert_eq!(
            missing_response,
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.receipt_not_found".into(),
                message: "receipt was not found".into(),
            })
        );
        assert_eq!(
            RpcErrorBody::from(RpcError::Block(BlockError::ResourceLimitExceeded {
                max_units: 10,
                actual_units: 12,
            })),
            RpcErrorBody {
                code: "block.resource_limit_exceeded".into(),
                message: "block resource limit exceeded".into(),
            }
        );

        let malformed = rpc
            .handle_json_request(br#"{"method":"does_not_exist"}"#)
            .unwrap();
        let malformed_response: RpcResponse = serde_json::from_slice(&malformed).unwrap();
        match malformed_response {
            RpcResponse::Error(error) => assert_eq!(error.code, "rpc.decode_error"),
            response => panic!("expected decode error, got {response:?}"),
        }

        let authorization = SignedValidatorMessage {
            signer: "validator-1".into(),
            key_id: "consensus-key-1".into(),
            network_id: "detta-testnet".into(),
            chain_id: "detta-local".into(),
            domain: ValidatorSignatureDomain::ValidatorSetMetadataUpdate,
            message: Box::new(ProtocolMessage::ValidatorSetMetadataUpdate(
                ValidatorSetMetadataUpdate {
                    update_id: "validator-set-update-1".into(),
                    add_validators: vec![],
                    remove_validators: vec![],
                    expires_at_height: None,
                },
            )),
            signature_hex: "aa".repeat(64),
        };
        let unsupported_response =
            rpc.handle_request(RpcRequest::ProposeValidatorSetMetadataUpdate { authorization });
        assert_eq!(
            unsupported_response,
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.unsupported_node_method".into(),
                message: "method must be handled by a persistent validator node".into(),
            })
        );
        assert_eq!(
            rpc.handle_request(RpcRequest::GetPersistentNodeSnapshotRoots),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.unsupported_node_method".into(),
                message: "method must be handled by a persistent validator node".into(),
            })
        );
        assert_eq!(
            rpc.handle_request(RpcRequest::GetSnapshotMetadataRootStatus),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.unsupported_node_method".into(),
                message: "method must be handled by a persistent validator node".into(),
            })
        );
        assert_eq!(
            rpc.handle_request(RpcRequest::GetSnapshotSyncClientMetrics),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.unsupported_node_method".into(),
                message: "method must be handled by a persistent validator node".into(),
            })
        );
        assert_eq!(
            rpc.handle_request(RpcRequest::GetRequiredSnapshotMetadataRoots),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.unsupported_node_method".into(),
                message: "method must be handled by a persistent validator node".into(),
            })
        );
        assert_eq!(
            rpc.handle_request(RpcRequest::GetSnapshotImportAuditRecords {
                offset: 0,
                limit: 10,
            }),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.unsupported_node_method".into(),
                message: "method must be handled by a persistent validator node".into(),
            })
        );
        assert_eq!(
            rpc.handle_request(RpcRequest::GetSnapshotImportAuditRoot),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.unsupported_node_method".into(),
                message: "method must be handled by a persistent validator node".into(),
            })
        );
        assert_eq!(
            rpc.handle_request(RpcRequest::GetSnapshotImportAuditConfigRoot),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.unsupported_node_method".into(),
                message: "method must be handled by a persistent validator node".into(),
            })
        );
        assert_eq!(
            rpc.handle_request(RpcRequest::GetSnapshotImportAuditConfig),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.unsupported_node_method".into(),
                message: "method must be handled by a persistent validator node".into(),
            })
        );
        assert_eq!(
            rpc.handle_request(RpcRequest::GetSlashingRecord {
                validator_id: "validator-1".into(),
            }),
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.unsupported_node_method".into(),
                message: "method must be handled by a persistent validator node".into(),
            })
        );
    }

    #[test]
    fn json_rpc_decode_fuzz_smoke_rejects_malformed_corpus() {
        let valid_request = serde_json::to_vec(&RpcRequest::GetBalance {
            contract: "TokenA".into(),
            owner: "Alice".into(),
            asset: "USDC".into(),
        })
        .unwrap();
        let mut corpus = vec![
            Vec::new(),
            vec![0xff, b'{', b'}'],
            b"{".to_vec(),
            b"[]".to_vec(),
            b"null".to_vec(),
            b"true".to_vec(),
            b"42".to_vec(),
            br#""get_state_root""#.to_vec(),
            br#"{"method":"does_not_exist"}"#.to_vec(),
            br#"{"method":"get_balance"}"#.to_vec(),
            br#"{"method":"get_balance","params":{"contract":123,"owner":"Alice","asset":"USDC"}}"#
                .to_vec(),
            br#"{"method":"submit_transaction","params":null}"#.to_vec(),
            br#"{"method":"get_blocks_page","params":{"start_height":"one","limit":10}}"#
                .to_vec(),
            br#"{"method":"get_subscription_events","params":{"subscription_id":false,"from_sequence":0,"limit":10}}"#
                .to_vec(),
        ];
        for split in 0..valid_request.len() {
            corpus.push(valid_request[..split].to_vec());
        }

        for payload in corpus {
            let response = json_rpc_response_for_request(&payload, |_| {
                panic!("malformed RPC decode corpus unexpectedly reached handler")
            })
            .unwrap();
            let response: RpcResponse = serde_json::from_slice(&response).unwrap();

            match response {
                RpcResponse::Error(error) => assert_eq!(error.code, "rpc.decode_error"),
                response => panic!("expected decode error for {payload:?}, got {response:?}"),
            }
        }
    }

    #[test]
    fn json_rpc_decode_fuzz_smoke_accepts_valid_boundary_requests() {
        let mut rpc = seeded_rpc();
        let corpus = vec![
            RpcRequest::GetBalance {
                contract: "TokenA".into(),
                owner: "Alice".into(),
                asset: "USDC".into(),
            },
            RpcRequest::GetEventsPage {
                offset: usize::MAX,
                limit: usize::MAX,
            },
            RpcRequest::GetBlocksPage {
                start_height: u64::MAX,
                limit: usize::MAX,
            },
            RpcRequest::Subscribe {
                topics: vec![
                    SubscriptionTopic::Events,
                    SubscriptionTopic::Blocks,
                    SubscriptionTopic::Events,
                ],
            },
            RpcRequest::GetSubscriptionEvents {
                subscription_id: "missing-subscription".into(),
                from_sequence: u64::MAX,
                limit: usize::MAX,
            },
        ];

        for request in corpus {
            let payload = serde_json::to_vec(&request).unwrap();
            let response = rpc.handle_json_request(&payload).unwrap();
            let response: RpcResponse = serde_json::from_slice(&response).unwrap();

            if let RpcResponse::Error(error) = response {
                assert_ne!(error.code, "rpc.decode_error");
            }
        }
    }

    #[test]
    fn http_json_rpc_server_serves_json_rpc_post() {
        let body = serde_json::to_vec(&RpcRequest::GetBalance {
            contract: "TokenA".into(),
            owner: "Alice".into(),
            asset: "USDC".into(),
        })
        .unwrap();

        let (status, response) = http_round_trip(
            http_post_request("/rpc", &body),
            DEFAULT_MAX_RPC_REQUEST_BYTES,
        );

        assert_eq!(status, 200);
        assert_eq!(response, RpcResponse::Ok(RpcResult::Amount(100)));
    }

    #[test]
    fn http_json_rpc_server_returns_stable_transport_errors() {
        let method_not_allowed =
            b"GET /rpc HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n".to_vec();
        let (status, response) = http_round_trip(method_not_allowed, DEFAULT_MAX_RPC_REQUEST_BYTES);
        assert_eq!(status, 405);
        assert_eq!(
            response,
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.http_method_not_allowed".into(),
                message: "JSON RPC HTTP endpoint requires POST".into(),
            })
        );

        let oversized_body = serde_json::to_vec(&RpcRequest::GetStateRoot).unwrap();
        let (status, response) = http_round_trip(http_post_request("/rpc", &oversized_body), 8);
        assert_eq!(status, 413);
        assert_eq!(
            response,
            RpcResponse::Error(RpcErrorBody {
                code: "rpc.request_too_large".into(),
                message: "JSON RPC request body exceeded maximum size".into(),
            })
        );
    }

    #[test]
    fn json_rpc_tcp_server_serves_stateful_requests_over_socket() {
        let server = JsonRpcServer::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let server_thread = std::thread::spawn(move || {
            let mut rpc = seeded_rpc();
            server.serve_next_connection(&mut rpc).unwrap();
        });

        let mut stream = TcpStream::connect(addr).unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());

        write_request(
            &mut stream,
            &RpcRequest::Subscribe {
                topics: vec![
                    SubscriptionTopic::Blocks,
                    SubscriptionTopic::Receipts,
                    SubscriptionTopic::Events,
                ],
            },
        );
        let subscription = match read_response(&mut reader) {
            RpcResponse::Ok(RpcResult::SubscriptionStatus(subscription)) => {
                assert_eq!(subscription.subscription_id, "sub-1");
                assert_eq!(subscription.next_sequence, 1);
                subscription
            }
            response => panic!("expected subscription status, got {response:?}"),
        };

        write_request(
            &mut stream,
            &RpcRequest::SubmitTransaction {
                transaction: transfer_tx(),
            },
        );
        assert_eq!(
            read_response(&mut reader),
            RpcResponse::Ok(RpcResult::Submitted)
        );

        write_request(&mut stream, &RpcRequest::GetMempoolStatus);
        match read_response(&mut reader) {
            RpcResponse::Ok(RpcResult::MempoolStatus(status)) => {
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
            }
            response => panic!("expected mempool status, got {response:?}"),
        }

        write_request(
            &mut stream,
            &RpcRequest::ProduceBlock {
                height: 1,
                timestamp: 1_000,
            },
        );
        match read_response(&mut reader) {
            RpcResponse::Ok(RpcResult::Block(block)) => {
                assert_eq!(block.header.height, 1);
                assert_eq!(block.transactions.len(), 1);
            }
            response => panic!("expected produced block, got {response:?}"),
        }

        write_request(
            &mut stream,
            &RpcRequest::GetBlocksPage {
                start_height: 1,
                limit: 10,
            },
        );
        match read_response(&mut reader) {
            RpcResponse::Ok(RpcResult::BlocksPage(page)) => {
                assert_eq!(page.blocks.len(), 1);
                assert_eq!(page.blocks[0].header.height, 1);
                assert_eq!(page.start_height, 1);
                assert_eq!(page.limit, 10);
                assert_eq!(page.highest_height, 1);
            }
            response => panic!("expected block page, got {response:?}"),
        }

        write_request(
            &mut stream,
            &RpcRequest::GetSubscriptionEvents {
                subscription_id: subscription.subscription_id.clone(),
                from_sequence: subscription.next_sequence,
                limit: 10,
            },
        );
        match read_response(&mut reader) {
            RpcResponse::Ok(RpcResult::SubscriptionEvents(page)) => {
                assert_eq!(page.subscription_id, subscription.subscription_id);
                assert_eq!(page.events.len(), 3);
                assert_eq!(page.next_sequence, 4);
                assert_eq!(page.limit, 10);
                assert_eq!(
                    page.events
                        .iter()
                        .map(|event| event.topic)
                        .collect::<Vec<_>>(),
                    vec![
                        SubscriptionTopic::Blocks,
                        SubscriptionTopic::Receipts,
                        SubscriptionTopic::Events,
                    ]
                );
            }
            response => panic!("expected subscription events, got {response:?}"),
        }

        write_request(
            &mut stream,
            &RpcRequest::GetReceipt {
                tx_hash: "tx1".into(),
            },
        );
        match read_response(&mut reader) {
            RpcResponse::Ok(RpcResult::Receipt(receipt)) => {
                assert_eq!(receipt.status, TxStatus::Committed);
            }
            response => panic!("expected receipt, got {response:?}"),
        }

        write_request(
            &mut stream,
            &RpcRequest::GetReceiptProof {
                height: 1,
                index: 0,
            },
        );
        match read_response(&mut reader) {
            RpcResponse::Ok(RpcResult::ReceiptProof(proof)) => {
                assert!(proof.verify());
            }
            response => panic!("expected receipt proof, got {response:?}"),
        }

        write_request(&mut stream, &RpcRequest::GetEventProof { index: 0 });
        match read_response(&mut reader) {
            RpcResponse::Ok(RpcResult::EventProof(proof)) => {
                assert!(proof.verify());
            }
            response => panic!("expected event proof, got {response:?}"),
        }

        write_request(
            &mut stream,
            &RpcRequest::GetEventsPage {
                offset: 0,
                limit: 10,
            },
        );
        match read_response(&mut reader) {
            RpcResponse::Ok(RpcResult::EventsPage(page)) => {
                assert_eq!(page.events.len(), 1);
                assert_eq!(page.offset, 0);
                assert_eq!(page.limit, 10);
                assert_eq!(page.total_events, 1);
            }
            response => panic!("expected event page, got {response:?}"),
        }

        write_request(
            &mut stream,
            &RpcRequest::GetBalance {
                contract: "TokenA".into(),
                owner: "Bob".into(),
                asset: "USDC".into(),
            },
        );
        assert_eq!(
            read_response(&mut reader),
            RpcResponse::Ok(RpcResult::Amount(60))
        );

        stream.shutdown(Shutdown::Write).unwrap();
        server_thread.join().unwrap();
    }
}
