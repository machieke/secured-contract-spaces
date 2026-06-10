#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fail() {
  echo "readiness manifest verification failed: $*" >&2
  exit 1
}

for tool in awk jq sha256sum cargo; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    fail "required tool missing: $tool"
  fi
done

public_manifest="ops/detta-public-testnet-readiness.json"
mainnet_manifest="ops/detta-mainnet-candidate-readiness.json"
audit_manifest="security/detta-audit-findings.json"

for manifest in "$public_manifest" "$mainnet_manifest" "$audit_manifest"; do
  [ -f "$manifest" ] || fail "missing manifest: $manifest"
  jq empty "$manifest" || fail "invalid JSON: $manifest"
done

require_json() {
  local manifest="$1"
  local filter="$2"
  local message="$3"
  if ! jq -e "$filter" "$manifest" >/dev/null; then
    fail "$manifest: $message"
  fi
}

require_json "$public_manifest" \
  '.schema == "detta.public-testnet-readiness.v1" and .schema_version == 1 and .project == "DeTTa"' \
  "schema, version, or project mismatch"
require_json "$mainnet_manifest" \
  '.schema == "detta.mainnet-candidate-readiness.v1" and .schema_version == 1 and .project == "DeTTa"' \
  "schema, version, or project mismatch"
require_json "$audit_manifest" \
  '.schema == "detta.audit-findings.v1" and .schema_version == 1 and .project == "DeTTa"' \
  "schema, version, or project mismatch"

require_json "$public_manifest" \
  '.release_gate_command == "DETTA_E2E_FULL=1 DETTA_REQUIRE_DEP_AUDIT=1 scripts/detta-release-gate.sh"' \
  "unexpected release gate command"
require_json "$mainnet_manifest" \
  '.release_gate_command == "DETTA_E2E_FULL=1 DETTA_REQUIRE_DEP_AUDIT=1 scripts/detta-release-gate.sh"' \
  "unexpected release gate command"
require_json "$public_manifest" \
  '(.ready_for_public_testnet | type == "boolean")
  and (.blockers | type == "array")
  and ((.ready_for_public_testnet and (.blockers | length == 0))
       or ((.ready_for_public_testnet | not) and (.blockers | length > 0)))' \
  "public-testnet readiness and blocker state are inconsistent"
require_json "$mainnet_manifest" \
  '(.ready_for_mainnet | type == "boolean")
  and (.blockers | type == "array")
  and ((.ready_for_mainnet and (.blockers | length == 0))
       or ((.ready_for_mainnet | not) and (.blockers | length > 0)))' \
  "mainnet readiness and blocker state are inconsistent"
require_json "$audit_manifest" \
  '([.findings[]? | select(.status == "Open" or .status == "InRemediation")] | length) == 0' \
  "audit findings must be closed or accepted before packaging"

require_relative_path() {
  local path="$1"
  case "$path" in
    ""|/*|*"/../"*|"../"*|*".."|*\\*)
      fail "unsafe readiness evidence path: $path"
      ;;
  esac
  [ -e "$path" ] || fail "readiness evidence path is missing: $path"
}

check_manifest_evidence() {
  local manifest="$1"
  jq -r '.evidence[]?' "$manifest" | while IFS= read -r path; do
    require_relative_path "$path"
  done
}

check_manifest_evidence "$public_manifest"
check_manifest_evidence "$mainnet_manifest"

jq -c '.signed_artifacts[]?' "$mainnet_manifest" | while IFS= read -r artifact; do
  path="$(jq -r '.path' <<<"$artifact")"
  signature_path="$(jq -r '.signature_path' <<<"$artifact")"
  expected_sha="$(jq -r '.sha256' <<<"$artifact")"
  require_relative_path "$path"
  require_relative_path "$signature_path"
  actual_sha="$(sha256sum "$path" | awk '{print $1}')"
  [ "$actual_sha" = "$expected_sha" ] ||
    fail "signed artifact checksum mismatch for $path: expected $expected_sha, got $actual_sha"
done

cargo test -p detta-verify readiness -- --nocapture

printf 'readiness manifests verified: %s %s\n' "$public_manifest" "$mainnet_manifest"
