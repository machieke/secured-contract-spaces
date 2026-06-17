# DeTTa Data Availability Layer

This document records the current experimental DA vocabulary, threat model, and
versioned payload schema for DeTTa. The implementation plan remains in
`detta-data-availability-layer-implementation-plan.md`.

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

## Durable Storage Indexes

The storage layer persists DA objects by content hash and also keeps coordinate
indexes for operator and sync workflows:

- manifests by block height;
- manifests by execution block hash;
- certificates by manifest hash;
- certificates by block height;
- certificates by execution block hash.

Index lookups return lists, not singletons, so fork or equivocation evidence for
the same height or block hash can remain discoverable. Each lookup reloads the
referenced object and rejects stale, unsorted, duplicate, or mismatched index
entries. `rebuild_da_indexes` reconstructs the indexes from stored manifests and
certificates after restore or repair, and the DA store root includes the index
root so index drift is externally visible.

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
