# DeTTa Formal Model Checking Runbook

This runbook keeps DeTTa proof artifacts reproducible for release review.

## Required Repository Checks

Run these from the repository root:

```sh
cargo test -p detta-verify
sha256sum -c models/detta-proof-artifact-manifest.sha256
sha256sum models/DeTTaBlockExecution.tla
sha256sum models/DeTTaBlockExecution.cfg
```

`cargo test -p detta-verify` checks that:

- `detta_verify::proof_artifact_manifest()` matches
  `models/detta-proof-artifact-manifest.json`;
- the checked-in manifest root matches
  `models/detta-proof-artifact-manifest.sha256`;
- the manifest includes the current `models/DeTTaBlockExecution.tla` SHA-256.
- the manifest binds every checked-in restricted evaluator fixture JSON and
  evaluator fixture attestation file.

## Proof Manifest v2 Runtime Artifacts

`models/detta-proof-artifact-manifest.json` uses
`detta.proof-artifact-manifest.v2`. Version 2 separates proof artifacts into:

- `model_artifacts`: formal model files such as TLA+ modules and bounded model
  checker configs;
- `runtime_artifacts`: deterministic runtime fixtures and attestations that
  support theorem evidence but are not standalone formal models.

Restricted evaluator runtime artifacts include:

- proof trace JSON fixtures;
- forbidden primitive, resource exhaustion, and arithmetic overflow fixtures;
- `.sha256` release attestations for those fixture files;
- the dedicated proof trace root attestation for the compact JSON
  serialization of `report.trace`.

The manifest stores the SHA-256 of each artifact file. For fixture JSON files,
that root authenticates the fixture document. For fixture `.sha256` files, that
root authenticates the release attestation file itself. The evaluator crate also
checks each attestation against the corresponding fixture bytes or trace root.

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

`models/DeTTaBlockExecution.cfg` is the checked-in bounded TLC config for the
abstract block-execution model. With TLC available, run:

```sh
tlc2.TLC -deadlock -workers auto -config models/DeTTaBlockExecution.cfg models/DeTTaBlockExecution.tla
```

When adding a new bounded config file, give finite values for:

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

When theorem coverage, model files, or runtime artifacts change:

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
