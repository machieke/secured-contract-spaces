# DeTTa Formal Model Checking Runbook

This runbook keeps DeTTa proof artifacts reproducible for release review.

## Required Repository Checks

Run these from the repository root:

```sh
cargo test -p detta-verify
sha256sum -c models/detta-proof-artifact-manifest.sha256
sha256sum models/DeTTaBlockExecution.tla
```

`cargo test -p detta-verify` checks that:

- `detta_verify::proof_artifact_manifest()` matches
  `models/detta-proof-artifact-manifest.json`;
- the checked-in manifest root matches
  `models/detta-proof-artifact-manifest.sha256`;
- the manifest includes the current `models/DeTTaBlockExecution.tla` SHA-256.

## Current TLA Artifact

`models/DeTTaBlockExecution.tla` is an abstract SCS block-execution model. The
operators currently intended for model-checking or theorem mapping are:

- `Spec`
- `TypeOK`
- `DispatcherOnlyMutation`
- `WriteScopeSafety`
- `AtomicRevert`
- `ReplaySafety`

The model intentionally abstracts over concrete cryptographic hashing,
signatures, networking, and storage encodings. Rust tests cover those concrete
implementation obligations.

## TLC/Apalache Use

Before running an external model checker, add a bounded config file that gives
finite values for:

- `Contracts`
- `Methods`
- `Principals`
- `Keys`
- `Values`
- `TxIds`
- `Errors`
- `NoValue`
- `GenesisHash`

The config should name `Spec` as the behavior specification and should check the
safety operators listed above. Commit the config file and add its SHA-256 to
`detta-proof-artifact-manifest.json` before treating the model-check run as a
release artifact.

## Refresh Procedure

When theorem coverage or model files change:

1. Update `detta_verify::scs_theorem_coverage()` or the model file.
2. Update `models/detta-proof-artifact-manifest.json`.
3. Recompute and update `models/detta-proof-artifact-manifest.sha256`.
4. Run `cargo test -p detta-verify`.
5. Run the full gate:

```sh
cargo fmt && cargo test && cargo clippy --all-targets -- -D warnings
```

The manifest must remain timestamp-free and deterministic so release
attestations are stable across machines.
