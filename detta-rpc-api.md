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

- `submit_transaction`: admit a legacy transaction into the local mempool after
  checking its chain, nonce, size, budget, and admission signature status.
- `submit_signed_transaction`: verify a client Ed25519 signed transaction
  envelope, require the public key to be registered for the sender account in
  the authenticated registry, mark the verified inner transaction as
  signature-valid, and admit it into the local mempool. Invalid signatures
  return `mempool.invalid_signature`; valid signatures from unregistered sender
  keys return `mempool.unauthorized_signer`; expired transactions return
  `mempool.transaction_expired`.
  Account signer grants are managed by normal transactions against the deployed
  account-registry contract using `registerAccountKey` and `revokeAccountKey`;
  their state is visible through registry proof RPCs.
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
  count for persistent operator nodes, plus DA manifest/share/payload,
  challenge, custody-failure, repair-lag, and byte counters.
- `get_operator_alerts`: fetch evaluated operator alerts for peer isolation,
  stalled consensus, root mismatch, excessive reverts, slashing evidence, disk
  pressure, RPC overload, mempool saturation, missing DA shares, pending or
  lagging DA repair, custody failures, and DA challenge failures.
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
- `get_aspect_module`: fetch a registered aspect module record by module hash.
- `get_aspect_module_proof`: fetch a Merkle proof for a registered aspect
  module by module hash.
- `get_aspect_module_artifacts`: fetch the module's root-authenticated artifact
  report, including bundle IDs, ABI entries, method policies, storage schema,
  registry schema, and invariant definitions when the module was submitted with
  source-backed IR.
- `get_aspect_modules`: list registered aspect module records.
- `get_scheduled_upgrades`: list scheduled governance code upgrades and their
  execution flags.
- `get_scheduled_policy_updates`: list scheduled governance policy updates and
  their execution flags.
- `get_upgrade_rehearsal_report`: rehearse a scheduled upgrade on forked state
  and return invariant failures plus authenticated roots.

Read-only view and proof methods must not alter nonce, event, registry, storage,
or global roots.

## Aspect Module Workflow

Clients deploy programmable DeFi behavior through the factory:

1. Submit a verified source-backed module with a `SubmitAspectModule`
   transaction against `FactoryA`.
2. Inspect the returned module through `get_aspect_module_artifacts` and verify
   module inclusion with `get_aspect_module_proof`.
3. Deploy a contract with `DeployAspectContract`, passing the registered
   `module_hash`, selected bundle ID, and optional initializer projection.
4. Call exported projections with `Method::Other("<projection>")`, then verify
   receipts and storage proofs for the resulting aspect state.

Roots-only module records remain queryable, but their artifact report has
`has_ir: false` and empty ABI, policy, schema, and invariant maps. Source-backed
modules expose those maps directly so wallets and deployment tools can review
the behavior before deployment.

## Persistent Validator Methods

Persistent validator nodes additionally handle:

- `get_finality_certificate`: fetch a persisted finality certificate by block
  height for light-client and audit verification.
- `get_slashing_record`: fetch a persisted slashing record by validator ID for
  equivocation evidence audits.
- `get_da_manifest`: fetch a persisted experimental DA manifest by manifest
  hash.
- `get_da_share`: fetch one persisted experimental DA share by manifest hash
  and share index.
- `get_da_certificate`: fetch a persisted experimental DA availability
  certificate by certificate hash.
- `get_da_challenge_record`: fetch a persisted DA share challenge record by
  challenge hash, including response and slashing evidence when present.
- `get_da_payload`: reconstruct and return the canonical experimental DA
  payload from locally persisted shares.
- `get_da_namespace`: reconstruct the canonical experimental DA payload and
  return one namespace section, such as `detta.block`, `detta.tx`, or
  `detta.receipt`.
- `get_da_sample_proofs`: derive deterministic light-client sample indices
  from manifest hash, block hash, and client randomness, then return the
  sampled shares with share-root inclusion proofs and optional namespace
  proofs.
- `get_da_status`: report manifest availability, DA certificate availability,
  expected and missing share indices, and payload reconstruction status for a
  manifest hash.
- `get_da_repair_status`: report whether a manifest currently needs repair,
  which share indices are missing, and whether payload reconstruction succeeds.
- `get_da_coding_fraud_proof`: evaluate whether a locally stored manifest's
  committed shares are a valid erasure encoding of its committed payload, and
  return a transferable, slashable coding-fraud proof built from the proposer's
  own committed data shares when they are not. Requires the manifest and all
  data shares to be locally present.
- `get_da_storage_stats`: report persisted experimental DA manifest count,
  expected shares, stored shares, missing shares, reconstructed payloads,
  challenge and repair records, validated index files, and DA byte totals.
- `get_da_retention_audit`: report manifest retention class, active/expired
  window, payload/share presence, missing policy classes, and unsatisfied local
  retention obligations under the active DA retention policy.
- `get_da_retention_prune_plan`: report expired payload/share files and byte
  totals that are no longer required by the active DA retention policy. This is
  non-destructive and does not remove manifests, certificates, indexes, or audit
  evidence.
- `get_da_manifest_index_by_height`: return DA manifest index entries for a
  block height.
- `get_da_manifest_index_by_block_hash`: return DA manifest index entries for an
  execution block hash.
- `get_da_manifest_index_by_namespace`: return DA manifest index entries for
  manifests that contain the requested DA namespace.
- `get_da_manifest_index_by_retention_class`: return DA manifest index entries
  for manifests assigned to the requested retention class.
- `get_da_certificate_index_by_manifest`: return DA certificate index entries
  for a manifest hash.
- `get_da_certificate_index_by_height`: return DA certificate index entries for
  a block height.
- `get_da_certificate_index_by_block_hash`: return DA certificate index entries
  for an execution block hash.
- `register_application_da_profile`: persist an active application DA profile
  registration. Built-in clients can register `detta.defi`, `social.demo`, or
  `checkpoint.demo`; arbitrary applications submit the typed profile JSON.
- `plan_application_da_profile_registration`: persist a pending application DA
  profile registration plus a lifecycle record with requester, reason, request
  height, and timelock execution height. Profiles under reserved `detta.*`
  application ids are rejected unless they match the built-in DeTTa profile.
- `activate_application_da_profile`: activate a pending profile after its
  timelock height has elapsed and append an activation lifecycle record.
- `deprecate_application_da_profile`: mark an active profile inactive for new
  payload production while keeping the profile registration available for
  historical verification.
- `record_application_da_profile_migration`: append migration evidence that a
  profile supersedes an existing registered profile.
- `produce_application_da_batch`: validate a submitted `ApplicationDaPayload`
  against its registered active profile, encode Reed-Solomon shares, persist
  payload/manifest/shares, create an application DA availability certificate,
  and return manifest/certificate hashes plus payload/share commitments.
- `get_application_da_profile`: fetch an application DA profile registration by
  profile id.
- `get_application_da_profile_lifecycle_records`: list deterministic lifecycle
  records for planned, activated, deprecated, and migrated application DA
  profiles.
- `get_application_da_profile_index_by_application_id`: list profile
  registrations for an application id.
- `get_application_da_profile_index_by_application_version`: list profile
  registrations for an application id and profile version.
- `get_application_da_manifest`: fetch an application DA manifest by hash.
- `get_application_da_share`: fetch one application DA share by manifest hash
  and share index.
- `get_application_da_certificate`: fetch an application DA availability
  certificate by certificate hash.
- `get_application_da_payload`: return a stored application DA payload,
  reconstructing and caching it from threshold shares when needed.
- `get_application_da_reconstructed_payload`: explicitly reconstruct and return
  the application DA payload through the same verified reconstruction path.
- `get_application_da_namespace`: return one namespace section from the
  reconstructed application DA payload.
- `get_application_da_sample_proofs`: return deterministic sample shares,
  share-root inclusion proofs, optional namespace proofs, and a verification
  report for an application DA manifest.
- `get_application_da_status`: report application manifest availability,
  certificate availability, expected and missing shares, payload
  reconstructability, application id, profile id, and coordinate.
- `get_application_da_repair_status`: report whether an application DA manifest
  needs repair based on missing shares or failed payload reconstruction.
- `get_application_da_retention_audit`: report per-application retention
  obligations, missing policy classes, payload/share presence, and satisfaction.
- `get_application_da_retention_prune_plan`: list prunable application payload
  and share files without marking manifests, certificates, indexes, or evidence
  as removable.
- `get_application_da_manifest_index_by_application_id`: list application
  manifest entries by application id.
- `get_application_da_manifest_index_by_profile_id`: list application manifest
  entries by profile id.
- `get_application_da_manifest_index_by_coordinate`: list application manifest
  entries for an exact application coordinate.
- `get_application_da_manifest_index_by_namespace`: list application manifests
  containing a namespace.
- `get_application_da_manifest_index_by_retention_class`: list application
  manifests assigned to a retention class.
- `get_application_da_manifest_index_by_application_root`: list application
  manifests by committed application root.
- `get_application_da_certificate_index_by_manifest`: list application
  certificates for a manifest hash.
- `get_application_da_certificate_index_by_application_id`: list application
  certificates for an application id.
- `get_application_da_certificate_index_by_profile_id`: list application
  certificates for a profile id.
- `get_application_da_certificate_index_by_coordinate`: list application
  certificates for an exact application coordinate.
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
`get_receipt_proof` from durable block storage after restart. On
`experimental/data-availability-layer`, persistent nodes also serve
experimental DeTTa DA and application-neutral DA profiles, manifests, shares,
certificates, reconstructed payloads, namespace sections, sampling proofs, and
status/index reports from durable DA storage. This lets wallets, indexers, and
auditors recover finalized history even when the in-memory validator indexes
are empty after process recovery.

## Proof Methods

Proof responses contain the proved value and a Merkle proof:

- storage and registry proofs verify against the corresponding block or
  snapshot root;
- receipt proofs verify against `BlockHeader.receipt_root`;
- event proofs verify against the current cumulative event root;
- outbox proofs verify against `BlockHeader.outbox_root` for bridge finality
  verification;
- aspect module proofs verify module records against the aspect module root,
  which is part of the authenticated global state.

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
