#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-packaged-client-flow-$(git rev-parse --short HEAD)}"
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
manifest_name="detta-release-${safe_version}.json"
archive_path="$out_dir/$archive_name"
genesis_path="$out_dir/$genesis_name"
manifest_path="$out_dir/$manifest_name"
report_name="detta-packaged-client-flow-drill-${safe_version}.json"
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

mkdir -p "$tmpdir/unpack" "$tmpdir/node" "$tmpdir/client"
tar -xzf "$archive_path" -C "$tmpdir/unpack"
node_binary="$tmpdir/unpack/detta-node"
client_binary="$tmpdir/unpack/detta-client"
test -x "$node_binary"
test -x "$client_binary"

port="${DETTA_PACKAGED_CLIENT_FLOW_PORT:-18780}"
while nc -z 127.0.0.1 "$port" >/dev/null 2>&1; do
  port=$((port + 1))
done
rpc_addr="127.0.0.1:$port"

"$node_binary" serve \
  --storage "$tmpdir/node" \
  --genesis "$genesis_path" \
  --validator-id validator-packaged-client-flow \
  --rpc "$rpc_addr" \
  --transport tcp \
  --max-connections 32 \
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
  echo "packaged node did not become ready for packaged client drill" >&2
  exit 1
fi

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
    echo "packaged client command did not commit as expected: $name" >&2
    cat "$file" >&2
    exit 1
  fi
}

deploy_token="$(
  run_client deploy-token \
    deploy-token \
    --rpc "$rpc_addr" \
    --chain-id "$chain_id" \
    --sender Issuer \
    --nonce 1 \
    --tx-hash packaged-client-deploy-token-1 \
    --contract ClientToken \
    --asset CLT \
    --initial-holder Alice \
    --initial-supply 1000 \
    --produce-height 1
)"
expect_committed deploy-token "$deploy_token" 1

deploy_pool="$(
  run_client deploy-pool \
    deploy-pool \
    --rpc "$rpc_addr" \
    --chain-id "$chain_id" \
    --sender Issuer \
    --nonce 2 \
    --tx-hash packaged-client-deploy-pool-1 \
    --contract ClientPool \
    --asset-a CLT \
    --asset-b USDC \
    --produce-height 2
)"
expect_committed deploy-pool "$deploy_pool" 2

add_liquidity="$(
  run_client add-liquidity \
    add-liquidity \
    --rpc "$rpc_addr" \
    --chain-id "$chain_id" \
    --sender Alice \
    --nonce 1 \
    --tx-hash packaged-client-add-liquidity-1 \
    --pool ClientPool \
    --asset-a-amount 100 \
    --asset-b-amount 50 \
    --produce-height 3
)"
expect_committed add-liquidity "$add_liquidity" 3

buy_token="$(
  run_client buy-token \
    swap \
    --rpc "$rpc_addr" \
    --chain-id "$chain_id" \
    --sender Bob \
    --nonce 1 \
    --tx-hash packaged-client-buy-token-1 \
    --pool ClientPool \
    --input-asset USDC \
    --amount-in 10 \
    --min-output 15 \
    --produce-height 4
)"
expect_committed buy-token "$buy_token" 4

sell_token="$(
  run_client sell-token \
    swap \
    --rpc "$rpc_addr" \
    --chain-id "$chain_id" \
    --sender Bob \
    --nonce 2 \
    --tx-hash packaged-client-sell-token-1 \
    --pool ClientPool \
    --input-asset CLT \
    --amount-in 15 \
    --min-output 8 \
    --produce-height 5
)"
expect_committed sell-token "$sell_token" 5

state_root="$(
  run_client state-root \
    state-root \
    --rpc "$rpc_addr"
)"
if ! jq -e '.result == "state_root" and (.data | type == "string" and length == 64)' \
  "$state_root" >/dev/null; then
  echo "packaged client state-root command did not return a root" >&2
  cat "$state_root" >&2
  exit 1
fi

jq -n -e \
  --arg schema "detta.packaged-client-flow-drill.v1" \
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
  --slurpfile deploy_token "$deploy_token" \
  --slurpfile deploy_pool "$deploy_pool" \
  --slurpfile add_liquidity "$add_liquidity" \
  --slurpfile buy_token "$buy_token" \
  --slurpfile sell_token "$sell_token" \
  --slurpfile state_root "$state_root" \
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
    packaged_binaries: ["detta-node", "detta-client"],
    workflow: [
      {name: "deploy_token", command: "detta-client deploy-token", result: $deploy_token[0]},
      {name: "deploy_pool", command: "detta-client deploy-pool", result: $deploy_pool[0]},
      {name: "add_liquidity", command: "detta-client add-liquidity", result: $add_liquidity[0]},
      {name: "buy_token", command: "detta-client swap", result: $buy_token[0]},
      {name: "sell_token", command: "detta-client swap", result: $sell_token[0]},
      {name: "state_root", command: "detta-client state-root", result: $state_root[0]}
    ],
    status: $status
  }' >"$report_path"

(
  cd "$out_dir"
  sha256sum "$report_name" >"$report_name.sha256"
  sha256sum -c "$report_name.sha256"
)

printf 'packaged client flow drill report: %s\n' "$report_path"
