use detta_aspects::{AspectModuleIr, Expr};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AspectValue {
    Unit,
    Bool(bool),
    Amount(u128),
    Atom(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AspectHostOp {
    StateGet {
        state: String,
    },
    StateSet {
        state: String,
        value: AspectValue,
    },
    RegistryGet {
        registry: String,
    },
    RegistrySet {
        registry: String,
        value: AspectValue,
    },
    RegistryConsume {
        registry: String,
        value: AspectValue,
    },
    Emit {
        event: String,
    },
    CallContract {
        contract: String,
        method: String,
        args: Vec<AspectValue>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AspectExecutionReport {
    pub trace: Vec<AspectHostOp>,
    pub return_value: AspectValue,
    pub steps_used: u64,
    pub trace_root: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AspectEvalError {
    UnknownAction { aspect: String, action: String },
    MissingActionBody { aspect: String, action: String },
    InvalidActionBody(String),
    InvalidExpression(String),
    UnknownVariable(String),
    StepBudgetExceeded,
    ArithmeticOverflow,
    DivisionByZero,
    RequireFailed(String),
    TypeMismatch(String),
    StackDepthExceeded,
}

pub struct AspectActionEvaluator<'a> {
    module: &'a AspectModuleIr,
    max_steps: u64,
    max_stack_depth: usize,
}

impl<'a> AspectActionEvaluator<'a> {
    pub fn new(module: &'a AspectModuleIr, max_steps: u64) -> Self {
        Self {
            module,
            max_steps,
            max_stack_depth: 64,
        }
    }

    pub fn execute_action(
        &self,
        aspect: &str,
        action: &str,
        args: Vec<AspectValue>,
    ) -> Result<AspectExecutionReport, AspectEvalError> {
        let mut state = AspectEvalState {
            steps_used: 0,
            trace: Vec::new(),
            bindings: BTreeMap::new(),
            stack_depth: 0,
        };
        let return_value = self.execute_named_action(aspect, action, args, &mut state)?;
        let trace_root = aspect_trace_root(&state.trace);
        Ok(AspectExecutionReport {
            trace: state.trace,
            return_value,
            steps_used: state.steps_used,
            trace_root,
        })
    }

    fn execute_named_action(
        &self,
        aspect: &str,
        action: &str,
        args: Vec<AspectValue>,
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if state.stack_depth >= self.max_stack_depth {
            return Err(AspectEvalError::StackDepthExceeded);
        }
        state.stack_depth += 1;
        let result = self.execute_named_action_inner(aspect, action, args, state);
        state.stack_depth -= 1;
        result
    }

    fn execute_named_action_inner(
        &self,
        aspect: &str,
        action: &str,
        args: Vec<AspectValue>,
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        let key = qualified(aspect, action);
        let action_def =
            self.module
                .actions
                .get(&key)
                .ok_or_else(|| AspectEvalError::UnknownAction {
                    aspect: aspect.into(),
                    action: action.into(),
                })?;
        let body = action_def
            .body
            .as_ref()
            .ok_or_else(|| AspectEvalError::MissingActionBody {
                aspect: aspect.into(),
                action: action.into(),
            })?;
        let (params, rhs) = action_body_parts(action, body)?;
        if params.len() != args.len() {
            return Err(AspectEvalError::InvalidActionBody(format!(
                "expected {} args for {action}, got {}",
                params.len(),
                args.len()
            )));
        }

        let previous_bindings = state.bindings.clone();
        for (param, value) in params.into_iter().zip(args) {
            state.bindings.insert(param, value);
        }
        let result = self.eval_expr(rhs, state);
        state.bindings = previous_bindings;
        result
    }

    fn eval_expr(
        &self,
        expr: &Expr,
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        match expr {
            Expr::Atom(atom) => self.eval_atom(atom, state),
            Expr::List(items) => {
                state.charge(self.max_steps)?;
                let Some(Expr::Atom(head)) = items.first() else {
                    return Err(AspectEvalError::InvalidExpression(aspect_expr_source(expr)));
                };
                match head.as_str() {
                    "safe-add" => self.eval_checked_binary_amount(items, state, u128::checked_add),
                    "safe-sub" => self.eval_checked_binary_amount(items, state, u128::checked_sub),
                    "safe-mul" => self.eval_checked_binary_amount(items, state, u128::checked_mul),
                    "safe-div" => self.eval_div(items, state),
                    "require" => self.eval_require(items, state),
                    "if" => self.eval_if(items, state),
                    "and" => self.eval_and(items, state),
                    "or" => self.eval_or(items, state),
                    "not" => self.eval_not(items, state),
                    "=" => self.eval_eq(items, state),
                    "<" => self.eval_cmp(items, state, |left, right| left < right),
                    "<=" => self.eval_cmp(items, state, |left, right| left <= right),
                    ">" => self.eval_cmp(items, state, |left, right| left > right),
                    ">=" => self.eval_cmp(items, state, |left, right| left >= right),
                    "state-get" => self.eval_state_get(items, state),
                    "state-set!" => self.eval_state_set(items, state),
                    "registry-get" => self.eval_registry_get(items, state),
                    "registry-set!" => self.eval_registry_set(items, state),
                    "registry-consume!" => self.eval_registry_consume(items, state),
                    "emit!" => self.eval_emit(items, state),
                    "call-contract!" => self.eval_call_contract(items, state),
                    action_name => self.eval_action_call(action_name, items, state),
                }
            }
        }
    }

    fn eval_atom(
        &self,
        atom: &str,
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if let Some(variable) = atom.strip_prefix('$') {
            return state
                .bindings
                .get(variable)
                .cloned()
                .ok_or_else(|| AspectEvalError::UnknownVariable(variable.into()));
        }
        if atom == "True" {
            return Ok(AspectValue::Bool(true));
        }
        if atom == "False" {
            return Ok(AspectValue::Bool(false));
        }
        if let Ok(value) = atom.parse::<u128>() {
            return Ok(AspectValue::Amount(value));
        }
        Ok(AspectValue::Atom(atom.into()))
    }

    fn eval_checked_binary_amount(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
        operation: fn(u128, u128) -> Option<u128>,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 3 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected binary expression, got {}",
                items.len() - 1
            )));
        }
        let left = self.eval_expr(&items[1], state)?.into_amount()?;
        let right = self.eval_expr(&items[2], state)?.into_amount()?;
        operation(left, right)
            .map(AspectValue::Amount)
            .ok_or(AspectEvalError::ArithmeticOverflow)
    }

    fn eval_div(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 3 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected safe-div with 2 args, got {}",
                items.len() - 1
            )));
        }
        let left = self.eval_expr(&items[1], state)?.into_amount()?;
        let right = self.eval_expr(&items[2], state)?.into_amount()?;
        if right == 0 {
            return Err(AspectEvalError::DivisionByZero);
        }
        Ok(AspectValue::Amount(left / right))
    }

    fn eval_require(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 3 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected require with 2 args, got {}",
                items.len() - 1
            )));
        }
        let condition = self.eval_expr(&items[1], state)?.into_bool()?;
        if condition {
            Ok(AspectValue::Unit)
        } else {
            Err(AspectEvalError::RequireFailed(aspect_expr_source(
                &items[2],
            )))
        }
    }

    fn eval_if(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 4 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected if with 3 args, got {}",
                items.len() - 1
            )));
        }
        if self.eval_expr(&items[1], state)?.into_bool()? {
            self.eval_expr(&items[2], state)
        } else {
            self.eval_expr(&items[3], state)
        }
    }

    fn eval_and(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 3 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected and with 2 args, got {}",
                items.len() - 1
            )));
        }
        let left = self.eval_expr(&items[1], state)?.into_bool()?;
        if !left {
            return Ok(AspectValue::Bool(false));
        }
        Ok(AspectValue::Bool(
            self.eval_expr(&items[2], state)?.into_bool()?,
        ))
    }

    fn eval_or(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 3 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected or with 2 args, got {}",
                items.len() - 1
            )));
        }
        let left = self.eval_expr(&items[1], state)?.into_bool()?;
        if left {
            return Ok(AspectValue::Bool(true));
        }
        Ok(AspectValue::Bool(
            self.eval_expr(&items[2], state)?.into_bool()?,
        ))
    }

    fn eval_not(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 2 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected not with 1 arg, got {}",
                items.len() - 1
            )));
        }
        Ok(AspectValue::Bool(
            !self.eval_expr(&items[1], state)?.into_bool()?,
        ))
    }

    fn eval_eq(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 3 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected = with 2 args, got {}",
                items.len() - 1
            )));
        }
        Ok(AspectValue::Bool(
            self.eval_expr(&items[1], state)? == self.eval_expr(&items[2], state)?,
        ))
    }

    fn eval_cmp(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
        operation: fn(u128, u128) -> bool,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 3 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected comparison with 2 args, got {}",
                items.len() - 1
            )));
        }
        let left = self.eval_expr(&items[1], state)?.into_amount()?;
        let right = self.eval_expr(&items[2], state)?.into_amount()?;
        Ok(AspectValue::Bool(operation(left, right)))
    }

    fn eval_state_get(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        let state_name = atom_arg(items, 1, "state-get")?;
        state.trace.push(AspectHostOp::StateGet {
            state: state_name.into(),
        });
        Ok(AspectValue::Unit)
    }

    fn eval_state_set(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 3 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected state-set! with 2 args, got {}",
                items.len() - 1
            )));
        }
        let state_name = atom_arg(items, 1, "state-set!")?;
        let value = self.eval_expr(&items[2], state)?;
        state.trace.push(AspectHostOp::StateSet {
            state: state_name.into(),
            value,
        });
        Ok(AspectValue::Unit)
    }

    fn eval_registry_get(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        let registry = atom_arg(items, 1, "registry-get")?;
        state.trace.push(AspectHostOp::RegistryGet {
            registry: registry.into(),
        });
        Ok(AspectValue::Unit)
    }

    fn eval_registry_set(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 3 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected registry-set! with 2 args, got {}",
                items.len() - 1
            )));
        }
        let registry = atom_arg(items, 1, "registry-set!")?;
        let value = self.eval_expr(&items[2], state)?;
        state.trace.push(AspectHostOp::RegistrySet {
            registry: registry.into(),
            value,
        });
        Ok(AspectValue::Unit)
    }

    fn eval_registry_consume(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 3 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected registry-consume! with 2 args, got {}",
                items.len() - 1
            )));
        }
        let registry = atom_arg(items, 1, "registry-consume!")?;
        let value = self.eval_expr(&items[2], state)?;
        state.trace.push(AspectHostOp::RegistryConsume {
            registry: registry.into(),
            value,
        });
        Ok(AspectValue::Unit)
    }

    fn eval_emit(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() != 2 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected emit! with 1 arg, got {}",
                items.len() - 1
            )));
        }
        state.trace.push(AspectHostOp::Emit {
            event: aspect_expr_source(&items[1]),
        });
        Ok(AspectValue::Unit)
    }

    fn eval_call_contract(
        &self,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        if items.len() < 3 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "expected call-contract! with target, method, and args, got {}",
                items.len() - 1
            )));
        }
        let contract = atom_arg(items, 1, "call-contract!")?;
        let method = atom_arg(items, 2, "call-contract!")?;
        let mut args = Vec::new();
        for item in items.iter().skip(3) {
            args.push(self.eval_expr(item, state)?);
        }
        state.trace.push(AspectHostOp::CallContract {
            contract: contract.into(),
            method: method.into(),
            args,
        });
        Ok(AspectValue::Unit)
    }

    fn eval_action_call(
        &self,
        action_name: &str,
        items: &[Expr],
        state: &mut AspectEvalState,
    ) -> Result<AspectValue, AspectEvalError> {
        let matching_actions = self
            .module
            .actions
            .values()
            .filter(|action| action.action == action_name)
            .collect::<Vec<_>>();
        if matching_actions.len() != 1 {
            return Err(AspectEvalError::InvalidExpression(format!(
                "unknown or ambiguous expression head {action_name}"
            )));
        }
        let mut args = Vec::new();
        for item in items.iter().skip(1) {
            args.push(self.eval_expr(item, state)?);
        }
        self.execute_named_action(
            &matching_actions[0].aspect,
            &matching_actions[0].action,
            args,
            state,
        )
    }
}

struct AspectEvalState {
    steps_used: u64,
    trace: Vec<AspectHostOp>,
    bindings: BTreeMap<String, AspectValue>,
    stack_depth: usize,
}

impl AspectEvalState {
    fn charge(&mut self, max_steps: u64) -> Result<(), AspectEvalError> {
        if self.steps_used >= max_steps {
            return Err(AspectEvalError::StepBudgetExceeded);
        }
        self.steps_used += 1;
        Ok(())
    }
}

impl AspectValue {
    fn into_amount(self) -> Result<u128, AspectEvalError> {
        match self {
            AspectValue::Amount(value) => Ok(value),
            value => Err(AspectEvalError::TypeMismatch(format!(
                "expected amount, got {value:?}"
            ))),
        }
    }

    fn into_bool(self) -> Result<bool, AspectEvalError> {
        match self {
            AspectValue::Bool(value) => Ok(value),
            value => Err(AspectEvalError::TypeMismatch(format!(
                "expected bool, got {value:?}"
            ))),
        }
    }
}

pub fn aspect_trace_root(trace: &[AspectHostOp]) -> String {
    let bytes = serde_json::to_vec(trace).expect("aspect host trace serializes");
    let digest = Sha256::digest(bytes);
    hex_lower(&digest)
}

fn action_body_parts<'a>(
    action: &str,
    body: &'a Expr,
) -> Result<(Vec<String>, &'a Expr), AspectEvalError> {
    let Expr::List(items) = body else {
        return Err(AspectEvalError::InvalidActionBody(aspect_expr_source(body)));
    };
    if items.len() != 3 {
        return Err(AspectEvalError::InvalidActionBody(aspect_expr_source(body)));
    }
    match items.first() {
        Some(Expr::Atom(head)) if head == "=" => {}
        _ => return Err(AspectEvalError::InvalidActionBody(aspect_expr_source(body))),
    }
    let Expr::List(signature) = &items[1] else {
        return Err(AspectEvalError::InvalidActionBody(aspect_expr_source(body)));
    };
    match signature.first() {
        Some(Expr::Atom(head)) if head == action => {}
        _ => return Err(AspectEvalError::InvalidActionBody(aspect_expr_source(body))),
    }
    let mut params = Vec::new();
    for param in signature.iter().skip(1) {
        let Expr::Atom(param) = param else {
            return Err(AspectEvalError::InvalidActionBody(aspect_expr_source(body)));
        };
        let Some(name) = param.strip_prefix('$') else {
            return Err(AspectEvalError::InvalidActionBody(aspect_expr_source(body)));
        };
        params.push(name.into());
    }
    Ok((params, &items[2]))
}

fn atom_arg<'a>(items: &'a [Expr], index: usize, form: &str) -> Result<&'a str, AspectEvalError> {
    match items.get(index) {
        Some(Expr::Atom(atom)) => Ok(atom),
        Some(expr) => Err(AspectEvalError::InvalidExpression(format!(
            "expected atom for {form}, got {}",
            aspect_expr_source(expr)
        ))),
        None => Err(AspectEvalError::InvalidExpression(format!(
            "missing arg {index} for {form}"
        ))),
    }
}

fn aspect_expr_source(expr: &Expr) -> String {
    match expr {
        Expr::Atom(atom) => atom.clone(),
        Expr::List(items) => {
            let body = items
                .iter()
                .map(aspect_expr_source)
                .collect::<Vec<_>>()
                .join(" ");
            format!("({body})")
        }
    }
}

fn qualified(namespace: &str, name: &str) -> String {
    format!("{namespace}::{name}")
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
    use detta_aspects::parse_verify_module;

    fn aspect_module(source: &str) -> AspectModuleIr {
        let (_, _, verified) = parse_verify_module(source).unwrap();
        verified.ir
    }

    #[test]
    fn aspect_action_evaluator_emits_guarded_state_write_trace() {
        let module = aspect_module(
            "
            (: Bool Type)
            (: Amount Type)
            (aspect WriterAspect)
            (owns WriterAspect balance Amount)
            (action WriterAspect writeBalance)
            (derived WriterAspect writeBalance
              (= (writeBalance $amount)
                 (state-set! balance (safe-add $amount 1))))
            (bundle WriterBundle)
            (bundle-includes WriterBundle WriterAspect)
            (projection WriterBundle Write (= (API.write $amount) (writeBalance $amount)))
            (method-abi WriterBundle Write (args (amount Amount)) Bool)
            (method-policy WriterBundle Write TxSender (effects WriteState) (invariants))
        ",
        );
        let report = AspectActionEvaluator::new(&module, 16)
            .execute_action("WriterAspect", "writeBalance", vec![AspectValue::Amount(9)])
            .unwrap();

        assert_eq!(
            report.trace,
            vec![AspectHostOp::StateSet {
                state: "balance".into(),
                value: AspectValue::Amount(10),
            }]
        );
        assert_eq!(report.return_value, AspectValue::Unit);
        assert_eq!(report.trace_root, aspect_trace_root(&report.trace));
        assert!(report.steps_used > 0);
    }

    #[test]
    fn aspect_action_evaluator_handles_require_and_boolean_conditions() {
        let module = aspect_module(
            "
            (: Bool Type)
            (: Amount Type)
            (aspect GuardAspect)
            (action GuardAspect guarded)
            (derived GuardAspect guarded
              (= (guarded $amount)
                 (if (< $amount 10)
                     (safe-add $amount 1)
                     (require False TOO_LARGE))))
            (bundle GuardBundle)
            (bundle-includes GuardBundle GuardAspect)
            (projection GuardBundle Guarded (= (API.guarded $amount) (guarded $amount)))
            (method-abi GuardBundle Guarded (args (amount Amount)) Amount)
            (method-policy GuardBundle Guarded TxSender (effects) (invariants))
        ",
        );
        let evaluator = AspectActionEvaluator::new(&module, 16);

        assert_eq!(
            evaluator
                .execute_action("GuardAspect", "guarded", vec![AspectValue::Amount(8)])
                .unwrap()
                .return_value,
            AspectValue::Amount(9)
        );
        assert_eq!(
            evaluator
                .execute_action("GuardAspect", "guarded", vec![AspectValue::Amount(10)])
                .unwrap_err(),
            AspectEvalError::RequireFailed("TOO_LARGE".into())
        );
    }

    #[test]
    fn aspect_action_evaluator_rejects_arithmetic_overflow() {
        let module = aspect_module(
            "
            (: Amount Type)
            (aspect MathAspect)
            (action MathAspect addOne)
            (derived MathAspect addOne (= (addOne $amount) (safe-add $amount 1)))
            (bundle MathBundle)
            (bundle-includes MathBundle MathAspect)
            (projection MathBundle AddOne (= (API.addOne $amount) (addOne $amount)))
            (method-abi MathBundle AddOne (args (amount Amount)) Amount)
            (method-policy MathBundle AddOne TxSender (effects) (invariants))
        ",
        );
        let error = AspectActionEvaluator::new(&module, 16)
            .execute_action("MathAspect", "addOne", vec![AspectValue::Amount(u128::MAX)])
            .unwrap_err();

        assert_eq!(error, AspectEvalError::ArithmeticOverflow);
    }

    #[test]
    fn aspect_action_evaluator_enforces_step_budget() {
        let module = aspect_module(
            "
            (: Amount Type)
            (aspect MathAspect)
            (action MathAspect addTwo)
            (derived MathAspect addTwo
              (= (addTwo $amount)
                 (safe-add (safe-add $amount 1) 1)))
            (bundle MathBundle)
            (bundle-includes MathBundle MathAspect)
            (projection MathBundle AddTwo (= (API.addTwo $amount) (addTwo $amount)))
            (method-abi MathBundle AddTwo (args (amount Amount)) Amount)
            (method-policy MathBundle AddTwo TxSender (effects) (invariants))
        ",
        );
        let error = AspectActionEvaluator::new(&module, 1)
            .execute_action("MathAspect", "addTwo", vec![AspectValue::Amount(1)])
            .unwrap_err();

        assert_eq!(error, AspectEvalError::StepBudgetExceeded);
    }

    #[test]
    fn aspect_action_evaluator_emits_registry_and_call_traces() {
        let module = aspect_module(
            "
            (: Bool Type)
            (: Amount Type)
            (aspect AllowanceAspect)
            (registry-owns AllowanceAspect allowance Amount)
            (action AllowanceAspect spendAndCall)
            (derived AllowanceAspect spendAndCall
              (= (spendAndCall $amount)
                 (call-contract! TokenA transfer (registry-consume! allowance $amount))))
            (bundle AllowanceBundle)
            (bundle-includes AllowanceBundle AllowanceAspect)
            (projection AllowanceBundle SpendAndCall (= (API.spend $amount) (spendAndCall $amount)))
            (method-abi AllowanceBundle SpendAndCall (args (amount Amount)) Bool)
            (method-policy AllowanceBundle SpendAndCall TxSender (effects ConsumeRegistryGrant CallContract) (invariants))
        ",
        );
        let report = AspectActionEvaluator::new(&module, 16)
            .execute_action(
                "AllowanceAspect",
                "spendAndCall",
                vec![AspectValue::Amount(5)],
            )
            .unwrap();

        assert_eq!(
            report.trace,
            vec![
                AspectHostOp::RegistryConsume {
                    registry: "allowance".into(),
                    value: AspectValue::Amount(5),
                },
                AspectHostOp::CallContract {
                    contract: "TokenA".into(),
                    method: "transfer".into(),
                    args: vec![AspectValue::Unit],
                },
            ]
        );
    }
}
