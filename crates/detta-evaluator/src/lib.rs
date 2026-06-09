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
pub const RESTRICTED_EVALUATOR_RESOURCE_EXHAUSTION_SCHEMA: &str =
    "detta.restricted-evaluator-resource-exhaustion.v1";
pub const RESTRICTED_EVALUATOR_RESOURCE_EXHAUSTION_SCHEMA_VERSION: u32 = 1;
pub const RESTRICTED_EVALUATOR_ARITHMETIC_OVERFLOW_SCHEMA: &str =
    "detta.restricted-evaluator-arithmetic-overflow.v1";
pub const RESTRICTED_EVALUATOR_ARITHMETIC_OVERFLOW_SCHEMA_VERSION: u32 = 1;
pub const RESTRICTED_EVALUATOR_FIXTURE_INVENTORY_SCHEMA: &str =
    "detta.restricted-evaluator-fixture-inventory.v1";
pub const RESTRICTED_EVALUATOR_FIXTURE_INVENTORY_SCHEMA_VERSION: u32 = 1;

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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResourceExhaustionCase {
    pub name: String,
    pub max_steps: u64,
    pub script: Vec<Instruction>,
    pub expected_error: EvaluatorError,
    pub committed_report: Option<ExecutionReport>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RestrictedEvaluatorResourceExhaustionFixture {
    pub schema: String,
    pub schema_version: u32,
    pub evaluator: String,
    pub cases: Vec<ResourceExhaustionCase>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArithmeticOverflowCase {
    pub name: String,
    pub max_steps: u64,
    pub left: String,
    pub right: String,
    pub expected_error: EvaluatorError,
    pub committed_report: Option<ExecutionReport>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RestrictedEvaluatorArithmeticOverflowFixture {
    pub schema: String,
    pub schema_version: u32,
    pub evaluator: String,
    pub cases: Vec<ArithmeticOverflowCase>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvaluatorFixtureInventoryEntry {
    pub name: String,
    pub fixture_schema: String,
    pub fixture_path: String,
    pub fixture_sha256: String,
    pub attestation_path: String,
    pub attestation_sha256: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_root_attestation_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_root_attestation_sha256: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EvaluatorFixtureInventory {
    pub schema: String,
    pub schema_version: u32,
    pub evaluator: String,
    pub fixtures: Vec<EvaluatorFixtureInventoryEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    UnexpectedEof,
    UnexpectedToken { expected: String, found: String },
    UnknownInstruction(String),
    UnknownMethod(String),
    UnknownPrimitive(String),
    UnknownStateKey(String),
    UnknownStateValue(String),
    InvalidNumber(String),
    TrailingTokens(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Token {
    LParen,
    RParen,
    Atom(String),
}

pub fn parse_restricted_script(source: &str) -> Result<Vec<Instruction>, ParseError> {
    let tokens = tokenize(source);
    let mut parser = Parser {
        tokens,
        position: 0,
    };
    let mut instructions = Vec::new();

    while !parser.is_eof() {
        instructions.push(parser.parse_instruction()?);
    }

    Ok(instructions)
}

pub fn canonical_script_source(instructions: &[Instruction]) -> String {
    instructions
        .iter()
        .map(canonical_instruction)
        .collect::<Vec<_>>()
        .join("\n")
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn is_eof(&self) -> bool {
        self.position >= self.tokens.len()
    }

    fn next(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.position).cloned();
        if token.is_some() {
            self.position += 1;
        }
        token
    }

    fn expect_lparen(&mut self) -> Result<(), ParseError> {
        match self.next() {
            Some(Token::LParen) => Ok(()),
            Some(token) => Err(ParseError::UnexpectedToken {
                expected: "(".into(),
                found: describe_token(&token),
            }),
            None => Err(ParseError::UnexpectedEof),
        }
    }

    fn expect_rparen(&mut self) -> Result<(), ParseError> {
        match self.next() {
            Some(Token::RParen) => Ok(()),
            Some(token) => Err(ParseError::UnexpectedToken {
                expected: ")".into(),
                found: describe_token(&token),
            }),
            None => Err(ParseError::UnexpectedEof),
        }
    }

    fn expect_atom(&mut self) -> Result<String, ParseError> {
        match self.next() {
            Some(Token::Atom(atom)) => Ok(atom),
            Some(token) => Err(ParseError::UnexpectedToken {
                expected: "atom".into(),
                found: describe_token(&token),
            }),
            None => Err(ParseError::UnexpectedEof),
        }
    }

    fn parse_instruction(&mut self) -> Result<Instruction, ParseError> {
        self.expect_lparen()?;
        let op = self.expect_atom()?;
        let instruction = match op.as_str() {
            "use-primitive" => {
                let primitive = parse_primitive(&self.expect_atom()?)?;
                Instruction::UsePrimitive(primitive)
            }
            "state-get" => Instruction::StateGet(self.parse_state_key()?),
            "state-set" => {
                let key = self.parse_state_key()?;
                let value = self.parse_state_value()?;
                Instruction::StateSet(key, value)
            }
            "call-contract" => {
                let contract = self.expect_atom()?;
                let method = parse_method(&self.expect_atom()?)?;
                Instruction::CallContract { contract, method }
            }
            "pure-add" => {
                let left = parse_amount(&self.expect_atom()?)?;
                let right = parse_amount(&self.expect_atom()?)?;
                Instruction::PureAdd { left, right }
            }
            "abort" => Instruction::Abort,
            _ => return Err(ParseError::UnknownInstruction(op)),
        };
        self.expect_rparen()?;
        Ok(instruction)
    }

    fn parse_state_key(&mut self) -> Result<StateKey, ParseError> {
        self.expect_lparen()?;
        let key_kind = self.expect_atom()?;
        let key = match key_kind.as_str() {
            "balance" => StateKey::Balance {
                contract: self.expect_atom()?,
                owner: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "total-supply" => StateKey::TotalSupply {
                contract: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "reserve" => StateKey::Reserve {
                contract: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "amm-fee-collected" => StateKey::AmmFeeCollected {
                contract: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "lp-supply" => StateKey::LpSupply {
                contract: self.expect_atom()?,
            },
            "lp-balance" => StateKey::LpBalance {
                contract: self.expect_atom()?,
                owner: self.expect_atom()?,
            },
            "oracle-price" => StateKey::OraclePrice {
                contract: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "oracle-timestamp" => StateKey::OracleTimestamp {
                contract: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "bridge-message-consumed" => StateKey::BridgeMessageConsumed {
                contract: self.expect_atom()?,
                message_id: self.expect_atom()?,
            },
            "collateral" => StateKey::Collateral {
                contract: self.expect_atom()?,
                borrower: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "debt" => StateKey::Debt {
                contract: self.expect_atom()?,
                borrower: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "debt-last-accrual-height" => StateKey::DebtLastAccrualHeight {
                contract: self.expect_atom()?,
                borrower: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "bad-debt" => StateKey::BadDebt {
                contract: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "stake-balance" => StateKey::StakeBalance {
                contract: self.expect_atom()?,
                staker: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "total-staked" => StateKey::TotalStaked {
                contract: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "pending-unbond" => StateKey::PendingUnbond {
                contract: self.expect_atom()?,
                staker: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "total-pending-unbond" => StateKey::TotalPendingUnbond {
                contract: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "staking-penalty-collected" => StateKey::StakingPenaltyCollected {
                contract: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "unbond-ready-height" => StateKey::UnbondReadyHeight {
                contract: self.expect_atom()?,
                staker: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "staking-reward-balance" => StateKey::StakingRewardBalance {
                contract: self.expect_atom()?,
                staker: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            "staking-last-reward-height" => StateKey::StakingLastRewardHeight {
                contract: self.expect_atom()?,
                staker: self.expect_atom()?,
                asset: self.expect_atom()?,
            },
            _ => return Err(ParseError::UnknownStateKey(key_kind)),
        };
        self.expect_rparen()?;
        Ok(key)
    }

    fn parse_state_value(&mut self) -> Result<StateValue, ParseError> {
        self.expect_lparen()?;
        let value_kind = self.expect_atom()?;
        let value = match value_kind.as_str() {
            "uint" => StateValue::UInt(parse_amount(&self.expect_atom()?)?),
            _ => return Err(ParseError::UnknownStateValue(value_kind)),
        };
        self.expect_rparen()?;
        Ok(value)
    }
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
    bytes_root(&bytes)
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

pub fn restricted_evaluator_resource_exhaustion_fixture(
) -> RestrictedEvaluatorResourceExhaustionFixture {
    let alice_balance = StateKey::Balance {
        contract: "TokenA".into(),
        owner: "Alice".into(),
        asset: "USDC".into(),
    };
    let max_steps = 1;
    let script = vec![
        Instruction::StateSet(alice_balance, StateValue::UInt(90)),
        Instruction::UsePrimitive(EvaluatorPrimitive::PureData),
    ];
    let expected_error = RestrictedScriptEvaluator::new(max_steps)
        .execute(&script)
        .expect_err("golden resource exhaustion script should fail");

    RestrictedEvaluatorResourceExhaustionFixture {
        schema: RESTRICTED_EVALUATOR_RESOURCE_EXHAUSTION_SCHEMA.to_string(),
        schema_version: RESTRICTED_EVALUATOR_RESOURCE_EXHAUSTION_SCHEMA_VERSION,
        evaluator: "detta.restricted-script-evaluator".into(),
        cases: vec![ResourceExhaustionCase {
            name: "step-budget-exhaustion-discards-emitted-trace".into(),
            max_steps,
            script,
            expected_error,
            committed_report: None,
        }],
    }
}

pub fn restricted_evaluator_arithmetic_overflow_fixture(
) -> RestrictedEvaluatorArithmeticOverflowFixture {
    let max_steps = 1;
    let left = u128::MAX;
    let right = 1u128;
    let script = vec![Instruction::PureAdd { left, right }];
    let expected_error = RestrictedScriptEvaluator::new(max_steps)
        .execute(&script)
        .expect_err("golden arithmetic overflow script should fail");

    RestrictedEvaluatorArithmeticOverflowFixture {
        schema: RESTRICTED_EVALUATOR_ARITHMETIC_OVERFLOW_SCHEMA.to_string(),
        schema_version: RESTRICTED_EVALUATOR_ARITHMETIC_OVERFLOW_SCHEMA_VERSION,
        evaluator: "detta.restricted-script-evaluator".into(),
        cases: vec![ArithmeticOverflowCase {
            name: "u128-addition-overflow-discards-report".into(),
            max_steps,
            left: left.to_string(),
            right: right.to_string(),
            expected_error,
            committed_report: None,
        }],
    }
}

pub fn restricted_evaluator_fixture_inventory() -> EvaluatorFixtureInventory {
    EvaluatorFixtureInventory {
        schema: RESTRICTED_EVALUATOR_FIXTURE_INVENTORY_SCHEMA.to_string(),
        schema_version: RESTRICTED_EVALUATOR_FIXTURE_INVENTORY_SCHEMA_VERSION,
        evaluator: "detta.restricted-script-evaluator".into(),
        fixtures: vec![
            EvaluatorFixtureInventoryEntry {
                name: "proof-trace".into(),
                fixture_schema: RESTRICTED_EVALUATOR_PROOF_TRACE_SCHEMA.into(),
                fixture_path: "models/detta-restricted-evaluator-proof-trace.json".into(),
                fixture_sha256: bytes_root(include_bytes!(
                    "../../../models/detta-restricted-evaluator-proof-trace.json"
                )),
                attestation_path: "models/detta-restricted-evaluator-proof-trace.sha256".into(),
                attestation_sha256: bytes_root(include_bytes!(
                    "../../../models/detta-restricted-evaluator-proof-trace.sha256"
                )),
                trace_root: Some(restricted_evaluator_proof_trace_fixture().trace_root),
                trace_root_attestation_path: Some(
                    "models/detta-restricted-evaluator-proof-trace-root.sha256".into(),
                ),
                trace_root_attestation_sha256: Some(bytes_root(include_bytes!(
                    "../../../models/detta-restricted-evaluator-proof-trace-root.sha256"
                ))),
            },
            EvaluatorFixtureInventoryEntry {
                name: "forbidden-primitives".into(),
                fixture_schema: RESTRICTED_EVALUATOR_FORBIDDEN_PRIMITIVE_SCHEMA.into(),
                fixture_path: "models/detta-restricted-evaluator-forbidden-primitives.json".into(),
                fixture_sha256: bytes_root(include_bytes!(
                    "../../../models/detta-restricted-evaluator-forbidden-primitives.json"
                )),
                attestation_path: "models/detta-restricted-evaluator-forbidden-primitives.sha256"
                    .into(),
                attestation_sha256: bytes_root(include_bytes!(
                    "../../../models/detta-restricted-evaluator-forbidden-primitives.sha256"
                )),
                trace_root: None,
                trace_root_attestation_path: None,
                trace_root_attestation_sha256: None,
            },
            EvaluatorFixtureInventoryEntry {
                name: "resource-exhaustion".into(),
                fixture_schema: RESTRICTED_EVALUATOR_RESOURCE_EXHAUSTION_SCHEMA.into(),
                fixture_path: "models/detta-restricted-evaluator-resource-exhaustion.json".into(),
                fixture_sha256: bytes_root(include_bytes!(
                    "../../../models/detta-restricted-evaluator-resource-exhaustion.json"
                )),
                attestation_path: "models/detta-restricted-evaluator-resource-exhaustion.sha256"
                    .into(),
                attestation_sha256: bytes_root(include_bytes!(
                    "../../../models/detta-restricted-evaluator-resource-exhaustion.sha256"
                )),
                trace_root: None,
                trace_root_attestation_path: None,
                trace_root_attestation_sha256: None,
            },
            EvaluatorFixtureInventoryEntry {
                name: "arithmetic-overflow".into(),
                fixture_schema: RESTRICTED_EVALUATOR_ARITHMETIC_OVERFLOW_SCHEMA.into(),
                fixture_path: "models/detta-restricted-evaluator-arithmetic-overflow.json".into(),
                fixture_sha256: bytes_root(include_bytes!(
                    "../../../models/detta-restricted-evaluator-arithmetic-overflow.json"
                )),
                attestation_path: "models/detta-restricted-evaluator-arithmetic-overflow.sha256"
                    .into(),
                attestation_sha256: bytes_root(include_bytes!(
                    "../../../models/detta-restricted-evaluator-arithmetic-overflow.sha256"
                )),
                trace_root: None,
                trace_root_attestation_path: None,
                trace_root_attestation_sha256: None,
            },
        ],
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

fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut atom = String::new();

    for ch in source.chars() {
        match ch {
            '(' => {
                push_atom(&mut tokens, &mut atom);
                tokens.push(Token::LParen);
            }
            ')' => {
                push_atom(&mut tokens, &mut atom);
                tokens.push(Token::RParen);
            }
            ch if ch.is_whitespace() => push_atom(&mut tokens, &mut atom),
            _ => atom.push(ch),
        }
    }
    push_atom(&mut tokens, &mut atom);

    tokens
}

fn push_atom(tokens: &mut Vec<Token>, atom: &mut String) {
    if !atom.is_empty() {
        tokens.push(Token::Atom(std::mem::take(atom)));
    }
}

fn describe_token(token: &Token) -> String {
    match token {
        Token::LParen => "(".into(),
        Token::RParen => ")".into(),
        Token::Atom(atom) => atom.clone(),
    }
}

fn parse_primitive(value: &str) -> Result<EvaluatorPrimitive, ParseError> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|_| ParseError::UnknownPrimitive(value.to_string()))
}

fn parse_method(value: &str) -> Result<detta_core::Method, ParseError> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|_| ParseError::UnknownMethod(value.to_string()))
}

fn parse_amount(value: &str) -> Result<u128, ParseError> {
    value
        .parse::<u128>()
        .map_err(|_| ParseError::InvalidNumber(value.to_string()))
}

fn canonical_instruction(instruction: &Instruction) -> String {
    match instruction {
        Instruction::UsePrimitive(primitive) => {
            format!("(use-primitive {})", canonical_primitive(primitive))
        }
        Instruction::StateGet(key) => format!("(state-get {})", canonical_state_key(key)),
        Instruction::StateSet(key, value) => {
            format!(
                "(state-set {} {})",
                canonical_state_key(key),
                canonical_state_value(value)
            )
        }
        Instruction::CallContract { contract, method } => {
            format!("(call-contract {} {})", contract, canonical_method(method))
        }
        Instruction::PureAdd { left, right } => format!("(pure-add {left} {right})"),
        Instruction::Abort => "(abort)".into(),
    }
}

fn canonical_primitive(primitive: &EvaluatorPrimitive) -> String {
    format!("{primitive:?}")
}

fn canonical_method(method: &detta_core::Method) -> String {
    format!("{method:?}")
}

fn canonical_state_value(value: &StateValue) -> String {
    match value {
        StateValue::UInt(value) => format!("(uint {value})"),
    }
}

fn canonical_state_key(key: &StateKey) -> String {
    match key {
        StateKey::Balance {
            contract,
            owner,
            asset,
        } => format!("(balance {contract} {owner} {asset})"),
        StateKey::TotalSupply { contract, asset } => {
            format!("(total-supply {contract} {asset})")
        }
        StateKey::Reserve { contract, asset } => format!("(reserve {contract} {asset})"),
        StateKey::AmmFeeCollected { contract, asset } => {
            format!("(amm-fee-collected {contract} {asset})")
        }
        StateKey::LpSupply { contract } => format!("(lp-supply {contract})"),
        StateKey::LpBalance { contract, owner } => format!("(lp-balance {contract} {owner})"),
        StateKey::OraclePrice { contract, asset } => {
            format!("(oracle-price {contract} {asset})")
        }
        StateKey::OracleTimestamp { contract, asset } => {
            format!("(oracle-timestamp {contract} {asset})")
        }
        StateKey::BridgeMessageConsumed {
            contract,
            message_id,
        } => format!("(bridge-message-consumed {contract} {message_id})"),
        StateKey::Collateral {
            contract,
            borrower,
            asset,
        } => format!("(collateral {contract} {borrower} {asset})"),
        StateKey::Debt {
            contract,
            borrower,
            asset,
        } => format!("(debt {contract} {borrower} {asset})"),
        StateKey::DebtLastAccrualHeight {
            contract,
            borrower,
            asset,
        } => format!("(debt-last-accrual-height {contract} {borrower} {asset})"),
        StateKey::BadDebt { contract, asset } => format!("(bad-debt {contract} {asset})"),
        StateKey::StakeBalance {
            contract,
            staker,
            asset,
        } => format!("(stake-balance {contract} {staker} {asset})"),
        StateKey::TotalStaked { contract, asset } => {
            format!("(total-staked {contract} {asset})")
        }
        StateKey::PendingUnbond {
            contract,
            staker,
            asset,
        } => format!("(pending-unbond {contract} {staker} {asset})"),
        StateKey::TotalPendingUnbond { contract, asset } => {
            format!("(total-pending-unbond {contract} {asset})")
        }
        StateKey::StakingPenaltyCollected { contract, asset } => {
            format!("(staking-penalty-collected {contract} {asset})")
        }
        StateKey::UnbondReadyHeight {
            contract,
            staker,
            asset,
        } => format!("(unbond-ready-height {contract} {staker} {asset})"),
        StateKey::StakingRewardBalance {
            contract,
            staker,
            asset,
        } => format!("(staking-reward-balance {contract} {staker} {asset})"),
        StateKey::StakingLastRewardHeight {
            contract,
            staker,
            asset,
        } => format!("(staking-last-reward-height {contract} {staker} {asset})"),
    }
}

fn bytes_root(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    hex_lower(&digest)
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
    fn restricted_parser_round_trips_canonical_script_source() {
        let source = "
            (state-get (balance TokenA Alice USDC))
            (state-set (balance TokenA Alice USDC) (uint 90))
            (call-contract AMM Swap)
            (pure-add 40 2)
        ";
        let alice = balance_key("Alice");
        let expected = vec![
            Instruction::StateGet(alice.clone()),
            Instruction::StateSet(alice.clone(), StateValue::UInt(90)),
            Instruction::CallContract {
                contract: "AMM".into(),
                method: Method::Swap,
            },
            Instruction::PureAdd { left: 40, right: 2 },
        ];

        let parsed = parse_restricted_script(source).unwrap();
        let canonical = canonical_script_source(&parsed);

        assert_eq!(parsed, expected);
        assert_eq!(
            canonical,
            "(state-get (balance TokenA Alice USDC))\n\
             (state-set (balance TokenA Alice USDC) (uint 90))\n\
             (call-contract AMM Swap)\n\
             (pure-add 40 2)"
        );
        assert_eq!(parse_restricted_script(&canonical).unwrap(), expected);

        let report = RestrictedScriptEvaluator::new(10).execute(&parsed).unwrap();
        assert_eq!(report.steps_used, 4);
        assert_eq!(report.values, vec![42]);
        assert_eq!(
            report.trace,
            vec![
                TraceOp::StateGet(alice.clone()),
                TraceOp::StateSet(alice),
                TraceOp::CallContract {
                    contract: "AMM".into(),
                    method: Method::Swap,
                }
            ]
        );
    }

    #[test]
    fn restricted_parser_rejects_unknown_terms() {
        let unknown_key = parse_restricted_script("(state-get (raw-space Public))").unwrap_err();
        assert_eq!(unknown_key, ParseError::UnknownStateKey("raw-space".into()));

        let unknown_method =
            parse_restricted_script("(call-contract AMM DefinitelyNotAMethod)").unwrap_err();
        assert_eq!(
            unknown_method,
            ParseError::UnknownMethod("DefinitelyNotAMethod".into())
        );
    }

    #[test]
    fn parsed_forbidden_primitive_still_traps_at_execution() {
        let script = parse_restricted_script("(use-primitive RawAddAtom)").unwrap();
        let error = RestrictedScriptEvaluator::new(1)
            .execute(&script)
            .unwrap_err();

        assert_eq!(error, EvaluatorError::ForbiddenPrimitive);
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
    fn restricted_evaluator_proof_trace_root_matches_checked_in_attestation() {
        let fixture = restricted_evaluator_proof_trace_fixture();
        let attestation =
            include_str!("../../../models/detta-restricted-evaluator-proof-trace-root.sha256");
        let (root, file_name) = attestation.trim().split_once("  ").unwrap();

        assert_eq!(file_name, "detta-restricted-evaluator-proof-trace.trace");
        assert_eq!(root, fixture.trace_root);
        assert_eq!(root, trace_root(&fixture.report.trace));
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
        assert_attested_fixture_root(
            include_bytes!("../../../models/detta-restricted-evaluator-resource-exhaustion.json"),
            include_str!("../../../models/detta-restricted-evaluator-resource-exhaustion.sha256"),
            "detta-restricted-evaluator-resource-exhaustion.json",
        );
        assert_attested_fixture_root(
            include_bytes!("../../../models/detta-restricted-evaluator-arithmetic-overflow.json"),
            include_str!("../../../models/detta-restricted-evaluator-arithmetic-overflow.sha256"),
            "detta-restricted-evaluator-arithmetic-overflow.json",
        );
        assert_attested_fixture_root(
            include_bytes!("../../../models/detta-restricted-evaluator-fixture-inventory.json"),
            include_str!("../../../models/detta-restricted-evaluator-fixture-inventory.sha256"),
            "detta-restricted-evaluator-fixture-inventory.json",
        );
    }

    fn assert_attested_fixture_root(bytes: &[u8], attestation: &str, expected_filename: &str) {
        let digest = Sha256::digest(bytes);
        let expected = format!("{}  {}\n", hex_lower(&digest), expected_filename);

        assert_eq!(attestation, expected);
    }

    #[test]
    fn restricted_evaluator_resource_exhaustion_fixture_matches_checked_in_json() {
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-resource-exhaustion.json"
        ))
        .unwrap();
        let actual =
            serde_json::to_value(restricted_evaluator_resource_exhaustion_fixture()).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn restricted_evaluator_resource_exhaustion_fixture_json_is_stable() {
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&restricted_evaluator_resource_exhaustion_fixture())
                .unwrap()
        );

        assert_eq!(
            actual,
            include_str!("../../../models/detta-restricted-evaluator-resource-exhaustion.json")
        );
    }

    #[test]
    fn restricted_evaluator_resource_exhaustion_fixture_reverts_report() {
        let fixture = restricted_evaluator_resource_exhaustion_fixture();
        let case = fixture.cases.first().unwrap();
        let error = RestrictedScriptEvaluator::new(case.max_steps)
            .execute(&case.script)
            .unwrap_err();

        assert_eq!(
            fixture.schema,
            RESTRICTED_EVALUATOR_RESOURCE_EXHAUSTION_SCHEMA
        );
        assert_eq!(
            fixture.schema_version,
            RESTRICTED_EVALUATOR_RESOURCE_EXHAUSTION_SCHEMA_VERSION
        );
        assert_eq!(error, EvaluatorError::StepBudgetExceeded);
        assert_eq!(case.expected_error, EvaluatorError::StepBudgetExceeded);
        assert_eq!(case.committed_report, None);
    }

    #[test]
    fn restricted_evaluator_arithmetic_overflow_fixture_matches_checked_in_json() {
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-arithmetic-overflow.json"
        ))
        .unwrap();
        let actual =
            serde_json::to_value(restricted_evaluator_arithmetic_overflow_fixture()).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn restricted_evaluator_arithmetic_overflow_fixture_json_is_stable() {
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&restricted_evaluator_arithmetic_overflow_fixture())
                .unwrap()
        );

        assert_eq!(
            actual,
            include_str!("../../../models/detta-restricted-evaluator-arithmetic-overflow.json")
        );
    }

    #[test]
    fn restricted_evaluator_arithmetic_overflow_fixture_reverts_report() {
        let fixture = restricted_evaluator_arithmetic_overflow_fixture();
        let case = fixture.cases.first().unwrap();
        let script = vec![Instruction::PureAdd {
            left: case.left.parse::<u128>().unwrap(),
            right: case.right.parse::<u128>().unwrap(),
        }];
        let error = RestrictedScriptEvaluator::new(case.max_steps)
            .execute(&script)
            .unwrap_err();

        assert_eq!(
            fixture.schema,
            RESTRICTED_EVALUATOR_ARITHMETIC_OVERFLOW_SCHEMA
        );
        assert_eq!(
            fixture.schema_version,
            RESTRICTED_EVALUATOR_ARITHMETIC_OVERFLOW_SCHEMA_VERSION
        );
        assert_eq!(error, EvaluatorError::ArithmeticOverflow);
        assert_eq!(case.expected_error, EvaluatorError::ArithmeticOverflow);
        assert_eq!(case.committed_report, None);
    }

    #[test]
    fn restricted_evaluator_fixture_inventory_matches_checked_in_json() {
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let actual = serde_json::to_value(restricted_evaluator_fixture_inventory()).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn restricted_evaluator_fixture_inventory_json_is_stable() {
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&restricted_evaluator_fixture_inventory()).unwrap()
        );

        assert_eq!(
            actual,
            include_str!("../../../models/detta-restricted-evaluator-fixture-inventory.json")
        );
    }

    #[test]
    fn restricted_evaluator_fixture_inventory_covers_all_fixture_schemas() {
        let inventory = restricted_evaluator_fixture_inventory();
        let fixture_names = inventory
            .fixtures
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        let fixture_schemas = inventory
            .fixtures
            .iter()
            .map(|entry| entry.fixture_schema.as_str())
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(
            inventory.schema,
            RESTRICTED_EVALUATOR_FIXTURE_INVENTORY_SCHEMA
        );
        assert_eq!(
            inventory.schema_version,
            RESTRICTED_EVALUATOR_FIXTURE_INVENTORY_SCHEMA_VERSION
        );
        assert_eq!(
            fixture_names,
            std::collections::BTreeSet::from([
                "proof-trace",
                "forbidden-primitives",
                "resource-exhaustion",
                "arithmetic-overflow",
            ])
        );
        assert_eq!(
            fixture_schemas,
            std::collections::BTreeSet::from([
                RESTRICTED_EVALUATOR_PROOF_TRACE_SCHEMA,
                RESTRICTED_EVALUATOR_FORBIDDEN_PRIMITIVE_SCHEMA,
                RESTRICTED_EVALUATOR_RESOURCE_EXHAUSTION_SCHEMA,
                RESTRICTED_EVALUATOR_ARITHMETIC_OVERFLOW_SCHEMA,
            ])
        );
    }
}
