#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
  echo "readiness status report failed: $*" >&2
  exit 1
}

for tool in awk git jq sha256sum sort wc; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    fail "required tool missing: $tool"
  fi
done

public_manifest="ops/detta-public-testnet-readiness.json"
mainnet_manifest="ops/detta-mainnet-candidate-readiness.json"
audit_manifest="security/detta-audit-findings.json"
out_path="${1:-${DETTA_READINESS_STATUS_OUT:-}}"

case "${DETTA_SKIP_READINESS_VERIFY:-0}" in
  1 | true | TRUE | yes | YES) ;;
  *) scripts/detta-verify-readiness-manifests.sh >/dev/null ;;
esac

source_state_json="$(scripts/detta-source-state-report.sh)"
git_commit="$(git rev-parse HEAD)"
source_date_epoch="${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}"
public_sha="$(sha256sum "$public_manifest" | awk '{print $1}')"
mainnet_sha="$(sha256sum "$mainnet_manifest" | awk '{print $1}')"
audit_sha="$(sha256sum "$audit_manifest" | awk '{print $1}')"

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

evidence_paths="$tmpdir/readiness-evidence-paths.txt"
evidence_inventory="$tmpdir/readiness-evidence-inventory.jsonl"

{
  jq -r '.evidence[]?' "$public_manifest"
  jq -r '.evidence[]?' "$mainnet_manifest"
  printf '%s\n' \
    "$public_manifest" \
    "$mainnet_manifest" \
    "$audit_manifest" \
    "scripts/detta-verify-readiness-manifests.sh" \
    "scripts/detta-readiness-status-report.sh"
} | sort -u >"$evidence_paths"

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

while IFS= read -r path; do
  [ -n "$path" ] || continue
  [ -e "$path" ] || fail "evidence path is missing after verification: $path"
  sha="$(sha256sum "$path" | awk '{print $1}')"
  bytes="$(wc -c <"$path" | tr -d '[:space:]')"
  category="$(category_for_path "$path")"
  jq -cn \
    --arg path "$path" \
    --arg category "$category" \
    --arg sha "$sha" \
    --argjson bytes "$bytes" \
    '{path: $path, category: $category, sha256: $sha, bytes: $bytes}'
done <"$evidence_paths" >"$evidence_inventory"

evidence_count="$(wc -l <"$evidence_inventory" | tr -d '[:space:]')"
evidence_inventory_sha="$(sha256sum "$evidence_inventory" | awk '{print $1}')"

render_report() {
  jq -n \
    --arg schema "detta.readiness-status-report.v1" \
    --arg project "DeTTa" \
    --arg git_commit "$git_commit" \
    --arg public_manifest "$public_manifest" \
    --arg mainnet_manifest "$mainnet_manifest" \
    --arg audit_manifest "$audit_manifest" \
    --arg public_sha "$public_sha" \
    --arg mainnet_sha "$mainnet_sha" \
    --arg audit_sha "$audit_sha" \
    --arg evidence_inventory_sha "$evidence_inventory_sha" \
    --argjson source_date_epoch "$source_date_epoch" \
    --argjson source_state "$source_state_json" \
    --argjson evidence_count "$evidence_count" \
    --slurpfile public "$public_manifest" \
    --slurpfile mainnet "$mainnet_manifest" \
    --slurpfile audit "$audit_manifest" \
    --slurpfile evidence "$evidence_inventory" \
    '{
      schema: $schema,
      schema_version: 1,
      project: $project,
      git_commit: $git_commit,
      source_state: $source_state,
      source_date_epoch: $source_date_epoch,
      manifests: {
        public_testnet: {
          path: $public_manifest,
          sha256: $public_sha,
          ready: $public[0].ready_for_public_testnet,
          blockers: $public[0].blockers
        },
        mainnet_candidate: {
          path: $mainnet_manifest,
          sha256: $mainnet_sha,
          ready: $mainnet[0].ready_for_mainnet,
          blockers: $mainnet[0].blockers,
          release_candidate_id: $mainnet[0].release_candidate_id,
          satisfied_gates: $mainnet[0].satisfied_gates,
          required_gates: $mainnet[0].required_gates
        },
        audit_findings: {
          path: $audit_manifest,
          sha256: $audit_sha,
          finding_count: ($audit[0].findings | length),
          unresolved_count: ([$audit[0].findings[]? | select(.status == "Open" or .status == "InRemediation")] | length)
        }
      },
      evidence_inventory: {
        count: $evidence_count,
        sha256: $evidence_inventory_sha,
        files: $evidence
      },
      status:
        (if $mainnet[0].ready_for_mainnet then "mainnet_candidate_ready"
         elif $public[0].ready_for_public_testnet then "public_testnet_ready"
         else "blocked"
         end),
      blockers:
        (([$public[0].blockers[]? | {scope: "public_testnet", blocker: .}])
         + ([$mainnet[0].blockers[]? | {scope: "mainnet_candidate", blocker: .}])
         + ([$audit[0].findings[]? | select(.status == "Open" or .status == "InRemediation") | {
             scope: "external_audit",
             finding_id: .id,
             severity: .severity,
             status: .status
           }])),
      verification_commands: [
        "scripts/detta-verify-readiness-manifests.sh",
        "DETTA_E2E_FULL=1 DETTA_REQUIRE_DEP_AUDIT=1 scripts/detta-release-gate.sh"
      ]
    }'
}

if [ -n "$out_path" ]; then
  mkdir -p "$(dirname "$out_path")"
  render_report >"$out_path"
  sha256sum "$out_path" >"$out_path.sha256"
  printf 'readiness status report: %s\n' "$out_path"
else
  render_report
fi
