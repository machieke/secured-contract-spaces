use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::defi_genesis_state;
use detta_e2e::network::spawn_tcp_rpc_server;
use detta_rpc::{RpcErrorBody, RpcRequest, RpcResult, DEFAULT_MAX_RESTRICTED_EVALUATOR_STEPS};
use sha2::{Digest, Sha256};

#[test]
fn client_exercises_restricted_evaluator_safety_boundaries() {
    let (tcp_addr, tcp_server) = spawn_tcp_rpc_server(defi_genesis_state()).unwrap();
    let mut client = TcpRpcClient::connect(tcp_addr).unwrap();

    let state_root_before = state_root(&mut client);
    let inventory = match client
        .ok(RpcRequest::GetRestrictedEvaluatorFixtureInventory)
        .unwrap()
    {
        RpcResult::RestrictedEvaluatorFixtureInventory(inventory) => inventory,
        result => panic!("expected evaluator fixture inventory, got {result:?}"),
    };
    assert_eq!(inventory.evaluator, "detta.restricted-script-evaluator");
    assert!(inventory
        .fixtures
        .iter()
        .any(|entry| entry.name == "proof-trace" && entry.trace_root.is_some()));

    let source = "
        (state-get (balance TokenA Alice USDC))
        (state-set (balance TokenA Alice USDC) (uint 90))
        (call-contract AMM Swap)
        (pure-add 40 2)
    ";
    let report = match client
        .ok(RpcRequest::EvaluateRestrictedScript {
            source: source.into(),
            max_steps: 10,
        })
        .unwrap()
    {
        RpcResult::RestrictedEvaluation(report) => report,
        result => panic!("expected restricted evaluation report, got {result:?}"),
    };
    assert_eq!(report.evaluator, "detta.restricted-script-evaluator");
    assert_eq!(report.max_steps, 10);
    assert_eq!(report.report.steps_used, 4);
    assert_eq!(report.report.values, vec![42]);
    assert_eq!(
        report.canonical_source,
        "(state-get (balance TokenA Alice USDC))\n\
         (state-set (balance TokenA Alice USDC) (uint 90))\n\
         (call-contract AMM Swap)\n\
         (pure-add 40 2)"
    );
    assert_eq!(
        report.trace_root,
        sha256_hex(&serde_json::to_vec(&report.report.trace).unwrap())
    );

    let repeated_report = match client
        .ok(RpcRequest::EvaluateRestrictedScript {
            source: report.canonical_source.clone(),
            max_steps: 10,
        })
        .unwrap()
    {
        RpcResult::RestrictedEvaluation(report) => report,
        result => panic!("expected restricted evaluation report, got {result:?}"),
    };
    assert_eq!(repeated_report.report.trace, report.report.trace);
    assert_eq!(repeated_report.trace_root, report.trace_root);

    assert_rpc_error(
        client
            .error(RpcRequest::EvaluateRestrictedScript {
                source: "(use-primitive RawAddAtom)".into(),
                max_steps: 1,
            })
            .unwrap(),
        "evaluator.forbidden_primitive",
    );
    assert_rpc_error(
        client
            .error(RpcRequest::EvaluateRestrictedScript {
                source: "(use-primitive PureData)\n(use-primitive PureData)".into(),
                max_steps: 1,
            })
            .unwrap(),
        "evaluator.step_budget_exceeded",
    );
    assert_rpc_error(
        client
            .error(RpcRequest::EvaluateRestrictedScript {
                source: "(pure-add 340282366920938463463374607431768211455 1)".into(),
                max_steps: 1,
            })
            .unwrap(),
        "evaluator.arithmetic_overflow",
    );
    assert_rpc_error(
        client
            .error(RpcRequest::EvaluateRestrictedScript {
                source: "(use-primitive NotAThing)".into(),
                max_steps: 1,
            })
            .unwrap(),
        "evaluator.parse_error",
    );
    assert_rpc_error(
        client
            .error(RpcRequest::EvaluateRestrictedScript {
                source: "(pure-add 1 1)".into(),
                max_steps: DEFAULT_MAX_RESTRICTED_EVALUATOR_STEPS + 1,
            })
            .unwrap(),
        "evaluator.step_budget_too_large",
    );
    assert_eq!(state_root(&mut client), state_root_before);

    client.close();
    tcp_server.join().unwrap();
}

fn state_root(client: &mut TcpRpcClient) -> String {
    match client.ok(RpcRequest::GetStateRoot).unwrap() {
        RpcResult::StateRoot(root) => root,
        result => panic!("expected state root, got {result:?}"),
    }
}

fn assert_rpc_error(error: RpcErrorBody, expected_code: &str) {
    assert_eq!(error.code, expected_code);
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
