use detta_core::{Method, StateKey, StateValue, Transaction, TxStatus};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, temp_dir, tx_to, TOKEN_CONTRACT, USDC,
};
use detta_e2e::network::spawn_tcp_persistent_node;
use detta_e2e::proofs::assert_storage_proof_matches_root;
use detta_network::{Envelope, NetworkError, NetworkMessage, TcpProtocolStream};
use detta_node::{NetworkIngestOutcome, PersistentValidatorNode};
use detta_protocol::{encode_message, ProtocolError, PROTOCOL_MAGIC};
use detta_rpc::{RpcRequest, RpcResult};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::thread;

#[test]
fn corrupt_tcp_protocol_frame_is_rejected_without_poisoning_valid_gossip() {
    let validator_dir = temp_dir("e2e-tcp-protocol-hardening");
    let mut node = PersistentValidatorNode::bootstrap(
        "validator-1",
        defi_genesis_state(),
        validator_dir.path(),
    )
    .unwrap();

    reject_corrupt_frame_over_tcp(tx_to(
        TOKEN_CONTRACT,
        "e2e-corrupt-frame-transfer-ignored-1",
        "Alice",
        1,
        Method::Transfer,
        vec![principal("Bob"), asset(USDC), amount(25)],
    ));
    assert_eq!(node.pending_len(), 0);

    receive_valid_transaction_over_tcp(
        &mut node,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-after-corrupt-frame-transfer-1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(25)],
        ),
    );
    assert_eq!(node.pending_len(), 1);
    drop(node);

    let restarted = PersistentValidatorNode::restart("validator-1", validator_dir.path()).unwrap();
    let (rpc_addr, rpc_server) = spawn_tcp_persistent_node(restarted).unwrap();
    let mut client = TcpRpcClient::connect(rpc_addr).unwrap();

    let block = produce_block(&mut client, 1, 1_000);
    assert_eq!(
        receipt_status(&mut client, "e2e-after-corrupt-frame-transfer-1"),
        TxStatus::Committed
    );
    assert_storage_value(
        &mut client,
        StateKey::Balance {
            contract: TOKEN_CONTRACT.into(),
            owner: "Bob".into(),
            asset: USDC.into(),
        },
        75,
        &block.header.storage_root,
    );

    client.close();
    rpc_server.join().unwrap();
}

fn reject_corrupt_frame_over_tcp(transaction: Transaction) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let sender = thread::spawn(move || {
        let mut bytes = encode_message(&NetworkMessage::Transaction(transaction)).unwrap();
        bytes[0] = b'X';
        let mut stream = TcpStream::connect(addr).unwrap();
        stream.write_all(&bytes).unwrap();
        stream.flush().unwrap();
    });
    let (stream, _) = listener.accept().unwrap();
    let mut tcp = TcpProtocolStream::from_stream(stream);
    assert_eq!(
        tcp.receive().unwrap_err(),
        NetworkError::Protocol(ProtocolError::BadMagic {
            actual: [
                b'X',
                PROTOCOL_MAGIC[1],
                PROTOCOL_MAGIC[2],
                PROTOCOL_MAGIC[3],
            ],
        })
    );
    sender.join().unwrap();
}

fn receive_valid_transaction_over_tcp(
    node: &mut PersistentValidatorNode,
    transaction: Transaction,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let sender = thread::spawn(move || {
        let mut tcp = TcpProtocolStream::connect(addr).unwrap();
        tcp.send(&NetworkMessage::Transaction(transaction)).unwrap();
    });
    let (stream, _) = listener.accept().unwrap();
    let mut tcp = TcpProtocolStream::from_stream(stream);
    let envelope = Envelope {
        from: "valid-peer-after-corrupt-frame".into(),
        to: "validator-1".into(),
        message: tcp.receive().unwrap(),
    };
    assert_eq!(
        node.ingest_network_envelope(&envelope).unwrap(),
        NetworkIngestOutcome::TransactionAccepted
    );
    sender.join().unwrap();
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
