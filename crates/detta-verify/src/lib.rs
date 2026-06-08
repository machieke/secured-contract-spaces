use detta_core::{
    Argument, Block, ChainId, ContractId, ContractKind, DeTTaState, ExecutionError,
    InvariantFailure, Method, StateKey, Transaction,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LintError {
    ContractHasNoExports {
        contract: String,
    },
    ContractHasNoDeclaredInvariants {
        contract: String,
    },
    ContractExportMissingPolicy {
        contract: String,
        method: Method,
    },
    ContractPolicyWithoutExport {
        contract: String,
        method: Method,
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
    ScheduledPolicyUpdateTargetMissing {
        update_id: String,
        target: String,
    },
    ScheduledPolicyUpdateGovernanceMissing {
        update_id: String,
        governance: String,
    },
    ScheduledPolicyUpdateMethodMissing {
        update_id: String,
        target: String,
        method: Method,
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
        for method in contract.exported_methods() {
            if !contract.method_policies().contains_key(method) {
                errors.push(LintError::ContractExportMissingPolicy {
                    contract: contract.contract_id.clone(),
                    method: method.clone(),
                });
            }
        }
        for method in contract.method_policies().keys() {
            if !contract.exported_methods().contains(method) {
                errors.push(LintError::ContractPolicyWithoutExport {
                    contract: contract.contract_id.clone(),
                    method: method.clone(),
                });
            }
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

    for update in state.scheduled_policy_updates() {
        if !contract_ids.contains(&update.target_contract) {
            errors.push(LintError::ScheduledPolicyUpdateTargetMissing {
                update_id: update.update_id.clone(),
                target: update.target_contract.clone(),
            });
        } else if let Some(target) = state.contract(&update.target_contract) {
            if target.method_policy(&update.method).is_none() {
                errors.push(LintError::ScheduledPolicyUpdateMethodMissing {
                    update_id: update.update_id.clone(),
                    target: update.target_contract.clone(),
                    method: update.method.clone(),
                });
            }
        }
        if !contract_ids.contains(&update.governance_contract) {
            errors.push(LintError::ScheduledPolicyUpdateGovernanceMissing {
                update_id: update.update_id.clone(),
                governance: update.governance_contract.clone(),
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

pub fn check_declared_invariants(state: &DeTTaState) -> Vec<InvariantFailure> {
    state.check_declared_invariants()
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DifferentialReplayError {
    NotEnoughReplicas,
    ReplicaMismatch {
        replica: usize,
        expected_root: String,
        actual_root: String,
    },
    BlockMismatch {
        replica: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TheoremEvidenceKind {
    RuntimeTest,
    Model,
    Verifier,
    Fixture,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TheoremEvidence {
    pub kind: TheoremEvidenceKind,
    pub reference: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SafetyTheoremCoverage {
    pub id: &'static str,
    pub name: &'static str,
    pub evidence: Vec<TheoremEvidence>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ModelArtifactRoot {
    pub path: &'static str,
    pub sha256: String,
}

pub const PROOF_ARTIFACT_MANIFEST_SCHEMA: &str = "detta.proof-artifact-manifest.v2";
pub const PROOF_ARTIFACT_MANIFEST_SCHEMA_VERSION: u32 = 2;
pub const PROOF_ARTIFACT_MANIFEST_PROJECT: &str = "DeTTa";
pub const PROOF_ARTIFACT_MANIFEST_SCOPE: &str =
    "Secured Contract Spaces runtime safety obligations";
pub const PROOF_MODEL_ARTIFACT_COUNT: usize = 2;
pub const PROOF_RUNTIME_ARTIFACT_COUNT: usize = 11;
pub const PROOF_RELEASE_ATTESTATION_COUNT: usize = 7;
pub const SHA256_HEX_LENGTH: usize = 64;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProofArtifactManifest {
    pub schema: &'static str,
    pub schema_version: u32,
    pub project: &'static str,
    pub scope: &'static str,
    pub model_artifacts: Vec<ModelArtifactRoot>,
    pub runtime_artifacts: Vec<ModelArtifactRoot>,
    pub theorem_count: usize,
    pub coverage: Vec<SafetyTheoremCoverage>,
}

pub fn proof_artifact_manifest() -> ProofArtifactManifest {
    let coverage = scs_theorem_coverage();
    ProofArtifactManifest {
        schema: PROOF_ARTIFACT_MANIFEST_SCHEMA,
        schema_version: PROOF_ARTIFACT_MANIFEST_SCHEMA_VERSION,
        project: PROOF_ARTIFACT_MANIFEST_PROJECT,
        scope: PROOF_ARTIFACT_MANIFEST_SCOPE,
        model_artifacts: proof_model_artifacts(),
        runtime_artifacts: proof_runtime_artifacts(),
        theorem_count: coverage.len(),
        coverage,
    }
}

pub fn proof_model_artifacts() -> Vec<ModelArtifactRoot> {
    vec![
        ModelArtifactRoot {
            path: "models/DeTTaBlockExecution.tla",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/DeTTaBlockExecution.tla"
            )),
        },
        ModelArtifactRoot {
            path: "models/DeTTaBlockExecution.cfg",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/DeTTaBlockExecution.cfg"
            )),
        },
    ]
}

pub fn proof_runtime_artifacts() -> Vec<ModelArtifactRoot> {
    vec![
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-proof-trace.json",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-proof-trace.json"
            )),
        },
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-proof-trace.sha256",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-proof-trace.sha256"
            )),
        },
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-proof-trace-root.sha256",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-proof-trace-root.sha256"
            )),
        },
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-forbidden-primitives.json",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-forbidden-primitives.json"
            )),
        },
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-forbidden-primitives.sha256",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-forbidden-primitives.sha256"
            )),
        },
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-resource-exhaustion.json",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-resource-exhaustion.json"
            )),
        },
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-resource-exhaustion.sha256",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-resource-exhaustion.sha256"
            )),
        },
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-arithmetic-overflow.json",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-arithmetic-overflow.json"
            )),
        },
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-arithmetic-overflow.sha256",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-arithmetic-overflow.sha256"
            )),
        },
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-fixture-inventory.json",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-fixture-inventory.json"
            )),
        },
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-fixture-inventory.sha256",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-fixture-inventory.sha256"
            )),
        },
    ]
}

pub fn proof_artifact_manifest_root_bytes(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

pub fn checked_in_proof_artifact_manifest_root() -> String {
    proof_artifact_manifest_root_bytes(include_bytes!(
        "../../../models/detta-proof-artifact-manifest.json"
    ))
}

pub fn scs_theorem_coverage() -> Vec<SafetyTheoremCoverage> {
    use TheoremEvidenceKind::{Fixture, Model, RuntimeTest, Verifier};

    vec![
        SafetyTheoremCoverage {
            id: "THM-001",
            name: "Dispatcher-Only External Mutation",
            evidence: vec![
                TheoremEvidence {
                    kind: RuntimeTest,
                    reference: "detta_core::tests::missing_method_policy_is_denied",
                },
                TheoremEvidence {
                    kind: Model,
                    reference: "models/DeTTaBlockExecution.tla::DispatcherOnlyMutation",
                },
            ],
        },
        SafetyTheoremCoverage {
            id: "THM-002",
            name: "Contract Isolation",
            evidence: vec![
                TheoremEvidence {
                    kind: Verifier,
                    reference: "detta_verify::verify_kernel_trace",
                },
                TheoremEvidence {
                    kind: RuntimeTest,
                    reference: "detta_verify::tests::symbolic_trace_rejects_cross_contract_write",
                },
            ],
        },
        SafetyTheoremCoverage {
            id: "THM-003",
            name: "Method Write-Scope Safety",
            evidence: vec![
                TheoremEvidence {
                    kind: Verifier,
                    reference: "detta_verify::verify_kernel_trace",
                },
                TheoremEvidence {
                    kind: RuntimeTest,
                    reference: "detta_verify::tests::symbolic_trace_rejects_out_of_scope_write",
                },
                TheoremEvidence {
                    kind: Fixture,
                    reference: "models/detta-restricted-evaluator-proof-trace.json",
                },
            ],
        },
        SafetyTheoremCoverage {
            id: "THM-004",
            name: "No Authority From Syntax",
            evidence: vec![TheoremEvidence {
                kind: RuntimeTest,
                reference: "detta_evaluator::tests::transfer_like_arguments_are_plain_data_not_authority",
            }],
        },
        SafetyTheoremCoverage {
            id: "THM-005",
            name: "Live Registry Authorization",
            evidence: vec![TheoremEvidence {
                kind: RuntimeTest,
                reference: "detta_core::tests::transfer_from_requires_live_registry_allowance",
            }],
        },
        SafetyTheoremCoverage {
            id: "THM-006",
            name: "Registry Consumption Atomicity",
            evidence: vec![
                TheoremEvidence {
                    kind: RuntimeTest,
                    reference: "detta_core::tests::allowance_consumption_reverts_with_failed_transfer",
                },
                TheoremEvidence {
                    kind: Model,
                    reference: "models/DeTTaBlockExecution.tla::AtomicRevert",
                },
            ],
        },
        SafetyTheoremCoverage {
            id: "THM-007",
            name: "Event Atomicity",
            evidence: vec![
                TheoremEvidence {
                    kind: RuntimeTest,
                    reference: "detta_core::tests::insufficient_balance_reverts_storage_registry_and_events",
                },
                TheoremEvidence {
                    kind: Model,
                    reference: "models/DeTTaBlockExecution.tla::AtomicRevert",
                },
            ],
        },
        SafetyTheoremCoverage {
            id: "THM-008",
            name: "Invariant Preservation",
            evidence: vec![
                TheoremEvidence {
                    kind: RuntimeTest,
                    reference: "detta_core::tests::transfer_preserves_supply_and_commits_event",
                },
                TheoremEvidence {
                    kind: RuntimeTest,
                    reference: "detta_core::tests::invariant_failure_reverts_full_transition",
                },
            ],
        },
        SafetyTheoremCoverage {
            id: "THM-009",
            name: "Determinism",
            evidence: vec![
                TheoremEvidence {
                    kind: Verifier,
                    reference: "detta_verify::verify_differential_replay",
                },
                TheoremEvidence {
                    kind: RuntimeTest,
                    reference: "detta_core::tests::deterministic_replay_produces_identical_roots",
                },
                TheoremEvidence {
                    kind: Fixture,
                    reference: "models/detta-restricted-evaluator-resource-exhaustion.json",
                },
            ],
        },
        SafetyTheoremCoverage {
            id: "THM-010",
            name: "Replay Safety",
            evidence: vec![
                TheoremEvidence {
                    kind: RuntimeTest,
                    reference: "detta_core::tests::replayed_nonce_is_rejected",
                },
                TheoremEvidence {
                    kind: Model,
                    reference: "models/DeTTaBlockExecution.tla::ReplaySafety",
                },
            ],
        },
        SafetyTheoremCoverage {
            id: "THM-011",
            name: "Caller Integrity",
            evidence: vec![TheoremEvidence {
                kind: RuntimeTest,
                reference: "detta_core::tests::caller_identity_cannot_be_forged",
            }],
        },
        SafetyTheoremCoverage {
            id: "THM-012",
            name: "No Write-Scope Leakage Across Calls",
            evidence: vec![
                TheoremEvidence {
                    kind: RuntimeTest,
                    reference: "detta_core::tests::router_cross_contract_call_does_not_inherit_user_allowance",
                },
                TheoremEvidence {
                    kind: Model,
                    reference: "models/DeTTaBlockExecution.tla::WriteScopeSafety",
                },
            ],
        },
        SafetyTheoremCoverage {
            id: "THM-013",
            name: "View Read-Only Safety",
            evidence: vec![TheoremEvidence {
                kind: RuntimeTest,
                reference: "detta_core::tests::view_reads_are_read_only",
            }],
        },
        SafetyTheoremCoverage {
            id: "THM-014",
            name: "Schema Safety",
            evidence: vec![
                TheoremEvidence {
                    kind: RuntimeTest,
                    reference: "detta_core::tests::arithmetic_overflow_reverts_without_state_or_events",
                },
                TheoremEvidence {
                    kind: Fixture,
                    reference: "models/detta-restricted-evaluator-arithmetic-overflow.json",
                },
                TheoremEvidence {
                    kind: Model,
                    reference: "models/DeTTaBlockExecution.tla::TypeOK",
                },
            ],
        },
        SafetyTheoremCoverage {
            id: "THM-015",
            name: "Raw Primitive Exclusion",
            evidence: vec![
                TheoremEvidence {
                    kind: RuntimeTest,
                    reference: "detta_evaluator::tests::evaluator_rejects_forbidden_primitive",
                },
                TheoremEvidence {
                    kind: Fixture,
                    reference: "models/detta-restricted-evaluator-forbidden-primitives.json",
                },
            ],
        },
    ]
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
    verify_differential_replay(initial, transactions, height, 2)
        .map_err(|_| ExecutionError::InvariantViolation)
}

pub fn verify_differential_replay(
    initial: &DeTTaState,
    transactions: Vec<Transaction>,
    height: u64,
    replicas: usize,
) -> Result<Block, DifferentialReplayError> {
    if replicas < 2 {
        return Err(DifferentialReplayError::NotEnoughReplicas);
    }

    let (expected_block, expected_state) =
        initial.build_block(height, transactions.clone(), 1_000, "verifier", "cert");
    let expected_root = expected_state.global_state_root();

    for replica in 1..replicas {
        let (block, state) =
            initial.build_block(height, transactions.clone(), 1_000, "verifier", "cert");
        let actual_root = state.global_state_root();
        if actual_root != expected_root {
            return Err(DifferentialReplayError::ReplicaMismatch {
                replica,
                expected_root,
                actual_root,
            });
        }
        if block != expected_block {
            return Err(DifferentialReplayError::BlockMismatch { replica });
        }
    }

    Ok(expected_block)
}

pub fn deterministic_transfer_corpus(
    chain_id: ChainId,
    target: ContractId,
    asset: String,
    seed: u64,
    count: usize,
) -> Vec<Transaction> {
    let principals = ["Alice", "Bob", "Carol"];
    let mut nonces = [0u64; 3];
    let mut rng = seed;
    let mut transactions = Vec::with_capacity(count);

    for index in 0..count {
        rng = lcg_next(rng);
        let sender_index = (rng as usize) % principals.len();
        rng = lcg_next(rng);
        let mut recipient_index = (rng as usize) % principals.len();
        if recipient_index == sender_index {
            recipient_index = (recipient_index + 1) % principals.len();
        }
        rng = lcg_next(rng);
        let amount = (rng as u128 % 17) + 1;
        nonces[sender_index] += 1;

        transactions.push(Transaction {
            chain_id: chain_id.clone(),
            tx_hash: format!("fuzz-tx-{seed}-{index}"),
            sender: principals[sender_index].to_string(),
            nonce: nonces[sender_index],
            target: target.clone(),
            method: Method::Transfer,
            args: vec![
                Argument::Principal(principals[recipient_index].to_string()),
                Argument::Asset(asset.clone()),
                Argument::Amount(amount),
            ],
            signature_ok: true,
            budget: 1_000_000,
        });
    }

    transactions
}

fn lcg_next(value: u64) -> u64 {
    value
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1)
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
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
    use std::collections::BTreeMap;

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
        assert_eq!(check_declared_invariants(&state), vec![]);
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
    fn scs_theorem_coverage_is_complete_and_auditable() {
        let coverage = scs_theorem_coverage();
        let ids: BTreeSet<_> = coverage.iter().map(|entry| entry.id).collect();

        assert_eq!(coverage.len(), 15);
        assert_eq!(ids.len(), 15);
        for index in 1..=15 {
            let id = format!("THM-{index:03}");
            assert!(ids.contains(id.as_str()), "{id} has no coverage entry");
        }
        for entry in coverage {
            assert!(!entry.name.is_empty());
            assert!(!entry.evidence.is_empty(), "{} has no evidence", entry.id);
            for evidence in entry.evidence {
                assert!(!evidence.reference.is_empty());
            }
        }
    }

    #[test]
    fn scs_theorem_coverage_order_is_stable() {
        let coverage_ids: Vec<_> = scs_theorem_coverage()
            .iter()
            .map(|entry| entry.id)
            .collect();

        assert_eq!(
            coverage_ids,
            vec![
                "THM-001", "THM-002", "THM-003", "THM-004", "THM-005", "THM-006", "THM-007",
                "THM-008", "THM-009", "THM-010", "THM-011", "THM-012", "THM-013", "THM-014",
                "THM-015",
            ]
        );
    }

    #[test]
    fn scs_theorem_ids_use_fixed_format() {
        for (index, entry) in scs_theorem_coverage().iter().enumerate() {
            let expected_id = format!("THM-{:03}", index + 1);
            let suffix = entry
                .id
                .strip_prefix("THM-")
                .expect("theorem ID must use THM- prefix");

            assert_eq!(entry.id, expected_id.as_str());
            assert_eq!(suffix.len(), 3, "{} must use three digits", entry.id);
            assert!(
                suffix.bytes().all(|byte| byte.is_ascii_digit()),
                "{} must use an ASCII decimal suffix",
                entry.id
            );
        }
    }

    #[test]
    fn scs_theorem_ids_map_to_expected_names() {
        let names_by_id: BTreeMap<_, _> = scs_theorem_coverage()
            .iter()
            .map(|entry| (entry.id, entry.name))
            .collect();

        assert_eq!(
            names_by_id,
            BTreeMap::from([
                ("THM-001", "Dispatcher-Only External Mutation"),
                ("THM-002", "Contract Isolation"),
                ("THM-003", "Method Write-Scope Safety"),
                ("THM-004", "No Authority From Syntax"),
                ("THM-005", "Live Registry Authorization"),
                ("THM-006", "Registry Consumption Atomicity"),
                ("THM-007", "Event Atomicity"),
                ("THM-008", "Invariant Preservation"),
                ("THM-009", "Determinism"),
                ("THM-010", "Replay Safety"),
                ("THM-011", "Caller Integrity"),
                ("THM-012", "No Write-Scope Leakage Across Calls"),
                ("THM-013", "View Read-Only Safety"),
                ("THM-014", "Schema Safety"),
                ("THM-015", "Raw Primitive Exclusion"),
            ])
        );
    }

    #[test]
    fn scs_theorem_names_are_unique() {
        let coverage = scs_theorem_coverage();
        let names: BTreeSet<_> = coverage.iter().map(|entry| entry.name).collect();

        assert_eq!(names.len(), coverage.len());
    }

    #[test]
    fn scs_theorem_evidence_order_is_stable() {
        use TheoremEvidenceKind::{Fixture, Model, RuntimeTest, Verifier};

        let evidence_by_theorem: BTreeMap<_, Vec<_>> = scs_theorem_coverage()
            .into_iter()
            .map(|entry| {
                (
                    entry.id,
                    entry
                        .evidence
                        .into_iter()
                        .map(|evidence| (evidence.kind, evidence.reference))
                        .collect(),
                )
            })
            .collect();

        assert_eq!(
            evidence_by_theorem,
            BTreeMap::from([
                (
                    "THM-001",
                    vec![
                        (
                            RuntimeTest,
                            "detta_core::tests::missing_method_policy_is_denied",
                        ),
                        (
                            Model,
                            "models/DeTTaBlockExecution.tla::DispatcherOnlyMutation",
                        ),
                    ],
                ),
                (
                    "THM-002",
                    vec![
                        (Verifier, "detta_verify::verify_kernel_trace"),
                        (
                            RuntimeTest,
                            "detta_verify::tests::symbolic_trace_rejects_cross_contract_write",
                        ),
                    ],
                ),
                (
                    "THM-003",
                    vec![
                        (Verifier, "detta_verify::verify_kernel_trace"),
                        (
                            RuntimeTest,
                            "detta_verify::tests::symbolic_trace_rejects_out_of_scope_write",
                        ),
                        (
                            Fixture,
                            "models/detta-restricted-evaluator-proof-trace.json",
                        ),
                    ],
                ),
                (
                    "THM-004",
                    vec![(
                        RuntimeTest,
                        "detta_evaluator::tests::transfer_like_arguments_are_plain_data_not_authority",
                    )],
                ),
                (
                    "THM-005",
                    vec![(
                        RuntimeTest,
                        "detta_core::tests::transfer_from_requires_live_registry_allowance",
                    )],
                ),
                (
                    "THM-006",
                    vec![
                        (
                            RuntimeTest,
                            "detta_core::tests::allowance_consumption_reverts_with_failed_transfer",
                        ),
                        (Model, "models/DeTTaBlockExecution.tla::AtomicRevert"),
                    ],
                ),
                (
                    "THM-007",
                    vec![
                        (
                            RuntimeTest,
                            "detta_core::tests::insufficient_balance_reverts_storage_registry_and_events",
                        ),
                        (Model, "models/DeTTaBlockExecution.tla::AtomicRevert"),
                    ],
                ),
                (
                    "THM-008",
                    vec![
                        (
                            RuntimeTest,
                            "detta_core::tests::transfer_preserves_supply_and_commits_event",
                        ),
                        (
                            RuntimeTest,
                            "detta_core::tests::invariant_failure_reverts_full_transition",
                        ),
                    ],
                ),
                (
                    "THM-009",
                    vec![
                        (Verifier, "detta_verify::verify_differential_replay"),
                        (
                            RuntimeTest,
                            "detta_core::tests::deterministic_replay_produces_identical_roots",
                        ),
                        (
                            Fixture,
                            "models/detta-restricted-evaluator-resource-exhaustion.json",
                        ),
                    ],
                ),
                (
                    "THM-010",
                    vec![
                        (
                            RuntimeTest,
                            "detta_core::tests::replayed_nonce_is_rejected",
                        ),
                        (Model, "models/DeTTaBlockExecution.tla::ReplaySafety"),
                    ],
                ),
                (
                    "THM-011",
                    vec![(
                        RuntimeTest,
                        "detta_core::tests::caller_identity_cannot_be_forged",
                    )],
                ),
                (
                    "THM-012",
                    vec![
                        (
                            RuntimeTest,
                            "detta_core::tests::router_cross_contract_call_does_not_inherit_user_allowance",
                        ),
                        (Model, "models/DeTTaBlockExecution.tla::WriteScopeSafety"),
                    ],
                ),
                (
                    "THM-013",
                    vec![(RuntimeTest, "detta_core::tests::view_reads_are_read_only")],
                ),
                (
                    "THM-014",
                    vec![
                        (
                            RuntimeTest,
                            "detta_core::tests::arithmetic_overflow_reverts_without_state_or_events",
                        ),
                        (
                            Fixture,
                            "models/detta-restricted-evaluator-arithmetic-overflow.json",
                        ),
                        (Model, "models/DeTTaBlockExecution.tla::TypeOK"),
                    ],
                ),
                (
                    "THM-015",
                    vec![
                        (
                            RuntimeTest,
                            "detta_evaluator::tests::evaluator_rejects_forbidden_primitive",
                        ),
                        (
                            Fixture,
                            "models/detta-restricted-evaluator-forbidden-primitives.json",
                        ),
                    ],
                ),
            ])
        );
    }

    #[test]
    fn scs_theorem_evidence_kinds_cover_expected_set() {
        let coverage = scs_theorem_coverage();
        let evidence_kinds: BTreeSet<_> = coverage
            .iter()
            .flat_map(|entry| &entry.evidence)
            .map(|evidence| theorem_evidence_kind_name(&evidence.kind))
            .collect();

        assert_eq!(
            evidence_kinds,
            BTreeSet::from(["Fixture", "Model", "RuntimeTest", "Verifier"])
        );
        for entry in coverage {
            assert!(
                entry
                    .evidence
                    .iter()
                    .any(|evidence| matches!(evidence.kind, TheoremEvidenceKind::RuntimeTest)),
                "{} must retain at least one runtime test evidence anchor",
                entry.id
            );
        }
    }

    #[test]
    fn scs_theorem_evidence_entries_are_unique_per_theorem() {
        for entry in scs_theorem_coverage() {
            let mut evidence_entries = BTreeSet::new();
            for evidence in &entry.evidence {
                assert!(
                    evidence_entries.insert((
                        theorem_evidence_kind_name(&evidence.kind),
                        evidence.reference
                    )),
                    "{} repeats evidence {}:{}",
                    entry.id,
                    theorem_evidence_kind_name(&evidence.kind),
                    evidence.reference
                );
            }
        }
    }

    fn theorem_evidence_kind_name(kind: &TheoremEvidenceKind) -> &'static str {
        match kind {
            TheoremEvidenceKind::RuntimeTest => "RuntimeTest",
            TheoremEvidenceKind::Model => "Model",
            TheoremEvidenceKind::Verifier => "Verifier",
            TheoremEvidenceKind::Fixture => "Fixture",
        }
    }

    #[test]
    fn theorem_runtime_test_evidence_uses_expected_namespaces() {
        let allowed_prefixes = [
            "detta_core::tests::",
            "detta_evaluator::tests::",
            "detta_verify::tests::",
        ];

        for entry in scs_theorem_coverage() {
            for evidence in entry
                .evidence
                .iter()
                .filter(|evidence| matches!(evidence.kind, TheoremEvidenceKind::RuntimeTest))
            {
                assert!(
                    allowed_prefixes
                        .iter()
                        .any(|prefix| evidence.reference.starts_with(prefix)),
                    "{} runtime test evidence uses an unexpected namespace: {}",
                    entry.id,
                    evidence.reference
                );
            }
        }
    }

    #[test]
    fn theorem_runtime_test_evidence_covers_expected_crates() {
        let runtime_test_crates: BTreeSet<_> = scs_theorem_coverage()
            .into_iter()
            .flat_map(|entry| entry.evidence)
            .filter(|evidence| matches!(evidence.kind, TheoremEvidenceKind::RuntimeTest))
            .map(|evidence| evidence.reference.split_once("::").unwrap().0)
            .collect();

        assert_eq!(
            runtime_test_crates,
            BTreeSet::from(["detta_core", "detta_evaluator", "detta_verify"])
        );
    }

    #[test]
    fn theorem_verifier_evidence_uses_expected_namespace() {
        for entry in scs_theorem_coverage() {
            for evidence in entry
                .evidence
                .iter()
                .filter(|evidence| matches!(evidence.kind, TheoremEvidenceKind::Verifier))
            {
                assert!(
                    evidence.reference.starts_with("detta_verify::verify_"),
                    "{} verifier evidence uses an unexpected namespace: {}",
                    entry.id,
                    evidence.reference
                );
            }
        }
    }

    #[test]
    fn theorem_verifier_evidence_covers_expected_functions() {
        let verifier_functions: BTreeSet<_> = scs_theorem_coverage()
            .into_iter()
            .flat_map(|entry| entry.evidence)
            .filter(|evidence| matches!(evidence.kind, TheoremEvidenceKind::Verifier))
            .map(|evidence| evidence.reference)
            .collect();

        assert_eq!(
            verifier_functions,
            BTreeSet::from([
                "detta_verify::verify_kernel_trace",
                "detta_verify::verify_differential_replay",
            ])
        );
    }

    #[test]
    fn theorem_model_evidence_uses_expected_namespace() {
        for entry in scs_theorem_coverage() {
            for evidence in entry
                .evidence
                .iter()
                .filter(|evidence| matches!(evidence.kind, TheoremEvidenceKind::Model))
            {
                assert!(
                    evidence
                        .reference
                        .starts_with("models/DeTTaBlockExecution.tla::"),
                    "{} model evidence uses an unexpected namespace: {}",
                    entry.id,
                    evidence.reference
                );
                assert!(
                    !evidence.reference.rsplit_once("::").unwrap().1.is_empty(),
                    "{} model evidence must name a TLA+ operator",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn theorem_fixture_evidence_uses_expected_namespace() {
        for entry in scs_theorem_coverage() {
            for evidence in entry
                .evidence
                .iter()
                .filter(|evidence| matches!(evidence.kind, TheoremEvidenceKind::Fixture))
            {
                assert!(
                    evidence
                        .reference
                        .starts_with("models/detta-restricted-evaluator-"),
                    "{} fixture evidence uses an unexpected namespace: {}",
                    entry.id,
                    evidence.reference
                );
                assert!(
                    evidence.reference.ends_with(".json"),
                    "{} fixture evidence must reference JSON fixtures",
                    entry.id
                );
            }
        }
    }

    #[test]
    fn theorem_fixture_evidence_references_evaluator_fixture_inventory() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let inventory_fixture_paths: BTreeSet<_> = inventory
            .fixtures
            .iter()
            .map(|entry| entry.fixture_path.as_str())
            .collect();

        for entry in scs_theorem_coverage() {
            for evidence in entry
                .evidence
                .iter()
                .filter(|evidence| matches!(evidence.kind, TheoremEvidenceKind::Fixture))
            {
                assert!(
                    inventory_fixture_paths.contains(evidence.reference),
                    "{} fixture evidence is not listed in evaluator fixture inventory: {}",
                    entry.id,
                    evidence.reference
                );
            }
        }
    }

    #[test]
    fn theorem_fixture_evidence_covers_expected_fixture_schemas() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let inventory_fixture_schemas: BTreeMap<_, _> = inventory
            .fixtures
            .iter()
            .map(|entry| (entry.fixture_path.as_str(), entry.fixture_schema.as_str()))
            .collect();
        let theorem_fixture_schemas: BTreeSet<_> = scs_theorem_coverage()
            .into_iter()
            .flat_map(|entry| entry.evidence)
            .filter(|evidence| matches!(evidence.kind, TheoremEvidenceKind::Fixture))
            .map(|evidence| {
                inventory_fixture_schemas
                    .get(evidence.reference)
                    .copied()
                    .unwrap()
            })
            .collect();

        assert_eq!(
            theorem_fixture_schemas,
            BTreeSet::from([
                "detta.restricted-evaluator-proof-trace.v1",
                "detta.restricted-evaluator-forbidden-primitive.v1",
                "detta.restricted-evaluator-resource-exhaustion.v1",
                "detta.restricted-evaluator-arithmetic-overflow.v1",
            ])
        );
    }

    #[test]
    fn theorem_fixture_evidence_covers_expected_fixture_names() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let inventory_fixture_names: BTreeMap<_, _> = inventory
            .fixtures
            .iter()
            .map(|entry| (entry.fixture_path.as_str(), entry.name.as_str()))
            .collect();
        let theorem_fixture_names: BTreeSet<_> = scs_theorem_coverage()
            .into_iter()
            .flat_map(|entry| entry.evidence)
            .filter(|evidence| matches!(evidence.kind, TheoremEvidenceKind::Fixture))
            .map(|evidence| {
                inventory_fixture_names
                    .get(evidence.reference)
                    .copied()
                    .unwrap()
            })
            .collect();

        assert_eq!(
            theorem_fixture_names,
            BTreeSet::from([
                "proof-trace",
                "forbidden-primitives",
                "resource-exhaustion",
                "arithmetic-overflow",
            ])
        );
    }

    #[test]
    fn proof_artifact_manifest_matches_checked_in_json() {
        let expected: serde_json::Value = serde_json::from_str(include_str!(
            "../../../models/detta-proof-artifact-manifest.json"
        ))
        .unwrap();
        let actual = serde_json::to_value(proof_artifact_manifest()).unwrap();

        assert_eq!(actual, expected);
    }

    #[test]
    fn proof_artifact_manifest_json_fixture_is_stable() {
        let actual = format!(
            "{}\n",
            serde_json::to_string_pretty(&proof_artifact_manifest()).unwrap()
        );

        assert_eq!(
            actual,
            include_str!("../../../models/detta-proof-artifact-manifest.json")
        );
    }

    #[test]
    fn proof_artifact_manifest_schema_version_is_explicit() {
        let manifest = proof_artifact_manifest();

        assert_eq!(manifest.schema, PROOF_ARTIFACT_MANIFEST_SCHEMA);
        assert_eq!(
            manifest.schema_version,
            PROOF_ARTIFACT_MANIFEST_SCHEMA_VERSION
        );
        assert_eq!(PROOF_ARTIFACT_MANIFEST_SCHEMA_VERSION, 2);
    }

    #[test]
    fn proof_artifact_manifest_project_and_scope_are_explicit() {
        let manifest = proof_artifact_manifest();

        assert_eq!(manifest.project, PROOF_ARTIFACT_MANIFEST_PROJECT);
        assert_eq!(manifest.scope, PROOF_ARTIFACT_MANIFEST_SCOPE);
        assert_eq!(manifest.project, "DeTTa");
        assert_eq!(
            manifest.scope,
            "Secured Contract Spaces runtime safety obligations"
        );
    }

    #[test]
    fn proof_artifact_manifest_theorem_count_matches_coverage() {
        let manifest = proof_artifact_manifest();

        assert_eq!(manifest.theorem_count, manifest.coverage.len());
        assert_eq!(manifest.theorem_count, scs_theorem_coverage().len());
    }

    #[test]
    fn proof_artifact_manifest_artifact_counts_are_stable() {
        let manifest = proof_artifact_manifest();

        assert_eq!(manifest.model_artifacts.len(), PROOF_MODEL_ARTIFACT_COUNT);
        assert_eq!(
            manifest.runtime_artifacts.len(),
            PROOF_RUNTIME_ARTIFACT_COUNT
        );
        assert_eq!(
            manifest.model_artifacts.len(),
            proof_model_artifacts().len()
        );
        assert_eq!(
            manifest.runtime_artifacts.len(),
            proof_runtime_artifacts().len()
        );
    }

    #[test]
    fn proof_artifact_manifest_root_matches_checked_in_attestation() {
        let attestation = include_str!("../../../models/detta-proof-artifact-manifest.sha256");
        let (root, file_name) = attestation.trim().split_once("  ").unwrap();

        assert_eq!(file_name, "detta-proof-artifact-manifest.json");
        assert_eq!(checked_in_proof_artifact_manifest_root(), root);
    }

    #[test]
    fn proof_release_attestations_use_sha256sum_format() {
        assert_sha256sum_attestation_format(
            include_str!("../../../models/detta-proof-artifact-manifest.sha256"),
            "detta-proof-artifact-manifest.json",
        );
        assert_sha256sum_attestation_format(
            include_str!("../../../models/detta-restricted-evaluator-proof-trace.sha256"),
            "detta-restricted-evaluator-proof-trace.json",
        );
        assert_sha256sum_attestation_format(
            include_str!("../../../models/detta-restricted-evaluator-proof-trace-root.sha256"),
            "detta-restricted-evaluator-proof-trace.trace",
        );
        assert_sha256sum_attestation_format(
            include_str!("../../../models/detta-restricted-evaluator-forbidden-primitives.sha256"),
            "detta-restricted-evaluator-forbidden-primitives.json",
        );
        assert_sha256sum_attestation_format(
            include_str!("../../../models/detta-restricted-evaluator-resource-exhaustion.sha256"),
            "detta-restricted-evaluator-resource-exhaustion.json",
        );
        assert_sha256sum_attestation_format(
            include_str!("../../../models/detta-restricted-evaluator-arithmetic-overflow.sha256"),
            "detta-restricted-evaluator-arithmetic-overflow.json",
        );
        assert_sha256sum_attestation_format(
            include_str!("../../../models/detta-restricted-evaluator-fixture-inventory.sha256"),
            "detta-restricted-evaluator-fixture-inventory.json",
        );
    }

    #[test]
    fn proof_release_attestation_count_is_stable() {
        let attestations = [
            include_str!("../../../models/detta-proof-artifact-manifest.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-proof-trace.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-proof-trace-root.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-forbidden-primitives.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-resource-exhaustion.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-arithmetic-overflow.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-fixture-inventory.sha256"),
        ];

        assert_eq!(attestations.len(), PROOF_RELEASE_ATTESTATION_COUNT);
    }

    #[test]
    fn proof_release_attestation_filenames_are_unique() {
        let filenames: BTreeSet<_> = [
            include_str!("../../../models/detta-proof-artifact-manifest.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-proof-trace.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-proof-trace-root.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-forbidden-primitives.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-resource-exhaustion.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-arithmetic-overflow.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-fixture-inventory.sha256"),
        ]
        .into_iter()
        .map(|attestation| sha256sum_attestation_parts(attestation).1)
        .collect();

        assert_eq!(filenames.len(), PROOF_RELEASE_ATTESTATION_COUNT);
    }

    #[test]
    fn proof_release_attestation_roots_have_sha256_hex_length() {
        for attestation in [
            include_str!("../../../models/detta-proof-artifact-manifest.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-proof-trace.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-proof-trace-root.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-forbidden-primitives.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-resource-exhaustion.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-arithmetic-overflow.sha256"),
            include_str!("../../../models/detta-restricted-evaluator-fixture-inventory.sha256"),
        ] {
            let (root, file_name) = sha256sum_attestation_parts(attestation);

            assert_eq!(
                root.len(),
                SHA256_HEX_LENGTH,
                "{file_name} attestation root must be a SHA-256 hex digest"
            );
        }
    }

    #[test]
    fn proof_release_attestation_filenames_bind_target_artifacts() {
        assert_sha256sum_attestation_filename_binds_artifact(
            include_str!("../../../models/detta-proof-artifact-manifest.sha256"),
            "models/detta-proof-artifact-manifest.json",
        );
        assert_sha256sum_attestation_filename_binds_artifact(
            include_str!("../../../models/detta-restricted-evaluator-proof-trace.sha256"),
            "models/detta-restricted-evaluator-proof-trace.json",
        );
        assert_sha256sum_attestation_filename_binds_artifact(
            include_str!("../../../models/detta-restricted-evaluator-forbidden-primitives.sha256"),
            "models/detta-restricted-evaluator-forbidden-primitives.json",
        );
        assert_sha256sum_attestation_filename_binds_artifact(
            include_str!("../../../models/detta-restricted-evaluator-resource-exhaustion.sha256"),
            "models/detta-restricted-evaluator-resource-exhaustion.json",
        );
        assert_sha256sum_attestation_filename_binds_artifact(
            include_str!("../../../models/detta-restricted-evaluator-arithmetic-overflow.sha256"),
            "models/detta-restricted-evaluator-arithmetic-overflow.json",
        );
        assert_sha256sum_attestation_filename_binds_artifact(
            include_str!("../../../models/detta-restricted-evaluator-fixture-inventory.sha256"),
            "models/detta-restricted-evaluator-fixture-inventory.json",
        );
        assert_sha256sum_attestation_filename_matches(
            include_str!("../../../models/detta-restricted-evaluator-proof-trace-root.sha256"),
            "detta-restricted-evaluator-proof-trace.trace",
        );
    }

    #[test]
    fn proof_release_attestation_roots_match_target_bytes() {
        assert_sha256sum_attestation_root_matches_bytes(
            include_str!("../../../models/detta-proof-artifact-manifest.sha256"),
            include_bytes!("../../../models/detta-proof-artifact-manifest.json"),
        );
        assert_sha256sum_attestation_root_matches_bytes(
            include_str!("../../../models/detta-restricted-evaluator-proof-trace.sha256"),
            include_bytes!("../../../models/detta-restricted-evaluator-proof-trace.json"),
        );
        assert_sha256sum_attestation_root_matches_bytes(
            include_str!("../../../models/detta-restricted-evaluator-forbidden-primitives.sha256"),
            include_bytes!("../../../models/detta-restricted-evaluator-forbidden-primitives.json"),
        );
        assert_sha256sum_attestation_root_matches_bytes(
            include_str!("../../../models/detta-restricted-evaluator-resource-exhaustion.sha256"),
            include_bytes!("../../../models/detta-restricted-evaluator-resource-exhaustion.json"),
        );
        assert_sha256sum_attestation_root_matches_bytes(
            include_str!("../../../models/detta-restricted-evaluator-arithmetic-overflow.sha256"),
            include_bytes!("../../../models/detta-restricted-evaluator-arithmetic-overflow.json"),
        );
        assert_sha256sum_attestation_root_matches_bytes(
            include_str!("../../../models/detta-restricted-evaluator-fixture-inventory.sha256"),
            include_bytes!("../../../models/detta-restricted-evaluator-fixture-inventory.json"),
        );

        let fixture: ProofTraceFixtureForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-proof-trace.json"
        ))
        .unwrap();
        let trace_bytes = serde_json::to_vec(&fixture.report.trace).unwrap();
        assert_sha256sum_attestation_root_matches_bytes(
            include_str!("../../../models/detta-restricted-evaluator-proof-trace-root.sha256"),
            &trace_bytes,
        );
    }

    fn assert_sha256sum_attestation_format(attestation: &str, expected_file_name: &str) {
        assert!(attestation.ends_with('\n'));
        assert_eq!(attestation.matches('\n').count(), 1);

        let (root, file_name) = sha256sum_attestation_parts(attestation);

        assert_lowercase_sha256_hex(root, expected_file_name);
        assert_eq!(file_name, expected_file_name);
        assert!(!file_name.contains('/'));
        assert!(!file_name.contains('\\'));
    }

    fn assert_sha256sum_attestation_filename_binds_artifact(
        attestation: &str,
        target_artifact_path: &str,
    ) {
        let expected_file_name = target_artifact_path.rsplit('/').next().unwrap();

        assert_sha256sum_attestation_filename_matches(attestation, expected_file_name);
    }

    fn assert_sha256sum_attestation_filename_matches(attestation: &str, expected_file_name: &str) {
        let (_, file_name) = sha256sum_attestation_parts(attestation);

        assert_eq!(file_name, expected_file_name);
    }

    fn assert_sha256sum_attestation_root_matches_bytes(attestation: &str, target_bytes: &[u8]) {
        let (root, _) = sha256sum_attestation_parts(attestation);

        assert_eq!(root, proof_artifact_manifest_root_bytes(target_bytes));
    }

    fn sha256sum_attestation_parts(attestation: &str) -> (&str, &str) {
        attestation
            .strip_suffix('\n')
            .unwrap()
            .split_once("  ")
            .unwrap()
    }

    #[test]
    fn proof_model_artifacts_include_tla_roots() {
        assert_eq!(
            proof_model_artifacts(),
            vec![
                ModelArtifactRoot {
                    path: "models/DeTTaBlockExecution.tla",
                    sha256: "b1ad4c2b5bed6d8efd44451ef1a69665fe62ff234b8aa49ee8bfc0d10a8a6fc9"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/DeTTaBlockExecution.cfg",
                    sha256: "9149c3554fb6fb6971913c9ca7615c54289dbd04c9e612baae3b49bf203e6598"
                        .into(),
                },
            ]
        );
    }

    #[test]
    fn proof_model_artifact_order_is_stable() {
        let artifact_paths: Vec<_> = proof_model_artifacts()
            .iter()
            .map(|artifact| artifact.path)
            .collect();

        assert_eq!(
            artifact_paths,
            vec![
                "models/DeTTaBlockExecution.tla",
                "models/DeTTaBlockExecution.cfg",
            ]
        );
    }

    #[test]
    fn proof_runtime_artifacts_include_evaluator_fixture_roots() {
        assert_eq!(
            proof_runtime_artifacts(),
            vec![
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-proof-trace.json",
                    sha256: "513d919a2f2038b02519512b5416b35bdddf5158cb8dadb792c7b6ed61147a38"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-proof-trace.sha256",
                    sha256: "6e94330e7034919bd12ad6b03b4acf785dc923ae32dca9b110eb024541a8926b"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-proof-trace-root.sha256",
                    sha256: "962954607a2b57634bebd9f634f04d0e4206d35c948257398c198d3fe8a7febc"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-forbidden-primitives.json",
                    sha256: "8da9bb850bf189c0ce69753717a47c40173636f564710dcab288d8ee1ac2622c"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-forbidden-primitives.sha256",
                    sha256: "b5f1a047ea11a03a17a574a3835e8b63bf681f3ee4c655462f9e448c97865648"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-resource-exhaustion.json",
                    sha256: "ba6a1a61581be50de96532e86e3f3e2c9c8792a9506c4149a58fe2a7be9e1806"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-resource-exhaustion.sha256",
                    sha256: "0b0f0f62127febf4108114beeb10ba383adcf2582cbb5f263038cba960f8a0d0"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-arithmetic-overflow.json",
                    sha256: "8135d0a60e36e5369d80ca85985311745b3f3172bbc84a639cf3ec4cef602f2f"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-arithmetic-overflow.sha256",
                    sha256: "d8f9f4c782699c7e56fe852968f82e54ddb0a1fe701add00dd986c300a2cb11d"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-fixture-inventory.json",
                    sha256: "d50ccc0023d9bf691de61c2f4db609843645967a37e8df4e7c97097268f3e18f"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-fixture-inventory.sha256",
                    sha256: "40660cbc15f9d9b4a5b3081a0f9aa2c2a390b2043c26fef041333701b0da1a66"
                        .into(),
                },
            ]
        );
    }

    #[test]
    fn proof_runtime_artifact_order_is_stable() {
        let artifact_paths: Vec<_> = proof_runtime_artifacts()
            .iter()
            .map(|artifact| artifact.path)
            .collect();

        assert_eq!(
            artifact_paths,
            vec![
                "models/detta-restricted-evaluator-proof-trace.json",
                "models/detta-restricted-evaluator-proof-trace.sha256",
                "models/detta-restricted-evaluator-proof-trace-root.sha256",
                "models/detta-restricted-evaluator-forbidden-primitives.json",
                "models/detta-restricted-evaluator-forbidden-primitives.sha256",
                "models/detta-restricted-evaluator-resource-exhaustion.json",
                "models/detta-restricted-evaluator-resource-exhaustion.sha256",
                "models/detta-restricted-evaluator-arithmetic-overflow.json",
                "models/detta-restricted-evaluator-arithmetic-overflow.sha256",
                "models/detta-restricted-evaluator-fixture-inventory.json",
                "models/detta-restricted-evaluator-fixture-inventory.sha256",
            ]
        );
    }

    #[test]
    fn proof_runtime_artifact_paths_are_unique() {
        let artifacts = proof_runtime_artifacts();
        let paths: BTreeSet<_> = artifacts.iter().map(|artifact| artifact.path).collect();

        assert_eq!(paths.len(), artifacts.len());
    }

    #[test]
    fn proof_model_artifact_paths_are_unique() {
        let artifacts = proof_model_artifacts();
        let paths: BTreeSet<_> = artifacts.iter().map(|artifact| artifact.path).collect();

        assert_eq!(paths.len(), artifacts.len());
    }

    #[test]
    fn proof_artifact_roots_are_lowercase_sha256_hex() {
        for artifact in proof_model_artifacts()
            .into_iter()
            .chain(proof_runtime_artifacts())
        {
            assert_lowercase_sha256_hex(&artifact.sha256, artifact.path);
        }
    }

    #[test]
    fn proof_artifact_roots_have_sha256_hex_length() {
        for artifact in proof_model_artifacts()
            .into_iter()
            .chain(proof_runtime_artifacts())
        {
            assert_eq!(
                artifact.sha256.len(),
                SHA256_HEX_LENGTH,
                "{} root must be a SHA-256 hex digest",
                artifact.path
            );
        }
    }

    fn assert_lowercase_sha256_hex(root: &str, path: &str) {
        assert_eq!(
            root.len(),
            SHA256_HEX_LENGTH,
            "{path} root is not a SHA-256 hex digest"
        );
        assert!(
            root.bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "{path} root must use lowercase hex"
        );
    }

    #[test]
    fn proof_artifact_paths_are_models_namespace_relative() {
        for artifact in proof_model_artifacts()
            .into_iter()
            .chain(proof_runtime_artifacts())
        {
            assert!(
                artifact.path.starts_with("models/"),
                "{} must stay under the models/ namespace",
                artifact.path
            );
            assert!(
                !artifact.path.starts_with('/'),
                "{} must not be absolute",
                artifact.path
            );
            assert!(
                !artifact.path.contains(".."),
                "{} must not contain parent traversal",
                artifact.path
            );
            assert!(
                !artifact.path.contains('\\'),
                "{} must use forward slash separators",
                artifact.path
            );
            assert!(
                !artifact.path.contains("//"),
                "{} must not contain empty path segments",
                artifact.path
            );
        }
    }

    #[test]
    fn proof_artifact_paths_use_allowed_extensions() {
        let allowed_suffixes = [".tla", ".cfg", ".json", ".sha256"];

        for artifact in proof_model_artifacts()
            .into_iter()
            .chain(proof_runtime_artifacts())
        {
            assert!(
                allowed_suffixes
                    .iter()
                    .any(|suffix| artifact.path.ends_with(suffix)),
                "{} must use an allowed proof artifact suffix",
                artifact.path
            );
        }
    }

    #[test]
    fn proof_model_and_runtime_artifact_paths_are_disjoint() {
        let model_paths: BTreeSet<_> = proof_model_artifacts()
            .into_iter()
            .map(|artifact| artifact.path)
            .collect();

        for artifact in proof_runtime_artifacts() {
            assert!(
                !model_paths.contains(artifact.path),
                "{} must not be listed as both model and runtime artifact",
                artifact.path
            );
        }
    }

    #[test]
    fn proof_runtime_artifacts_bind_all_evaluator_attestations() {
        let runtime_paths: BTreeSet<_> = proof_runtime_artifacts()
            .into_iter()
            .map(|artifact| artifact.path)
            .collect();

        for required in [
            "models/detta-restricted-evaluator-proof-trace.sha256",
            "models/detta-restricted-evaluator-proof-trace-root.sha256",
            "models/detta-restricted-evaluator-forbidden-primitives.sha256",
            "models/detta-restricted-evaluator-resource-exhaustion.sha256",
            "models/detta-restricted-evaluator-arithmetic-overflow.sha256",
            "models/detta-restricted-evaluator-fixture-inventory.sha256",
        ] {
            assert!(
                runtime_paths.contains(required),
                "{required} is not bound in the proof manifest runtime artifacts"
            );
        }
    }

    #[test]
    fn proof_runtime_evaluator_attestations_are_covered_by_inventory() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let mut inventory_attestation_paths = BTreeSet::new();
        for entry in inventory.fixtures {
            inventory_attestation_paths.insert(entry.attestation_path);
            if let Some(path) = entry.trace_root_attestation_path {
                inventory_attestation_paths.insert(path);
            }
        }

        for artifact in proof_runtime_artifacts().into_iter().filter(|artifact| {
            artifact
                .path
                .starts_with("models/detta-restricted-evaluator-")
                && artifact.path.ends_with(".sha256")
        }) {
            assert!(
                artifact.path == "models/detta-restricted-evaluator-fixture-inventory.sha256"
                    || inventory_attestation_paths.contains(artifact.path),
                "{} is not covered by the evaluator fixture inventory",
                artifact.path
            );
        }
    }

    #[test]
    fn proof_runtime_evaluator_fixture_json_are_covered_by_inventory() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let inventory_fixture_paths: BTreeSet<_> = inventory
            .fixtures
            .into_iter()
            .map(|entry| entry.fixture_path)
            .collect();

        for artifact in proof_runtime_artifacts().into_iter().filter(|artifact| {
            artifact
                .path
                .starts_with("models/detta-restricted-evaluator-")
                && artifact.path.ends_with(".json")
        }) {
            assert!(
                artifact.path == "models/detta-restricted-evaluator-fixture-inventory.json"
                    || inventory_fixture_paths.contains(artifact.path),
                "{} is not covered by the evaluator fixture inventory",
                artifact.path
            );
        }
    }

    #[test]
    fn proof_runtime_artifacts_bind_all_evaluator_fixture_json() {
        let runtime_paths: BTreeSet<_> = proof_runtime_artifacts()
            .into_iter()
            .map(|artifact| artifact.path)
            .collect();

        for required in [
            "models/detta-restricted-evaluator-proof-trace.json",
            "models/detta-restricted-evaluator-forbidden-primitives.json",
            "models/detta-restricted-evaluator-resource-exhaustion.json",
            "models/detta-restricted-evaluator-arithmetic-overflow.json",
            "models/detta-restricted-evaluator-fixture-inventory.json",
        ] {
            assert!(
                runtime_paths.contains(required),
                "{required} is not bound in the proof manifest runtime artifacts"
            );
        }
    }

    #[test]
    fn proof_runtime_evaluator_attestations_match_fixture_roots() {
        let runtime_roots: BTreeMap<_, _> = proof_runtime_artifacts()
            .into_iter()
            .map(|artifact| (artifact.path, artifact.sha256))
            .collect();

        assert_fixture_attestation_matches_runtime_root(
            include_str!("../../../models/detta-restricted-evaluator-proof-trace.sha256"),
            "detta-restricted-evaluator-proof-trace.json",
            "models/detta-restricted-evaluator-proof-trace.json",
            &runtime_roots,
        );
        assert_fixture_attestation_matches_runtime_root(
            include_str!("../../../models/detta-restricted-evaluator-forbidden-primitives.sha256"),
            "detta-restricted-evaluator-forbidden-primitives.json",
            "models/detta-restricted-evaluator-forbidden-primitives.json",
            &runtime_roots,
        );
        assert_fixture_attestation_matches_runtime_root(
            include_str!("../../../models/detta-restricted-evaluator-resource-exhaustion.sha256"),
            "detta-restricted-evaluator-resource-exhaustion.json",
            "models/detta-restricted-evaluator-resource-exhaustion.json",
            &runtime_roots,
        );
        assert_fixture_attestation_matches_runtime_root(
            include_str!("../../../models/detta-restricted-evaluator-arithmetic-overflow.sha256"),
            "detta-restricted-evaluator-arithmetic-overflow.json",
            "models/detta-restricted-evaluator-arithmetic-overflow.json",
            &runtime_roots,
        );
        assert_fixture_attestation_matches_runtime_root(
            include_str!("../../../models/detta-restricted-evaluator-fixture-inventory.sha256"),
            "detta-restricted-evaluator-fixture-inventory.json",
            "models/detta-restricted-evaluator-fixture-inventory.json",
            &runtime_roots,
        );
    }

    fn assert_fixture_attestation_matches_runtime_root(
        attestation: &str,
        expected_file_name: &str,
        runtime_artifact_path: &str,
        runtime_roots: &BTreeMap<&'static str, String>,
    ) {
        let (root, file_name) = attestation.trim().split_once("  ").unwrap();

        assert_eq!(file_name, expected_file_name);
        assert_eq!(
            runtime_roots.get(runtime_artifact_path).map(String::as_str),
            Some(root)
        );
    }

    #[derive(serde::Deserialize)]
    struct EvaluatorFixtureInventoryForTest {
        schema: String,
        schema_version: u32,
        evaluator: String,
        fixtures: Vec<EvaluatorFixtureInventoryEntryForTest>,
    }

    #[derive(serde::Deserialize)]
    struct EvaluatorFixtureInventoryEntryForTest {
        name: String,
        fixture_schema: String,
        fixture_path: String,
        fixture_sha256: String,
        attestation_path: String,
        attestation_sha256: String,
        trace_root: Option<String>,
        trace_root_attestation_path: Option<String>,
        trace_root_attestation_sha256: Option<String>,
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_schema_is_explicit() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();

        assert_eq!(
            inventory.schema,
            "detta.restricted-evaluator-fixture-inventory.v1"
        );
        assert_eq!(inventory.schema_version, 1);
        assert_eq!(inventory.evaluator, "detta.restricted-script-evaluator");
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_entry_names_are_unique() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let names: BTreeSet<_> = inventory
            .fixtures
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();

        assert_eq!(names.len(), inventory.fixtures.len());
        for entry in inventory.fixtures {
            assert!(!entry.name.is_empty());
        }
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_schemas_are_unique() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let fixture_schemas: BTreeSet<_> = inventory
            .fixtures
            .iter()
            .map(|entry| entry.fixture_schema.as_str())
            .collect();

        assert_eq!(fixture_schemas.len(), inventory.fixtures.len());
        for entry in inventory.fixtures {
            assert!(entry
                .fixture_schema
                .starts_with("detta.restricted-evaluator-"));
        }
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_schemas_cover_expected_set() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let fixture_schemas: BTreeSet<_> = inventory
            .fixtures
            .iter()
            .map(|entry| entry.fixture_schema.as_str())
            .collect();

        assert_eq!(
            fixture_schemas,
            BTreeSet::from([
                "detta.restricted-evaluator-proof-trace.v1",
                "detta.restricted-evaluator-forbidden-primitive.v1",
                "detta.restricted-evaluator-resource-exhaustion.v1",
                "detta.restricted-evaluator-arithmetic-overflow.v1",
            ])
        );
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_names_map_to_expected_schemas() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let name_to_schema: BTreeMap<_, _> = inventory
            .fixtures
            .iter()
            .map(|entry| (entry.name.as_str(), entry.fixture_schema.as_str()))
            .collect();

        assert_eq!(
            name_to_schema,
            BTreeMap::from([
                ("proof-trace", "detta.restricted-evaluator-proof-trace.v1"),
                (
                    "forbidden-primitives",
                    "detta.restricted-evaluator-forbidden-primitive.v1"
                ),
                (
                    "resource-exhaustion",
                    "detta.restricted-evaluator-resource-exhaustion.v1"
                ),
                (
                    "arithmetic-overflow",
                    "detta.restricted-evaluator-arithmetic-overflow.v1"
                ),
            ])
        );
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_names_map_to_expected_paths() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let name_to_paths: BTreeMap<_, _> = inventory
            .fixtures
            .iter()
            .map(|entry| {
                (
                    entry.name.as_str(),
                    (
                        entry.fixture_path.as_str(),
                        entry.attestation_path.as_str(),
                        entry.trace_root_attestation_path.as_deref(),
                    ),
                )
            })
            .collect();

        assert_eq!(
            name_to_paths,
            BTreeMap::from([
                (
                    "proof-trace",
                    (
                        "models/detta-restricted-evaluator-proof-trace.json",
                        "models/detta-restricted-evaluator-proof-trace.sha256",
                        Some("models/detta-restricted-evaluator-proof-trace-root.sha256"),
                    ),
                ),
                (
                    "forbidden-primitives",
                    (
                        "models/detta-restricted-evaluator-forbidden-primitives.json",
                        "models/detta-restricted-evaluator-forbidden-primitives.sha256",
                        None,
                    ),
                ),
                (
                    "resource-exhaustion",
                    (
                        "models/detta-restricted-evaluator-resource-exhaustion.json",
                        "models/detta-restricted-evaluator-resource-exhaustion.sha256",
                        None,
                    ),
                ),
                (
                    "arithmetic-overflow",
                    (
                        "models/detta-restricted-evaluator-arithmetic-overflow.json",
                        "models/detta-restricted-evaluator-arithmetic-overflow.sha256",
                        None,
                    ),
                ),
            ])
        );
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_names_map_to_expected_roots() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let name_to_roots: BTreeMap<_, _> = inventory
            .fixtures
            .iter()
            .map(|entry| {
                (
                    entry.name.as_str(),
                    (
                        entry.fixture_sha256.as_str(),
                        entry.attestation_sha256.as_str(),
                        entry.trace_root.as_deref(),
                        entry.trace_root_attestation_sha256.as_deref(),
                    ),
                )
            })
            .collect();

        assert_eq!(
            name_to_roots,
            BTreeMap::from([
                (
                    "proof-trace",
                    (
                        "513d919a2f2038b02519512b5416b35bdddf5158cb8dadb792c7b6ed61147a38",
                        "6e94330e7034919bd12ad6b03b4acf785dc923ae32dca9b110eb024541a8926b",
                        Some("a90c256f6a8f5f16dcf5bc3cc063aca87f8cfaff4c17196f0d9fcebac70c4947",),
                        Some("962954607a2b57634bebd9f634f04d0e4206d35c948257398c198d3fe8a7febc",),
                    ),
                ),
                (
                    "forbidden-primitives",
                    (
                        "8da9bb850bf189c0ce69753717a47c40173636f564710dcab288d8ee1ac2622c",
                        "b5f1a047ea11a03a17a574a3835e8b63bf681f3ee4c655462f9e448c97865648",
                        None,
                        None,
                    ),
                ),
                (
                    "resource-exhaustion",
                    (
                        "ba6a1a61581be50de96532e86e3f3e2c9c8792a9506c4149a58fe2a7be9e1806",
                        "0b0f0f62127febf4108114beeb10ba383adcf2582cbb5f263038cba960f8a0d0",
                        None,
                        None,
                    ),
                ),
                (
                    "arithmetic-overflow",
                    (
                        "8135d0a60e36e5369d80ca85985311745b3f3172bbc84a639cf3ec4cef602f2f",
                        "d8f9f4c782699c7e56fe852968f82e54ddb0a1fe701add00dd986c300a2cb11d",
                        None,
                        None,
                    ),
                ),
            ])
        );
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_paths_are_unique() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let mut paths = BTreeSet::new();

        for entry in &inventory.fixtures {
            assert!(
                paths.insert(entry.fixture_path.as_str()),
                "{} is duplicated in the evaluator fixture inventory",
                entry.fixture_path
            );
            assert!(
                paths.insert(entry.attestation_path.as_str()),
                "{} is duplicated in the evaluator fixture inventory",
                entry.attestation_path
            );
            if let Some(path) = &entry.trace_root_attestation_path {
                assert!(
                    paths.insert(path.as_str()),
                    "{path} is duplicated in the evaluator fixture inventory"
                );
            }
        }
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_paths_use_models_namespace_and_expected_suffixes()
    {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();

        for entry in &inventory.fixtures {
            assert_inventory_path_is_models_relative(&entry.fixture_path);
            assert!(
                entry.fixture_path.ends_with(".json"),
                "{} must point at a JSON evaluator fixture",
                entry.fixture_path
            );

            assert_inventory_path_is_models_relative(&entry.attestation_path);
            assert!(
                entry.attestation_path.ends_with(".sha256"),
                "{} must point at a SHA-256 attestation",
                entry.attestation_path
            );

            if let Some(path) = &entry.trace_root_attestation_path {
                assert_inventory_path_is_models_relative(path);
                assert!(
                    path.ends_with(".sha256"),
                    "{path} must point at a SHA-256 trace-root attestation"
                );
            }
        }
    }

    fn assert_inventory_path_is_models_relative(path: &str) {
        assert!(
            path.starts_with("models/"),
            "{path} must stay under models/"
        );
        assert!(!path.starts_with('/'), "{path} must not be absolute");
        assert!(!path.contains(".."), "{path} must not use parent traversal");
        assert!(
            !path.contains('\\'),
            "{path} must use forward slash separators"
        );
        assert!(
            !path.contains("//"),
            "{path} must not contain empty path segments"
        );
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_attestation_paths_bind_target_filenames() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();

        for entry in &inventory.fixtures {
            let fixture_stem = entry.fixture_path.strip_suffix(".json").unwrap();
            assert_eq!(
                entry.attestation_path,
                format!("{fixture_stem}.sha256"),
                "{} must attest {} by basename",
                entry.attestation_path,
                entry.fixture_path
            );

            if let Some(path) = &entry.trace_root_attestation_path {
                assert_eq!(
                    path,
                    &format!("{fixture_stem}-root.sha256"),
                    "{path} must be the trace-root attestation for {}",
                    entry.fixture_path
                );
            }
        }
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_trace_root_metadata_is_proof_trace_only() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let trace_root_entries: BTreeSet<_> = inventory
            .fixtures
            .iter()
            .filter(|entry| {
                entry.trace_root.is_some()
                    || entry.trace_root_attestation_path.is_some()
                    || entry.trace_root_attestation_sha256.is_some()
            })
            .map(|entry| entry.name.as_str())
            .collect();

        assert_eq!(trace_root_entries, BTreeSet::from(["proof-trace"]));
        for entry in &inventory.fixtures {
            let has_trace_root = entry.trace_root.is_some();
            assert_eq!(
                has_trace_root,
                entry.trace_root_attestation_path.is_some(),
                "{} must keep trace root and trace-root attestation path together",
                entry.name
            );
            assert_eq!(
                has_trace_root,
                entry.trace_root_attestation_sha256.is_some(),
                "{} must keep trace root and trace-root attestation root together",
                entry.name
            );
        }
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_order_is_stable() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let fixture_order: Vec<_> = inventory
            .fixtures
            .iter()
            .map(|entry| entry.name.as_str())
            .collect();

        assert_eq!(
            fixture_order,
            vec![
                "proof-trace",
                "forbidden-primitives",
                "resource-exhaustion",
                "arithmetic-overflow",
            ]
        );
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_roots_are_lowercase_sha256_hex() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();

        for entry in inventory.fixtures {
            assert_lowercase_sha256_hex(&entry.fixture_sha256, &entry.fixture_path);
            assert_lowercase_sha256_hex(&entry.attestation_sha256, &entry.attestation_path);
            if let Some(root) = &entry.trace_root {
                assert_lowercase_sha256_hex(root, &entry.name);
            }
            if let (Some(path), Some(root)) = (
                &entry.trace_root_attestation_path,
                &entry.trace_root_attestation_sha256,
            ) {
                assert_lowercase_sha256_hex(root, path);
            }
        }
    }

    #[test]
    fn proof_runtime_artifacts_match_evaluator_fixture_inventory_entries() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let runtime_roots: BTreeMap<_, _> = proof_runtime_artifacts()
            .into_iter()
            .map(|artifact| (artifact.path, artifact.sha256))
            .collect();

        assert_eq!(inventory.fixtures.len(), 4);
        for entry in inventory.fixtures {
            assert_eq!(
                runtime_roots
                    .get(entry.fixture_path.as_str())
                    .map(String::as_str),
                Some(entry.fixture_sha256.as_str()),
                "{} fixture root is not bound in proof runtime artifacts",
                entry.fixture_path
            );
            assert_eq!(
                runtime_roots
                    .get(entry.attestation_path.as_str())
                    .map(String::as_str),
                Some(entry.attestation_sha256.as_str()),
                "{} attestation root is not bound in proof runtime artifacts",
                entry.attestation_path
            );

            match (
                entry.trace_root_attestation_path,
                entry.trace_root_attestation_sha256,
            ) {
                (Some(path), Some(root)) => assert_eq!(
                    runtime_roots.get(path.as_str()).map(String::as_str),
                    Some(root.as_str()),
                    "{path} trace-root attestation is not bound in proof runtime artifacts"
                ),
                (None, None) => {}
                _ => panic!("trace-root attestation path and root must both be present or absent"),
            }
        }
    }

    #[derive(serde::Deserialize)]
    struct ProofTraceFixtureForTest {
        trace_root: String,
        report: ProofTraceReportForTest,
    }

    #[derive(serde::Deserialize)]
    struct ProofTraceReportForTest {
        trace: Vec<TraceOp>,
    }

    #[test]
    fn proof_runtime_trace_root_attestation_matches_fixture_trace() {
        let fixture: ProofTraceFixtureForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-proof-trace.json"
        ))
        .unwrap();
        let trace_bytes = serde_json::to_vec(&fixture.report.trace).unwrap();
        let trace_root = proof_artifact_manifest_root_bytes(&trace_bytes);
        let attestation =
            include_str!("../../../models/detta-restricted-evaluator-proof-trace-root.sha256");
        let (attested_root, file_name) = attestation.trim().split_once("  ").unwrap();

        assert_eq!(file_name, "detta-restricted-evaluator-proof-trace.trace");
        assert_eq!(fixture.trace_root, trace_root);
        assert_eq!(attested_root, trace_root);
    }

    #[test]
    fn proof_runtime_evaluator_fixture_inventory_trace_root_matches_fixture() {
        let inventory: EvaluatorFixtureInventoryForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-fixture-inventory.json"
        ))
        .unwrap();
        let trace_entries = inventory
            .fixtures
            .iter()
            .filter(|entry| entry.trace_root.is_some())
            .collect::<Vec<_>>();
        let fixture: ProofTraceFixtureForTest = serde_json::from_str(include_str!(
            "../../../models/detta-restricted-evaluator-proof-trace.json"
        ))
        .unwrap();
        let trace_bytes = serde_json::to_vec(&fixture.report.trace).unwrap();
        let trace_root = proof_artifact_manifest_root_bytes(&trace_bytes);
        let attestation =
            include_str!("../../../models/detta-restricted-evaluator-proof-trace-root.sha256");
        let (attested_root, file_name) = sha256sum_attestation_parts(attestation);

        assert_eq!(trace_entries.len(), 1);
        let entry = trace_entries.first().unwrap();
        let inventory_trace_root = entry.trace_root.as_deref().unwrap();

        assert_eq!(entry.name.as_str(), "proof-trace");
        assert_eq!(
            entry.trace_root_attestation_path.as_deref(),
            Some("models/detta-restricted-evaluator-proof-trace-root.sha256")
        );
        assert_eq!(file_name, "detta-restricted-evaluator-proof-trace.trace");
        assert_eq!(inventory_trace_root, fixture.trace_root);
        assert_eq!(inventory_trace_root, trace_root);
        assert_eq!(inventory_trace_root, attested_root);
    }

    #[test]
    fn theorem_fixture_evidence_references_runtime_artifacts() {
        let runtime_artifacts: BTreeSet<_> = proof_runtime_artifacts()
            .into_iter()
            .map(|artifact| artifact.path)
            .collect();
        let fixture_references: BTreeSet<_> = scs_theorem_coverage()
            .into_iter()
            .flat_map(|theorem| theorem.evidence)
            .filter(|evidence| evidence.kind == TheoremEvidenceKind::Fixture)
            .map(|evidence| evidence.reference)
            .collect();

        assert_eq!(
            fixture_references,
            BTreeSet::from([
                "models/detta-restricted-evaluator-proof-trace.json",
                "models/detta-restricted-evaluator-forbidden-primitives.json",
                "models/detta-restricted-evaluator-resource-exhaustion.json",
                "models/detta-restricted-evaluator-arithmetic-overflow.json",
            ])
        );
        for reference in fixture_references {
            assert!(
                runtime_artifacts.contains(reference),
                "{reference} is not bound as a runtime artifact"
            );
        }
    }

    #[test]
    fn theorem_model_evidence_references_existing_tla_operators() {
        let operators = tla_operator_names(include_str!("../../../models/DeTTaBlockExecution.tla"));

        for theorem in scs_theorem_coverage() {
            for evidence in theorem.evidence {
                if let Some(operator) = evidence
                    .reference
                    .strip_prefix("models/DeTTaBlockExecution.tla::")
                {
                    assert!(
                        operators.contains(operator),
                        "{} references missing TLA operator {operator}",
                        theorem.id
                    );
                }
            }
        }
    }

    #[test]
    fn theorem_model_evidence_covers_expected_tla_operators() {
        let evidence_operators: BTreeSet<_> = scs_theorem_coverage()
            .into_iter()
            .flat_map(|theorem| theorem.evidence)
            .filter_map(|evidence| {
                evidence
                    .reference
                    .strip_prefix("models/DeTTaBlockExecution.tla::")
            })
            .collect();

        assert_eq!(
            evidence_operators,
            BTreeSet::from([
                "DispatcherOnlyMutation",
                "AtomicRevert",
                "ReplaySafety",
                "WriteScopeSafety",
                "TypeOK",
            ])
        );
    }

    fn tla_operator_names(source: &str) -> BTreeSet<String> {
        source
            .lines()
            .filter_map(|line| {
                let line = line.trim_start();
                if line.starts_with("\\*") {
                    return None;
                }
                let (candidate, _) = line.split_once("==")?;
                let name = candidate
                    .trim()
                    .split(|character: char| character == '(' || character.is_whitespace())
                    .next()?;
                if name.is_empty()
                    || !name
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric() || character == '_')
                {
                    return None;
                }
                Some(name.to_string())
            })
            .collect()
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

    #[test]
    fn differential_replay_accepts_generated_transfer_corpus() {
        let state = seeded_state();
        let transactions = deterministic_transfer_corpus(
            "detta-local".into(),
            "TokenA".into(),
            "USDC".into(),
            7,
            32,
        );

        let block = verify_differential_replay(&state, transactions, 1, 4).unwrap();

        assert_eq!(block.transactions.len(), 32);
        assert_eq!(block.header.height, 1);
    }

    #[test]
    fn differential_replay_requires_multiple_replicas() {
        let state = seeded_state();

        assert_eq!(
            verify_differential_replay(&state, Vec::new(), 1, 1),
            Err(DifferentialReplayError::NotEnoughReplicas)
        );
    }
}
