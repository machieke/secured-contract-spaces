use detta_core::{Block, DeTTaState, Method};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, temp_dir, tx_to, CHAIN_ID, TOKEN_CONTRACT, USDC,
};
use detta_e2e::network::{spawn_snapshot_sync_peer, spawn_tcp_persistent_node};
use detta_network::TcpProtocolStream;
use detta_node::{
    fetch_verified_snapshot_chunk_set_over_tcp_with_metrics, PersistentValidatorNode,
};
use detta_rpc::{RpcRequest, RpcResult};

#[test]
fn client_observes_state_synced_node_matching_source_state() {
    let source_dir = temp_dir("e2e-state-sync-source");
    let sink_dir = temp_dir("e2e-state-sync-sink");

    let source_node = PersistentValidatorNode::bootstrap(
        "validator-source",
        defi_genesis_state(),
        source_dir.path(),
    )
    .unwrap();
    let (source_rpc_addr, source_rpc_server) = spawn_tcp_persistent_node(source_node).unwrap();
    let mut source_client = TcpRpcClient::connect(source_rpc_addr).unwrap();
    submit_ok(
        &mut source_client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-state-sync-transfer-1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(13)],
        ),
    );
    let source_block = produce_block(&mut source_client, 1, 1_000);
    assert_eq!(balance(&mut source_client, "Bob"), 63);
    let source_root = state_root(&mut source_client);
    assert_eq!(source_root, source_block.header.global_state_root);
    source_client.close();
    source_rpc_server.join().unwrap();

    let source_node =
        PersistentValidatorNode::restart("validator-source", source_dir.path()).unwrap();
    let required_metadata_roots = source_node.snapshot_metadata_roots().unwrap();
    assert!(!required_metadata_roots.is_empty());
    let (sync_addr, sync_server) = spawn_snapshot_sync_peer(source_node, 64).unwrap();
    let mut sync_stream = TcpProtocolStream::connect(sync_addr).unwrap();
    let (chunk_set, sync_metrics) = fetch_verified_snapshot_chunk_set_over_tcp_with_metrics(
        &mut sync_stream,
        source_root.clone(),
        2,
        &required_metadata_roots,
    )
    .unwrap();
    drop(sync_stream);
    sync_server.join().unwrap();
    assert!(sync_metrics.metadata_roots_verified);
    assert_eq!(sync_metrics.chunks_received, chunk_set.chunks.len() as u32);

    let sink_node = PersistentValidatorNode::bootstrap(
        "validator-sink",
        DeTTaState::new(CHAIN_ID),
        sink_dir.path(),
    )
    .unwrap();
    let imported = sink_node
        .import_snapshot_chunk_set(&chunk_set, &required_metadata_roots)
        .unwrap();
    assert_eq!(imported.global_state_root, source_root);
    sink_node
        .persist_snapshot_sync_client_metrics(&sync_metrics)
        .unwrap();
    drop(sink_node);

    let synced_node = PersistentValidatorNode::restart("validator-sink", sink_dir.path()).unwrap();
    let (synced_addr, synced_server) = spawn_tcp_persistent_node(synced_node).unwrap();
    let mut synced_client = TcpRpcClient::connect(synced_addr).unwrap();

    assert_eq!(state_root(&mut synced_client), source_root);
    assert_eq!(balance(&mut synced_client, "Bob"), 63);
    let required_roots = match synced_client
        .ok(RpcRequest::GetRequiredSnapshotMetadataRoots)
        .unwrap()
    {
        RpcResult::RequiredSnapshotMetadataRoots(report) => report,
        result => panic!("expected required metadata roots, got {result:?}"),
    };
    assert_eq!(required_roots.roots, required_metadata_roots);
    assert!(!required_roots.root.is_empty());

    let metrics = match synced_client
        .ok(RpcRequest::GetSnapshotSyncClientMetrics)
        .unwrap()
    {
        RpcResult::SnapshotSyncClientMetrics(Some(metrics)) => metrics,
        result => panic!("expected persisted sync metrics, got {result:?}"),
    };
    assert_eq!(metrics.chunks_received, sync_metrics.chunks_received);
    assert!(metrics.metadata_roots_verified);
    assert_eq!(
        metrics.required_metadata_roots_root,
        sync_metrics.required_metadata_roots_root
    );

    let import_records = match synced_client
        .ok(RpcRequest::GetSnapshotImportAuditRecords {
            offset: 0,
            limit: 10,
        })
        .unwrap()
    {
        RpcResult::SnapshotImportAuditRecords(records) => records,
        result => panic!("expected snapshot import audit records, got {result:?}"),
    };
    assert_eq!(import_records.len(), 1);
    assert_eq!(import_records[0].snapshot_root, source_root);
    assert!(import_records[0].metadata_roots_verified);

    synced_client.close();
    synced_server.join().unwrap();
}

fn submit_ok(client: &mut TcpRpcClient, transaction: detta_core::Transaction) {
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

fn balance(client: &mut TcpRpcClient, owner: &str) -> u128 {
    match client
        .ok(RpcRequest::GetBalance {
            contract: TOKEN_CONTRACT.into(),
            owner: owner.into(),
            asset: USDC.into(),
        })
        .unwrap()
    {
        RpcResult::Amount(amount) => amount,
        result => panic!("expected amount, got {result:?}"),
    }
}

fn state_root(client: &mut TcpRpcClient) -> String {
    match client.ok(RpcRequest::GetStateRoot).unwrap() {
        RpcResult::StateRoot(root) => root,
        result => panic!("expected state root, got {result:?}"),
    }
}
