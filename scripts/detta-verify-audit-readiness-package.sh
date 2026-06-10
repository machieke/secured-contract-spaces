#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

usage() {
  echo "usage: scripts/detta-verify-audit-readiness-package.sh <detta-audit-readiness-package.tar.gz>" >&2
}

fail() {
  echo "audit readiness package verification failed: $*" >&2
  exit 1
}

if [ "$#" -ne 1 ]; then
  usage
  exit 1
fi

input_package="$1"
if [ ! -f "$input_package" ]; then
  fail "package not found: $input_package"
fi

package_dir="$(cd "$(dirname "$input_package")" && pwd)"
package_name="$(basename "$input_package")"
package_path="$package_dir/$package_name"

for tool in jq sha256sum tar awk wc cmp; do
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
  local sha_dir sha_name
  sha_dir="$(cd "$(dirname "$sha_path")" && pwd)"
  sha_name="$(basename "$sha_path")"
  (
    cd "$sha_dir"
    sha256sum -c "$sha_name" >/dev/null
  )
}

verify_inventory_hash_and_count() {
  local inventory="$1"
  local expected_sha="$2"
  local expected_count="$3"
  local actual_sha actual_count

  if [ ! -f "$inventory" ]; then
    fail "inventory missing: $inventory"
  fi

  actual_sha="$(sha256_of_file "$inventory")"
  if [ "$actual_sha" != "$expected_sha" ]; then
    fail "inventory hash mismatch for $inventory"
  fi

  actual_count="$(wc -l <"$inventory" | tr -d '[:space:]')"
  if [ "$actual_count" != "$expected_count" ]; then
    fail "inventory count mismatch for $inventory: expected $expected_count, got $actual_count"
  fi
}

verify_inventory_entries() {
  local inventory="$1"
  local root="$2"
  local prefix="$3"
  local line path relative file expected_sha actual_sha expected_bytes actual_bytes

  while IFS= read -r line || [ -n "$line" ]; do
    if [ -z "$line" ]; then
      continue
    fi
    if ! jq -e '
      (.path | type == "string" and length > 0)
      and (.category | type == "string" and length > 0)
      and (.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
      and (.bytes | type == "number")
      and (.bytes >= 0)
    ' <<<"$line" >/dev/null; then
      fail "invalid inventory entry: $line"
    fi

    path="$(jq -r '.path' <<<"$line")"
    require_safe_relative_path "$path"
    if [ -n "$prefix" ]; then
      case "$path" in
        "$prefix"/*) relative="${path#"$prefix"/}" ;;
        *) fail "inventory path $path does not use required prefix $prefix/" ;;
      esac
      require_safe_relative_path "$relative"
    else
      relative="$path"
    fi

    file="$root/$relative"
    if [ ! -f "$file" ]; then
      fail "inventoried file missing: $path"
    fi

    expected_sha="$(jq -r '.sha256' <<<"$line")"
    actual_sha="$(sha256_of_file "$file")"
    if [ "$actual_sha" != "$expected_sha" ]; then
      fail "inventoried file hash mismatch: $path"
    fi

    expected_bytes="$(jq -r '.bytes' <<<"$line")"
    actual_bytes="$(bytes_of_file "$file")"
    if [ "$actual_bytes" != "$expected_bytes" ]; then
      fail "inventoried file byte count mismatch: $path"
    fi
  done <"$inventory"
}

require_inventory_path() {
  local inventory="$1"
  local required_path="$2"
  if ! jq -s -e --arg path "$required_path" 'any(.[]; .path == $path)' "$inventory" >/dev/null; then
    fail "mandatory source evidence missing from inventory: $required_path"
  fi
}

verify_report_bound_file() {
  local report="$1"
  local jq_path_expr="$2"
  local jq_sha_expr="$3"
  local root="$4"
  local prefix="$5"
  local path expected_sha relative file actual_sha

  path="$(jq -r "$jq_path_expr" "$report")"
  require_safe_relative_path "$path"
  if [ -n "$prefix" ]; then
    case "$path" in
      "$prefix"/*) relative="${path#"$prefix"/}" ;;
      *) fail "report path $path does not use required prefix $prefix/" ;;
    esac
    require_safe_relative_path "$relative"
  else
    relative="$path"
  fi

  file="$root/$relative"
  if [ ! -f "$file" ]; then
    fail "report-bound file missing: $path"
  fi

  expected_sha="$(jq -r "$jq_sha_expr" "$report")"
  actual_sha="$(sha256_of_file "$file")"
  if [ "$actual_sha" != "$expected_sha" ]; then
    fail "report-bound file hash mismatch: $path"
  fi
}

verify_sha256sum_file_if_present "$package_path.sha256"

tmpdir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmpdir"
}
trap cleanup EXIT

entries="$tmpdir/tar-entries.txt"
extract_dir="$tmpdir/extract"
mkdir -p "$extract_dir"

tar -tzf "$package_path" >"$entries"
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

tar -xzf "$package_path" -C "$extract_dir"

shopt -s nullglob
reports=("$extract_dir"/audit/detta-audit-readiness-*.json)
shopt -u nullglob
if [ "${#reports[@]}" -ne 1 ]; then
  fail "expected exactly one audit readiness report in package, found ${#reports[@]}"
fi

report_path="${reports[0]}"
report_name="$(basename "$report_path")"
external_report_path="$package_dir/$report_name"
if [ -f "$external_report_path" ] && ! cmp -s "$external_report_path" "$report_path"; then
  fail "external report does not match packaged report: $report_name"
fi
verify_sha256sum_file_if_present "$external_report_path.sha256"

if ! jq -e '
  .schema == "detta.audit-readiness-package.v1"
  and .schema_version == 1
  and .project == "DeTTa"
  and .status == "passed"
  and (.version | type == "string" and length > 0)
  and (.git_commit | type == "string" and length > 0)
  and (.target | type == "string" and length > 0)
  and (.chain_id | type == "string" and length > 0)
  and (.package.path | type == "string" and length > 0)
  and (.source_inventory.count | type == "number")
  and (.source_inventory.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.source_inventory.files | type == "array")
  and ((.source_inventory.files | length) == .source_inventory.count)
  and (.generated_release_inventory.count | type == "number")
  and (.generated_release_inventory.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.generated_release_inventory.files | type == "array")
  and ((.generated_release_inventory.files | length) == .generated_release_inventory.count)
  and (.security_findings.finding_count | type == "number")
  and (.proof_artifacts.manifest_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.release_manifest.sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.review_scope | type == "array" and length >= 8)
' "$report_path" >/dev/null; then
  fail "audit readiness report schema is invalid"
fi

reported_package="$(jq -r '.package.path' "$report_path")"
if [ "$reported_package" != "$package_name" ]; then
  fail "report package path $reported_package does not match $package_name"
fi

tracked_inventory="$extract_dir/audit/tracked-inventory.jsonl"
generated_inventory="$extract_dir/audit/generated-release-inventory.jsonl"
verify_inventory_hash_and_count \
  "$tracked_inventory" \
  "$(jq -r '.source_inventory.sha256' "$report_path")" \
  "$(jq -r '.source_inventory.count' "$report_path")"
verify_inventory_hash_and_count \
  "$generated_inventory" \
  "$(jq -r '.generated_release_inventory.sha256' "$report_path")" \
  "$(jq -r '.generated_release_inventory.count' "$report_path")"

verify_inventory_entries "$tracked_inventory" "$extract_dir/source" ""
verify_inventory_entries "$generated_inventory" "$extract_dir/release" "release"

for required in \
  README.md \
  secured-contract-spaces.md \
  scs-formal.md \
  detta-production-implementation-plan.md \
  detta-secure-metta-aspect-generalization-plan.md \
  models/detta-proof-artifact-manifest.json \
  security/detta-audit-findings.json \
  scripts/detta-release-gate.sh \
  scripts/detta-audit-readiness-package.sh \
  scripts/detta-verify-audit-readiness-package.sh \
  crates/detta-core/src/lib.rs \
  crates/detta-node/src/bin/detta-node.rs \
  crates/detta-node/src/bin/detta-client.rs; do
  require_inventory_path "$tracked_inventory" "$required"
done

verify_report_bound_file \
  "$report_path" \
  '.security_findings.path' \
  '.security_findings.sha256' \
  "$extract_dir/source" \
  ""
verify_report_bound_file \
  "$report_path" \
  '.proof_artifacts.manifest_path' \
  '.proof_artifacts.manifest_sha256' \
  "$extract_dir/source" \
  ""
verify_report_bound_file \
  "$report_path" \
  '.release_manifest.path' \
  '.release_manifest.sha256' \
  "$extract_dir/release" \
  "release"

if ! jq -e '
  .schema == "detta.audit-findings.v1"
  and .schema_version == 1
  and .project == "DeTTa"
  and (.categories | type == "array" and length >= 4)
  and (.findings | type == "array")
  and ([.findings[] | select(.status == "Open" or .status == "InRemediation")] | length == 0)
' "$extract_dir/source/security/detta-audit-findings.json" >/dev/null; then
  fail "packaged audit findings manifest is not closed"
fi

jq empty "$extract_dir/source/models/detta-proof-artifact-manifest.json"
release_manifest_file="$extract_dir/$(jq -r '.release_manifest.path' "$report_path")"
jq empty "$release_manifest_file"

printf 'audit readiness package verified: %s\n' "$package_path"
