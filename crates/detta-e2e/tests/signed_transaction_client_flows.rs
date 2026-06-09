use detta_core::{Argument, GrantKey, Method, TxStatus};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, text, tx_to, ACCOUNT_REGISTRY_CONTRACT,
    TOKEN_CONTRACT, USDC,
};
use detta_e2e::network::spawn_tcp_rpc_server;
use detta_rpc::{RpcErrorBody, RpcRequest, RpcResult, SignedClientTransaction};

#[test]
fn client_submits_ed25519_signed_transactions_and_rejects_tampering() {
    let signed = SignedClientTransaction::sign_with_seed(
        tx_to(
            TOKEN_CONTRACT,
            "e2e-signed-client-transfer-1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(12)],
        ),
        [7; 32],
    );
    let second_key_transfer = SignedClientTransaction::sign_with_seed(
        tx_to(
            TOKEN_CONTRACT,
            "e2e-signed-client-rotated-key-transfer-1",
            "Alice",
            3,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(5)],
        ),
        [8; 32],
    );
    let mut state = defi_genesis_state();
    state
        .register_account_key("Alice", signed.public_key_hex.clone())
        .unwrap();
    let (addr, server) = spawn_tcp_rpc_server(state).unwrap();
    let mut client = TcpRpcClient::connect(addr).unwrap();

    assert!(!signed.transaction.signature_ok);
    assert!(signed.verify());
    submit_signed_ok(&mut client, signed);

    let unregistered = SignedClientTransaction::sign_with_seed(
        tx_to(
            TOKEN_CONTRACT,
            "e2e-signed-client-unregistered-1",
            "Alice",
            2,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(1)],
        ),
        [9; 32],
    );
    assert!(unregistered.verify());
    assert_rpc_error(
        client
            .error(RpcRequest::SubmitSignedTransaction {
                signed: unregistered,
            })
            .unwrap(),
        "mempool.unauthorized_signer",
    );

    let register_second_key = SignedClientTransaction::sign_with_seed(
        tx_to(
            ACCOUNT_REGISTRY_CONTRACT,
            "e2e-signed-client-register-key-1",
            "Alice",
            2,
            Method::RegisterAccountKey,
            vec![text(&second_key_transfer.public_key_hex)],
        ),
        [7; 32],
    );
    submit_signed_ok(&mut client, register_second_key);

    let key_registration_block = produce_block(&mut client, 1, 1_000);
    assert_eq!(key_registration_block.transactions.len(), 2);
    assert_eq!(
        receipt_status(&mut client, "e2e-signed-client-transfer-1"),
        TxStatus::Committed
    );
    assert_eq!(
        receipt_status(&mut client, "e2e-signed-client-register-key-1"),
        TxStatus::Committed
    );
    assert_account_key_proof(
        &mut client,
        "Alice",
        &second_key_transfer.public_key_hex,
        &key_registration_block.header.registry_root,
    );

    submit_signed_ok(&mut client, second_key_transfer.clone());
    let rotated_key_block = produce_block(&mut client, 2, 2_000);
    assert_eq!(rotated_key_block.transactions.len(), 1);
    assert_eq!(
        receipt_status(&mut client, "e2e-signed-client-rotated-key-transfer-1"),
        TxStatus::Committed
    );

    let revoke_second_key = SignedClientTransaction::sign_with_seed(
        tx_to(
            ACCOUNT_REGISTRY_CONTRACT,
            "e2e-signed-client-revoke-key-1",
            "Alice",
            4,
            Method::RevokeAccountKey,
            vec![text(&second_key_transfer.public_key_hex)],
        ),
        [8; 32],
    );
    submit_signed_ok(&mut client, revoke_second_key);
    assert_eq!(
        receipt_status_after_block(&mut client, 3, 3_000, "e2e-signed-client-revoke-key-1"),
        TxStatus::Committed
    );

    let post_revoke = SignedClientTransaction::sign_with_seed(
        tx_to(
            TOKEN_CONTRACT,
            "e2e-signed-client-revoked-key-transfer-1",
            "Alice",
            5,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(1)],
        ),
        [8; 32],
    );
    assert_rpc_error(
        client
            .error(RpcRequest::SubmitSignedTransaction {
                signed: post_revoke,
            })
            .unwrap(),
        "mempool.unauthorized_signer",
    );

    let mut tampered = SignedClientTransaction::sign_with_seed(
        tx_to(
            TOKEN_CONTRACT,
            "e2e-signed-client-tampered-1",
            "Alice",
            6,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(1)],
        ),
        [7; 32],
    );
    tampered.transaction.args[2] = Argument::Amount(2);
    assert!(!tampered.verify());
    assert_rpc_error(
        client
            .error(RpcRequest::SubmitSignedTransaction { signed: tampered })
            .unwrap(),
        "mempool.invalid_signature",
    );

    let mut expired_transaction = tx_to(
        TOKEN_CONTRACT,
        "e2e-signed-client-expired-transfer-1",
        "Alice",
        7,
        Method::Transfer,
        vec![principal("Bob"), asset(USDC), amount(1)],
    );
    expired_transaction.valid_until_height = Some(3);
    assert_rpc_error(
        client
            .error(RpcRequest::SubmitSignedTransaction {
                signed: SignedClientTransaction::sign_with_seed(expired_transaction, [7; 32]),
            })
            .unwrap(),
        "mempool.transaction_expired",
    );

    assert!(stored_transaction_signature_ok(
        &mut client,
        "e2e-signed-client-transfer-1"
    ));
    assert!(stored_transaction_signature_ok(
        &mut client,
        "e2e-signed-client-rotated-key-transfer-1"
    ));
    assert_eq!(balance(&mut client, "Bob"), 67);

    client.close();
    server.join().unwrap();
}

fn submit_signed_ok(client: &mut TcpRpcClient, signed: SignedClientTransaction) {
    assert!(matches!(
        client
            .ok(RpcRequest::SubmitSignedTransaction { signed })
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

fn receipt_status_after_block(
    client: &mut TcpRpcClient,
    height: u64,
    timestamp: u64,
    tx_hash: &str,
) -> TxStatus {
    produce_block(client, height, timestamp);
    receipt_status(client, tx_hash)
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

fn assert_account_key_proof(
    client: &mut TcpRpcClient,
    account: &str,
    public_key_hex: &str,
    expected_registry_root: &str,
) {
    let key = GrantKey::AccountSigner {
        account: account.into(),
        public_key_hex: public_key_hex.into(),
    };
    match client.ok(RpcRequest::GetRegistryProof { key }).unwrap() {
        RpcResult::RegistryProof(proof) => {
            assert!(proof.verify());
            assert_eq!(proof.proof.root, expected_registry_root);
            assert!(proof.grant.active);
            assert!(!proof.grant.revoked);
        }
        result => panic!("expected registry proof, got {result:?}"),
    }
}

fn stored_transaction_signature_ok(client: &mut TcpRpcClient, tx_hash: &str) -> bool {
    match client
        .ok(RpcRequest::GetTransaction {
            tx_hash: tx_hash.into(),
        })
        .unwrap()
    {
        RpcResult::Transaction(transaction) => transaction.signature_ok,
        result => panic!("expected transaction, got {result:?}"),
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

fn assert_rpc_error(error: RpcErrorBody, code: &str) {
    assert_eq!(error.code, code);
}
