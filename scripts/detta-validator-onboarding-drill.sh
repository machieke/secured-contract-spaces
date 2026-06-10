#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-validator-onboarding-$(git rev-parse --short HEAD)}"
safe_version="$(printf '%s' "$version" | tr -c 'A-Za-z0-9._-' '-')"
target_triple="${DETTA_TARGET_TRIPLE:-$(rustc -vV | awk '/^host:/ {print $2}')}"
out_dir="${DETTA_RELEASE_OUT:-$repo_root/dist}"
chain_id="${DETTA_RELEASE_CHAIN_ID:-detta-local}"
validator_count="${DETTA_VALIDATOR_ONBOARDING_COUNT:-4}"

for tool in jq nc tar sha256sum; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool missing: $tool" >&2
    exit 1
  fi
done

if ! [[ "$validator_count" =~ ^[0-9]+$ ]] || [ "$validator_count" -lt 1 ]; then
  echo "DETTA_VALIDATOR_ONBOARDING_COUNT must be a positive integer" >&2
  exit 1
fi

DETTA_RELEASE_VERSION="$version" \
  DETTA_RELEASE_OUT="$out_dir" \
  DETTA_RELEASE_CHAIN_ID="$chain_id" \
  scripts/detta-package-release.sh >/dev/null

archive_name="detta-node-${safe_version}-${target_triple}.tar.gz"
genesis_name="detta-demo-genesis-${safe_version}.json"
archive_path="$out_dir/$archive_name"
genesis_path="$out_dir/$genesis_name"
report_name="detta-validator-onboarding-drill-${safe_version}.json"
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

mkdir -p "$tmpdir/unpack" "$tmpdir/rpc" "$tmpdir/summaries" "$tmpdir/nodes"
tar -xzf "$archive_path" -C "$tmpdir/unpack"
binary="$tmpdir/unpack/detta-node"
test -x "$binary"

genesis_root="$(jq -r '.global_state_root // empty' "$genesis_path")"
if [ -z "$genesis_root" ]; then
  echo "failed to read genesis root from $genesis_path" >&2
  exit 1
fi

next_port="${DETTA_VALIDATOR_ONBOARDING_PORT:-18680}"
summary_files=()

reserve_port() {
  while nc -z 127.0.0.1 "$next_port" >/dev/null 2>&1; do
    next_port=$((next_port + 1))
  done
  printf '%s\n' "$next_port"
  next_port=$((next_port + 1))
}

rpc() {
  local name="$1"
  local port="$2"
  local request="$3"
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

run_node_phase() {
  local validator_id="$1"
  local phase="$2"
  local storage_dir="$3"
  local output="$4"
  local port health_response state_root_response roots_response
  local genesis_args=()

  if [ "$phase" = "first_boot" ]; then
    genesis_args=(--genesis "$genesis_path")
  fi

  port="$(reserve_port)"
  "$binary" serve \
    --storage "$storage_dir" \
    "${genesis_args[@]}" \
    --validator-id "$validator_id" \
    --rpc "127.0.0.1:$port" \
    --transport tcp \
    --max-connections 4 \
    >"$tmpdir/${validator_id}-${phase}.stdout" \
    2>"$tmpdir/${validator_id}-${phase}.stderr" &
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
    cat "$tmpdir/${validator_id}-${phase}.stderr" >&2 || true
    echo "validator $validator_id did not become ready during $phase" >&2
    exit 1
  fi

  health_response="$(rpc "${validator_id}-${phase}-health" "$port" '{"method":"get_node_health"}')"
  expect_ok "${validator_id}-${phase}-health" "$health_response"
  state_root_response="$(rpc "${validator_id}-${phase}-state-root" "$port" '{"method":"get_state_root"}')"
  expect_ok "${validator_id}-${phase}-state-root" "$state_root_response"
  roots_response="$(
    rpc "${validator_id}-${phase}-persistent-roots" "$port" \
      '{"method":"get_persistent_node_snapshot_roots"}'
  )"
  expect_ok "${validator_id}-${phase}-persistent-roots" "$roots_response"

  wait "$server_pid"
  server_pid=""

  if ! jq -e \
    --arg validator_id "$validator_id" \
    --arg chain_id "$chain_id" \
    --arg root "$genesis_root" \
    '.body.data.validator_id == $validator_id
      and .body.data.chain_id == $chain_id
      and .body.data.global_state_root == $root
      and .body.data.height == 0
      and .body.data.pending_mempool_transactions == 0
      and .body.data.pending_validator_set_metadata_updates == 0' \
    "$health_response" >/dev/null; then
    echo "validator health did not match onboarding expectations for $validator_id/$phase" >&2
    cat "$health_response" >&2
    exit 1
  fi
  if ! jq -e --arg root "$genesis_root" '.body.data == $root' "$state_root_response" >/dev/null; then
    echo "state root did not match genesis root for $validator_id/$phase" >&2
    cat "$state_root_response" >&2
    exit 1
  fi
  if ! jq -e --arg root "$genesis_root" '.body.data.global_state_root == $root' "$roots_response" >/dev/null; then
    echo "persistent snapshot roots did not match genesis root for $validator_id/$phase" >&2
    cat "$roots_response" >&2
    exit 1
  fi

  jq -n \
    --arg phase "$phase" \
    --argjson port "$port" \
    --slurpfile health "$health_response" \
    --slurpfile state_root "$state_root_response" \
    --slurpfile roots "$roots_response" \
    '{
      phase: $phase,
      rpc_port: $port,
      health: $health[0].body.data,
      state_root: $state_root[0].body.data,
      persistent_roots: $roots[0].body.data
    }' >"$output"
}

for index in $(seq 1 "$validator_count"); do
  validator_id="validator-$index"
  storage_dir="$tmpdir/nodes/$validator_id"
  mkdir -p "$storage_dir"

  first_summary="$tmpdir/summaries/${validator_id}-first.json"
  restart_summary="$tmpdir/summaries/${validator_id}-restart.json"
  validator_summary="$tmpdir/summaries/${validator_id}.json"

  run_node_phase "$validator_id" first_boot "$storage_dir" "$first_summary"
  run_node_phase "$validator_id" restart "$storage_dir" "$restart_summary"

  if ! jq -n -e --slurpfile first "$first_summary" --slurpfile restart "$restart_summary" '
    $first[0].state_root == $restart[0].state_root
    and $first[0].health.global_state_root == $restart[0].health.global_state_root
    and $first[0].persistent_roots.global_state_root == $restart[0].persistent_roots.global_state_root
  ' >/dev/null; then
    echo "restart roots diverged for $validator_id" >&2
    exit 1
  fi

  jq -n \
    --arg validator_id "$validator_id" \
    --slurpfile first "$first_summary" \
    --slurpfile restart "$restart_summary" \
    '{
      validator_id: $validator_id,
      first_boot: $first[0],
      restart: $restart[0],
      roots_match_after_restart: true
    }' >"$validator_summary"
  summary_files+=("$validator_summary")
done

jq -s \
  --arg schema "detta.validator-onboarding-drill.v1" \
  --arg project "DeTTa" \
  --arg version "$safe_version" \
  --arg target "$target_triple" \
  --arg chain_id "$chain_id" \
  --arg archive "$archive_name" \
  --arg genesis "$genesis_name" \
  --arg genesis_root "$genesis_root" \
  --argjson validator_count "$validator_count" \
  '{
    schema: $schema,
    schema_version: 1,
    project: $project,
    version: $version,
    target: $target,
    chain_id: $chain_id,
    archive: $archive,
    genesis: $genesis,
    genesis_root: $genesis_root,
    validator_count: $validator_count,
    validator_identity_pattern: "validator-<index>",
    transport: "tcp",
    drill_steps: [
      "boot_packaged_validator_from_genesis",
      "verify_validator_health_roots",
      "verify_persistent_snapshot_roots",
      "restart_validator_without_genesis",
      "verify_restart_roots_match_first_boot"
    ],
    validators: .,
    status: "passed"
  }' "${summary_files[@]}" >"$report_path"

(
  cd "$out_dir"
  sha256sum "$report_name" >"$report_name.sha256"
  sha256sum -c "$report_name.sha256"
)

printf 'validator onboarding drill report: %s\n' "$report_path"
