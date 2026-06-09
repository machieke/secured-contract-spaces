use detta_core::{Method, TxStatus};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, temp_dir, tx_to, TOKEN_CONTRACT, USDC,
};
use detta_e2e::network::spawn_tcp_persistent_node;
use detta_node::PersistentValidatorNode;
use detta_rpc::{OperatorAlertPolicy, OperatorAlertSeverity, RpcRequest, RpcResult};

#[test]
fn client_observes_operator_metrics_and_induced_alerts() {
    let validator_dir = temp_dir("e2e-operator-observability");
    let mut node = PersistentValidatorNode::bootstrap(
        "validator-1",
        defi_genesis_state(),
        validator_dir.path(),
    )
    .unwrap();
    node.set_operator_alert_policy(OperatorAlertPolicy {
        min_peer_count: 1,
        max_finality_lag: 0,
        max_storage_bytes: u64::MAX,
        max_rpc_error_count: 0,
        max_mempool_size: 0,
        max_latest_block_failure_ratio_per_mille: 0,
    });

    let (addr, server) = spawn_tcp_persistent_node(node).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();

    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-operator-reverted-transfer-1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(10_000)],
        ),
    );
    produce_block(&mut client, 1, 1_000);
    assert_eq!(
        receipt_status(&mut client, "e2e-operator-reverted-transfer-1"),
        TxStatus::Reverted
    );

    assert!(client.error(RpcRequest::GetBlock { height: 99 }).is_ok());

    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-operator-pending-transfer-1",
            "Alice",
            2,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(1)],
        ),
    );

    let metrics = match client.ok(RpcRequest::GetOperatorMetrics).unwrap() {
        RpcResult::OperatorMetrics(metrics) => metrics,
        result => panic!("expected operator metrics, got {result:?}"),
    };
    assert_eq!(metrics.validator_id.as_deref(), Some("validator-1"));
    assert_eq!(metrics.peer_count, Some(0));
    assert_eq!(metrics.consensus_height, 1);
    assert_eq!(metrics.highest_finalized_height, None);
    assert_eq!(metrics.finality_lag, None);
    assert_eq!(metrics.mempool_size, 1);
    assert_eq!(metrics.rpc_error_count, 1);
    assert!(metrics.storage_bytes.unwrap_or_default() > 0);

    let report = match client.ok(RpcRequest::GetOperatorAlerts).unwrap() {
        RpcResult::OperatorAlerts(report) => report,
        result => panic!("expected operator alerts, got {result:?}"),
    };
    let codes: Vec<_> = report
        .alerts
        .iter()
        .map(|alert| (alert.code.as_str(), alert.severity.clone()))
        .collect();
    assert_eq!(
        codes,
        vec![
            ("operator.peer_isolation", OperatorAlertSeverity::Critical),
            (
                "operator.stalled_consensus",
                OperatorAlertSeverity::Critical
            ),
            ("operator.excessive_reverts", OperatorAlertSeverity::Warning),
            ("operator.rpc_overload", OperatorAlertSeverity::Warning),
            (
                "operator.mempool_saturation",
                OperatorAlertSeverity::Warning
            ),
        ]
    );
    assert!(!report.root_mismatch);
    assert_eq!(report.slashing_record_count, 0);
    assert_eq!(report.latest_block_failure_count, 1);
    assert_eq!(report.latest_block_receipt_count, 1);
    assert_eq!(report.metrics.mempool_size, 1);
    assert_eq!(report.metrics.rpc_error_count, 1);

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
