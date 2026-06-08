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
        project: "DeTTa",
        scope: "Secured Contract Spaces runtime safety obligations",
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
            path: "models/detta-restricted-evaluator-forbidden-primitives.json",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-forbidden-primitives.json"
            )),
        },
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-resource-exhaustion.json",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-resource-exhaustion.json"
            )),
        },
        ModelArtifactRoot {
            path: "models/detta-restricted-evaluator-arithmetic-overflow.json",
            sha256: proof_artifact_manifest_root_bytes(include_bytes!(
                "../../../models/detta-restricted-evaluator-arithmetic-overflow.json"
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
    fn proof_artifact_manifest_root_matches_checked_in_attestation() {
        let attestation = include_str!("../../../models/detta-proof-artifact-manifest.sha256");
        let (root, file_name) = attestation.trim().split_once("  ").unwrap();

        assert_eq!(file_name, "detta-proof-artifact-manifest.json");
        assert_eq!(checked_in_proof_artifact_manifest_root(), root);
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
                    path: "models/detta-restricted-evaluator-forbidden-primitives.json",
                    sha256: "8da9bb850bf189c0ce69753717a47c40173636f564710dcab288d8ee1ac2622c"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-resource-exhaustion.json",
                    sha256: "ba6a1a61581be50de96532e86e3f3e2c9c8792a9506c4149a58fe2a7be9e1806"
                        .into(),
                },
                ModelArtifactRoot {
                    path: "models/detta-restricted-evaluator-arithmetic-overflow.json",
                    sha256: "8135d0a60e36e5369d80ca85985311745b3f3172bbc84a639cf3ec4cef602f2f"
                        .into(),
                },
            ]
        );
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
