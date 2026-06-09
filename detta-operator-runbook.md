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

- verify the binary with `scripts/detta-release-gate.sh`;
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
