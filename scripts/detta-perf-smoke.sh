#!/usr/bin/env bash
#
# CI perf-smoke: builds and runs the cross-layer benchmark harness in --check
# mode, which asserts hardware-independent cost and latency-ratio invariants for
# the DA production/verification path. Exits non-zero on a regression.
#
# Tune the coarse absolute backstop for slow runners via DETTA_PERF_SMOKE_MAX_MS
# (milliseconds; default 2000).
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo run --locked -p detta-e2e --bin layer_benchmarks --release -- --check
