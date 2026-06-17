#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-da-stability-drill-$(git rev-parse --short HEAD)}"
safe_version="$(printf '%s' "$version" | tr -c 'A-Za-z0-9._-' '-')"
out_dir="${DETTA_RELEASE_OUT:-$repo_root/dist}"
chain_id="${DETTA_RELEASE_CHAIN_ID:-detta-local}"
source_date_epoch="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}"
report_name="detta-da-stability-drill-${safe_version}.json"
report_path="$out_dir/$report_name"

for tool in awk git jq sha256sum wc; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool missing: $tool" >&2
    exit 1
  fi
done

mkdir -p "$out_dir"
tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

checks_jsonl="$tmpdir/da-stability-checks.jsonl"
: >"$checks_jsonl"

run_check() {
  local label="$1"
  shift
  local log="$tmpdir/${label}.log"
  printf 'running DA stability drill check: %s\n' "$label"
  if ! "$@" >"$log" 2>&1; then
    cat "$log" >&2 || true
    echo "DA stability drill check failed: $label" >&2
    exit 1
  fi
  local sha bytes
  sha="$(sha256sum "$log" | awk '{print $1}')"
  bytes="$(wc -c <"$log" | tr -d '[:space:]')"
  jq -cn \
    --arg check_label "$label" \
    --arg command "$*" \
    --arg stdout_sha256 "$sha" \
    --argjson stdout_bytes "$bytes" \
    '{
      "label": $check_label,
      command: $command,
      stdout_sha256: $stdout_sha256,
      stdout_bytes: $stdout_bytes,
      status: "passed"
    }' >>"$checks_jsonl"
}

run_check da_committed_restart \
  cargo test -p detta-node persistent_validator_produces_da_committed_block_and_survives_restart
run_check da_certified_checkpoint_replay \
  cargo test -p detta-node persistent_node_replays_da_certified_block_after_checkpoint_import
run_check da_snapshot_share_retrieval \
  cargo test -p detta-node fetches_da_snapshot_shares_from_multiple_tcp_peers_and_skips_invalid_share
run_check da_store_reopen_and_indexes \
  cargo test -p detta-storage persists_and_loads_da_share_set
run_check production_da_finality \
  cargo test -p detta-consensus production_da_finality_requires_valid_da_certificate_before_replay

source_state_json="$(scripts/detta-source-state-report.sh)"
checks_count="$(wc -l <"$checks_jsonl" | tr -d '[:space:]')"
checks_sha="$(sha256sum "$checks_jsonl" | awk '{print $1}')"

jq -n -e \
  --arg schema "detta.da-stability-drill.v1" \
  --arg project "DeTTa" \
  --arg version "$safe_version" \
  --arg source_version "$version" \
  --arg chain_id "$chain_id" \
  --arg status "passed" \
  --arg checks_sha "$checks_sha" \
  --argjson source_date_epoch "$source_date_epoch" \
  --argjson source_state "$source_state_json" \
  --argjson checks_count "$checks_count" \
  --slurpfile checks "$checks_jsonl" \
  '{
    schema: $schema,
    schema_version: 1,
    project: $project,
    version: $version,
    source_version: $source_version,
    chain_id: $chain_id,
    source_date_epoch: $source_date_epoch,
    source_state: $source_state,
    drill_steps: [
      "produce_da_committed_block_and_restart",
      "replay_da_certified_block_after_checkpoint",
      "retrieve_snapshot_shares_from_multiple_peers",
      "verify_da_store_reopen_and_indexes",
      "verify_production_da_finality"
    ],
    checks: {
      count: $checks_count,
      sha256: $checks_sha,
      results: $checks
    },
    status: $status
  }' >"$report_path"

(
  cd "$out_dir"
  sha256sum "$report_name" >"$report_name.sha256"
  sha256sum -c "$report_name.sha256"
)

printf 'DA stability drill report: %s\n' "$report_path"
