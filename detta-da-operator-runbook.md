# DeTTa DA Operator Runbook

This runbook covers DeTTa's DA v1 production candidate on the
`experimental/data-availability-layer` branch. Use it with
`detta-data-availability-layer.md` and the implementation tracker in
`detta-data-availability-layer-implementation-plan.md`.

## Retention Classes

Current storage supports these DA retention classes:

- `Hot`: recent block payloads and all shares needed for fast replay.
- `Warm`: recent historical payloads where payload bytes may be retained while
  some shares can be repaired from peers.
- `Cold`: older block data retained mainly for audit and archive retrieval.
- `Checkpoint`: snapshot/checkpoint payloads used for state sync.

Each policy entry specifies:

- whether reconstructed payloads are retained;
- whether all shares are retained;
- minimum retention in blocks;
- optional payload byte budget.

## Retention Policy Guidance

Recommended production defaults before mainnet hardening:

- retain all `Hot` shares for at least the finality challenge window;
- retain checkpoint shares longer than ordinary block shares;
- retain reconstructed payloads for any block that contains governance,
  bridge, oracle, or aspect deployment records;
- never prune the only local copy of a manifest or DA certificate;
- require at least one successful archive reconstruction test before reducing
  local share retention.

Operators should treat retention changes as governance-sensitive operational
changes. A policy that drops shares too early can preserve data integrity roots
while weakening practical data availability.

DA slashing policy changes are chain-governed through the
`detta.da-slashing-policy` scope. Operators should retain the scheduling and
execution receipts for any change to missing-response slashability,
invalid-response slashability, or observed-delay windows.

## Operator Checks

Before pruning, use raw RPC or the matching `detta-client` DA inspection
commands:

1. Query `get_da_storage_stats`.
2. Query `get_da_retention_audit`.
3. Verify `unsatisfied_manifest_count == 0` and inspect any active manifest
   whose `retention_satisfied` field is false.
4. Query `get_da_retention_prune_plan` and confirm every candidate manifest is
   expired or contains shares no longer required by the active policy while a
   retained payload is present.
5. Query `get_application_da_retention_audit` and confirm active application
   manifests have `retention_satisfied == true`; custom application retention
   classes remain unsatisfied until a governed policy maps them.
6. Query `get_application_da_retention_prune_plan` and confirm candidates only
   name application payload and share files.
7. Keep manifests, DA certificates, indexes, challenge records, and audit
   records out of pruning scope.
8. Query `get_operator_alerts`.
9. Confirm no `operator.da_missing_shares`, `operator.da_repair_pending`,
   `operator.da_repair_lag`, `operator.application_da_missing_shares`,
   `operator.application_da_repair_pending`,
   `operator.application_da_repair_lag`, `operator.da_custody_failure`, or
   `operator.da_challenge_failure` alert is active.
10. Reconstruct at least one recent DA payload with `get_da_payload`.
11. Reconstruct at least one recent application DA payload with
    `get_application_da_payload`.
12. Verify light-client samples with `get_da_sample_proofs` and
    `get_application_da_sample_proofs`.
13. Confirm DA index bytes are nonzero after finalized DA blocks and that
   manifest/certificate lookups by block coordinates, namespace, and retention
   class return expected entries.
14. Confirm application manifest/certificate lookups by application id, profile
    id, coordinate, namespace, retention class, and application root return
    expected entries.
15. Confirm the active DA slashing policy matches the current governance
   decision before processing challenge evidence.

The packaged client command names mirror the workflow: `produce-da-block`,
`da-stats`, `da-retention-audit`, `da-retention-prune-plan`,
`application-da-retention-audit`, `application-da-retention-prune-plan`,
`da-payload`, `application-da-payload`, `da-sample-proofs`,
`application-da-sample-proofs`, `da-manifest-index-by-*`,
`application-da-manifest-index-by-*`, `da-certificate-index-by-*`, and
`application-da-certificate-index-by-*`.

After pruning:

1. Query `get_da_storage_stats` again and record the byte delta.
2. Query `get_da_retention_audit` and `get_application_da_retention_audit`
   again and confirm active manifests remain satisfied.
3. Re-run payload reconstruction for retained hot/checkpoint manifests and
   representative retained application manifests.
4. Run state sync from a DA checkpoint when checkpoint shares were touched.
5. Store the before/after stats and command output in the release or operator
   evidence bundle.
6. Rebuild DA indexes from stored manifests and certificates if backup restore,
   manual repair, or disk maintenance touched `da/indexes`.

## Repair Handling

When `operator.da_repair_pending` or `operator.da_repair_lag` fires:

1. Identify the manifest with `get_da_repair_status`.
2. Fetch missing shares from independent peers.
3. Reject any share that does not verify against the manifest share root.
4. Reconstruct the payload once the threshold is met.
5. Persist the repaired payload and shares.
6. Clear or supersede the repair record only after reconstruction succeeds.

When `operator.application_da_repair_pending` or
`operator.application_da_repair_lag` fires:

1. Identify the application manifest with `get_application_da_repair_status`.
2. Fetch missing application shares from independent peers.
3. Reject any share that does not verify against the application manifest share
   root and profile id.
4. Reconstruct the application payload once the threshold is met.
5. Persist the repaired application payload and shares.
6. Clear or supersede the application repair record only after reconstruction
   succeeds and `get_application_da_retention_audit` is satisfied.

If a peer serves an invalid share, keep the peer score/evidence with the
operator incident notes. Repeated invalid DA responses should trigger peer
isolation and possible challenge escalation.

## Evidence To Retain

For each release candidate or audit window, retain:

- DA manifests for finalized blocks in scope;
- DA certificates;
- DA index roots or lookup evidence for representative finalized blocks;
- DA slashing policy schedule/execute receipts when policy changed;
- challenge records and slashing evidence;
- DA storage stats before and after pruning;
- DA retention audit reports before and after pruning;
- DA retention prune-plan reports used to justify removed local payload/share
  files;
- application DA retention audit reports before and after pruning;
- application DA retention prune-plan reports used to justify removed local
  application payload/share files;
- sample proof bundles for representative blocks;
- application DA sample proof bundles for representative application manifests;
- repair records and repair completion notes;
- application DA repair records and repair completion notes;
- checkpoint DA manifests and import audit records.

These artifacts demonstrate that finalized blocks were not only valid by
execution and consensus roots, but also reconstructable under the DA protocol.
