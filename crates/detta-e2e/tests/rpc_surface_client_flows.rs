use detta_consensus::EquivocationEvidence;
use detta_core::{Block, Method, Transaction};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, temp_dir, tx_to, CHAIN_ID, TOKEN_CONTRACT, USDC,
};
use detta_e2e::network::{spawn_tcp_persistent_node, spawn_tcp_rpc_server};
use detta_node::PersistentValidatorNode;
use detta_protocol::{ProtocolMessage, ValidatorSetMetadataUpdate, ValidatorSigningKey};
use detta_rpc::{RpcRequest, RpcResult};

#[test]
fn client_exercises_remaining_public_rpc_surface() {
    let imported_block = block_for_import();
    let (import_addr, import_server) = spawn_tcp_rpc_server(defi_genesis_state()).unwrap();
    let mut import_client = TcpRpcClient::connect(import_addr).unwrap();
    assert!(matches!(
        import_client
            .ok(RpcRequest::ImportBlock {
                block: Box::new(imported_block.clone())
            })
            .unwrap(),
        RpcResult::Imported
    ));
    assert_eq!(balance(&mut import_client, "Bob"), 57);
    import_client.close();
    import_server.join().unwrap();

    let dir = temp_dir("e2e-rpc-surface-validator-metadata");
    let signer_key = validator_key("validator-1", 7);
    let peer_key = validator_key("validator-2", 8);
    let added_key = validator_key("validator-3", 9);
    let node = PersistentValidatorNode::bootstrap_with_validator_set(
        "validator-1",
        defi_genesis_state(),
        dir.path(),
        "detta-testnet",
        vec![signer_key.public_key(), peer_key.public_key()],
    )
    .unwrap();
    let expected_slashing = node
        .persist_equivocation_evidence(EquivocationEvidence {
            validator_id: "validator-1".into(),
            height: 11,
            first_block_hash: "block-a".into(),
            second_block_hash: "block-b".into(),
        })
        .unwrap();

    let update = ValidatorSetMetadataUpdate {
        update_id: "e2e-validator-set-update-1".into(),
        add_validators: vec![added_key.public_key()],
        remove_validators: vec![],
        expires_at_height: None,
    };
    let signer_authorization = signer_key
        .sign_message(
            "detta-testnet",
            CHAIN_ID,
            ProtocolMessage::ValidatorSetMetadataUpdate(update.clone()),
        )
        .unwrap();
    let peer_authorization = peer_key
        .sign_message(
            "detta-testnet",
            CHAIN_ID,
            ProtocolMessage::ValidatorSetMetadataUpdate(update),
        )
        .unwrap();

    let (addr, server) = spawn_tcp_persistent_node(node).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();

    match client
        .ok(RpcRequest::GetSlashingRecord {
            validator_id: "validator-1".into(),
        })
        .unwrap()
    {
        RpcResult::SlashingRecord(record) => assert_eq!(*record, expected_slashing),
        result => panic!("expected slashing record, got {result:?}"),
    }

    match client
        .ok(RpcRequest::ProposeValidatorSetMetadataUpdate {
            authorization: signer_authorization,
        })
        .unwrap()
    {
        RpcResult::ValidatorSetMetadataUpdateStatus(status) => {
            assert_eq!(status.update_id, "e2e-validator-set-update-1");
            assert_eq!(status.pending_authorizations, 1);
            assert_eq!(status.required_quorum, 2);
            assert!(!status.applied);
        }
        result => panic!("expected validator metadata status, got {result:?}"),
    }
    match client
        .ok(RpcRequest::GetValidatorSetMetadataUpdateStatus {
            update_id: "e2e-validator-set-update-1".into(),
        })
        .unwrap()
    {
        RpcResult::ValidatorSetMetadataUpdateStatus(status) => {
            assert_eq!(status.pending_authorizations, 1);
            assert!(!status.applied);
        }
        result => panic!("expected validator metadata status, got {result:?}"),
    }
    match client
        .ok(RpcRequest::ProposeValidatorSetMetadataUpdate {
            authorization: peer_authorization,
        })
        .unwrap()
    {
        RpcResult::ValidatorSetMetadataUpdateStatus(status) => {
            assert_eq!(status.pending_authorizations, 0);
            assert!(status.applied);
            assert_eq!(status.required_quorum, 3);
        }
        result => panic!("expected applied validator metadata status, got {result:?}"),
    }
    match client
        .ok(RpcRequest::GetValidatorSetMetadataAuditRecords {
            offset: 0,
            limit: 10,
        })
        .unwrap()
    {
        RpcResult::ValidatorSetMetadataAuditRecords(records) => {
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].update_id, "e2e-validator-set-update-1");
        }
        result => panic!("expected validator metadata audit records, got {result:?}"),
    }
    match client.ok(RpcRequest::GetSnapshotImportAuditRoot).unwrap() {
        RpcResult::SnapshotImportAuditRoot(root) => assert!(!root.is_empty()),
        result => panic!("expected snapshot import audit root, got {result:?}"),
    }
    assert!(matches!(
        client
            .ok(RpcRequest::GetSnapshotImportAuditConfigRoot)
            .unwrap(),
        RpcResult::SnapshotImportAuditConfigRoot(None)
    ));
    match client.ok(RpcRequest::GetSnapshotImportAuditConfig).unwrap() {
        RpcResult::SnapshotImportAuditConfig(config) => {
            assert!(config.max_records > 0);
            assert!(config.max_page_size > 0);
        }
        result => panic!("expected snapshot import audit config, got {result:?}"),
    }

    client.close();
    server.join().unwrap();
}

fn block_for_import() -> Block {
    let (addr, server) = spawn_tcp_rpc_server(defi_genesis_state()).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();
    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-import-source-transfer-1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(7)],
        ),
    );
    let block = produce_block(&mut client, 1, 1_000);
    client.close();
    server.join().unwrap();
    block
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

fn validator_key(validator_id: &str, seed_byte: u8) -> ValidatorSigningKey {
    ValidatorSigningKey::from_seed(validator_id, "consensus-key-1", [seed_byte; 32])
}
