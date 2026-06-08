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
    QueueBridgeMessage,
    RedeemBridgeMessage,
    PauseContract,
    UnpauseContract,
    ScheduleUpgrade,
    ExecuteUpgrade,
    DepositCollateral,
    Borrow,
    Stake,
    Unstake,
    RouteTransferFrom,
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
    Collateral {
        contract: ContractId,
        borrower: Principal,
        asset: AssetId,
    },
    Debt {
        contract: ContractId,
        borrower: Principal,
        asset: AssetId,
    },
    StakeBalance {
        contract: ContractId,
        staker: Principal,
        asset: AssetId,
    },
    TotalStaked {
        contract: ContractId,
        asset: AssetId,
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
            StateKey::Collateral { contract, .. } => contract,
            StateKey::Debt { contract, .. } => contract,
            StateKey::StakeBalance { contract, .. } => contract,
            StateKey::TotalStaked { contract, .. } => contract,
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
    declared_invariants: BTreeSet<ContractInvariant>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScheduledUpgrade {
    pub upgrade_id: String,
    pub governance_contract: ContractId,
    pub target_contract: ContractId,
    pub new_code_hash: String,
    pub execute_after_height: u64,
    pub executed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CrossShardMessage {
    pub source_chain: ChainId,
    pub source_contract: ContractId,
    pub source_height: u64,
    pub destination_chain: ChainId,
    pub destination_contract: ContractId,
    pub message_id: String,
    pub recipient: Principal,
    pub asset: AssetId,
    pub amount: Amount,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ContractKind {
    Token,
    AmmPool {
        asset_a: AssetId,
        asset_b: AssetId,
    },
    Oracle {
        asset: AssetId,
        max_age: u64,
    },
    Bridge {
        source_chain: ChainId,
    },
    Governance {
        governed_contract: ContractId,
        timelock_delay: u64,
    },
    LendingVault {
        collateral_asset: AssetId,
        debt_asset: AssetId,
        oracle_contract: ContractId,
        ltv_bps: u64,
        max_oracle_age: u64,
    },
    Staking {
        asset: AssetId,
    },
    Router {
        token_contract: ContractId,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum ContractInvariant {
    TokenSupplyMatchesBalances,
    AmmPoolAssetsDistinct,
    AmmLpSupplyMatchesBalances,
    OracleUpdatesRequireLiveGrant,
    OracleFreshnessCheckedByConsumers,
    BridgeInboundMessagesConsumedOnce,
    BridgeOutboundMessageIdsUnique,
    GovernanceChangesRequireAdminGrant,
    GovernanceUpgradesRespectTimelock,
    LendingBorrowWithinCollateralLimit,
    StakingTotalMatchesBalances,
    RouterDoesNotInheritCallerWriteScope,
}

impl ContractRecord {
    pub fn exported_methods(&self) -> &BTreeSet<Method> {
        &self.exported_methods
    }

    pub fn declared_invariants(&self) -> &BTreeSet<ContractInvariant> {
        &self.declared_invariants
    }

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
            declared_invariants: BTreeSet::from([ContractInvariant::TokenSupplyMatchesBalances]),
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
            declared_invariants: BTreeSet::from([
                ContractInvariant::AmmPoolAssetsDistinct,
                ContractInvariant::AmmLpSupplyMatchesBalances,
            ]),
        }
    }

    fn oracle(contract_id: ContractId, code_hash: String, asset: AssetId, max_age: u64) -> Self {
        Self {
            contract_id,
            code_hash,
            kind: ContractKind::Oracle { asset, max_age },
            exported_methods: BTreeSet::from([Method::SubmitPrice]),
            declared_invariants: BTreeSet::from([
                ContractInvariant::OracleUpdatesRequireLiveGrant,
                ContractInvariant::OracleFreshnessCheckedByConsumers,
            ]),
        }
    }

    fn bridge(contract_id: ContractId, code_hash: String, source_chain: ChainId) -> Self {
        Self {
            contract_id,
            code_hash,
            kind: ContractKind::Bridge { source_chain },
            exported_methods: BTreeSet::from([
                Method::QueueBridgeMessage,
                Method::RedeemBridgeMessage,
            ]),
            declared_invariants: BTreeSet::from([
                ContractInvariant::BridgeInboundMessagesConsumedOnce,
                ContractInvariant::BridgeOutboundMessageIdsUnique,
            ]),
        }
    }

    fn governance(
        contract_id: ContractId,
        code_hash: String,
        governed_contract: ContractId,
        timelock_delay: u64,
    ) -> Self {
        Self {
            contract_id,
            code_hash,
            kind: ContractKind::Governance {
                governed_contract,
                timelock_delay,
            },
            exported_methods: BTreeSet::from([
                Method::PauseContract,
                Method::UnpauseContract,
                Method::ScheduleUpgrade,
                Method::ExecuteUpgrade,
            ]),
            declared_invariants: BTreeSet::from([
                ContractInvariant::GovernanceChangesRequireAdminGrant,
                ContractInvariant::GovernanceUpgradesRespectTimelock,
            ]),
        }
    }

    fn lending_vault(
        contract_id: ContractId,
        code_hash: String,
        collateral_asset: AssetId,
        debt_asset: AssetId,
        oracle_contract: ContractId,
        ltv_bps: u64,
        max_oracle_age: u64,
    ) -> Self {
        Self {
            contract_id,
            code_hash,
            kind: ContractKind::LendingVault {
                collateral_asset,
                debt_asset,
                oracle_contract,
                ltv_bps,
                max_oracle_age,
            },
            exported_methods: BTreeSet::from([Method::DepositCollateral, Method::Borrow]),
            declared_invariants: BTreeSet::from([
                ContractInvariant::LendingBorrowWithinCollateralLimit,
            ]),
        }
    }

    fn staking(contract_id: ContractId, code_hash: String, asset: AssetId) -> Self {
        Self {
            contract_id,
            code_hash,
            kind: ContractKind::Staking { asset },
            exported_methods: BTreeSet::from([Method::Stake, Method::Unstake]),
            declared_invariants: BTreeSet::from([ContractInvariant::StakingTotalMatchesBalances]),
        }
    }

    fn router(contract_id: ContractId, code_hash: String, token_contract: ContractId) -> Self {
        Self {
            contract_id,
            code_hash,
            kind: ContractKind::Router { token_contract },
            exported_methods: BTreeSet::from([Method::RouteTransferFrom]),
            declared_invariants: BTreeSet::from([
                ContractInvariant::RouterDoesNotInheritCallerWriteScope,
            ]),
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
    CrossShardMessageQueued {
        message_id: String,
        destination_chain: ChainId,
        destination_contract: ContractId,
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
    UpgradeScheduled {
        upgrade_id: String,
        contract: ContractId,
        new_code_hash: String,
        execute_after_height: u64,
        admin: Principal,
    },
    UpgradeExecuted {
        upgrade_id: String,
        contract: ContractId,
        old_code_hash: String,
        new_code_hash: String,
        admin: Principal,
    },
    CollateralDeposited {
        borrower: Principal,
        asset: AssetId,
        amount: Amount,
    },
    Borrowed {
        borrower: Principal,
        asset: AssetId,
        amount: Amount,
    },
    Staked {
        staker: Principal,
        asset: AssetId,
        amount: Amount,
    },
    Unstaked {
        staker: Principal,
        asset: AssetId,
        amount: Amount,
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
    OutboundMessageReplay,
    UnauthorizedGovernance,
    ContractPaused,
    UpgradeAlreadyScheduled,
    UpgradeNotFound,
    UpgradeAlreadyExecuted,
    TimelockNotReady,
    MigrationInvariantViolation,
    InsufficientCollateral,
    InsufficientStake,
    ArithmeticOverflow,
    WriteScopeViolation,
    ContractIsolationViolation,
    ReentrancyViolation,
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
    OutboxRootMismatch,
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
    OutboxRootMismatch,
    GlobalStateRootMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StateSnapshot {
    pub state: DeTTaState,
    pub storage_root: String,
    pub registry_root: String,
    pub event_root: String,
    pub nonce_root: String,
    pub outbox_root: String,
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
    pub outbox_root: String,
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OutboxMessageProof {
    pub message: CrossShardMessage,
    pub proof: MerkleProof,
}

impl OutboxMessageProof {
    pub fn verify(&self) -> bool {
        self.proof.verify(&self.message)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatorNode {
    validator_id: String,
    state: DeTTaState,
    mempool: Mempool,
    blocks: BTreeMap<u64, Block>,
    transactions: BTreeMap<TxHash, Transaction>,
    receipts: BTreeMap<TxHash, Receipt>,
}

impl ValidatorNode {
    pub fn new(validator_id: impl Into<String>, state: DeTTaState) -> Self {
        Self {
            validator_id: validator_id.into(),
            state,
            mempool: Mempool::new(),
            blocks: BTreeMap::new(),
            transactions: BTreeMap::new(),
            receipts: BTreeMap::new(),
        }
    }

    pub fn state(&self) -> &DeTTaState {
        &self.state
    }

    pub fn get_block(&self, height: u64) -> Option<&Block> {
        self.blocks.get(&height)
    }

    pub fn get_transaction(&self, tx_hash: &str) -> Option<&Transaction> {
        self.transactions.get(tx_hash)
    }

    pub fn get_receipt(&self, tx_hash: &str) -> Option<&Receipt> {
        self.receipts.get(tx_hash)
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
        self.state.apply_block(block)?;
        for tx in &block.transactions {
            self.transactions.insert(tx.tx_hash.clone(), tx.clone());
        }
        for receipt in &block.receipts {
            self.receipts
                .insert(receipt.tx_hash.clone(), receipt.clone());
        }
        self.blocks.insert(block.header.height, block.clone());
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AuthorizedFrame {
    contract: ContractId,
    msg_sender: Principal,
    write_scope: BTreeSet<StateKey>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CallFrame {
    tx_sender: Principal,
    msg_sender: Principal,
    contract: ContractId,
    method: Method,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct CallContext {
    tx_sender: Principal,
    stack: Vec<CallFrame>,
    active_nonreentrant_contracts: BTreeSet<ContractId>,
}

impl CallContext {
    fn new(tx_sender: Principal) -> Self {
        Self {
            tx_sender,
            stack: Vec::new(),
            active_nonreentrant_contracts: BTreeSet::new(),
        }
    }

    fn enter(
        &mut self,
        contract: &ContractId,
        method: &Method,
        msg_sender: &Principal,
    ) -> Result<(), ExecutionError> {
        if self.active_nonreentrant_contracts.contains(contract) {
            return Err(ExecutionError::ReentrancyViolation);
        }

        self.active_nonreentrant_contracts.insert(contract.clone());
        self.stack.push(CallFrame {
            tx_sender: self.tx_sender.clone(),
            msg_sender: msg_sender.clone(),
            contract: contract.clone(),
            method: method.clone(),
        });
        Ok(())
    }

    fn exit(&mut self) {
        if let Some(frame) = self.stack.pop() {
            self.active_nonreentrant_contracts.remove(&frame.contract);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LendingConfig {
    collateral_asset: AssetId,
    debt_asset: AssetId,
    oracle_contract: ContractId,
    ltv_bps: u64,
    max_oracle_age: u64,
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
    scheduled_upgrades: BTreeMap<String, ScheduledUpgrade>,
    outbound_message_ids: BTreeSet<String>,
    cross_shard_outbox: Vec<CrossShardMessage>,
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
            scheduled_upgrades: BTreeMap::new(),
            outbound_message_ids: BTreeSet::new(),
            cross_shard_outbox: Vec::new(),
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
        if snapshot.outbox_root != snapshot.state.outbox_root() {
            return Err(SnapshotError::OutboxRootMismatch);
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
            outbox_root: self.outbox_root(),
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
        self.deploy_governance_with_timelock(contract, governed_contract, admin, 1)
    }

    pub fn deploy_governance_with_timelock(
        &mut self,
        contract: impl Into<ContractId>,
        governed_contract: impl Into<ContractId>,
        admin: impl Into<Principal>,
        timelock_delay: u64,
    ) -> Result<(), ExecutionError> {
        let contract = contract.into();
        let governed_contract = governed_contract.into();
        let admin = admin.into();
        let code_hash = root_of(&(
            "detta-governance-v1",
            &contract,
            &governed_contract,
            timelock_delay,
        ));
        self.contracts.insert(
            contract.clone(),
            ContractRecord::governance(
                contract.clone(),
                code_hash,
                governed_contract,
                timelock_delay,
            ),
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

    pub fn deploy_lending_vault(
        &mut self,
        contract: impl Into<ContractId>,
        collateral_asset: impl Into<AssetId>,
        debt_asset: impl Into<AssetId>,
        oracle_contract: impl Into<ContractId>,
        ltv_bps: u64,
        max_oracle_age: u64,
    ) -> Result<(), ExecutionError> {
        let contract = contract.into();
        let collateral_asset = collateral_asset.into();
        let debt_asset = debt_asset.into();
        let oracle_contract = oracle_contract.into();
        let code_hash = root_of(&(
            "detta-lending-vault-v1",
            &contract,
            &collateral_asset,
            &debt_asset,
            &oracle_contract,
            ltv_bps,
            max_oracle_age,
        ));
        self.contracts.insert(
            contract.clone(),
            ContractRecord::lending_vault(
                contract,
                code_hash,
                collateral_asset,
                debt_asset,
                oracle_contract,
                ltv_bps,
                max_oracle_age,
            ),
        );
        Ok(())
    }

    pub fn deploy_staking(
        &mut self,
        contract: impl Into<ContractId>,
        asset: impl Into<AssetId>,
    ) -> Result<(), ExecutionError> {
        let contract = contract.into();
        let asset = asset.into();
        let code_hash = root_of(&("detta-staking-v1", &contract, &asset));
        self.contracts.insert(
            contract.clone(),
            ContractRecord::staking(contract.clone(), code_hash, asset.clone()),
        );
        self.storage.insert(
            StateKey::TotalStaked { contract, asset },
            StateValue::UInt(0),
        );
        Ok(())
    }

    pub fn deploy_router(
        &mut self,
        contract: impl Into<ContractId>,
        token_contract: impl Into<ContractId>,
    ) -> Result<(), ExecutionError> {
        let contract = contract.into();
        let token_contract = token_contract.into();
        let code_hash = root_of(&("detta-router-v1", &contract, &token_contract));
        self.contracts.insert(
            contract.clone(),
            ContractRecord::router(contract, code_hash, token_contract),
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
        let checkpoint_contracts = self.contracts.clone();
        let checkpoint_scheduled_upgrades = self.scheduled_upgrades.clone();
        let checkpoint_outbound_message_ids = self.outbound_message_ids.clone();
        let checkpoint_cross_shard_outbox = self.cross_shard_outbox.clone();

        let msg_sender = tx.sender.clone();
        let mut call_context = CallContext::new(tx.sender.clone());
        match self.execute_call(&tx, msg_sender, &mut call_context) {
            Ok(return_value) => self.committed_receipt(tx.tx_hash, return_value),
            Err(error) => {
                self.contracts = checkpoint_contracts;
                self.storage = checkpoint_storage;
                self.registry = checkpoint_registry;
                self.events = checkpoint_events;
                self.used_certificate_nonces = checkpoint_certificate_nonces;
                self.paused_contracts = checkpoint_paused_contracts;
                self.scheduled_upgrades = checkpoint_scheduled_upgrades;
                self.outbound_message_ids = checkpoint_outbound_message_ids;
                self.cross_shard_outbox = checkpoint_cross_shard_outbox;
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
            outbox_root: working_state.outbox_root(),
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
        if block.header.outbox_root != working_state.outbox_root() {
            return Err(BlockError::OutboxRootMismatch);
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

    pub fn code_hash(&self, contract: impl Into<ContractId>) -> Option<&str> {
        let contract = contract.into();
        self.contracts
            .get(&contract)
            .map(|record| record.code_hash.as_str())
    }

    pub fn contract(&self, contract: impl Into<ContractId>) -> Option<&ContractRecord> {
        let contract = contract.into();
        self.contracts.get(&contract)
    }

    pub fn scheduled_upgrade(&self, upgrade_id: &str) -> Option<&ScheduledUpgrade> {
        self.scheduled_upgrades.get(upgrade_id)
    }

    pub fn contract_records(&self) -> impl Iterator<Item = &ContractRecord> {
        self.contracts.values()
    }

    pub fn scheduled_upgrades(&self) -> impl Iterator<Item = &ScheduledUpgrade> {
        self.scheduled_upgrades.values()
    }

    pub fn paused_contracts(&self) -> impl Iterator<Item = &ContractId> {
        self.paused_contracts.iter()
    }

    pub fn collateral(
        &self,
        contract: impl Into<ContractId>,
        borrower: impl Into<Principal>,
        asset: impl Into<AssetId>,
    ) -> Amount {
        let key = StateKey::Collateral {
            contract: contract.into(),
            borrower: borrower.into(),
            asset: asset.into(),
        };
        self.storage
            .get(&key)
            .map(StateValue::as_uint)
            .unwrap_or_default()
    }

    pub fn debt(
        &self,
        contract: impl Into<ContractId>,
        borrower: impl Into<Principal>,
        asset: impl Into<AssetId>,
    ) -> Amount {
        let key = StateKey::Debt {
            contract: contract.into(),
            borrower: borrower.into(),
            asset: asset.into(),
        };
        self.storage
            .get(&key)
            .map(StateValue::as_uint)
            .unwrap_or_default()
    }

    pub fn stake_balance(
        &self,
        contract: impl Into<ContractId>,
        staker: impl Into<Principal>,
        asset: impl Into<AssetId>,
    ) -> Amount {
        let key = StateKey::StakeBalance {
            contract: contract.into(),
            staker: staker.into(),
            asset: asset.into(),
        };
        self.storage
            .get(&key)
            .map(StateValue::as_uint)
            .unwrap_or_default()
    }

    pub fn total_staked(
        &self,
        contract: impl Into<ContractId>,
        asset: impl Into<AssetId>,
    ) -> Amount {
        let key = StateKey::TotalStaked {
            contract: contract.into(),
            asset: asset.into(),
        };
        self.storage
            .get(&key)
            .map(StateValue::as_uint)
            .unwrap_or_default()
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

    pub fn cross_shard_outbox(&self) -> &[CrossShardMessage] {
        &self.cross_shard_outbox
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

    pub fn outbox_message_proof(&self, index: usize) -> Option<OutboxMessageProof> {
        let message = self.cross_shard_outbox.get(index)?.clone();
        let proof = merkle_proof(&self.cross_shard_outbox, index)?;
        Some(OutboxMessageProof { message, proof })
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

    pub fn outbox_root(&self) -> String {
        merkle_root(&self.cross_shard_outbox)
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
            &self.scheduled_upgrades,
            &self.outbound_message_ids,
            self.outbox_root(),
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

    fn execute_call(
        &mut self,
        tx: &Transaction,
        msg_sender: Principal,
        call_context: &mut CallContext,
    ) -> Result<ReturnValue, ExecutionError> {
        call_context.enter(&tx.target, &tx.method, &msg_sender)?;
        let result = (|| {
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
                    Method::Transfer => self.transfer(tx, msg_sender),
                    Method::Approve => self.approve(tx, msg_sender),
                    Method::TransferFrom => self.transfer_from(tx, msg_sender),
                    Method::Permit => self.permit(tx),
                    _ => Err(ExecutionError::PolicyMissing),
                },
                ContractKind::AmmPool { asset_a, asset_b } => match tx.method {
                    Method::AddLiquidity => self.add_liquidity(tx, msg_sender, asset_a, asset_b),
                    Method::Swap => self.swap(tx, msg_sender, asset_a, asset_b),
                    _ => Err(ExecutionError::PolicyMissing),
                },
                ContractKind::Oracle { asset, max_age } => match tx.method {
                    Method::SubmitPrice => self.submit_price(tx, msg_sender, asset, max_age),
                    _ => Err(ExecutionError::PolicyMissing),
                },
                ContractKind::Bridge { source_chain } => match tx.method {
                    Method::QueueBridgeMessage => self.queue_bridge_message(tx, msg_sender),
                    Method::RedeemBridgeMessage => {
                        self.redeem_bridge_message(tx, msg_sender, source_chain)
                    }
                    _ => Err(ExecutionError::PolicyMissing),
                },
                ContractKind::Governance {
                    governed_contract,
                    timelock_delay,
                } => match tx.method {
                    Method::PauseContract => self.pause_contract(tx, msg_sender, governed_contract),
                    Method::UnpauseContract => {
                        self.unpause_contract(tx, msg_sender, governed_contract)
                    }
                    Method::ScheduleUpgrade => {
                        self.schedule_upgrade(tx, msg_sender, governed_contract, timelock_delay)
                    }
                    Method::ExecuteUpgrade => {
                        self.execute_upgrade(tx, msg_sender, governed_contract)
                    }
                    _ => Err(ExecutionError::PolicyMissing),
                },
                ContractKind::LendingVault {
                    collateral_asset,
                    debt_asset,
                    oracle_contract,
                    ltv_bps,
                    max_oracle_age,
                } => match tx.method {
                    Method::DepositCollateral => self.deposit_collateral(tx, collateral_asset),
                    Method::Borrow => self.borrow(
                        tx,
                        msg_sender,
                        LendingConfig {
                            collateral_asset,
                            debt_asset,
                            oracle_contract,
                            ltv_bps,
                            max_oracle_age,
                        },
                    ),
                    _ => Err(ExecutionError::PolicyMissing),
                },
                ContractKind::Staking { asset } => match tx.method {
                    Method::Stake => self.stake(tx, msg_sender, asset),
                    Method::Unstake => self.unstake(tx, msg_sender, asset),
                    _ => Err(ExecutionError::PolicyMissing),
                },
                ContractKind::Router { token_contract } => match tx.method {
                    Method::RouteTransferFrom => {
                        self.route_transfer_from(tx, msg_sender, token_contract, call_context)
                    }
                    _ => Err(ExecutionError::PolicyMissing),
                },
            }
        })();
        call_context.exit();
        result
    }

    fn transfer(
        &mut self,
        tx: &Transaction,
        msg_sender: Principal,
    ) -> Result<ReturnValue, ExecutionError> {
        let [to, asset, amount] = expect_args(&tx.args)?;
        let to = expect_principal(to)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;
        let from = msg_sender;

        let from_key = balance_key(&tx.target, &from, &asset);
        let to_key = balance_key(&tx.target, &to, &asset);
        let frame = AuthorizedFrame {
            contract: tx.target.clone(),
            msg_sender: from.clone(),
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

    fn approve(
        &mut self,
        tx: &Transaction,
        msg_sender: Principal,
    ) -> Result<ReturnValue, ExecutionError> {
        let [spender, asset, amount] = expect_args(&tx.args)?;
        let spender = expect_principal(spender)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;
        let owner = msg_sender;

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

    fn transfer_from(
        &mut self,
        tx: &Transaction,
        msg_sender: Principal,
    ) -> Result<ReturnValue, ExecutionError> {
        let [owner, to, asset, amount] = expect_args(&tx.args)?;
        let owner = expect_principal(owner)?;
        let to = expect_principal(to)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;
        let spender = msg_sender;

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
        msg_sender: Principal,
        asset_a: AssetId,
        asset_b: AssetId,
    ) -> Result<ReturnValue, ExecutionError> {
        let [amount_a, amount_b] = expect_args(&tx.args)?;
        let amount_a = expect_amount(amount_a)?;
        let amount_b = expect_amount(amount_b)?;
        let minted_lp = amount_a
            .checked_add(amount_b)
            .ok_or(ExecutionError::ArithmeticOverflow)?;
        let provider = msg_sender;

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
        msg_sender: Principal,
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
                trader: msg_sender,
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
        msg_sender: Principal,
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

        self.require_oracle_updater(&tx.target, &msg_sender)?;

        let price_key = oracle_price_key(&tx.target, &asset);
        let timestamp_key = oracle_timestamp_key(&tx.target, &asset);
        let frame = AuthorizedFrame {
            contract: tx.target.clone(),
            msg_sender: msg_sender.clone(),
            write_scope: BTreeSet::from([price_key.clone(), timestamp_key.clone()]),
        };

        self.state_set(&frame, price_key, StateValue::UInt(price))?;
        self.state_set(&frame, timestamp_key, StateValue::UInt(timestamp as Amount))?;

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::PriceUpdated {
                updater: msg_sender,
                asset,
                price,
                timestamp,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn queue_bridge_message(
        &mut self,
        tx: &Transaction,
        _msg_sender: Principal,
    ) -> Result<ReturnValue, ExecutionError> {
        let [destination_chain, destination_contract, message_id, recipient, asset, amount] =
            expect_args(&tx.args)?;
        let destination_chain = expect_text(destination_chain)?;
        let destination_contract = expect_text(destination_contract)?;
        let message_id = expect_text(message_id)?;
        let recipient = expect_principal(recipient)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;

        if self.outbound_message_ids.contains(&message_id) {
            return Err(ExecutionError::OutboundMessageReplay);
        }

        let message = CrossShardMessage {
            source_chain: self.chain_id.clone(),
            source_contract: tx.target.clone(),
            source_height: self.height,
            destination_chain: destination_chain.clone(),
            destination_contract: destination_contract.clone(),
            message_id: message_id.clone(),
            recipient: recipient.clone(),
            asset: asset.clone(),
            amount,
        };
        self.outbound_message_ids.insert(message_id.clone());
        self.cross_shard_outbox.push(message);

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::CrossShardMessageQueued {
                message_id,
                destination_chain,
                destination_contract,
                recipient,
                asset,
                amount,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn redeem_bridge_message(
        &mut self,
        tx: &Transaction,
        msg_sender: Principal,
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
            msg_sender,
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
        msg_sender: Principal,
        governed_contract: ContractId,
    ) -> Result<ReturnValue, ExecutionError> {
        self.require_governance_admin(&tx.target, &msg_sender)?;
        self.paused_contracts.insert(governed_contract.clone());
        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::ContractPaused {
                contract: governed_contract,
                admin: msg_sender,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn unpause_contract(
        &mut self,
        tx: &Transaction,
        msg_sender: Principal,
        governed_contract: ContractId,
    ) -> Result<ReturnValue, ExecutionError> {
        self.require_governance_admin(&tx.target, &msg_sender)?;
        self.paused_contracts.remove(&governed_contract);
        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::ContractUnpaused {
                contract: governed_contract,
                admin: msg_sender,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn schedule_upgrade(
        &mut self,
        tx: &Transaction,
        msg_sender: Principal,
        governed_contract: ContractId,
        timelock_delay: u64,
    ) -> Result<ReturnValue, ExecutionError> {
        let [upgrade_id, new_code_hash] = expect_args(&tx.args)?;
        let upgrade_id = expect_text(upgrade_id)?;
        let new_code_hash = expect_text(new_code_hash)?;

        self.require_governance_admin(&tx.target, &msg_sender)?;
        if self.scheduled_upgrades.contains_key(&upgrade_id) {
            return Err(ExecutionError::UpgradeAlreadyScheduled);
        }

        let execute_after_height = self
            .height
            .checked_add(timelock_delay)
            .ok_or(ExecutionError::ArithmeticOverflow)?;
        let upgrade = ScheduledUpgrade {
            upgrade_id: upgrade_id.clone(),
            governance_contract: tx.target.clone(),
            target_contract: governed_contract.clone(),
            new_code_hash: new_code_hash.clone(),
            execute_after_height,
            executed: false,
        };
        self.scheduled_upgrades.insert(upgrade_id.clone(), upgrade);

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::UpgradeScheduled {
                upgrade_id,
                contract: governed_contract,
                new_code_hash,
                execute_after_height,
                admin: msg_sender,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn execute_upgrade(
        &mut self,
        tx: &Transaction,
        msg_sender: Principal,
        governed_contract: ContractId,
    ) -> Result<ReturnValue, ExecutionError> {
        let [upgrade_id] = expect_args(&tx.args)?;
        let upgrade_id = expect_text(upgrade_id)?;

        self.require_governance_admin(&tx.target, &msg_sender)?;

        let storage_root_before = self.storage_root();
        let registry_root_before = self.registry_root();
        let upgrade = self
            .scheduled_upgrades
            .get_mut(&upgrade_id)
            .ok_or(ExecutionError::UpgradeNotFound)?;

        if upgrade.executed {
            return Err(ExecutionError::UpgradeAlreadyExecuted);
        }
        if upgrade.governance_contract != tx.target || upgrade.target_contract != governed_contract
        {
            return Err(ExecutionError::UpgradeNotFound);
        }
        if self.height < upgrade.execute_after_height {
            return Err(ExecutionError::TimelockNotReady);
        }

        let target = self
            .contracts
            .get_mut(&governed_contract)
            .ok_or(ExecutionError::ContractNotFound)?;
        let old_code_hash = target.code_hash.clone();
        let new_code_hash = upgrade.new_code_hash.clone();
        target.code_hash = new_code_hash.clone();
        upgrade.executed = true;

        if self.storage_root() != storage_root_before
            || self.registry_root() != registry_root_before
        {
            return Err(ExecutionError::MigrationInvariantViolation);
        }

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::UpgradeExecuted {
                upgrade_id,
                contract: governed_contract,
                old_code_hash,
                new_code_hash,
                admin: msg_sender,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn deposit_collateral(
        &mut self,
        tx: &Transaction,
        collateral_asset: AssetId,
    ) -> Result<ReturnValue, ExecutionError> {
        let [asset, amount] = expect_args(&tx.args)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;
        if asset != collateral_asset {
            return Err(ExecutionError::InvalidPoolAsset);
        }

        let borrower = tx.sender.clone();
        let collateral_key = collateral_key(&tx.target, &borrower, &asset);
        let frame = AuthorizedFrame {
            contract: tx.target.clone(),
            msg_sender: borrower.clone(),
            write_scope: BTreeSet::from([collateral_key.clone()]),
        };
        let current = self.uint_at(&collateral_key);
        self.state_set(
            &frame,
            collateral_key,
            StateValue::UInt(
                current
                    .checked_add(amount)
                    .ok_or(ExecutionError::ArithmeticOverflow)?,
            ),
        )?;

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::CollateralDeposited {
                borrower,
                asset,
                amount,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn borrow(
        &mut self,
        tx: &Transaction,
        msg_sender: Principal,
        config: LendingConfig,
    ) -> Result<ReturnValue, ExecutionError> {
        let [asset, amount] = expect_args(&tx.args)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;
        if asset != config.debt_asset {
            return Err(ExecutionError::InvalidPoolAsset);
        }

        let borrower = msg_sender;
        let oracle_timestamp =
            self.oracle_timestamp(&config.oracle_contract, &config.collateral_asset);
        if oracle_timestamp.saturating_add(config.max_oracle_age) < self.height {
            return Err(ExecutionError::StaleOraclePrice);
        }

        let price = self.oracle_price(&config.oracle_contract, &config.collateral_asset);
        if price == 0 {
            return Err(ExecutionError::StaleOraclePrice);
        }

        let collateral = self.collateral(&tx.target, &borrower, &config.collateral_asset);
        let debt_key = debt_key(&tx.target, &borrower, &config.debt_asset);
        let current_debt = self.uint_at(&debt_key);
        let next_debt = current_debt
            .checked_add(amount)
            .ok_or(ExecutionError::ArithmeticOverflow)?;

        let collateral_value = collateral
            .checked_mul(price)
            .ok_or(ExecutionError::ArithmeticOverflow)?;
        let max_debt = collateral_value
            .checked_mul(config.ltv_bps as Amount)
            .ok_or(ExecutionError::ArithmeticOverflow)?
            / 10_000;

        if next_debt > max_debt {
            return Err(ExecutionError::InsufficientCollateral);
        }

        let frame = AuthorizedFrame {
            contract: tx.target.clone(),
            msg_sender: borrower.clone(),
            write_scope: BTreeSet::from([debt_key.clone()]),
        };
        self.state_set(&frame, debt_key, StateValue::UInt(next_debt))?;

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::Borrowed {
                borrower,
                asset,
                amount,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn stake(
        &mut self,
        tx: &Transaction,
        msg_sender: Principal,
        staking_asset: AssetId,
    ) -> Result<ReturnValue, ExecutionError> {
        let [asset, amount] = expect_args(&tx.args)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;
        if asset != staking_asset {
            return Err(ExecutionError::InvalidPoolAsset);
        }

        let staker = msg_sender;
        let stake_key = stake_balance_key(&tx.target, &staker, &asset);
        let total_key = total_staked_key(&tx.target, &asset);
        let frame = AuthorizedFrame {
            contract: tx.target.clone(),
            msg_sender: staker.clone(),
            write_scope: BTreeSet::from([stake_key.clone(), total_key.clone()]),
        };

        let current_stake = self.uint_at(&stake_key);
        let total_staked = self.uint_at(&total_key);
        self.state_set(
            &frame,
            stake_key,
            StateValue::UInt(
                current_stake
                    .checked_add(amount)
                    .ok_or(ExecutionError::ArithmeticOverflow)?,
            ),
        )?;
        self.state_set(
            &frame,
            total_key,
            StateValue::UInt(
                total_staked
                    .checked_add(amount)
                    .ok_or(ExecutionError::ArithmeticOverflow)?,
            ),
        )?;

        if !self.staking_invariants_hold(&tx.target, &asset) {
            return Err(ExecutionError::InvariantViolation);
        }

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::Staked {
                staker,
                asset,
                amount,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn unstake(
        &mut self,
        tx: &Transaction,
        msg_sender: Principal,
        staking_asset: AssetId,
    ) -> Result<ReturnValue, ExecutionError> {
        let [asset, amount] = expect_args(&tx.args)?;
        let asset = expect_asset(asset)?;
        let amount = expect_amount(amount)?;
        if asset != staking_asset {
            return Err(ExecutionError::InvalidPoolAsset);
        }

        let staker = msg_sender;
        let stake_key = stake_balance_key(&tx.target, &staker, &asset);
        let total_key = total_staked_key(&tx.target, &asset);
        let current_stake = self.uint_at(&stake_key);
        if current_stake < amount {
            return Err(ExecutionError::InsufficientStake);
        }
        let total_staked = self.uint_at(&total_key);
        if total_staked < amount {
            return Err(ExecutionError::InvariantViolation);
        }

        let frame = AuthorizedFrame {
            contract: tx.target.clone(),
            msg_sender: staker.clone(),
            write_scope: BTreeSet::from([stake_key.clone(), total_key.clone()]),
        };
        self.state_set(&frame, stake_key, StateValue::UInt(current_stake - amount))?;
        self.state_set(&frame, total_key, StateValue::UInt(total_staked - amount))?;

        if !self.staking_invariants_hold(&tx.target, &asset) {
            return Err(ExecutionError::InvariantViolation);
        }

        self.emit(
            &tx.target,
            &tx.tx_hash,
            EventPayload::Unstaked {
                staker,
                asset,
                amount,
            },
        );
        Ok(ReturnValue::Unit)
    }

    fn route_transfer_from(
        &mut self,
        tx: &Transaction,
        _msg_sender: Principal,
        token_contract: ContractId,
        call_context: &mut CallContext,
    ) -> Result<ReturnValue, ExecutionError> {
        let [owner, to, asset, amount] = expect_args(&tx.args)?;
        let child_tx = Transaction {
            chain_id: tx.chain_id.clone(),
            tx_hash: tx.tx_hash.clone(),
            sender: tx.sender.clone(),
            nonce: tx.nonce,
            target: token_contract,
            method: Method::TransferFrom,
            args: vec![owner.clone(), to.clone(), asset.clone(), amount.clone()],
            signature_ok: true,
            budget: tx.budget,
        };

        self.execute_call(&child_tx, tx.target.clone(), call_context)
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

    fn staking_invariants_hold(&self, contract: &ContractId, asset: &AssetId) -> bool {
        let total_staked = self.total_staked(contract.clone(), asset.clone());
        let stake_sum = self
            .storage
            .iter()
            .filter_map(|(key, value)| match key {
                StateKey::StakeBalance {
                    contract: stake_contract,
                    asset: stake_asset,
                    ..
                } if stake_contract == contract && stake_asset == asset => Some(value.as_uint()),
                _ => None,
            })
            .try_fold(0u128, |acc, value| acc.checked_add(value));

        stake_sum == Some(total_staked)
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

fn collateral_key(contract: &ContractId, borrower: &Principal, asset: &AssetId) -> StateKey {
    StateKey::Collateral {
        contract: contract.clone(),
        borrower: borrower.clone(),
        asset: asset.clone(),
    }
}

fn debt_key(contract: &ContractId, borrower: &Principal, asset: &AssetId) -> StateKey {
    StateKey::Debt {
        contract: contract.clone(),
        borrower: borrower.clone(),
        asset: asset.clone(),
    }
}

fn stake_balance_key(contract: &ContractId, staker: &Principal, asset: &AssetId) -> StateKey {
    StateKey::StakeBalance {
        contract: contract.clone(),
        staker: staker.clone(),
        asset: asset.clone(),
    }
}

fn total_staked_key(contract: &ContractId, asset: &AssetId) -> StateKey {
    StateKey::TotalStaked {
        contract: contract.clone(),
        asset: asset.clone(),
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
    fn deployed_defi_contracts_declare_invariants() {
        let mut state = seeded_state();
        state.deploy_amm_pool("PoolA", "USDC", "ETH").unwrap();
        state
            .deploy_oracle("OracleA", "USDC", "Reporter", 10)
            .unwrap();
        state.deploy_bridge("BridgeA", "SourceChain").unwrap();
        state.deploy_governance("GovA", "TokenA", "Admin").unwrap();
        state
            .deploy_lending_vault("VaultA", "USDC", "dUSD", "OracleA", 5_000, 10)
            .unwrap();
        state.deploy_staking("StakeA", "USDC").unwrap();
        state.deploy_router("RouterA", "TokenA").unwrap();

        let declared: BTreeSet<_> = state
            .contract_records()
            .flat_map(|record| record.declared_invariants().iter().cloned())
            .collect();

        assert!(state
            .contract_records()
            .all(|record| !record.declared_invariants().is_empty()));
        assert!(declared.contains(&ContractInvariant::TokenSupplyMatchesBalances));
        assert!(declared.contains(&ContractInvariant::AmmLpSupplyMatchesBalances));
        assert!(declared.contains(&ContractInvariant::OracleFreshnessCheckedByConsumers));
        assert!(declared.contains(&ContractInvariant::BridgeInboundMessagesConsumedOnce));
        assert!(declared.contains(&ContractInvariant::GovernanceUpgradesRespectTimelock));
        assert!(declared.contains(&ContractInvariant::LendingBorrowWithinCollateralLimit));
        assert!(declared.contains(&ContractInvariant::StakingTotalMatchesBalances));
        assert!(declared.contains(&ContractInvariant::RouterDoesNotInheritCallerWriteScope));
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
    fn router_cross_contract_transfer_from_uses_router_as_spender() {
        let mut state = seeded_state();
        state.deploy_router("RouterA", "TokenA").unwrap();

        let approve = state.apply_transaction(tx(
            "tx1",
            "Alice",
            1,
            Method::Approve,
            vec![principal("RouterA"), asset("USDC"), amount(100)],
        ));
        assert_eq!(approve.status, TxStatus::Committed);

        let routed = state.apply_transaction(tx_to(
            "RouterA",
            "tx2",
            "Alice",
            2,
            Method::RouteTransferFrom,
            vec![
                principal("Alice"),
                principal("Bob"),
                asset("USDC"),
                amount(10),
            ],
        ));

        assert_eq!(routed.status, TxStatus::Committed);
        assert_eq!(
            state.allowance_remaining("TokenA", "Alice", "RouterA", "USDC"),
            Some(90)
        );
        assert_eq!(state.balance("TokenA", "Alice", "USDC"), 90);
        assert_eq!(state.balance("TokenA", "Bob", "USDC"), 60);
    }

    #[test]
    fn router_cross_contract_call_does_not_inherit_user_allowance() {
        let mut state = seeded_state();
        state.deploy_router("RouterA", "TokenA").unwrap();
        state.apply_transaction(tx(
            "tx1",
            "Alice",
            1,
            Method::Approve,
            vec![principal("Dex"), asset("USDC"), amount(100)],
        ));

        let routed = state.apply_transaction(tx_to(
            "RouterA",
            "tx2",
            "Alice",
            2,
            Method::RouteTransferFrom,
            vec![
                principal("Alice"),
                principal("Bob"),
                asset("USDC"),
                amount(10),
            ],
        ));

        assert_eq!(routed.status, TxStatus::Reverted);
        assert_eq!(routed.error, Some(ExecutionError::RegistryGrantMissing));
        assert_eq!(state.balance("TokenA", "Alice", "USDC"), 100);
        assert_eq!(state.balance("TokenA", "Bob", "USDC"), 50);
        assert_eq!(
            state.allowance_remaining("TokenA", "Alice", "Dex", "USDC"),
            Some(100)
        );
    }

    #[test]
    fn reentrant_same_contract_call_is_blocked() {
        let mut state = seeded_state();
        state.deploy_router("RouterA", "RouterA").unwrap();
        let storage_before = state.storage_root();
        let registry_before = state.registry_root();
        let event_root_before = state.event_root();

        let receipt = state.apply_transaction(tx_to(
            "RouterA",
            "tx1",
            "Alice",
            1,
            Method::RouteTransferFrom,
            vec![
                principal("Alice"),
                principal("Bob"),
                asset("USDC"),
                amount(10),
            ],
        ));

        assert_eq!(receipt.status, TxStatus::Reverted);
        assert_eq!(receipt.error, Some(ExecutionError::ReentrancyViolation));
        assert_eq!(state.storage_root(), storage_before);
        assert_eq!(state.registry_root(), registry_before);
        assert_eq!(state.event_root(), event_root_before);
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
    fn bridge_queues_cross_shard_message_with_proof() {
        let mut state = DeTTaState::new("ShardA");
        state.deploy_bridge("BridgeA", "ShardB").unwrap();
        let storage_before = state.storage_root();
        let registry_before = state.registry_root();

        let txs = vec![Transaction {
            chain_id: "ShardA".into(),
            tx_hash: "tx1".into(),
            sender: "Alice".into(),
            nonce: 1,
            target: "BridgeA".into(),
            method: Method::QueueBridgeMessage,
            args: vec![
                text("ShardB"),
                text("BridgeB"),
                text("msg-1"),
                principal("Bob"),
                asset("USDC"),
                amount(100),
            ],
            signature_ok: true,
            budget: 1_000_000,
        }];
        let (block, next_state) = state.build_block(1, txs, 1_000, "validator-1", "cert-1");

        assert_eq!(block.receipts[0].status, TxStatus::Committed);
        assert_eq!(next_state.cross_shard_outbox().len(), 1);
        assert_eq!(next_state.storage_root(), storage_before);
        assert_eq!(next_state.registry_root(), registry_before);
        assert_eq!(block.header.outbox_root, next_state.outbox_root());

        let proof = next_state.outbox_message_proof(0).unwrap();
        assert!(proof.verify());
        assert_eq!(proof.proof.root, block.header.outbox_root);
        assert_eq!(proof.message.destination_chain, "ShardB");
        assert_eq!(proof.message.destination_contract, "BridgeB");
    }

    #[test]
    fn bridge_rejects_duplicate_outbound_message_id() {
        let mut state = DeTTaState::new("ShardA");
        state.deploy_bridge("BridgeA", "ShardB").unwrap();

        let first = state.apply_transaction(Transaction {
            chain_id: "ShardA".into(),
            tx_hash: "tx1".into(),
            sender: "Alice".into(),
            nonce: 1,
            target: "BridgeA".into(),
            method: Method::QueueBridgeMessage,
            args: vec![
                text("ShardB"),
                text("BridgeB"),
                text("msg-1"),
                principal("Bob"),
                asset("USDC"),
                amount(100),
            ],
            signature_ok: true,
            budget: 1_000_000,
        });
        let outbox_root = state.outbox_root();

        let duplicate = state.apply_transaction(Transaction {
            chain_id: "ShardA".into(),
            tx_hash: "tx2".into(),
            sender: "Alice".into(),
            nonce: 2,
            target: "BridgeA".into(),
            method: Method::QueueBridgeMessage,
            args: vec![
                text("ShardB"),
                text("BridgeB"),
                text("msg-1"),
                principal("Bob"),
                asset("USDC"),
                amount(100),
            ],
            signature_ok: true,
            budget: 1_000_000,
        });

        assert_eq!(first.status, TxStatus::Committed);
        assert_eq!(duplicate.status, TxStatus::Reverted);
        assert_eq!(duplicate.error, Some(ExecutionError::OutboundMessageReplay));
        assert_eq!(state.outbox_root(), outbox_root);
        assert_eq!(state.cross_shard_outbox().len(), 1);
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

    #[test]
    fn governance_schedules_and_executes_timelocked_code_upgrade() {
        let mut state = seeded_state();
        state
            .deploy_governance_with_timelock("GovA", "TokenA", "Admin", 2)
            .unwrap();
        let old_code_hash = state.code_hash("TokenA").unwrap().to_string();
        let storage_before = state.storage_root();
        let registry_before = state.registry_root();

        let schedule = state.apply_transaction(tx_to(
            "GovA",
            "tx1",
            "Admin",
            1,
            Method::ScheduleUpgrade,
            vec![text("upgrade-1"), text("token-code-v2")],
        ));
        assert_eq!(schedule.status, TxStatus::Committed);
        assert_eq!(
            state
                .scheduled_upgrade("upgrade-1")
                .unwrap()
                .execute_after_height,
            2
        );

        let early_state = state.clone();
        let (early_block, _) = early_state.build_block(
            1,
            vec![tx_to(
                "GovA",
                "tx2",
                "Admin",
                2,
                Method::ExecuteUpgrade,
                vec![text("upgrade-1")],
            )],
            1_000,
            "validator-1",
            "cert-1",
        );
        assert_eq!(early_block.receipts[0].status, TxStatus::Reverted);
        assert_eq!(
            early_block.receipts[0].error,
            Some(ExecutionError::TimelockNotReady)
        );

        let (block, next_state) = state.build_block(
            2,
            vec![tx_to(
                "GovA",
                "tx3",
                "Admin",
                2,
                Method::ExecuteUpgrade,
                vec![text("upgrade-1")],
            )],
            2_000,
            "validator-1",
            "cert-2",
        );

        assert_eq!(block.receipts[0].status, TxStatus::Committed);
        assert_eq!(next_state.code_hash("TokenA"), Some("token-code-v2"));
        assert_ne!(next_state.code_hash("TokenA").unwrap(), old_code_hash);
        assert!(next_state.scheduled_upgrade("upgrade-1").unwrap().executed);
        assert_eq!(next_state.storage_root(), storage_before);
        assert_eq!(next_state.registry_root(), registry_before);
        assert!(matches!(
            next_state.events().last().unwrap().payload,
            EventPayload::UpgradeExecuted { .. }
        ));
    }

    #[test]
    fn governance_rejects_non_admin_upgrade_schedule() {
        let mut state = seeded_state();
        state
            .deploy_governance_with_timelock("GovA", "TokenA", "Admin", 2)
            .unwrap();

        let schedule = state.apply_transaction(tx_to(
            "GovA",
            "tx1",
            "Mallory",
            1,
            Method::ScheduleUpgrade,
            vec![text("upgrade-1"), text("token-code-v2")],
        ));

        assert_eq!(schedule.status, TxStatus::Reverted);
        assert_eq!(schedule.error, Some(ExecutionError::UnauthorizedGovernance));
        assert!(state.scheduled_upgrade("upgrade-1").is_none());
    }

    #[test]
    fn lending_vault_allows_borrow_with_sufficient_collateral() {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_oracle("OracleA", "ATOM", "Reporter", 10)
            .unwrap();
        state
            .deploy_lending_vault("VaultA", "ATOM", "USDC", "OracleA", 5_000, 10)
            .unwrap();
        state.apply_transaction(tx_to(
            "OracleA",
            "tx1",
            "Reporter",
            1,
            Method::SubmitPrice,
            vec![asset("ATOM"), amount(2), amount(0)],
        ));
        state.apply_transaction(tx_to(
            "VaultA",
            "tx2",
            "Alice",
            1,
            Method::DepositCollateral,
            vec![asset("ATOM"), amount(100)],
        ));

        let borrow = state.apply_transaction(tx_to(
            "VaultA",
            "tx3",
            "Alice",
            2,
            Method::Borrow,
            vec![asset("USDC"), amount(100)],
        ));

        assert_eq!(borrow.status, TxStatus::Committed);
        assert_eq!(state.collateral("VaultA", "Alice", "ATOM"), 100);
        assert_eq!(state.debt("VaultA", "Alice", "USDC"), 100);
    }

    #[test]
    fn lending_vault_rejects_undercollateralized_borrow() {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_oracle("OracleA", "ATOM", "Reporter", 10)
            .unwrap();
        state
            .deploy_lending_vault("VaultA", "ATOM", "USDC", "OracleA", 5_000, 10)
            .unwrap();
        state.apply_transaction(tx_to(
            "OracleA",
            "tx1",
            "Reporter",
            1,
            Method::SubmitPrice,
            vec![asset("ATOM"), amount(2), amount(0)],
        ));
        state.apply_transaction(tx_to(
            "VaultA",
            "tx2",
            "Alice",
            1,
            Method::DepositCollateral,
            vec![asset("ATOM"), amount(100)],
        ));
        let debt_before = state.debt("VaultA", "Alice", "USDC");

        let borrow = state.apply_transaction(tx_to(
            "VaultA",
            "tx3",
            "Alice",
            2,
            Method::Borrow,
            vec![asset("USDC"), amount(101)],
        ));

        assert_eq!(borrow.status, TxStatus::Reverted);
        assert_eq!(borrow.error, Some(ExecutionError::InsufficientCollateral));
        assert_eq!(state.debt("VaultA", "Alice", "USDC"), debt_before);
    }

    #[test]
    fn lending_vault_rejects_stale_oracle_price() {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_oracle("OracleA", "ATOM", "Reporter", 10)
            .unwrap();
        state
            .deploy_lending_vault("VaultA", "ATOM", "USDC", "OracleA", 5_000, 10)
            .unwrap();
        state.apply_transaction(tx_to(
            "OracleA",
            "tx1",
            "Reporter",
            1,
            Method::SubmitPrice,
            vec![asset("ATOM"), amount(2), amount(50)],
        ));
        state.apply_transaction(tx_to(
            "VaultA",
            "tx2",
            "Alice",
            1,
            Method::DepositCollateral,
            vec![asset("ATOM"), amount(100)],
        ));

        let txs = vec![tx_to(
            "VaultA",
            "tx3",
            "Alice",
            2,
            Method::Borrow,
            vec![asset("USDC"), amount(100)],
        )];
        let (block, next_state) = state.build_block(100, txs, 1_000, "validator-1", "cert-1");

        assert_eq!(block.receipts[0].status, TxStatus::Reverted);
        assert_eq!(
            block.receipts[0].error,
            Some(ExecutionError::StaleOraclePrice)
        );
        assert_eq!(next_state.debt("VaultA", "Alice", "USDC"), 0);
    }

    #[test]
    fn staking_updates_staker_balance_and_total_staked() {
        let mut state = DeTTaState::new("detta-local");
        state.deploy_staking("StakeA", "ATOM").unwrap();

        let stake = state.apply_transaction(tx_to(
            "StakeA",
            "tx1",
            "Alice",
            1,
            Method::Stake,
            vec![asset("ATOM"), amount(100)],
        ));
        assert_eq!(stake.status, TxStatus::Committed);
        assert_eq!(state.stake_balance("StakeA", "Alice", "ATOM"), 100);
        assert_eq!(state.total_staked("StakeA", "ATOM"), 100);

        let unstake = state.apply_transaction(tx_to(
            "StakeA",
            "tx2",
            "Alice",
            2,
            Method::Unstake,
            vec![asset("ATOM"), amount(40)],
        ));
        assert_eq!(unstake.status, TxStatus::Committed);
        assert_eq!(state.stake_balance("StakeA", "Alice", "ATOM"), 60);
        assert_eq!(state.total_staked("StakeA", "ATOM"), 60);
    }

    #[test]
    fn staking_reverts_over_unstake() {
        let mut state = DeTTaState::new("detta-local");
        state.deploy_staking("StakeA", "ATOM").unwrap();
        state.apply_transaction(tx_to(
            "StakeA",
            "tx1",
            "Alice",
            1,
            Method::Stake,
            vec![asset("ATOM"), amount(50)],
        ));
        let storage_before = state.storage_root();

        let unstake = state.apply_transaction(tx_to(
            "StakeA",
            "tx2",
            "Alice",
            2,
            Method::Unstake,
            vec![asset("ATOM"), amount(51)],
        ));

        assert_eq!(unstake.status, TxStatus::Reverted);
        assert_eq!(unstake.error, Some(ExecutionError::InsufficientStake));
        assert_eq!(state.storage_root(), storage_before);
        assert_eq!(state.stake_balance("StakeA", "Alice", "ATOM"), 50);
        assert_eq!(state.total_staked("StakeA", "ATOM"), 50);
    }
}
