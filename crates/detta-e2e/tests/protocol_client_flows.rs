use detta_core::{
    EventPayload, ExecutionError, Method, PolicyEffect, ReturnValue, StateKey, StateValue, TxStatus,
};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, bridge_finality_certificate, certificate, defi_genesis_state, principal, text,
    tx_to, ADMIN, ATOM, BRIDGE_CONTRACT, GOVERNANCE_CONTRACT, ORACLE_CONTRACT, REPORTER,
    ROUTER_CONTRACT, STAKE_CONTRACT, TOKEN_CONTRACT, USDC, VAULT_CONTRACT,
};
use detta_e2e::network::spawn_tcp_rpc_server;
use detta_e2e::proofs::{assert_outbox_proof_matches_root, assert_storage_proof_matches_root};
use detta_rpc::{RpcRequest, RpcResult};

#[test]
fn client_executes_oracle_lending_staking_governance_bridge_and_router_flows() {
    let (tcp_addr, tcp_server) = spawn_tcp_rpc_server(defi_genesis_state()).unwrap();
    let mut client = TcpRpcClient::connect(tcp_addr).unwrap();

    submit_ok(
        &mut client,
        tx_to(
            ORACLE_CONTRACT,
            "e2e-oracle-price-1",
            REPORTER,
            1,
            Method::SubmitPrice,
            vec![asset(ATOM), amount(2), amount(0)],
        ),
    );
    let oracle_block = produce_block(&mut client, 1, 1_000);
    assert_committed(&mut client, "e2e-oracle-price-1");
    assert_storage_value(
        &mut client,
        StateKey::OraclePrice {
            contract: ORACLE_CONTRACT.into(),
            asset: ATOM.into(),
        },
        2,
        &oracle_block.header.storage_root,
    );

    submit_ok(
        &mut client,
        tx_to(
            ORACLE_CONTRACT,
            "e2e-oracle-unauthorized-1",
            "Mallory",
            1,
            Method::SubmitPrice,
            vec![asset(ATOM), amount(3), amount(0)],
        ),
    );
    let unauthorized_oracle_block = produce_block(&mut client, 2, 2_000);
    assert_reverted(
        &mut client,
        "e2e-oracle-unauthorized-1",
        ExecutionError::UnauthorizedOracleUpdater,
    );
    assert_eq!(
        unauthorized_oracle_block.header.storage_root,
        oracle_block.header.storage_root
    );

    submit_ok(
        &mut client,
        tx_to(
            VAULT_CONTRACT,
            "e2e-lending-deposit-1",
            "Alice",
            1,
            Method::DepositCollateral,
            vec![asset(ATOM), amount(100)],
        ),
    );
    let deposit_block = produce_block(&mut client, 3, 3_000);
    assert_committed(&mut client, "e2e-lending-deposit-1");
    assert_storage_value(
        &mut client,
        StateKey::Collateral {
            contract: VAULT_CONTRACT.into(),
            borrower: "Alice".into(),
            asset: ATOM.into(),
        },
        100,
        &deposit_block.header.storage_root,
    );

    submit_ok(
        &mut client,
        tx_to(
            VAULT_CONTRACT,
            "e2e-lending-borrow-1",
            "Alice",
            2,
            Method::Borrow,
            vec![asset(USDC), amount(100)],
        ),
    );
    let borrow_block = produce_block(&mut client, 4, 4_000);
    assert_committed(&mut client, "e2e-lending-borrow-1");
    assert_storage_value(
        &mut client,
        StateKey::Debt {
            contract: VAULT_CONTRACT.into(),
            borrower: "Alice".into(),
            asset: USDC.into(),
        },
        100,
        &borrow_block.header.storage_root,
    );

    submit_ok(
        &mut client,
        tx_to(
            VAULT_CONTRACT,
            "e2e-lending-over-borrow-1",
            "Alice",
            3,
            Method::Borrow,
            vec![asset(USDC), amount(1)],
        ),
    );
    let over_borrow_block = produce_block(&mut client, 5, 5_000);
    assert_reverted(
        &mut client,
        "e2e-lending-over-borrow-1",
        ExecutionError::InsufficientCollateral,
    );
    assert_eq!(
        over_borrow_block.header.storage_root,
        borrow_block.header.storage_root
    );

    submit_ok(
        &mut client,
        tx_to(
            STAKE_CONTRACT,
            "e2e-staking-stake-1",
            "Alice",
            4,
            Method::Stake,
            vec![asset(ATOM), amount(100)],
        ),
    );
    let stake_block = produce_block(&mut client, 6, 6_000);
    assert_committed(&mut client, "e2e-staking-stake-1");
    assert_storage_value(
        &mut client,
        StateKey::StakeBalance {
            contract: STAKE_CONTRACT.into(),
            staker: "Alice".into(),
            asset: ATOM.into(),
        },
        100,
        &stake_block.header.storage_root,
    );
    assert_storage_value(
        &mut client,
        StateKey::TotalStaked {
            contract: STAKE_CONTRACT.into(),
            asset: ATOM.into(),
        },
        100,
        &stake_block.header.storage_root,
    );

    submit_ok(
        &mut client,
        tx_to(
            STAKE_CONTRACT,
            "e2e-staking-claim-1",
            "Alice",
            5,
            Method::ClaimStakingRewards,
            vec![asset(ATOM)],
        ),
    );
    produce_block(&mut client, 7, 7_000);
    let claim_receipt = receipt(&mut client, "e2e-staking-claim-1");
    assert_eq!(claim_receipt.status, TxStatus::Committed);
    assert_eq!(claim_receipt.return_value, Some(ReturnValue::UInt(100)));

    submit_ok(
        &mut client,
        tx_to(
            STAKE_CONTRACT,
            "e2e-staking-request-unstake-1",
            "Alice",
            6,
            Method::RequestUnstake,
            vec![asset(ATOM), amount(40)],
        ),
    );
    let request_unstake_block = produce_block(&mut client, 8, 8_000);
    assert_committed(&mut client, "e2e-staking-request-unstake-1");
    assert_storage_value(
        &mut client,
        StateKey::PendingUnbond {
            contract: STAKE_CONTRACT.into(),
            staker: "Alice".into(),
            asset: ATOM.into(),
        },
        40,
        &request_unstake_block.header.storage_root,
    );

    submit_ok(
        &mut client,
        tx_to(
            STAKE_CONTRACT,
            "e2e-staking-complete-early-1",
            "Alice",
            7,
            Method::CompleteUnstake,
            vec![asset(ATOM), amount(40)],
        ),
    );
    let early_unstake_block = produce_block(&mut client, 9, 9_000);
    assert_reverted(
        &mut client,
        "e2e-staking-complete-early-1",
        ExecutionError::TimelockNotReady,
    );
    assert_eq!(
        early_unstake_block.header.storage_root,
        request_unstake_block.header.storage_root
    );

    submit_ok(
        &mut client,
        tx_to(
            STAKE_CONTRACT,
            "e2e-staking-complete-1",
            "Alice",
            8,
            Method::CompleteUnstake,
            vec![asset(ATOM), amount(40)],
        ),
    );
    let complete_unstake_block = produce_block(&mut client, 10, 10_000);
    assert_committed(&mut client, "e2e-staking-complete-1");
    assert_storage_value(
        &mut client,
        StateKey::PendingUnbond {
            contract: STAKE_CONTRACT.into(),
            staker: "Alice".into(),
            asset: ATOM.into(),
        },
        0,
        &complete_unstake_block.header.storage_root,
    );

    submit_ok(
        &mut client,
        tx_to(
            GOVERNANCE_CONTRACT,
            "e2e-governance-schedule-upgrade-1",
            ADMIN,
            1,
            Method::ScheduleUpgrade,
            vec![text("upgrade-e2e-1"), text("token-code-e2e-v2")],
        ),
    );
    produce_block(&mut client, 11, 11_000);
    assert_committed(&mut client, "e2e-governance-schedule-upgrade-1");
    let scheduled_upgrades = match client.ok(RpcRequest::GetScheduledUpgrades).unwrap() {
        RpcResult::ScheduledUpgrades(upgrades) => upgrades,
        result => panic!("expected scheduled upgrades, got {result:?}"),
    };
    assert!(scheduled_upgrades.iter().any(|upgrade| {
        upgrade.upgrade_id == "upgrade-e2e-1"
            && upgrade.new_code_hash == "token-code-e2e-v2"
            && !upgrade.executed
    }));
    let rehearsal = match client
        .ok(RpcRequest::GetUpgradeRehearsalReport {
            upgrade_id: "upgrade-e2e-1".into(),
        })
        .unwrap()
    {
        RpcResult::UpgradeRehearsalReport(report) => report,
        result => panic!("expected upgrade rehearsal report, got {result:?}"),
    };
    assert_eq!(rehearsal.upgrade_id, "upgrade-e2e-1");
    assert!(!rehearsal.ready_at_height);

    submit_ok(
        &mut client,
        tx_to(
            GOVERNANCE_CONTRACT,
            "e2e-governance-execute-upgrade-early-1",
            ADMIN,
            2,
            Method::ExecuteUpgrade,
            vec![text("upgrade-e2e-1")],
        ),
    );
    produce_block(&mut client, 12, 12_000);
    assert_reverted(
        &mut client,
        "e2e-governance-execute-upgrade-early-1",
        ExecutionError::TimelockNotReady,
    );

    submit_ok(
        &mut client,
        tx_to(
            GOVERNANCE_CONTRACT,
            "e2e-governance-execute-upgrade-1",
            ADMIN,
            3,
            Method::ExecuteUpgrade,
            vec![text("upgrade-e2e-1")],
        ),
    );
    produce_block(&mut client, 13, 13_000);
    assert_committed(&mut client, "e2e-governance-execute-upgrade-1");
    let token_contract = contract(&mut client, TOKEN_CONTRACT);
    assert_eq!(token_contract.code_hash, "token-code-e2e-v2");

    submit_ok(
        &mut client,
        tx_to(
            GOVERNANCE_CONTRACT,
            "e2e-governance-pause-1",
            ADMIN,
            4,
            Method::PauseContract,
            vec![],
        ),
    );
    produce_block(&mut client, 14, 14_000);
    assert_committed(&mut client, "e2e-governance-pause-1");

    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-token-paused-transfer-1",
            "Alice",
            9,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(1)],
        ),
    );
    produce_block(&mut client, 15, 15_000);
    assert_reverted(
        &mut client,
        "e2e-token-paused-transfer-1",
        ExecutionError::ContractPaused,
    );

    submit_ok(
        &mut client,
        tx_to(
            GOVERNANCE_CONTRACT,
            "e2e-governance-unpause-1",
            ADMIN,
            5,
            Method::UnpauseContract,
            vec![],
        ),
    );
    produce_block(&mut client, 16, 16_000);
    assert_committed(&mut client, "e2e-governance-unpause-1");

    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-token-after-unpause-transfer-1",
            "Alice",
            10,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(1)],
        ),
    );
    produce_block(&mut client, 17, 17_000);
    assert_committed(&mut client, "e2e-token-after-unpause-transfer-1");

    submit_ok(
        &mut client,
        tx_to(
            GOVERNANCE_CONTRACT,
            "e2e-governance-schedule-policy-1",
            ADMIN,
            6,
            Method::SchedulePolicyUpdate,
            vec![
                text("policy-e2e-1"),
                text("transfer"),
                text("registryWrite"),
            ],
        ),
    );
    produce_block(&mut client, 18, 18_000);
    assert_committed(&mut client, "e2e-governance-schedule-policy-1");
    let scheduled_policy_updates = match client.ok(RpcRequest::GetScheduledPolicyUpdates).unwrap() {
        RpcResult::ScheduledPolicyUpdates(updates) => updates,
        result => panic!("expected scheduled policy updates, got {result:?}"),
    };
    assert!(scheduled_policy_updates.iter().any(|update| {
        update.update_id == "policy-e2e-1"
            && update.method == Method::Transfer
            && update.effect == PolicyEffect::RegistryWrite
            && !update.executed
    }));

    submit_ok(
        &mut client,
        tx_to(
            GOVERNANCE_CONTRACT,
            "e2e-governance-execute-policy-early-1",
            ADMIN,
            7,
            Method::ExecutePolicyUpdate,
            vec![text("policy-e2e-1")],
        ),
    );
    produce_block(&mut client, 19, 19_000);
    assert_reverted(
        &mut client,
        "e2e-governance-execute-policy-early-1",
        ExecutionError::TimelockNotReady,
    );

    submit_ok(
        &mut client,
        tx_to(
            GOVERNANCE_CONTRACT,
            "e2e-governance-execute-policy-1",
            ADMIN,
            8,
            Method::ExecutePolicyUpdate,
            vec![text("policy-e2e-1")],
        ),
    );
    produce_block(&mut client, 20, 20_000);
    assert_committed(&mut client, "e2e-governance-execute-policy-1");
    assert!(contract(&mut client, TOKEN_CONTRACT)
        .method_policy(&Method::Transfer)
        .unwrap()
        .effects
        .contains(&PolicyEffect::RegistryWrite));

    submit_ok(
        &mut client,
        tx_to(
            BRIDGE_CONTRACT,
            "e2e-bridge-redeem-1",
            "Relayer",
            1,
            Method::RedeemBridgeMessage,
            vec![
                text("msg-1"),
                principal("Alice"),
                asset(USDC),
                amount(100),
                certificate(&bridge_finality_certificate()),
            ],
        ),
    );
    let bridge_redeem_block = produce_block(&mut client, 21, 21_000);
    assert_committed(&mut client, "e2e-bridge-redeem-1");
    assert_storage_value(
        &mut client,
        StateKey::BridgeMessageConsumed {
            contract: BRIDGE_CONTRACT.into(),
            message_id: "msg-1".into(),
        },
        1,
        &bridge_redeem_block.header.storage_root,
    );

    submit_ok(
        &mut client,
        tx_to(
            BRIDGE_CONTRACT,
            "e2e-bridge-replay-1",
            "Relayer",
            2,
            Method::RedeemBridgeMessage,
            vec![
                text("msg-1"),
                principal("Alice"),
                asset(USDC),
                amount(100),
                certificate(&bridge_finality_certificate()),
            ],
        ),
    );
    produce_block(&mut client, 22, 22_000);
    assert_reverted(
        &mut client,
        "e2e-bridge-replay-1",
        ExecutionError::BridgeMessageReplay,
    );

    submit_ok(
        &mut client,
        tx_to(
            BRIDGE_CONTRACT,
            "e2e-bridge-queue-1",
            "Alice",
            11,
            Method::QueueBridgeMessage,
            vec![
                text("ShardB"),
                text("BridgeB"),
                text("outbound-msg-1"),
                principal("Bob"),
                asset(USDC),
                amount(25),
            ],
        ),
    );
    let bridge_queue_block = produce_block(&mut client, 23, 23_000);
    assert_committed(&mut client, "e2e-bridge-queue-1");
    let outbox_proof = match client
        .ok(RpcRequest::GetOutboxMessageProof { index: 0 })
        .unwrap()
    {
        RpcResult::OutboxMessageProof(proof) => proof,
        result => panic!("expected outbox proof, got {result:?}"),
    };
    assert_eq!(outbox_proof.message.message_id, "outbound-msg-1");
    assert_outbox_proof_matches_root(&outbox_proof, &bridge_queue_block.header.outbox_root);

    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-router-approve-1",
            "Alice",
            12,
            Method::Approve,
            vec![principal(ROUTER_CONTRACT), asset(USDC), amount(10)],
        ),
    );
    produce_block(&mut client, 24, 24_000);
    assert_committed(&mut client, "e2e-router-approve-1");

    submit_ok(
        &mut client,
        tx_to(
            ROUTER_CONTRACT,
            "e2e-router-transfer-1",
            "Dapp",
            1,
            Method::RouteTransferFrom,
            vec![
                principal("Alice"),
                principal("Dave"),
                asset(USDC),
                amount(7),
            ],
        ),
    );
    produce_block(&mut client, 25, 25_000);
    assert_committed(&mut client, "e2e-router-transfer-1");
    assert_eq!(token_balance(&mut client, "Dave"), 7);

    submit_ok(
        &mut client,
        tx_to(
            ROUTER_CONTRACT,
            "e2e-router-allowance-exceeded-1",
            "Dapp",
            2,
            Method::RouteTransferFrom,
            vec![
                principal("Alice"),
                principal("Dave"),
                asset(USDC),
                amount(4),
            ],
        ),
    );
    let router_fail_block = produce_block(&mut client, 26, 26_000);
    assert_reverted(
        &mut client,
        "e2e-router-allowance-exceeded-1",
        ExecutionError::AllowanceExceeded,
    );
    assert_eq!(token_balance(&mut client, "Dave"), 7);
    assert!(matches!(
        events(&mut client).last().map(|event| &event.payload),
        Some(EventPayload::Transfer { to, amount, .. }) if to == "Dave" && *amount == 7
    ));
    assert_storage_value(
        &mut client,
        StateKey::Balance {
            contract: TOKEN_CONTRACT.into(),
            owner: "Dave".into(),
            asset: USDC.into(),
        },
        7,
        &router_fail_block.header.storage_root,
    );

    client.close();
    tcp_server.join().unwrap();
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
        result => panic!("expected produced block, got {result:?}"),
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

fn assert_storage_value(
    client: &mut TcpRpcClient,
    key: StateKey,
    expected_value: u128,
    expected_root: &str,
) {
    let proof = storage_proof(client, key);
    assert_eq!(proof.value, StateValue::UInt(expected_value));
    assert_storage_proof_matches_root(&proof, expected_root);
}

fn storage_proof(client: &mut TcpRpcClient, key: StateKey) -> detta_core::StorageProof {
    match client.ok(RpcRequest::GetStorageProof { key }).unwrap() {
        RpcResult::StorageProof(proof) => *proof,
        result => panic!("expected storage proof, got {result:?}"),
    }
}

fn token_balance(client: &mut TcpRpcClient, owner: &str) -> u128 {
    match client
        .ok(RpcRequest::GetBalance {
            contract: TOKEN_CONTRACT.into(),
            owner: owner.into(),
            asset: USDC.into(),
        })
        .unwrap()
    {
        RpcResult::Amount(amount) => amount,
        result => panic!("expected token balance, got {result:?}"),
    }
}

fn contract(client: &mut TcpRpcClient, contract: &str) -> detta_core::ContractRecord {
    match client
        .ok(RpcRequest::GetContract {
            contract: contract.into(),
        })
        .unwrap()
    {
        RpcResult::Contract(contract) => *contract,
        result => panic!("expected contract descriptor, got {result:?}"),
    }
}

fn events(client: &mut TcpRpcClient) -> Vec<detta_core::Event> {
    match client.ok(RpcRequest::GetEvents).unwrap() {
        RpcResult::Events(events) => events,
        result => panic!("expected event list, got {result:?}"),
    }
}
