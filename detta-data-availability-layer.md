# DeTTa Data Availability Layer

This document records the DA v1 vocabulary, threat model, and versioned payload
schema for DeTTa's production DA candidate on the
`experimental/data-availability-layer` branch. The implementation plan remains
in `detta-data-availability-layer-implementation-plan.md`.

## Glossary

- **DA payload**: the canonical bytes required to independently replay, audit,
  or synchronize a block or checkpoint. The current payload schema is
  `detta.da-payload.v1` and is serialized by `DaPayload`.
- **Namespace**: a deterministic section of the DA payload, such as
  `detta.block`, `detta.tx`, `detta.receipt`, or `detta.snapshot`.
- **DA manifest**: the `detta.da-manifest.v1` commitment record for a payload.
  It binds chain, height, execution block hash, payload hash, namespace root,
  share root, erasure scheme, share counts, share hashes, and namespace ranges.
- **Share**: one encoded payload shard addressed by manifest hash and share
  index. Shares are individually hash checked against the manifest.
- **Share root**: the Merkle root over manifest share hashes.
- **Namespace root**: the canonical hash over manifest namespace ranges.
- **Reconstruction threshold**: the minimum number of valid shares required to
  reconstruct the payload.
- **DA availability vote**: a validator statement that it verified assigned
  custody or sampled shares for a manifest.
- **DA certificate**: a quorum aggregation of DA availability votes for one
  manifest.
- **DA index**: durable lookup metadata under `da/indexes` that maps manifests
  by height and block hash, and certificates by manifest hash, height, and block
  hash. Index entries are checked against the stored manifest or certificate on
  load.
- **Custody assignment**: deterministic validator-to-share assignment used
  before signing DA availability votes.
- **Sample schedule**: deterministic light-client sample indices derived from
  the manifest hash, block hash, and client randomness.
- **Repair record**: durable local evidence that a manifest needs missing-share
  repair or payload reconstruction work.
- **DA slashing policy**: chain-governed parameters for whether missing
  challenge responses or invalid challenge responses are slashable, plus
  minimum and maximum observed-delay windows for admissible evidence.
- **Coding-fraud proof**: transferable evidence (`DaCodingFraudProof`) that a
  manifest's committed shares are not a valid erasure encoding of its committed
  payload, derived from the proposer's own hash-bound data shares and slashing
  the block proposer when recorded in consensus.

## Threat Model

The DA layer is designed to detect or survive:

- a proposer that commits a header while withholding the payload;
- a proposer that equivocates between manifests for the same height or block;
- a proposer that commits a manifest whose `payload_hash` and `namespace_root`
  match the real payload but whose `share_root`/`share_hashes` are not a valid
  erasure encoding of it. Full-payload verification rebuilds the share set
  deterministically from the manifest parameters and rejects any manifest that
  does not commit the payload (`verify_manifest_commits_payload`, enforced in
  `verify_data_availability_payload`);
- validators signing availability without verifying assigned shares;
- peers serving shares from the wrong manifest;
- peers serving corrupted shares;
- peers omitting shares during state sync;
- stale or replayed DA manifests and certificates;
- payloads that exceed bounded memory, disk, or request limits;
- loss of aspect module deployment bytes after contract deployment;
- missing bridge, oracle, governance, or receipt inputs needed for audit;
- recent validators going offline while archive/full nodes still need to
  reconstruct data.

The current layer does not provide payload privacy. DeTTa DA payload data is
public unless a future privacy layer encrypts payload sections before
commitment.

## Versioned Payload Schema

The current canonical payload schema is `detta.da-payload.v1`, represented in
code by `DA_PAYLOAD_SCHEMA` and `DA_PAYLOAD_VERSION = 1`.

`DaPayload` fields:

- `chain_id`: DeTTa chain identifier.
- `height`: block or checkpoint height.
- `previous_block_hash`: previous execution block hash or checkpoint anchor.
- `block_payload_version`: must equal `1`.
- `namespaces`: sorted canonical namespace sections.

Each namespace section contains:

- `namespace`: lowercase dot-separated namespace ID.
- `records`: ordered DA records for that namespace.

Current DA record variants include:

- block headers;
- signed transactions;
- aspect artifacts;
- governance payloads;
- bridge proofs;
- oracle evidence;
- receipts;
- events;
- compact state diffs;
- snapshot chunk references;
- snapshot chunk manifests;
- snapshot chunks.

Canonicalization rules:

- namespace IDs must be lowercase ASCII letters, digits, and dots;
- namespace IDs cannot be empty, start or end with a dot, or contain `..`;
- sections with the same namespace are merged;
- namespace sections are sorted by namespace;
- empty namespace sections are rejected;
- canonical JSON bytes are hashed for payload and manifest commitments.

Compatibility rule: new payload fields or record semantics require a new
payload version and schema name. Existing v1 payload bytes must remain stable so
fixture roots, manifests, certificates, and replay evidence stay auditable.

## Production DA v1 Profile

The production v1 profile is represented in code by
`DaProductionProfile::v1()` and schema `detta.da-production-profile.v1`.

Production v1 decisions:

- Share commitments use Merkle SHA-256 roots over encoded share hashes.
- Erasure coding uses Reed-Solomon v1; KZG commitments are deferred to a future
  payload/manifest version. Until then, coding-correctness is enforced two ways.
  First, any node that verifies the full payload re-derives the share set and
  rejects a manifest whose commitment does not encode the payload
  (`verify_manifest_commits_payload`). Second, a node that fetched the
  `original_share_count` committed data shares can publish a transferable,
  slashable coding-fraud proof (`DaCodingFraudProof`) that demonstrates — from
  the proposer's own hash-bound shares — that the manifest is not a valid
  encoding of its committed payload (either a re-encoded parity share or the
  decoded payload hash diverges from the commitment). The proof is verifiable by
  anyone holding only the manifest and, in consensus, slashes the block proposer
  (`record_data_availability_coding_fault`). A polynomial commitment (KZG) would
  additionally let a light client reject coding fraud from a single sampled share
  without reconstructing the data-share set.
- DA block production derives equal data/parity Reed-Solomon share counts from
  the operator-provided target share size while respecting the v1 max-share
  bound.
- Validator DA votes use deterministic custody assignments and may also carry
  light-client sample indices.
- Production RPC payload serving requires a full locally verified or
  reconstructed payload, not a partial best-effort response.
- Block DA payloads must carry one `detta.block` header section. If transaction
  records are present in `detta.tx`, matching receipt records must be present in
  `detta.receipt`.
- Raw events are not mandatory block DA records in v1. Event integrity is
  replay-deterministic through the block `event_root` and per-receipt
  `event_root_after`; optional `detta.event` sections are type-checked when
  present.
- Data gas is priced in 1024-byte units by the v1 profile helper.
- Validator hot retention is at least 65,536 blocks; archive/checkpoint
  retention is at least 1,048,576 blocks.
- DA slashing remains governed by timelocked validator-set governance.
- Archive/storage providers are expected to be governance-registered before
  public mainnet.

The enforced production block-payload validator rejects unsupported namespaces,
records in the wrong namespace, multiple block headers, receipts without
transactions, and transaction/receipt count mismatches.

## External Blob Storage Adapters

Application-neutral DA supports large-object integration through external blob
reference records instead of putting bulk media bytes directly into DA payloads.
The canonical record schema is `detta.external-blob-reference.v1`, represented
by `DaExternalBlobReference`.

The DA crate includes deterministic reference adapters for:

- `IpfsAdapter`: canonical `ipfs://...` references;
- `ArweaveAdapter`: canonical `ar://...` references;
- `FilecoinAdapter`: canonical `filecoin://...` references.

Each adapter can turn an already-uploaded blob locator plus local bytes into a
hash-bound `ExternalContentAddress` DA record. The record commits:

- backend;
- canonical URI;
- SHA-256 content hash of the external blob bytes;
- MIME content type;
- byte size;
- optional provider/pinning/deal reference;
- optional availability proof string.

Retrieval is two-step. First, a client retrieves and verifies the DA manifest,
payload, namespace proof, and external reference record. Then the client fetches
the actual bytes from IPFS, Arweave, Filecoin, a gateway, or a provider API and
calls `verify_external_blob_record` or the matching adapter's
`verify_record_blob`. Verification rejects wrong backends, noncanonical record
JSON, invalid URI schemes, size mismatches, and hash mismatches.

External blob operations are now modeled as canonical, hash-addressed records:

- `DaExternalBlobLifecycleRecord` tracks upload, pin, archive, Filecoin deal,
  repair, and deprecation stages for a committed external reference.
- `DaExternalBlobProviderHealthRecord` records provider reachability checks,
  latency, degradation, and unavailable-provider failure reasons.
- `DaExternalBlobRepairJob` records repair intent and planned actions such as
  repinning, mirroring to another backend, renewing a Filecoin deal, or
  replacing a provider.
- `DaExternalBlobRetrievalVerification` is the retrieval verification API: it
  binds verifier, provider, height, returned byte count, returned content hash,
  and success/failure reason to the committed `DaExternalBlobReference`.
- `DaExternalBlobReplicationPolicy` and
  `verify_external_blob_replication_policy` support multi-backend policies such
  as `ipfs-plus-arweave-archive`.
- `DaExternalBlobAvailabilityChallenge` and
  `DaExternalBlobChallengeEvidence` create DA challenge evidence when a
  committed external blob becomes unavailable, returns incorrect bytes, or fails
  a replication policy.

Persistent nodes store blob lifecycle, provider health, repair-job, and
challenge-evidence records under `da/applications`, so they are included in the
application DA root, storage stats, backups, and restore flows. RPC clients can
record and fetch those append-only logs with the
`record_application_da_external_blob_*` and
`get_application_da_external_blob_*` methods, and can call
`verify_application_da_external_blob_retrieval` to verify retrieved bytes
against a committed external-reference record without making DeTTa DA serve
large blob data directly.

This keeps DeTTa DA responsible for durable publication, indexing,
certification, and audit commitments, while blob networks handle large-byte
storage and serving.

## Client SDK

`crates/detta-client-sdk` provides a Rust SDK over the external blob and
application DA APIs. `sdk/javascript` provides the browser JavaScript SDK for
web clients using `fetch` and `crypto.subtle`. The browser SDK's main extension
point is data-driven: `defineApplication` describes namespaces, record
policies, root bindings, retention classes, and coordinate templates, then
`sdk.application(definition)` builds Rust-compatible profiles, record
envelopes, and canonical application DA batches. Social avatar and PurpleFrenZ
chat helpers are recipes over that generic layer.

For external blob flows, `uploadExternalBlobReference` uploads bytes through a
pluggable blob client and commits a canonical external-reference record.
`retrieve_verified_blob`/`retrieveVerifiedBlob` fetches the external bytes,
verifies them against the committed reference, and records the same
retrieval-verification evidence exposed by RPC.

The SDK is operationally atomic and retry-friendly for DeTTa writes: profile
registration can be retried, duplicate registration is treated as success, the
application DA batch is produced in one RPC call, and lifecycle metadata is
recorded only after batch production succeeds. The external blob upload itself
is outside DeTTa consensus, so production integrations should use content
addressed or idempotent provider uploads and provider repair policies.

Fresh and restarted persistent nodes commit `DaRetentionPolicyConfig::production_default()`
when no DA retention policy is already present. Operator-provided policies are
left intact. `get_da_storage_stats` reports the active retention policy, its
content root, and its encoded byte size. `get_da_retention_audit` classifies
stored manifests against the active policy using the node's current height and
reports active versus expired manifests, missing policy classes, and any local
payload/share retention obligations that are not satisfied.
`get_da_retention_prune_plan` is a non-destructive dry run that reports expired
payload/share files and byte totals that are no longer required by the active
policy; it deliberately excludes manifests, DA certificates, indexes, challenge
records, and audit evidence. The current v1 classification is derived from
manifest namespaces: checkpoint payloads use `Checkpoint`, and ordinary block DA
payloads use `Hot`.

Persistent validator `get_node_health`, `get_operator_metrics`, and embedded
operator alert metrics expose the active `da_production_profile`. Operators can
compare that object with `DaProductionProfile::v1()` to confirm the node is
running the expected DA commitment, custody, retention, slashing, archive, and
data-gas policy.

## Durable Storage Indexes

The storage layer persists DA objects by content hash and also keeps coordinate
indexes for operator and sync workflows:

- manifests by block height;
- manifests by execution block hash;
- manifests by DA namespace;
- manifests by derived retention class;
- certificates by manifest hash;
- certificates by block height;
- certificates by execution block hash.

Index lookups return lists, not singletons, so fork or equivocation evidence for
the same height or block hash can remain discoverable. Each lookup reloads the
referenced object and rejects stale, unsorted, duplicate, or mismatched index
entries, including namespace and retention-class mismatches. `rebuild_da_indexes`
reconstructs the indexes from stored manifests and certificates after restore or
repair, and the DA store root includes the index root so index drift is
externally visible. Persistent-node RPC exposes manifest index lookups by height,
block hash, namespace, and retention class, plus certificate index lookups by
manifest hash, height, and block hash for operator and indexer evidence
collection.

## Governed Slashing Policy

The active `DaSlashingPolicy` is part of deterministic DeTTa state and is
updated only through a timelocked governance contract scoped to
`detta.da-slashing-policy`. The policy controls:

- whether missing-response evidence is slashable;
- whether invalid-response evidence is slashable;
- the minimum observed delay between the DA height and evidence observation;
- the maximum observed delay, where `0` means unbounded.

Persistent nodes validate DA challenge evidence against the active policy before
persisting a slashing record. Consensus clusters use the same validation before
removing a validator from the active quorum.
