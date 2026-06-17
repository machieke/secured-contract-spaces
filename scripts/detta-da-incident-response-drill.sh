#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-da-incident-drill-$(git rev-parse --short HEAD)}"
safe_version="$(printf '%s' "$version" | tr -c 'A-Za-z0-9._-' '-')"
out_dir="${DETTA_RELEASE_OUT:-$repo_root/dist}"
chain_id="${DETTA_RELEASE_CHAIN_ID:-detta-local}"
source_date_epoch="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}"
report_name="detta-da-incident-response-drill-${safe_version}.json"
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

checks_jsonl="$tmpdir/da-incident-checks.jsonl"
: >"$checks_jsonl"

run_check() {
  local label="$1"
  shift
  local log="$tmpdir/${label}.log"
  printf 'running DA incident drill check: %s\n' "$label"
  if ! "$@" >"$log" 2>&1; then
    cat "$log" >&2 || true
    echo "DA incident drill check failed: $label" >&2
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

run_check signed_da_challenge_slashing \
  cargo test -p detta-node node_ingests_signed_da_challenge_flow_and_persists_slashing_evidence
run_check governed_policy_rejection \
  cargo test -p detta-node node_da_slashing_policy_rejects_disabled_invalid_response_fault
run_check consensus_da_slashing \
  cargo test -p detta-consensus data_availability_challenge_fault_slashes_validator
run_check consensus_policy_rejection \
  cargo test -p detta-consensus data_availability_slashing_policy_rejects_disallowed_faults
run_check durable_challenge_record \
  cargo test -p detta-storage persists_and_loads_da_challenge_record

source_state_json="$(scripts/detta-source-state-report.sh)"
checks_count="$(wc -l <"$checks_jsonl" | tr -d '[:space:]')"
checks_sha="$(sha256sum "$checks_jsonl" | awk '{print $1}')"

jq -n -e \
  --arg schema "detta.da-incident-response-drill.v1" \
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
      "persist_signed_da_challenge_flow",
      "verify_da_slashing_policy_rejection",
      "record_consensus_da_slashing",
      "verify_consensus_policy_rejection",
      "persist_da_challenge_record"
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

printf 'DA incident response drill report: %s\n' "$report_path"
