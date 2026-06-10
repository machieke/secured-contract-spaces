#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-audit-readiness-$(git rev-parse --short HEAD)}"
safe_version="$(printf '%s' "$version" | tr -c 'A-Za-z0-9._-' '-')"
target_triple="${DETTA_TARGET_TRIPLE:-$(rustc -vV | awk '/^host:/ {print $2}')}"
source_date_epoch="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}"
out_dir="${DETTA_RELEASE_OUT:-$repo_root/dist}"
chain_id="${DETTA_RELEASE_CHAIN_ID:-detta-local}"
git_commit="$(git rev-parse HEAD)"
source_state_json="$(scripts/detta-source-state-report.sh)"
source_worktree_clean="$(jq -r '.worktree_clean' <<<"$source_state_json")"

for tool in jq sha256sum tar git awk wc; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool missing: $tool" >&2
    exit 1
  fi
done

case "${DETTA_REQUIRE_CLEAN_AUDIT_PACKAGE:-0}" in
  1 | true | TRUE | yes | YES)
    if [ "$source_worktree_clean" != "true" ]; then
      echo "audit package source tree is dirty; commit or stash changes before final audit packaging" >&2
      echo "$source_state_json" >&2
      exit 1
    fi
    ;;
esac

DETTA_RELEASE_VERSION="$version" \
  DETTA_RELEASE_OUT="$out_dir" \
  DETTA_RELEASE_CHAIN_ID="$chain_id" \
  scripts/detta-package-release.sh >/dev/null

archive_name="detta-node-${safe_version}-${target_triple}.tar.gz"
genesis_name="detta-demo-genesis-${safe_version}.json"
faucet_name="detta-faucet-sample-${safe_version}.json"
release_manifest_name="detta-release-${safe_version}.json"
archive_path="$out_dir/$archive_name"
genesis_path="$out_dir/$genesis_name"
faucet_path="$out_dir/$faucet_name"
release_manifest_path="$out_dir/$release_manifest_name"
report_name="detta-audit-readiness-${safe_version}.json"
report_path="$out_dir/$report_name"
package_name="detta-audit-readiness-package-${safe_version}.tar.gz"
package_path="$out_dir/$package_name"

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

tracked_inventory="$tmpdir/tracked-inventory.jsonl"
generated_inventory="$tmpdir/generated-inventory.jsonl"
stage="$tmpdir/package"
mkdir -p "$stage/source" "$stage/release" "$stage/audit"

category_for_path() {
  local path="$1"
  case "$path" in
    crates/*) printf 'source\n' ;;
    scripts/*) printf 'release_script\n' ;;
    models/*) printf 'formal_or_proof_artifact\n' ;;
    docs/*|*.md) printf 'documentation\n' ;;
    ops/*) printf 'readiness_manifest\n' ;;
    security/*) printf 'security_manifest\n' ;;
    Cargo.toml|Cargo.lock|deny.toml) printf 'build_or_dependency_config\n' ;;
    *) printf 'repository_file\n' ;;
  esac
}

git ls-files | while IFS= read -r path; do
  if [ ! -f "$path" ]; then
    continue
  fi
  sha="$(sha256sum "$path" | awk '{print $1}')"
  bytes="$(wc -c <"$path" | tr -d '[:space:]')"
  category="$(category_for_path "$path")"
  jq -cn \
    --arg path "$path" \
    --arg category "$category" \
    --arg sha "$sha" \
    --argjson bytes "$bytes" \
    '{path: $path, category: $category, sha256: $sha, bytes: $bytes}'
done >"$tracked_inventory"

tracked_count="$(wc -l <"$tracked_inventory" | tr -d '[:space:]')"
tracked_inventory_sha="$(sha256sum "$tracked_inventory" | awk '{print $1}')"

git ls-files -z |
  tar --null --files-from - --create --file - |
  tar --extract --file - --directory "$stage/source"

for generated in \
  "$archive_name" "$archive_name.sha256" \
  "$genesis_name" "$genesis_name.sha256" \
  "$faucet_name" "$faucet_name.sha256" \
  "$release_manifest_name" "$release_manifest_name.sha256"; do
  source_path="$out_dir/$generated"
  if [ ! -f "$source_path" ]; then
    echo "generated audit evidence missing: $source_path" >&2
    exit 1
  fi
  install -m 0644 "$source_path" "$stage/release/$generated"
  sha="$(sha256sum "$source_path" | awk '{print $1}')"
  bytes="$(wc -c <"$source_path" | tr -d '[:space:]')"
  jq -cn \
    --arg path "release/$generated" \
    --arg sha "$sha" \
    --argjson bytes "$bytes" \
    '{path: $path, category: "generated_release_artifact", sha256: $sha, bytes: $bytes}'
done >"$generated_inventory"

generated_count="$(wc -l <"$generated_inventory" | tr -d '[:space:]')"
generated_inventory_sha="$(sha256sum "$generated_inventory" | awk '{print $1}')"

if ! jq -e '
  .schema == "detta.audit-findings.v1"
  and .schema_version == 1
  and .project == "DeTTa"
  and (.categories | type == "array" and length >= 4)
  and (.findings | type == "array")
  and ([.findings[] | select(.status == "Open" or .status == "InRemediation")] | length == 0)
' security/detta-audit-findings.json >/dev/null; then
  echo "audit findings manifest is not ready for audit packaging" >&2
  cat security/detta-audit-findings.json >&2
  exit 1
fi

audit_findings_sha="$(sha256sum security/detta-audit-findings.json | awk '{print $1}')"
proof_manifest_sha="$(sha256sum models/detta-proof-artifact-manifest.json | awk '{print $1}')"
release_manifest_sha="$(sha256sum "$release_manifest_path" | awk '{print $1}')"

jq -n -e \
  --arg schema "detta.audit-readiness-package.v1" \
  --arg project "DeTTa" \
  --arg version "$safe_version" \
  --arg source_version "$version" \
  --arg target "$target_triple" \
  --arg chain_id "$chain_id" \
  --arg git_commit "$git_commit" \
  --argjson source_state "$source_state_json" \
  --arg release_manifest "release/$release_manifest_name" \
  --arg release_manifest_sha "$release_manifest_sha" \
  --arg tracked_inventory_sha "$tracked_inventory_sha" \
  --arg generated_inventory_sha "$generated_inventory_sha" \
  --arg audit_findings_sha "$audit_findings_sha" \
  --arg proof_manifest_sha "$proof_manifest_sha" \
  --arg package "$package_name" \
  --arg package_sha256_path "$package_name.sha256" \
  --arg status "passed" \
  --argjson source_date_epoch "$source_date_epoch" \
  --argjson tracked_count "$tracked_count" \
  --argjson generated_count "$generated_count" \
  --slurpfile tracked_inventory "$tracked_inventory" \
  --slurpfile generated_inventory "$generated_inventory" \
  --slurpfile audit_findings security/detta-audit-findings.json \
  '{
    schema: $schema,
    schema_version: 1,
    project: $project,
    version: $version,
    source_version: $source_version,
    git_commit: $git_commit,
    source_state: $source_state,
    target: $target,
    chain_id: $chain_id,
    source_date_epoch: $source_date_epoch,
    release_manifest: {
      path: $release_manifest,
      sha256: $release_manifest_sha
    },
    package: {
      path: $package,
      sha256_path: $package_sha256_path
    },
    source_inventory: {
      count: $tracked_count,
      sha256: $tracked_inventory_sha,
      files: $tracked_inventory
    },
    generated_release_inventory: {
      count: $generated_count,
      sha256: $generated_inventory_sha,
      files: $generated_inventory
    },
    security_findings: {
      path: "security/detta-audit-findings.json",
      sha256: $audit_findings_sha,
      categories: $audit_findings[0].categories,
      finding_count: ($audit_findings[0].findings | length)
    },
    proof_artifacts: {
      manifest_path: "models/detta-proof-artifact-manifest.json",
      manifest_sha256: $proof_manifest_sha
    },
    review_scope: [
      "secured_contract_spaces_specification",
      "restricted_metta_aspect_language",
      "aspect_parser_verifier_and_runtime",
      "guarded_kernel_host_calls",
      "deterministic_executor_and_state_roots",
      "consensus_networking_state_sync_and_rpc",
      "defi_token_amm_oracle_lending_staking_bridge_governance_flows",
      "operator_release_drills_and_readiness_manifests"
    ],
    status: $status
  }' >"$report_path"

install -m 0644 "$report_path" "$stage/audit/$report_name"
install -m 0644 "$tracked_inventory" "$stage/audit/tracked-inventory.jsonl"
install -m 0644 "$generated_inventory" "$stage/audit/generated-release-inventory.jsonl"

tar \
  --sort=name \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  --mtime="@$source_date_epoch" \
  --use-compress-program='gzip -n' \
  -cf "$package_path" \
  -C "$stage" \
  .

(
  cd "$out_dir"
  sha256sum "$package_name" >"$package_name.sha256"
  sha256sum "$report_name" >"$report_name.sha256"
  sha256sum -c "$package_name.sha256"
  sha256sum -c "$report_name.sha256"
)

scripts/detta-verify-audit-readiness-package.sh "$package_path"

printf 'audit readiness report: %s\n' "$report_path"
printf 'audit readiness package: %s\n' "$package_path"
