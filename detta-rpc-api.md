# DeTTa JSON RPC API

DeTTa exposes the same typed JSON RPC wire format over two transports:

- bounded HTTP `POST` requests to `/` or `/rpc`;
- line-delimited JSON RPC over TCP, where each request is a single JSON object
  followed by `\n` and each response is a single JSON object followed by `\n`.

The wire enum is defined in `crates/detta-rpc/src/lib.rs` as `RpcRequest` and
`RpcResult`. `detta-rpc-openapi.json` is the versioned OpenAPI 3.1 schema for
the bounded HTTP JSON RPC endpoint. Stable JSON fixture tests in that crate pin
selected client-facing response shapes.

## Request and Response Shape

Public requests use the serde-tagged form:

```json
{"method":"get_state_root"}
```

Requests with parameters use `params`:

```json
{"method":"get_balance","params":{"contract":"TokenA","owner":"Alice","asset":"USDC"}}
```

Successful responses use:

```json
{"status":"ok","body":{"result":"state_root","data":"..."}}
```

Errors use stable code strings:

```json
{"status":"error","body":{"code":"rpc.receipt_not_found","message":"receipt was not found"}}
```

Malformed JSON requests return `rpc.decode_error`. Oversized line-delimited
requests are rejected by the TCP transport before dispatch. Oversized HTTP
bodies return HTTP `413` with stable RPC code `rpc.request_too_large`; HTTP
requests without `Content-Length`, non-`POST` methods, malformed headers, or
unknown paths return stable JSON RPC error bodies with non-`200` HTTP status
codes.

## Public Methods

The shared `RpcService` supports:

- `submit_transaction`: admit a signed transaction into the local mempool.
- `produce_block`: produce and apply a local block from pending transactions.
- `import_block`: validate and apply a supplied block.
- `get_transaction`: fetch a committed transaction by hash.
- `get_receipt`: fetch a committed receipt by transaction hash.
- `get_receipt_proof`: fetch a Merkle proof for a receipt by block height and
  receipt index.
- `get_block`: fetch a committed block by height.
- `get_blocks_page`: fetch a bounded page of committed blocks from a start
  height for catch-up clients.
- `get_node_health`: fetch chain, height, mempool, and root diagnostics.
- `get_operator_metrics`: fetch peer count, mempool size, consensus and
  finality height, recent execution/proof latency, storage bytes, and RPC error
  count for persistent operator nodes.
- `get_operator_alerts`: fetch evaluated operator alerts for peer isolation,
  stalled consensus, root mismatch, excessive reverts, slashing evidence, disk
  pressure, RPC overload, and mempool saturation.
- `get_mempool_status`: fetch pending transaction count, per-sender pending
  counts, admission limits, and the current block resource limit.
- `get_state_root`: fetch the latest global state root.
- `get_snapshot`: fetch the latest state snapshot.
- `get_balance`: read a token balance view.
- `get_total_supply`: read a token supply view.
- `get_storage_proof`: fetch a storage inclusion proof.
- `get_storage_non_inclusion_proof`: fetch a storage absence proof.
- `get_registry_proof`: fetch a registry inclusion proof.
- `get_registry_non_inclusion_proof`: fetch a registry absence proof.
- `get_outbox_message_proof`: fetch a cross-shard outbox message proof.
- `get_event_proof`: fetch a Merkle proof for an event by global event index.
- `get_events`: fetch the current event log.
- `get_events_page`: fetch a bounded event-log page with `offset`, effective
  `limit`, and `total_events` for indexer catch-up.
- `subscribe`: create a polling subscription for `blocks`, `receipts`,
  `events`, and/or `finality`; an empty topic list subscribes to all four
  topics.
- `get_subscription_events`: fetch a bounded page of subscription
  notifications from a subscription sequence cursor.
- `get_contract`: fetch a deployed contract descriptor.
- `get_scheduled_upgrades`: list scheduled governance code upgrades and their
  execution flags.
- `get_scheduled_policy_updates`: list scheduled governance policy updates and
  their execution flags.
- `get_upgrade_rehearsal_report`: rehearse a scheduled upgrade on forked state
  and return invariant failures plus authenticated roots.

Read-only view and proof methods must not alter nonce, event, registry, storage,
or global roots.

## Persistent Validator Methods

Persistent validator nodes additionally handle:

- `get_finality_certificate`: fetch a persisted finality certificate by block
  height for light-client and audit verification.
- `get_slashing_record`: fetch a persisted slashing record by validator ID for
  equivocation evidence audits.
- `propose_validator_set_metadata_update`: submit a signed validator-set update
  authorization.
- `get_validator_set_metadata_update_status`: report pending authorization
  count, quorum, and applied state.
- `get_validator_set_metadata_audit_records`: page validator-set metadata audit
  records.
- `get_persistent_node_snapshot_roots`: return state roots plus persistent
  metadata audit roots.
- `get_snapshot_metadata_root_status`: explain effective, local, and imported
  snapshot metadata roots.
- `get_snapshot_sync_client_metrics`: return persisted state-sync diagnostics.
- `get_required_snapshot_metadata_roots`: return the exact metadata-root set
  required by the last verified snapshot import.
- `get_snapshot_import_audit_records`: page snapshot import audit records.
- `get_snapshot_import_audit_root`: return the snapshot import audit root.
- `get_snapshot_import_audit_config_root`: return the import audit retention
  config root.
- `get_snapshot_import_audit_config`: return import audit retention limits.

Persistent nodes serve `get_block`, `get_transaction`, `get_receipt`, and
`get_receipt_proof` from durable block storage after restart. This lets wallets,
indexers, and auditors recover finalized history even when the in-memory
validator indexes are empty after process recovery.

## Proof Methods

Proof responses contain the proved value and a Merkle proof:

- storage and registry proofs verify against the corresponding block or
  snapshot root;
- receipt proofs verify against `BlockHeader.receipt_root`;
- event proofs verify against the current cumulative event root;
- outbox proofs verify against `BlockHeader.outbox_root` for bridge finality
  verification.

Clients should call each proof type's `verify` logic or reproduce the same
leaf/node hashing rules before trusting a value.

## Operator Endpoint Guards

Public request compatibility is unchanged. Operator deployments can opt into
wrapper handlers:

- `AuthenticatedJsonRpcHandler`: expects an authenticated envelope:

```json
{"bearer_token":"operator-secret","request":{"method":"get_state_root"}}
```

Invalid or missing tokens return `rpc.authentication_required`.

- `RateLimitedJsonRpcHandler`: caps accepted requests per handler instance.
  Excess requests return `rpc.rate_limited`.

Wrappers compose through the `JsonRpcHandler` trait and can protect
operator-only server instances without changing the public RPC request enum.

## Stability

Patch releases must preserve method tags, result tags, and stable error codes
unless a protocol-versioned migration explicitly says otherwise. Fixture tests
currently cover node health, snapshot metadata root status, receipt-proof, and
event-proof response shapes.
