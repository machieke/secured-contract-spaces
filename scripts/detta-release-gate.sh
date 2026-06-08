#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test -p detta-verify
cargo test -p detta-verify tests::differential_replay_accepts_generated_transfer_corpus -- --exact
cargo build --locked --release

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
