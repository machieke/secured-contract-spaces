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

expect_da_manifest_index_contains() {
  local name="$1"
  local file="$2"
  local manifest_hash="$3"
  if ! jq -e \
    --arg manifest_hash "$manifest_hash" \
    '.result == "da_manifest_index"
      and any(.data[]; .manifest_hash == $manifest_hash)' \
    "$file" >/dev/null; then
    echo "DA manifest index did not contain produced manifest: $name" >&2
    cat "$file" >&2
    exit 1
  fi
}

expect_da_certificate_index_empty() {
  local name="$1"
  local file="$2"
  if ! jq -e \
    '.result == "da_certificate_index" and .data == []' \
    "$file" >/dev/null; then
    echo "DA certificate index was expected to be empty for uncertified packaged DA block: $name" >&2
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

submit_da_token="$(
  run_client submit-da-token \
    deploy-token \
    --rpc "$rpc_addr" \
    --chain-id "$chain_id" \
    --sender Issuer \
    --nonce 3 \
    --tx-hash packaged-client-da-deploy-token-1 \
    --contract ClientDaToken \
    --asset DCLT \
    --initial-holder Alice \
    --initial-supply 10
)"
if ! jq -e \
  '.submitted.result == "submitted"
    and .produced_block == null
    and .receipt == null' \
  "$submit_da_token" >/dev/null; then
  echo "packaged client submit-only command did not leave transaction pending" >&2
  cat "$submit_da_token" >&2
  exit 1
fi

produce_da_block="$(
  run_client produce-da-block \
    produce-da-block \
    --rpc "$rpc_addr" \
    --height 6 \
    --timestamp 6000 \
    --share-size 96
)"
if ! jq -e \
  '.result == "block"
    and .data.header.height == 6
    and (.data.transactions | length) == 1
    and (.data.receipts | length) == 1
    and (.data.header.data_availability.manifest_hash | type == "string" and length == 64)
    and (.data.header.data_availability.payload_root | type == "string" and length == 64)
    and (.data.header.data_availability.share_root | type == "string" and length == 64)
    and (.data.header.data_availability | has("certificate_hash") | not)' \
  "$produce_da_block" >/dev/null; then
  echo "packaged client produce-da-block command did not return a DA-committed block" >&2
  cat "$produce_da_block" >&2
  exit 1
fi
da_manifest_hash="$(jq -r '.data.header.data_availability.manifest_hash' "$produce_da_block")"
da_payload_root="$(jq -r '.data.header.data_availability.payload_root' "$produce_da_block")"
da_share_root="$(jq -r '.data.header.data_availability.share_root' "$produce_da_block")"

da_receipt="$(
  run_client da-receipt \
    receipt \
    --rpc "$rpc_addr" \
    --tx-hash packaged-client-da-deploy-token-1
)"
if ! jq -e '.result == "receipt" and .data.status == "Committed"' \
  "$da_receipt" >/dev/null; then
  echo "packaged client receipt command did not find DA-produced transaction receipt" >&2
  cat "$da_receipt" >&2
  exit 1
fi

da_manifest="$(
  run_client da-manifest \
    da-manifest \
    --rpc "$rpc_addr" \
    --manifest-hash "$da_manifest_hash"
)"
if ! jq -e \
  --arg payload_root "$da_payload_root" \
  --arg share_root "$da_share_root" \
  '.result == "da_manifest"
    and .data.height == 6
    and .data.payload_hash == $payload_root
    and .data.share_root == $share_root
    and .data.encoded_share_count > 0
    and (.data.share_hashes | length) == .data.encoded_share_count
    and .data.share_size_bytes == 96' \
  "$da_manifest" >/dev/null; then
  echo "packaged client da-manifest command did not return expected manifest" >&2
  cat "$da_manifest" >&2
  exit 1
fi
da_manifest_block_hash="$(jq -r '.data.block_hash' "$da_manifest")"

da_share="$(
  run_client da-share \
    da-share \
    --rpc "$rpc_addr" \
    --manifest-hash "$da_manifest_hash" \
    --index 0
)"
if ! jq -e \
  --arg manifest_hash "$da_manifest_hash" \
  '.result == "da_share"
    and .data.manifest_hash == $manifest_hash
    and .data.index == 0
    and (.data.bytes | type == "array" and length > 0)
    and (.data.share_hash | type == "string" and length == 64)' \
  "$da_share" >/dev/null; then
  echo "packaged client da-share command did not return share 0" >&2
  cat "$da_share" >&2
  exit 1
fi

da_payload="$(
  run_client da-payload \
    da-payload \
    --rpc "$rpc_addr" \
    --manifest-hash "$da_manifest_hash"
)"
if ! jq -e \
  '.result == "da_payload"
    and .data.height == 6
    and ((.data.namespaces | map(.namespace) | index("detta.block")) != null)
    and ((.data.namespaces | map(.namespace) | index("detta.tx")) != null)
    and ((.data.namespaces | map(.namespace) | index("detta.receipt")) != null)' \
  "$da_payload" >/dev/null; then
  echo "packaged client da-payload command did not reconstruct expected namespaces" >&2
  cat "$da_payload" >&2
  exit 1
fi

da_namespace="$(
  run_client da-namespace \
    da-namespace \
    --rpc "$rpc_addr" \
    --manifest-hash "$da_manifest_hash" \
    --namespace detta.tx
)"
if ! jq -e \
  '.result == "da_namespace"
    and .data.namespace == "detta.tx"
    and (.data.records | length) == 1' \
  "$da_namespace" >/dev/null; then
  echo "packaged client da-namespace command did not return transaction namespace" >&2
  cat "$da_namespace" >&2
  exit 1
fi

da_sample_proofs="$(
  run_client da-sample-proofs \
    da-sample-proofs \
    --rpc "$rpc_addr" \
    --manifest-hash "$da_manifest_hash" \
    --client-randomness packaged-client-da-sampling \
    --sample-count 2 \
    --namespaces detta.tx,detta.receipt
)"
if ! jq -e \
  '.result == "da_sample_proofs"
    and .data.verification.valid == true
    and (.data.sample_proofs | length) > 0
    and (.data.namespace_proofs | length) == 2' \
  "$da_sample_proofs" >/dev/null; then
  echo "packaged client da-sample-proofs command did not return valid sampling proofs" >&2
  cat "$da_sample_proofs" >&2
  exit 1
fi

da_status="$(
  run_client da-status \
    da-status \
    --rpc "$rpc_addr" \
    --manifest-hash "$da_manifest_hash"
)"
if ! jq -e \
  --arg manifest_hash "$da_manifest_hash" \
  '.result == "da_status"
    and .data.manifest_hash == $manifest_hash
    and .data.manifest_available == true
    and .data.certificate_available == false
    and .data.stored_share_count == .data.expected_share_count
    and (.data.missing_share_indices | length) == 0
    and .data.payload_reconstructable == true' \
  "$da_status" >/dev/null; then
  echo "packaged client da-status command did not report available uncertified DA payload" >&2
  cat "$da_status" >&2
  exit 1
fi

da_repair_status="$(
  run_client da-repair-status \
    da-repair-status \
    --rpc "$rpc_addr" \
    --manifest-hash "$da_manifest_hash"
)"
if ! jq -e \
  '.result == "da_repair_status"
    and .data.repair_needed == false
    and (.data.missing_share_indices | length) == 0
    and .data.payload_reconstructable == true' \
  "$da_repair_status" >/dev/null; then
  echo "packaged client da-repair-status command did not report healthy DA shares" >&2
  cat "$da_repair_status" >&2
  exit 1
fi

da_stats="$(
  run_client da-stats \
    da-stats \
    --rpc "$rpc_addr"
)"
if ! jq -e \
  '.result == "da_storage_stats"
    and .data.manifest_count >= 1
    and .data.expected_share_count >= 1
    and .data.stored_share_count == .data.expected_share_count
    and .data.missing_share_count == 0
    and .data.payload_count >= 1
    and .data.certificate_count == 0
    and (.data.retention_policy_root | type == "string" and length == 64)' \
  "$da_stats" >/dev/null; then
  echo "packaged client da-stats command did not report expected DA store counters" >&2
  cat "$da_stats" >&2
  exit 1
fi

da_retention_audit="$(
  run_client da-retention-audit \
    da-retention-audit \
    --rpc "$rpc_addr"
)"
if ! jq -e \
  --arg manifest_hash "$da_manifest_hash" \
  '.result == "da_retention_audit"
    and .data.manifest_count >= 1
    and .data.unsatisfied_manifest_count == 0
    and any(.data.entries[]; .manifest_hash == $manifest_hash and .retention_satisfied == true)' \
  "$da_retention_audit" >/dev/null; then
  echo "packaged client da-retention-audit command did not report satisfied retention" >&2
  cat "$da_retention_audit" >&2
  exit 1
fi

da_retention_prune_plan="$(
  run_client da-retention-prune-plan \
    da-retention-prune-plan \
    --rpc "$rpc_addr"
)"
if ! jq -e \
  '.result == "da_retention_prune_plan"
    and .data.manifest_count >= 1
    and .data.candidate_manifest_count == 0
    and .data.prunable_payload_count == 0
    and .data.prunable_share_count == 0' \
  "$da_retention_prune_plan" >/dev/null; then
  echo "packaged client da-retention-prune-plan command did not report an empty prune plan" >&2
  cat "$da_retention_prune_plan" >&2
  exit 1
fi

da_manifest_index_height="$(
  run_client da-manifest-index-by-height \
    da-manifest-index-by-height \
    --rpc "$rpc_addr" \
    --height 6
)"
expect_da_manifest_index_contains height "$da_manifest_index_height" "$da_manifest_hash"

da_manifest_index_block="$(
  run_client da-manifest-index-by-block \
    da-manifest-index-by-block \
    --rpc "$rpc_addr" \
    --block-hash "$da_manifest_block_hash"
)"
expect_da_manifest_index_contains block "$da_manifest_index_block" "$da_manifest_hash"

da_manifest_index_namespace="$(
  run_client da-manifest-index-by-namespace \
    da-manifest-index-by-namespace \
    --rpc "$rpc_addr" \
    --namespace detta.tx
)"
expect_da_manifest_index_contains namespace "$da_manifest_index_namespace" "$da_manifest_hash"

da_manifest_index_retention="$(
  run_client da-manifest-index-by-retention \
    da-manifest-index-by-retention \
    --rpc "$rpc_addr" \
    --class hot
)"
expect_da_manifest_index_contains retention "$da_manifest_index_retention" "$da_manifest_hash"

da_certificate_index_manifest="$(
  run_client da-certificate-index-by-manifest \
    da-certificate-index-by-manifest \
    --rpc "$rpc_addr" \
    --manifest-hash "$da_manifest_hash"
)"
expect_da_certificate_index_empty manifest "$da_certificate_index_manifest"

da_certificate_index_height="$(
  run_client da-certificate-index-by-height \
    da-certificate-index-by-height \
    --rpc "$rpc_addr" \
    --height 6
)"
expect_da_certificate_index_empty height "$da_certificate_index_height"

da_certificate_index_block="$(
  run_client da-certificate-index-by-block \
    da-certificate-index-by-block \
    --rpc "$rpc_addr" \
    --block-hash "$da_manifest_block_hash"
)"
expect_da_certificate_index_empty block "$da_certificate_index_block"

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
  --slurpfile submit_da_token "$submit_da_token" \
  --slurpfile produce_da_block "$produce_da_block" \
  --slurpfile da_receipt "$da_receipt" \
  --slurpfile da_manifest "$da_manifest" \
  --slurpfile da_share "$da_share" \
  --slurpfile da_payload "$da_payload" \
  --slurpfile da_namespace "$da_namespace" \
  --slurpfile da_sample_proofs "$da_sample_proofs" \
  --slurpfile da_status "$da_status" \
  --slurpfile da_repair_status "$da_repair_status" \
  --slurpfile da_stats "$da_stats" \
  --slurpfile da_retention_audit "$da_retention_audit" \
  --slurpfile da_retention_prune_plan "$da_retention_prune_plan" \
  --slurpfile da_manifest_index_height "$da_manifest_index_height" \
  --slurpfile da_manifest_index_block "$da_manifest_index_block" \
  --slurpfile da_manifest_index_namespace "$da_manifest_index_namespace" \
  --slurpfile da_manifest_index_retention "$da_manifest_index_retention" \
  --slurpfile da_certificate_index_manifest "$da_certificate_index_manifest" \
  --slurpfile da_certificate_index_height "$da_certificate_index_height" \
  --slurpfile da_certificate_index_block "$da_certificate_index_block" \
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
      {name: "submit_da_token", command: "detta-client deploy-token", result: $submit_da_token[0]},
      {name: "produce_da_block", command: "detta-client produce-da-block", result: $produce_da_block[0]},
      {name: "da_receipt", command: "detta-client receipt", result: $da_receipt[0]},
      {name: "da_manifest", command: "detta-client da-manifest", result: $da_manifest[0]},
      {name: "da_share", command: "detta-client da-share", result: $da_share[0]},
      {name: "da_payload", command: "detta-client da-payload", result: $da_payload[0]},
      {name: "da_namespace", command: "detta-client da-namespace", result: $da_namespace[0]},
      {name: "da_sample_proofs", command: "detta-client da-sample-proofs", result: $da_sample_proofs[0]},
      {name: "da_status", command: "detta-client da-status", result: $da_status[0]},
      {name: "da_repair_status", command: "detta-client da-repair-status", result: $da_repair_status[0]},
      {name: "da_stats", command: "detta-client da-stats", result: $da_stats[0]},
      {name: "da_retention_audit", command: "detta-client da-retention-audit", result: $da_retention_audit[0]},
      {name: "da_retention_prune_plan", command: "detta-client da-retention-prune-plan", result: $da_retention_prune_plan[0]},
      {name: "da_manifest_index_by_height", command: "detta-client da-manifest-index-by-height", result: $da_manifest_index_height[0]},
      {name: "da_manifest_index_by_block", command: "detta-client da-manifest-index-by-block", result: $da_manifest_index_block[0]},
      {name: "da_manifest_index_by_namespace", command: "detta-client da-manifest-index-by-namespace", result: $da_manifest_index_namespace[0]},
      {name: "da_manifest_index_by_retention", command: "detta-client da-manifest-index-by-retention", result: $da_manifest_index_retention[0]},
      {name: "da_certificate_index_by_manifest", command: "detta-client da-certificate-index-by-manifest", result: $da_certificate_index_manifest[0]},
      {name: "da_certificate_index_by_height", command: "detta-client da-certificate-index-by-height", result: $da_certificate_index_height[0]},
      {name: "da_certificate_index_by_block", command: "detta-client da-certificate-index-by-block", result: $da_certificate_index_block[0]},
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
