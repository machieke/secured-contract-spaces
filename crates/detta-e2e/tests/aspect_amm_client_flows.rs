use detta_core::{ContractKind, EventPayload, Method, ReturnValue, StateKey, StateValue, TxStatus};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, text, tx_to, FACTORY_CONTRACT, TOKEN_CONTRACT,
    USDC,
};
use detta_e2e::network::spawn_tcp_rpc_server;
use detta_e2e::proofs::assert_storage_proof_matches_root;
use detta_rpc::{RpcRequest, RpcResult};

const ASPECT_SOURCE: &str =
    include_str!("../../../models/aspects/stdlib/minimal-transfer-token.metta");
const ASPECT_TOKEN: &str = "AspectPoolToken";
const ASPECT_POOL: &str = "AspectTokenPool";

#[test]
fn client_deploys_aspect_token_creates_liquidity_and_swaps_pool_asset() {
    let (addr, server) = spawn_tcp_rpc_server(defi_genesis_state()).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();

    submit_ok(
        &mut client,
        tx_to(
            FACTORY_CONTRACT,
            "e2e-aspect-amm-submit-module-1",
            "Issuer",
            1,
            Method::SubmitAspectModule,
            vec![text("ERC20ConformantToken"), text(ASPECT_SOURCE)],
        ),
    );
    produce_block(&mut client, 1, 1_000);
    assert_committed(&mut client, "e2e-aspect-amm-submit-module-1");

    let module = aspect_modules(&mut client)
        .into_iter()
        .find(|module| module.module_id == "ERC20ConformantToken")
        .expect("submitted aspect module should be visible");
    let artifacts = aspect_module_artifacts(&mut client, &module.module_hash);
    assert!(artifacts
        .bundle_ids
        .contains(&"ERC20ConformantToken".into()));
    assert!(artifacts
        .abi
        .contains_key("ERC20ConformantToken::ERC20-transfer"));

    submit_ok(
        &mut client,
        tx_to(
            FACTORY_CONTRACT,
            "e2e-aspect-amm-deploy-token-1",
            "Issuer",
            2,
            Method::DeployAspectContract,
            vec![
                text(ASPECT_TOKEN),
                text(&module.module_hash),
                text("ERC20ConformantToken"),
                text("ERC20-initialize"),
                principal("Alice"),
                amount(1_000),
            ],
        ),
    );
    let token_block = produce_block(&mut client, 2, 2_000);
    assert_committed(&mut client, "e2e-aspect-amm-deploy-token-1");
    assert!(matches!(
        contract_kind(&mut client, ASPECT_TOKEN),
        ContractKind::AspectModule {
            module_hash,
            bundle_id,
            ..
        } if module_hash == module.module_hash && bundle_id == "ERC20ConformantToken"
    ));
    assert_aspect_balance(
        &mut client,
        "Alice",
        1_000,
        &token_block.header.storage_root,
    );

    submit_ok(
        &mut client,
        tx_to(
            FACTORY_CONTRACT,
            "e2e-aspect-amm-deploy-pool-1",
            "Issuer",
            3,
            Method::DeployAmmPool,
            vec![text(ASPECT_POOL), asset(ASPECT_TOKEN), asset(USDC)],
        ),
    );
    produce_block(&mut client, 3, 3_000);
    assert_committed(&mut client, "e2e-aspect-amm-deploy-pool-1");
    assert_eq!(
        contract_kind(&mut client, ASPECT_POOL),
        ContractKind::AmmPool {
            asset_a: ASPECT_TOKEN.into(),
            asset_b: USDC.into(),
            swap_fee_bps: 30,
        }
    );

    submit_ok(
        &mut client,
        tx_to(
            ASPECT_POOL,
            "e2e-aspect-amm-add-liquidity-1",
            "Alice",
            1,
            Method::AddLiquidity,
            vec![amount(100), amount(50)],
        ),
    );
    let liquidity_block = produce_block(&mut client, 4, 4_000);
    let liquidity_receipt = receipt(&mut client, "e2e-aspect-amm-add-liquidity-1");
    assert_eq!(liquidity_receipt.status, TxStatus::Committed);
    assert_eq!(liquidity_receipt.return_value, Some(ReturnValue::UInt(150)));
    assert_storage_value(
        &mut client,
        StateKey::Reserve {
            contract: ASPECT_POOL.into(),
            asset: ASPECT_TOKEN.into(),
        },
        100,
        &liquidity_block.header.storage_root,
    );
    assert_storage_value(
        &mut client,
        StateKey::Reserve {
            contract: ASPECT_POOL.into(),
            asset: USDC.into(),
        },
        50,
        &liquidity_block.header.storage_root,
    );
    assert_aspect_balance(
        &mut client,
        "Alice",
        900,
        &liquidity_block.header.storage_root,
    );
    assert_aspect_balance(
        &mut client,
        ASPECT_POOL,
        100,
        &liquidity_block.header.storage_root,
    );
    assert_native_balance(&mut client, "Alice", 150);
    assert_native_balance(&mut client, ASPECT_POOL, 50);

    submit_ok(
        &mut client,
        tx_to(
            ASPECT_POOL,
            "e2e-aspect-amm-buy-token-1",
            "Bob",
            1,
            Method::Swap,
            vec![asset(USDC), amount(10), amount(15)],
        ),
    );
    let buy_block = produce_block(&mut client, 5, 5_000);
    let buy_receipt = receipt(&mut client, "e2e-aspect-amm-buy-token-1");
    assert_eq!(buy_receipt.status, TxStatus::Committed);
    assert_eq!(buy_receipt.return_value, Some(ReturnValue::UInt(15)));
    assert_eq!(reserve(&mut client, ASPECT_TOKEN), 85);
    assert_eq!(reserve(&mut client, USDC), 60);
    assert_eq!(amm_fee_collected(&mut client, USDC), 1);
    assert_aspect_balance(&mut client, "Bob", 15, &buy_block.header.storage_root);
    assert_aspect_balance(&mut client, ASPECT_POOL, 85, &buy_block.header.storage_root);
    assert_native_balance(&mut client, "Bob", 40);
    assert_native_balance(&mut client, ASPECT_POOL, 60);

    submit_ok(
        &mut client,
        tx_to(
            ASPECT_POOL,
            "e2e-aspect-amm-sell-token-1",
            "Bob",
            2,
            Method::Swap,
            vec![asset(ASPECT_TOKEN), amount(15), amount(8)],
        ),
    );
    let sell_block = produce_block(&mut client, 6, 6_000);
    let sell_receipt = receipt(&mut client, "e2e-aspect-amm-sell-token-1");
    assert_eq!(sell_receipt.status, TxStatus::Committed);
    assert_eq!(sell_receipt.return_value, Some(ReturnValue::UInt(8)));
    assert_eq!(reserve(&mut client, ASPECT_TOKEN), 100);
    assert_eq!(reserve(&mut client, USDC), 52);
    assert_eq!(amm_fee_collected(&mut client, ASPECT_TOKEN), 1);
    assert_aspect_balance(&mut client, "Bob", 0, &sell_block.header.storage_root);
    assert_aspect_balance(
        &mut client,
        ASPECT_POOL,
        100,
        &sell_block.header.storage_root,
    );
    assert_native_balance(&mut client, "Bob", 48);
    assert_native_balance(&mut client, ASPECT_POOL, 52);
    assert!(matches!(
        events(&mut client).last().map(|event| &event.payload),
        Some(EventPayload::Swap {
            input_asset,
            output_asset,
            amount_in: 15,
            amount_out: 8,
            ..
        }) if input_asset == ASPECT_TOKEN && output_asset == USDC
    ));
    assert_storage_value(
        &mut client,
        StateKey::Reserve {
            contract: ASPECT_POOL.into(),
            asset: ASPECT_TOKEN.into(),
        },
        100,
        &sell_block.header.storage_root,
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

fn aspect_modules(client: &mut TcpRpcClient) -> Vec<detta_core::AspectModuleRecord> {
    match client.ok(RpcRequest::GetAspectModules).unwrap() {
        RpcResult::AspectModules(modules) => modules,
        result => panic!("expected aspect modules, got {result:?}"),
    }
}

fn aspect_module_artifacts(
    client: &mut TcpRpcClient,
    module_hash: &str,
) -> detta_rpc::AspectModuleArtifactReport {
    match client
        .ok(RpcRequest::GetAspectModuleArtifacts {
            module_hash: module_hash.into(),
        })
        .unwrap()
    {
        RpcResult::AspectModuleArtifacts(report) => *report,
        result => panic!("expected aspect module artifacts, got {result:?}"),
    }
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

fn reserve(client: &mut TcpRpcClient, asset: &str) -> u128 {
    match storage_proof(
        client,
        StateKey::Reserve {
            contract: ASPECT_POOL.into(),
            asset: asset.into(),
        },
    )
    .value
    {
        StateValue::UInt(value) => value,
    }
}

fn amm_fee_collected(client: &mut TcpRpcClient, asset: &str) -> u128 {
    match storage_proof(
        client,
        StateKey::AmmFeeCollected {
            contract: ASPECT_POOL.into(),
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

fn assert_aspect_balance(
    client: &mut TcpRpcClient,
    owner: &str,
    expected_value: u128,
    expected_root: &str,
) {
    let proof = storage_proof(
        client,
        StateKey::AspectState {
            contract: ASPECT_TOKEN.into(),
            aspect: "StaticBalanceAspect".into(),
            state: "balanceOf".into(),
            key: vec![owner.into()],
        },
    );
    assert_eq!(proof.value, StateValue::UInt(expected_value));
    assert_storage_proof_matches_root(&proof, expected_root);
}

fn assert_native_balance(client: &mut TcpRpcClient, owner: &str, expected_value: u128) {
    match client
        .ok(RpcRequest::GetBalance {
            contract: TOKEN_CONTRACT.into(),
            owner: owner.into(),
            asset: USDC.into(),
        })
        .unwrap()
    {
        RpcResult::Amount(amount) => assert_eq!(amount, expected_value),
        result => panic!("expected native balance amount, got {result:?}"),
    }
}

fn events(client: &mut TcpRpcClient) -> Vec<detta_core::Event> {
    match client.ok(RpcRequest::GetEvents).unwrap() {
        RpcResult::Events(events) => events,
        result => panic!("expected events, got {result:?}"),
    }
}
