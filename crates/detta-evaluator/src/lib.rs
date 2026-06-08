use detta_core::{EvaluatorPrimitive, ExecutionError, RestrictedEvaluator, StateKey, StateValue};
use detta_verify::{verify_kernel_trace, TraceError, TraceOp};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

pub const RESTRICTED_EVALUATOR_PROOF_TRACE_SCHEMA: &str =
    "detta.restricted-evaluator-proof-trace.v1";
pub const RESTRICTED_EVALUATOR_PROOF_TRACE_SCHEMA_VERSION: u32 = 1;
pub const RESTRICTED_EVALUATOR_FORBIDDEN_PRIMITIVE_SCHEMA: &str =
    "detta.restricted-evaluator-forbidden-primitive.v1";
pub const RESTRICTED_EVALUATOR_FORBIDDEN_PRIMITIVE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Instruction {
    UsePrimitive(EvaluatorPrimitive),
    StateGet(StateKey),
    StateSet(StateKey, StateValue),
    CallContract {
        contract: String,
        method: detta_core::Method,
    },
    PureAdd {
        left: u128,
        right: u128,
    },
    Abort,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ExecutionReport {
    pub trace: Vec<TraceOp>,
    pub values: Vec<u128>,
    pub steps_used: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SymbolicVerifierReport {
    pub contract: String,
    pub allowed_write_scope: Vec<StateKey>,
    pub errors: Vec<TraceError>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RestrictedEvaluatorProofTraceFixture {
    pub schema: String,
    pub schema_version: u32,
    pub evaluator: String,
    pub max_steps: u64,
    pub script: Vec<Instruction>,
    pub report: ExecutionReport,
    pub trace_root: String,
    pub symbolic_verifier: SymbolicVerifierReport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum EvaluatorError {
    ForbiddenPrimitive,
    StepBudgetExceeded,
    ArithmeticOverflow,
    Aborted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ForbiddenPrimitiveCase {
    pub primitive: EvaluatorPrimitive,
    pub script: Vec<Instruction>,
    pub expected_error: EvaluatorError,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RestrictedEvaluatorForbiddenPrimitiveFixture {
    pub schema: String,
    pub schema_version: u32,
    pub evaluator: String,
    pub max_steps: u64,
    pub allowed_primitives: Vec<EvaluatorPrimitive>,
    pub forbidden_cases: Vec<ForbiddenPrimitiveCase>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestrictedScriptEvaluator {
    primitive_gate: RestrictedEvaluator,
    max_steps: u64,
}

impl RestrictedScriptEvaluator {
    pub fn new(max_steps: u64) -> Self {
        Self {
            primitive_gate: RestrictedEvaluator,
            max_steps,
        }
    }

    pub fn execute(&self, instructions: &[Instruction]) -> Result<ExecutionReport, EvaluatorError> {
        let mut trace = Vec::new();
        let mut values = Vec::new();
        let mut steps_used = 0u64;

        for instruction in instructions {
            steps_used = steps_used
                .checked_add(1)
                .ok_or(EvaluatorError::StepBudgetExceeded)?;
            if steps_used > self.max_steps {
                return Err(EvaluatorError::StepBudgetExceeded);
            }

            match instruction {
                Instruction::UsePrimitive(primitive) => self
                    .primitive_gate
                    .validate_primitive(primitive)
                    .map_err(map_execution_error)?,
                Instruction::StateGet(key) => {
                    self.primitive_gate
                        .validate_primitive(&EvaluatorPrimitive::StateGet)
                        .map_err(map_execution_error)?;
                    trace.push(TraceOp::StateGet(key.clone()));
                }
                Instruction::StateSet(key, _) => {
                    self.primitive_gate
                        .validate_primitive(&EvaluatorPrimitive::StateSet)
                        .map_err(map_execution_error)?;
                    trace.push(TraceOp::StateSet(key.clone()));
                }
                Instruction::CallContract { contract, method } => {
                    self.primitive_gate
                        .validate_primitive(&EvaluatorPrimitive::CallContract)
                        .map_err(map_execution_error)?;
                    trace.push(TraceOp::CallContract {
                        contract: contract.clone(),
                        method: method.clone(),
                    });
                }
                Instruction::PureAdd { left, right } => {
                    self.primitive_gate
                        .validate_primitive(&EvaluatorPrimitive::PureArithmetic)
                        .map_err(map_execution_error)?;
                    values.push(
                        left.checked_add(*right)
                            .ok_or(EvaluatorError::ArithmeticOverflow)?,
                    );
                }
                Instruction::Abort => {
                    self.primitive_gate
                        .validate_primitive(&EvaluatorPrimitive::Abort)
                        .map_err(map_execution_error)?;
                    trace.push(TraceOp::Abort);
                    return Err(EvaluatorError::Aborted);
                }
            }
        }

        Ok(ExecutionReport {
            trace,
            values,
            steps_used,
        })
    }
}

pub fn trace_root(trace: &[TraceOp]) -> String {
    let bytes = serde_json::to_vec(trace).expect("trace serialization should not fail");
    let digest = Sha256::digest(bytes);
    hex_lower(&digest)
}

pub fn restricted_evaluator_proof_trace_fixture() -> RestrictedEvaluatorProofTraceFixture {
    let max_steps = 10;
    let evaluator = RestrictedScriptEvaluator::new(max_steps);
    let alice_balance = StateKey::Balance {
        contract: "TokenA".into(),
        owner: "Alice".into(),
        asset: "USDC".into(),
    };
    let script = vec![
        Instruction::StateGet(alice_balance.clone()),
        Instruction::StateSet(alice_balance.clone(), StateValue::UInt(90)),
        Instruction::CallContract {
            contract: "AMM".into(),
            method: detta_core::Method::Swap,
        },
        Instruction::PureAdd { left: 40, right: 2 },
    ];
    let report = evaluator
        .execute(&script)
        .expect("golden restricted evaluator script should execute");
    let allowed_write_scope = vec![alice_balance];
    let write_scope = allowed_write_scope.iter().cloned().collect::<BTreeSet<_>>();
    let errors = verify_kernel_trace("TokenA", &write_scope, &report.trace);

    RestrictedEvaluatorProofTraceFixture {
        schema: RESTRICTED_EVALUATOR_PROOF_TRACE_SCHEMA.to_string(),
        schema_version: RESTRICTED_EVALUATOR_PROOF_TRACE_SCHEMA_VERSION,
        evaluator: "detta.restricted-script-evaluator".into(),
        max_steps,
        trace_root: trace_root(&report.trace),
        script,
        report,
        symbolic_verifier: SymbolicVerifierReport {
            contract: "TokenA".into(),
            allowed_write_scope,
            errors,
        },
    }
}

pub fn restricted_evaluator_forbidden_primitive_fixture(
) -> RestrictedEvaluatorForbiddenPrimitiveFixture {
    let max_steps = 1;
    let evaluator = RestrictedScriptEvaluator::new(max_steps);
    let forbidden_cases = forbidden_contract_primitives()
        .into_iter()
        .map(|primitive| {
            let script = vec![Instruction::UsePrimitive(primitive.clone())];
            let expected_error = evaluator
                .execute(&script)
                .expect_err("golden forbidden primitive script should fail");
            ForbiddenPrimitiveCase {
                primitive,
                script,
                expected_error,
            }
        })
        .collect();

    RestrictedEvaluatorForbiddenPrimitiveFixture {
        schema: RESTRICTED_EVALUATOR_FORBIDDEN_PRIMITIVE_SCHEMA.to_string(),
        schema_version: RESTRICTED_EVALUATOR_FORBIDDEN_PRIMITIVE_SCHEMA_VERSION,
        evaluator: "detta.restricted-script-evaluator".into(),
        max_steps,
        allowed_primitives: allowed_contract_primitives(),
        forbidden_cases,
    }
}

fn allowed_contract_primitives() -> Vec<EvaluatorPrimitive> {
    vec![
        EvaluatorPrimitive::StateGet,
        EvaluatorPrimitive::StateSet,
        EvaluatorPrimitive::RegistryGet,
        EvaluatorPrimitive::RegistrySetGuarded,
        EvaluatorPrimitive::EmitEvent,
        EvaluatorPrimitive::CallContract,
        EvaluatorPrimitive::Abort,
        EvaluatorPrimitive::PureArithmetic,
        EvaluatorPrimitive::PureComparison,
        EvaluatorPrimitive::PureData,
    ]
}

fn forbidden_contract_primitives() -> Vec<EvaluatorPrimitive> {
    vec![
        EvaluatorPrimitive::RawAddAtom,
        EvaluatorPrimitive::RawRemoveAtom,
        EvaluatorPrimitive::RawPrivateMatch,
        EvaluatorPrimitive::PrologAssert,
        EvaluatorPrimitive::PrologRetract,
        EvaluatorPrimitive::PythonCall,
        EvaluatorPrimitive::FileSystemAccess,
        EvaluatorPrimitive::ProcessExecution,
        EvaluatorPrimitive::NetworkAccess,
        EvaluatorPrimitive::WallClockAccess,
        EvaluatorPrimitive::Randomness,
        EvaluatorPrimitive::ArbitraryImport,
    ]
}

fn map_execution_error(error: ExecutionError) -> EvaluatorError {
    match error {
        ExecutionError::ForbiddenPrimitive => EvaluatorError::ForbiddenPrimitive,
        ExecutionError::ArithmeticOverflow => EvaluatorError::ArithmeticOverflow,
        _ => EvaluatorError::ForbiddenPrimitive,
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Amount, Argument, Method};

    fn balance_key(owner: &str) -> StateKey {
        StateKey::Balance {
            contract: "TokenA".into(),
            owner: owner.into(),
            asset: "USDC".into(),
        }
    }

    #[test]
    fn evaluator_emits_state_write_trace() {
        let evaluator = RestrictedScriptEvaluator::new(10);
        let alice = balance_key("Alice");
        let report = evaluator
            .execute(&[
                Instruction::StateGet(alice.clone()),
                Instruction::StateSet(alice.clone(), StateValue::UInt(90 as Amount)),
            ])
            .unwrap();

        assert_eq!(
            report.trace,
            vec![
                TraceOp::StateGet(alice.clone()),
                TraceOp::StateSet(alice.clone())
            ]
        );
        assert_eq!(
            verify_kernel_trace("TokenA", &BTreeSet::from([alice]), &report.trace),
            vec![]
        );
    }

    #[test]
    fn evaluator_rejects_forbidden_primitive() {
        let evaluator = RestrictedScriptEvaluator::new(10);
        let error = evaluator
            .execute(&[Instruction::UsePrimitive(EvaluatorPrimitive::RawAddAtom)])
            .unwrap_err();

        assert_eq!(error, EvaluatorError::ForbiddenPrimitive);
    }

    #[test]
    fn evaluator_enforces_step_budget() {
        let evaluator = RestrictedScriptEvaluator::new(1);
        let error = evaluator
            .execute(&[
                Instruction::UsePrimitive(EvaluatorPrimitive::PureData),
                Instruction::UsePrimitive(EvaluatorPrimitive::PureData),
            ])
            .unwrap_err();

        assert_eq!(error, EvaluatorError::StepBudgetExceeded);
    }

    #[test]
    fn identical_traces_have_identical_roots() {
        let evaluator = RestrictedScriptEvaluator::new(10);
        let script = [
            Instruction::CallContract {
                contract: "TokenA".into(),
                method: Method::TransferFrom,
            },
            Instruction::PureAdd { left: 1, right: 2 },
        ];
        let left = evaluator.execute(&script).unwrap();
        let right = evaluator.execute(&script).unwrap();

        assert_eq!(trace_root(&left.trace), trace_root(&right.trace));
        assert_eq!(left.values, vec![3]);
        assert_eq!(right.values, vec![3]);
    }

    #[test]
    fn evaluator_trace_can_be_rejected_by_symbolic_verifier() {
        let evaluator = RestrictedScriptEvaluator::new(10);
        let alice = balance_key("Alice");
        let mallory = balance_key("Mallory");
        let report = evaluator
            .execute(&[Instruction::StateSet(mallory.clone(), StateValue::UInt(1))])
            .unwrap();

        let errors = verify_kernel_trace("TokenA", &BTreeSet::from([alice]), &report.trace);

        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn transfer_like_arguments_are_plain_data_not_authority() {
        let evaluator = RestrictedScriptEvaluator::new(10);
        let report = evaluator
            .execute(&[
                Instruction::UsePrimitive(EvaluatorPrimitive::PureData),
                Instruction::CallContract {
                    contract: "TokenA".into(),
                    method: Method::TransferFrom,
                },
            ])
            .unwrap();

        assert_eq!(
            report.trace,
            vec![TraceOp::CallContract {
                contract: "TokenA".into(),
                method: Method::TransferFrom,
            }]
        );
        let _args = [
            Argument::Principal("Alice".into()),
            Argument::Principal("Bob".into()),
            Argument::Asset("USDC".into()),
            Argument::Amount(10),
        ];
    }

    #[test]
    fn restricted_evaluator_proof_trace_fixture_matches_checked_in_json() {
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-proof-trace.json"
        ))
        .unwrap();
        let actual = serde_json::to_value(restricted_evaluator_proof_trace_fixture()).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn restricted_evaluator_proof_trace_fixture_json_is_stable() {
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&restricted_evaluator_proof_trace_fixture()).unwrap()
        );

        assert_eq!(
            actual,
            include_str!("../../../models/detta-restricted-evaluator-proof-trace.json")
        );
    }

    #[test]
    fn restricted_evaluator_proof_trace_fixture_verifies_trace() {
        let fixture = restricted_evaluator_proof_trace_fixture();

        assert_eq!(fixture.schema, RESTRICTED_EVALUATOR_PROOF_TRACE_SCHEMA);
        assert_eq!(
            fixture.schema_version,
            RESTRICTED_EVALUATOR_PROOF_TRACE_SCHEMA_VERSION
        );
        assert_eq!(fixture.report.steps_used, 4);
        assert_eq!(fixture.report.values, vec![42]);
        assert_eq!(
            fixture.trace_root,
            "a90c256f6a8f5f16dcf5bc3cc063aca87f8cfaff4c17196f0d9fcebac70c4947"
        );
        assert!(fixture.symbolic_verifier.errors.is_empty());
    }

    #[test]
    fn restricted_evaluator_forbidden_primitive_fixture_matches_checked_in_json() {
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-forbidden-primitives.json"
        ))
        .unwrap();
        let actual =
            serde_json::to_value(restricted_evaluator_forbidden_primitive_fixture()).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn restricted_evaluator_forbidden_primitive_fixture_json_is_stable() {
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&restricted_evaluator_forbidden_primitive_fixture())
                .unwrap()
        );

        assert_eq!(
            actual,
            include_str!("../../../models/detta-restricted-evaluator-forbidden-primitives.json")
        );
    }

    #[test]
    fn restricted_evaluator_forbidden_primitive_fixture_covers_escape_hatches() {
        let fixture = restricted_evaluator_forbidden_primitive_fixture();

        assert_eq!(
            fixture.schema,
            RESTRICTED_EVALUATOR_FORBIDDEN_PRIMITIVE_SCHEMA
        );
        assert_eq!(
            fixture.schema_version,
            RESTRICTED_EVALUATOR_FORBIDDEN_PRIMITIVE_SCHEMA_VERSION
        );
        assert_eq!(fixture.allowed_primitives, allowed_contract_primitives());
        assert_eq!(fixture.forbidden_cases.len(), 12);
        for case in fixture.forbidden_cases {
            assert_eq!(case.expected_error, EvaluatorError::ForbiddenPrimitive);
            assert_eq!(case.script, vec![Instruction::UsePrimitive(case.primitive)]);
        }
    }

    #[test]
    fn restricted_evaluator_fixture_roots_match_checked_in_attestations() {
        assert_attested_fixture_root(
            include_bytes!("../../../models/detta-restricted-evaluator-proof-trace.json"),
            include_str!("../../../models/detta-restricted-evaluator-proof-trace.sha256"),
            "detta-restricted-evaluator-proof-trace.json",
        );
        assert_attested_fixture_root(
            include_bytes!("../../../models/detta-restricted-evaluator-forbidden-primitives.json"),
            include_str!("../../../models/detta-restricted-evaluator-forbidden-primitives.sha256"),
            "detta-restricted-evaluator-forbidden-primitives.json",
        );
    }

    fn assert_attested_fixture_root(bytes: &[u8], attestation: &str, expected_filename: &str) {
        let digest = Sha256::digest(bytes);
        let expected = format!("{}  {}\n", hex_lower(&digest), expected_filename);

        assert_eq!(attestation, expected);
    }
}
