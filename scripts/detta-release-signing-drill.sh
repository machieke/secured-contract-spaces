#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="${DETTA_RELEASE_VERSION:-signing-drill}"
safe_version="$(printf '%s' "$version" | tr -c 'A-Za-z0-9._-' '-')"
out_dir="${DETTA_RELEASE_OUT:-$repo_root/dist}"
chain_id="${DETTA_RELEASE_CHAIN_ID:-detta-local}"
signer_name="${DETTA_RELEASE_SIGNING_DRILL_NAME:-DeTTa Release Signing Drill}"
signer_email="${DETTA_RELEASE_SIGNING_DRILL_EMAIL:-detta-release-signing-drill@example.invalid}"
signer_uid="$signer_name <$signer_email>"

for tool in jq sha256sum gpg awk; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "required tool missing: $tool" >&2
    exit 1
  fi
done

mkdir -p "$out_dir"

stage="$(mktemp -d)"
cleanup() {
  rm -rf "$stage"
}
trap cleanup EXIT

gpg_home="$stage/gnupg"
mkdir -m 0700 "$gpg_home"

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

release_log="$stage/package.log"
DETTA_RELEASE_VERSION="$version" \
  DETTA_RELEASE_OUT="$out_dir" \
  DETTA_RELEASE_CHAIN_ID="$chain_id" \
  scripts/detta-package-release.sh >"$release_log"

manifest_path="$out_dir/detta-release-$safe_version.json"
if [ ! -f "$manifest_path" ]; then
  echo "release manifest not found after packaging: $manifest_path" >&2
  cat "$release_log" >&2 || true
  exit 1
fi

key_params="$stage/release-signing-key.params"
cat >"$key_params" <<KEY
%no-protection
Key-Type: RSA
Key-Length: 2048
Key-Usage: sign
Name-Real: $signer_name
Name-Email: $signer_email
Expire-Date: 1d
%commit
KEY

GNUPGHOME="$gpg_home" \
  gpg --batch --pinentry-mode loopback --generate-key "$key_params" >/dev/null

fingerprint="$(
  GNUPGHOME="$gpg_home" \
    gpg --batch --with-colons --list-secret-keys "$signer_email" |
    awk -F: '/^fpr:/ {print $10; exit}'
)"
if [ -z "$fingerprint" ]; then
  echo "failed to discover generated signing key fingerprint" >&2
  exit 1
fi

public_key_name="detta-release-signing-drill-$safe_version.pub.asc"
public_key_path="$out_dir/$public_key_name"
GNUPGHOME="$gpg_home" \
  gpg --batch --armor --export "$fingerprint" >"$public_key_path"
public_key_sha="$(sha256sum "$public_key_path" | awk '{print $1}')"

mapfile -t checksum_paths < <(
  jq -r '[.manifest_sha256_path] + [.artifacts[].sha256_path] | unique[]' \
    "$manifest_path"
)
if [ "${#checksum_paths[@]}" -eq 0 ]; then
  echo "release manifest did not declare checksum files to sign" >&2
  exit 1
fi

signed_checksums_jsonl="$stage/signed-checksums.jsonl"
for checksum_path in "${checksum_paths[@]}"; do
  reject_unsafe_path "checksum" "$checksum_path"
  checksum_file="$out_dir/$checksum_path"
  signature_path="$checksum_path.sig"
  signature_file="$out_dir/$signature_path"
  if [ ! -f "$checksum_file" ]; then
    echo "checksum not found: $checksum_file" >&2
    exit 1
  fi

  GNUPGHOME="$gpg_home" \
    gpg --batch --yes --pinentry-mode loopback --armor \
      --local-user "$fingerprint" \
      --output "$signature_file" \
      --detach-sign "$checksum_file"

  checksum_sha="$(sha256sum "$checksum_file" | awk '{print $1}')"
  signature_sha="$(sha256sum "$signature_file" | awk '{print $1}')"
  jq -cn \
    --arg checksum_path "$checksum_path" \
    --arg signature_path "$signature_path" \
    --arg checksum_sha "$checksum_sha" \
    --arg signature_sha "$signature_sha" \
    '{
      checksum_path: $checksum_path,
      signature_path: $signature_path,
      checksum_sha256: $checksum_sha,
      signature_sha256: $signature_sha
    }' >>"$signed_checksums_jsonl"
done

verify_log="$stage/verify.log"
GNUPGHOME="$gpg_home" \
  DETTA_RELEASE_SIGNER_FINGERPRINT="$fingerprint" \
  scripts/detta-verify-release-signatures.sh "$manifest_path" >"$verify_log"

report_name="detta-release-signing-drill-$safe_version.json"
report_path="$out_dir/$report_name"
jq -n -e \
  --arg schema "detta.release-signing-drill.v1" \
  --arg project "DeTTa" \
  --arg version "$safe_version" \
  --arg source_version "$version" \
  --arg manifest "$(basename "$manifest_path")" \
  --arg chain_id "$chain_id" \
  --arg signer_uid "$signer_uid" \
  --arg signer_fingerprint "$fingerprint" \
  --arg public_key_path "$public_key_name" \
  --arg public_key_sha "$public_key_sha" \
  --arg verify_script "scripts/detta-verify-release-signatures.sh" \
  --arg status "passed" \
  --slurpfile signed_checksums "$signed_checksums_jsonl" \
  '{
    schema: $schema,
    schema_version: 1,
    project: $project,
    version: $version,
    source_version: $source_version,
    chain_id: $chain_id,
    release_manifest: $manifest,
    signing_key: {
      uid: $signer_uid,
      fingerprint: $signer_fingerprint,
      public_key_path: $public_key_path,
      public_key_sha256: $public_key_sha
    },
    signed_checksums: $signed_checksums,
    verification: {
      script: $verify_script,
      required_fingerprint: $signer_fingerprint
    },
    status: $status
  }' >"$report_path"

(
  cd "$out_dir"
  sha256sum "$report_name" >"$report_name.sha256"
  sha256sum -c "$report_name.sha256"
)

printf 'release signing drill report: %s\n' "$report_path"
