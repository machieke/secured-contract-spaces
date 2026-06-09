#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cargo metadata --format-version 1 --locked >/dev/null

if ! cargo deny --version >/dev/null 2>&1; then
  cat >&2 <<'MSG'
cargo-deny is not installed; skipping local dependency and supply-chain audit.
CI runs cargo-deny with the checked-in deny.toml policy.
Install it with `cargo install cargo-deny`, or set DETTA_REQUIRE_DEP_AUDIT=1 to make this a hard local failure.
MSG
  case "${DETTA_REQUIRE_DEP_AUDIT:-0}" in
    1 | true | TRUE | yes | YES)
      exit 1
      ;;
  esac
  exit 0
fi

cargo deny check advisories bans sources
