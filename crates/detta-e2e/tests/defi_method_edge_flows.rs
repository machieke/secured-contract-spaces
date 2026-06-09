use detta_core::{DeTTaState, ExecutionError, GrantKey, Method, StateKey, StateValue, TxStatus};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, certificate, defi_genesis_state, principal, tx_to, ATOM, CHAIN_ID,
    SECONDARY_TOKEN_CONTRACT, STAKE_CONTRACT, TOKEN_CONTRACT, USDC, VAULT_CONTRACT,
};
use detta_e2e::network::spawn_tcp_rpc_server;
use detta_e2e::proofs::{assert_registry_proof_matches_root, assert_storage_proof_matches_root};
use detta_rpc::{RpcRequest, RpcResult};

const INSTANT_STAKE_CONTRACT: &str = "StakeInstant";
const ORACLE_CONTRACT: &str = "OracleA";
const REPORTER: &str = "Reporter";

#[test]
fn client_exercises_permit_liquidation_and_direct_unstake_edges() {
    let (addr, server) = spawn_tcp_rpc_server(edge_genesis_state()).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();

    let permit_certificate =
        "permit:detta-local:TokenA:Alice:Dex:USDC:25:e2e-client-permit-nonce-1";
    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-client-permit-1",
            "Relayer",
            1,
            Method::Permit,
            vec![
                principal("Alice"),
                principal("Dex"),
                asset(USDC),
                amount(25),
                certificate(permit_certificate),
            ],
        ),
    );
    let permit_block = produce_block(&mut client, 1, 1_000);
    assert_committed(&mut client, "e2e-client-permit-1");
    let permit_proof = registry_proof(
        &mut client,
        GrantKey::Allowance {
            contract: TOKEN_CONTRACT.into(),
            owner: "Alice".into(),
            spender: "Dex".into(),
            asset: USDC.into(),
        },
    );
    assert_eq!(permit_proof.grant.limit, Some(25));
    assert_registry_proof_matches_root(&permit_proof, &permit_block.header.registry_root);

    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-client-permit-replay-1",
            "Relayer",
            2,
            Method::Permit,
            vec![
                principal("Alice"),
                principal("Dex"),
                asset(USDC),
                amount(25),
                certificate(permit_certificate),
            ],
        ),
    );
    produce_block(&mut client, 2, 2_000);
    assert_reverted(
        &mut client,
        "e2e-client-permit-replay-1",
        ExecutionError::CertificateReplay,
    );

    submit_ok(
        &mut client,
        tx_to(
            ORACLE_CONTRACT,
            "e2e-edge-oracle-price-1",
            REPORTER,
            1,
            Method::SubmitPrice,
            vec![asset(ATOM), amount(2), amount(0)],
        ),
    );
    produce_block(&mut client, 3, 3_000);
    assert_committed(&mut client, "e2e-edge-oracle-price-1");

    submit_ok(
        &mut client,
        tx_to(
            VAULT_CONTRACT,
            "e2e-edge-lending-deposit-1",
            "Alice",
            1,
            Method::DepositCollateral,
            vec![asset(ATOM), amount(100)],
        ),
    );
    produce_block(&mut client, 4, 4_000);
    assert_committed(&mut client, "e2e-edge-lending-deposit-1");
    assert_eq!(
        balance(&mut client, SECONDARY_TOKEN_CONTRACT, "Alice", ATOM),
        900
    );
    assert_eq!(
        balance(&mut client, SECONDARY_TOKEN_CONTRACT, VAULT_CONTRACT, ATOM),
        100
    );

    submit_ok(
        &mut client,
        tx_to(
            VAULT_CONTRACT,
            "e2e-edge-lending-borrow-1",
            "Alice",
            2,
            Method::Borrow,
            vec![asset(USDC), amount(100)],
        ),
    );
    produce_block(&mut client, 5, 5_000);
    assert_committed(&mut client, "e2e-edge-lending-borrow-1");
    assert_eq!(balance(&mut client, TOKEN_CONTRACT, "Alice", USDC), 300);
    assert_eq!(
        balance(&mut client, TOKEN_CONTRACT, VAULT_CONTRACT, USDC),
        900
    );

    submit_ok(
        &mut client,
        tx_to(
            ORACLE_CONTRACT,
            "e2e-edge-oracle-price-drop-1",
            REPORTER,
            2,
            Method::SubmitPrice,
            vec![asset(ATOM), amount(1), amount(6)],
        ),
    );
    produce_block(&mut client, 6, 6_000);
    assert_committed(&mut client, "e2e-edge-oracle-price-drop-1");

    submit_ok(
        &mut client,
        tx_to(
            VAULT_CONTRACT,
            "e2e-edge-lending-liquidate-1",
            "Liquidator",
            1,
            Method::Liquidate,
            vec![principal("Alice"), asset(USDC), amount(100)],
        ),
    );
    let liquidation_block = produce_block(&mut client, 7, 7_000);
    assert_committed(&mut client, "e2e-edge-lending-liquidate-1");
    assert_storage_value(
        &mut client,
        StateKey::Debt {
            contract: VAULT_CONTRACT.into(),
            borrower: "Alice".into(),
            asset: USDC.into(),
        },
        0,
        &liquidation_block.header.storage_root,
    );
    assert_storage_value(
        &mut client,
        StateKey::Collateral {
            contract: VAULT_CONTRACT.into(),
            borrower: "Alice".into(),
            asset: ATOM.into(),
        },
        0,
        &liquidation_block.header.storage_root,
    );
    assert_storage_absent(
        &mut client,
        StateKey::BadDebt {
            contract: VAULT_CONTRACT.into(),
            asset: USDC.into(),
        },
    );
    assert_eq!(
        balance(&mut client, TOKEN_CONTRACT, "Liquidator", USDC),
        900
    );
    assert_eq!(
        balance(&mut client, TOKEN_CONTRACT, VAULT_CONTRACT, USDC),
        1_000
    );
    assert_eq!(
        balance(&mut client, SECONDARY_TOKEN_CONTRACT, "Liquidator", ATOM),
        100
    );
    assert_eq!(
        balance(&mut client, SECONDARY_TOKEN_CONTRACT, VAULT_CONTRACT, ATOM),
        0
    );

    submit_ok(
        &mut client,
        tx_to(
            VAULT_CONTRACT,
            "e2e-edge-lending-liquidate-again-1",
            "Liquidator",
            2,
            Method::Liquidate,
            vec![principal("Alice"), asset(USDC), amount(1)],
        ),
    );
    produce_block(&mut client, 8, 8_000);
    assert_reverted(
        &mut client,
        "e2e-edge-lending-liquidate-again-1",
        ExecutionError::InsufficientCollateral,
    );

    submit_ok(
        &mut client,
        tx_to(
            INSTANT_STAKE_CONTRACT,
            "e2e-edge-instant-stake-1",
            "Alice",
            3,
            Method::Stake,
            vec![asset(ATOM), amount(30)],
        ),
    );
    produce_block(&mut client, 9, 9_000);
    assert_committed(&mut client, "e2e-edge-instant-stake-1");

    submit_ok(
        &mut client,
        tx_to(
            INSTANT_STAKE_CONTRACT,
            "e2e-edge-instant-unstake-1",
            "Alice",
            4,
            Method::Unstake,
            vec![asset(ATOM), amount(10)],
        ),
    );
    let instant_unstake_block = produce_block(&mut client, 10, 10_000);
    assert_committed(&mut client, "e2e-edge-instant-unstake-1");
    assert_storage_value(
        &mut client,
        StateKey::StakeBalance {
            contract: INSTANT_STAKE_CONTRACT.into(),
            staker: "Alice".into(),
            asset: ATOM.into(),
        },
        20,
        &instant_unstake_block.header.storage_root,
    );
    assert_eq!(
        balance(&mut client, SECONDARY_TOKEN_CONTRACT, "Alice", ATOM),
        880
    );
    assert_eq!(
        balance(
            &mut client,
            SECONDARY_TOKEN_CONTRACT,
            INSTANT_STAKE_CONTRACT,
            ATOM
        ),
        20
    );

    submit_ok(
        &mut client,
        tx_to(
            STAKE_CONTRACT,
            "e2e-edge-delayed-stake-1",
            "Alice",
            5,
            Method::Stake,
            vec![asset(ATOM), amount(30)],
        ),
    );
    produce_block(&mut client, 11, 11_000);
    assert_committed(&mut client, "e2e-edge-delayed-stake-1");

    submit_ok(
        &mut client,
        tx_to(
            STAKE_CONTRACT,
            "e2e-edge-delayed-unstake-1",
            "Alice",
            6,
            Method::Unstake,
            vec![asset(ATOM), amount(10)],
        ),
    );
    produce_block(&mut client, 12, 12_000);
    assert_reverted(
        &mut client,
        "e2e-edge-delayed-unstake-1",
        ExecutionError::TimelockNotReady,
    );
    assert_eq!(
        balance(&mut client, SECONDARY_TOKEN_CONTRACT, "Alice", ATOM),
        850
    );
    assert_eq!(
        balance(&mut client, SECONDARY_TOKEN_CONTRACT, STAKE_CONTRACT, ATOM),
        30
    );

    client.close();
    server.join().unwrap();
}

fn edge_genesis_state() -> DeTTaState {
    let mut state = defi_genesis_state();
    state
        .deploy_staking(INSTANT_STAKE_CONTRACT, ATOM)
        .expect("instant staking genesis fixture should deploy");
    assert_eq!(state.chain_id(), CHAIN_ID);
    state
}

fn submit_ok(client: &mut TcpRpcClient, transaction: detta_core::Transaction) {
    assert!(matches!(
        client
            .ok(RpcRequest::SubmitTransaction { transaction })
            .unwrap(),
        RpcResult::Submitted
    ));
}

fn produce_block(client: &mut TcpRpcClient, height: u64, timestamp: u64) -> detta_core::Block {
    match client
        .ok(RpcRequest::ProduceBlock { height, timestamp })
        .unwrap()
    {
        RpcResult::Block(block) => *block,
        result => panic!("expected block, got {result:?}"),
    }
}

fn receipt(client: &mut TcpRpcClient, tx_hash: &str) -> detta_core::Receipt {
    match client
        .ok(RpcRequest::GetReceipt {
            tx_hash: tx_hash.into(),
        })
        .unwrap()
    {
        RpcResult::Receipt(receipt) => *receipt,
        result => panic!("expected receipt, got {result:?}"),
    }
}

fn assert_committed(client: &mut TcpRpcClient, tx_hash: &str) {
    assert_eq!(receipt(client, tx_hash).status, TxStatus::Committed);
}

fn assert_reverted(client: &mut TcpRpcClient, tx_hash: &str, error: ExecutionError) {
    let receipt = receipt(client, tx_hash);
    assert_eq!(receipt.status, TxStatus::Reverted);
    assert_eq!(receipt.error, Some(error));
}

fn balance(client: &mut TcpRpcClient, contract: &str, owner: &str, asset: &str) -> u128 {
    match client
        .ok(RpcRequest::GetBalance {
            contract: contract.into(),
            owner: owner.into(),
            asset: asset.into(),
        })
        .unwrap()
    {
        RpcResult::Amount(amount) => amount,
        result => panic!("expected balance amount, got {result:?}"),
    }
}

fn registry_proof(client: &mut TcpRpcClient, key: GrantKey) -> detta_core::RegistryProof {
    match client.ok(RpcRequest::GetRegistryProof { key }).unwrap() {
        RpcResult::RegistryProof(proof) => *proof,
        result => panic!("expected registry proof, got {result:?}"),
    }
}

fn assert_storage_value(
    client: &mut TcpRpcClient,
    key: StateKey,
    expected_value: u128,
    expected_root: &str,
) {
    let proof = match client.ok(RpcRequest::GetStorageProof { key }).unwrap() {
        RpcResult::StorageProof(proof) => *proof,
        result => panic!("expected storage proof, got {result:?}"),
    };
    assert_eq!(proof.value, StateValue::UInt(expected_value));
    assert_storage_proof_matches_root(&proof, expected_root);
}

fn assert_storage_absent(client: &mut TcpRpcClient, key: StateKey) {
    let proof = match client
        .ok(RpcRequest::GetStorageNonInclusionProof { key })
        .unwrap()
    {
        RpcResult::StorageNonInclusionProof(proof) => *proof,
        result => panic!("expected storage non-inclusion proof, got {result:?}"),
    };
    assert!(proof.verify());
}
