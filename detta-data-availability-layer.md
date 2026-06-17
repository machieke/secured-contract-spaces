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

## Threat Model

The DA layer is designed to detect or survive:

- a proposer that commits a header while withholding the payload;
- a proposer that equivocates between manifests for the same height or block;
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
  payload/manifest version.
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
