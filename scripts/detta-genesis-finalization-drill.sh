#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-genesis-finalization-$(git rev-parse --short HEAD)}"
safe_version="$(printf '%s' "$version" | tr -c 'A-Za-z0-9._-' '-')"
target_triple="${DETTA_TARGET_TRIPLE:-$(rustc -vV | awk '/^host:/ {print $2}')}"
out_dir="${DETTA_RELEASE_OUT:-$repo_root/dist}"
chain_id="${DETTA_RELEASE_CHAIN_ID:-detta-local}"
validators_csv="${DETTA_FINAL_GENESIS_VALIDATORS:-validator-1,validator-2,validator-3,validator-4}"
min_validators="${DETTA_FINAL_GENESIS_MIN_VALIDATORS:-4}"

for tool in jq sha256sum awk; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool missing: $tool" >&2
    exit 1
  fi
done

if ! [[ "$min_validators" =~ ^[0-9]+$ ]] || [ "$min_validators" -lt 1 ]; then
  echo "DETTA_FINAL_GENESIS_MIN_VALIDATORS must be a positive integer" >&2
  exit 1
fi

DETTA_RELEASE_VERSION="$version" \
  DETTA_RELEASE_OUT="$out_dir" \
  DETTA_RELEASE_CHAIN_ID="$chain_id" \
  scripts/detta-package-release.sh >/dev/null

archive_name="detta-node-${safe_version}-${target_triple}.tar.gz"
genesis_name="detta-demo-genesis-${safe_version}.json"
faucet_name="detta-faucet-sample-${safe_version}.json"
manifest_name="detta-release-${safe_version}.json"
archive_path="$out_dir/$archive_name"
genesis_path="$out_dir/$genesis_name"
faucet_path="$out_dir/$faucet_name"
manifest_path="$out_dir/$manifest_name"
report_name="detta-genesis-finalization-${safe_version}.json"
report_path="$out_dir/$report_name"

for path in "$archive_path" "$genesis_path" "$faucet_path" "$manifest_path"; do
  if [ ! -f "$path" ]; then
    echo "required release artifact missing: $path" >&2
    exit 1
  fi
done

(
  cd "$out_dir"
  sha256sum -c "$archive_name.sha256"
  sha256sum -c "$genesis_name.sha256"
  sha256sum -c "$faucet_name.sha256"
  sha256sum -c "$manifest_name.sha256"
) >/dev/null

archive_sha="$(awk '{print $1}' "$archive_path.sha256")"
genesis_sha="$(awk '{print $1}' "$genesis_path.sha256")"
faucet_sha="$(awk '{print $1}' "$faucet_path.sha256")"
manifest_sha="$(awk '{print $1}' "$manifest_path.sha256")"

if ! jq -e \
  --arg chain_id "$chain_id" \
  --arg genesis "$genesis_name" \
  --arg genesis_sha "$genesis_sha" \
  --arg faucet "$faucet_name" \
  --arg faucet_sha "$faucet_sha" \
  '.schema == "detta.release-artifacts.v1"
    and .chain_id == $chain_id
    and any(.artifacts[]; .kind == "demo_genesis" and .path == $genesis and .sha256 == $genesis_sha)
    and any(.artifacts[]; .kind == "sample_faucet_transaction" and .path == $faucet and .sha256 == $faucet_sha)' \
  "$manifest_path" >/dev/null; then
  echo "release manifest does not bind genesis and faucet artifacts as expected" >&2
  cat "$manifest_path" >&2
  exit 1
fi

validators_json="$(
  printf '%s' "$validators_csv" |
    jq -R 'split(",") | map(gsub("^\\s+|\\s+$"; "")) | map(select(length > 0))'
)"
validator_count="$(jq 'length' <<<"$validators_json")"
unique_validator_count="$(jq 'unique | length' <<<"$validators_json")"
if [ "$validator_count" -lt "$min_validators" ]; then
  echo "genesis finalization requires at least $min_validators validators, got $validator_count" >&2
  exit 1
fi
if [ "$validator_count" -ne "$unique_validator_count" ]; then
  echo "genesis finalization validators must be unique" >&2
  jq -r '.[]' <<<"$validators_json" >&2
  exit 1
fi

quorum="${DETTA_FINAL_GENESIS_QUORUM:-$(((validator_count * 2) / 3 + 1))}"
if ! [[ "$quorum" =~ ^[0-9]+$ ]] || [ "$quorum" -lt 1 ]; then
  echo "DETTA_FINAL_GENESIS_QUORUM must be a positive integer" >&2
  exit 1
fi
if [ "$quorum" -gt "$validator_count" ]; then
  echo "genesis quorum $quorum exceeds validator count $validator_count" >&2
  exit 1
fi

if ! jq -e '
  (.global_state_root | type == "string" and test("^[a-f0-9]{64}$"))
  and (.storage_root | type == "string" and test("^[a-f0-9]{64}$"))
  and (.registry_root | type == "string" and test("^[a-f0-9]{64}$"))
  and (.policy_root | type == "string" and test("^[a-f0-9]{64}$"))
  and (.event_root | type == "string" and test("^[a-f0-9]{64}$"))
  and (.nonce_root | type == "string" and test("^[a-f0-9]{64}$"))
  and (.outbox_root | type == "string" and test("^[a-f0-9]{64}$"))
' "$genesis_path" >/dev/null; then
  echo "genesis snapshot is missing required authenticated roots" >&2
  cat "$genesis_path" >&2
  exit 1
fi

jq -n -e \
  --arg schema "detta.genesis-finalization.v1" \
  --arg project "DeTTa" \
  --arg version "$safe_version" \
  --arg source_version "$version" \
  --arg target "$target_triple" \
  --arg chain_id "$chain_id" \
  --arg archive "$archive_name" \
  --arg archive_sha "$archive_sha" \
  --arg genesis "$genesis_name" \
  --arg genesis_sha "$genesis_sha" \
  --arg faucet "$faucet_name" \
  --arg faucet_sha "$faucet_sha" \
  --arg manifest "$manifest_name" \
  --arg manifest_sha "$manifest_sha" \
  --argjson validators "$validators_json" \
  --argjson validator_count "$validator_count" \
  --argjson quorum "$quorum" \
  --argjson max_byzantine_faults "$(((validator_count - 1) / 3))" \
  --slurpfile genesis_snapshot "$genesis_path" \
  '{
    schema: $schema,
    schema_version: 1,
    project: $project,
    version: $version,
    source_version: $source_version,
    target: $target,
    chain_id: $chain_id,
    release_manifest: {
      path: $manifest,
      sha256: $manifest_sha
    },
    release_archive: {
      path: $archive,
      sha256: $archive_sha,
      included_binaries: ["detta-node", "detta-client"]
    },
    genesis: {
      path: $genesis,
      sha256: $genesis_sha,
      global_state_root: $genesis_snapshot[0].global_state_root,
      storage_root: $genesis_snapshot[0].storage_root,
      registry_root: $genesis_snapshot[0].registry_root,
      policy_root: $genesis_snapshot[0].policy_root,
      event_root: $genesis_snapshot[0].event_root,
      nonce_root: $genesis_snapshot[0].nonce_root,
      outbox_root: $genesis_snapshot[0].outbox_root
    },
    faucet_sample: {
      path: $faucet,
      sha256: $faucet_sha
    },
    validator_set: {
      validator_count: $validator_count,
      quorum: $quorum,
      max_byzantine_faults: $max_byzantine_faults,
      validators: $validators
    },
    required_publication_steps: [
      "sign_genesis_checksum_with_release_key",
      "publish_release_manifest_and_genesis_finalization_report",
      "collect_validator_operator_acknowledgements",
      "pin_report_hash_in_public_testnet_or_mainnet_readiness_manifest"
    ],
    status: "passed"
  }' >"$report_path"

(
  cd "$out_dir"
  sha256sum "$report_name" >"$report_name.sha256"
  sha256sum -c "$report_name.sha256"
)

printf 'genesis finalization report: %s\n' "$report_path"
