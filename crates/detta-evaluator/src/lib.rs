use detta_core::{EvaluatorPrimitive, ExecutionError, RestrictedEvaluator, StateKey, StateValue};
use detta_verify::TraceOp;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EvaluatorError {
    ForbiddenPrimitive,
    StepBudgetExceeded,
    ArithmeticOverflow,
    Aborted,
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
    use detta_verify::verify_kernel_trace;
    use std::collections::BTreeSet;

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
}
