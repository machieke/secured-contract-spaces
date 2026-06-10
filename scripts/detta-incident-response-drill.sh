#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-incident-drill-$(git rev-parse --short HEAD)}"
safe_version="$(printf '%s' "$version" | tr -c 'A-Za-z0-9._-' '-')"
target_triple="${DETTA_TARGET_TRIPLE:-$(rustc -vV | awk '/^host:/ {print $2}')}"
out_dir="${DETTA_RELEASE_OUT:-$repo_root/dist}"
chain_id="${DETTA_RELEASE_CHAIN_ID:-detta-local}"

for tool in jq nc tar sha256sum; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool missing: $tool" >&2
    exit 1
  fi
done

DETTA_RELEASE_VERSION="$version" \
  DETTA_RELEASE_OUT="$out_dir" \
  DETTA_RELEASE_CHAIN_ID="$chain_id" \
  scripts/detta-package-release.sh >/dev/null

archive_name="detta-node-${safe_version}-${target_triple}.tar.gz"
genesis_name="detta-demo-genesis-${safe_version}.json"
archive_path="$out_dir/$archive_name"
genesis_path="$out_dir/$genesis_name"
report_name="detta-incident-response-drill-${safe_version}.json"
report_path="$out_dir/$report_name"

tmpdir="$(mktemp -d)"
server_pid=""
cleanup() {
  if [ -n "$server_pid" ]; then
    kill "$server_pid" >/dev/null 2>&1 || true
    wait "$server_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$tmpdir"
}
trap cleanup EXIT

mkdir -p "$tmpdir/unpack" "$tmpdir/node" "$tmpdir/rpc"
tar -xzf "$archive_path" -C "$tmpdir/unpack"
binary="$tmpdir/unpack/detta-node"
test -x "$binary"

genesis_root="$(jq -r '.global_state_root // empty' "$genesis_path")"
if [ -z "$genesis_root" ]; then
  echo "failed to read genesis root from $genesis_path" >&2
  exit 1
fi

port="${DETTA_INCIDENT_DRILL_PORT:-18480}"
while nc -z 127.0.0.1 "$port" >/dev/null 2>&1; do
  port=$((port + 1))
done

"$binary" serve \
  --storage "$tmpdir/node" \
  --genesis "$genesis_path" \
  --validator-id validator-drill \
  --rpc "127.0.0.1:$port" \
  --transport tcp \
  --max-connections 24 \
  >"$tmpdir/stdout" \
  2>"$tmpdir/stderr" &
server_pid="$!"

ready=0
for _ in $(seq 1 50); do
  if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
    ready=1
    break
  fi
  sleep 0.1
done
if [ "$ready" -ne 1 ]; then
  cat "$tmpdir/stderr" >&2 || true
  echo "packaged node did not become ready" >&2
  exit 1
fi

rpc() {
  local name="$1"
  local request="$2"
  local output="$tmpdir/rpc/$name.json"
  printf '%s\n' "$request" | nc -N 127.0.0.1 "$port" >"$output"
  if ! jq empty "$output" >/dev/null; then
    echo "invalid JSON response for $name" >&2
    cat "$output" >&2 || true
    exit 1
  fi
  printf '%s\n' "$output"
}

expect_ok() {
  local name="$1"
  local file="$2"
  if ! jq -e '.status == "ok"' "$file" >/dev/null; then
    echo "expected ok RPC response for $name" >&2
    cat "$file" >&2
    exit 1
  fi
}

expect_error() {
  local name="$1"
  local file="$2"
  if ! jq -e '.status == "error"' "$file" >/dev/null; then
    echo "expected error RPC response for $name" >&2
    cat "$file" >&2
    exit 1
  fi
}

state_root_response="$(rpc state_root '{"method":"get_state_root"}')"
expect_ok state_root "$state_root_response"
state_root="$(jq -r '.body.data' "$state_root_response")"
if [ "$state_root" != "$genesis_root" ]; then
  echo "state root mismatch: expected $genesis_root got $state_root" >&2
  exit 1
fi

reverted_tx="$(
  jq -cn --arg chain "$chain_id" '{
    chain_id: $chain,
    tx_hash: "incident-drill-reverted-transfer-1",
    sender: "Alice",
    nonce: 1,
    valid_until_height: null,
    target: "TokenA",
    method: "Transfer",
    args: [{"Principal":"Bob"},{"Asset":"USDC"},{"Amount":10000}],
    signature_ok: true,
    budget: 1000000
  }'
)"
submit_reverted_response="$(
  rpc submit_reverted "$(
    jq -cn --argjson transaction "$reverted_tx" \
      '{method:"submit_transaction", params:{transaction:$transaction}}'
  )"
)"
expect_ok submit_reverted "$submit_reverted_response"

block_response="$(rpc produce_block '{"method":"produce_block","params":{"height":1,"timestamp":1000}}')"
expect_ok produce_block "$block_response"

receipt_response="$(
  rpc reverted_receipt \
    '{"method":"get_receipt","params":{"tx_hash":"incident-drill-reverted-transfer-1"}}'
)"
expect_ok reverted_receipt "$receipt_response"
if ! jq -e '.body.data.status == "Reverted"' "$receipt_response" >/dev/null; then
  echo "expected reverted receipt in incident drill" >&2
  cat "$receipt_response" >&2
  exit 1
fi

missing_block_response="$(rpc missing_block '{"method":"get_block","params":{"height":999999}}')"
expect_error missing_block "$missing_block_response"

pending_tx="$(
  jq -cn --arg chain "$chain_id" '{
    chain_id: $chain,
    tx_hash: "incident-drill-pending-transfer-1",
    sender: "Alice",
    nonce: 2,
    valid_until_height: null,
    target: "TokenA",
    method: "Transfer",
    args: [{"Principal":"Bob"},{"Asset":"USDC"},{"Amount":1}],
    signature_ok: true,
    budget: 1000000
  }'
)"
submit_pending_response="$(
  rpc submit_pending "$(
    jq -cn --argjson transaction "$pending_tx" \
      '{method:"submit_transaction", params:{transaction:$transaction}}'
  )"
)"
expect_ok submit_pending "$submit_pending_response"

mempool_response="$(rpc mempool_status '{"method":"get_mempool_status"}')"
expect_ok mempool_status "$mempool_response"
if ! jq -e '.body.data.pending_transactions == 1' "$mempool_response" >/dev/null; then
  echo "expected one pending transaction after incident drill setup" >&2
  cat "$mempool_response" >&2
  exit 1
fi

health_response="$(rpc node_health '{"method":"get_node_health"}')"
expect_ok node_health "$health_response"

metrics_response="$(rpc operator_metrics '{"method":"get_operator_metrics"}')"
expect_ok operator_metrics "$metrics_response"

alerts_response="$(rpc operator_alerts '{"method":"get_operator_alerts"}')"
expect_ok operator_alerts "$alerts_response"
if ! jq -e '
  (.body.data.root_mismatch == false)
  and (.body.data.latest_block_failure_count == 1)
  and (.body.data.latest_block_receipt_count == 1)
  and ([.body.data.alerts[].code] | index("operator.peer_isolation") != null)
  and ([.body.data.alerts[].code] | index("operator.stalled_consensus") != null)
  and ([.body.data.alerts[].code] | index("operator.excessive_reverts") != null)
' "$alerts_response" >/dev/null; then
  echo "incident drill did not observe the expected operator alert signals" >&2
  cat "$alerts_response" >&2
  exit 1
fi

metadata_response="$(rpc snapshot_metadata_roots '{"method":"get_snapshot_metadata_root_status"}')"
expect_ok snapshot_metadata_roots "$metadata_response"

slashing_response="$(
  rpc missing_slashing_record \
    '{"method":"get_slashing_record","params":{"validator_id":"validator-drill"}}'
)"
expect_error missing_slashing_record "$slashing_response"

jq -n \
  --arg schema "detta.incident-response-drill.v1" \
  --arg project "DeTTa" \
  --arg version "$safe_version" \
  --arg target "$target_triple" \
  --arg chain_id "$chain_id" \
  --arg archive "$archive_name" \
  --arg genesis "$genesis_name" \
  --arg state_root "$state_root" \
  --slurpfile receipt "$receipt_response" \
  --slurpfile missing_block "$missing_block_response" \
  --slurpfile mempool "$mempool_response" \
  --slurpfile health "$health_response" \
  --slurpfile metrics "$metrics_response" \
  --slurpfile alerts "$alerts_response" \
  --slurpfile metadata "$metadata_response" \
  --slurpfile slashing "$slashing_response" \
  '{
    schema: $schema,
    schema_version: 1,
    project: $project,
    version: $version,
    target: $target,
    chain_id: $chain_id,
    archive: $archive,
    genesis: $genesis,
    transport: "tcp",
    drill_steps: [
      "boot_packaged_node",
      "verify_genesis_state_root",
      "submit_reverting_transfer",
      "produce_block",
      "verify_reverted_receipt",
      "record_expected_rpc_error",
      "submit_pending_transfer",
      "inspect_health_metrics_alerts",
      "inspect_snapshot_metadata_roots",
      "verify_no_slashing_record"
    ],
    observations: {
      state_root: $state_root,
      reverted_receipt_status: $receipt[0].body.data.status,
      expected_missing_block_error: $missing_block[0].body.code,
      pending_mempool_total: $mempool[0].body.data.pending_transactions,
      health: $health[0].body.data,
      metrics: $metrics[0].body.data,
      alert_codes: [$alerts[0].body.data.alerts[].code],
      root_mismatch: $alerts[0].body.data.root_mismatch,
      latest_block_failure_count: $alerts[0].body.data.latest_block_failure_count,
      latest_block_receipt_count: $alerts[0].body.data.latest_block_receipt_count,
      snapshot_metadata_root_status: $metadata[0].body.data,
      expected_slashing_lookup_error: $slashing[0].body.code
    },
    status: "passed"
  }' >"$report_path"

(
  cd "$out_dir"
  sha256sum "$report_name" >"$report_name.sha256"
  sha256sum -c "$report_name.sha256"
)

printf 'incident response drill report: %s\n' "$report_path"
