use detta_core::{Block, Method, StateKey, StateValue, Transaction};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, temp_dir, text, tx_to, ADMIN, ATOM,
    BRIDGE_CONTRACT, GOVERNANCE_CONTRACT, ORACLE_CONTRACT, POOL_CONTRACT, REPORTER,
    SECONDARY_TOKEN_CONTRACT, STAKE_CONTRACT, TOKEN_CONTRACT, USDC, VAULT_CONTRACT,
};
use detta_e2e::network::spawn_tcp_persistent_node;
use detta_e2e::proofs::{assert_outbox_proof_matches_root, assert_storage_proof_matches_root};
use detta_node::PersistentValidatorNode;
use detta_rpc::{RpcRequest, RpcResult};

#[test]
fn four_persistent_validators_converge_on_imported_defi_workload() {
    let validator_ids = [
        "validator-e2e-1",
        "validator-e2e-2",
        "validator-e2e-3",
        "validator-e2e-4",
    ];
    let dirs = validator_ids
        .iter()
        .map(|validator_id| temp_dir(validator_id))
        .collect::<Vec<_>>();

    let proposer =
        PersistentValidatorNode::bootstrap(validator_ids[0], defi_genesis_state(), dirs[0].path())
            .unwrap();
    let (proposer_addr, proposer_server) = spawn_tcp_persistent_node(proposer).unwrap();
    let mut proposer_client = TcpRpcClient::connect(proposer_addr).unwrap();

    submit_ok(
        &mut proposer_client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-four-validator-transfer-1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(11)],
        ),
    );
    let block_1 = produce_block(&mut proposer_client, 1, 1_000);
    assert_eq!(
        balance(&mut proposer_client, TOKEN_CONTRACT, "Bob", USDC),
        61
    );

    submit_ok(
        &mut proposer_client,
        tx_to(
            POOL_CONTRACT,
            "e2e-four-validator-liquidity-1",
            "Alice",
            2,
            Method::AddLiquidity,
            vec![amount(40), amount(80)],
        ),
    );
    let block_2 = produce_block(&mut proposer_client, 2, 2_000);
    assert_eq!(reserve(&mut proposer_client, USDC), 40);
    assert_eq!(reserve(&mut proposer_client, ATOM), 80);

    submit_ok(
        &mut proposer_client,
        tx_to(
            ORACLE_CONTRACT,
            "e2e-four-validator-oracle-price-1",
            REPORTER,
            1,
            Method::SubmitPrice,
            vec![asset(ATOM), amount(2), amount(0)],
        ),
    );
    let block_3 = produce_block(&mut proposer_client, 3, 3_000);

    submit_ok(
        &mut proposer_client,
        tx_to(
            VAULT_CONTRACT,
            "e2e-four-validator-lending-deposit-1",
            "Alice",
            3,
            Method::DepositCollateral,
            vec![asset(ATOM), amount(100)],
        ),
    );
    let block_4 = produce_block(&mut proposer_client, 4, 4_000);

    submit_ok(
        &mut proposer_client,
        tx_to(
            VAULT_CONTRACT,
            "e2e-four-validator-lending-borrow-1",
            "Alice",
            4,
            Method::Borrow,
            vec![asset(USDC), amount(50)],
        ),
    );
    let block_5 = produce_block(&mut proposer_client, 5, 5_000);

    submit_ok(
        &mut proposer_client,
        tx_to(
            STAKE_CONTRACT,
            "e2e-four-validator-stake-1",
            "Alice",
            5,
            Method::Stake,
            vec![asset(ATOM), amount(25)],
        ),
    );
    let block_6 = produce_block(&mut proposer_client, 6, 6_000);

    submit_ok(
        &mut proposer_client,
        tx_to(
            BRIDGE_CONTRACT,
            "e2e-four-validator-bridge-queue-1",
            "Alice",
            6,
            Method::QueueBridgeMessage,
            vec![
                text("ShardB"),
                text("BridgeB"),
                text("four-validator-outbound-1"),
                principal("Bob"),
                asset(USDC),
                amount(9),
            ],
        ),
    );
    let block_7 = produce_block(&mut proposer_client, 7, 7_000);

    submit_ok(
        &mut proposer_client,
        tx_to(
            GOVERNANCE_CONTRACT,
            "e2e-four-validator-governance-schedule-1",
            ADMIN,
            1,
            Method::ScheduleUpgrade,
            vec![text("e2e-four-validator-upgrade-1"), text("token-code-v2")],
        ),
    );
    let block_8 = produce_block(&mut proposer_client, 8, 8_000);

    let blocks = vec![
        block_1.clone(),
        block_2.clone(),
        block_3.clone(),
        block_4.clone(),
        block_5.clone(),
        block_6.clone(),
        block_7.clone(),
        block_8.clone(),
    ];
    let final_root = state_root(&mut proposer_client);
    assert_eq!(final_root, block_8.header.global_state_root);
    assert_broader_workload_state(&mut proposer_client, &block_8);

    proposer_client.close();
    proposer_server.join().unwrap();

    for (index, validator_id) in validator_ids.iter().enumerate().skip(1) {
        let node = PersistentValidatorNode::bootstrap(
            *validator_id,
            defi_genesis_state(),
            dirs[index].path(),
        )
        .unwrap();
        let (addr, server) = spawn_tcp_persistent_node(node).unwrap();
        let mut client = TcpRpcClient::connect(addr).unwrap();

        for block in &blocks {
            import_block(&mut client, block.clone());
        }
        assert_eq!(state_root(&mut client), final_root);
        assert_eq!(balance(&mut client, TOKEN_CONTRACT, "Bob", USDC), 61);
        assert_eq!(reserve(&mut client, USDC), 40);
        assert_eq!(reserve(&mut client, ATOM), 80);
        assert_eq!(blocks_page_highest(&mut client), 8);
        assert_storage_value(
            &mut client,
            StateKey::Balance {
                contract: TOKEN_CONTRACT.into(),
                owner: "Bob".into(),
                asset: USDC.into(),
            },
            61,
            &block_8.header.storage_root,
        );
        assert_broader_workload_state(&mut client, &block_8);

        client.close();
        server.join().unwrap();
    }

    for (index, validator_id) in validator_ids.iter().enumerate() {
        let restarted =
            PersistentValidatorNode::restart(*validator_id, dirs[index].path()).unwrap();
        let (addr, server) = spawn_tcp_persistent_node(restarted).unwrap();
        let mut client = TcpRpcClient::connect(addr).unwrap();

        assert_eq!(state_root(&mut client), final_root);
        assert_eq!(balance(&mut client, TOKEN_CONTRACT, "Bob", USDC), 61);
        assert_eq!(blocks_page_highest(&mut client), 8);
        assert_broader_workload_state(&mut client, &block_8);

        client.close();
        server.join().unwrap();
    }
}

fn submit_ok(client: &mut TcpRpcClient, transaction: Transaction) {
    assert!(matches!(
        client
            .ok(RpcRequest::SubmitTransaction { transaction })
            .unwrap(),
        RpcResult::Submitted
    ));
}

fn produce_block(client: &mut TcpRpcClient, height: u64, timestamp: u64) -> Block {
    match client
        .ok(RpcRequest::ProduceBlock { height, timestamp })
        .unwrap()
    {
        RpcResult::Block(block) => *block,
        result => panic!("expected block, got {result:?}"),
    }
}

fn import_block(client: &mut TcpRpcClient, block: Block) {
    assert!(matches!(
        client
            .ok(RpcRequest::ImportBlock {
                block: Box::new(block)
            })
            .unwrap(),
        RpcResult::Imported
    ));
}

fn state_root(client: &mut TcpRpcClient) -> String {
    match client.ok(RpcRequest::GetStateRoot).unwrap() {
        RpcResult::StateRoot(root) => root,
        result => panic!("expected state root, got {result:?}"),
    }
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
        result => panic!("expected amount, got {result:?}"),
    }
}

fn reserve(client: &mut TcpRpcClient, asset: &str) -> u128 {
    match client
        .ok(RpcRequest::GetStorageProof {
            key: StateKey::Reserve {
                contract: POOL_CONTRACT.into(),
                asset: asset.into(),
            },
        })
        .unwrap()
    {
        RpcResult::StorageProof(proof) => match proof.value {
            StateValue::UInt(value) => value,
        },
        result => panic!("expected storage proof, got {result:?}"),
    }
}

fn assert_storage_value(
    client: &mut TcpRpcClient,
    key: StateKey,
    expected: u128,
    storage_root: &str,
) {
    match client.ok(RpcRequest::GetStorageProof { key }).unwrap() {
        RpcResult::StorageProof(proof) => {
            assert_eq!(proof.value, StateValue::UInt(expected));
            assert_storage_proof_matches_root(&proof, storage_root);
        }
        result => panic!("expected storage proof, got {result:?}"),
    }
}

fn assert_broader_workload_state(client: &mut TcpRpcClient, final_block: &Block) {
    assert_storage_value(
        client,
        StateKey::Reserve {
            contract: POOL_CONTRACT.into(),
            asset: USDC.into(),
        },
        40,
        &final_block.header.storage_root,
    );
    assert_storage_value(
        client,
        StateKey::OraclePrice {
            contract: ORACLE_CONTRACT.into(),
            asset: ATOM.into(),
        },
        2,
        &final_block.header.storage_root,
    );
    assert_storage_value(
        client,
        StateKey::Debt {
            contract: VAULT_CONTRACT.into(),
            borrower: "Alice".into(),
            asset: USDC.into(),
        },
        50,
        &final_block.header.storage_root,
    );
    assert_storage_value(
        client,
        StateKey::StakeBalance {
            contract: STAKE_CONTRACT.into(),
            staker: "Alice".into(),
            asset: ATOM.into(),
        },
        25,
        &final_block.header.storage_root,
    );
    assert_eq!(balance(client, TOKEN_CONTRACT, "Alice", USDC), 190);
    assert_eq!(balance(client, TOKEN_CONTRACT, "Bob", USDC), 61);
    assert_eq!(balance(client, TOKEN_CONTRACT, POOL_CONTRACT, USDC), 40);
    assert_eq!(balance(client, TOKEN_CONTRACT, VAULT_CONTRACT, USDC), 950);
    assert_eq!(balance(client, TOKEN_CONTRACT, BRIDGE_CONTRACT, USDC), 9);
    assert_eq!(
        balance(client, SECONDARY_TOKEN_CONTRACT, "Alice", ATOM),
        795
    );
    assert_eq!(
        balance(client, SECONDARY_TOKEN_CONTRACT, POOL_CONTRACT, ATOM),
        80
    );
    assert_eq!(
        balance(client, SECONDARY_TOKEN_CONTRACT, VAULT_CONTRACT, ATOM),
        100
    );
    assert_eq!(
        balance(client, SECONDARY_TOKEN_CONTRACT, STAKE_CONTRACT, ATOM),
        25
    );
    assert!(scheduled_upgrade_exists(
        client,
        "e2e-four-validator-upgrade-1"
    ));
    assert_outbox_message(
        client,
        "four-validator-outbound-1",
        &final_block.header.outbox_root,
    );
}

fn scheduled_upgrade_exists(client: &mut TcpRpcClient, upgrade_id: &str) -> bool {
    match client.ok(RpcRequest::GetScheduledUpgrades).unwrap() {
        RpcResult::ScheduledUpgrades(upgrades) => upgrades
            .iter()
            .any(|upgrade| upgrade.upgrade_id == upgrade_id && !upgrade.executed),
        result => panic!("expected scheduled upgrades, got {result:?}"),
    }
}

fn assert_outbox_message(client: &mut TcpRpcClient, message_id: &str, outbox_root: &str) {
    match client
        .ok(RpcRequest::GetOutboxMessageProof { index: 0 })
        .unwrap()
    {
        RpcResult::OutboxMessageProof(proof) => {
            assert_eq!(proof.message.message_id, message_id);
            assert_outbox_proof_matches_root(&proof, outbox_root);
        }
        result => panic!("expected outbox proof, got {result:?}"),
    }
}

fn blocks_page_highest(client: &mut TcpRpcClient) -> u64 {
    match client
        .ok(RpcRequest::GetBlocksPage {
            start_height: 1,
            limit: 10,
        })
        .unwrap()
    {
        RpcResult::BlocksPage(page) => page.highest_height,
        result => panic!("expected blocks page, got {result:?}"),
    }
}
