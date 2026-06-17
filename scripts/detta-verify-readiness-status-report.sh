#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
  echo "readiness status report verification failed: $*" >&2
  exit 1
}

if [ "$#" -ne 1 ]; then
  echo "usage: scripts/detta-verify-readiness-status-report.sh <detta-readiness-status.json>" >&2
  exit 2
fi

for tool in awk jq sha256sum wc; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    fail "required tool missing: $tool"
  fi
done

report_path="$1"
[ -f "$report_path" ] || fail "missing report: $report_path"
base_dir="${DETTA_READINESS_STATUS_BASE_DIR:-$repo_root}"
[ -d "$base_dir" ] || fail "readiness status base directory is missing: $base_dir"
base_dir="$(cd "$base_dir" && pwd)"

if [ -f "$report_path.sha256" ]; then
  (
    cd "$(dirname "$report_path")"
    sha256sum -c "$(basename "$report_path").sha256" >/dev/null
  ) || fail "report checksum verification failed"
fi

jq empty "$report_path" || fail "invalid JSON: $report_path"

if ! jq -e '
  ((.schema == "detta.readiness-status-report.v1" and .schema_version == 1)
   or (.schema == "detta.readiness-status-report.v2" and .schema_version == 2))
  and .project == "DeTTa"
  and (.git_commit | type == "string" and length > 0)
  and (.source_state.schema == "detta.source-state.v1")
  and (.source_date_epoch | type == "number")
  and (.status == "blocked" or .status == "public_testnet_ready" or .status == "mainnet_candidate_ready")
  and (.manifests.public_testnet.path == "ops/detta-public-testnet-readiness.json")
  and (.manifests.mainnet_candidate.path == "ops/detta-mainnet-candidate-readiness.json")
  and (.manifests.audit_findings.path == "security/detta-audit-findings.json")
  and (if .schema_version == 2 then
      .formal_proof_artifacts.manifest.path == "models/detta-proof-artifact-manifest.json"
      and .formal_proof_artifacts.manifest.sha256_path == "models/detta-proof-artifact-manifest.sha256"
      and (.formal_proof_artifacts.manifest.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
      and (.formal_proof_artifacts.manifest.schema | type == "string" and length > 0)
      and (.formal_proof_artifacts.manifest.schema_version | type == "number")
      and (.formal_proof_artifacts.manifest.model_artifact_count | type == "number")
      and (.formal_proof_artifacts.manifest.runtime_artifact_count | type == "number")
      and (.formal_proof_artifacts.manifest.theorem_count | type == "number")
      and .data_availability_evidence.layer_document.path == "detta-data-availability-layer.md"
      and .data_availability_evidence.implementation_plan.path == "detta-data-availability-layer-implementation-plan.md"
      and .data_availability_evidence.protocol_source.path == "crates/detta-da/src/lib.rs"
      and .data_availability_evidence.incident_drill_script.path == "scripts/detta-da-incident-response-drill.sh"
      and .data_availability_evidence.stability_drill_script.path == "scripts/detta-da-stability-drill.sh"
      and (.data_availability_evidence.layer_document.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
      and (.data_availability_evidence.implementation_plan.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
      and (.data_availability_evidence.protocol_source.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
      and (.data_availability_evidence.incident_drill_script.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
      and (.data_availability_evidence.stability_drill_script.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
    else true end)
  and (.evidence_inventory.count | type == "number")
  and (.evidence_inventory.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.evidence_inventory.files | type == "array")
  and ((.evidence_inventory.files | length) == .evidence_inventory.count)
  and (.blockers | type == "array")
  and (.verification_commands | type == "array" and length >= 2)
' "$report_path" >/dev/null; then
  fail "report schema is invalid"
fi

repo_file_path() {
  local path="$1"
  case "$path" in
    ""|/*|*"/../"*|"../"*|*".."|*\\*)
      fail "unsafe report path: $path"
      ;;
  esac
  local resolved="$base_dir/$path"
  [ -e "$resolved" ] || fail "report-bound path is missing: $path under $base_dir"
  printf '%s\n' "$resolved"
}

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

verify_bound_file() {
  local path_filter="$1"
  local sha_filter="$2"
  local path
  local expected_sha
  local actual_sha
  local resolved_path
  path="$(jq -r "$path_filter" "$report_path")"
  expected_sha="$(jq -r "$sha_filter" "$report_path")"
  resolved_path="$(repo_file_path "$path")"
  actual_sha="$(sha256sum "$resolved_path" | awk '{print $1}')"
  [ "$actual_sha" = "$expected_sha" ] ||
    fail "checksum mismatch for $path: expected $expected_sha, got $actual_sha"
}

verify_bound_file '.manifests.public_testnet.path' '.manifests.public_testnet.sha256'
verify_bound_file '.manifests.mainnet_candidate.path' '.manifests.mainnet_candidate.sha256'
verify_bound_file '.manifests.audit_findings.path' '.manifests.audit_findings.sha256'
if [ "$(jq -r '.schema_version' "$report_path")" = "2" ]; then
  verify_bound_file '.formal_proof_artifacts.manifest.path' '.formal_proof_artifacts.manifest.sha256'
  verify_bound_file '.data_availability_evidence.layer_document.path' '.data_availability_evidence.layer_document.sha256'
  verify_bound_file '.data_availability_evidence.implementation_plan.path' '.data_availability_evidence.implementation_plan.sha256'
  verify_bound_file '.data_availability_evidence.protocol_source.path' '.data_availability_evidence.protocol_source.sha256'
  verify_bound_file '.data_availability_evidence.incident_drill_script.path' '.data_availability_evidence.incident_drill_script.sha256'
  verify_bound_file '.data_availability_evidence.stability_drill_script.path' '.data_availability_evidence.stability_drill_script.sha256'
fi

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

inventory_jsonl="$tmpdir/evidence-inventory.jsonl"
jq -c '.evidence_inventory.files[]' "$report_path" >"$inventory_jsonl"
actual_inventory_count="$(wc -l <"$inventory_jsonl" | tr -d '[:space:]')"
expected_inventory_count="$(jq -r '.evidence_inventory.count' "$report_path")"
[ "$actual_inventory_count" = "$expected_inventory_count" ] ||
  fail "evidence inventory count mismatch: expected $expected_inventory_count, got $actual_inventory_count"

actual_inventory_sha="$(sha256sum "$inventory_jsonl" | awk '{print $1}')"
expected_inventory_sha="$(jq -r '.evidence_inventory.sha256' "$report_path")"
[ "$actual_inventory_sha" = "$expected_inventory_sha" ] ||
  fail "evidence inventory checksum mismatch: expected $expected_inventory_sha, got $actual_inventory_sha"

jq -c '.evidence_inventory.files[]' "$report_path" | while IFS= read -r entry; do
  path="$(jq -r '.path' <<<"$entry")"
  category="$(jq -r '.category' <<<"$entry")"
  expected_sha="$(jq -r '.sha256' <<<"$entry")"
  expected_bytes="$(jq -r '.bytes' <<<"$entry")"
  resolved_path="$(repo_file_path "$path")"
  [ "$(category_for_path "$path")" = "$category" ] ||
    fail "category mismatch for evidence path: $path"
  actual_sha="$(sha256sum "$resolved_path" | awk '{print $1}')"
  [ "$actual_sha" = "$expected_sha" ] ||
    fail "checksum mismatch for evidence path $path: expected $expected_sha, got $actual_sha"
  actual_bytes="$(wc -c <"$resolved_path" | tr -d '[:space:]')"
  [ "$actual_bytes" = "$expected_bytes" ] ||
    fail "byte count mismatch for evidence path $path: expected $expected_bytes, got $actual_bytes"
done

require_inventory_path() {
  local required_path="$1"
  if ! jq -e --arg path "$required_path" '
    any(.evidence_inventory.files[]; .path == $path)
  ' "$report_path" >/dev/null; then
    fail "required evidence path missing from readiness inventory: $required_path"
  fi
}

public_manifest_path="$(repo_file_path ops/detta-public-testnet-readiness.json)"
mainnet_manifest_path="$(repo_file_path ops/detta-mainnet-candidate-readiness.json)"
audit_manifest_path="$(repo_file_path security/detta-audit-findings.json)"

if ! jq -e --slurpfile public "$public_manifest_path" '
  .manifests.public_testnet.ready == $public[0].ready_for_public_testnet
  and .manifests.public_testnet.blockers == $public[0].blockers
' "$report_path" >/dev/null; then
  fail "public-testnet manifest state does not match report"
fi

if ! jq -e --slurpfile mainnet "$mainnet_manifest_path" '
  .manifests.mainnet_candidate.ready == $mainnet[0].ready_for_mainnet
  and .manifests.mainnet_candidate.blockers == $mainnet[0].blockers
  and .manifests.mainnet_candidate.release_candidate_id == $mainnet[0].release_candidate_id
  and .manifests.mainnet_candidate.satisfied_gates == $mainnet[0].satisfied_gates
  and .manifests.mainnet_candidate.required_gates == $mainnet[0].required_gates
' "$report_path" >/dev/null; then
  fail "mainnet-candidate manifest state does not match report"
fi

if ! jq -e --slurpfile audit "$audit_manifest_path" '
  .manifests.audit_findings.finding_count == ($audit[0].findings | length)
  and .manifests.audit_findings.unresolved_count == ([$audit[0].findings[]? | select(.status == "Open" or .status == "InRemediation")] | length)
' "$report_path" >/dev/null; then
  fail "audit finding manifest state does not match report"
fi

if [ "$(jq -r '.schema_version' "$report_path")" = "2" ]; then
  proof_manifest_path="$(repo_file_path models/detta-proof-artifact-manifest.json)"
  proof_manifest_sha_path="$(repo_file_path models/detta-proof-artifact-manifest.sha256)"
  (
    cd "$(dirname "$proof_manifest_sha_path")"
    sha256sum -c "$(basename "$proof_manifest_sha_path")" >/dev/null
  ) || fail "proof artifact manifest checksum verification failed"

  require_inventory_path "models/detta-proof-artifact-manifest.json"
  require_inventory_path "models/detta-proof-artifact-manifest.sha256"
  require_inventory_path "detta-data-availability-layer.md"
  require_inventory_path "detta-data-availability-layer-implementation-plan.md"
  require_inventory_path "crates/detta-da/src/lib.rs"
  require_inventory_path "scripts/detta-da-incident-response-drill.sh"
  require_inventory_path "scripts/detta-da-stability-drill.sh"

  if ! jq -e --slurpfile proof "$proof_manifest_path" '
    .formal_proof_artifacts.manifest.schema == $proof[0].schema
    and .formal_proof_artifacts.manifest.schema_version == $proof[0].schema_version
    and .formal_proof_artifacts.manifest.model_artifact_count == ($proof[0].model_artifacts | length)
    and .formal_proof_artifacts.manifest.runtime_artifact_count == ($proof[0].runtime_artifacts | length)
    and .formal_proof_artifacts.manifest.theorem_count == $proof[0].theorem_count
  ' "$report_path" >/dev/null; then
    fail "formal proof artifact manifest state does not match report"
  fi

  while IFS= read -r artifact || [ -n "$artifact" ]; do
    [ -n "$artifact" ] || continue
    artifact_path="$(jq -r '.path' <<<"$artifact")"
    expected_sha="$(jq -r '.sha256' <<<"$artifact")"
    resolved_artifact_path="$(repo_file_path "$artifact_path")"
    require_inventory_path "$artifact_path"
    actual_sha="$(sha256sum "$resolved_artifact_path" | awk '{print $1}')"
    [ "$actual_sha" = "$expected_sha" ] ||
      fail "proof artifact checksum mismatch for $artifact_path: expected $expected_sha, got $actual_sha"
  done < <(jq -c '.model_artifacts[]?, .runtime_artifacts[]?' "$proof_manifest_path")
fi

if ! jq -e '
  .status ==
    (if .manifests.mainnet_candidate.ready then "mainnet_candidate_ready"
     elif .manifests.public_testnet.ready then "public_testnet_ready"
     else "blocked"
     end)
' "$report_path" >/dev/null; then
  fail "status does not match manifest readiness flags"
fi

if ! jq -e '
  (.verification_commands | index("scripts/detta-verify-readiness-manifests.sh") != null)
  and (.verification_commands | index("DETTA_E2E_FULL=1 DETTA_REQUIRE_DEP_AUDIT=1 scripts/detta-release-gate.sh") != null)
' "$report_path" >/dev/null; then
  fail "verification commands are incomplete"
fi

printf 'readiness status report verified: %s\n' "$report_path"
