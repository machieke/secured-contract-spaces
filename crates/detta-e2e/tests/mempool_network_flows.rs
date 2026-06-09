use detta_core::{Method, StateKey, StateValue, Transaction, TxStatus};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, temp_dir, tx_to, TOKEN_CONTRACT, USDC,
};
use detta_e2e::network::spawn_tcp_persistent_node;
use detta_e2e::proofs::assert_storage_proof_matches_root;
use detta_network::{Envelope, NetworkMessage, TcpProtocolStream};
use detta_node::{NetworkIngestOutcome, PersistentValidatorNode};
use detta_rpc::{RpcRequest, RpcResult};
use std::net::TcpListener;
use std::thread;

#[test]
fn tcp_transaction_gossip_survives_restart_and_commits_through_rpc() {
    let validator_dir = temp_dir("e2e-mempool-network-validator");
    let mut node = PersistentValidatorNode::bootstrap(
        "validator-1",
        defi_genesis_state(),
        validator_dir.path(),
    )
    .unwrap();

    let transaction = tx_to(
        TOKEN_CONTRACT,
        "e2e-network-gossiped-transfer-1",
        "Alice",
        1,
        Method::Transfer,
        vec![principal("Bob"), asset(USDC), amount(15)],
    );
    receive_transaction_over_tcp(&mut node, transaction);
    drop(node);

    let restarted = PersistentValidatorNode::restart("validator-1", validator_dir.path()).unwrap();
    let (rpc_addr, rpc_server) = spawn_tcp_persistent_node(restarted).unwrap();
    let mut client = TcpRpcClient::connect(rpc_addr).unwrap();

    assert_eq!(mempool_pending(&mut client), 1);
    let block = produce_block(&mut client, 1, 1_000);
    assert_eq!(mempool_pending(&mut client), 0);
    assert_eq!(
        receipt_status(&mut client, "e2e-network-gossiped-transfer-1"),
        TxStatus::Committed
    );
    assert_eq!(balance(&mut client, "Bob"), 65);
    assert_storage_value(
        &mut client,
        StateKey::Balance {
            contract: TOKEN_CONTRACT.into(),
            owner: "Bob".into(),
            asset: USDC.into(),
        },
        65,
        &block.header.storage_root,
    );

    client.close();
    rpc_server.join().unwrap();
}

fn receive_transaction_over_tcp(node: &mut PersistentValidatorNode, transaction: Transaction) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let sender = thread::spawn(move || {
        let mut tcp = TcpProtocolStream::connect(addr).unwrap();
        tcp.send(&NetworkMessage::Transaction(transaction)).unwrap();
    });
    let (stream, _) = listener.accept().unwrap();
    let mut tcp = TcpProtocolStream::from_stream(stream);
    let envelope = Envelope {
        from: "client-peer-1".into(),
        to: "validator-1".into(),
        message: tcp.receive().unwrap(),
    };
    assert_eq!(
        node.ingest_network_envelope(&envelope).unwrap(),
        NetworkIngestOutcome::TransactionAccepted
    );
    sender.join().unwrap();
}

fn mempool_pending(client: &mut TcpRpcClient) -> usize {
    match client.ok(RpcRequest::GetMempoolStatus).unwrap() {
        RpcResult::MempoolStatus(status) => status.pending_transactions,
        result => panic!("expected mempool status, got {result:?}"),
    }
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
        result => panic!("expected balance amount, got {result:?}"),
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
