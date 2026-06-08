use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub type ChainId = String;
pub type ContractId = String;
pub type Principal = String;
pub type AssetId = String;
pub type TxHash = String;
pub type Nonce = u64;
pub type Amount = u128;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Method {
    Transfer,
    Approve,
    TransferFrom,
    Permit,
    AddLiquidity,
    Swap,
    SubmitPrice,
    RedeemBridgeMessage,
    PauseContract,
    UnpauseContract,
    Other(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Argument {
    Principal(Principal),
    Asset(AssetId),
    Amount(Amount),
    Text(String),
    Certificate(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum StateKey {
    Balance {
        contract: ContractId,
        owner: Principal,
        asset: AssetId,
    },
    TotalSupply {
        contract: ContractId,
        asset: AssetId,
    },
    Reserve {
        contract: ContractId,
        asset: AssetId,
    },
    LpSupply {
        contract: ContractId,
    },
    LpBalance {
        contract: ContractId,
        owner: Principal,
    },
    OraclePrice {
        contract: ContractId,
        asset: AssetId,
    },
    OracleTimestamp {
        contract: ContractId,
        asset: AssetId,
    },
    BridgeMessageConsumed {
        contract: ContractId,
        message_id: String,
    },
}

impl StateKey {
    fn owner_contract(&self) -> &ContractId {
        match self {
            StateKey::Balance { contract, .. } => contract,
            StateKey::TotalSupply { contract, .. } => contract,
            StateKey::Reserve { contract, .. } => contract,
            StateKey::LpSupply { contract } => contract,
            StateKey::LpBalance { contract, .. } => contract,
            StateKey::OraclePrice { contract, .. } => contract,
            StateKey::OracleTimestamp { contract, .. } => contract,
            StateKey::BridgeMessageConsumed { contract, .. } => contract,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum StateValue {
    UInt(Amount),
}

impl StateValue {
    fn as_uint(&self) -> Amount {
        match self {
            StateValue::UInt(value) => *value,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum GrantRight {
    SpendAllowance,
    UpdateOracle,
    GovernanceAdmin,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum GrantKey {
    Allowance {
        contract: ContractId,
        owner: Principal,
        spender: Principal,
        asset: AssetId,
    },
    OracleUpdater {
        contract: ContractId,
        subject: Principal,
    },
    GovernanceAdmin {
        contract: ContractId,
        subject: Principal,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Grant {
    pub issuer: Principal,
    pub subject: Principal,
    pub rights: BTreeSet<GrantRight>,
    pub limit: Option<Amount>,
    pub spent: Amount,
    pub active: bool,
    pub revoked: bool,
}

impl Grant {
    fn allowance(owner: Principal, spender: Principal, amount: Amount) -> Self {
        Self {
            issuer: owner,
            subject: spender,
            rights: BTreeSet::from([GrantRight::SpendAllowance]),
            limit: Some(amount),
            spent: 0,
            active: true,
            revoked: false,
        }
    }

    fn oracle_updater(subject: Principal) -> Self {
        Self {
            issuer: subject.clone(),
            subject,
            rights: BTreeSet::from([GrantRight::UpdateOracle]),
            limit: None,
            spent: 0,
            active: true,
            revoked: false,
        }
    }

    fn governance_admin(subject: Principal) -> Self {
        Self {
            issuer: subject.clone(),
            subject,
            rights: BTreeSet::from([GrantRight::GovernanceAdmin]),
            limit: None,
            spent: 0,
            active: true,
            revoked: false,
        }
    }

    fn remaining(&self) -> Amount {
        self.limit
            .map(|limit| limit.saturating_sub(self.spent))
            .unwrap_or(Amount::MAX)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContractRecord {
    pub contract_id: ContractId,
    pub code_hash: String,
    pub kind: ContractKind,
    exported_methods: BTreeSet<Method>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ContractKind {
    Token,
    AmmPool { asset_a: AssetId, asset_b: AssetId },
    Oracle { asset: AssetId, max_age: u64 },
    Bridge { source_chain: ChainId },
    Governance { governed_contract: ContractId },
}

impl ContractRecord {
    fn token(contract_id: ContractId, code_hash: String) -> Self {
        Self {
            contract_id,
            code_hash,
            kind: ContractKind::Token,
            exported_methods: BTreeSet::from([
                Method::Transfer,
                Method::Approve,
                Method::TransferFrom,
                Method::Permit,
            ]),
        }
    }

    fn amm_pool(
        contract_id: ContractId,
        code_hash: String,
        asset_a: AssetId,
        asset_b: AssetId,
    ) -> Self {
        Self {
            contract_id,
            code_hash,
            kind: ContractKind::AmmPool { asset_a, asset_b },
            exported_methods: BTreeSet::from([Method::AddLiquidity, Method::Swap]),
        }
    }

    fn oracle(contract_id: ContractId, code_hash: String, asset: AssetId, max_age: u64) -> Self {
        Self {
            contract_id,
            code_hash,
            kind: ContractKind::Oracle { asset, max_age },
            exported_methods: BTreeSet::from([Method::SubmitPrice]),
        }
    }

    fn bridge(contract_id: ContractId, code_hash: String, source_chain: ChainId) -> Self {
        Self {
            contract_id,
            code_hash,
            kind: ContractKind::Bridge { source_chain },
            exported_methods: BTreeSet::from([Method::RedeemBridgeMessage]),
        }
    }

    fn governance(
        contract_id: ContractId,
        code_hash: String,
        governed_contract: ContractId,
    ) -> Self {
        Self {
            contract_id,
            code_hash,
            kind: ContractKind::Governance { governed_contract },
            exported_methods: BTreeSet::from([Method::PauseContract, Method::UnpauseContract]),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Transaction {
    pub chain_id: ChainId,
    pub tx_hash: TxHash,
    pub sender: Principal,
    pub nonce: Nonce,
    pub target: ContractId,
    pub method: Method,
    pub args: Vec<Argument>,
    pub signature_ok: bool,
    pub budget: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EventPayload {
    Transfer {
        from: Principal,
        to: Principal,
        asset: AssetId,
        amount: Amount,
    },
    Approval {
        owner: Principal,
        spender: Principal,
        asset: AssetId,
        amount: Amount,
    },
    LiquidityAdded {
        provider: Principal,
        asset_a: AssetId,
        amount_a: Amount,
        asset_b: AssetId,
        amount_b: Amount,
        minted_lp: Amount,
    },
    Swap {
        trader: Principal,
        input_asset: AssetId,
        output_asset: AssetId,
        amount_in: Amount,
        amount_out: Amount,
    },
    PriceUpdated {
        updater: Principal,
        asset: AssetId,
        price: Amount,
        timestamp: u64,
    },
    BridgeMessageRedeemed {
        message_id: String,
        recipient: Principal,
        asset: AssetId,
        amount: Amount,
    },
    ContractPaused {
        contract: ContractId,
        admin: Principal,
    },
    ContractUnpaused {
        contract: ContractId,
        admin: Principal,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub contract: ContractId,
    pub tx_hash: TxHash,
    pub index: u64,
    pub payload: EventPayload,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TxStatus {
    Committed,
    Reverted,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExecutionError {
    ChainMismatch,
    InvalidSignature,
    NonceReplay,
    ContractNotFound,
    MethodNotExported,
    InvalidArguments,
    PolicyMissing,
    RegistryGrantMissing,
    RegistryGrantInactive,
    RegistryGrantRevoked,
    RegistryRightMissing,
    AllowanceExceeded,
    InsufficientBalance,
    InsufficientLiquidity,
    SlippageExceeded,
    InvalidPoolAsset,
    UnauthorizedOracleUpdater,
    StaleOraclePrice,
    InvalidBridgeMessage,
    BridgeMessageReplay,
    UnauthorizedGovernance,
    ContractPaused,
    ArithmeticOverflow,
    WriteScopeViolation,
    ContractIsolationViolation,
    InvariantViolation,
    InvalidCertificate,
    CertificateReplay,
    ForbiddenPrimitive,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BlockError {
    ChainMismatch,
    PreviousBlockMismatch,
    TxRootMismatch,
    ReceiptRootMismatch,
    ReceiptMismatch,
    StorageRootMismatch,
    RegistryRootMismatch,
    EventRootMismatch,
    NonceRootMismatch,
    GlobalStateRootMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MempoolError {
    ChainMismatch,
    InvalidSignature,
    DuplicateTransaction,
    NonceAlreadyUsed,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Mempool {
    pending: Vec<Transaction>,
    tx_hashes: BTreeSet<TxHash>,
}

impl Mempool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn submit(&mut self, state: &DeTTaState, tx: Transaction) -> Result<(), MempoolError> {
        if tx.chain_id != state.chain_id {
            return Err(MempoolError::ChainMismatch);
        }
        if !tx.signature_ok {
            return Err(MempoolError::InvalidSignature);
        }
        if self.tx_hashes.contains(&tx.tx_hash) {
            return Err(MempoolError::DuplicateTransaction);
        }
        if state.used_nonces.contains(&(tx.sender.clone(), tx.nonce)) {
            return Err(MempoolError::NonceAlreadyUsed);
        }

        self.tx_hashes.insert(tx.tx_hash.clone());
        self.pending.push(tx);
        Ok(())
    }

    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }

    pub fn drain_ordered(&mut self) -> Vec<Transaction> {
        let mut transactions = std::mem::take(&mut self.pending);
        self.tx_hashes.clear();
        transactions.sort_by(|left, right| {
            (left.sender.as_str(), left.nonce, left.tx_hash.as_str()).cmp(&(
                right.sender.as_str(),
                right.nonce,
                right.tx_hash.as_str(),
            ))
        });
        transactions
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SnapshotError {
    StorageRootMismatch,
    RegistryRootMismatch,
    EventRootMismatch,
    NonceRootMismatch,
    GlobalStateRootMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub state: DeTTaState,
    pub storage_root: String,
    pub registry_root: String,
    pub event_root: String,
    pub nonce_root: String,
    pub global_state_root: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ReturnValue {
    Unit,
    UInt(Amount),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EvaluatorPrimitive {
    StateGet,
    StateSet,
    RegistryGet,
    RegistrySetGuarded,
    EmitEvent,
    CallContract,
    Abort,
    PureArithmetic,
    PureComparison,
    PureData,
    RawAddAtom,
    RawRemoveAtom,
    RawPrivateMatch,
    PrologAssert,
    PrologRetract,
    PythonCall,
    FileSystemAccess,
    ProcessExecution,
    NetworkAccess,
    WallClockAccess,
    Randomness,
    ArbitraryImport,
}

impl EvaluatorPrimitive {
    pub fn allowed_in_contracts(&self) -> bool {
        matches!(
            self,
            EvaluatorPrimitive::StateGet
                | EvaluatorPrimitive::StateSet
                | EvaluatorPrimitive::RegistryGet
                | EvaluatorPrimitive::RegistrySetGuarded
                | EvaluatorPrimitive::EmitEvent
                | EvaluatorPrimitive::CallContract
                | EvaluatorPrimitive::Abort
                | EvaluatorPrimitive::PureArithmetic
                | EvaluatorPrimitive::PureComparison
                | EvaluatorPrimitive::PureData
        )
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RestrictedEvaluator;

impl RestrictedEvaluator {
    pub fn validate_primitive(&self, primitive: &EvaluatorPrimitive) -> Result<(), ExecutionError> {
        if primitive.allowed_in_contracts() {
            Ok(())
        } else {
            Err(ExecutionError::ForbiddenPrimitive)
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Receipt {
    pub tx_hash: TxHash,
    pub status: TxStatus,
    pub error: Option<ExecutionError>,
    pub return_value: Option<ReturnValue>,
    pub storage_root_after: String,
    pub registry_root_after: String,
    pub event_root_after: String,
    pub nonce_root_after: String,
    pub global_state_root_after: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BlockHeader {
    pub chain_id: ChainId,
    pub height: u64,
    pub previous_block_hash: String,
    pub tx_root: String,
    pub receipt_root: String,
    pub global_state_root: String,
    pub storage_root: String,
    pub registry_root: String,
    pub event_root: String,
    pub nonce_root: String,
    pub timestamp: u64,
    pub proposer: String,
    pub consensus_certificate: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    pub receipts: Vec<Receipt>,
}

impl Block {
    pub fn block_hash(&self) -> String {
        root_of(&self.header)
    }

    pub fn receipt_proof(&self, index: usize) -> Option<ReceiptProof> {
        let receipt = self.receipts.get(index)?.clone();
        let proof = merkle_proof(&self.receipts, index)?;
        Some(ReceiptProof { receipt, proof })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SiblingSide {
    Left,
    Right,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MerkleStep {
    pub side: SiblingSide,
    pub hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MerkleProof {
    pub root: String,
    pub leaf_hash: String,
    pub index: usize,
    pub leaf_count: usize,
    pub path: Vec<MerkleStep>,
}

impl MerkleProof {
    pub fn verify<T: Serialize>(&self, value: &T) -> bool {
        if self.leaf_hash != hex_lower(&merkle_leaf(value)) {
            return false;
        }

        let mut current = match hex_to_bytes(&self.leaf_hash) {
            Some(bytes) => bytes,
            None => return false,
        };

        for step in &self.path {
            let sibling = match hex_to_bytes(&step.hash) {
                Some(bytes) => bytes,
                None => return false,
            };
            current = match step.side {
                SiblingSide::Left => merkle_node(&sibling, &current),
                SiblingSide::Right => merkle_node(&current, &sibling),
            };
        }

        hex_lower(&current) == self.root
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StorageProof {
    pub key: StateKey,
    pub value: StateValue,
    pub proof: MerkleProof,
}

impl StorageProof {
    pub fn verify(&self) -> bool {
        self.proof.verify(&(self.key.clone(), self.value.clone()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StorageNonInclusionProof {
    pub key: StateKey,
    pub root: String,
    pub leaf_count: usize,
    pub predecessor: Option<StorageProof>,
    pub successor: Option<StorageProof>,
}

impl StorageNonInclusionProof {
    pub fn verify(&self) -> bool {
        verify_non_inclusion(
            &self.key,
            &self.root,
            self.leaf_count,
            self.predecessor.as_ref().map(|proof| {
                (
                    &proof.key,
                    &proof.proof,
                    proof.verify() && proof.proof.root == self.root,
                )
            }),
            self.successor.as_ref().map(|proof| {
                (
                    &proof.key,
                    &proof.proof,
                    proof.verify() && proof.proof.root == self.root,
                )
            }),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegistryProof {
    pub key: GrantKey,
    pub grant: Grant,
    pub proof: MerkleProof,
}

impl RegistryProof {
    pub fn verify(&self) -> bool {
        self.proof.verify(&(self.key.clone(), self.grant.clone()))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RegistryNonInclusionProof {
    pub key: GrantKey,
    pub root: String,
    pub leaf_count: usize,
    pub predecessor: Option<RegistryProof>,
    pub successor: Option<RegistryProof>,
}

impl RegistryNonInclusionProof {
    pub fn verify(&self) -> bool {
        verify_non_inclusion(
            &self.key,
            &self.root,
            self.leaf_count,
            self.predecessor.as_ref().map(|proof| {
                (
                    &proof.key,
                    &proof.proof,
                    proof.verify() && proof.proof.root == self.root,
                )
            }),
            self.successor.as_ref().map(|proof| {
                (
                    &proof.key,
                    &proof.proof,
                    proof.verify() && proof.proof.root == self.root,
                )
            }),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventProof {
    pub event: Event,
    pub proof: MerkleProof,
}

impl EventProof {
    pub fn verify(&self) -> bool {
        self.proof.verify(&self.event)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReceiptProof {
    pub receipt: Receipt,
    pub proof: MerkleProof,
}

impl ReceiptProof {
    pub fn verify(&self) -> bool {
        self.proof.verify(&self.receipt)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatorNode {
    validator_id: String,
    state: DeTTaState,
    mempool: Mempool,
}

impl ValidatorNode {
    pub fn new(validator_id: impl Into<String>, state: DeTTaState) -> Self {
        Self {
            validator_id: validator_id.into(),
            state,
            mempool: Mempool::new(),
        }
    }

    pub fn state(&self) -> &DeTTaState {
        &self.state
    }

    pub fn pending_len(&self) -> usize {
        self.mempool.pending_len()
    }

    pub fn submit_transaction(&mut self, tx: Transaction) -> Result<(), MempoolError> {
        self.mempool.submit(&self.state, tx)
    }

    pub fn propose_block(
        &self,
        height: u64,
        transactions: Vec<Transaction>,
        timestamp: u64,
    ) -> Block {
        let certificate = format!("sim-cert:{}:{}", self.validator_id, height);
        self.state
            .build_block(
                height,
                transactions,
                timestamp,
                &self.validator_id,
                certificate,
            )
            .0
    }

    pub fn propose_pending_block(&mut self, height: u64, timestamp: u64) -> Block {
        let transactions = self.mempool.drain_ordered();
        self.propose_block(height, transactions, timestamp)
    }

    pub fn validate_and_apply(&mut self, block: &Block) -> Result<(), BlockError> {
        self.state.apply_block(block)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AuthorizedFrame {
    contract: ContractId,
    msg_sender: Principal,
    write_scope: BTreeSet<StateKey>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeTTaState {
    chain_id: ChainId,
    height: u64,
    finalized_block_hash: String,
    contracts: BTreeMap<ContractId, ContractRecord>,
    storage: BTreeMap<StateKey, StateValue>,
    registry: BTreeMap<GrantKey, Grant>,
    used_nonces: BTreeSet<(Principal, Nonce)>,
    used_certificate_nonces: BTreeSet<String>,
    paused_contracts: BTreeSet<ContractId>,
    events: Vec<Event>,
}

impl DeTTaState {
    pub fn new(chain_id: impl Into<ChainId>) -> Self {
        Self {
            chain_id: chain_id.into(),
            height: 0,
            finalized_block_hash: "genesis".into(),
            contracts: BTreeMap::new(),
            storage: BTreeMap::new(),
            registry: BTreeMap::new(),
            used_nonces: BTreeSet::new(),
            used_certificate_nonces: BTreeSet::new(),
            paused_contracts: BTreeSet::new(),
            events: Vec::new(),
        }
    }

    pub fn from_snapshot(snapshot: StateSnapshot) -> Result<Self, SnapshotError> {
        if snapshot.storage_root != snapshot.state.storage_root() {
            return Err(SnapshotError::StorageRootMismatch);
        }
        if snapshot.registry_root != snapshot.state.registry_root() {
            return Err(SnapshotError::RegistryRootMismatch);
        }
        if snapshot.event_root != snapshot.state.event_root() {
            return Err(SnapshotError::EventRootMismatch);
        }
        if snapshot.nonce_root != snapshot.state.nonce_root() {
            return Err(SnapshotError::NonceRootMismatch);
        }
        if snapshot.global_state_root != snapshot.state.global_state_root() {
            return Err(SnapshotError::GlobalStateRootMismatch);
        }
        Ok(snapshot.state)
    }

    pub fn snapshot(&self) -> StateSnapshot {
        StateSnapshot {
            state: self.clone(),
            storage_root: self.storage_root(),
            registry_root: self.registry_root(),
            event_root: self.event_root(),
            nonce_root: self.nonce_root(),
            global_state_root: self.global_state_root(),
        }
    }

    pub fn deploy_token(
        &mut self,
        contract: impl Into<ContractId>,
        asset: impl Into<AssetId>,
        balances: Vec<(Principal, Amount)>,
    ) -> Result<(), ExecutionError> {
        let contract = contract.into();
        let asset = asset.into();
        let code_hash = root_of(&("detta-token-v1", &contract));
        let mut total = 0u128;

        self.contracts.insert(
            contract.clone(),
            ContractRecord::token(contract.clone(), code_hash),
        );

        for (owner, amount) in balances {
            total = total
                .checked_add(amount)
                .ok_or(ExecutionError::ArithmeticOverflow)?;
            self.storage.insert(
                StateKey::Balance {
                    contract: contract.clone(),
                    owner,
                    asset: asset.clone(),
                },
                StateValue::UInt(amount),
            );
        }

        self.storage.insert(
            StateKey::TotalSupply {
                contract: contract.clone(),
                asset: asset.clone(),
            },
            StateValue::UInt(total),
        );

        if !self.token_invariants_hold(&contract, &asset) {
            return Err(ExecutionError::InvariantViolation);
        }

        Ok(())
    }

    pub fn deploy_amm_pool(
        &mut self,
        contract: impl Into<ContractId>,
        asset_a: impl Into<AssetId>,
        asset_b: impl Into<AssetId>,
    ) -> Result<(), ExecutionError> {
        let contract = contract.into();
        let asset_a = asset_a.into();
        let asset_b = asset_b.into();
        if asset_a == asset_b {
            return Err(ExecutionError::InvalidPoolAsset);
        }

        let code_hash = root_of(&("detta-amm-v1", &contract, &asset_a, &asset_b));
        self.contracts.insert(
            contract.clone(),
            ContractRecord::amm_pool(
                contract.clone(),
                code_hash,
                asset_a.clone(),
                asset_b.clone(),
            ),
        );
        self.storage.insert(
            StateKey::Reserve {
                contract: contract.clone(),
                asset: asset_a,
            },
            StateValue::UInt(0),
        );
        self.storage.insert(
            StateKey::Reserve {
                contract: contract.clone(),
                asset: asset_b,
            },
            StateValue::UInt(0),
        );
        self.storage.insert(
            StateKey::LpSupply {
                contract: contract.clone(),
            },
            StateValue::UInt(0),
        );

        if !self.amm_invariants_hold(&contract) {
            return Err(ExecutionError::InvariantViolation);
        }

        Ok(())
    }

    pub fn deploy_oracle(
        &mut self,
        contract: impl Into<ContractId>,
        asset: impl Into<AssetId>,
        updater: impl Into<Principal>,
        max_age: u64,
    ) -> Result<(), ExecutionError> {
        let contract = contract.into();
        let asset = asset.into();
        let updater = updater.into();
        let code_hash = root_of(&("detta-oracle-v1", &contract, &asset, max_age));

        self.contracts.insert(
            contract.clone(),
            ContractRecord::oracle(contract.clone(), code_hash, asset.clone(), max_age),
        );
        self.storage.insert(
            StateKey::OraclePrice {
                contract: contract.clone(),
                asset: asset.clone(),
            },
            StateValue::UInt(0),
        );
        self.storage.insert(
            StateKey::OracleTimestamp {
                contract: contract.clone(),
                asset,
            },
            StateValue::UInt(0),
        );
        self.registry.insert(
            GrantKey::OracleUpdater {
                contract,
                subject: updater.clone(),
            },
            Grant::oracle_updater(updater),
        );

        Ok(())
    }

    pub fn deploy_bridge(
        &mut self,
        contract: impl Into<ContractId>,
        source_chain: impl Into<ChainId>,
    ) -> Result<(), ExecutionError> {
        let contract = contract.into();
        let source_chain = source_chain.into();
        let code_hash = root_of(&("detta-bridge-v1", &contract, &source_chain));
        self.contracts.insert(
            contract.clone(),
            ContractRecord::bridge(contract, code_hash, source_chain),
        );
        Ok(())
    }

    pub fn deploy_governance(
        &mut self,
        contract: impl Into<ContractId>,
        governed_contract: impl Into<ContractId>,
        admin: impl Into<Principal>,
    ) -> Result<(), ExecutionError> {
        let contract = contract.into();
        let governed_contract = governed_contract.into();
        let admin = admin.into();
        let code_hash = root_of(&("detta-governance-v1", &contract, &governed_contract));
        self.contracts.insert(
            contract.clone(),
            ContractRecord::governance(contract.clone(), code_hash, governed_contract),
        );
        self.registry.insert(
            GrantKey::GovernanceAdmin {
                contract,
                subject: admin.clone(),
            },
            Grant::governance_admin(admin),
        );
        Ok(())
    }

    pub fn apply_transaction(&mut self, tx: Transaction) -> Receipt {
        if tx.chain_id != self.chain_id {
            return self.rejected_receipt(tx.tx_hash, ExecutionError::ChainMismatch);
        }

        if !tx.signature_ok {
            return self.rejected_receipt(tx.tx_hash, ExecutionError::InvalidSignature);
        }

        let nonce_key = (tx.sender.clone(), tx.nonce);
        if self.used_nonces.contains(&nonce_key) {
            return self.rejected_receipt(tx.tx_hash, ExecutionError::NonceReplay);
        }

        self.used_nonces.insert(nonce_key);

        let checkpoint_storage = self.storage.clone();
        let checkpoint_registry = self.registry.clone();
        let checkpoint_events = self.events.clone();
        let checkpoint_certificate_nonces = self.used_certificate_nonces.clone();
        let checkpoint_paused_contracts = self.paused_contracts.clone();

        match self.execute_call(&tx) {
            Ok(return_value) => self.committed_receipt(tx.tx_hash, return_value),
            Err(error) => {
                self.storage = checkpoint_storage;
                self.registry = checkpoint_registry;
                self.events = checkpoint_events;
                self.used_certificate_nonces = checkpoint_certificate_nonces;
                self.paused_contracts = checkpoint_paused_contracts;
                self.reverted_receipt(tx.tx_hash, error)
            }
        }
    }

    pub fn build_block(
        &self,
        height: u64,
        transactions: Vec<Transaction>,
        timestamp: u64,
        proposer: impl Into<String>,
        consensus_certificate: impl Into<String>,
    ) -> (Block, DeTTaState) {
        let mut working_state = self.clone();
        working_state.height = height;

        let receipts: Vec<_> = transactions
            .iter()
            .cloned()
            .map(|tx| working_state.apply_transaction(tx))
            .collect();

        let header = BlockHeader {
            chain_id: self.chain_id.clone(),
            height,
            previous_block_hash: self.finalized_block_hash.clone(),
            tx_root: merkle_root(&transactions),
            receipt_root: merkle_root(&receipts),
            global_state_root: working_state.global_state_root(),
            storage_root: working_state.storage_root(),
            registry_root: working_state.registry_root(),
            event_root: working_state.event_root(),
            nonce_root: working_state.nonce_root(),
            timestamp,
            proposer: proposer.into(),
            consensus_certificate: consensus_certificate.into(),
        };

        let block = Block {
            header,
            transactions,
            receipts,
        };

        working_state.finalized_block_hash = block.block_hash();
        (block, working_state)
    }

    pub fn apply_block(&mut self, block: &Block) -> Result<(), BlockError> {
        if block.header.chain_id != self.chain_id {
            return Err(BlockError::ChainMismatch);
        }
        if block.header.previous_block_hash != self.finalized_block_hash {
            return Err(BlockError::PreviousBlockMismatch);
        }
        if block.header.tx_root != merkle_root(&block.transactions) {
            return Err(BlockError::TxRootMismatch);
        }

        let mut working_state = self.clone();
        working_state.height = block.header.height;
        let receipts: Vec<_> = block
            .transactions
            .iter()
            .cloned()
            .map(|tx| working_state.apply_transaction(tx))
            .collect();

        if receipts != block.receipts {
            return Err(BlockError::ReceiptMismatch);
        }
        if block.header.receipt_root != merkle_root(&receipts) {
            return Err(BlockError::ReceiptRootMismatch);
        }
        if block.header.storage_root != working_state.storage_root() {
            return Err(BlockError::StorageRootMismatch);
        }
        if block.header.registry_root != working_state.registry_root() {
            return Err(BlockError::RegistryRootMismatch);
        }
        if block.header.event_root != working_state.event_root() {
            return Err(BlockError::EventRootMismatch);
        }
        if block.header.nonce_root != working_state.nonce_root() {
            return Err(BlockError::NonceRootMismatch);
        }
        if block.header.global_state_root != working_state.global_state_root() {
            return Err(BlockError::GlobalStateRootMismatch);
        }

        working_state.finalized_block_hash = block.block_hash();
        *self = working_state;
        Ok(())
    }

    pub fn balance(
        &self,
        contract: impl Into<ContractId>,
        owner: impl Into<Principal>,
        asset: impl Into<AssetId>,
    ) -> Amount {
        let key = StateKey::Balance {
            contract: contract.into(),
            owner: owner.into(),
            asset: asset.into(),
        };
        self.storage
            .get(&key)
            .map(StateValue::as_uint)
            .unwrap_or_default()
    }

    pub fn total_supply(
        &self,
        contract: impl Into<ContractId>,
        asset: impl Into<AssetId>,
    ) -> Amount {
        let key = StateKey::TotalSupply {
            contract: contract.into(),
            asset: asset.into(),
        };
        self.storage
            .get(&key)
            .map(StateValue::as_uint)
            .unwrap_or_default()
    }

    pub fn reserve(&self, contract: impl Into<ContractId>, asset: impl Into<AssetId>) -> Amount {
        let key = StateKey::Reserve {
            contract: contract.into(),
            asset: asset.into(),
        };
        self.storage
            .get(&key)
            .map(StateValue::as_uint)
            .unwrap_or_default()
    }

    pub fn lp_supply(&self, contract: impl Into<ContractId>) -> Amount {
        let key = StateKey::LpSupply {
            contract: contract.into(),
        };
        self.storage
            .get(&key)
            .map(StateValue::as_uint)
            .unwrap_or_default()
    }

    pub fn lp_balance(
        &self,
        contract: impl Into<ContractId>,
        owner: impl Into<Principal>,
    ) -> Amount {
        let key = StateKey::LpBalance {
            contract: contract.into(),
            owner: owner.into(),
        };
        self.storage
            .get(&key)
            .map(StateValue::as_uint)
            .unwrap_or_default()
    }

    pub fn oracle_price(
        &self,
        contract: impl Into<ContractId>,
        asset: impl Into<AssetId>,
    ) -> Amount {
        let key = StateKey::OraclePrice {
            contract: contract.into(),
            asset: asset.into(),
        };
        self.storage
            .get(&key)
            .map(StateValue::as_uint)
            .unwrap_or_default()
    }

    pub fn oracle_timestamp(
        &self,
        contract: impl Into<ContractId>,
        asset: impl Into<AssetId>,
    ) -> u64 {
        let key = StateKey::OracleTimestamp {
            contract: contract.into(),
            asset: asset.into(),
        };
        self.storage
            .get(&key)
            .map(StateValue::as_uint)
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or_default()
    }

    pub fn is_paused(&self, contract: impl Into<ContractId>) -> bool {
        self.paused_contracts.contains(&contract.into())
    }

    pub fn allowance_remaining(
        &self,
        contract: impl Into<ContractId>,
        owner: impl Into<Principal>,
        spender: impl Into<Principal>,
        asset: impl Into<AssetId>,
    ) -> Option<Amount> {
        let key = GrantKey::Allowance {
            contract: contract.into(),
            owner: owner.into(),
            spender: spender.into(),
            asset: asset.into(),
        };
        self.registry.get(&key).map(Grant::remaining)
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn storage_proof(&self, key: &StateKey) -> Option<StorageProof> {
        let entries = self.storage_entries();
        let index = entries.iter().position(|(entry_key, _)| entry_key == key)?;
        let (_, value) = entries[index].clone();
        let proof = merkle_proof(&entries, index)?;
        Some(StorageProof {
            key: key.clone(),
            value,
            proof,
        })
    }

    pub fn storage_non_inclusion_proof(&self, key: &StateKey) -> Option<StorageNonInclusionProof> {
        if self.storage.contains_key(key) {
            return None;
        }

        let entries = self.storage_entries();
        let insertion_index = entries
            .binary_search_by(|(entry_key, _)| entry_key.cmp(key))
            .expect_err("existing key was checked above");

        let predecessor = insertion_index
            .checked_sub(1)
            .and_then(|index| self.storage_proof(&entries[index].0));
        let successor = entries
            .get(insertion_index)
            .and_then(|(entry_key, _)| self.storage_proof(entry_key));

        Some(StorageNonInclusionProof {
            key: key.clone(),
            root: self.storage_root(),
            leaf_count: entries.len(),
            predecessor,
            successor,
        })
    }

    pub fn registry_proof(&self, key: &GrantKey) -> Option<RegistryProof> {
        let entries = self.registry_entries();
        let index = entries.iter().position(|(entry_key, _)| entry_key == key)?;
        let (_, grant) = entries[index].clone();
        let proof = merkle_proof(&entries, index)?;
        Some(RegistryProof {
            key: key.clone(),
            grant,
            proof,
        })
    }

    pub fn registry_non_inclusion_proof(
        &self,
        key: &GrantKey,
    ) -> Option<RegistryNonInclusionProof> {
        if self.registry.contains_key(key) {
            return None;
        }

        let entries = self.registry_entries();
        let insertion_index = entries
            .binary_search_by(|(entry_key, _)| entry_key.cmp(key))
            .expect_err("existing key was checked above");

        let predecessor = insertion_index
            .checked_sub(1)
            .and_then(|index| self.registry_proof(&entries[index].0));
        let successor = entries
            .get(insertion_index)
            .and_then(|(entry_key, _)| self.registry_proof(entry_key));

        Some(RegistryNonInclusionProof {
            key: key.clone(),
            root: self.registry_root(),
            leaf_count: entries.len(),
            predecessor,
            successor,
        })
    }

    pub fn event_proof(&self, index: usize) -> Option<EventProof> {
        let event = self.events.get(index)?.clone();
        let proof = merkle_proof(&self.events, index)?;
        Some(EventProof { event, proof })
    }

    pub fn storage_root(&self) -> String {
        merkle_root(&self.storage_entries())
    }

    pub fn registry_root(&self) -> String {
        merkle_root(&self.registry_entries())
    }

    pub fn event_root(&self) -> String {
        merkle_root(&self.events)
    }

    pub fn nonce_root(&self) -> String {
        root_of(&(
            self.used_nonces.clone(),
            self.used_certificate_nonces.clone(),
        ))
    }

    pub fn global_state_root(&self) -> String {
        root_of(&(
            &self.chain_id,
            self.height,
            &self.contracts,
            self.storage_root(),
            self.registry_root(),
            &self.used_nonces,
            &self.used_certificate_nonces,
            &self.paused_contracts,
            self.event_root(),
        ))
    }

    fn storage_entries(&self) -> Vec<(StateKey, StateValue)> {
        self.storage
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }

    fn registry_entries(&self) -> Vec<(GrantKey, Grant)> {
        self.registry
            .iter()
            .map(|(key, grant)| (key.clone(), grant.clone()))
            .collect()
    }

    fn execute_call(&mut self, tx: &Transaction) -> Result<ReturnValue, ExecutionError> {
        let contract = self
            .contracts
            .get(&tx.target)
            .cloned()
            .ok_or(ExecutionError::ContractNotFound)?;

        if !contract.exported_methods.contains(&tx.method) {
            return Err(ExecutionError::MethodNotExported);
        }

        if self.paused_contracts.contains(&tx.target) {
            return Err(ExecutionError::ContractPaused);
        }

        match contract.kind {
            ContractKind::Token => match tx.method {
                Method::Transfer => self.transfer(tx),
                Method::Approve => self.approve(tx),
                Method::TransferFrom => self.transfer_from(tx),
                Method::Permit => self.permit(tx),
                _ => Err(ExecutionError::PolicyMissing),
            },
            ContractKind::AmmPool { asset_a, asset_b } => match tx.method {
                Method::AddLiquidity => self.add_liquidity(tx, asset_a, asset_b),
                Method::Swap => self.swap(tx, asset_a, asset_b),
                _ => Err(ExecutionError::PolicyMissing),
            },
            ContractKind::Oracle { asset, max_age } => match tx.method {
                Method::SubmitPrice => self.submit_price(tx, asset, max_age),
                _ => Err(ExecutionError::PolicyMissing),
            },
            ContractKind::Bridge { source_chain } => match tx.method {
                Method::RedeemBridgeMessage => self.redeem_bridge_message(tx, source_chain),
                _ => Err(ExecutionError::PolicyMissing),
            },
            ContractKind::Governance { governed_contract } => match tx.method {
                Method::PauseContract => self.pause_contract(tx, governed_contract),
                Method::UnpauseContract => self.unpause_contract(tx, governed_contract),
                _ => Err(ExecutionError::PolicyMissing),
            },
        }
    }

    fn transfer(&mut self, tx: &Transaction) -> Result<ReturnValue, ExecutionError> {
        let [to, asset, amount] = expect_args(&tx.args)?;
        let to = expect_principal(to)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;
        let from = tx.sender.clone();

        let from_key = balance_key(&tx.target, &from, &asset);
        let to_key = balance_key(&tx.target, &to, &asset);
        let frame = AuthorizedFrame {
            contract: tx.target.clone(),
            msg_sender: tx.sender.clone(),
            write_scope: BTreeSet::from([from_key.clone(), to_key.clone()]),
        };

        let from_balance = self.uint_at(&from_key);
        if from_balance < amount {
            return Err(ExecutionError::InsufficientBalance);
        }
        let to_balance = self.uint_at(&to_key);
        let new_to = to_balance
            .checked_add(amount)
            .ok_or(ExecutionError::ArithmeticOverflow)?;

        self.state_set(&frame, from_key, StateValue::UInt(from_balance - amount))?;
        self.state_set(&frame, to_key, StateValue::UInt(new_to))?;

        if !self.token_invariants_hold(&tx.target, &asset) {
            return Err(ExecutionError::InvariantViolation);
        }

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::Transfer {
                from,
                to,
                asset,
                amount,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn approve(&mut self, tx: &Transaction) -> Result<ReturnValue, ExecutionError> {
        let [spender, asset, amount] = expect_args(&tx.args)?;
        let spender = expect_principal(spender)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;
        let owner = tx.sender.clone();

        let key = GrantKey::Allowance {
            contract: tx.target.clone(),
            owner: owner.clone(),
            spender: spender.clone(),
            asset: asset.clone(),
        };
        self.registry.insert(
            key,
            Grant::allowance(owner.clone(), spender.clone(), amount),
        );

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::Approval {
                owner,
                spender,
                asset,
                amount,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn transfer_from(&mut self, tx: &Transaction) -> Result<ReturnValue, ExecutionError> {
        let [owner, to, asset, amount] = expect_args(&tx.args)?;
        let owner = expect_principal(owner)?;
        let to = expect_principal(to)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;
        let spender = tx.sender.clone();

        self.consume_allowance(&tx.target, &owner, &spender, &asset, amount)?;

        let owner_key = balance_key(&tx.target, &owner, &asset);
        let to_key = balance_key(&tx.target, &to, &asset);
        let frame = AuthorizedFrame {
            contract: tx.target.clone(),
            msg_sender: spender,
            write_scope: BTreeSet::from([owner_key.clone(), to_key.clone()]),
        };

        let owner_balance = self.uint_at(&owner_key);
        if owner_balance < amount {
            return Err(ExecutionError::InsufficientBalance);
        }
        let to_balance = self.uint_at(&to_key);
        let new_to = to_balance
            .checked_add(amount)
            .ok_or(ExecutionError::ArithmeticOverflow)?;

        self.state_set(&frame, owner_key, StateValue::UInt(owner_balance - amount))?;
        self.state_set(&frame, to_key, StateValue::UInt(new_to))?;

        if !self.token_invariants_hold(&tx.target, &asset) {
            return Err(ExecutionError::InvariantViolation);
        }

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::Transfer {
                from: owner,
                to,
                asset,
                amount,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn permit(&mut self, tx: &Transaction) -> Result<ReturnValue, ExecutionError> {
        let [owner, spender, asset, amount, certificate] = expect_args(&tx.args)?;
        let owner = expect_principal(owner)?;
        let spender = expect_principal(spender)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;
        let certificate = expect_certificate(certificate)?;

        let nonce = verify_permit_certificate(
            &certificate,
            &self.chain_id,
            &tx.target,
            &owner,
            &spender,
            &asset,
            amount,
        )?;

        if self.used_certificate_nonces.contains(&nonce) {
            return Err(ExecutionError::CertificateReplay);
        }
        self.used_certificate_nonces.insert(nonce);

        let key = GrantKey::Allowance {
            contract: tx.target.clone(),
            owner: owner.clone(),
            spender: spender.clone(),
            asset: asset.clone(),
        };
        self.registry.insert(
            key,
            Grant::allowance(owner.clone(), spender.clone(), amount),
        );

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::Approval {
                owner,
                spender,
                asset,
                amount,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn add_liquidity(
        &mut self,
        tx: &Transaction,
        asset_a: AssetId,
        asset_b: AssetId,
    ) -> Result<ReturnValue, ExecutionError> {
        let [amount_a, amount_b] = expect_args(&tx.args)?;
        let amount_a = expect_amount(amount_a)?;
        let amount_b = expect_amount(amount_b)?;
        let minted_lp = amount_a
            .checked_add(amount_b)
            .ok_or(ExecutionError::ArithmeticOverflow)?;
        let provider = tx.sender.clone();

        let reserve_a_key = reserve_key(&tx.target, &asset_a);
        let reserve_b_key = reserve_key(&tx.target, &asset_b);
        let lp_supply_key = lp_supply_key(&tx.target);
        let lp_balance_key = lp_balance_key(&tx.target, &provider);
        let frame = AuthorizedFrame {
            contract: tx.target.clone(),
            msg_sender: provider.clone(),
            write_scope: BTreeSet::from([
                reserve_a_key.clone(),
                reserve_b_key.clone(),
                lp_supply_key.clone(),
                lp_balance_key.clone(),
            ]),
        };

        let reserve_a = self.uint_at(&reserve_a_key);
        let reserve_b = self.uint_at(&reserve_b_key);
        let lp_supply = self.uint_at(&lp_supply_key);
        let lp_balance = self.uint_at(&lp_balance_key);

        self.state_set(
            &frame,
            reserve_a_key,
            StateValue::UInt(
                reserve_a
                    .checked_add(amount_a)
                    .ok_or(ExecutionError::ArithmeticOverflow)?,
            ),
        )?;
        self.state_set(
            &frame,
            reserve_b_key,
            StateValue::UInt(
                reserve_b
                    .checked_add(amount_b)
                    .ok_or(ExecutionError::ArithmeticOverflow)?,
            ),
        )?;
        self.state_set(
            &frame,
            lp_supply_key,
            StateValue::UInt(
                lp_supply
                    .checked_add(minted_lp)
                    .ok_or(ExecutionError::ArithmeticOverflow)?,
            ),
        )?;
        self.state_set(
            &frame,
            lp_balance_key,
            StateValue::UInt(
                lp_balance
                    .checked_add(minted_lp)
                    .ok_or(ExecutionError::ArithmeticOverflow)?,
            ),
        )?;

        if !self.amm_invariants_hold(&tx.target) {
            return Err(ExecutionError::InvariantViolation);
        }

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::LiquidityAdded {
                provider,
                asset_a,
                amount_a,
                asset_b,
                amount_b,
                minted_lp,
            },
        );
        Ok(ReturnValue::UInt(minted_lp))
    }

    fn swap(
        &mut self,
        tx: &Transaction,
        asset_a: AssetId,
        asset_b: AssetId,
    ) -> Result<ReturnValue, ExecutionError> {
        let [input_asset, amount_in, min_output] = expect_args(&tx.args)?;
        let input_asset = expect_asset(input_asset)?;
        let amount_in = expect_amount(amount_in)?;
        let min_output = expect_amount(min_output)?;
        let output_asset = if input_asset == asset_a {
            asset_b
        } else if input_asset == asset_b {
            asset_a
        } else {
            return Err(ExecutionError::InvalidPoolAsset);
        };

        let input_key = reserve_key(&tx.target, &input_asset);
        let output_key = reserve_key(&tx.target, &output_asset);
        let frame = AuthorizedFrame {
            contract: tx.target.clone(),
            msg_sender: tx.sender.clone(),
            write_scope: BTreeSet::from([input_key.clone(), output_key.clone()]),
        };

        let reserve_in = self.uint_at(&input_key);
        let reserve_out = self.uint_at(&output_key);
        if reserve_in == 0 || reserve_out == 0 {
            return Err(ExecutionError::InsufficientLiquidity);
        }

        let amount_in_with_fee = amount_in
            .checked_mul(997)
            .ok_or(ExecutionError::ArithmeticOverflow)?
            / 1000;
        let numerator = reserve_out
            .checked_mul(amount_in_with_fee)
            .ok_or(ExecutionError::ArithmeticOverflow)?;
        let denominator = reserve_in
            .checked_add(amount_in_with_fee)
            .ok_or(ExecutionError::ArithmeticOverflow)?;
        let amount_out = numerator / denominator;

        if amount_out == 0 || amount_out > reserve_out {
            return Err(ExecutionError::InsufficientLiquidity);
        }
        if amount_out < min_output {
            return Err(ExecutionError::SlippageExceeded);
        }

        self.state_set(
            &frame,
            input_key,
            StateValue::UInt(
                reserve_in
                    .checked_add(amount_in)
                    .ok_or(ExecutionError::ArithmeticOverflow)?,
            ),
        )?;
        self.state_set(
            &frame,
            output_key,
            StateValue::UInt(reserve_out - amount_out),
        )?;

        if !self.amm_invariants_hold(&tx.target) {
            return Err(ExecutionError::InvariantViolation);
        }

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::Swap {
                trader: tx.sender.clone(),
                input_asset,
                output_asset,
                amount_in,
                amount_out,
            },
        );
        Ok(ReturnValue::UInt(amount_out))
    }

    fn submit_price(
        &mut self,
        tx: &Transaction,
        oracle_asset: AssetId,
        max_age: u64,
    ) -> Result<ReturnValue, ExecutionError> {
        let [asset, price, timestamp] = expect_args(&tx.args)?;
        let asset = expect_asset(asset)?;
        let price = expect_amount(price)?;
        let timestamp = expect_amount(timestamp)?;
        let timestamp = u64::try_from(timestamp).map_err(|_| ExecutionError::ArithmeticOverflow)?;

        if asset != oracle_asset {
            return Err(ExecutionError::InvalidPoolAsset);
        }
        if price == 0 {
            return Err(ExecutionError::InvariantViolation);
        }
        if timestamp.saturating_add(max_age) < self.height {
            return Err(ExecutionError::StaleOraclePrice);
        }

        self.require_oracle_updater(&tx.target, &tx.sender)?;

        let price_key = oracle_price_key(&tx.target, &asset);
        let timestamp_key = oracle_timestamp_key(&tx.target, &asset);
        let frame = AuthorizedFrame {
            contract: tx.target.clone(),
            msg_sender: tx.sender.clone(),
            write_scope: BTreeSet::from([price_key.clone(), timestamp_key.clone()]),
        };

        self.state_set(&frame, price_key, StateValue::UInt(price))?;
        self.state_set(&frame, timestamp_key, StateValue::UInt(timestamp as Amount))?;

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::PriceUpdated {
                updater: tx.sender.clone(),
                asset,
                price,
                timestamp,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn redeem_bridge_message(
        &mut self,
        tx: &Transaction,
        source_chain: ChainId,
    ) -> Result<ReturnValue, ExecutionError> {
        let [message_id, recipient, asset, amount, certificate] = expect_args(&tx.args)?;
        let message_id = expect_text(message_id)?;
        let recipient = expect_principal(recipient)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;
        let certificate = expect_certificate(certificate)?;

        verify_bridge_certificate(
            &certificate,
            &source_chain,
            &tx.target,
            &message_id,
            &recipient,
            &asset,
            amount,
        )?;

        let consumed_key = bridge_message_consumed_key(&tx.target, &message_id);
        if self.uint_at(&consumed_key) != 0 {
            return Err(ExecutionError::BridgeMessageReplay);
        }

        let frame = AuthorizedFrame {
            contract: tx.target.clone(),
            msg_sender: tx.sender.clone(),
            write_scope: BTreeSet::from([consumed_key.clone()]),
        };
        self.state_set(&frame, consumed_key, StateValue::UInt(1))?;

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::BridgeMessageRedeemed {
                message_id,
                recipient,
                asset,
                amount,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn pause_contract(
        &mut self,
        tx: &Transaction,
        governed_contract: ContractId,
    ) -> Result<ReturnValue, ExecutionError> {
        self.require_governance_admin(&tx.target, &tx.sender)?;
        self.paused_contracts.insert(governed_contract.clone());
        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::ContractPaused {
                contract: governed_contract,
                admin: tx.sender.clone(),
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn unpause_contract(
        &mut self,
        tx: &Transaction,
        governed_contract: ContractId,
    ) -> Result<ReturnValue, ExecutionError> {
        self.require_governance_admin(&tx.target, &tx.sender)?;
        self.paused_contracts.remove(&governed_contract);
        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::ContractUnpaused {
                contract: governed_contract,
                admin: tx.sender.clone(),
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn consume_allowance(
        &mut self,
        contract: &ContractId,
        owner: &Principal,
        spender: &Principal,
        asset: &AssetId,
        amount: Amount,
    ) -> Result<(), ExecutionError> {
        let key = GrantKey::Allowance {
            contract: contract.clone(),
            owner: owner.clone(),
            spender: spender.clone(),
            asset: asset.clone(),
        };

        let mut grant = self
            .registry
            .get(&key)
            .cloned()
            .ok_or(ExecutionError::RegistryGrantMissing)?;

        if !grant.active {
            return Err(ExecutionError::RegistryGrantInactive);
        }
        if grant.revoked {
            return Err(ExecutionError::RegistryGrantRevoked);
        }
        if grant.subject != *spender {
            return Err(ExecutionError::RegistryGrantMissing);
        }
        if !grant.rights.contains(&GrantRight::SpendAllowance) {
            return Err(ExecutionError::RegistryRightMissing);
        }
        if grant.remaining() < amount {
            return Err(ExecutionError::AllowanceExceeded);
        }

        grant.spent = grant
            .spent
            .checked_add(amount)
            .ok_or(ExecutionError::ArithmeticOverflow)?;
        self.registry.insert(key, grant);
        Ok(())
    }

    fn require_oracle_updater(
        &self,
        contract: &ContractId,
        updater: &Principal,
    ) -> Result<(), ExecutionError> {
        let key = GrantKey::OracleUpdater {
            contract: contract.clone(),
            subject: updater.clone(),
        };
        let grant = self
            .registry
            .get(&key)
            .ok_or(ExecutionError::UnauthorizedOracleUpdater)?;

        if !grant.active || grant.revoked || grant.subject != *updater {
            return Err(ExecutionError::UnauthorizedOracleUpdater);
        }
        if !grant.rights.contains(&GrantRight::UpdateOracle) {
            return Err(ExecutionError::UnauthorizedOracleUpdater);
        }
        Ok(())
    }

    fn require_governance_admin(
        &self,
        contract: &ContractId,
        admin: &Principal,
    ) -> Result<(), ExecutionError> {
        let key = GrantKey::GovernanceAdmin {
            contract: contract.clone(),
            subject: admin.clone(),
        };
        let grant = self
            .registry
            .get(&key)
            .ok_or(ExecutionError::UnauthorizedGovernance)?;

        if !grant.active || grant.revoked || grant.subject != *admin {
            return Err(ExecutionError::UnauthorizedGovernance);
        }
        if !grant.rights.contains(&GrantRight::GovernanceAdmin) {
            return Err(ExecutionError::UnauthorizedGovernance);
        }
        Ok(())
    }

    fn state_set(
        &mut self,
        frame: &AuthorizedFrame,
        key: StateKey,
        value: StateValue,
    ) -> Result<(), ExecutionError> {
        if key.owner_contract() != &frame.contract {
            return Err(ExecutionError::ContractIsolationViolation);
        }
        if !frame.write_scope.contains(&key) {
            return Err(ExecutionError::WriteScopeViolation);
        }
        self.storage.insert(key, value);
        Ok(())
    }

    fn uint_at(&self, key: &StateKey) -> Amount {
        self.storage
            .get(key)
            .map(StateValue::as_uint)
            .unwrap_or_default()
    }

    fn token_invariants_hold(&self, contract: &ContractId, asset: &AssetId) -> bool {
        let total_supply = self.total_supply(contract.clone(), asset.clone());
        let balance_sum = self
            .storage
            .iter()
            .filter_map(|(key, value)| match key {
                StateKey::Balance {
                    contract: balance_contract,
                    asset: balance_asset,
                    ..
                } if balance_contract == contract && balance_asset == asset => {
                    Some(value.as_uint())
                }
                _ => None,
            })
            .try_fold(0u128, |acc, value| acc.checked_add(value));

        balance_sum == Some(total_supply)
    }

    fn amm_invariants_hold(&self, contract: &ContractId) -> bool {
        let lp_supply = self.lp_supply(contract.clone());
        let lp_balance_sum = self
            .storage
            .iter()
            .filter_map(|(key, value)| match key {
                StateKey::LpBalance {
                    contract: balance_contract,
                    ..
                } if balance_contract == contract => Some(value.as_uint()),
                _ => None,
            })
            .try_fold(0u128, |acc, value| acc.checked_add(value));

        lp_balance_sum == Some(lp_supply)
    }

    fn emit(&mut self, contract: &ContractId, tx_hash: &TxHash, payload: EventPayload) {
        self.events.push(Event {
            contract: contract.clone(),
            tx_hash: tx_hash.clone(),
            index: self.events.len() as u64,
            payload,
        });
    }

    fn committed_receipt(&self, tx_hash: TxHash, return_value: ReturnValue) -> Receipt {
        self.receipt(tx_hash, TxStatus::Committed, None, Some(return_value))
    }

    fn reverted_receipt(&self, tx_hash: TxHash, error: ExecutionError) -> Receipt {
        self.receipt(tx_hash, TxStatus::Reverted, Some(error), None)
    }

    fn rejected_receipt(&self, tx_hash: TxHash, error: ExecutionError) -> Receipt {
        self.receipt(tx_hash, TxStatus::Rejected, Some(error), None)
    }

    fn receipt(
        &self,
        tx_hash: TxHash,
        status: TxStatus,
        error: Option<ExecutionError>,
        return_value: Option<ReturnValue>,
    ) -> Receipt {
        Receipt {
            tx_hash,
            status,
            error,
            return_value,
            storage_root_after: self.storage_root(),
            registry_root_after: self.registry_root(),
            event_root_after: self.event_root(),
            nonce_root_after: self.nonce_root(),
            global_state_root_after: self.global_state_root(),
        }
    }
}

fn expect_args<const N: usize>(args: &[Argument]) -> Result<&[Argument; N], ExecutionError> {
    args.try_into()
        .map_err(|_| ExecutionError::InvalidArguments)
}

fn expect_principal(arg: &Argument) -> Result<Principal, ExecutionError> {
    match arg {
        Argument::Principal(value) => Ok(value.clone()),
        _ => Err(ExecutionError::InvalidArguments),
    }
}

fn expect_asset(arg: &Argument) -> Result<AssetId, ExecutionError> {
    match arg {
        Argument::Asset(value) => Ok(value.clone()),
        _ => Err(ExecutionError::InvalidArguments),
    }
}

fn expect_amount(arg: &Argument) -> Result<Amount, ExecutionError> {
    match arg {
        Argument::Amount(value) => Ok(*value),
        _ => Err(ExecutionError::InvalidArguments),
    }
}

fn expect_text(arg: &Argument) -> Result<String, ExecutionError> {
    match arg {
        Argument::Text(value) => Ok(value.clone()),
        _ => Err(ExecutionError::InvalidArguments),
    }
}

fn expect_certificate(arg: &Argument) -> Result<String, ExecutionError> {
    match arg {
        Argument::Certificate(value) => Ok(value.clone()),
        _ => Err(ExecutionError::InvalidArguments),
    }
}

fn balance_key(contract: &ContractId, owner: &Principal, asset: &AssetId) -> StateKey {
    StateKey::Balance {
        contract: contract.clone(),
        owner: owner.clone(),
        asset: asset.clone(),
    }
}

fn reserve_key(contract: &ContractId, asset: &AssetId) -> StateKey {
    StateKey::Reserve {
        contract: contract.clone(),
        asset: asset.clone(),
    }
}

fn lp_supply_key(contract: &ContractId) -> StateKey {
    StateKey::LpSupply {
        contract: contract.clone(),
    }
}

fn lp_balance_key(contract: &ContractId, owner: &Principal) -> StateKey {
    StateKey::LpBalance {
        contract: contract.clone(),
        owner: owner.clone(),
    }
}

fn oracle_price_key(contract: &ContractId, asset: &AssetId) -> StateKey {
    StateKey::OraclePrice {
        contract: contract.clone(),
        asset: asset.clone(),
    }
}

fn oracle_timestamp_key(contract: &ContractId, asset: &AssetId) -> StateKey {
    StateKey::OracleTimestamp {
        contract: contract.clone(),
        asset: asset.clone(),
    }
}

fn bridge_message_consumed_key(contract: &ContractId, message_id: &str) -> StateKey {
    StateKey::BridgeMessageConsumed {
        contract: contract.clone(),
        message_id: message_id.to_string(),
    }
}

fn verify_permit_certificate(
    certificate: &str,
    chain_id: &ChainId,
    contract: &ContractId,
    owner: &Principal,
    spender: &Principal,
    asset: &AssetId,
    amount: Amount,
) -> Result<String, ExecutionError> {
    let parts: Vec<_> = certificate.split(':').collect();
    if parts.len() != 8 {
        return Err(ExecutionError::InvalidCertificate);
    }

    let amount_text = amount.to_string();
    let expected = [
        "permit",
        chain_id.as_str(),
        contract.as_str(),
        owner.as_str(),
        spender.as_str(),
        asset.as_str(),
        amount_text.as_str(),
    ];

    if parts[..7] != expected {
        return Err(ExecutionError::InvalidCertificate);
    }

    Ok(parts[7].to_string())
}

fn verify_bridge_certificate(
    certificate: &str,
    source_chain: &ChainId,
    contract: &ContractId,
    message_id: &str,
    recipient: &Principal,
    asset: &AssetId,
    amount: Amount,
) -> Result<(), ExecutionError> {
    let parts: Vec<_> = certificate.split(':').collect();
    if parts.len() != 7 {
        return Err(ExecutionError::InvalidBridgeMessage);
    }

    let amount_text = amount.to_string();
    let expected = [
        "bridge",
        source_chain.as_str(),
        contract.as_str(),
        message_id,
        recipient.as_str(),
        asset.as_str(),
        amount_text.as_str(),
    ];

    if parts != expected {
        return Err(ExecutionError::InvalidBridgeMessage);
    }

    Ok(())
}

fn root_of<T: Serialize>(value: &T) -> String {
    let bytes = canonical_bytes(value);
    let digest = Sha256::digest(bytes);
    hex_lower(&digest)
}

fn merkle_root<T: Serialize>(values: &[T]) -> String {
    let leaves: Vec<_> = values.iter().map(merkle_leaf).collect();
    hex_lower(&merkle_root_bytes(leaves))
}

fn merkle_proof<T: Serialize>(values: &[T], index: usize) -> Option<MerkleProof> {
    if index >= values.len() {
        return None;
    }

    let leaves: Vec<_> = values.iter().map(merkle_leaf).collect();
    let root = hex_lower(&merkle_root_bytes(leaves.clone()));
    let leaf_hash = hex_lower(&leaves[index]);
    let mut path = Vec::new();
    let mut current_index = index;
    let mut level = leaves;

    while level.len() > 1 {
        let sibling_index = if current_index % 2 == 0 {
            if current_index + 1 < level.len() {
                current_index + 1
            } else {
                current_index
            }
        } else {
            current_index - 1
        };

        let side = if sibling_index < current_index {
            SiblingSide::Left
        } else {
            SiblingSide::Right
        };
        path.push(MerkleStep {
            side,
            hash: hex_lower(&level[sibling_index]),
        });

        level = next_merkle_level(level);
        current_index /= 2;
    }

    Some(MerkleProof {
        root,
        leaf_hash,
        index,
        leaf_count: values.len(),
        path,
    })
}

fn merkle_root_bytes(leaves: Vec<Vec<u8>>) -> Vec<u8> {
    if leaves.is_empty() {
        return Sha256::digest(b"detta-empty-merkle-root").to_vec();
    }

    let mut level = leaves;
    while level.len() > 1 {
        level = next_merkle_level(level);
    }
    level.remove(0)
}

fn next_merkle_level(level: Vec<Vec<u8>>) -> Vec<Vec<u8>> {
    let mut next = Vec::with_capacity(level.len().div_ceil(2));
    for pair in level.chunks(2) {
        let left = &pair[0];
        let right = pair.get(1).unwrap_or(left);
        next.push(merkle_node(left, right));
    }
    next
}

fn merkle_leaf<T: Serialize>(value: &T) -> Vec<u8> {
    let bytes = canonical_bytes(value);
    let mut hasher = Sha256::new();
    hasher.update(b"detta-merkle-leaf");
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    hasher.finalize().to_vec()
}

fn merkle_node(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"detta-merkle-node");
    hasher.update((left.len() as u64).to_be_bytes());
    hasher.update(left);
    hasher.update((right.len() as u64).to_be_bytes());
    hasher.update(right);
    hasher.finalize().to_vec()
}

fn canonical_bytes<T: Serialize>(value: &T) -> Vec<u8> {
    serde_json::to_vec(value).expect("serializing deterministic state should not fail")
}

fn verify_non_inclusion<K: Ord>(
    key: &K,
    root: &str,
    leaf_count: usize,
    predecessor: Option<(&K, &MerkleProof, bool)>,
    successor: Option<(&K, &MerkleProof, bool)>,
) -> bool {
    if leaf_count == 0 {
        return predecessor.is_none()
            && successor.is_none()
            && root == merkle_root::<(String, String)>(&[]);
    }

    match (predecessor, successor) {
        (Some((pred_key, pred_proof, pred_valid)), Some((succ_key, succ_proof, succ_valid))) => {
            pred_valid
                && succ_valid
                && pred_key < key
                && key < succ_key
                && pred_proof.index + 1 == succ_proof.index
                && pred_proof.leaf_count == leaf_count
                && succ_proof.leaf_count == leaf_count
        }
        (None, Some((succ_key, succ_proof, succ_valid))) => {
            succ_valid
                && key < succ_key
                && succ_proof.index == 0
                && succ_proof.leaf_count == leaf_count
        }
        (Some((pred_key, pred_proof, pred_valid)), None) => {
            pred_valid
                && pred_key < key
                && pred_proof.index + 1 == leaf_count
                && pred_proof.leaf_count == leaf_count
        }
        (None, None) => false,
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

fn hex_to_bytes(value: &str) -> Option<Vec<u8>> {
    if !value.len().is_multiple_of(2) {
        return None;
    }

    value
        .as_bytes()
        .chunks(2)
        .map(|pair| {
            let high = hex_value(pair[0])?;
            let low = hex_value(pair[1])?;
            Some((high << 4) | low)
        })
        .collect()
}

fn hex_value(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn tx(
        tx_hash: &str,
        sender: &str,
        nonce: Nonce,
        method: Method,
        args: Vec<Argument>,
    ) -> Transaction {
        tx_to("TokenA", tx_hash, sender, nonce, method, args)
    }

    fn tx_to(
        target: &str,
        tx_hash: &str,
        sender: &str,
        nonce: Nonce,
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

    fn principal(value: &str) -> Argument {
        Argument::Principal(value.into())
    }

    fn asset(value: &str) -> Argument {
        Argument::Asset(value.into())
    }

    fn amount(value: Amount) -> Argument {
        Argument::Amount(value)
    }

    fn text(value: &str) -> Argument {
        Argument::Text(value.into())
    }

    #[test]
    fn transfer_preserves_supply_and_commits_event() {
        let mut state = seeded_state();

        let receipt = state.apply_transaction(tx(
            "tx1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset("USDC"), amount(10)],
        ));

        assert_eq!(receipt.status, TxStatus::Committed);
        assert_eq!(state.balance("TokenA", "Alice", "USDC"), 90);
        assert_eq!(state.balance("TokenA", "Bob", "USDC"), 60);
        assert_eq!(state.total_supply("TokenA", "USDC"), 150);
        assert_eq!(state.events().len(), 1);
    }

    #[test]
    fn insufficient_balance_reverts_storage_registry_and_events() {
        let mut state = seeded_state();
        let storage_before = state.storage_root();
        let registry_before = state.registry_root();
        let event_before = state.event_root();

        let receipt = state.apply_transaction(tx(
            "tx1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset("USDC"), amount(1_000)],
        ));

        assert_eq!(receipt.status, TxStatus::Reverted);
        assert_eq!(receipt.error, Some(ExecutionError::InsufficientBalance));
        assert_eq!(state.storage_root(), storage_before);
        assert_eq!(state.registry_root(), registry_before);
        assert_eq!(state.event_root(), event_before);
        assert_eq!(state.balance("TokenA", "Alice", "USDC"), 100);
        assert_eq!(state.balance("TokenA", "Bob", "USDC"), 50);
    }

    #[test]
    fn transfer_from_requires_live_registry_allowance() {
        let mut state = seeded_state();

        let approve = state.apply_transaction(tx(
            "tx1",
            "Alice",
            1,
            Method::Approve,
            vec![principal("Dex"), asset("USDC"), amount(100)],
        ));
        assert_eq!(approve.status, TxStatus::Committed);

        let success = state.apply_transaction(tx(
            "tx2",
            "Dex",
            1,
            Method::TransferFrom,
            vec![
                principal("Alice"),
                principal("Bob"),
                asset("USDC"),
                amount(10),
            ],
        ));
        assert_eq!(success.status, TxStatus::Committed);
        assert_eq!(
            state.allowance_remaining("TokenA", "Alice", "Dex", "USDC"),
            Some(90)
        );
        assert_eq!(state.balance("TokenA", "Alice", "USDC"), 90);
        assert_eq!(state.balance("TokenA", "Bob", "USDC"), 60);

        let failure = state.apply_transaction(tx(
            "tx3",
            "Mallory",
            1,
            Method::TransferFrom,
            vec![
                principal("Alice"),
                principal("Bob"),
                asset("USDC"),
                amount(10),
            ],
        ));
        assert_eq!(failure.status, TxStatus::Reverted);
        assert_eq!(failure.error, Some(ExecutionError::RegistryGrantMissing));
    }

    #[test]
    fn allowance_consumption_reverts_with_failed_transfer() {
        let mut state = seeded_state();

        state.apply_transaction(tx(
            "tx1",
            "Alice",
            1,
            Method::Approve,
            vec![principal("Dex"), asset("USDC"), amount(1_000)],
        ));
        let registry_before = state.registry_root();

        let failure = state.apply_transaction(tx(
            "tx2",
            "Dex",
            1,
            Method::TransferFrom,
            vec![
                principal("Alice"),
                principal("Bob"),
                asset("USDC"),
                amount(1_000),
            ],
        ));

        assert_eq!(failure.status, TxStatus::Reverted);
        assert_eq!(failure.error, Some(ExecutionError::InsufficientBalance));
        assert_eq!(state.registry_root(), registry_before);
        assert_eq!(
            state.allowance_remaining("TokenA", "Alice", "Dex", "USDC"),
            Some(1_000)
        );
        assert_eq!(state.balance("TokenA", "Alice", "USDC"), 100);
    }

    #[test]
    fn missing_method_policy_is_denied() {
        let mut state = seeded_state();

        let receipt = state.apply_transaction(tx(
            "tx1",
            "Alice",
            1,
            Method::Other("undocumented".into()),
            vec![],
        ));

        assert_eq!(receipt.status, TxStatus::Reverted);
        assert_eq!(receipt.error, Some(ExecutionError::MethodNotExported));
    }

    #[test]
    fn caller_identity_cannot_be_forged() {
        let mut state = seeded_state();

        let forged = state.apply_transaction(tx(
            "tx1",
            "Mallory",
            1,
            Method::TransferFrom,
            vec![
                principal("Alice"),
                principal("Bob"),
                asset("USDC"),
                amount(10),
            ],
        ));

        assert_eq!(forged.status, TxStatus::Reverted);
        assert_eq!(forged.error, Some(ExecutionError::RegistryGrantMissing));
        assert_eq!(state.balance("TokenA", "Alice", "USDC"), 100);
    }

    #[test]
    fn replayed_nonce_is_rejected() {
        let mut state = seeded_state();

        let first = state.apply_transaction(tx(
            "tx1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset("USDC"), amount(1)],
        ));
        let storage_after_first = state.storage_root();

        let replay = state.apply_transaction(tx(
            "tx2",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset("USDC"), amount(1)],
        ));

        assert_eq!(first.status, TxStatus::Committed);
        assert_eq!(replay.status, TxStatus::Rejected);
        assert_eq!(replay.error, Some(ExecutionError::NonceReplay));
        assert_eq!(state.storage_root(), storage_after_first);
    }

    #[test]
    fn permit_creates_registry_grant_and_replay_fails() {
        let mut state = seeded_state();
        let certificate = "permit:detta-local:TokenA:Alice:Dex:USDC:25:permit-nonce-1";

        let permit = state.apply_transaction(tx(
            "tx1",
            "Relayer",
            1,
            Method::Permit,
            vec![
                principal("Alice"),
                principal("Dex"),
                asset("USDC"),
                amount(25),
                Argument::Certificate(certificate.into()),
            ],
        ));
        assert_eq!(permit.status, TxStatus::Committed);
        assert_eq!(
            state.allowance_remaining("TokenA", "Alice", "Dex", "USDC"),
            Some(25)
        );

        let replay = state.apply_transaction(tx(
            "tx2",
            "Relayer",
            2,
            Method::Permit,
            vec![
                principal("Alice"),
                principal("Dex"),
                asset("USDC"),
                amount(25),
                Argument::Certificate(certificate.into()),
            ],
        ));
        assert_eq!(replay.status, TxStatus::Reverted);
        assert_eq!(replay.error, Some(ExecutionError::CertificateReplay));
        assert_eq!(
            state.allowance_remaining("TokenA", "Alice", "Dex", "USDC"),
            Some(25)
        );
    }

    #[test]
    fn view_reads_are_read_only() {
        let state = seeded_state();
        let root_before = state.global_state_root();

        assert_eq!(state.balance("TokenA", "Alice", "USDC"), 100);
        assert_eq!(state.total_supply("TokenA", "USDC"), 150);
        assert_eq!(state.global_state_root(), root_before);
    }

    #[test]
    fn deterministic_replay_produces_identical_roots() {
        let txs = vec![
            tx(
                "tx1",
                "Alice",
                1,
                Method::Approve,
                vec![principal("Dex"), asset("USDC"), amount(100)],
            ),
            tx(
                "tx2",
                "Dex",
                1,
                Method::TransferFrom,
                vec![
                    principal("Alice"),
                    principal("Bob"),
                    asset("USDC"),
                    amount(10),
                ],
            ),
            tx(
                "tx3",
                "Bob",
                1,
                Method::Transfer,
                vec![principal("Alice"), asset("USDC"), amount(5)],
            ),
        ];

        let mut left = seeded_state();
        let mut right = seeded_state();

        for tx in txs.clone() {
            left.apply_transaction(tx);
        }
        for tx in txs {
            right.apply_transaction(tx);
        }

        assert_eq!(left.storage_root(), right.storage_root());
        assert_eq!(left.registry_root(), right.registry_root());
        assert_eq!(left.event_root(), right.event_root());
        assert_eq!(left.global_state_root(), right.global_state_root());
    }

    #[test]
    fn block_replay_from_independent_node_reaches_same_roots() {
        let proposer_state = seeded_state();
        let validator_initial_state = seeded_state();
        let txs = vec![
            tx(
                "tx1",
                "Alice",
                1,
                Method::Approve,
                vec![principal("Dex"), asset("USDC"), amount(100)],
            ),
            tx(
                "tx2",
                "Dex",
                1,
                Method::TransferFrom,
                vec![
                    principal("Alice"),
                    principal("Bob"),
                    asset("USDC"),
                    amount(10),
                ],
            ),
        ];

        let (block, expected_state) =
            proposer_state.build_block(1, txs, 1_000, "validator-1", "cert-1");
        let mut validator_state = validator_initial_state;

        validator_state.apply_block(&block).unwrap();

        assert_eq!(
            validator_state.storage_root(),
            expected_state.storage_root()
        );
        assert_eq!(
            validator_state.registry_root(),
            expected_state.registry_root()
        );
        assert_eq!(validator_state.event_root(), expected_state.event_root());
        assert_eq!(
            validator_state.global_state_root(),
            expected_state.global_state_root()
        );
    }

    #[test]
    fn block_with_wrong_storage_root_is_rejected() {
        let proposer_state = seeded_state();
        let txs = vec![tx(
            "tx1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset("USDC"), amount(10)],
        )];
        let (mut block, _) = proposer_state.build_block(1, txs, 1_000, "validator-1", "cert-1");
        block.header.storage_root = "bad-root".into();

        let mut validator_state = seeded_state();
        let storage_before = validator_state.storage_root();
        let error = validator_state.apply_block(&block).unwrap_err();

        assert_eq!(error, BlockError::StorageRootMismatch);
        assert_eq!(validator_state.storage_root(), storage_before);
    }

    #[test]
    fn multiple_validators_finalize_same_block_roots() {
        let txs = vec![
            tx(
                "tx1",
                "Alice",
                1,
                Method::Transfer,
                vec![principal("Bob"), asset("USDC"), amount(10)],
            ),
            tx(
                "tx2",
                "Bob",
                1,
                Method::Transfer,
                vec![principal("Alice"), asset("USDC"), amount(5)],
            ),
        ];

        let proposer = ValidatorNode::new("validator-1", seeded_state());
        let block = proposer.propose_block(1, txs, 1_000);
        let mut validators = vec![
            ValidatorNode::new("validator-1", seeded_state()),
            ValidatorNode::new("validator-2", seeded_state()),
            ValidatorNode::new("validator-3", seeded_state()),
        ];

        for validator in &mut validators {
            validator.validate_and_apply(&block).unwrap();
        }

        let expected_root = validators[0].state().global_state_root();
        assert!(validators
            .iter()
            .all(|validator| validator.state().global_state_root() == expected_root));
        assert!(validators
            .iter()
            .all(|validator| validator.state().storage_root() == block.header.storage_root));
    }

    #[test]
    fn validator_mempool_orders_pending_transactions_into_block() {
        let mut proposer = ValidatorNode::new("validator-1", seeded_state());
        let tx_b = tx(
            "tx-b",
            "Bob",
            1,
            Method::Transfer,
            vec![principal("Alice"), asset("USDC"), amount(5)],
        );
        let tx_a = tx(
            "tx-a",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset("USDC"), amount(10)],
        );

        proposer.submit_transaction(tx_b).unwrap();
        proposer.submit_transaction(tx_a.clone()).unwrap();
        assert_eq!(proposer.pending_len(), 2);
        assert_eq!(
            proposer.submit_transaction(tx_a),
            Err(MempoolError::DuplicateTransaction)
        );

        let block = proposer.propose_pending_block(1, 1_000);

        assert_eq!(proposer.pending_len(), 0);
        assert_eq!(block.transactions[0].sender, "Alice");
        assert_eq!(block.transactions[1].sender, "Bob");
        assert_eq!(block.receipts.len(), 2);
    }

    #[test]
    fn full_node_can_sync_from_verified_state_snapshot() {
        let proposer = ValidatorNode::new("validator-1", seeded_state());
        let txs = vec![tx(
            "tx1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset("USDC"), amount(10)],
        )];
        let block = proposer.propose_block(1, txs, 1_000);
        let mut validator = ValidatorNode::new("validator-2", seeded_state());
        validator.validate_and_apply(&block).unwrap();

        let snapshot = validator.state().snapshot();
        let synced = DeTTaState::from_snapshot(snapshot).unwrap();

        assert_eq!(synced.storage_root(), validator.state().storage_root());
        assert_eq!(synced.registry_root(), validator.state().registry_root());
        assert_eq!(synced.event_root(), validator.state().event_root());
        assert_eq!(
            synced.global_state_root(),
            validator.state().global_state_root()
        );
    }

    #[test]
    fn tampered_state_snapshot_is_rejected() {
        let mut snapshot = seeded_state().snapshot();
        snapshot.storage_root = "bad-root".into();

        let error = DeTTaState::from_snapshot(snapshot).unwrap_err();

        assert_eq!(error, SnapshotError::StorageRootMismatch);
    }

    #[test]
    fn light_client_can_verify_storage_registry_event_and_receipt_proofs() {
        let proposer_state = seeded_state();
        let txs = vec![
            tx(
                "tx1",
                "Alice",
                1,
                Method::Approve,
                vec![principal("Dex"), asset("USDC"), amount(100)],
            ),
            tx(
                "tx2",
                "Dex",
                1,
                Method::TransferFrom,
                vec![
                    principal("Alice"),
                    principal("Bob"),
                    asset("USDC"),
                    amount(10),
                ],
            ),
        ];
        let (block, next_state) =
            proposer_state.build_block(1, txs, 1_000, "validator-1", "cert-1");

        let balance_key = StateKey::Balance {
            contract: "TokenA".into(),
            owner: "Bob".into(),
            asset: "USDC".into(),
        };
        let storage_proof = next_state.storage_proof(&balance_key).unwrap();
        assert!(storage_proof.verify());
        assert_eq!(storage_proof.proof.root, block.header.storage_root);

        let grant_key = GrantKey::Allowance {
            contract: "TokenA".into(),
            owner: "Alice".into(),
            spender: "Dex".into(),
            asset: "USDC".into(),
        };
        let registry_proof = next_state.registry_proof(&grant_key).unwrap();
        assert!(registry_proof.verify());
        assert_eq!(registry_proof.proof.root, block.header.registry_root);

        let event_proof = next_state.event_proof(1).unwrap();
        assert!(event_proof.verify());
        assert_eq!(event_proof.proof.root, block.header.event_root);

        let receipt_proof = block.receipt_proof(1).unwrap();
        assert!(receipt_proof.verify());
        assert_eq!(receipt_proof.proof.root, block.header.receipt_root);
    }

    #[test]
    fn light_client_can_verify_storage_and_registry_non_inclusion() {
        let state = seeded_state();

        let missing_balance = StateKey::Balance {
            contract: "TokenA".into(),
            owner: "Charlie".into(),
            asset: "USDC".into(),
        };
        let storage_absence = state.storage_non_inclusion_proof(&missing_balance).unwrap();
        assert!(storage_absence.verify());
        assert_eq!(storage_absence.root, state.storage_root());
        assert!(state.storage_proof(&missing_balance).is_none());

        let existing_balance = StateKey::Balance {
            contract: "TokenA".into(),
            owner: "Alice".into(),
            asset: "USDC".into(),
        };
        assert!(state
            .storage_non_inclusion_proof(&existing_balance)
            .is_none());

        let missing_grant = GrantKey::Allowance {
            contract: "TokenA".into(),
            owner: "Alice".into(),
            spender: "Dex".into(),
            asset: "USDC".into(),
        };
        let registry_absence = state.registry_non_inclusion_proof(&missing_grant).unwrap();
        assert!(registry_absence.verify());
        assert_eq!(registry_absence.root, state.registry_root());
        assert!(state.registry_proof(&missing_grant).is_none());
    }

    #[test]
    fn restricted_evaluator_rejects_forbidden_escape_hatches() {
        let evaluator = RestrictedEvaluator;
        let allowed = [
            EvaluatorPrimitive::StateGet,
            EvaluatorPrimitive::StateSet,
            EvaluatorPrimitive::RegistryGet,
            EvaluatorPrimitive::RegistrySetGuarded,
            EvaluatorPrimitive::EmitEvent,
            EvaluatorPrimitive::CallContract,
            EvaluatorPrimitive::Abort,
            EvaluatorPrimitive::PureArithmetic,
            EvaluatorPrimitive::PureComparison,
            EvaluatorPrimitive::PureData,
        ];
        let forbidden = [
            EvaluatorPrimitive::RawAddAtom,
            EvaluatorPrimitive::RawRemoveAtom,
            EvaluatorPrimitive::RawPrivateMatch,
            EvaluatorPrimitive::PrologAssert,
            EvaluatorPrimitive::PrologRetract,
            EvaluatorPrimitive::PythonCall,
            EvaluatorPrimitive::FileSystemAccess,
            EvaluatorPrimitive::ProcessExecution,
            EvaluatorPrimitive::NetworkAccess,
            EvaluatorPrimitive::WallClockAccess,
            EvaluatorPrimitive::Randomness,
            EvaluatorPrimitive::ArbitraryImport,
        ];

        for primitive in allowed {
            assert_eq!(evaluator.validate_primitive(&primitive), Ok(()));
        }

        for primitive in forbidden {
            assert_eq!(
                evaluator.validate_primitive(&primitive),
                Err(ExecutionError::ForbiddenPrimitive)
            );
        }
    }

    #[test]
    fn amm_add_liquidity_updates_reserves_and_lp_accounting() {
        let mut state = DeTTaState::new("detta-local");
        state.deploy_amm_pool("PoolAB", "USDC", "ATOM").unwrap();

        let receipt = state.apply_transaction(tx_to(
            "PoolAB",
            "tx1",
            "Alice",
            1,
            Method::AddLiquidity,
            vec![amount(1_000), amount(500)],
        ));

        assert_eq!(receipt.status, TxStatus::Committed);
        assert_eq!(receipt.return_value, Some(ReturnValue::UInt(1_500)));
        assert_eq!(state.reserve("PoolAB", "USDC"), 1_000);
        assert_eq!(state.reserve("PoolAB", "ATOM"), 500);
        assert_eq!(state.lp_supply("PoolAB"), 1_500);
        assert_eq!(state.lp_balance("PoolAB", "Alice"), 1_500);
    }

    #[test]
    fn amm_swap_respects_constant_product_formula_and_slippage_bound() {
        let mut state = DeTTaState::new("detta-local");
        state.deploy_amm_pool("PoolAB", "USDC", "ATOM").unwrap();
        state.apply_transaction(tx_to(
            "PoolAB",
            "tx1",
            "Alice",
            1,
            Method::AddLiquidity,
            vec![amount(1_000), amount(500)],
        ));

        let receipt = state.apply_transaction(tx_to(
            "PoolAB",
            "tx2",
            "Bob",
            1,
            Method::Swap,
            vec![asset("USDC"), amount(100), amount(45)],
        ));

        assert_eq!(receipt.status, TxStatus::Committed);
        assert_eq!(receipt.return_value, Some(ReturnValue::UInt(45)));
        assert_eq!(state.reserve("PoolAB", "USDC"), 1_100);
        assert_eq!(state.reserve("PoolAB", "ATOM"), 455);
        assert_eq!(state.lp_supply("PoolAB"), 1_500);
        assert_eq!(state.lp_balance("PoolAB", "Alice"), 1_500);
    }

    #[test]
    fn amm_slippage_failure_reverts_reserves_and_events() {
        let mut state = DeTTaState::new("detta-local");
        state.deploy_amm_pool("PoolAB", "USDC", "ATOM").unwrap();
        state.apply_transaction(tx_to(
            "PoolAB",
            "tx1",
            "Alice",
            1,
            Method::AddLiquidity,
            vec![amount(1_000), amount(500)],
        ));
        let storage_before = state.storage_root();
        let event_before = state.event_root();

        let receipt = state.apply_transaction(tx_to(
            "PoolAB",
            "tx2",
            "Bob",
            1,
            Method::Swap,
            vec![asset("USDC"), amount(100), amount(46)],
        ));

        assert_eq!(receipt.status, TxStatus::Reverted);
        assert_eq!(receipt.error, Some(ExecutionError::SlippageExceeded));
        assert_eq!(state.storage_root(), storage_before);
        assert_eq!(state.event_root(), event_before);
        assert_eq!(state.reserve("PoolAB", "USDC"), 1_000);
        assert_eq!(state.reserve("PoolAB", "ATOM"), 500);
    }

    #[test]
    fn oracle_accepts_authorized_fresh_price_update() {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_oracle("OracleA", "USDC", "Reporter", 10)
            .unwrap();

        let receipt = state.apply_transaction(tx_to(
            "OracleA",
            "tx1",
            "Reporter",
            1,
            Method::SubmitPrice,
            vec![asset("USDC"), amount(100_000_000), amount(0)],
        ));

        assert_eq!(receipt.status, TxStatus::Committed);
        assert_eq!(state.oracle_price("OracleA", "USDC"), 100_000_000);
        assert_eq!(state.oracle_timestamp("OracleA", "USDC"), 0);
    }

    #[test]
    fn oracle_rejects_unauthorized_updater() {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_oracle("OracleA", "USDC", "Reporter", 10)
            .unwrap();

        let receipt = state.apply_transaction(tx_to(
            "OracleA",
            "tx1",
            "Mallory",
            1,
            Method::SubmitPrice,
            vec![asset("USDC"), amount(100_000_000), amount(0)],
        ));

        assert_eq!(receipt.status, TxStatus::Reverted);
        assert_eq!(
            receipt.error,
            Some(ExecutionError::UnauthorizedOracleUpdater)
        );
        assert_eq!(state.oracle_price("OracleA", "USDC"), 0);
    }

    #[test]
    fn oracle_rejects_stale_price_update_in_block_execution() {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_oracle("OracleA", "USDC", "Reporter", 10)
            .unwrap();
        let storage_before = state.storage_root();

        let txs = vec![tx_to(
            "OracleA",
            "tx1",
            "Reporter",
            1,
            Method::SubmitPrice,
            vec![asset("USDC"), amount(100_000_000), amount(50)],
        )];
        let (block, next_state) = state.build_block(100, txs, 1_000, "validator-1", "cert-1");

        assert_eq!(block.receipts[0].status, TxStatus::Reverted);
        assert_eq!(
            block.receipts[0].error,
            Some(ExecutionError::StaleOraclePrice)
        );
        assert_eq!(next_state.oracle_price("OracleA", "USDC"), 0);
        assert_eq!(block.header.storage_root, storage_before);
    }

    #[test]
    fn bridge_redeems_message_once_and_rejects_replay() {
        let mut state = DeTTaState::new("detta-local");
        state.deploy_bridge("BridgeA", "SourceChain").unwrap();
        let certificate = "bridge:SourceChain:BridgeA:msg-1:Alice:USDC:100";

        let first = state.apply_transaction(tx_to(
            "BridgeA",
            "tx1",
            "Relayer",
            1,
            Method::RedeemBridgeMessage,
            vec![
                text("msg-1"),
                principal("Alice"),
                asset("USDC"),
                amount(100),
                Argument::Certificate(certificate.into()),
            ],
        ));
        assert_eq!(first.status, TxStatus::Committed);

        let replay = state.apply_transaction(tx_to(
            "BridgeA",
            "tx2",
            "Relayer",
            2,
            Method::RedeemBridgeMessage,
            vec![
                text("msg-1"),
                principal("Alice"),
                asset("USDC"),
                amount(100),
                Argument::Certificate(certificate.into()),
            ],
        ));
        assert_eq!(replay.status, TxStatus::Reverted);
        assert_eq!(replay.error, Some(ExecutionError::BridgeMessageReplay));
    }

    #[test]
    fn bridge_rejects_invalid_message_certificate() {
        let mut state = DeTTaState::new("detta-local");
        state.deploy_bridge("BridgeA", "SourceChain").unwrap();

        let receipt = state.apply_transaction(tx_to(
            "BridgeA",
            "tx1",
            "Relayer",
            1,
            Method::RedeemBridgeMessage,
            vec![
                text("msg-1"),
                principal("Alice"),
                asset("USDC"),
                amount(100),
                Argument::Certificate("bridge:bad".into()),
            ],
        ));

        assert_eq!(receipt.status, TxStatus::Reverted);
        assert_eq!(receipt.error, Some(ExecutionError::InvalidBridgeMessage));
    }

    #[test]
    fn governance_admin_can_pause_and_unpause_contract() {
        let mut state = seeded_state();
        state.deploy_governance("GovA", "TokenA", "Admin").unwrap();

        let pause = state.apply_transaction(tx_to(
            "GovA",
            "tx1",
            "Admin",
            1,
            Method::PauseContract,
            vec![],
        ));
        assert_eq!(pause.status, TxStatus::Committed);
        assert!(state.is_paused("TokenA"));

        let blocked = state.apply_transaction(tx(
            "tx2",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset("USDC"), amount(10)],
        ));
        assert_eq!(blocked.status, TxStatus::Reverted);
        assert_eq!(blocked.error, Some(ExecutionError::ContractPaused));
        assert_eq!(state.balance("TokenA", "Alice", "USDC"), 100);

        let unpause = state.apply_transaction(tx_to(
            "GovA",
            "tx3",
            "Admin",
            2,
            Method::UnpauseContract,
            vec![],
        ));
        assert_eq!(unpause.status, TxStatus::Committed);
        assert!(!state.is_paused("TokenA"));

        let transfer = state.apply_transaction(tx(
            "tx4",
            "Alice",
            2,
            Method::Transfer,
            vec![principal("Bob"), asset("USDC"), amount(10)],
        ));
        assert_eq!(transfer.status, TxStatus::Committed);
        assert_eq!(state.balance("TokenA", "Alice", "USDC"), 90);
    }

    #[test]
    fn governance_rejects_non_admin_pause() {
        let mut state = seeded_state();
        state.deploy_governance("GovA", "TokenA", "Admin").unwrap();

        let pause = state.apply_transaction(tx_to(
            "GovA",
            "tx1",
            "Mallory",
            1,
            Method::PauseContract,
            vec![],
        ));

        assert_eq!(pause.status, TxStatus::Reverted);
        assert_eq!(pause.error, Some(ExecutionError::UnauthorizedGovernance));
        assert!(!state.is_paused("TokenA"));
    }
}
