use detta_core::{Block, ContractKind, DeTTaState, ExecutionError, Method, StateKey, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LintError {
    ContractHasNoExports {
        contract: String,
    },
    ContractHasNoDeclaredInvariants {
        contract: String,
    },
    GovernanceTargetMissing {
        governance: String,
        target: String,
    },
    LendingOracleMissing {
        vault: String,
        oracle: String,
    },
    ScheduledUpgradeTargetMissing {
        upgrade_id: String,
        target: String,
    },
    ScheduledUpgradeGovernanceMissing {
        upgrade_id: String,
        governance: String,
    },
    PausedContractMissing {
        contract: String,
    },
}

pub fn lint_state(state: &DeTTaState) -> Vec<LintError> {
    let contract_ids: BTreeSet<_> = state
        .contract_records()
        .map(|contract| contract.contract_id.clone())
        .collect();
    let mut errors = Vec::new();

    for contract in state.contract_records() {
        if contract.exported_methods().is_empty() {
            errors.push(LintError::ContractHasNoExports {
                contract: contract.contract_id.clone(),
            });
        }
        if contract.declared_invariants().is_empty() {
            errors.push(LintError::ContractHasNoDeclaredInvariants {
                contract: contract.contract_id.clone(),
            });
        }

        match &contract.kind {
            ContractKind::Governance {
                governed_contract, ..
            } if !contract_ids.contains(governed_contract) => {
                errors.push(LintError::GovernanceTargetMissing {
                    governance: contract.contract_id.clone(),
                    target: governed_contract.clone(),
                });
            }
            ContractKind::LendingVault {
                oracle_contract, ..
            } if !contract_ids.contains(oracle_contract) => {
                errors.push(LintError::LendingOracleMissing {
                    vault: contract.contract_id.clone(),
                    oracle: oracle_contract.clone(),
                });
            }
            _ => {}
        }
    }

    for upgrade in state.scheduled_upgrades() {
        if !contract_ids.contains(&upgrade.target_contract) {
            errors.push(LintError::ScheduledUpgradeTargetMissing {
                upgrade_id: upgrade.upgrade_id.clone(),
                target: upgrade.target_contract.clone(),
            });
        }
        if !contract_ids.contains(&upgrade.governance_contract) {
            errors.push(LintError::ScheduledUpgradeGovernanceMissing {
                upgrade_id: upgrade.upgrade_id.clone(),
                governance: upgrade.governance_contract.clone(),
            });
        }
    }

    for paused in state.paused_contracts() {
        if !contract_ids.contains(paused) {
            errors.push(LintError::PausedContractMissing {
                contract: paused.clone(),
            });
        }
    }

    errors
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TraceOp {
    StateGet(StateKey),
    StateSet(StateKey),
    Abort,
    CallContract { contract: String, method: Method },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TraceError {
    WriteScopeViolation { key: StateKey },
    ContractIsolationViolation { key: StateKey, contract: String },
}

pub fn verify_kernel_trace(
    contract: &str,
    write_scope: &BTreeSet<StateKey>,
    trace: &[TraceOp],
) -> Vec<TraceError> {
    let mut errors = Vec::new();

    for op in trace {
        if let TraceOp::StateSet(key) = op {
            if owner_of_key(key) != contract {
                errors.push(TraceError::ContractIsolationViolation {
                    key: key.clone(),
                    contract: contract.to_string(),
                });
            }
            if !write_scope.contains(key) {
                errors.push(TraceError::WriteScopeViolation { key: key.clone() });
            }
        }
    }

    errors
}

pub fn verify_deterministic_replay(
    initial: &DeTTaState,
    transactions: Vec<Transaction>,
    height: u64,
) -> Result<Block, ExecutionError> {
    let (left_block, left_state) =
        initial.build_block(height, transactions.clone(), 1_000, "verifier-a", "cert-a");
    let (right_block, right_state) =
        initial.build_block(height, transactions, 1_000, "verifier-a", "cert-a");

    if left_block != right_block
        || left_state.storage_root() != right_state.storage_root()
        || left_state.registry_root() != right_state.registry_root()
        || left_state.event_root() != right_state.event_root()
        || left_state.global_state_root() != right_state.global_state_root()
    {
        return Err(ExecutionError::InvariantViolation);
    }

    Ok(left_block)
}

fn owner_of_key(key: &StateKey) -> &str {
    match key {
        StateKey::Balance { contract, .. }
        | StateKey::TotalSupply { contract, .. }
        | StateKey::Reserve { contract, .. }
        | StateKey::LpSupply { contract }
        | StateKey::LpBalance { contract, .. }
        | StateKey::OraclePrice { contract, .. }
        | StateKey::OracleTimestamp { contract, .. }
        | StateKey::BridgeMessageConsumed { contract, .. }
        | StateKey::Collateral { contract, .. }
        | StateKey::Debt { contract, .. }
        | StateKey::StakeBalance { contract, .. }
        | StateKey::TotalStaked { contract, .. } => contract,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Argument, DeTTaState, Method, Transaction};

    fn seeded_state() -> DeTTaState {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_token(
                "TokenA",
                "USDC",
                vec![("Alice".into(), 100), ("Bob".into(), 50)],
            )
            .unwrap();
        state
    }

    #[test]
    fn linter_accepts_valid_token_state() {
        let state = seeded_state();

        assert_eq!(lint_state(&state), vec![]);
    }

    #[test]
    fn linter_accepts_deployed_defi_contracts_with_invariant_manifests() {
        let mut state = seeded_state();
        state.deploy_amm_pool("PoolA", "USDC", "ETH").unwrap();
        state
            .deploy_oracle("OracleA", "USDC", "Reporter", 10)
            .unwrap();
        state.deploy_bridge("BridgeA", "SourceChain").unwrap();
        state.deploy_governance("GovA", "TokenA", "Admin").unwrap();
        state
            .deploy_lending_vault("VaultA", "USDC", "dUSD", "OracleA", 5_000, 10)
            .unwrap();
        state.deploy_staking("StakeA", "USDC").unwrap();
        state.deploy_router("RouterA", "TokenA").unwrap();

        assert_eq!(lint_state(&state), vec![]);
    }

    #[test]
    fn symbolic_trace_rejects_out_of_scope_write() {
        let allowed_key = StateKey::Balance {
            contract: "TokenA".into(),
            owner: "Alice".into(),
            asset: "USDC".into(),
        };
        let forbidden_key = StateKey::Balance {
            contract: "TokenA".into(),
            owner: "Mallory".into(),
            asset: "USDC".into(),
        };
        let write_scope = BTreeSet::from([allowed_key.clone()]);
        let trace = vec![
            TraceOp::StateSet(allowed_key),
            TraceOp::StateSet(forbidden_key.clone()),
        ];

        assert_eq!(
            verify_kernel_trace("TokenA", &write_scope, &trace),
            vec![TraceError::WriteScopeViolation { key: forbidden_key }]
        );
    }

    #[test]
    fn symbolic_trace_rejects_cross_contract_write() {
        let key = StateKey::Balance {
            contract: "TokenB".into(),
            owner: "Alice".into(),
            asset: "USDC".into(),
        };
        let write_scope = BTreeSet::from([key.clone()]);
        let trace = vec![TraceOp::StateSet(key.clone())];

        assert_eq!(
            verify_kernel_trace("TokenA", &write_scope, &trace),
            vec![TraceError::ContractIsolationViolation {
                key,
                contract: "TokenA".into()
            }]
        );
    }

    #[test]
    fn replay_verifier_accepts_deterministic_block() {
        let state = seeded_state();
        let tx = Transaction {
            chain_id: "detta-local".into(),
            tx_hash: "tx1".into(),
            sender: "Alice".into(),
            nonce: 1,
            target: "TokenA".into(),
            method: Method::Transfer,
            args: vec![
                Argument::Principal("Bob".into()),
                Argument::Asset("USDC".into()),
                Argument::Amount(10),
            ],
            signature_ok: true,
            budget: 1_000_000,
        };

        let block = verify_deterministic_replay(&state, vec![tx], 1).unwrap();

        assert_eq!(block.header.height, 1);
        assert_eq!(block.transactions.len(), 1);
    }
}
