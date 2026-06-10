#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-$(git describe --tags --always --dirty)}"
safe_version="$(printf '%s' "$version" | tr -c 'A-Za-z0-9._-' '-')"
git_commit="$(git rev-parse HEAD)"
target_triple="${DETTA_TARGET_TRIPLE:-$(rustc -vV | awk '/^host:/ {print $2}')}"
source_date_epoch="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}"
chain_id="${DETTA_RELEASE_CHAIN_ID:-detta-local}"
out_dir="${DETTA_RELEASE_OUT:-$repo_root/dist}"

mkdir -p "$out_dir"

cargo build --locked --release --bin detta-node --bin detta-client

binary="$repo_root/target/release/detta-node"
client_binary="$repo_root/target/release/detta-client"
stage="$(mktemp -d)"
trap 'rm -rf "$stage"' EXIT

install -m 0755 "$binary" "$stage/detta-node"
install -m 0755 "$client_binary" "$stage/detta-client"
install -m 0644 README.md "$stage/README.md"
install -m 0644 detta-rpc-api.md "$stage/detta-rpc-api.md"
install -m 0644 detta-client-token-liquidity-guide.md \
  "$stage/detta-client-token-liquidity-guide.md"
install -m 0644 detta-client-aspect-token-guide.md \
  "$stage/detta-client-aspect-token-guide.md"
install -m 0644 detta-operator-runbook.md "$stage/detta-operator-runbook.md"

archive_name="detta-node-${safe_version}-${target_triple}.tar.gz"
archive_path="$out_dir/$archive_name"
tar \
  --sort=name \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  --mtime="@$source_date_epoch" \
  --use-compress-program='gzip -n' \
  -cf "$archive_path" \
  -C "$stage" \
  .

genesis_name="detta-demo-genesis-${safe_version}.json"
genesis_path="$out_dir/$genesis_name"
"$binary" write-genesis --output "$genesis_path" --chain-id "$chain_id" >/dev/null

faucet_name="detta-faucet-sample-${safe_version}.json"
faucet_path="$out_dir/$faucet_name"
"$binary" faucet-tx \
  --to Alice \
  --amount 100 \
  --nonce 1 \
  --tx-hash faucet-alice-1 \
  --chain-id "$chain_id" \
  >"$faucet_path"

(
  cd "$out_dir"
  sha256sum "$archive_name" >"$archive_name.sha256"
  sha256sum "$genesis_name" >"$genesis_name.sha256"
  sha256sum "$faucet_name" >"$faucet_name.sha256"
)

archive_sha="$(awk '{print $1}' "$archive_path.sha256")"
genesis_sha="$(awk '{print $1}' "$genesis_path.sha256")"
faucet_sha="$(awk '{print $1}' "$faucet_path.sha256")"

manifest_name="detta-release-${safe_version}.json"
manifest_path="$out_dir/$manifest_name"
cat >"$manifest_path" <<JSON
{
  "schema": "detta.release-artifacts.v1",
  "schema_version": 1,
  "project": "DeTTa",
  "version": "$safe_version",
  "source_version": "$version",
  "git_commit": "$git_commit",
  "target": "$target_triple",
  "source_date_epoch": $source_date_epoch,
  "chain_id": "$chain_id",
  "release_gate_command": "DETTA_E2E_FULL=1 DETTA_REQUIRE_DEP_AUDIT=1 scripts/detta-release-gate.sh",
  "included_binaries": ["detta-node", "detta-client"],
  "manifest_sha256_path": "$manifest_name.sha256",
  "manifest_signature_path": "$manifest_name.sha256.sig",
  "artifacts": [
    {
      "kind": "validator_binary_archive",
      "path": "$archive_name",
      "sha256": "$archive_sha",
      "sha256_path": "$archive_name.sha256",
      "signature_path": "$archive_name.sha256.sig"
    },
    {
      "kind": "demo_genesis",
      "path": "$genesis_name",
      "sha256": "$genesis_sha",
      "sha256_path": "$genesis_name.sha256",
      "signature_path": "$genesis_name.sha256.sig"
    },
    {
      "kind": "sample_faucet_transaction",
      "path": "$faucet_name",
      "sha256": "$faucet_sha",
      "sha256_path": "$faucet_name.sha256",
      "signature_path": "$faucet_name.sha256.sig"
    }
  ],
  "detached_signature_command": "gpg --armor --output <release>.sha256.sig --detach-sign <release>.sha256"
}
JSON
(
  cd "$out_dir"
  sha256sum "$manifest_name" >"$manifest_name.sha256"
)

(
  cd "$out_dir"
  sha256sum -c "$archive_name.sha256"
  sha256sum -c "$genesis_name.sha256"
  sha256sum -c "$faucet_name.sha256"
  sha256sum -c "$manifest_name.sha256"
)

printf 'release manifest: %s\n' "$manifest_path"
