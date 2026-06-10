# DeTTa Operator Runbook

This runbook binds the production DeTTa implementation plan to concrete
operator procedures that can be exercised with the current node, storage, RPC,
state-sync, governance, and validator-key surfaces.

## Deployment Topology

A production DeTTa network should run at least four independently hosted
validators for a 3-of-4 quorum, plus full nodes for RPC, indexing, and state
sync. Validators expose the peer protocol only to allowlisted peers. Public RPC
nodes expose bounded HTTP JSON RPC at `/rpc`; operator RPC should compose
`AuthenticatedJsonRpcHandler` and `RateLimitedJsonRpcHandler`.

Each validator data directory contains:

- durable blocks, receipts, finality certificates, slashing records, and
  snapshots;
- durable consensus-signing records that prevent conflicting validator
  signatures after restart, crash recovery, or network partition;
- pending mempool transactions;
- validator-set metadata, keyring, pending authorizations, and audit records;
- snapshot import audit records and state-sync diagnostics.

## Preflight

Before joining a validator:

- verify the binary with
  `DETTA_E2E_FULL=1 DETTA_REQUIRE_DEP_AUDIT=1 scripts/detta-release-gate.sh`;
- build the operator binary with `cargo build --release --bin detta-node`;
- generate or verify the launch genesis snapshot with
  `detta-node write-genesis`;
- start the persistent validator with `detta-node serve --storage ...`;
- package release-candidate artifacts with `scripts/detta-package-release.sh`
  and sign the generated `*.sha256` files with the release key; set
  `DETTA_REQUIRE_CLEAN_RELEASE_SOURCE=1` for final publication packaging;
- verify the signed release bundle with
  `scripts/detta-verify-release-signatures.sh`;
- run `scripts/detta-release-candidate-evidence-bundle.sh` after the hard gate
  when retained release-candidate evidence is needed under `dist/`, then rerun
  `scripts/detta-verify-release-candidate-evidence-bundle.sh` after artifact
  transfer;
- run `scripts/detta-release-signing-drill.sh` and retain the generated
  signing-drill report before the production release-key ceremony;
- run `scripts/detta-genesis-finalization-drill.sh` and retain the generated
  genesis-finalization report before publishing launch artifacts;
- run `scripts/detta-operator-launch-rehearsal.sh` against the packaged
  artifacts and retain the generated rehearsal report;
- run `scripts/detta-incident-response-drill.sh` against the packaged
  artifacts and retain the generated incident-drill report;
- run `scripts/detta-governance-bootstrap-drill.sh` against the packaged
  artifacts and retain the generated governance-bootstrap report;
- run `scripts/detta-validator-onboarding-drill.sh` against the packaged
  artifacts and retain the generated validator-onboarding report;
- run `scripts/detta-packaged-client-flow-drill.sh` against the packaged
  artifacts and retain the generated packaged-client report;
- run `scripts/detta-public-testnet-stability-drill.sh` against the packaged
  artifacts and retain the generated local stability report;
- run `scripts/detta-audit-readiness-package.sh` and retain the generated
  audit-readiness package and report for external reviewers, then rerun
  `scripts/detta-verify-audit-readiness-package.sh` after artifact transfer;
  set `DETTA_REQUIRE_CLEAN_AUDIT_PACKAGE=1` for final reviewer handoff;
- check `ops/detta-public-testnet-readiness.json` and confirm it does not claim
  public-testnet readiness while blockers remain;
- check `ops/detta-mainnet-candidate-readiness.json` and confirm it does not
  claim mainnet readiness while launch, audit, signing, or rehearsal blockers
  remain;
- run `scripts/detta-verify-readiness-manifests.sh` to verify readiness
  schemas, evidence paths, signed-artifact hashes, and `detta-verify`
  readiness tests;
- run `scripts/detta-readiness-status-report.sh dist/detta-readiness-status.json`
  and retain the report plus
  `dist/detta-readiness-status.json.sha256` with launch evidence;
- after transferring the report, rerun
  `scripts/detta-verify-readiness-status-report.sh dist/detta-readiness-status.json`;
- confirm `detta-rpc-openapi.json` matches the deployed binary version;
- initialize the validator with the expected `network_id`, `chain_id`, and
  trusted validator-set metadata;
- confirm `get_node_health` reports the expected chain, height, validator key
  count, mempool size, and authenticated roots;
- confirm `get_snapshot_metadata_root_status` has no unexpected local/imported
  root mismatch.

## Monitoring

Poll these RPCs on each validator and RPC node:

- `get_node_health`: chain identity, height, mempool size, trusted validator
  key count, pending validator-set updates, and state roots.
- `get_operator_metrics`: observed peer count, mempool size, consensus height,
  finalized height, finality lag, recent block/proof latency, storage bytes,
  and RPC error count.
- `get_operator_alerts`: evaluated stalled-consensus, root-mismatch,
  excessive-revert, peer-isolation, slashing-evidence, disk-pressure,
  RPC-overload, and mempool-saturation alerts.
- `get_mempool_status`: pending total, per-sender pressure, admission limits,
  and block resource limit.
- `get_finality_certificate`: latest finalized height and signer set.
- `get_slashing_record`: equivocation evidence when an alert names a validator.
- `get_snapshot_sync_client_metrics`: retry attempts, stream failures, chunks
  received, manifests received, and metadata-root verification status.
- `get_snapshot_import_audit_root` and `get_snapshot_import_audit_records`:
  snapshot import history and retention pressure.
- `get_blocks_page` and `get_events_page`: indexer catch-up and gap checks.

Alert on:

- height lag against a majority of validators;
- missing or stale finality certificates;
- persistent mempool saturation or one sender dominating pending slots;
- any snapshot metadata root mismatch;
- nonempty slashing records;
- repeated state-sync stream failures;
- disk growth that threatens snapshot, block, or audit retention.

## Recovery

For process restart, restart from the same data directory and verify:

- pending mempool entries reload with `get_mempool_status`;
- finalized history is served with `get_block`, `get_transaction`,
  `get_receipt`, and proof RPCs;
- consensus-signing records remain in the validator data directory before the
  validator key is allowed to sign again;
- finality certificates and slashing records are still available;
- validator-set metadata and pending authorizations reload.

For backup restore, copy the validator data directory through the storage
backup path and retain the backup manifest. Restore into a fresh data directory,
restart the node, and verify the manifest's snapshot root, highest block height,
highest finality-certificate height, and restored block roots before admitting
the node back into validator service.

For a corrupted or stale node, bootstrap from a trusted snapshot:

- fetch snapshot chunks and manifest from a healthy peer;
- verify required metadata roots before import;
- import only if storage, registry, policy, event, nonce, outbox, global, and
  metadata roots match;
- persist required metadata roots and import audit records;
- restart and confirm the same roots are reported by node health and snapshot
  metadata status.

## Upgrades

Governance upgrades must be scheduled, rehearsed, and then executed:

- schedule code or policy changes through the governance contract;
- wait for the timelock height;
- fetch `get_scheduled_upgrades` or `get_scheduled_policy_updates`;
- fetch `get_upgrade_rehearsal_report` and require no invariant failures;
- execute the upgrade only after rehearsal roots and old/new code hashes are
  reviewed;
- verify the post-execution block roots and emitted audit events.

Operators must reject manual state edits and contract changes that bypass
governance, timelocks, rehearsal, and invariant checks.

## Validator-Key Rotation

Validator-set changes use signed validator-set metadata updates:

- create a metadata update with added/removed validator keys and an expiry
  height;
- collect signed authorizations from the current validator quorum;
- submit authorizations through `propose_validator_set_metadata_update`;
- monitor `get_validator_set_metadata_update_status` until applied;
- confirm `get_validator_set_metadata_audit_records` includes the update;
- restart one node and confirm the keyring reloads before rolling the rest.

Reject updates with stale expiry heights, mismatched network/chain IDs, unknown
signers, duplicate keys, or insufficient quorum.

## Incident Response

For suspected equivocation:

- fetch slashing evidence with `get_slashing_record`;
- compare finality certificate signers around the height;
- quarantine the validator key and schedule a validator-set metadata update.

For RPC overload:

- move public traffic to bounded HTTP JSON RPC nodes;
- keep operator RPC behind bearer-token authentication and request limits;
- watch `get_mempool_status` for sender concentration and admission pressure.

For state-sync faults:

- inspect `get_snapshot_sync_client_metrics`;
- retry with a different peer;
- require metadata-root verification before serving the imported state.

For bridge incidents:

- verify source-chain finality certificates against trusted validator sets;
- reject proofs with unknown signers, lowered quorum, tampered outbox roots, or
  replayed message IDs;
- rotate bridge trusted source validator sets only through the governed process
  adopted by the deployment.

## Public Testnet Readiness

The checked readiness manifest is `ops/detta-public-testnet-readiness.json`.
Operators should treat `ready_for_public_testnet: false` as authoritative even
when local release gates pass. Public-testnet launch requires:

- a completed `DETTA_E2E_FULL=1 scripts/detta-release-gate.sh` run;
- state sync, RPC, governance, bridge, and DeFi workflow evidence;
- a planned stability window of at least 168 hours;
- no unresolved high or critical audit findings;
- signed and published validator artifacts, finalized launch genesis, and
  published faucet/sample-client distribution.

When those conditions are met, update the manifest, keep the blocker list
empty, run `scripts/detta-verify-readiness-manifests.sh`, and regenerate
`dist/detta-readiness-status.json`, then verify it with
`scripts/detta-verify-readiness-status-report.sh` before publishing the launch
candidate.

## Mainnet Candidate Readiness

The checked mainnet readiness manifest is
`ops/detta-mainnet-candidate-readiness.json`. Operators should treat
`ready_for_mainnet: false` as authoritative even when local release gates pass.
Mainnet candidacy requires:

- public-testnet readiness already achieved;
- closed or governance-accepted audit findings;
- all production acceptance gates passing;
- finalized genesis artifacts;
- validator onboarding evidence;
- governance bootstrap evidence;
- completed launch rehearsal and incident-response drill;
- reproducible release artifacts with SHA-256 roots and detached signature
  metadata.

The hard release gate runs packaged-node launch, genesis-finalization,
incident-response, governance-bootstrap, validator-onboarding,
packaged-client-flow, local-stability, audit-readiness, and release-signing
drills from a temporary release directory. Use the standalone scripts below
when the operator needs retained release-candidate reports under `dist/`.

Use `scripts/detta-release-candidate-evidence-bundle.sh` to produce retained
release-candidate evidence after the hard gate passes. The script reruns the
packaged operator drills under one release version, verifies report checksums,
collects release artifacts, drill reports, audit-readiness package, readiness
status report, signing public key, checksum files, and detached signatures,
then writes
`dist/detta-release-candidate-evidence-<version>.json`,
`dist/detta-release-candidate-evidence-<version>.jsonl`, and
`dist/detta-release-candidate-evidence-<version>.tar.gz` plus SHA-256
attestations. It also runs
`scripts/detta-verify-release-candidate-evidence-bundle.sh` against the
bundle. Retain this bundle with the release-candidate publication record and
have reviewers rerun the verifier after transfer; it does not replace the hard
release gate or the real public-testnet stability window.

Use `scripts/detta-package-release.sh` after the hard release gate passes. The
script produces a deterministic archive containing `detta-node` and
`detta-client`, a demo genesis snapshot, sample faucet transaction, SHA-256
attestations, and
`dist/detta-release-<version>.json`. The manifest includes `source_state`
metadata binding the release to the current Git commit, tree, tracked-change
count, untracked-file count, and diff/status roots. For final publication,
export `DETTA_REQUIRE_CLEAN_RELEASE_SOURCE=1` before packaging and before
running `scripts/detta-verify-release-signatures.sh`; development rehearsals
may leave it unset so dirty local gates are reported instead of rejected. Sign
every generated `*.sha256` file with the release key, including the release
manifest checksum:

```sh
for checksum in dist/*.sha256; do
  gpg --armor --output "$checksum.sig" --detach-sign "$checksum"
done
```

Verify the bundle before publication:

```sh
DETTA_RELEASE_SIGNER_FINGERPRINT=<fingerprint> \
  scripts/detta-verify-release-signatures.sh dist/detta-release-<version>.json
```

Publish the matching `*.sig` files, and copy the paths, hashes, signer
identity, and signature paths into
`ops/detta-mainnet-candidate-readiness.json`.

Exercise the signing path in CI or staging with a temporary key:

```sh
DETTA_RELEASE_VERSION=<version> scripts/detta-release-signing-drill.sh
```

The drill packages the release candidate, generates an ephemeral signing key,
signs the manifest and artifact checksum attestations, verifies the bundle
with `scripts/detta-verify-release-signatures.sh`, and writes
`dist/detta-release-signing-drill-<version>.json`. This proves the release
signing pipeline is executable, but it does not replace signing the final
publication bundle with the production release key.

Use `scripts/detta-genesis-finalization-drill.sh` before publishing launch
artifacts. The drill packages the release candidate, verifies release-manifest
bindings for the genesis and faucet artifacts, validates authenticated genesis
roots, records the launch validator identities and quorum, and writes
`dist/detta-genesis-finalization-<version>.json` plus a SHA-256 attestation.
Set `DETTA_FINAL_GENESIS_VALIDATORS` to the comma-separated validator ids and
`DETTA_FINAL_GENESIS_QUORUM` when the default two-thirds-plus-one quorum is not
the intended launch threshold. Retain this report as finalized-genesis
evidence, then sign and publish it with the release key and validator-operator
acknowledgements.

Use `scripts/detta-operator-launch-rehearsal.sh` before publishing a release
candidate. The rehearsal unpacks the packaged validator archive, starts it from
the generated genesis, verifies TCP `get_state_root` against the genesis root,
and writes `dist/detta-launch-rehearsal-<version>.json` plus a SHA-256
attestation. Retain this report as operator-launch evidence.

Use `scripts/detta-incident-response-drill.sh` before publishing a release
candidate and during validator onboarding. The drill starts the packaged
validator, creates a reverted transfer, records an expected RPC error, leaves a
pending transaction in the mempool, checks operator health, metrics, alert
codes, snapshot metadata roots, and absence of slashing evidence, then writes
`dist/detta-incident-response-drill-<version>.json` plus a SHA-256 attestation.
Retain this report as incident-response evidence.

Use `scripts/detta-governance-bootstrap-drill.sh` before publishing a release
candidate and before declaring governance bootstrap complete. The drill starts
the packaged validator, schedules a timelocked code upgrade through `GovA`,
checks the queue and rehearsal report, verifies early execution is rejected,
executes after the timelock, then repeats the timelock check for a method
policy update. It writes
`dist/detta-governance-bootstrap-drill-<version>.json` plus a SHA-256
attestation. Retain this report as governance-bootstrap evidence.

Use `scripts/detta-validator-onboarding-drill.sh` before publishing a release
candidate and before declaring validator onboarding complete. The drill starts
the packaged validator binary with four validator identities against the same
genesis, verifies each identity's health, state root, and persistent snapshot
roots, restarts each validator without a genesis file, and verifies the roots
remain stable. It writes
`dist/detta-validator-onboarding-drill-<version>.json` plus a SHA-256
attestation. Retain this report as validator-onboarding evidence.

Use `scripts/detta-packaged-client-flow-drill.sh` before publishing the sample
client. The drill unpacks the release archive, starts packaged `detta-node`,
runs packaged `detta-client` through token deployment, pool deployment,
liquidity, buy, and sell commands, verifies committed receipts, and writes
`dist/detta-packaged-client-flow-drill-<version>.json` plus a SHA-256
attestation. Retain this report as packaged-client evidence.

Use `scripts/detta-public-testnet-stability-drill.sh` before opening the real
public-testnet stability window. The drill runs a packaged local node through a
bounded multi-block DeFi workload, verifies health, operator metrics, mempool
drain, root consistency, and alert status, and writes
`dist/detta-public-testnet-stability-drill-<version>.json` plus a SHA-256
attestation. Retain this report as local preflight evidence; it does not
replace the required 168-hour public testnet stability window.

Use `scripts/detta-audit-readiness-package.sh` before handing a release
candidate to external reviewers. The script packages tracked source, generated
release artifacts, checksum attestations, proof manifests, readiness manifests,
and `security/detta-audit-findings.json` into
`dist/detta-audit-readiness-package-<version>.tar.gz`, then writes
`dist/detta-audit-readiness-<version>.json` plus SHA-256 attestations and runs
`scripts/detta-verify-audit-readiness-package.sh` against the package. The
audit report must carry the same `source_state` metadata as the release
manifest. For final reviewer handoff, export
`DETTA_REQUIRE_CLEAN_AUDIT_PACKAGE=1` while creating and verifying the package.
Retain the package and report as reviewer input, and have reviewers rerun the
verifier after transfer; audit completion still requires closed or
governance-accepted findings in the checked security manifest.

When those conditions are met, update the manifest, keep the blocker list
empty, include every signed release artifact, and run
`scripts/detta-verify-readiness-manifests.sh`, then regenerate
`dist/detta-readiness-status.json` and verify it with
`scripts/detta-verify-readiness-status-report.sh` before proposing a mainnet
release candidate.
