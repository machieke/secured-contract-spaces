#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-release-candidate-$(git rev-parse --short HEAD)}"
safe_version="$(printf '%s' "$version" | tr -c 'A-Za-z0-9._-' '-')"
target_triple="${DETTA_TARGET_TRIPLE:-$(rustc -vV | awk '/^host:/ {print $2}')}"
source_date_epoch="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}"
out_dir="${DETTA_RELEASE_OUT:-$repo_root/dist}"
chain_id="${DETTA_RELEASE_CHAIN_ID:-detta-local}"

for tool in jq sha256sum tar git awk wc; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool missing: $tool" >&2
    exit 1
  fi
done

mkdir -p "$out_dir"

run_release_drill() {
  local label="$1"
  local script="$2"
  printf 'running retained release evidence drill: %s\n' "$label"
  DETTA_RELEASE_VERSION="$version" \
    DETTA_RELEASE_OUT="$out_dir" \
    DETTA_RELEASE_CHAIN_ID="$chain_id" \
    "$script"
}

run_release_drill launch scripts/detta-operator-launch-rehearsal.sh
run_release_drill genesis-finalization scripts/detta-genesis-finalization-drill.sh
run_release_drill incident-response scripts/detta-incident-response-drill.sh
run_release_drill governance-bootstrap scripts/detta-governance-bootstrap-drill.sh
run_release_drill validator-onboarding scripts/detta-validator-onboarding-drill.sh
run_release_drill packaged-client-flow scripts/detta-packaged-client-flow-drill.sh
run_release_drill public-testnet-stability scripts/detta-public-testnet-stability-drill.sh
run_release_drill audit-readiness scripts/detta-audit-readiness-package.sh
run_release_drill release-signing scripts/detta-release-signing-drill.sh

manifest_name="detta-release-${safe_version}.json"
manifest_path="$out_dir/$manifest_name"
report_name="detta-release-candidate-evidence-${safe_version}.json"
report_path="$out_dir/$report_name"
evidence_inventory_name="detta-release-candidate-evidence-${safe_version}.jsonl"
evidence_jsonl="$out_dir/$evidence_inventory_name"
bundle_name="detta-release-candidate-evidence-${safe_version}.tar.gz"
bundle_path="$out_dir/$bundle_name"
tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT
: >"$evidence_jsonl"

stage="$tmpdir/evidence"
mkdir -p "$stage/release" "$stage/reports" "$stage/audit" "$stage/signatures"

sha256_of_file() {
  sha256sum "$1" | awk '{print $1}'
}

bytes_of_file() {
  wc -c <"$1" | tr -d '[:space:]'
}

reject_unsafe_path() {
  local label="$1"
  local path="$2"
  case "$path" in
    "" | /* | .. | ../* | */../* | */.. | *\\*)
      echo "unsafe $label path in evidence bundle: ${path:-<empty>}" >&2
      exit 1
      ;;
  esac
}

verify_sha256sum_file_if_present() {
  local file="$1"
  local sha_file="$file.sha256"
  if [ ! -f "$sha_file" ]; then
    return
  fi
  (
    cd "$(dirname "$sha_file")"
    sha256sum -c "$(basename "$sha_file")" >/dev/null
  )
}

add_evidence_file() {
  local kind="$1"
  local source_path="$2"
  local bundle_dir="$3"
  local require_passed_status="$4"
  local base destination bundle_path_for_file sha bytes schema status

  if [ ! -f "$source_path" ]; then
    echo "evidence file missing: $source_path" >&2
    exit 1
  fi
  verify_sha256sum_file_if_present "$source_path"

  base="$(basename "$source_path")"
  destination="$stage/$bundle_dir/$base"
  install -m 0644 "$source_path" "$destination"
  bundle_path_for_file="$bundle_dir/$base"
  sha="$(sha256_of_file "$source_path")"
  bytes="$(bytes_of_file "$source_path")"
  schema=""
  status=""
  if jq empty "$source_path" >/dev/null 2>&1; then
    schema="$(jq -r '.schema // empty' "$source_path")"
    status="$(jq -r '.status // empty' "$source_path")"
    if [ "$require_passed_status" = "yes" ] && [ "$status" != "passed" ]; then
      echo "evidence report did not pass: $source_path" >&2
      cat "$source_path" >&2
      exit 1
    fi
  elif [ "$require_passed_status" = "yes" ]; then
    echo "evidence report is not JSON: $source_path" >&2
    exit 1
  fi

  jq -cn \
    --arg kind "$kind" \
    --arg path "$(basename "$source_path")" \
    --arg bundle_path "$bundle_path_for_file" \
    --arg sha "$sha" \
    --arg schema "$schema" \
    --arg status "$status" \
    --argjson bytes "$bytes" \
    '{
      kind: $kind,
      path: $path,
      bundle_path: $bundle_path,
      sha256: $sha,
      bytes: $bytes,
      schema: $schema,
      status: $status
    }' >>"$evidence_jsonl"
}

add_release_path_from_manifest() {
  local kind="$1"
  local manifest_relative_path="$2"
  local bundle_dir="$3"
  reject_unsafe_path "$kind" "$manifest_relative_path"
  add_evidence_file "$kind" "$out_dir/$manifest_relative_path" "$bundle_dir" no
}

if [ ! -f "$manifest_path" ]; then
  echo "release manifest missing after retained evidence drills: $manifest_path" >&2
  exit 1
fi
if ! jq -e '.schema == "detta.release-artifacts.v1" and .source_state.schema == "detta.source-state.v1"' \
  "$manifest_path" >/dev/null; then
  echo "release manifest missing expected source-state metadata" >&2
  cat "$manifest_path" >&2
  exit 1
fi

add_evidence_file release_manifest "$manifest_path" release no
add_release_path_from_manifest release_manifest_checksum \
  "$(jq -r '.manifest_sha256_path' "$manifest_path")" release
if [ -f "$out_dir/$(jq -r '.manifest_signature_path' "$manifest_path")" ]; then
  add_release_path_from_manifest release_manifest_signature \
    "$(jq -r '.manifest_signature_path' "$manifest_path")" signatures
fi

while IFS= read -r artifact; do
  artifact_path="$(jq -r '.path' <<<"$artifact")"
  checksum_path="$(jq -r '.sha256_path' <<<"$artifact")"
  signature_path="$(jq -r '.signature_path' <<<"$artifact")"
  add_release_path_from_manifest release_artifact "$artifact_path" release
  add_release_path_from_manifest release_artifact_checksum "$checksum_path" release
  if [ -f "$out_dir/$signature_path" ]; then
    add_release_path_from_manifest release_artifact_signature "$signature_path" signatures
  fi
done < <(jq -c '.artifacts[]' "$manifest_path")

for drill_report in \
  "detta-launch-rehearsal-${safe_version}.json" \
  "detta-genesis-finalization-${safe_version}.json" \
  "detta-incident-response-drill-${safe_version}.json" \
  "detta-governance-bootstrap-drill-${safe_version}.json" \
  "detta-validator-onboarding-drill-${safe_version}.json" \
  "detta-packaged-client-flow-drill-${safe_version}.json" \
  "detta-public-testnet-stability-drill-${safe_version}.json" \
  "detta-release-signing-drill-${safe_version}.json"; do
  add_evidence_file retained_drill_report "$out_dir/$drill_report" reports yes
  add_evidence_file retained_drill_report_checksum "$out_dir/$drill_report.sha256" reports no
done

audit_report="detta-audit-readiness-${safe_version}.json"
audit_package="detta-audit-readiness-package-${safe_version}.tar.gz"
add_evidence_file audit_readiness_report "$out_dir/$audit_report" audit yes
add_evidence_file audit_readiness_report_checksum "$out_dir/$audit_report.sha256" audit no
add_evidence_file audit_readiness_package "$out_dir/$audit_package" audit no
add_evidence_file audit_readiness_package_checksum "$out_dir/$audit_package.sha256" audit no

public_key="detta-release-signing-drill-${safe_version}.pub.asc"
if [ -f "$out_dir/$public_key" ]; then
  add_evidence_file release_signing_public_key "$out_dir/$public_key" signatures no
fi

source_state_json="$(jq -c '.source_state' "$manifest_path")"
evidence_count="$(wc -l <"$evidence_jsonl" | tr -d '[:space:]')"
evidence_inventory_sha="$(sha256_of_file "$evidence_jsonl")"

jq -n -e \
  --arg schema "detta.release-candidate-evidence.v1" \
  --arg project "DeTTa" \
  --arg version "$safe_version" \
  --arg source_version "$version" \
  --arg target "$target_triple" \
  --arg chain_id "$chain_id" \
  --arg release_manifest "$manifest_name" \
  --arg release_manifest_sha "$(sha256_of_file "$manifest_path")" \
  --arg evidence_inventory_sha "$evidence_inventory_sha" \
  --arg bundle "$bundle_name" \
  --arg status "passed" \
  --argjson source_date_epoch "$source_date_epoch" \
  --argjson evidence_count "$evidence_count" \
  --argjson source_state "$source_state_json" \
  --slurpfile evidence "$evidence_jsonl" \
  '{
    schema: $schema,
    schema_version: 1,
    project: $project,
    version: $version,
    source_version: $source_version,
    target: $target,
    chain_id: $chain_id,
    source_date_epoch: $source_date_epoch,
    source_state: $source_state,
    release_manifest: {
      path: $release_manifest,
      sha256: $release_manifest_sha
    },
    evidence_inventory: {
      count: $evidence_count,
      sha256: $evidence_inventory_sha,
      files: $evidence
    },
    bundle: {
      path: $bundle,
      sha256_path: ($bundle + ".sha256")
    },
    retained_drills: [
      "operator_launch_rehearsal",
      "genesis_finalization",
      "incident_response",
      "governance_bootstrap",
      "validator_onboarding",
      "packaged_client_flow",
      "public_testnet_stability",
      "audit_readiness",
      "release_signing"
    ],
    status: $status
  }' >"$report_path"

install -m 0644 "$report_path" "$stage/reports/$report_name"
install -m 0644 "$evidence_jsonl" "$stage/reports/$evidence_inventory_name"

tar \
  --sort=name \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  --mtime="@$source_date_epoch" \
  --use-compress-program='gzip -n' \
  -cf "$bundle_path" \
  -C "$stage" \
  .

(
  cd "$out_dir"
  sha256sum "$report_name" >"$report_name.sha256"
  sha256sum "$evidence_inventory_name" >"$evidence_inventory_name.sha256"
  sha256sum "$bundle_name" >"$bundle_name.sha256"
  sha256sum -c "$report_name.sha256"
  sha256sum -c "$evidence_inventory_name.sha256"
  sha256sum -c "$bundle_name.sha256"
)

printf 'release candidate evidence report: %s\n' "$report_path"
printf 'release candidate evidence bundle: %s\n' "$bundle_path"
