use detta_core::{ContractKind, EventPayload, Method, StateKey, StateValue, TxStatus};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{amount, defi_genesis_state, principal, text, tx_to, FACTORY_CONTRACT};
use detta_e2e::network::spawn_tcp_rpc_server;
use detta_e2e::proofs::assert_storage_proof_matches_root;
use detta_rpc::{RpcRequest, RpcResult};

const ASPECT_SOURCE: &str =
    include_str!("../../../models/aspects/stdlib/minimal-transfer-token.metta");
const ASPECT_TOKEN: &str = "ClientAspectToken";

#[test]
fn client_submits_inspects_deploys_and_calls_aspect_token() {
    let (addr, server) = spawn_tcp_rpc_server(defi_genesis_state()).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();

    submit_ok(
        &mut client,
        tx_to(
            FACTORY_CONTRACT,
            "e2e-aspect-submit-module-1",
            "Issuer",
            1,
            Method::SubmitAspectModule,
            vec![text("ERC20ConformantToken"), text(ASPECT_SOURCE)],
        ),
    );
    produce_block(&mut client, 1, 1_000);
    assert_committed(&mut client, "e2e-aspect-submit-module-1");

    let module = aspect_modules(&mut client)
        .into_iter()
        .find(|module| module.module_id == "ERC20ConformantToken")
        .expect("submitted aspect module should be visible");
    assert!(module.canonical_source.is_some());
    assert!(module.ir.is_some());

    let proof = match client
        .ok(RpcRequest::GetAspectModuleProof {
            module_hash: module.module_hash.clone(),
        })
        .unwrap()
    {
        RpcResult::AspectModuleProof(proof) => *proof,
        result => panic!("expected aspect module proof, got {result:?}"),
    };
    assert!(proof.verify());

    let artifacts = aspect_module_artifacts(&mut client, &module.module_hash);
    assert_eq!(artifacts.module_hash, module.module_hash);
    assert!(artifacts.has_canonical_source);
    assert!(artifacts.has_ir);
    assert!(artifacts
        .bundle_ids
        .contains(&"ERC20ConformantToken".into()));
    assert!(artifacts
        .abi
        .contains_key("ERC20ConformantToken::ERC20-transfer"));
    assert!(artifacts
        .policies
        .contains_key("ERC20ConformantToken::ERC20-transfer"));
    assert!(artifacts
        .storage_schema
        .contains_key("StaticBalanceAspect::balanceOf"));
    assert!(artifacts
        .invariants
        .contains_key("TransferableBalanceAspect::TBA-3"));

    submit_ok(
        &mut client,
        tx_to(
            FACTORY_CONTRACT,
            "e2e-aspect-deploy-token-1",
            "Issuer",
            2,
            Method::DeployAspectContract,
            vec![
                text(ASPECT_TOKEN),
                text(&module.module_hash),
                text("ERC20ConformantToken"),
                text("ERC20-initialize"),
                principal("Alice"),
                amount(100),
            ],
        ),
    );
    produce_block(&mut client, 2, 2_000);
    assert_committed(&mut client, "e2e-aspect-deploy-token-1");
    assert!(matches!(
        contract_kind(&mut client, ASPECT_TOKEN),
        ContractKind::AspectModule {
            module_hash,
            bundle_id,
            ..
        } if module_hash == module.module_hash && bundle_id == "ERC20ConformantToken"
    ));

    submit_ok(
        &mut client,
        tx_to(
            ASPECT_TOKEN,
            "e2e-aspect-transfer-1",
            "Alice",
            1,
            Method::Other("ERC20-transfer".into()),
            vec![principal("Bob"), amount(25)],
        ),
    );
    let transfer_block = produce_block(&mut client, 3, 3_000);
    assert_committed(&mut client, "e2e-aspect-transfer-1");
    assert_aspect_balance(
        &mut client,
        "Alice",
        75,
        &transfer_block.header.storage_root,
    );
    assert_aspect_balance(&mut client, "Bob", 25, &transfer_block.header.storage_root);
    assert!(matches!(
        events(&mut client).last().map(|event| &event.payload),
        Some(EventPayload::AspectEvent { module_hash, event })
            if module_hash == &module.module_hash && event == "(Transfer Alice Bob 25)"
    ));

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

fn assert_aspect_balance(
    client: &mut TcpRpcClient,
    owner: &str,
    expected_value: u128,
    expected_root: &str,
) {
    let proof = match client
        .ok(RpcRequest::GetStorageProof {
            key: StateKey::AspectState {
                contract: ASPECT_TOKEN.into(),
                aspect: "StaticBalanceAspect".into(),
                state: "balanceOf".into(),
                key: vec![owner.into()],
            },
        })
        .unwrap()
    {
        RpcResult::StorageProof(proof) => *proof,
        result => panic!("expected storage proof, got {result:?}"),
    };
    assert_eq!(proof.value, StateValue::UInt(expected_value));
    assert_storage_proof_matches_root(&proof, expected_root);
}

fn events(client: &mut TcpRpcClient) -> Vec<detta_core::Event> {
    match client.ok(RpcRequest::GetEvents).unwrap() {
        RpcResult::Events(events) => events,
        result => panic!("expected events, got {result:?}"),
    }
}
