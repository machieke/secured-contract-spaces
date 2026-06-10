#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-governance-drill-$(git rev-parse --short HEAD)}"
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
report_name="detta-governance-bootstrap-drill-${safe_version}.json"
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

port="${DETTA_GOVERNANCE_DRILL_PORT:-18580}"
while nc -z 127.0.0.1 "$port" >/dev/null 2>&1; do
  port=$((port + 1))
done

"$binary" serve \
  --storage "$tmpdir/node" \
  --genesis "$genesis_path" \
  --validator-id validator-governance-drill \
  --rpc "127.0.0.1:$port" \
  --transport tcp \
  --max-connections 64 \
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

submit_tx() {
  local name="$1"
  local tx_hash="$2"
  local sender="$3"
  local nonce="$4"
  local method="$5"
  local args="$6"
  local tx request response
  tx="$(
    jq -cn \
      --arg chain "$chain_id" \
      --arg tx_hash "$tx_hash" \
      --arg sender "$sender" \
      --argjson nonce "$nonce" \
      --arg method "$method" \
      --argjson args "$args" \
      '{
        chain_id: $chain,
        tx_hash: $tx_hash,
        sender: $sender,
        nonce: $nonce,
        valid_until_height: null,
        target: "GovA",
        method: $method,
        args: $args,
        signature_ok: true,
        budget: 1000000
      }'
  )"
  request="$(
    jq -cn --argjson transaction "$tx" \
      '{method:"submit_transaction", params:{transaction:$transaction}}'
  )"
  response="$(rpc "$name" "$request")"
  expect_ok "$name" "$response"
}

produce_block() {
  local name="$1"
  local height="$2"
  local timestamp="$3"
  local response
  response="$(
    rpc "$name" "$(
      jq -cn --argjson height "$height" --argjson timestamp "$timestamp" \
        '{method:"produce_block", params:{height:$height, timestamp:$timestamp}}'
    )"
  )"
  expect_ok "$name" "$response"
}

receipt() {
  local name="$1"
  local tx_hash="$2"
  local response
  response="$(
    rpc "$name" "$(
      jq -cn --arg tx_hash "$tx_hash" \
        '{method:"get_receipt", params:{tx_hash:$tx_hash}}'
    )"
  )"
  expect_ok "$name" "$response"
  printf '%s\n' "$response"
}

state_root_response="$(rpc state_root '{"method":"get_state_root"}')"
expect_ok state_root "$state_root_response"
state_root="$(jq -r '.body.data' "$state_root_response")"
if [ "$state_root" != "$genesis_root" ]; then
  echo "state root mismatch: expected $genesis_root got $state_root" >&2
  exit 1
fi

token_before_response="$(rpc token_before '{"method":"get_contract","params":{"contract":"TokenA"}}')"
expect_ok token_before "$token_before_response"
old_code_hash="$(jq -r '.body.data.code_hash' "$token_before_response")"

submit_tx \
  submit_schedule_upgrade \
  governance-drill-schedule-upgrade-1 \
  Admin \
  1 \
  ScheduleUpgrade \
  '[{"Text":"governance-drill-upgrade-1"},{"Text":"token-code-governance-drill-v2"}]'
produce_block produce_schedule_upgrade 1 1000
schedule_upgrade_receipt="$(receipt receipt_schedule_upgrade governance-drill-schedule-upgrade-1)"
if ! jq -e '.body.data.status == "Committed"' "$schedule_upgrade_receipt" >/dev/null; then
  echo "governance upgrade scheduling did not commit" >&2
  cat "$schedule_upgrade_receipt" >&2
  exit 1
fi

scheduled_upgrades_response="$(rpc scheduled_upgrades '{"method":"get_scheduled_upgrades"}')"
expect_ok scheduled_upgrades "$scheduled_upgrades_response"
if ! jq -e '
  any(.body.data[];
    .upgrade_id == "governance-drill-upgrade-1"
    and .governance_contract == "GovA"
    and .target_contract == "TokenA"
    and .new_code_hash == "token-code-governance-drill-v2"
    and .execute_after_height == 3
    and .executed == false)
' "$scheduled_upgrades_response" >/dev/null; then
  echo "scheduled upgrade was not visible in governance queue" >&2
  cat "$scheduled_upgrades_response" >&2
  exit 1
fi

rehearsal_response="$(
  rpc upgrade_rehearsal \
    '{"method":"get_upgrade_rehearsal_report","params":{"upgrade_id":"governance-drill-upgrade-1"}}'
)"
expect_ok upgrade_rehearsal "$rehearsal_response"
if ! jq -e --arg old "$old_code_hash" '
  .body.data.old_code_hash == $old
  and .body.data.new_code_hash == "token-code-governance-drill-v2"
  and .body.data.ready_at_height == false
  and (.body.data.invariant_failures | length) == 0
' "$rehearsal_response" >/dev/null; then
  echo "upgrade rehearsal report did not match expected bootstrap state" >&2
  cat "$rehearsal_response" >&2
  exit 1
fi

submit_tx \
  submit_early_upgrade \
  governance-drill-execute-upgrade-early-1 \
  Admin \
  2 \
  ExecuteUpgrade \
  '[{"Text":"governance-drill-upgrade-1"}]'
produce_block produce_early_upgrade 2 2000
early_upgrade_receipt="$(receipt receipt_early_upgrade governance-drill-execute-upgrade-early-1)"
if ! jq -e '
  .body.data.status == "Reverted"
  and .body.data.error == "TimelockNotReady"
' "$early_upgrade_receipt" >/dev/null; then
  echo "early upgrade execution did not fail with TimelockNotReady" >&2
  cat "$early_upgrade_receipt" >&2
  exit 1
fi

submit_tx \
  submit_execute_upgrade \
  governance-drill-execute-upgrade-1 \
  Admin \
  3 \
  ExecuteUpgrade \
  '[{"Text":"governance-drill-upgrade-1"}]'
produce_block produce_execute_upgrade 3 3000
execute_upgrade_receipt="$(receipt receipt_execute_upgrade governance-drill-execute-upgrade-1)"
if ! jq -e '.body.data.status == "Committed"' "$execute_upgrade_receipt" >/dev/null; then
  echo "timelocked upgrade execution did not commit" >&2
  cat "$execute_upgrade_receipt" >&2
  exit 1
fi

token_after_response="$(rpc token_after '{"method":"get_contract","params":{"contract":"TokenA"}}')"
expect_ok token_after "$token_after_response"
if ! jq -e '.body.data.code_hash == "token-code-governance-drill-v2"' "$token_after_response" >/dev/null; then
  echo "token code hash did not reflect executed governance upgrade" >&2
  cat "$token_after_response" >&2
  exit 1
fi

submit_tx \
  submit_schedule_policy \
  governance-drill-schedule-policy-1 \
  Admin \
  4 \
  SchedulePolicyUpdate \
  '[{"Text":"governance-drill-policy-1"},{"Text":"transfer"},{"Text":"registryWrite"}]'
produce_block produce_schedule_policy 4 4000
schedule_policy_receipt="$(receipt receipt_schedule_policy governance-drill-schedule-policy-1)"
if ! jq -e '.body.data.status == "Committed"' "$schedule_policy_receipt" >/dev/null; then
  echo "governance policy scheduling did not commit" >&2
  cat "$schedule_policy_receipt" >&2
  exit 1
fi

scheduled_policy_response="$(rpc scheduled_policy_updates '{"method":"get_scheduled_policy_updates"}')"
expect_ok scheduled_policy_updates "$scheduled_policy_response"
if ! jq -e '
  any(.body.data[];
    .update_id == "governance-drill-policy-1"
    and .governance_contract == "GovA"
    and .target_contract == "TokenA"
    and .method == "Transfer"
    and .effect == "RegistryWrite"
    and .execute_after_height == 6
    and .executed == false)
' "$scheduled_policy_response" >/dev/null; then
  echo "scheduled policy update was not visible in governance queue" >&2
  cat "$scheduled_policy_response" >&2
  exit 1
fi

submit_tx \
  submit_early_policy \
  governance-drill-execute-policy-early-1 \
  Admin \
  5 \
  ExecutePolicyUpdate \
  '[{"Text":"governance-drill-policy-1"}]'
produce_block produce_early_policy 5 5000
early_policy_receipt="$(receipt receipt_early_policy governance-drill-execute-policy-early-1)"
if ! jq -e '
  .body.data.status == "Reverted"
  and .body.data.error == "TimelockNotReady"
' "$early_policy_receipt" >/dev/null; then
  echo "early policy update execution did not fail with TimelockNotReady" >&2
  cat "$early_policy_receipt" >&2
  exit 1
fi

submit_tx \
  submit_execute_policy \
  governance-drill-execute-policy-1 \
  Admin \
  6 \
  ExecutePolicyUpdate \
  '[{"Text":"governance-drill-policy-1"}]'
produce_block produce_execute_policy 6 6000
execute_policy_receipt="$(receipt receipt_execute_policy governance-drill-execute-policy-1)"
if ! jq -e '.body.data.status == "Committed"' "$execute_policy_receipt" >/dev/null; then
  echo "timelocked policy update execution did not commit" >&2
  cat "$execute_policy_receipt" >&2
  exit 1
fi

scheduled_policy_after_response="$(rpc scheduled_policy_updates_after '{"method":"get_scheduled_policy_updates"}')"
expect_ok scheduled_policy_updates_after "$scheduled_policy_after_response"
if ! jq -e '
  any(.body.data[];
    .update_id == "governance-drill-policy-1"
    and .executed == true)
' "$scheduled_policy_after_response" >/dev/null; then
  echo "scheduled policy update did not mark executed" >&2
  cat "$scheduled_policy_after_response" >&2
  exit 1
fi

final_health_response="$(rpc final_node_health '{"method":"get_node_health"}')"
expect_ok final_node_health "$final_health_response"

jq -n \
  --arg schema "detta.governance-bootstrap-drill.v1" \
  --arg project "DeTTa" \
  --arg version "$safe_version" \
  --arg target "$target_triple" \
  --arg chain_id "$chain_id" \
  --arg archive "$archive_name" \
  --arg genesis "$genesis_name" \
  --arg state_root "$state_root" \
  --arg old_code_hash "$old_code_hash" \
  --slurpfile scheduled_upgrades "$scheduled_upgrades_response" \
  --slurpfile rehearsal "$rehearsal_response" \
  --slurpfile early_upgrade "$early_upgrade_receipt" \
  --slurpfile execute_upgrade "$execute_upgrade_receipt" \
  --slurpfile token_after "$token_after_response" \
  --slurpfile scheduled_policy "$scheduled_policy_response" \
  --slurpfile early_policy "$early_policy_receipt" \
  --slurpfile execute_policy "$execute_policy_receipt" \
  --slurpfile scheduled_policy_after "$scheduled_policy_after_response" \
  --slurpfile final_health "$final_health_response" \
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
      "schedule_timelocked_upgrade",
      "verify_upgrade_queue",
      "verify_upgrade_rehearsal",
      "reject_early_upgrade_execution",
      "execute_upgrade_after_timelock",
      "schedule_timelocked_policy_update",
      "verify_policy_update_queue",
      "reject_early_policy_execution",
      "execute_policy_update_after_timelock",
      "verify_final_node_health"
    ],
    observations: {
      state_root: $state_root,
      old_token_code_hash: $old_code_hash,
      new_token_code_hash: $token_after[0].body.data.code_hash,
      scheduled_upgrade: (
        $scheduled_upgrades[0].body.data[]
        | select(.upgrade_id == "governance-drill-upgrade-1")
      ),
      upgrade_rehearsal: $rehearsal[0].body.data,
      early_upgrade_error: $early_upgrade[0].body.data.error,
      upgrade_execution_status: $execute_upgrade[0].body.data.status,
      scheduled_policy_update_before_execution: (
        $scheduled_policy[0].body.data[]
        | select(.update_id == "governance-drill-policy-1")
      ),
      early_policy_update_error: $early_policy[0].body.data.error,
      policy_update_execution_status: $execute_policy[0].body.data.status,
      scheduled_policy_update_after_execution: (
        $scheduled_policy_after[0].body.data[]
        | select(.update_id == "governance-drill-policy-1")
      ),
      final_health: $final_health[0].body.data
    },
    status: "passed"
  }' >"$report_path"

(
  cd "$out_dir"
  sha256sum "$report_name" >"$report_name.sha256"
  sha256sum -c "$report_name.sha256"
)

printf 'governance bootstrap drill report: %s\n' "$report_path"
