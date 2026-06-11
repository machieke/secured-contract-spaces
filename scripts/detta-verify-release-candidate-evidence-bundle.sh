#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

usage() {
  echo "usage: scripts/detta-verify-release-candidate-evidence-bundle.sh <detta-release-candidate-evidence.tar.gz>" >&2
}

fail() {
  echo "release candidate evidence verification failed: $*" >&2
  exit 1
}

if [ "$#" -ne 1 ]; then
  usage
  exit 1
fi

input_bundle="$1"
if [ ! -f "$input_bundle" ]; then
  fail "bundle not found: $input_bundle"
fi

bundle_dir="$(cd "$(dirname "$input_bundle")" && pwd)"
bundle_name="$(basename "$input_bundle")"
bundle_path="$bundle_dir/$bundle_name"

for tool in cp gpg jq sha256sum tar awk wc cmp; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    fail "required tool missing: $tool"
  fi
done

sha256_of_file() {
  sha256sum "$1" | awk '{print $1}'
}

bytes_of_file() {
  wc -c <"$1" | tr -d '[:space:]'
}

require_safe_relative_path() {
  local path="$1"
  case "$path" in
    "" | /* | .. | ../* | */../* | */.. | *\\*)
      fail "unsafe relative path: $path"
      ;;
  esac
}

verify_sha256sum_file_if_present() {
  local sha_path="$1"
  if [ ! -f "$sha_path" ]; then
    return
  fi
  (
    cd "$(dirname "$sha_path")"
    sha256sum -c "$(basename "$sha_path")" >/dev/null
  )
}

verify_sha256sum_file_if_present "$bundle_path.sha256"

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

entries="$tmpdir/tar-entries.txt"
extract_dir="$tmpdir/extract"
mkdir -p "$extract_dir"

tar -tzf "$bundle_path" >"$entries"
while IFS= read -r entry || [ -n "$entry" ]; do
  case "$entry" in
    "." | "./")
      continue
      ;;
    ./*)
      relative="${entry#./}"
      ;;
    *)
      fail "unsafe archive entry: $entry"
      ;;
  esac
  require_safe_relative_path "$relative"
done <"$entries"

tar -xzf "$bundle_path" -C "$extract_dir"

shopt -s nullglob
reports=("$extract_dir"/reports/detta-release-candidate-evidence-*.json)
inventories=("$extract_dir"/reports/detta-release-candidate-evidence-*.jsonl)
shopt -u nullglob
if [ "${#reports[@]}" -ne 1 ]; then
  fail "expected exactly one release candidate evidence report, found ${#reports[@]}"
fi
if [ "${#inventories[@]}" -ne 1 ]; then
  fail "expected exactly one release candidate evidence inventory, found ${#inventories[@]}"
fi

report_path="${reports[0]}"
inventory_path="${inventories[0]}"
report_name="$(basename "$report_path")"
external_report_path="$bundle_dir/$report_name"
if [ -f "$external_report_path" ] && ! cmp -s "$external_report_path" "$report_path"; then
  fail "external report does not match packaged report: $report_name"
fi
verify_sha256sum_file_if_present "$external_report_path.sha256"
verify_sha256sum_file_if_present "$bundle_dir/$(basename "$inventory_path").sha256"

if ! jq -e '
  .schema == "detta.release-candidate-evidence.v1"
  and .schema_version == 1
  and .project == "DeTTa"
  and .status == "passed"
  and (.version | type == "string" and length > 0)
  and (.target | type == "string" and length > 0)
  and (.chain_id | type == "string" and length > 0)
  and (.source_state.schema == "detta.source-state.v1")
  and (.source_state.schema_version == 1)
  and (.source_state.git_commit | type == "string" and length > 0)
  and (.source_state.git_tree | type == "string" and test("^([0-9a-f]{40}|[0-9a-f]{64})$"))
  and (.source_state.worktree_clean | type == "boolean")
  and (.source_state.tracked_change_count | type == "number")
  and (.source_state.untracked_file_count | type == "number")
  and (.source_state.status_porcelain_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.source_state.tracked_diff_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.source_state.staged_diff_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.source_state.unstaged_diff_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.release_manifest.path | type == "string" and length > 0)
  and (.release_manifest.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.readiness_status_report.path | type == "string" and length > 0)
  and (.readiness_status_report.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.evidence_inventory.count | type == "number")
  and (.evidence_inventory.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.evidence_inventory.files | type == "array")
  and ((.evidence_inventory.files | length) == .evidence_inventory.count)
  and (.bundle.path | type == "string" and length > 0)
  and (.bundle.sha256_path | type == "string" and length > 0)
  and (.retained_drills | type == "array" and length >= 9)
' "$report_path" >/dev/null; then
  fail "release candidate evidence report schema is invalid"
fi

reported_bundle="$(jq -r '.bundle.path' "$report_path")"
if [ "$reported_bundle" != "$bundle_name" ]; then
  fail "report bundle path $reported_bundle does not match $bundle_name"
fi

case "${DETTA_REQUIRE_CLEAN_RELEASE_EVIDENCE:-0}" in
  1 | true | TRUE | yes | YES)
    if ! jq -e '.source_state.worktree_clean == true' "$report_path" >/dev/null; then
      fail "release candidate evidence source state is dirty"
    fi
    ;;
esac

inventory_sha="$(sha256_of_file "$inventory_path")"
expected_inventory_sha="$(jq -r '.evidence_inventory.sha256' "$report_path")"
if [ "$inventory_sha" != "$expected_inventory_sha" ]; then
  fail "evidence inventory hash mismatch"
fi
inventory_count="$(wc -l <"$inventory_path" | tr -d '[:space:]')"
expected_inventory_count="$(jq -r '.evidence_inventory.count' "$report_path")"
if [ "$inventory_count" != "$expected_inventory_count" ]; then
  fail "evidence inventory count mismatch: expected $expected_inventory_count got $inventory_count"
fi

while IFS= read -r line || [ -n "$line" ]; do
  if [ -z "$line" ]; then
    continue
  fi
  if ! jq -e '
    (.kind | type == "string" and length > 0)
    and (.path | type == "string" and length > 0)
    and (.bundle_path | type == "string" and length > 0)
    and (.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
    and (.bytes | type == "number")
    and (.bytes >= 0)
    and (.schema | type == "string")
    and (.status | type == "string")
  ' <<<"$line" >/dev/null; then
    fail "invalid evidence inventory entry: $line"
  fi

  kind="$(jq -r '.kind' <<<"$line")"
  bundle_file="$(jq -r '.bundle_path' <<<"$line")"
  expected_sha="$(jq -r '.sha256' <<<"$line")"
  expected_bytes="$(jq -r '.bytes' <<<"$line")"
  expected_status="$(jq -r '.status' <<<"$line")"
  require_safe_relative_path "$bundle_file"
  file="$extract_dir/$bundle_file"
  if [ ! -f "$file" ]; then
    fail "inventoried evidence file missing: $bundle_file"
  fi
  actual_sha="$(sha256_of_file "$file")"
  if [ "$actual_sha" != "$expected_sha" ]; then
    fail "inventoried evidence hash mismatch: $bundle_file"
  fi
  actual_bytes="$(bytes_of_file "$file")"
  if [ "$actual_bytes" != "$expected_bytes" ]; then
    fail "inventoried evidence byte count mismatch: $bundle_file"
  fi

  case "$kind" in
    retained_drill_report | audit_readiness_report)
      if [ "$expected_status" != "passed" ]; then
        fail "required passed evidence report did not pass: $bundle_file"
      fi
      ;;
  esac
done <"$inventory_path"

require_inventory_bundle_path() {
  local required_path="$1"
  if ! jq -s -e --arg path "$required_path" 'any(.[]; .bundle_path == $path)' "$inventory_path" >/dev/null; then
    fail "mandatory evidence missing from inventory: $required_path"
  fi
}

release_manifest_name="$(jq -r '.release_manifest.path' "$report_path")"
require_safe_relative_path "$release_manifest_name"
release_manifest_path="$extract_dir/release/$release_manifest_name"
if [ ! -f "$release_manifest_path" ]; then
  fail "release manifest missing from bundle: $release_manifest_name"
fi
if [ "$(sha256_of_file "$release_manifest_path")" != "$(jq -r '.release_manifest.sha256' "$report_path")" ]; then
  fail "release manifest hash mismatch"
fi
if ! jq -e --slurpfile report "$report_path" '
  .schema == "detta.release-artifacts.v1"
  and .source_state == $report[0].source_state
' "$release_manifest_path" >/dev/null; then
  fail "release manifest source state does not match evidence report"
fi

version="$(jq -r '.version' "$report_path")"
readiness_report_name="$(jq -r '.readiness_status_report.path' "$report_path")"
require_safe_relative_path "$readiness_report_name"
readiness_report_path="$extract_dir/reports/$readiness_report_name"
if [ ! -f "$readiness_report_path" ]; then
  fail "readiness status report missing from bundle: $readiness_report_name"
fi
if [ "$(sha256_of_file "$readiness_report_path")" != "$(jq -r '.readiness_status_report.sha256' "$report_path")" ]; then
  fail "readiness status report hash mismatch"
fi
if ! jq -e --slurpfile report "$report_path" '
  .source_state == $report[0].source_state
  and .git_commit == $report[0].source_state.git_commit
  and .source_date_epoch == $report[0].source_date_epoch
' "$readiness_report_path" >/dev/null; then
  fail "readiness status report source state does not match evidence report"
fi

require_inventory_bundle_path "release/detta-release-${version}.json"
require_inventory_bundle_path "reports/detta-readiness-status-${version}.json"
require_inventory_bundle_path "reports/detta-readiness-status-${version}.json.sha256"
require_inventory_bundle_path "reports/detta-launch-rehearsal-${version}.json"
require_inventory_bundle_path "reports/detta-genesis-finalization-${version}.json"
require_inventory_bundle_path "reports/detta-incident-response-drill-${version}.json"
require_inventory_bundle_path "reports/detta-governance-bootstrap-drill-${version}.json"
require_inventory_bundle_path "reports/detta-validator-onboarding-drill-${version}.json"
require_inventory_bundle_path "reports/detta-packaged-client-flow-drill-${version}.json"
require_inventory_bundle_path "reports/detta-public-testnet-stability-drill-${version}.json"
require_inventory_bundle_path "reports/detta-release-signing-drill-${version}.json"
require_inventory_bundle_path "audit/detta-audit-readiness-${version}.json"
require_inventory_bundle_path "audit/detta-audit-readiness-package-${version}.tar.gz"
require_inventory_bundle_path "signatures/detta-release-signing-drill-${version}.pub.asc"

signing_report_path="$extract_dir/reports/detta-release-signing-drill-${version}.json"
signing_public_key_path="$extract_dir/signatures/detta-release-signing-drill-${version}.pub.asc"
signer_fingerprint="$(jq -r '.signing_key.fingerprint // empty' "$signing_report_path")"
if [ -z "$signer_fingerprint" ]; then
  fail "release signing report missing signer fingerprint"
fi
if [ "$(sha256_of_file "$signing_public_key_path")" != "$(jq -r '.signing_key.public_key_sha256' "$signing_report_path")" ]; then
  fail "release signing public key hash does not match signing report"
fi

gpg_home="$tmpdir/gnupg-release-signature-verify"
mkdir -m 0700 "$gpg_home"
GNUPGHOME="$gpg_home" gpg --batch --import "$signing_public_key_path" >/dev/null
signature_verify_dir="$tmpdir/release-signature-verify"
mkdir -p "$signature_verify_dir"
cp "$extract_dir/release/"* "$signature_verify_dir/"
shopt -s nullglob
signature_files=("$extract_dir"/signatures/*.sig)
shopt -u nullglob
if [ "${#signature_files[@]}" -eq 0 ]; then
  fail "release signature files are missing from bundle"
fi
cp "${signature_files[@]}" "$signature_verify_dir/"

scripts/detta-verify-audit-readiness-package.sh \
  "$extract_dir/audit/detta-audit-readiness-package-${version}.tar.gz" >/dev/null
audit_source_extract_dir="$tmpdir/audit-source"
mkdir -p "$audit_source_extract_dir"
tar -xzf "$extract_dir/audit/detta-audit-readiness-package-${version}.tar.gz" \
  -C "$audit_source_extract_dir"
DETTA_READINESS_STATUS_BASE_DIR="$audit_source_extract_dir/source" \
  scripts/detta-verify-readiness-status-report.sh "$readiness_report_path" >/dev/null
audit_report_path="$extract_dir/audit/detta-audit-readiness-${version}.json"
if ! jq -e --slurpfile report "$report_path" '
  .source_state == $report[0].source_state
  and .git_commit == $report[0].source_state.git_commit
  and .source_date_epoch == $report[0].source_date_epoch
' "$audit_report_path" >/dev/null; then
  fail "audit readiness report source state does not match evidence report"
fi
GNUPGHOME="$gpg_home" \
  DETTA_RELEASE_SIGNER_FINGERPRINT="$signer_fingerprint" \
  scripts/detta-verify-release-signatures.sh \
  "$signature_verify_dir/detta-release-${version}.json" >/dev/null

printf 'release candidate evidence bundle verified: %s\n' "$bundle_path"
