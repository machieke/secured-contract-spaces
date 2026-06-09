use detta_e2e::client::{HttpRpcClient, TcpRpcClient};
use detta_e2e::fixtures::{defi_genesis_state, TOKEN_CONTRACT, USDC};
use detta_e2e::network::{spawn_http_rpc_server, spawn_tcp_rpc_server};
use detta_rpc::{RpcRequest, RpcResponse, RpcResult, DEFAULT_MAX_RPC_REQUEST_BYTES};

#[test]
fn adversarial_rpc_inputs_fail_without_corrupting_state() {
    let (tcp_addr, tcp_server) = spawn_tcp_rpc_server(defi_genesis_state()).unwrap();
    let mut tcp_client = TcpRpcClient::connect(tcp_addr).unwrap();

    assert_rpc_error(
        tcp_client.raw_line(b"{not json").unwrap(),
        "rpc.decode_error",
    );
    assert_rpc_error(
        tcp_client.raw_line(br#""NoSuchMethod""#).unwrap(),
        "rpc.decode_error",
    );
    assert_rpc_error(
        tcp_client
            .raw_line(br#"{"GetBalance":{"contract":"TokenA"}}"#)
            .unwrap(),
        "rpc.decode_error",
    );
    assert_rpc_error(
        tcp_client
            .raw_line(br#"{"GetEventsPage":{"offset":"zero","limit":1}}"#)
            .unwrap(),
        "rpc.decode_error",
    );
    assert_rpc_error(
        tcp_client.raw_line(br#"["GetStateRoot"]"#).unwrap(),
        "rpc.decode_error",
    );
    assert_eq!(token_balance(&mut tcp_client, "Alice"), 200);
    tcp_client.close();
    tcp_server.join().unwrap();

    let (http_addr, http_server) = spawn_http_rpc_server(defi_genesis_state(), 6).unwrap();
    let http_client = HttpRpcClient::new(http_addr);

    let (status, response) = http_client.post_bytes(b"{not json").unwrap();
    assert_eq!(status, 200);
    assert_rpc_error(response, "rpc.decode_error");

    let (status, response) = http_client
        .raw_request(b"GET /rpc HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n")
        .unwrap();
    assert_eq!(status, 405);
    assert_rpc_error(response, "rpc.http_method_not_allowed");

    let (status, response) = http_client
        .raw_request(b"POST /rpc HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    assert_eq!(status, 411);
    assert_rpc_error(response, "rpc.http_length_required");

    let (status, response) = http_client
        .raw_request(b"POST /missing HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\n\r\n")
        .unwrap();
    assert_eq!(status, 404);
    assert_rpc_error(response, "rpc.http_not_found");

    let oversized_request = format!(
        "POST /rpc HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
        DEFAULT_MAX_RPC_REQUEST_BYTES + 1
    );
    let (status, response) = http_client
        .raw_request(oversized_request.as_bytes())
        .unwrap();
    assert_eq!(status, 413);
    assert_rpc_error(response, "rpc.request_too_large");

    let (status, response) = http_client
        .post(RpcRequest::GetBalance {
            contract: TOKEN_CONTRACT.into(),
            owner: "Alice".into(),
            asset: USDC.into(),
        })
        .unwrap();
    assert_eq!(status, 200);
    assert_eq!(amount_result(response), 200);
    http_server.join().unwrap();
}

fn assert_rpc_error(response: RpcResponse, expected_code: &str) {
    match response {
        RpcResponse::Error(error) => assert_eq!(error.code, expected_code),
        response => panic!("expected RPC error {expected_code}, got {response:?}"),
    }
}

fn token_balance(client: &mut TcpRpcClient, owner: &str) -> u128 {
    match client
        .ok(RpcRequest::GetBalance {
            contract: TOKEN_CONTRACT.into(),
            owner: owner.into(),
            asset: USDC.into(),
        })
        .unwrap()
    {
        RpcResult::Amount(amount) => amount,
        result => panic!("expected amount result, got {result:?}"),
    }
}

fn amount_result(response: RpcResponse) -> u128 {
    match response {
        RpcResponse::Ok(RpcResult::Amount(amount)) => amount,
        response => panic!("expected amount result, got {response:?}"),
    }
}
