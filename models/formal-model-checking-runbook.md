# DeTTa Formal Model Checking Runbook

This runbook keeps DeTTa proof artifacts reproducible for release review.

## Required Repository Checks

Run these from the repository root:

```sh
cargo test -p detta-verify
(
  cd models
  sha256sum -c detta-proof-artifact-manifest.sha256
  sha256sum -c detta-restricted-evaluator-proof-trace.sha256
  sha256sum -c detta-restricted-evaluator-forbidden-primitives.sha256
  sha256sum -c detta-restricted-evaluator-resource-exhaustion.sha256
  sha256sum -c detta-restricted-evaluator-arithmetic-overflow.sha256
  sha256sum -c detta-restricted-evaluator-fixture-inventory.sha256
  sha256sum DeTTaBlockExecution.tla
  sha256sum DeTTaBlockExecution.cfg
  sha256sum detta-restricted-evaluator-proof-trace-root.sha256
)
```

`cargo test -p detta-verify` checks that:

- `detta_verify::proof_artifact_manifest()` matches
  `models/detta-proof-artifact-manifest.json`;
- the checked-in manifest root matches
  `models/detta-proof-artifact-manifest.sha256`;
- the manifest includes the current `models/DeTTaBlockExecution.tla` SHA-256.
- the manifest binds every checked-in restricted evaluator fixture JSON and
  evaluator fixture attestation file, including the fixture inventory.
- each evaluator fixture inventory entry matches the corresponding manifest
  `runtime_artifacts` root.
- runtime artifact paths are unique, so a duplicate path cannot shadow an
  earlier root.
- every model and runtime artifact root is a 64-character lowercase SHA-256
  hex digest.
- every model and runtime artifact path is repository-relative under
  `models/` without parent traversal or platform-specific separators.
- every model and runtime artifact path uses an allowed proof artifact suffix:
  `.tla`, `.cfg`, `.json`, or `.sha256`.
- model artifact paths are unique.
- model artifact order is stable: TLA+ module, then TLC config.
- model and runtime artifact path sets are disjoint.
- runtime artifact order is stable: evaluator proof trace, its attestations,
  forbidden primitive, resource exhaustion, arithmetic overflow, then fixture
  inventory artifacts.
- every release attestation file uses single-line `sha256sum` format with a
  lowercase SHA-256 root and expected basename.
- every release attestation filename is bound to the target artifact basename
  or compact trace target name.
- every release attestation root matches the target artifact bytes or compact
  proof-trace bytes.
- every evaluator release attestation in `runtime_artifacts` is covered by the
  evaluator fixture inventory, except the inventory self-attestation.
- every evaluator fixture JSON in `runtime_artifacts` is covered by the
  evaluator fixture inventory, except the inventory JSON itself.
- the evaluator fixture inventory schema, version, and evaluator identifier are
  explicit and stable.
- evaluator fixture inventory entry names are non-empty and unique.
- evaluator fixture inventory fixture schemas are namespaced and unique.
- evaluator fixture inventory fixture schemas cover proof trace, forbidden
  primitive, resource exhaustion, and arithmetic overflow fixtures.
- evaluator fixture inventory entry names map to the expected fixture schemas.
- evaluator fixture inventory entry names map to the expected fixture,
  attestation, and trace-root attestation paths.
- evaluator fixture inventory entry names map to the expected fixture,
  attestation, compact trace, and trace-root attestation roots.
- evaluator fixture inventory fixture, attestation, and trace-root attestation
  paths are unique.
- evaluator fixture inventory paths stay under `models/`, avoid traversal, and
  use `.json` for fixtures and `.sha256` for attestations.
- evaluator fixture inventory attestation paths bind to the target fixture
  filenames, with trace-root attestations using the `-root.sha256` form.
- evaluator fixture inventory trace-root metadata appears as an all-or-none
  group only on the proof-trace fixture.
- evaluator fixture inventory fixture order is stable: proof trace, forbidden
  primitive, resource exhaustion, then arithmetic overflow.
- evaluator fixture inventory roots are lowercase SHA-256 hex.
- the evaluator fixture inventory trace root matches the proof-trace fixture,
  compact trace bytes, and trace-root attestation.
- theorem coverage order is stable from `THM-001` through `THM-015`.
- theorem evidence order is stable for each theorem ID.
- theorem evidence uses all expected evidence kinds and every theorem retains
  at least one runtime test anchor.
- theorem runtime-test evidence references use expected `detta_core`,
  `detta_evaluator`, or `detta_verify` test namespaces.
- theorem verifier evidence references use the expected
  `detta_verify::verify_*` namespace.
- theorem model evidence references use the expected
  `models/DeTTaBlockExecution.tla::*` namespace.
- theorem fixture evidence references use the restricted evaluator JSON
  fixture namespace.
- theorem fixture evidence references are listed by the evaluator fixture
  inventory.
- theorem fixture evidence covers proof trace, forbidden primitive, resource
  exhaustion, and arithmetic overflow fixture schemas.
- theorem fixture evidence covers all evaluator fixture inventory entry names.
- theorem model evidence covers the expected TLA operator set.
- theorem runtime-test evidence covers `detta_core`, `detta_evaluator`, and
  `detta_verify` test crates.
- theorem verifier evidence covers `verify_kernel_trace` and
  `verify_differential_replay`.
- theorem evidence entries are unique within each theorem.
- theorem IDs map to the expected theorem names.
- theorem names are unique.
- theorem IDs use the fixed `THM-NNN` format and sequence.
- proof manifest `theorem_count` matches the SCS theorem coverage.
- proof manifest project and scope bind the bundle to DeTTa SCS runtime safety.
- proof manifest model and runtime artifact counts match expected release shape.
- proof manifest artifact roots use the 64-character SHA-256 hex length.
- proof release attestation roots use the 64-character SHA-256 hex length.
- proof release attestation count matches expected release shape.
- proof release attestation filenames are unique.
- proof release attestation filenames target only JSON artifacts and the trace
  root.
- proof release attestation filenames match the expected release set.
- proof release attestation tests use a single helper-backed fixture list.

The evaluator `sha256sum -c` commands verify fixture-file attestations. The
final `sha256sum` command reports the file root of the compact-trace-root
attestation, which is itself bound in the proof manifest. Rust tests recompute
the compact trace root from `report.trace` and compare it with that attestation.

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
- the fixture inventory that lists all evaluator fixture roots and
  attestations;
- `.sha256` release attestations for those fixture files;
- the dedicated proof trace root attestation for the compact JSON
  serialization of `report.trace`.

The manifest stores the SHA-256 of each artifact file. For fixture JSON files,
that root authenticates the fixture document. For fixture `.sha256` files, that
root authenticates the release attestation file itself. The evaluator crate also
checks each attestation against the corresponding fixture bytes or trace root.

## Evaluator Artifact Release Checklist

Before accepting a release proof bundle, verify:

- `cargo test -p detta-evaluator` passes, including fixture JSON and
  attestation checks;
- `cargo test -p detta-verify` passes, including proof manifest fixture,
  theorem evidence, runtime artifact, and attestation consistency checks;
- every restricted evaluator fixture JSON path appears in
  `runtime_artifacts`;
- every restricted evaluator fixture `.sha256` path appears in
  `runtime_artifacts`;
- the evaluator fixture inventory JSON and `.sha256` paths appear in
  `runtime_artifacts`;
- every `runtime_artifacts` path is unique;
- every `model_artifacts` path is unique;
- model artifact order is stable: TLA+ module, then TLC config;
- runtime artifact order is stable: evaluator proof trace, its attestations,
  forbidden primitive, resource exhaustion, arithmetic overflow, then fixture
  inventory artifacts;
- every model and runtime artifact root is lowercase SHA-256 hex;
- every model and runtime artifact path stays under `models/`;
- every model and runtime artifact path uses an allowed suffix;
- no artifact path appears in both `model_artifacts` and `runtime_artifacts`;
- every release attestation file uses the expected single-line `sha256sum`
  format;
- every release attestation filename matches the target artifact basename or
  compact trace target name;
- every release attestation root recomputes from its target bytes;
- every evaluator release attestation is listed by the evaluator fixture
  inventory or is the inventory self-attestation;
- every evaluator fixture JSON is listed by the evaluator fixture inventory or
  is the inventory JSON itself;
- the evaluator fixture inventory reports
  `detta.restricted-evaluator-fixture-inventory.v1`, `schema_version: 1`, and
  `detta.restricted-script-evaluator`;
- every evaluator fixture inventory entry name is non-empty and unique;
- every evaluator fixture inventory fixture schema is namespaced and unique;
- evaluator fixture inventory fixture schemas cover all four expected evaluator
  fixture families;
- every evaluator fixture inventory entry name maps to its expected fixture
  schema;
- every evaluator fixture inventory entry name maps to its expected fixture,
  attestation, and trace-root attestation paths;
- every evaluator fixture inventory entry name maps to its expected fixture,
  attestation, compact trace, and trace-root attestation roots;
- every evaluator fixture inventory path is unique across fixture,
  attestation, and trace-root attestation fields;
- every evaluator fixture inventory path stays under `models/`, avoids
  traversal, and uses the expected fixture or attestation suffix;
- every evaluator fixture inventory attestation path binds to its target
  fixture filename, with trace-root attestations using the `-root.sha256`
  form;
- evaluator fixture inventory trace-root metadata appears as an all-or-none
  group only on the proof-trace fixture;
- evaluator fixture inventory fixture order is stable: proof trace, forbidden
  primitive, resource exhaustion, then arithmetic overflow;
- every evaluator fixture inventory root uses lowercase SHA-256 hex;
- the evaluator fixture inventory trace root matches the proof-trace fixture,
  compact trace bytes, and trace-root attestation;
- every root listed inside the evaluator fixture inventory matches the
  corresponding `runtime_artifacts` entry;
- `detta-restricted-evaluator-fixture-inventory.sha256` verifies against the
  checked-in inventory JSON;
- every fixture evidence reference with `kind: "Fixture"` resolves to a
  `runtime_artifacts` path;
- theorem coverage order is stable from `THM-001` through `THM-015`;
- theorem evidence order is stable for each theorem ID;
- theorem evidence uses all expected evidence kinds and every theorem retains
  at least one runtime test anchor;
- theorem runtime-test evidence references use expected `detta_core`,
  `detta_evaluator`, or `detta_verify` test namespaces;
- theorem verifier evidence references use the expected
  `detta_verify::verify_*` namespace;
- theorem model evidence references use the expected
  `models/DeTTaBlockExecution.tla::*` namespace;
- theorem fixture evidence references use the restricted evaluator JSON
  fixture namespace;
- theorem fixture evidence references are listed by the evaluator fixture
  inventory;
- theorem fixture evidence covers proof trace, forbidden primitive, resource
  exhaustion, and arithmetic overflow fixture schemas;
- theorem fixture evidence covers all evaluator fixture inventory entry names;
- theorem model evidence covers the expected TLA operator set;
- theorem runtime-test evidence covers `detta_core`, `detta_evaluator`, and
  `detta_verify` test crates;
- theorem verifier evidence covers `verify_kernel_trace` and
  `verify_differential_replay`;
- theorem evidence entries are unique within each theorem;
- theorem IDs map to the expected theorem names;
- theorem names are unique;
- theorem IDs use the fixed `THM-NNN` format and sequence;
- proof manifest `theorem_count` matches the SCS theorem coverage;
- proof manifest project and scope bind the bundle to DeTTa SCS runtime safety;
- proof manifest model and runtime artifact counts match expected release shape;
- proof manifest artifact roots use the 64-character SHA-256 hex length;
- proof release attestation roots use the 64-character SHA-256 hex length;
- proof release attestation count matches expected release shape;
- proof release attestation filenames are unique;
- proof release attestation filenames target only JSON artifacts and the trace
  root;
- proof release attestation filenames match the expected release set;
- proof release attestation tests use a single helper-backed fixture list;
- `detta-restricted-evaluator-proof-trace-root.sha256` matches both the
  fixture `trace_root` field and the recomputed root of `report.trace`;
- `models/detta-proof-artifact-manifest.sha256` verifies after all fixture,
  theorem, or runtime artifact changes.

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
