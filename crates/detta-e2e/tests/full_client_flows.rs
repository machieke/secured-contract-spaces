use detta_core::{
    ContractInvariant, ContractKind, EventPayload, ExecutionError, GrantKey, Method, ReturnValue,
    StateKey, StateValue, TxStatus,
};
use detta_e2e::client::{ClientError, HttpRpcClient, TcpRpcClient};
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, invalid_signature_tx, principal, tx_to, ATOM, CHAIN_ID,
    POOL_CONTRACT, SECONDARY_TOKEN_CONTRACT, TOKEN_CONTRACT, USDC,
};
use detta_e2e::network::{spawn_http_rpc_server, spawn_tcp_rpc_server};
use detta_e2e::proofs::{
    assert_event_proof_matches_root, assert_receipt_proof_matches_root,
    assert_registry_proof_matches_root, assert_storage_proof_matches_root,
};
use detta_rpc::{RpcErrorBody, RpcRequest, RpcResponse, RpcResult, SubscriptionTopic};

#[test]
fn http_and_tcp_clients_execute_token_and_amm_flows() {
    let (http_addr, http_server) = spawn_http_rpc_server(defi_genesis_state(), 1).unwrap();
    let http_client = HttpRpcClient::new(http_addr);
    let (status, response) = http_client.post(RpcRequest::GetNodeHealth).unwrap();
    assert_eq!(status, 200);
    match response {
        RpcResponse::Ok(RpcResult::NodeHealth(report)) => {
            assert_eq!(report.chain_id, CHAIN_ID);
            assert_eq!(report.height, 0);
            assert_eq!(report.pending_mempool_transactions, 0);
        }
        response => panic!("expected node health response, got {response:?}"),
    }
    http_server.join().unwrap();

    let (tcp_addr, tcp_server) = spawn_tcp_rpc_server(defi_genesis_state()).unwrap();
    let mut client = TcpRpcClient::connect(tcp_addr).unwrap();

    let subscription = match client
        .ok(RpcRequest::Subscribe {
            topics: vec![
                SubscriptionTopic::Blocks,
                SubscriptionTopic::Receipts,
                SubscriptionTopic::Events,
            ],
        })
        .unwrap()
    {
        RpcResult::SubscriptionStatus(subscription) => subscription,
        result => panic!("expected subscription status, got {result:?}"),
    };

    assert_eq!(
        amount_result(client.ok(RpcRequest::GetTotalSupply {
            contract: TOKEN_CONTRACT.into(),
            asset: USDC.into(),
        })),
        2_250
    );
    assert_eq!(token_balance(&mut client, "Alice"), 200);
    assert_eq!(token_balance(&mut client, "Bob"), 50);

    let token_contract = contract_result(client.ok(RpcRequest::GetContract {
        contract: TOKEN_CONTRACT.into(),
    }));
    assert_eq!(token_contract.contract_id, TOKEN_CONTRACT);
    assert_eq!(token_contract.kind, ContractKind::Token);
    assert!(token_contract
        .exported_methods()
        .contains(&Method::Transfer));
    assert!(token_contract
        .declared_invariants()
        .contains(&ContractInvariant::TokenSupplyMatchesBalances));

    let pool_contract = contract_result(client.ok(RpcRequest::GetContract {
        contract: POOL_CONTRACT.into(),
    }));
    assert_eq!(
        pool_contract.kind,
        ContractKind::AmmPool {
            asset_a: USDC.into(),
            asset_b: ATOM.into(),
            swap_fee_bps: 30,
        }
    );
    assert!(pool_contract
        .declared_invariants()
        .contains(&ContractInvariant::AmmLpSupplyMatchesBalances));

    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-token-transfer-1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(25)],
        ),
    );
    assert_eq!(pending_mempool_transactions(&mut client), 1);
    let transfer_block = produce_block(&mut client, 1, 1_000);
    let transfer_receipt = receipt(&mut client, "e2e-token-transfer-1");
    assert_eq!(transfer_receipt.status, TxStatus::Committed);
    assert_eq!(token_balance(&mut client, "Alice"), 175);
    assert_eq!(token_balance(&mut client, "Bob"), 75);

    let bob_balance_key = StateKey::Balance {
        contract: TOKEN_CONTRACT.into(),
        owner: "Bob".into(),
        asset: USDC.into(),
    };
    let bob_balance_proof = storage_proof(&mut client, bob_balance_key);
    assert_eq!(bob_balance_proof.value, StateValue::UInt(75));
    assert_storage_proof_matches_root(&bob_balance_proof, &transfer_block.header.storage_root);

    let transfer_receipt_proof = receipt_proof(&mut client, 1, 0);
    assert_eq!(
        transfer_receipt_proof.receipt.tx_hash,
        "e2e-token-transfer-1"
    );
    assert_receipt_proof_matches_root(&transfer_receipt_proof, &transfer_block.header.receipt_root);

    let transfer_event_proof = event_proof(&mut client, 0);
    assert!(matches!(
        transfer_event_proof.event.payload,
        EventPayload::Transfer { amount: 25, .. }
    ));
    assert_event_proof_matches_root(&transfer_event_proof, &transfer_block.header.event_root);

    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-token-approve-1",
            "Alice",
            2,
            Method::Approve,
            vec![principal("Bob"), asset(USDC), amount(40)],
        ),
    );
    let approve_block = produce_block(&mut client, 2, 2_000);
    let allowance_key = GrantKey::Allowance {
        contract: TOKEN_CONTRACT.into(),
        owner: "Alice".into(),
        spender: "Bob".into(),
        asset: USDC.into(),
    };
    let allowance_proof = registry_proof(&mut client, allowance_key.clone());
    assert_eq!(allowance_proof.grant.limit, Some(40));
    assert_eq!(allowance_proof.grant.spent, 0);
    assert_registry_proof_matches_root(&allowance_proof, &approve_block.header.registry_root);

    submit_ok(
        &mut client,
        tx_to(
            TOKEN_CONTRACT,
            "e2e-token-transfer-from-1",
            "Bob",
            1,
            Method::TransferFrom,
            vec![
                principal("Alice"),
                principal("Carol"),
                asset(USDC),
                amount(30),
            ],
        ),
    );
    let transfer_from_block = produce_block(&mut client, 3, 3_000);
    let transfer_from_receipt = receipt(&mut client, "e2e-token-transfer-from-1");
    assert_eq!(transfer_from_receipt.status, TxStatus::Committed);
    assert_eq!(token_balance(&mut client, "Alice"), 145);
    assert_eq!(token_balance(&mut client, "Carol"), 30);
    let allowance_after_spend = registry_proof(&mut client, allowance_key);
    assert_eq!(allowance_after_spend.grant.limit, Some(40));
    assert_eq!(allowance_after_spend.grant.spent, 30);
    assert_registry_proof_matches_root(
        &allowance_after_spend,
        &transfer_from_block.header.registry_root,
    );

    let replay_error = client
        .error(RpcRequest::SubmitTransaction {
            transaction: tx_to(
                TOKEN_CONTRACT,
                "e2e-token-replay-1",
                "Alice",
                1,
                Method::Transfer,
                vec![principal("Bob"), asset(USDC), amount(1)],
            ),
        })
        .unwrap();
    assert_rpc_error(&replay_error, "mempool.nonce_already_used");

    let bad_signature_error = client
        .error(RpcRequest::SubmitTransaction {
            transaction: invalid_signature_tx(tx_to(
                TOKEN_CONTRACT,
                "e2e-token-bad-signature-1",
                "Alice",
                99,
                Method::Transfer,
                vec![principal("Bob"), asset(USDC), amount(1)],
            )),
        })
        .unwrap();
    assert_rpc_error(&bad_signature_error, "mempool.invalid_signature");
    assert_eq!(pending_mempool_transactions(&mut client), 0);

    submit_ok(
        &mut client,
        tx_to(
            POOL_CONTRACT,
            "e2e-amm-add-liquidity-1",
            "Alice",
            3,
            Method::AddLiquidity,
            vec![amount(50), amount(100)],
        ),
    );
    let liquidity_block = produce_block(&mut client, 4, 4_000);
    let liquidity_receipt = receipt(&mut client, "e2e-amm-add-liquidity-1");
    assert_eq!(liquidity_receipt.status, TxStatus::Committed);
    assert_eq!(liquidity_receipt.return_value, Some(ReturnValue::UInt(150)));
    assert_eq!(reserve(&mut client, USDC), 50);
    assert_eq!(reserve(&mut client, ATOM), 100);
    assert_eq!(token_balance(&mut client, "Alice"), 95);
    assert_eq!(token_balance(&mut client, POOL_CONTRACT), 50);
    assert_eq!(
        asset_balance(&mut client, SECONDARY_TOKEN_CONTRACT, "Alice", ATOM),
        900
    );
    assert_eq!(
        asset_balance(&mut client, SECONDARY_TOKEN_CONTRACT, POOL_CONTRACT, ATOM),
        100
    );
    let reserve_proof = storage_proof(
        &mut client,
        StateKey::Reserve {
            contract: POOL_CONTRACT.into(),
            asset: USDC.into(),
        },
    );
    assert_eq!(reserve_proof.value, StateValue::UInt(50));
    assert_storage_proof_matches_root(&reserve_proof, &liquidity_block.header.storage_root);

    submit_ok(
        &mut client,
        tx_to(
            POOL_CONTRACT,
            "e2e-amm-buy-atom-1",
            "Bob",
            2,
            Method::Swap,
            vec![asset(USDC), amount(10), amount(15)],
        ),
    );
    let buy_block = produce_block(&mut client, 5, 5_000);
    let buy_receipt = receipt(&mut client, "e2e-amm-buy-atom-1");
    assert_eq!(buy_receipt.status, TxStatus::Committed);
    assert_eq!(buy_receipt.return_value, Some(ReturnValue::UInt(15)));
    assert_eq!(reserve(&mut client, USDC), 60);
    assert_eq!(reserve(&mut client, ATOM), 85);
    assert_eq!(token_balance(&mut client, "Bob"), 65);
    assert_eq!(
        asset_balance(&mut client, SECONDARY_TOKEN_CONTRACT, "Bob", ATOM),
        1_015
    );
    let fee_proof = storage_proof(
        &mut client,
        StateKey::AmmFeeCollected {
            contract: POOL_CONTRACT.into(),
            asset: USDC.into(),
        },
    );
    assert_eq!(fee_proof.value, StateValue::UInt(1));
    assert_storage_proof_matches_root(&fee_proof, &buy_block.header.storage_root);

    submit_ok(
        &mut client,
        tx_to(
            POOL_CONTRACT,
            "e2e-amm-sell-atom-1",
            "Bob",
            3,
            Method::Swap,
            vec![asset(ATOM), amount(15), amount(8)],
        ),
    );
    let sell_block = produce_block(&mut client, 6, 6_000);
    let sell_receipt = receipt(&mut client, "e2e-amm-sell-atom-1");
    assert_eq!(sell_receipt.status, TxStatus::Committed);
    assert_eq!(sell_receipt.return_value, Some(ReturnValue::UInt(8)));
    assert_eq!(reserve(&mut client, USDC), 52);
    assert_eq!(reserve(&mut client, ATOM), 100);
    assert_eq!(token_balance(&mut client, "Bob"), 73);
    assert_eq!(
        asset_balance(&mut client, SECONDARY_TOKEN_CONTRACT, "Bob", ATOM),
        1_000
    );
    let sell_fee_proof = storage_proof(
        &mut client,
        StateKey::AmmFeeCollected {
            contract: POOL_CONTRACT.into(),
            asset: ATOM.into(),
        },
    );
    assert_eq!(sell_fee_proof.value, StateValue::UInt(1));
    assert_storage_proof_matches_root(&sell_fee_proof, &sell_block.header.storage_root);

    submit_ok(
        &mut client,
        tx_to(
            POOL_CONTRACT,
            "e2e-amm-slippage-1",
            "Bob",
            4,
            Method::Swap,
            vec![asset(USDC), amount(100), amount(1_000)],
        ),
    );
    let slippage_block = produce_block(&mut client, 7, 7_000);
    let slippage_receipt = receipt(&mut client, "e2e-amm-slippage-1");
    assert_eq!(slippage_receipt.status, TxStatus::Reverted);
    assert_eq!(
        slippage_receipt.error,
        Some(ExecutionError::SlippageExceeded)
    );
    assert_eq!(
        slippage_block.header.storage_root,
        sell_block.header.storage_root
    );
    assert_eq!(
        slippage_block.header.event_root,
        sell_block.header.event_root
    );
    assert_eq!(reserve(&mut client, USDC), 52);
    assert_eq!(reserve(&mut client, ATOM), 100);

    let event_page = match client
        .ok(RpcRequest::GetEventsPage {
            offset: 0,
            limit: 100,
        })
        .unwrap()
    {
        RpcResult::EventsPage(page) => page,
        result => panic!("expected event page, got {result:?}"),
    };
    assert_eq!(event_page.total_events, 6);
    assert!(matches!(
        event_page.events.last().map(|event| &event.payload),
        Some(EventPayload::Swap {
            amount_in: 15,
            amount_out: 8,
            ..
        })
    ));

    let subscription_events = match client
        .ok(RpcRequest::GetSubscriptionEvents {
            subscription_id: subscription.subscription_id,
            from_sequence: subscription.next_sequence,
            limit: 100,
        })
        .unwrap()
    {
        RpcResult::SubscriptionEvents(page) => page,
        result => panic!("expected subscription event page, got {result:?}"),
    };
    assert_eq!(subscription_events.events.len(), 20);
    assert_eq!(
        subscription_events
            .events
            .iter()
            .filter(|event| event.topic == SubscriptionTopic::Blocks)
            .count(),
        7
    );
    assert_eq!(
        subscription_events
            .events
            .iter()
            .filter(|event| event.topic == SubscriptionTopic::Receipts)
            .count(),
        7
    );
    assert_eq!(
        subscription_events
            .events
            .iter()
            .filter(|event| event.topic == SubscriptionTopic::Events)
            .count(),
        6
    );

    client.close();
    tcp_server.join().unwrap();
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
        result => panic!("expected produced block, got {result:?}"),
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

fn receipt_proof(client: &mut TcpRpcClient, height: u64, index: usize) -> detta_core::ReceiptProof {
    match client
        .ok(RpcRequest::GetReceiptProof { height, index })
        .unwrap()
    {
        RpcResult::ReceiptProof(proof) => *proof,
        result => panic!("expected receipt proof, got {result:?}"),
    }
}

fn storage_proof(client: &mut TcpRpcClient, key: StateKey) -> detta_core::StorageProof {
    match client.ok(RpcRequest::GetStorageProof { key }).unwrap() {
        RpcResult::StorageProof(proof) => *proof,
        result => panic!("expected storage proof, got {result:?}"),
    }
}

fn registry_proof(client: &mut TcpRpcClient, key: GrantKey) -> detta_core::RegistryProof {
    match client.ok(RpcRequest::GetRegistryProof { key }).unwrap() {
        RpcResult::RegistryProof(proof) => *proof,
        result => panic!("expected registry proof, got {result:?}"),
    }
}

fn event_proof(client: &mut TcpRpcClient, index: usize) -> detta_core::EventProof {
    match client.ok(RpcRequest::GetEventProof { index }).unwrap() {
        RpcResult::EventProof(proof) => *proof,
        result => panic!("expected event proof, got {result:?}"),
    }
}

fn pending_mempool_transactions(client: &mut TcpRpcClient) -> usize {
    match client.ok(RpcRequest::GetMempoolStatus).unwrap() {
        RpcResult::MempoolStatus(status) => status.pending_transactions,
        result => panic!("expected mempool status, got {result:?}"),
    }
}

fn token_balance(client: &mut TcpRpcClient, owner: &str) -> u128 {
    asset_balance(client, TOKEN_CONTRACT, owner, USDC)
}

fn asset_balance(client: &mut TcpRpcClient, contract: &str, owner: &str, asset: &str) -> u128 {
    amount_result(client.ok(RpcRequest::GetBalance {
        contract: contract.into(),
        owner: owner.into(),
        asset: asset.into(),
    }))
}

fn reserve(client: &mut TcpRpcClient, asset: &str) -> u128 {
    match storage_proof(
        client,
        StateKey::Reserve {
            contract: POOL_CONTRACT.into(),
            asset: asset.into(),
        },
    )
    .value
    {
        StateValue::UInt(value) => value,
    }
}

fn amount_result(result: Result<RpcResult, ClientError>) -> u128 {
    match result.unwrap() {
        RpcResult::Amount(amount) => amount,
        result => panic!("expected amount, got {result:?}"),
    }
}

fn contract_result(result: Result<RpcResult, ClientError>) -> detta_core::ContractRecord {
    match result.unwrap() {
        RpcResult::Contract(contract) => *contract,
        result => panic!("expected contract, got {result:?}"),
    }
}

fn assert_rpc_error(error: &RpcErrorBody, expected_code: &str) {
    assert_eq!(error.code, expected_code);
    assert!(!error.message.is_empty());
}
