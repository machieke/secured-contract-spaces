# DeTTa DA Operator Runbook

This runbook covers the current experimental DeTTa data availability layer.
Use it with `detta-data-availability-layer.md` and the implementation tracker
in `detta-data-availability-layer-implementation-plan.md`.

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

## Operator Checks

Before pruning:

1. Query `get_da_storage_stats`.
2. Verify `missing_share_count == 0` for manifests still inside the hot
   retention window.
3. Query `get_operator_alerts`.
4. Confirm no `operator.da_missing_shares`, `operator.da_repair_pending`,
   `operator.da_repair_lag`, `operator.da_custody_failure`, or
   `operator.da_challenge_failure` alert is active.
5. Reconstruct at least one recent DA payload with `get_da_payload`.
6. Verify light-client samples with `get_da_sample_proofs`.

After pruning:

1. Query `get_da_storage_stats` again and record the byte delta.
2. Re-run payload reconstruction for retained hot/checkpoint manifests.
3. Run state sync from a DA checkpoint when checkpoint shares were touched.
4. Store the before/after stats and command output in the release or operator
   evidence bundle.

## Repair Handling

When `operator.da_repair_pending` or `operator.da_repair_lag` fires:

1. Identify the manifest with `get_da_repair_status`.
2. Fetch missing shares from independent peers.
3. Reject any share that does not verify against the manifest share root.
4. Reconstruct the payload once the threshold is met.
5. Persist the repaired payload and shares.
6. Clear or supersede the repair record only after reconstruction succeeds.

If a peer serves an invalid share, keep the peer score/evidence with the
operator incident notes. Repeated invalid DA responses should trigger peer
isolation and possible challenge escalation.

## Evidence To Retain

For each release candidate or audit window, retain:

- DA manifests for finalized blocks in scope;
- DA certificates;
- challenge records and slashing evidence;
- DA storage stats before and after pruning;
- sample proof bundles for representative blocks;
- repair records and repair completion notes;
- checkpoint DA manifests and import audit records.

These artifacts demonstrate that finalized blocks were not only valid by
execution and consensus roots, but also reconstructable under the DA protocol.
