# DeTTa Formal Models

This directory contains formal-model artifacts for the DeTTa runtime.

- `DeTTaBlockExecution.tla` models the abstract SCS block-execution boundary:
  dispatcher-mediated calls, method-policy lookup, nonce replay prevention,
  active reentrancy locks, guarded writes, programmable aspect module
  admission, aspect-owned write scopes, commit/revert atomicity, and
  deterministic replay obligations.
- `DeTTaBlockExecution.cfg` is a bounded TLC configuration for the abstract
  block-execution model.
- `detta-proof-artifact-manifest.json` is the stable proof-artifact manifest
  exported by `detta_verify::proof_artifact_manifest()`. It includes theorem
  coverage, formal model artifact roots, and restricted evaluator runtime
  fixture and attestation roots. The verify crate tests that the checked-in JSON
  stays synchronized with theorem coverage, model files, runtime fixtures, and
  runtime fixture attestations, including the evaluator fixture inventory and
  each inventory entry's manifest-bound roots.
- `detta-proof-artifact-manifest.sha256` records the release-attestation root
  for the checked-in proof-artifact manifest.
- `detta-restricted-evaluator-proof-trace.json` is a golden fixture for the
  restricted evaluator. It binds a representative script, emitted kernel trace,
  trace root, and symbolic verifier result.
- `detta-restricted-evaluator-proof-trace.sha256` records the release
  attestation root for the proof trace fixture.
- `detta-restricted-evaluator-proof-trace-root.sha256` records the root of the
  compact JSON serialization of the emitted proof trace.
- `detta-restricted-evaluator-forbidden-primitives.json` is a golden fixture
  for the restricted evaluator's primitive boundary. It lists all allowed
  contract primitives and every forbidden escape hatch with the expected trap.
- `detta-restricted-evaluator-forbidden-primitives.sha256` records the release
  attestation root for the forbidden primitive fixture.
- `detta-restricted-evaluator-resource-exhaustion.json` is a golden fixture for
  step-budget exhaustion. It records the failing script, expected error, and
  absence of a committed execution report.
- `detta-restricted-evaluator-resource-exhaustion.sha256` records the release
  attestation root for the resource exhaustion fixture.
- `detta-restricted-evaluator-arithmetic-overflow.json` is a golden fixture for
  checked arithmetic overflow. It records decimal-string operands, expected
  error, and absence of a committed execution report.
- `detta-restricted-evaluator-arithmetic-overflow.sha256` records the release
  attestation root for the arithmetic overflow fixture.
- `detta-restricted-evaluator-fixture-inventory.json` is a stable inventory of
  restricted evaluator fixture schemas, fixture roots, attestation roots, and
  trace-root attestation metadata.
- `detta-restricted-evaluator-fixture-inventory.sha256` records the release
  attestation root for the fixture inventory.
- `aspects/stdlib/minimal-transfer-token.artifact.json` records the
  root-authenticated standard-library aspect artifact used by release gates,
  including source, IR, ABI, policy, storage-schema, registry-schema, and
  invariant roots for the checked-in 14-bundle token aspect module.
- `aspects/stdlib/minimal-transfer-token.proof-obligations.json` records the
  programmable-module proof obligations for the standard-library token bundle.
- `aspects/stdlib/*.sha256` files bind the checked-in aspect source, artifact,
  and proof-obligation manifest to release-gate checksum checks.
- `formal-model-checking-runbook.md` describes the repository checks, external
  model-checker preparation, and manifest refresh procedure.
- `../detta-restricted-evaluator-subset.md` documents the implemented
  restricted evaluator subset, fixture schemas, and refresh workflow.

The model is intentionally abstract over hashing, signatures, concrete storage
encoding, concrete MeTTa syntax, and networking. Programmable contracts are
represented by verified module artifacts, contract-to-module bindings, and
artifact-owned key sets. Parser, verifier, evaluator, and host-call details are
checked by Rust tests plus the standard-library proof-obligation manifest and
can be refined into separate models later.

Primary theorem mapping:

- `DispatcherOnlyMutation` corresponds to `THM-001`.
- `WriteScopeSafety` corresponds to `THM-003` and supports `THM-012`.
- `AtomicRevert` corresponds to `THM-006` and `THM-007`.
- `ReplaySafety` corresponds to `THM-010`.
- `ProgrammableModuleSoundness` corresponds to the programmable aspect module
  proof obligations in
  `aspects/stdlib/minimal-transfer-token.proof-obligations.json`.
- Deterministic replay is represented as a trace-comparison obligation and is
  exercised by the `detta-verify` differential replay harness.
