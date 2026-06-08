# DeTTa Formal Models

This directory contains formal-model artifacts for the DeTTa runtime.

- `DeTTaBlockExecution.tla` models the abstract SCS block-execution boundary:
  dispatcher-mediated calls, method-policy lookup, nonce replay prevention,
  active reentrancy locks, guarded writes, commit/revert atomicity, and
  deterministic replay obligations.
- `detta-proof-artifact-manifest.json` is the stable proof-artifact manifest
  exported by `detta_verify::proof_artifact_manifest()`. It includes theorem
  coverage and formal model artifact roots. The verify crate tests that the
  checked-in JSON stays synchronized with theorem coverage and model files.
- `detta-proof-artifact-manifest.sha256` records the release-attestation root
  for the checked-in proof-artifact manifest.

The model is intentionally abstract over hashing, signatures, concrete storage
encoding, and networking. Those are checked by the Rust implementation tests and
can be refined into separate models later.

Primary theorem mapping:

- `DispatcherOnlyMutation` corresponds to `THM-001`.
- `WriteScopeSafety` corresponds to `THM-003` and supports `THM-012`.
- `AtomicRevert` corresponds to `THM-006` and `THM-007`.
- `ReplaySafety` corresponds to `THM-010`.
- Deterministic replay is represented as a trace-comparison obligation and is
  exercised by the `detta-verify` differential replay harness.
