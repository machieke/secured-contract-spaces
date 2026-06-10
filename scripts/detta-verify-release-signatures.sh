#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage: scripts/detta-verify-release-signatures.sh <dist/detta-release-<version>.json>

Verifies a DeTTa release manifest, its SHA-256 checksum, the detached signature
for that checksum, every declared artifact checksum, and every artifact checksum
signature. Set DETTA_RELEASE_SIGNER_FINGERPRINT to require a specific signer.
USAGE
}

if [ "${1:-}" = "-h" ] || [ "${1:-}" = "--help" ]; then
  usage
  exit 0
fi

manifest_path="${1:-}"
if [ -z "$manifest_path" ]; then
  usage >&2
  exit 2
fi
if [ ! -f "$manifest_path" ]; then
  echo "release manifest not found: $manifest_path" >&2
  exit 1
fi

for tool in jq sha256sum gpg awk; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool missing: $tool" >&2
    exit 1
  fi
done

manifest_dir="$(cd "$(dirname "$manifest_path")" && pwd)"
manifest_file="$(basename "$manifest_path")"
gpg_bin="${DETTA_GPG:-gpg}"
required_fingerprint="${DETTA_RELEASE_SIGNER_FINGERPRINT:-}"

normalize_fingerprint() {
  printf '%s' "$1" | tr -d '[:space:]' | tr '[:lower:]' '[:upper:]'
}

reject_unsafe_path() {
  local label="$1"
  local path="$2"
  case "$path" in
    ""|/*|../*|*/../*|*/..)
      echo "unsafe $label path in release manifest: ${path:-<empty>}" >&2
      exit 1
      ;;
  esac
}

verify_detached_signature() {
  local signed_path="$1"
  local signature_path="$2"
  local label="$3"
  local status_file="$tmpdir/$label.status"
  local stderr_file="$tmpdir/$label.stderr"

  if [ ! -f "$signed_path" ]; then
    echo "signed checksum not found: $signed_path" >&2
    exit 1
  fi
  if [ ! -f "$signature_path" ]; then
    echo "detached signature not found: $signature_path" >&2
    echo "expected command form: gpg --armor --output <checksum>.sig --detach-sign <checksum>" >&2
    exit 1
  fi

  if ! "$gpg_bin" --batch --status-fd 1 --verify "$signature_path" "$signed_path" \
    >"$status_file" 2>"$stderr_file"; then
    cat "$stderr_file" >&2 || true
    echo "signature verification failed for $signature_path" >&2
    exit 1
  fi

  local signer
  signer="$(
    awk '/^\[GNUPG:\] VALIDSIG / {print $3; exit}' "$status_file"
  )"
  if [ -z "$signer" ]; then
    cat "$stderr_file" >&2 || true
    echo "signature verification did not expose a valid signer for $signature_path" >&2
    exit 1
  fi

  if [ -n "$required_fingerprint" ]; then
    local normalized_required normalized_signer
    normalized_required="$(normalize_fingerprint "$required_fingerprint")"
    normalized_signer="$(normalize_fingerprint "$signer")"
    if [ "$normalized_signer" != "$normalized_required" ]; then
      echo "unexpected signer for $signature_path: expected $normalized_required got $normalized_signer" >&2
      exit 1
    fi
  fi

  printf '%s\n' "$signer"
}

jq -e '
  .schema == "detta.release-artifacts.v1"
  and .schema_version == 1
  and .project == "DeTTa"
  and (.git_commit | type == "string" and length > 0)
  and (.source_state.schema == "detta.source-state.v1")
  and (.source_state.schema_version == 1)
  and (.source_state.git_commit == .git_commit)
  and (.source_state.git_tree | type == "string" and test("^([0-9a-f]{40}|[0-9a-f]{64})$"))
  and (.source_state.worktree_clean | type == "boolean")
  and (.source_state.tracked_change_count | type == "number")
  and (.source_state.untracked_file_count | type == "number")
  and (.source_state.status_porcelain_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.source_state.tracked_diff_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.source_state.staged_diff_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.source_state.unstaged_diff_sha256 | type == "string" and test("^[0-9a-f]{64}$"))
  and (.manifest_sha256_path | type == "string" and length > 0)
  and (.manifest_signature_path | type == "string" and length > 0)
  and (.artifacts | type == "array" and length > 0)
' "$manifest_path" >/dev/null

case "${DETTA_REQUIRE_CLEAN_RELEASE_SOURCE:-0}" in
  1 | true | TRUE | yes | YES)
    if ! jq -e '.source_state.worktree_clean == true' "$manifest_path" >/dev/null; then
      echo "release source state is dirty" >&2
      jq '.source_state' "$manifest_path" >&2
      exit 1
    fi
    ;;
esac

manifest_sha256_path="$(jq -r '.manifest_sha256_path' "$manifest_path")"
manifest_signature_path="$(jq -r '.manifest_signature_path' "$manifest_path")"
reject_unsafe_path "manifest checksum" "$manifest_sha256_path"
reject_unsafe_path "manifest signature" "$manifest_signature_path"

tmpdir="$(mktemp -d)"
trap 'rm -rf "$tmpdir"' EXIT

(
  cd "$manifest_dir"
  sha256sum -c "$manifest_sha256_path"
)

manifest_signer="$(
  verify_detached_signature \
    "$manifest_dir/$manifest_sha256_path" \
    "$manifest_dir/$manifest_signature_path" \
    "manifest"
)"
printf 'verified manifest signature: %s signed by %s\n' \
  "$manifest_signature_path" "$manifest_signer"

while IFS= read -r artifact; do
  kind="$(jq -r '.kind' <<<"$artifact")"
  path="$(jq -r '.path' <<<"$artifact")"
  expected_sha="$(jq -r '.sha256' <<<"$artifact")"
  sha256_path="$(jq -r '.sha256_path' <<<"$artifact")"
  signature_path="$(jq -r '.signature_path' <<<"$artifact")"

  reject_unsafe_path "artifact" "$path"
  reject_unsafe_path "artifact checksum" "$sha256_path"
  reject_unsafe_path "artifact signature" "$signature_path"

  artifact_path="$manifest_dir/$path"
  checksum_path="$manifest_dir/$sha256_path"
  detached_signature_path="$manifest_dir/$signature_path"

  if [ ! -f "$artifact_path" ]; then
    echo "artifact not found: $artifact_path" >&2
    exit 1
  fi
  if [ ! -f "$checksum_path" ]; then
    echo "artifact checksum not found: $checksum_path" >&2
    exit 1
  fi
  if [ ! -f "$detached_signature_path" ]; then
    echo "artifact signature not found: $detached_signature_path" >&2
    exit 1
  fi

  actual_sha="$(sha256sum "$artifact_path" | awk '{print $1}')"
  if [ "$actual_sha" != "$expected_sha" ]; then
    echo "artifact checksum mismatch for $path: expected $expected_sha got $actual_sha" >&2
    exit 1
  fi

  (
    cd "$manifest_dir"
    sha256sum -c "$sha256_path"
  )

  signer="$(
    verify_detached_signature "$checksum_path" "$detached_signature_path" "$kind"
  )"
  printf 'verified artifact signature: %s signed by %s\n' \
    "$signature_path" "$signer"
done < <(jq -c '.artifacts[]' "$manifest_path")

printf 'release signatures verified: %s\n' "$manifest_path"
