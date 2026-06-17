use detta_consensus::FinalityCertificate;
use detta_core::{GrantKey, Method, StateKey, StateValue, TxStatus};
use detta_da::DaProductionProfile;
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, temp_dir, tx_to, TOKEN_CONTRACT, USDC,
    VALIDATOR_ID,
};
use detta_e2e::network::spawn_tcp_persistent_node;
use detta_e2e::proofs::assert_receipt_proof_matches_root;
use detta_node::PersistentValidatorNode;
use detta_rpc::{RpcRequest, RpcResult};
use detta_storage::FileStorage;

#[test]
fn client_observes_persistent_node_restart_finality_and_backup_restore() {
    let validator_dir = temp_dir("persistent-validator");
    let backup_dir = temp_dir("persistent-backup");
    let restore_dir = temp_dir("persistent-restore");

    let node = PersistentValidatorNode::bootstrap(
        VALIDATOR_ID,
        defi_genesis_state(),
        validator_dir.path(),
    )
    .unwrap();
    let (addr, server) = spawn_tcp_persistent_node(node).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();

    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-persistent-transfer-1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(5)],
        ),
    );
    assert_eq!(mempool_pending(&mut client), 1);
    let block = produce_block(&mut client, 1, 1_000);
    assert_eq!(block.transactions.len(), 1);
    assert_eq!(mempool_pending(&mut client), 0);
    assert_eq!(
        receipt_status(&mut client, "e2e-persistent-transfer-1"),
        TxStatus::Committed
    );
    assert_eq!(balance(&mut client, "Bob"), 55);

    let receipt_proof = receipt_proof(&mut client, 1, 0);
    assert_eq!(receipt_proof.receipt.tx_hash, "e2e-persistent-transfer-1");
    assert_receipt_proof_matches_root(&receipt_proof, &block.header.receipt_root);

    let metrics = match client.ok(RpcRequest::GetOperatorMetrics).unwrap() {
        RpcResult::OperatorMetrics(metrics) => metrics,
        result => panic!("expected operator metrics, got {result:?}"),
    };
    assert_eq!(metrics.validator_id.as_deref(), Some(VALIDATOR_ID));
    assert_eq!(
        metrics.da_production_profile,
        Some(DaProductionProfile::v1())
    );
    assert_eq!(metrics.consensus_height, 1);
    assert_eq!(metrics.mempool_size, 0);
    assert!(metrics.last_block_execution_micros.is_some());
    assert!(metrics.storage_bytes.unwrap_or_default() > 0);

    let snapshot_roots = match client
        .ok(RpcRequest::GetPersistentNodeSnapshotRoots)
        .unwrap()
    {
        RpcResult::PersistentNodeSnapshotRoots(roots) => roots,
        result => panic!("expected persistent snapshot roots, got {result:?}"),
    };
    assert_eq!(
        snapshot_roots.global_state_root,
        block.header.global_state_root
    );
    assert_eq!(snapshot_roots.storage_root, block.header.storage_root);

    client.close();
    server.join().unwrap();

    let mut restarted =
        PersistentValidatorNode::restart(VALIDATOR_ID, validator_dir.path()).unwrap();
    let certificate = FinalityCertificate {
        height: 1,
        block_hash: block.block_hash(),
        signers: vec![VALIDATOR_ID.into()],
    };
    restarted
        .persist_finality_certificate(&certificate)
        .unwrap();
    drop(restarted);

    let storage = FileStorage::open(validator_dir.path()).unwrap();
    let manifest = storage.backup_to(backup_dir.path()).unwrap();
    assert_eq!(
        manifest.snapshot_root,
        Some(block.header.global_state_root.clone())
    );
    assert_eq!(manifest.highest_block_height, Some(1));
    assert_eq!(manifest.highest_finality_certificate_height, Some(1));
    assert!(manifest.file_count >= 3);
    assert!(manifest.total_bytes > 0);
    drop(storage);

    let restored_storage =
        FileStorage::restore_from_backup(backup_dir.path(), restore_dir.path()).unwrap();
    assert_eq!(
        restored_storage.load_finality_certificate(1).unwrap(),
        certificate
    );
    drop(restored_storage);

    let restored_node = PersistentValidatorNode::restart(VALIDATOR_ID, restore_dir.path()).unwrap();
    let (restored_addr, restored_server) = spawn_tcp_persistent_node(restored_node).unwrap();
    let mut restored_client = TcpRpcClient::connect(restored_addr).unwrap();

    assert_eq!(balance(&mut restored_client, "Bob"), 55);
    assert_eq!(
        block_result(
            restored_client
                .ok(RpcRequest::GetBlock { height: 1 })
                .unwrap()
        ),
        block
    );
    assert_eq!(finality_certificate(&mut restored_client, 1), certificate);
    assert_eq!(
        transaction_hash(&mut restored_client, "e2e-persistent-transfer-1"),
        "e2e-persistent-transfer-1"
    );
    assert_eq!(
        receipt_status(&mut restored_client, "e2e-persistent-transfer-1"),
        TxStatus::Committed
    );
    assert_eq!(
        snapshot_root(&mut restored_client),
        block.header.global_state_root
    );
    assert_eq!(
        blocks_page_highest(&mut restored_client),
        block.header.height
    );
    assert_eq!(events_page_total(&mut restored_client), 1);
    assert_storage_value(
        &mut restored_client,
        StateKey::Balance {
            contract: TOKEN_CONTRACT.into(),
            owner: "Bob".into(),
            asset: USDC.into(),
        },
        55,
        &block.header.storage_root,
    );
    assert!(storage_non_inclusion_proof(
        &mut restored_client,
        StateKey::Balance {
            contract: TOKEN_CONTRACT.into(),
            owner: "Carol".into(),
            asset: USDC.into(),
        },
    )
    .verify());
    assert!(registry_non_inclusion_proof(
        &mut restored_client,
        GrantKey::Allowance {
            contract: TOKEN_CONTRACT.into(),
            owner: "Alice".into(),
            spender: "Mallory".into(),
            asset: USDC.into(),
        },
    )
    .verify());

    let restored_metrics = match restored_client.ok(RpcRequest::GetOperatorMetrics).unwrap() {
        RpcResult::OperatorMetrics(metrics) => metrics,
        result => panic!("expected restored operator metrics, got {result:?}"),
    };
    assert_eq!(restored_metrics.highest_finalized_height, Some(1));
    assert_eq!(
        restored_metrics.da_production_profile,
        Some(DaProductionProfile::v1())
    );
    assert_eq!(restored_metrics.finality_lag, Some(0));

    let alerts = match restored_client.ok(RpcRequest::GetOperatorAlerts).unwrap() {
        RpcResult::OperatorAlerts(alerts) => alerts,
        result => panic!("expected operator alerts, got {result:?}"),
    };
    assert!(!alerts.root_mismatch);
    assert_eq!(alerts.slashing_record_count, 0);

    let metadata_status = match restored_client
        .ok(RpcRequest::GetSnapshotMetadataRootStatus)
        .unwrap()
    {
        RpcResult::SnapshotMetadataRootStatus(status) => status,
        result => panic!("expected snapshot metadata root status, got {result:?}"),
    };
    assert!(!metadata_status.validator_set_metadata_audit_root.is_empty());

    assert!(matches!(
        restored_client
            .ok(RpcRequest::GetSnapshotSyncClientMetrics)
            .unwrap(),
        RpcResult::SnapshotSyncClientMetrics(None)
    ));
    let required_roots = match restored_client
        .ok(RpcRequest::GetRequiredSnapshotMetadataRoots)
        .unwrap()
    {
        RpcResult::RequiredSnapshotMetadataRoots(report) => report,
        result => panic!("expected required snapshot metadata roots, got {result:?}"),
    };
    assert!(required_roots.roots.is_empty());
    assert!(!required_roots.root.is_empty());

    restored_client.close();
    restored_server.join().unwrap();
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

fn mempool_pending(client: &mut TcpRpcClient) -> usize {
    match client.ok(RpcRequest::GetMempoolStatus).unwrap() {
        RpcResult::MempoolStatus(status) => status.pending_transactions,
        result => panic!("expected mempool status, got {result:?}"),
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

fn receipt_status(client: &mut TcpRpcClient, tx_hash: &str) -> TxStatus {
    match client
        .ok(RpcRequest::GetReceipt {
            tx_hash: tx_hash.into(),
        })
        .unwrap()
    {
        RpcResult::Receipt(receipt) => receipt.status,
        result => panic!("expected receipt, got {result:?}"),
    }
}

fn receipt_proof(client: &mut TcpRpcClient, height: u64, index: usize) -> detta_core::ReceiptProof {
    match client
        .ok(RpcRequest::GetReceiptProof { height, index })
        .unwrap()
    {
        RpcResult::ReceiptProof(proof) => *proof,
        result => panic!("expected receipt proof, got {result:?}"),
    }
}

fn block_result(result: RpcResult) -> detta_core::Block {
    match result {
        RpcResult::Block(block) => *block,
        result => panic!("expected block, got {result:?}"),
    }
}

fn finality_certificate(client: &mut TcpRpcClient, height: u64) -> FinalityCertificate {
    match client
        .ok(RpcRequest::GetFinalityCertificate { height })
        .unwrap()
    {
        RpcResult::FinalityCertificate(certificate) => *certificate,
        result => panic!("expected finality certificate, got {result:?}"),
    }
}

fn transaction_hash(client: &mut TcpRpcClient, tx_hash: &str) -> String {
    match client
        .ok(RpcRequest::GetTransaction {
            tx_hash: tx_hash.into(),
        })
        .unwrap()
    {
        RpcResult::Transaction(transaction) => transaction.tx_hash,
        result => panic!("expected transaction, got {result:?}"),
    }
}

fn snapshot_root(client: &mut TcpRpcClient) -> String {
    match client.ok(RpcRequest::GetSnapshot).unwrap() {
        RpcResult::Snapshot(snapshot) => snapshot.global_state_root,
        result => panic!("expected snapshot, got {result:?}"),
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
        RpcResult::BlocksPage(page) => {
            assert_eq!(page.blocks.len(), 1);
            page.highest_height
        }
        result => panic!("expected blocks page, got {result:?}"),
    }
}

fn events_page_total(client: &mut TcpRpcClient) -> usize {
    match client
        .ok(RpcRequest::GetEventsPage {
            offset: 0,
            limit: 10,
        })
        .unwrap()
    {
        RpcResult::EventsPage(page) => {
            assert_eq!(page.events.len(), 1);
            page.total_events
        }
        result => panic!("expected events page, got {result:?}"),
    }
}

fn storage_non_inclusion_proof(
    client: &mut TcpRpcClient,
    key: StateKey,
) -> detta_core::StorageNonInclusionProof {
    match client
        .ok(RpcRequest::GetStorageNonInclusionProof { key })
        .unwrap()
    {
        RpcResult::StorageNonInclusionProof(proof) => *proof,
        result => panic!("expected storage non-inclusion proof, got {result:?}"),
    }
}

fn registry_non_inclusion_proof(
    client: &mut TcpRpcClient,
    key: GrantKey,
) -> detta_core::RegistryNonInclusionProof {
    match client
        .ok(RpcRequest::GetRegistryNonInclusionProof { key })
        .unwrap()
    {
        RpcResult::RegistryNonInclusionProof(proof) => *proof,
        result => panic!("expected registry non-inclusion proof, got {result:?}"),
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
    assert!(proof.verify());
    assert_eq!(proof.proof.root, expected_root);
}
