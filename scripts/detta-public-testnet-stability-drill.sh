#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-stability-drill-$(git rev-parse --short HEAD)}"
safe_version="$(printf '%s' "$version" | tr -c 'A-Za-z0-9._-' '-')"
target_triple="${DETTA_TARGET_TRIPLE:-$(rustc -vV | awk '/^host:/ {print $2}')}"
out_dir="${DETTA_RELEASE_OUT:-$repo_root/dist}"
chain_id="${DETTA_RELEASE_CHAIN_ID:-detta-local}"
block_count="${DETTA_STABILITY_DRILL_BLOCKS:-16}"

for tool in jq nc tar sha256sum; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool missing: $tool" >&2
    exit 1
  fi
done

if ! [[ "$block_count" =~ ^[0-9]+$ ]] || [ "$block_count" -lt 6 ]; then
  echo "DETTA_STABILITY_DRILL_BLOCKS must be an integer >= 6" >&2
  exit 1
fi

DETTA_RELEASE_VERSION="$version" \
  DETTA_RELEASE_OUT="$out_dir" \
  DETTA_RELEASE_CHAIN_ID="$chain_id" \
  scripts/detta-package-release.sh >/dev/null

archive_name="detta-node-${safe_version}-${target_triple}.tar.gz"
genesis_name="detta-demo-genesis-${safe_version}.json"
manifest_name="detta-release-${safe_version}.json"
archive_path="$out_dir/$archive_name"
genesis_path="$out_dir/$genesis_name"
manifest_path="$out_dir/$manifest_name"
report_name="detta-public-testnet-stability-drill-${safe_version}.json"
report_path="$out_dir/$report_name"

if ! jq -e '.included_binaries == ["detta-node", "detta-client"]' "$manifest_path" >/dev/null; then
  echo "release manifest does not declare both packaged binaries" >&2
  cat "$manifest_path" >&2 || true
  exit 1
fi

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

mkdir -p "$tmpdir/unpack" "$tmpdir/node" "$tmpdir/client" "$tmpdir/rpc" "$tmpdir/workload"
tar -xzf "$archive_path" -C "$tmpdir/unpack"
node_binary="$tmpdir/unpack/detta-node"
client_binary="$tmpdir/unpack/detta-client"
test -x "$node_binary"
test -x "$client_binary"

port="${DETTA_STABILITY_DRILL_PORT:-18880}"
while nc -z 127.0.0.1 "$port" >/dev/null 2>&1; do
  port=$((port + 1))
done
rpc_addr="127.0.0.1:$port"
max_connections=$((block_count + 96))

"$node_binary" serve \
  --storage "$tmpdir/node" \
  --genesis "$genesis_path" \
  --validator-id validator-stability-drill \
  --rpc "$rpc_addr" \
  --transport tcp \
  --max-connections "$max_connections" \
  >"$tmpdir/node.stdout" \
  2>"$tmpdir/node.stderr" &
server_pid="$!"

ready=0
for _ in $(seq 1 50); do
  if nc -z 127.0.0.1 "$port" >/dev/null 2>&1; then
    ready=1
    sleep 0.1
    break
  fi
  sleep 0.1
done
if [ "$ready" -ne 1 ]; then
  cat "$tmpdir/node.stderr" >&2 || true
  echo "packaged node did not become ready for stability drill" >&2
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
  if ! jq -e '.status == "ok"' "$output" >/dev/null; then
    echo "expected ok RPC response for $name" >&2
    cat "$output" >&2
    exit 1
  fi
  printf '%s\n' "$output"
}

run_client() {
  local name="$1"
  shift
  local output="$tmpdir/client/$name.json"
  "$client_binary" "$@" >"$output"
  if ! jq empty "$output" >/dev/null; then
    echo "invalid JSON response from packaged client command $name" >&2
    cat "$output" >&2 || true
    exit 1
  fi
  printf '%s\n' "$output"
}

expect_committed() {
  local name="$1"
  local file="$2"
  local height="$3"
  if ! jq -e \
    --argjson height "$height" \
    '.submitted.result == "submitted"
      and .produced_block.height == $height
      and .produced_block.transaction_count == 1
      and .receipt.status == "Committed"' \
    "$file" >/dev/null; then
    echo "stability workload command did not commit as expected: $name" >&2
    cat "$file" >&2
    exit 1
  fi
}

workload_jsonl="$tmpdir/workload/blocks.jsonl"

record_workload() {
  local name="$1"
  local file="$2"
  jq -c --arg name "$name" '{name: $name, result: .}' "$file" >>"$workload_jsonl"
}

deploy_token="$(
  run_client deploy-token \
    deploy-token \
    --rpc "$rpc_addr" \
    --chain-id "$chain_id" \
    --sender Issuer \
    --nonce 1 \
    --tx-hash stability-deploy-token-1 \
    --contract StabilityToken \
    --asset STBL \
    --initial-holder Alice \
    --initial-supply 100000 \
    --produce-height 1
)"
expect_committed deploy-token "$deploy_token" 1
record_workload deploy_token "$deploy_token"

deploy_pool="$(
  run_client deploy-pool \
    deploy-pool \
    --rpc "$rpc_addr" \
    --chain-id "$chain_id" \
    --sender Issuer \
    --nonce 2 \
    --tx-hash stability-deploy-pool-1 \
    --contract StabilityPool \
    --asset-a STBL \
    --asset-b USDC \
    --produce-height 2
)"
expect_committed deploy-pool "$deploy_pool" 2
record_workload deploy_pool "$deploy_pool"

add_liquidity="$(
  run_client add-liquidity \
    add-liquidity \
    --rpc "$rpc_addr" \
    --chain-id "$chain_id" \
    --sender Alice \
    --nonce 1 \
    --tx-hash stability-add-liquidity-1 \
    --pool StabilityPool \
    --asset-a-amount 1000 \
    --asset-b-amount 100 \
    --produce-height 3
)"
expect_committed add-liquidity "$add_liquidity" 3
record_workload add_liquidity "$add_liquidity"

faucet_tx="$(
  "$node_binary" faucet-tx \
    --to Bob \
    --amount 1000 \
    --nonce 1 \
    --tx-hash stability-faucet-bob-1 \
    --chain-id "$chain_id"
)"
submit_faucet="$(
  rpc submit-faucet "$(
    jq -cn --argjson transaction "$faucet_tx" \
      '{method:"submit_transaction", params:{transaction:$transaction}}'
  )"
)"
if ! jq -e '.body.result == "submitted"' "$submit_faucet" >/dev/null; then
  echo "faucet transaction was not submitted" >&2
  cat "$submit_faucet" >&2
  exit 1
fi
faucet_block="$(
  rpc produce-faucet-block \
    '{"method":"produce_block","params":{"height":4,"timestamp":4000}}'
)"
if ! jq -e '.body.result == "block" and .body.data.header.height == 4 and (.body.data.receipts[0].status == "Committed")' \
  "$faucet_block" >/dev/null; then
  echo "faucet block did not commit as expected" >&2
  cat "$faucet_block" >&2
  exit 1
fi
jq -c '{name:"fund_bob", result:.body.data}' "$faucet_block" >>"$workload_jsonl"

for height in $(seq 5 "$block_count"); do
  nonce=$((height - 4))
  swap="$(
    run_client "swap-$height" \
      swap \
      --rpc "$rpc_addr" \
      --chain-id "$chain_id" \
      --sender Bob \
      --nonce "$nonce" \
      --tx-hash "stability-swap-$height" \
      --pool StabilityPool \
      --input-asset USDC \
      --amount-in 10 \
      --min-output 0 \
      --produce-height "$height"
  )"
  expect_committed "swap-$height" "$swap" "$height"
  record_workload "swap_$height" "$swap"
done

state_root="$(rpc state-root '{"method":"get_state_root"}')"
health="$(rpc health '{"method":"get_node_health"}')"
metrics="$(rpc metrics '{"method":"get_operator_metrics"}')"
alerts="$(rpc alerts '{"method":"get_operator_alerts"}')"
mempool="$(rpc mempool '{"method":"get_mempool_status"}')"
da_stats="$(rpc da_stats '{"method":"get_da_storage_stats"}')"
da_retention_audit="$(rpc da_retention_audit '{"method":"get_da_retention_audit"}')"

if ! jq -e --argjson height "$block_count" '
  .body.data.height == $height
  and .body.data.pending_mempool_transactions == 0
  and .body.data.da_production_profile.schema == "detta.da-production-profile.v1"
  and .body.data.da_production_profile.schema_version == 1
  and .body.data.da_production_profile.full_payload_required_for_rpc == true
  and .body.data.da_production_profile.receipts_are_payload_records == true
' "$health" >/dev/null; then
  echo "node health did not match stability expectations" >&2
  cat "$health" >&2
  exit 1
fi

if ! jq -e --argjson height "$block_count" '
  .body.data.consensus_height == $height
  and .body.data.mempool_size == 0
  and .body.data.rpc_error_count == 0
  and .body.data.da_production_profile.schema == "detta.da-production-profile.v1"
  and .body.data.da_production_profile.schema_version == 1
  and .body.data.da_production_profile.min_custody_share_count >= 2
  and .body.data.da_production_profile.min_light_client_sample_count >= 3
  and (.body.data.finality_lag == null or .body.data.finality_lag <= 2)
' "$metrics" >/dev/null; then
  echo "operator metrics did not match stability expectations" >&2
  cat "$metrics" >&2
  exit 1
fi

if ! jq -e '
  .body.data.root_mismatch == false
  and .body.data.slashing_record_count == 0
  and .body.data.latest_block_failure_count == 0
  and .body.data.metrics.da_production_profile.schema == "detta.da-production-profile.v1"
  and .body.data.metrics.da_production_profile.schema_version == 1
  and (([.body.data.alerts[].code] - ["operator.peer_isolation", "operator.stalled_consensus"]) | length == 0)
' "$alerts" >/dev/null; then
  echo "operator alerts did not match stability expectations" >&2
  cat "$alerts" >&2
  exit 1
fi

if ! jq -e '.body.data.pending_transactions == 0' "$mempool" >/dev/null; then
  echo "mempool was not empty after stability workload" >&2
  cat "$mempool" >&2
  exit 1
fi

if ! jq -e '
  .body.data.retention_policy.schema? == null
  and .body.data.retention_policy_root != null
  and .body.data.retention_policy_bytes > 0
  and ([.body.data.retention_policy.policies[].class] | index("Hot") != null)
  and ([.body.data.retention_policy.policies[].class] | index("Checkpoint") != null)
' "$da_stats" >/dev/null; then
  echo "DA storage stats did not expose the expected retention policy" >&2
  cat "$da_stats" >&2
  exit 1
fi

if ! jq -e --argjson height "$block_count" '
  .body.data.current_height == $height
  and .body.data.policy_root != null
  and .body.data.policy != null
  and ((.body.data.active_manifest_count + .body.data.expired_manifest_count) == .body.data.manifest_count)
  and .body.data.missing_policy_class_count == 0
  and .body.data.unsatisfied_manifest_count == 0
  and ([.body.data.entries[].retention_satisfied] | all(. == true))
' "$da_retention_audit" >/dev/null; then
  echo "DA retention audit did not match stability expectations" >&2
  cat "$da_retention_audit" >&2
  exit 1
fi

jq -n -e \
  --arg schema "detta.public-testnet-stability-drill.v1" \
  --arg project "DeTTa" \
  --arg version "$safe_version" \
  --arg source_version "$version" \
  --arg target "$target_triple" \
  --arg chain_id "$chain_id" \
  --arg archive "$archive_name" \
  --arg genesis "$genesis_name" \
  --arg manifest "$manifest_name" \
  --arg rpc_addr "$rpc_addr" \
  --arg status "passed" \
  --argjson block_count "$block_count" \
  --slurpfile workload "$workload_jsonl" \
  --slurpfile state_root "$state_root" \
  --slurpfile health "$health" \
  --slurpfile metrics "$metrics" \
  --slurpfile alerts "$alerts" \
  --slurpfile mempool "$mempool" \
  --slurpfile da_stats "$da_stats" \
  --slurpfile da_retention_audit "$da_retention_audit" \
  '{
    schema: $schema,
    schema_version: 1,
    project: $project,
    version: $version,
    source_version: $source_version,
    target: $target,
    chain_id: $chain_id,
    archive: $archive,
    genesis: $genesis,
    release_manifest: $manifest,
    rpc_addr: $rpc_addr,
    block_count: $block_count,
    workload: $workload,
    observations: {
      state_root: $state_root[0].body.data,
      health: $health[0].body.data,
      metrics: $metrics[0].body.data,
      alert_codes: [$alerts[0].body.data.alerts[].code],
      root_mismatch: $alerts[0].body.data.root_mismatch,
      slashing_record_count: $alerts[0].body.data.slashing_record_count,
      latest_block_failure_count: $alerts[0].body.data.latest_block_failure_count,
      mempool: $mempool[0].body.data,
      da_storage_stats: $da_stats[0].body.data,
      da_retention_audit: $da_retention_audit[0].body.data
    },
    accepted_single_node_alerts: ["operator.peer_isolation", "operator.stalled_consensus"],
    status: $status
  }' >"$report_path"

(
  cd "$out_dir"
  sha256sum "$report_name" >"$report_name.sha256"
  sha256sum -c "$report_name.sha256"
)

printf 'public testnet stability drill report: %s\n' "$report_path"
