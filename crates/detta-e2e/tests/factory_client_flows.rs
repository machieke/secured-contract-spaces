use detta_core::{
    ContractKind, EventPayload, ExecutionError, Method, StateKey, StateValue, TxStatus,
};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, text, tx_to, FACTORY_CONTRACT, TOKEN_CONTRACT,
    USDC,
};
use detta_e2e::network::spawn_tcp_rpc_server;
use detta_e2e::proofs::assert_storage_proof_matches_root;
use detta_rpc::{RpcRequest, RpcResult};

const CLIENT_TOKEN: &str = "ClientToken";
const CLIENT_POOL: &str = "ClientPool";
const CLIENT_ASSET: &str = "CLT";

#[test]
fn client_deploys_token_creates_liquidity_and_swaps_on_new_pool() {
    let (addr, server) = spawn_tcp_rpc_server(defi_genesis_state()).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();

    submit_ok(
        &mut client,
        tx_to(
            FACTORY_CONTRACT,
            "e2e-factory-deploy-token-1",
            "Issuer",
            1,
            Method::DeployToken,
            vec![
                text(CLIENT_TOKEN),
                asset(CLIENT_ASSET),
                principal("Alice"),
                amount(1_000),
            ],
        ),
    );
    let token_block = produce_block(&mut client, 1, 1_000);
    assert_committed(&mut client, "e2e-factory-deploy-token-1");
    assert_eq!(
        contract_kind(&mut client, CLIENT_TOKEN),
        ContractKind::Token
    );
    assert_eq!(total_supply(&mut client, CLIENT_TOKEN, CLIENT_ASSET), 1_000);
    assert_eq!(
        balance(&mut client, CLIENT_TOKEN, "Alice", CLIENT_ASSET),
        1_000
    );
    assert!(matches!(
        events(&mut client).last().map(|event| &event.payload),
        Some(EventPayload::ContractDeployed { contract, kind, .. })
            if contract == CLIENT_TOKEN && kind == "token"
    ));

    submit_ok(
        &mut client,
        tx_to(
            FACTORY_CONTRACT,
            "e2e-factory-duplicate-token-1",
            "Issuer",
            2,
            Method::DeployToken,
            vec![
                text(CLIENT_TOKEN),
                asset(CLIENT_ASSET),
                principal("Issuer"),
                amount(1),
            ],
        ),
    );
    let duplicate_token_block = produce_block(&mut client, 2, 2_000);
    assert_reverted(
        &mut client,
        "e2e-factory-duplicate-token-1",
        ExecutionError::InvalidArguments,
    );
    assert_eq!(
        duplicate_token_block.header.storage_root,
        token_block.header.storage_root
    );

    submit_ok(
        &mut client,
        tx_to(
            FACTORY_CONTRACT,
            "e2e-factory-deploy-pool-1",
            "Issuer",
            3,
            Method::DeployAmmPool,
            vec![text(CLIENT_POOL), asset(CLIENT_ASSET), asset(USDC)],
        ),
    );
    produce_block(&mut client, 3, 3_000);
    assert_committed(&mut client, "e2e-factory-deploy-pool-1");
    assert_eq!(
        contract_kind(&mut client, CLIENT_POOL),
        ContractKind::AmmPool {
            asset_a: CLIENT_ASSET.into(),
            asset_b: USDC.into(),
            swap_fee_bps: 30,
        }
    );

    submit_ok(
        &mut client,
        tx_to(
            FACTORY_CONTRACT,
            "e2e-factory-duplicate-pool-1",
            "Issuer",
            4,
            Method::DeployAmmPool,
            vec![text(CLIENT_POOL), asset(CLIENT_ASSET), asset(USDC)],
        ),
    );
    produce_block(&mut client, 4, 4_000);
    assert_reverted(
        &mut client,
        "e2e-factory-duplicate-pool-1",
        ExecutionError::InvalidArguments,
    );

    submit_ok(
        &mut client,
        tx_to(
            CLIENT_POOL,
            "e2e-factory-pool-liquidity-1",
            "Alice",
            1,
            Method::AddLiquidity,
            vec![amount(100), amount(50)],
        ),
    );
    let liquidity_block = produce_block(&mut client, 5, 5_000);
    assert_committed(&mut client, "e2e-factory-pool-liquidity-1");
    assert_storage_value(
        &mut client,
        StateKey::Reserve {
            contract: CLIENT_POOL.into(),
            asset: CLIENT_ASSET.into(),
        },
        100,
        &liquidity_block.header.storage_root,
    );
    assert_storage_value(
        &mut client,
        StateKey::Reserve {
            contract: CLIENT_POOL.into(),
            asset: USDC.into(),
        },
        50,
        &liquidity_block.header.storage_root,
    );
    assert_eq!(
        balance(&mut client, CLIENT_TOKEN, "Alice", CLIENT_ASSET),
        900
    );
    assert_eq!(
        balance(&mut client, CLIENT_TOKEN, CLIENT_POOL, CLIENT_ASSET),
        100
    );
    assert_eq!(balance(&mut client, TOKEN_CONTRACT, "Alice", USDC), 150);
    assert_eq!(balance(&mut client, TOKEN_CONTRACT, CLIENT_POOL, USDC), 50);

    submit_ok(
        &mut client,
        tx_to(
            CLIENT_POOL,
            "e2e-factory-pool-buy-1",
            "Bob",
            1,
            Method::Swap,
            vec![asset(USDC), amount(10), amount(15)],
        ),
    );
    produce_block(&mut client, 6, 6_000);
    let buy_receipt = receipt(&mut client, "e2e-factory-pool-buy-1");
    assert_eq!(buy_receipt.status, TxStatus::Committed);
    assert_eq!(reserve(&mut client, CLIENT_POOL, CLIENT_ASSET), 85);
    assert_eq!(reserve(&mut client, CLIENT_POOL, USDC), 60);
    assert_eq!(balance(&mut client, CLIENT_TOKEN, "Bob", CLIENT_ASSET), 15);
    assert_eq!(balance(&mut client, TOKEN_CONTRACT, "Bob", USDC), 40);

    submit_ok(
        &mut client,
        tx_to(
            CLIENT_POOL,
            "e2e-factory-pool-sell-1",
            "Bob",
            2,
            Method::Swap,
            vec![asset(CLIENT_ASSET), amount(15), amount(8)],
        ),
    );
    let sell_block = produce_block(&mut client, 7, 7_000);
    assert_committed(&mut client, "e2e-factory-pool-sell-1");
    assert_eq!(reserve(&mut client, CLIENT_POOL, CLIENT_ASSET), 100);
    assert_eq!(reserve(&mut client, CLIENT_POOL, USDC), 52);
    assert_eq!(balance(&mut client, CLIENT_TOKEN, "Bob", CLIENT_ASSET), 0);
    assert_eq!(balance(&mut client, TOKEN_CONTRACT, "Bob", USDC), 48);

    assert!(matches!(
        events(&mut client).last().map(|event| &event.payload),
        Some(EventPayload::Swap {
            input_asset,
            output_asset,
            amount_out: 8,
            ..
        }) if input_asset == CLIENT_ASSET && output_asset == USDC
    ));
    assert_eq!(
        storage_proof(
            &mut client,
            StateKey::Reserve {
                contract: CLIENT_POOL.into(),
                asset: CLIENT_ASSET.into(),
            },
        )
        .proof
        .root,
        sell_block.header.storage_root
    );

    client.close();
    server.join().unwrap();
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

fn contract_kind(client: &mut TcpRpcClient, contract: &str) -> ContractKind {
    match client
        .ok(RpcRequest::GetContract {
            contract: contract.into(),
        })
        .unwrap()
    {
        RpcResult::Contract(record) => record.kind,
        result => panic!("expected contract descriptor, got {result:?}"),
    }
}

fn total_supply(client: &mut TcpRpcClient, contract: &str, asset: &str) -> u128 {
    match client
        .ok(RpcRequest::GetTotalSupply {
            contract: contract.into(),
            asset: asset.into(),
        })
        .unwrap()
    {
        RpcResult::Amount(amount) => amount,
        result => panic!("expected amount, got {result:?}"),
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

fn reserve(client: &mut TcpRpcClient, contract: &str, asset: &str) -> u128 {
    match storage_proof(
        client,
        StateKey::Reserve {
            contract: contract.into(),
            asset: asset.into(),
        },
    )
    .value
    {
        StateValue::UInt(value) => value,
    }
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

fn events(client: &mut TcpRpcClient) -> Vec<detta_core::Event> {
    match client.ok(RpcRequest::GetEvents).unwrap() {
        RpcResult::Events(events) => events,
        result => panic!("expected events, got {result:?}"),
    }
}
