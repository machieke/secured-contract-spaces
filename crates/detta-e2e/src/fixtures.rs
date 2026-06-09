use detta_core::{
    Amount, Argument, CrossShardFinalityProof, DeTTaState, Method, Nonce, Transaction,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub const CHAIN_ID: &str = "detta-local";
pub const VALIDATOR_ID: &str = "validator-e2e-1";
pub const TOKEN_CONTRACT: &str = "TokenA";
pub const SECONDARY_TOKEN_CONTRACT: &str = "TokenB";
pub const POOL_CONTRACT: &str = "PoolAB";
pub const ORACLE_CONTRACT: &str = "OracleA";
pub const VAULT_CONTRACT: &str = "VaultA";
pub const STAKE_CONTRACT: &str = "StakeA";
pub const GOVERNANCE_CONTRACT: &str = "GovA";
pub const BRIDGE_CONTRACT: &str = "BridgeA";
pub const ROUTER_CONTRACT: &str = "RouterA";
pub const FACTORY_CONTRACT: &str = "FactoryA";
pub const ACCOUNT_REGISTRY_CONTRACT: &str = "AccountsA";
pub const USDC: &str = "USDC";
pub const ATOM: &str = "ATOM";
pub const REPORTER: &str = "Reporter";
pub const ADMIN: &str = "Admin";

pub fn defi_genesis_state() -> DeTTaState {
    let mut state = DeTTaState::new(CHAIN_ID);
    state
        .deploy_token(
            TOKEN_CONTRACT,
            USDC,
            vec![
                ("Alice".into(), 200),
                ("Bob".into(), 50),
                (VAULT_CONTRACT.into(), 1_000),
                ("Liquidator".into(), 1_000),
            ],
        )
        .expect("token genesis fixture should deploy");
    state
        .deploy_token(
            SECONDARY_TOKEN_CONTRACT,
            ATOM,
            vec![("Alice".into(), 1_000), ("Bob".into(), 1_000)],
        )
        .expect("secondary token genesis fixture should deploy");
    state
        .deploy_amm_pool(POOL_CONTRACT, USDC, ATOM)
        .expect("AMM genesis fixture should deploy");
    state
        .deploy_oracle(ORACLE_CONTRACT, ATOM, REPORTER, 20)
        .expect("oracle genesis fixture should deploy");
    state
        .deploy_lending_vault(VAULT_CONTRACT, ATOM, USDC, ORACLE_CONTRACT, 5_000, 20)
        .expect("lending vault genesis fixture should deploy");
    state
        .deploy_staking_with_rewards(STAKE_CONTRACT, ATOM, 2, 1)
        .expect("staking genesis fixture should deploy");
    state
        .deploy_governance_with_timelock(GOVERNANCE_CONTRACT, TOKEN_CONTRACT, ADMIN, 2)
        .expect("governance genesis fixture should deploy");
    state
        .deploy_bridge_with_validator_set(
            BRIDGE_CONTRACT,
            "SourceChain",
            vec![
                "source-validator-1".into(),
                "source-validator-2".into(),
                "source-validator-3".into(),
            ],
            2,
        )
        .expect("bridge genesis fixture should deploy");
    state
        .deploy_router(ROUTER_CONTRACT, TOKEN_CONTRACT)
        .expect("router genesis fixture should deploy");
    state
        .deploy_factory(FACTORY_CONTRACT)
        .expect("factory genesis fixture should deploy");
    state
        .deploy_account_registry(ACCOUNT_REGISTRY_CONTRACT)
        .expect("account registry genesis fixture should deploy");
    state
}

pub fn tx_to(
    target: &str,
    tx_hash: &str,
    sender: &str,
    nonce: Nonce,
    method: Method,
    args: Vec<Argument>,
) -> Transaction {
    Transaction {
        chain_id: CHAIN_ID.into(),
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

pub fn invalid_signature_tx(mut tx: Transaction) -> Transaction {
    tx.signature_ok = false;
    tx
}

pub fn principal(value: &str) -> Argument {
    Argument::Principal(value.into())
}

pub fn asset(value: &str) -> Argument {
    Argument::Asset(value.into())
}

pub fn amount(value: Amount) -> Argument {
    Argument::Amount(value)
}

pub fn text(value: &str) -> Argument {
    Argument::Text(value.into())
}

pub fn certificate(value: &str) -> Argument {
    Argument::Certificate(value.into())
}

pub fn bridge_finality_certificate() -> String {
    serde_json::to_string(&bridge_finality_proof()).expect("bridge proof should serialize")
}

pub fn bridge_finality_proof() -> CrossShardFinalityProof {
    let mut source = DeTTaState::new("SourceChain");
    source
        .deploy_token("SourceTokenUSDC", USDC, vec![("Alice".into(), 100)])
        .expect("source token fixture should deploy");
    source
        .deploy_bridge("BridgeSource", CHAIN_ID)
        .expect("source bridge fixture should deploy");
    let (block, next_state) = source.build_block(
        1,
        vec![Transaction {
            chain_id: "SourceChain".into(),
            tx_hash: "e2e-source-bridge-queue-1".into(),
            sender: "Alice".into(),
            nonce: 1,
            valid_until_height: None,
            target: "BridgeSource".into(),
            method: Method::QueueBridgeMessage,
            args: vec![
                text(CHAIN_ID),
                text(BRIDGE_CONTRACT),
                text("msg-1"),
                principal("Alice"),
                asset(USDC),
                amount(100),
            ],
            signature_ok: true,
            budget: 1_000_000,
        }],
        1_000,
        "source-validator-1",
        "source-cert-1",
    );
    let message_proof = next_state
        .outbox_message_proof(0)
        .expect("source outbox proof should exist");

    CrossShardFinalityProof {
        source_chain: "SourceChain".into(),
        source_height: 1,
        finalized_block_hash: block.block_hash(),
        outbox_root: block.header.outbox_root,
        quorum: 2,
        signers: vec!["source-validator-1".into(), "source-validator-2".into()],
        message_proof,
    }
}

pub struct TestDir {
    path: PathBuf,
}

impl TestDir {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        if std::env::var_os("DETTA_E2E_KEEP_TMP").is_some() {
            return;
        }
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub fn temp_dir(name: &str) -> TestDir {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("detta-e2e-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).expect("e2e temp dir should be created");
    TestDir { path }
}
