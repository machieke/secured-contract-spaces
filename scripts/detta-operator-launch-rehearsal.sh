#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-rehearsal-$(git rev-parse --short HEAD)}"
safe_version="$(printf '%s' "$version" | tr -c 'A-Za-z0-9._-' '-')"
target_triple="${DETTA_TARGET_TRIPLE:-$(rustc -vV | awk '/^host:/ {print $2}')}"
out_dir="${DETTA_RELEASE_OUT:-$repo_root/dist}"
chain_id="${DETTA_RELEASE_CHAIN_ID:-detta-local}"

DETTA_RELEASE_VERSION="$version" \
  DETTA_RELEASE_OUT="$out_dir" \
  DETTA_RELEASE_CHAIN_ID="$chain_id" \
  scripts/detta-package-release.sh >/dev/null

archive_name="detta-node-${safe_version}-${target_triple}.tar.gz"
genesis_name="detta-demo-genesis-${safe_version}.json"
archive_path="$out_dir/$archive_name"
genesis_path="$out_dir/$genesis_name"
report_name="detta-launch-rehearsal-${safe_version}.json"
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

mkdir -p "$tmpdir/unpack" "$tmpdir/node"
tar -xzf "$archive_path" -C "$tmpdir/unpack"
binary="$tmpdir/unpack/detta-node"
test -x "$binary"

genesis_root="$(
  sed -n 's/.*"global_state_root": "\([a-f0-9]*\)".*/\1/p' "$genesis_path" | head -1
)"
if [ -z "$genesis_root" ]; then
  echo "failed to read genesis root from $genesis_path" >&2
  exit 1
fi

port="${DETTA_REHEARSAL_PORT:-18380}"
while nc -z 127.0.0.1 "$port" >/dev/null 2>&1; do
  port=$((port + 1))
done

"$binary" serve \
  --storage "$tmpdir/node" \
  --genesis "$genesis_path" \
  --validator-id validator-rehearsal \
  --rpc "127.0.0.1:$port" \
  --transport tcp \
  --max-connections 2 \
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

printf '{"method":"get_state_root"}\n' | nc -N 127.0.0.1 "$port" >"$tmpdir/response"
wait "$server_pid"
server_pid=""

response_root="$(
  sed -n 's/.*"data":"\([a-f0-9]*\)".*/\1/p' "$tmpdir/response" | head -1
)"
if [ "$response_root" != "$genesis_root" ]; then
  echo "state root mismatch: expected $genesis_root got ${response_root:-<empty>}" >&2
  cat "$tmpdir/response" >&2 || true
  exit 1
fi

cat >"$report_path" <<JSON
{
  "schema": "detta.operator-launch-rehearsal.v1",
  "schema_version": 1,
  "project": "DeTTa",
  "version": "$safe_version",
  "target": "$target_triple",
  "chain_id": "$chain_id",
  "archive": "$archive_name",
  "genesis": "$genesis_name",
  "transport": "tcp",
  "rpc_method": "get_state_root",
  "state_root": "$response_root",
  "status": "passed"
}
JSON

(
  cd "$out_dir"
  sha256sum "$report_name" >"$report_name.sha256"
  sha256sum -c "$report_name.sha256"
)

printf 'launch rehearsal report: %s\n' "$report_path"
