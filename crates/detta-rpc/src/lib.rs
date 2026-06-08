use detta_core::{
    Amount, AssetId, Block, BlockError, ContractId, ContractRecord, Event, GrantKey, MempoolError,
    OutboxMessageProof, Principal, Receipt, RegistryNonInclusionProof, RegistryProof, StateKey,
    StateSnapshot, StorageNonInclusionProof, StorageProof, Transaction, ValidatorNode,
};
use detta_protocol::SignedValidatorMessage;
use detta_storage::{
    SnapshotImportAuditConfig, SnapshotImportAuditRecord, ValidatorSetMetadataAuditRecord,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{self, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, ToSocketAddrs};

pub const DEFAULT_MAX_RPC_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RpcError {
    Mempool(MempoolError),
    Block(BlockError),
    BlockNotFound,
    ReceiptNotFound,
    TransactionNotFound,
    ContractNotFound,
    ProofNotFound,
    UnsupportedNodeMethod,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", content = "params", rename_all = "snake_case")]
pub enum RpcRequest {
    SubmitTransaction {
        transaction: Transaction,
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
    GetBlock {
        height: u64,
    },
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
    GetEvents,
    GetContract {
        contract: ContractId,
    },
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
pub struct RequiredSnapshotMetadataRootsReport {
    pub roots: BTreeMap<String, String>,
    pub root: String,
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
#[serde(tag = "result", content = "data", rename_all = "snake_case")]
pub enum RpcResult {
    Submitted,
    Imported,
    Block(Box<Block>),
    Transaction(Box<Transaction>),
    Receipt(Box<Receipt>),
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
    Events(Vec<Event>),
    Contract(Box<ContractRecord>),
    ValidatorSetMetadataUpdateStatus(ValidatorSetMetadataUpdateStatus),
    ValidatorSetMetadataAuditRecords(Vec<ValidatorSetMetadataAuditRecord>),
    SnapshotImportAuditRecords(Vec<SnapshotImportAuditRecord>),
    SnapshotImportAuditRoot(String),
    SnapshotImportAuditConfigRoot(Option<String>),
    SnapshotImportAuditConfig(SnapshotImportAuditConfig),
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

pub trait JsonRpcHandler {
    fn handle_json_request(&mut self, request: &[u8]) -> Result<Vec<u8>, RpcTransportError>;
}

pub struct RpcService {
    node: ValidatorNode,
}

impl RpcService {
    pub fn new(node: ValidatorNode) -> Self {
        Self { node }
    }

    pub fn node(&self) -> &ValidatorNode {
        &self.node
    }

    pub fn submit_transaction(&mut self, tx: Transaction) -> Result<(), RpcError> {
        self.node.submit_transaction(tx).map_err(RpcError::Mempool)
    }

    pub fn produce_block(&mut self, height: u64, timestamp: u64) -> Result<Block, RpcError> {
        let block = self.node.propose_pending_block(height, timestamp);
        self.node
            .validate_and_apply(&block)
            .map_err(RpcError::Block)?;
        Ok(block)
    }

    pub fn import_block(&mut self, block: &Block) -> Result<(), RpcError> {
        self.node.validate_and_apply(block).map_err(RpcError::Block)
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

    pub fn get_block(&self, height: u64) -> Result<Block, RpcError> {
        self.node
            .get_block(height)
            .cloned()
            .ok_or(RpcError::BlockNotFound)
    }

    pub fn get_state_root(&self) -> String {
        self.node.state().global_state_root()
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

    pub fn get_events(&self) -> Vec<Event> {
        self.node.state().events().to_vec()
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

    pub fn handle_request(&mut self, request: RpcRequest) -> RpcResponse {
        match request {
            RpcRequest::SubmitTransaction { transaction } => self
                .submit_transaction(transaction)
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
            RpcRequest::GetBlock { height } => self
                .get_block(height)
                .map(|block| RpcResult::Block(Box::new(block)))
                .into(),
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
            RpcRequest::GetEvents => RpcResponse::Ok(RpcResult::Events(self.get_events())),
            RpcRequest::GetContract { contract } => self
                .get_contract(contract)
                .map(|contract| RpcResult::Contract(Box::new(contract)))
                .into(),
            RpcRequest::ProposeValidatorSetMetadataUpdate { .. }
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
        RpcError::Mempool(MempoolError::DuplicateTransaction) => "mempool.duplicate_transaction",
        RpcError::Mempool(MempoolError::NonceAlreadyUsed) => "mempool.nonce_already_used",
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
        RpcError::BlockNotFound => "rpc.block_not_found",
        RpcError::ReceiptNotFound => "rpc.receipt_not_found",
        RpcError::TransactionNotFound => "rpc.transaction_not_found",
        RpcError::ContractNotFound => "rpc.contract_not_found",
        RpcError::ProofNotFound => "rpc.proof_not_found",
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
        RpcError::Mempool(MempoolError::DuplicateTransaction) => "transaction is already pending",
        RpcError::Mempool(MempoolError::NonceAlreadyUsed) => {
            "transaction nonce has already been committed"
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
        RpcError::BlockNotFound => "block was not found",
        RpcError::ReceiptNotFound => "receipt was not found",
        RpcError::TransactionNotFound => "transaction was not found",
        RpcError::ContractNotFound => "contract was not found",
        RpcError::ProofNotFound => "proof was not found",
        RpcError::UnsupportedNodeMethod => "method must be handled by a persistent validator node",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Argument, ContractInvariant, DeTTaState, Method, TxStatus};
    use detta_protocol::{ProtocolMessage, ValidatorSetMetadataUpdate, ValidatorSignatureDomain};
    use std::io::{BufRead, BufReader, Write};
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

    #[test]
    fn rpc_submits_transaction_produces_block_and_returns_receipt() {
        let mut rpc = seeded_rpc();
        rpc.submit_transaction(transfer_tx()).unwrap();

        let block = rpc.produce_block(1, 1_000).unwrap();
        let receipt = rpc.get_receipt("tx1").unwrap();

        assert_eq!(block.transactions.len(), 1);
        assert_eq!(receipt.status, TxStatus::Committed);
        assert_eq!(rpc.call_balance_view("TokenA", "Alice", "USDC"), 90);
        assert_eq!(rpc.call_balance_view("TokenA", "Bob", "USDC"), 60);
        assert_eq!(rpc.get_transaction("tx1").unwrap().tx_hash, "tx1");
        assert_eq!(rpc.get_block(1).unwrap().header.height, 1);
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
        let contract = rpc.get_contract("TokenA").unwrap();

        assert!(proof.verify());
        assert_eq!(proof.proof.root, block.header.storage_root);
        assert_eq!(contract.contract_id, "TokenA");
        assert!(contract.method_policy(&Method::Transfer).is_some());
        assert!(contract
            .declared_invariants()
            .contains(&ContractInvariant::TokenSupplyMatchesBalances));
        assert_eq!(rpc.get_state_root(), block.header.global_state_root);
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
            &RpcRequest::SubmitTransaction {
                transaction: transfer_tx(),
            },
        );
        assert_eq!(
            read_response(&mut reader),
            RpcResponse::Ok(RpcResult::Submitted)
        );

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
