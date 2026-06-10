#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

tmp_operator_artifacts="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_operator_artifacts"
}
trap cleanup EXIT

run_e2e_gate() {
  case "${DETTA_E2E_FULL:-0}" in
    1 | true | TRUE | yes | YES)
      cargo test -p detta-e2e -- --test-threads=1
      ;;
    *)
      cargo test -p detta-e2e --test aspect_module_client_flows -- --test-threads=1
      cargo test -p detta-e2e --test aspect_amm_client_flows -- --test-threads=1
      cargo test -p detta-e2e --test full_client_flows -- --test-threads=1
      cargo test -p detta-e2e --test defi_method_edge_flows -- --test-threads=1
      cargo test -p detta-e2e --test factory_client_flows -- --test-threads=1
      cargo test -p detta-e2e --test mempool_network_flows -- --test-threads=1
      cargo test -p detta-e2e --test operator_observability_client_flows -- --test-threads=1
      cargo test -p detta-e2e --test rpc_method_coverage_flows -- --test-threads=1
      cargo test -p detta-e2e --test signed_transaction_client_flows -- --test-threads=1
      cargo test -p detta-e2e --test tcp_protocol_hardening_flows -- --test-threads=1
      ;;
  esac
}

run_packaged_operator_gate() {
  DETTA_RELEASE_VERSION=release-gate-launch \
    DETTA_RELEASE_OUT="$tmp_operator_artifacts" \
    scripts/detta-operator-launch-rehearsal.sh
  DETTA_RELEASE_VERSION=release-gate-incident \
    DETTA_RELEASE_OUT="$tmp_operator_artifacts" \
    scripts/detta-incident-response-drill.sh
  DETTA_RELEASE_VERSION=release-gate-governance \
    DETTA_RELEASE_OUT="$tmp_operator_artifacts" \
    scripts/detta-governance-bootstrap-drill.sh
}

cargo fmt --check
cargo clippy --all-targets -- -D warnings
scripts/detta-dependency-audit.sh
cargo test --workspace --exclude detta-e2e
run_e2e_gate
cargo test -p detta-verify
cargo test -p detta-verify tests::differential_replay_accepts_generated_transfer_corpus -- --exact
cargo test -p detta-verify tests::differential_replay_accepts_generated_defi_corpus -- --exact
cargo build --locked --release
run_packaged_operator_gate
scripts/detta-model-check.sh

(
  cd models
  sha256sum -c detta-proof-artifact-manifest.sha256
  sha256sum -c detta-restricted-evaluator-proof-trace.sha256
  sha256sum -c detta-restricted-evaluator-forbidden-primitives.sha256
  sha256sum -c detta-restricted-evaluator-resource-exhaustion.sha256
  sha256sum -c detta-restricted-evaluator-arithmetic-overflow.sha256
  sha256sum -c detta-restricted-evaluator-fixture-inventory.sha256
  sha256sum DeTTaBlockExecution.tla
  sha256sum DeTTaBlockExecution.cfg
  sha256sum detta-restricted-evaluator-proof-trace-root.sha256
)

(
  cd models/aspects/stdlib
  sha256sum -c minimal-transfer-token.metta.sha256
  sha256sum -c minimal-transfer-token.artifact.sha256
  sha256sum -c minimal-transfer-token.proof-obligations.sha256
)
