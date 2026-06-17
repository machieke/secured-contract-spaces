# DeTTa Production Data Availability Layer Implementation Plan

**Project:** DeTTa  
**Branch:** `experimental/data-availability-layer`  
**Updated:** 2026-06-17
**Goal:** Add a production-grade data availability layer so finalized DeTTa
blocks are not only valid by consensus and execution roots, but also
independently retrievable, reconstructable, and auditable by honest nodes and
clients.

## Implementation Progress

- [x] Production DA implementation plan created.
- [x] Experimental `detta-da` crate added to the workspace.
- [x] DA namespace validation and deterministic namespace ordering implemented.
- [x] Canonical DA payload hashing implemented.
- [x] DA manifest hashing and deterministic chunk/share root commitments
  implemented.
- [x] Local deterministic chunk reconstruction implemented.
- [x] Golden DA fixture roots added for payload, manifest, and share-root
  stability.
- [x] Focused DA tests reject tampered shares, missing shares, wrong manifest
  hashes, and invalid namespaces.
- [x] Core block headers can carry optional experimental DA commitments without
  breaking legacy block serialization.
- [x] Core tests prove DA commitments are bound into block hashes when present.
- [x] Durable storage can persist, reload, backup, and restore DA manifests and
  shares.
- [x] Durable storage rejects DA shares whose stored bytes no longer match the
  committed share hash.
- [x] Protocol envelopes can carry DA manifest and share messages.
- [x] DA availability vote and certificate structs validate and hash canonical
  manifest commitments.
- [x] Protocol envelopes can carry DA availability vote and certificate
  messages.
- [x] DA availability votes use a dedicated signed validator-message domain.
- [x] DA availability votes can carry deterministic custody/sample share
  indices.
- [x] Deterministic validator custody share assignment is implemented.
- [x] Nodes can sign DA availability votes only after assigned custody shares
  verify against the manifest.
- [x] Signed DA availability votes can be verified and aggregated into a
  canonical DA certificate when quorum votes bind the same manifest.
- [x] Persistent nodes can produce an experimental DA-committed block, persist
  the matching share set, and reload it after restart.
- [x] Persistent nodes can gossip a block's committed DA manifest and shares
  from durable storage.
- [x] Persistent nodes store DA manifest/share gossip messages.
- [x] Persistent nodes store DA certificate gossip messages.
- [x] Persistent node RPC can retrieve DA manifests, individual shares,
  certificates, reconstructed payloads, and namespace sections.
- [x] Persistent node RPC exposes DA status reports for manifest availability,
  certificate availability, missing share indices, and payload reconstruction.
- [x] Persistent node RPC exposes DA repair status reports derived from missing
  shares and payload reconstruction.
- [x] Persistent node RPC exposes DA storage stats with manifest/share counts,
  byte totals, and missing-share counts.
- [x] Experimental DA manifests are anchored to the pre-DA execution/body block
  hash, while the manifest hash is bound into the final block header hash.
- [x] Consensus exposes an explicit DA-certified finality verifier that checks
  the block header DA commitment, manifest hash, certificate hash, active
  signer quorum, and pre-DA execution block hash.
- [x] Production-mode DA finality verifies reconstructed payload transaction
  and receipt roots against the block before validator replay.
- [x] Production-mode block finality requires block-header DA commitments and
  DA certificates through `FinalityMode::DataAvailabilityRequired`.
- [x] Persistent nodes store experimental DA certificates through node
  orchestration and RPC.
- [x] Consensus finality requires DA certificates in production mode.
- [x] RPC exposes experimental DA manifest/share/payload/namespace retrieval.
- [x] Reed-Solomon erasure coding and threshold reconstruction are implemented.
- [x] DA share challenge records, responses, invalid-response evidence, and
  timeout evidence are implemented.
- [x] DA challenge evidence can produce durable slashing records.
- [x] Checkpoint snapshots can be encoded as `detta.snapshot` DA payloads,
  reconstructed from Reed-Solomon threshold shares, and imported with DA
  manifest/certificate provenance in snapshot import audit records.
- [x] Checkpoint DA shares can be fetched from multiple TCP peers with bounded
  share requests, invalid share rejection, duplicate accounting, and
  threshold reconstruction.
- [x] DA-certified blocks can be reconstructed from DA payloads and replayed
  after checkpoint import.
- [x] DA-backed state sync is implemented.
- [x] DA payloads, repair records, retention policy structures, and DA store
  roots are persisted in `detta-storage`.
- [x] Persistent nodes install the production DA retention policy on bootstrap
  or restart when no operator policy exists.
- [x] DA storage stats RPC exposes the active retention policy, content root,
  and encoded policy size.
- [x] DA retention audit RPC reports active/expired manifest windows, missing
  policy classes, and unsatisfied local payload/share obligations.
- [x] DA manifest index RPC exposes namespace and retention-class lookups.
- [x] DA share requests are bounded by a node-level maximum and oversized
  requests are rejected.
- [x] DA storage counters and DA missing-share/repair alerts are exposed
  through operator metrics and alerts.
- [x] Persistent node health, operator metrics, and embedded alert metrics
  expose the active production DA profile.
- [x] Light-client DA sampling proof bundles are available over persistent-node
  RPC, with deterministic sample schedules, share-root inclusion proofs,
  namespace proofs, and local verification helpers.
- [x] DA protocol messages have a stable aggregate encoded-envelope fixture
  root covering requests, manifests, shares, votes, challenges, evidence, and
  certificates.
- [x] DA share sync metrics score peers positively for verified shares and
  negatively for invalid DA responses or failed requests.
- [x] DA share serving exposes a per-peer rate-limit hook for bounded request
  admission.
- [x] DA glossary, threat model, and versioned payload schema are documented
  and linked from the production roadmap.
- [x] Operator metrics and alerts cover DA custody-failure slashing evidence,
  pending repair lag, and DA challenge evidence failures.
- [x] DA retention and repair operator runbook is documented.

## 0. Current Baseline

DeTTa already has strong data integrity mechanisms:

- deterministic block execution and replay;
- block headers with transaction, receipt, storage, registry, policy, event,
  nonce, outbox, and global state roots;
- Merkle proofs for receipts, events, storage, registry, aspect modules, and
  cross-shard outbox messages;
- durable validator storage for blocks, certificates, mempool records,
  snapshots, validator metadata, audit records, and sync diagnostics;
- chunked snapshot sync with manifest hashes, chunk hashes, snapshot hashes,
  metadata-root checks, import audit records, and retry metrics;
- finality certificates and signed validator protocol envelopes.

This is sufficient for authenticated replay and verified state sync when at
least one honest reachable node retains and serves the data.

It is not yet a production data availability system. Today, availability is an
operational assumption: data is available if validators/full nodes keep it and
serve it. There is no DA certificate gating block finality, no erasure-coded
share custody, no random sampling protocol, no namespace retrieval proof, no
availability slashing, and no retention or incentive protocol.

## 1. Target Guarantee

A DeTTa block can be finalized only if the data required to independently
reconstruct, replay, audit, and synchronize that block is available under the
DA protocol.

The target finality condition becomes:

```text
valid_execution_roots
and valid_consensus_certificate
and valid_data_availability_certificate
```

The DA layer must guarantee:

- enough erasure-coded shares are available to reconstruct the canonical block
  payload;
- validators sign DA votes only after receiving and validating their required
  shares or samples;
- light clients can verify a DA commitment and certificate without downloading
  the full payload;
- full nodes can reconstruct missing block payloads from DA shares;
- auditors can retrieve all data required for deterministic replay;
- storage providers and validators can be challenged for unavailable signed
  shares;
- DeTTa state sync can use DA-backed block or checkpoint data rather than
  trusting a single snapshot-serving peer.

## 2. Non-Goals For The First Production Slice

The first production DA implementation should not attempt to solve every
research problem at once.

Non-goals for the first slice:

- replacing DeTTa consensus with a new consensus protocol;
- adding token-economic rewards before correctness is enforced;
- using trusted centralized object storage as the DA layer;
- accepting block finality before the DA certificate is complete;
- exposing arbitrary Atomspace data as DA payload without canonicalization;
- relying only on archive RPC nodes for availability;
- requiring zero-knowledge DA proofs in the first implementation.

The first implementation can use Merkle commitments and Reed-Solomon erasure
coding. KZG commitments, namespaced Merkle trees, DAS sampling improvements,
and storage-market incentives can be layered after the DA certificate and
retrieval path are correct.

## 3. Threat Model

The DA layer must handle:

- a proposer that publishes a valid-looking header but withholds block data;
- a proposer that equivocates between DA payloads for one block height/hash;
- validators that sign availability without storing/receiving required shares;
- peers that serve corrupted shares;
- peers that serve shares from the wrong manifest or namespace;
- peers that omit chunks during state sync;
- replay of stale DA manifests or certificates;
- a minority of validators going offline during retrieval;
- spam payloads designed to exceed memory, disk, or network budgets;
- aspect-module deployment data disappearing after contract deployment;
- bridge/governance proof inputs being unavailable after finality;
- long-range audit requirements where recent validators no longer retain all
  data.

The DA layer does not need to preserve privacy. DeTTa DeFi payload data is
assumed public unless a future privacy layer explicitly encrypts and commits to
payloads.

## 4. Data Classes Covered By DA

Every byte required for independent replay or audit must be committed into DA.

Mandatory DA payload classes:

- ordered signed transaction envelopes;
- transaction metadata needed for admission and replay;
- deployed restricted MeTTa aspect source;
- aspect IR/artifact records and proof-obligation references;
- governance proposal payloads and upgrade artifacts;
- oracle updates and signer evidence;
- bridge messages, replay IDs, and finality proof inputs;
- lending, staking, AMM, vault, wrapper, and token method inputs;
- receipts and events, unless deterministically regenerated and separately
  committed;
- cross-shard outbox data;
- block execution metadata needed to reproduce resource accounting;
- optional compact state diffs for faster sync;
- checkpoint/snapshot manifests and chunks when checkpoint sync is used.

The canonical block DA payload should be independently decodable from the block
header and DA manifest. A node must not need private local database state to
interpret the payload format.

## 5. High-Level Architecture

```text
transaction/aspect/governance inputs
    -> canonical DA payload
    -> namespace partitioning
    -> erasure-coded shares
    -> share commitment tree
    -> DA manifest
    -> share gossip and custody
    -> validator DA votes
    -> DA certificate
    -> consensus finality
    -> replay, RPC retrieval, audit, and state sync
```

New crate:

```text
crates/detta-da
```

Primary responsibilities:

- canonical DA payload encoding;
- namespace assignment;
- payload chunking;
- erasure coding;
- chunk/share commitment roots;
- DA manifests;
- availability vote structures;
- DA certificates;
- share verification;
- payload reconstruction;
- namespace extraction;
- challenge evidence;
- storage retention metadata;
- test fixtures and golden vectors.

## 6. Core Data Structures

### 6.1 DA Payload

```rust
pub struct DaPayload {
    pub chain_id: ChainId,
    pub height: u64,
    pub previous_block_hash: String,
    pub block_payload_version: u32,
    pub namespaces: Vec<DaNamespaceSection>,
}

pub struct DaNamespaceSection {
    pub namespace: DaNamespace,
    pub records: Vec<DaRecord>,
}

pub enum DaRecord {
    SignedTransaction(SignedTransactionEnvelope),
    AspectSource(AspectSourceRecord),
    AspectArtifact(AspectModuleRecord),
    GovernancePayload(GovernancePayloadRecord),
    BridgeProof(BridgeProofRecord),
    OracleEvidence(OracleEvidenceRecord),
    Receipt(Receipt),
    Event(Event),
    StateDiff(StateDiffRecord),
    SnapshotChunkReference(SnapshotChunkReference),
}
```

The exact record set should reuse existing DeTTa types where possible and wrap
them in versioned DA records. All encoding must be deterministic and covered by
golden fixtures.

### 6.2 DA Manifest

```rust
pub struct DaManifest {
    pub schema: String,
    pub schema_version: u32,
    pub chain_id: ChainId,
    pub height: u64,
    pub block_hash: String,
    pub payload_hash: String,
    pub payload_bytes: u64,
    pub namespace_root: String,
    pub share_root: String,
    pub erasure_scheme: ErasureScheme,
    pub original_share_count: u32,
    pub encoded_share_count: u32,
    pub reconstruction_threshold: u32,
    pub share_size_bytes: u32,
    pub share_hashes: Vec<String>,
    pub namespace_ranges: Vec<DaNamespaceRange>,
}
```

The manifest binds the payload, encoded shares, namespace layout, and erasure
parameters. It must be hashable by canonical JSON or another existing DeTTa
canonical encoding.

### 6.3 DA Certificate

```rust
pub struct DaAvailabilityVote {
    pub chain_id: ChainId,
    pub height: u64,
    pub block_hash: String,
    pub da_manifest_hash: String,
    pub validator_id: String,
    pub sampled_share_indices: Vec<u32>,
    pub custody_share_indices: Vec<u32>,
    pub signature: ValidatorSignature,
}

pub struct DaAvailabilityCertificate {
    pub chain_id: ChainId,
    pub height: u64,
    pub block_hash: String,
    pub da_manifest_hash: String,
    pub signers: Vec<String>,
    pub votes_root: String,
    pub quorum: u32,
}
```

The certificate is valid only if signed by quorum members of the active
validator set for the relevant height and all votes bind the same block hash
and DA manifest hash.

### 6.4 Block Header Additions

Extend `BlockHeader` with DA commitments:

```rust
pub struct BlockHeader {
    // existing fields...
    pub da_payload_root: String,
    pub da_manifest_hash: String,
    pub da_share_root: String,
    pub da_certificate_hash: String,
}
```

The first migration can use optional fields or a versioned block header to
avoid breaking existing fixtures abruptly. The final production mode must
require DA fields for all finalized blocks.

## 7. Namespace Model

Namespaces make retrieval precise and auditable.

Required namespaces:

- `detta.tx`: ordered signed transaction envelopes;
- `detta.receipt`: receipts if retained in DA;
- `detta.event`: events if retained in DA;
- `detta.aspect.source`: submitted restricted MeTTa source;
- `detta.aspect.artifact`: verified aspect artifacts;
- `detta.governance`: proposals, upgrades, timelocks, and policy updates;
- `detta.bridge`: bridge messages and finality proof inputs;
- `detta.oracle`: oracle update evidence;
- `detta.snapshot`: checkpoint manifests and snapshot chunks;
- `detta.state_diff`: optional state diffs;
- `detta.operator`: optional operator/audit metadata.

Initial implementation can use a sorted namespace table committed into the DA
manifest. Later implementation should move to a namespaced Merkle tree or
equivalent structure so clients can verify complete namespace inclusion or
absence without downloading unrelated namespaces.

## 8. Erasure Coding And Commitments

The production path should use systematic Reed-Solomon coding initially:

- split canonical payload into `k` original shares;
- produce `n` encoded shares;
- require any `k` shares to reconstruct;
- commit to all `n` share hashes;
- build a Merkle root over encoded share hashes;
- include erasure parameters in the manifest.

Initial defaults:

- `n = 2k` for 50% recovery overhead;
- maximum share size configurable by network policy;
- maximum payload size enforced during mempool and block proposal;
- deterministic chunk ordering;
- canonical manifest hash over the manifest minus signatures.

Future upgrade path:

- KZG commitments for compact sample proofs;
- two-dimensional Reed-Solomon extension for stronger DAS;
- namespaced share commitments;
- configurable redundancy by block data gas or risk profile.

## 9. Consensus Integration

### 9.1 Proposal Flow

1. Proposer builds normal DeTTa block candidate.
2. Proposer builds canonical DA payload for that block.
3. Proposer erasure-codes payload into shares.
4. Proposer gossips DA manifest and shares before or alongside block proposal.
5. Validators verify:
   - manifest hash matches block header;
   - payload root matches decoded payload where enough data is present;
   - assigned shares match share hashes;
   - sampled shares verify against the share root;
   - payload namespaces are valid;
   - DA size/resource policy is satisfied.
6. Validators store custody shares.
7. Validators sign DA availability votes.
8. Block finality vote is valid only when the DA certificate is available.

### 9.2 Finality Rule

Consensus must reject finalization when:

- the DA manifest is missing;
- the DA certificate is missing;
- the DA certificate does not meet validator quorum;
- DA votes bind a different block hash or manifest hash;
- the block header DA fields do not match the DA manifest;
- DA payload replay does not produce the block transaction root and execution
  roots.

### 9.3 Fork And Equivocation Handling

If a validator observes two DA manifests for the same `(chain_id, height,
block_hash)` with different hashes, it records equivocation evidence. If a
proposer signs or gossips conflicting block headers or DA manifests, that
evidence must be slashable.

## 10. Network Protocol

Add protocol messages:

```rust
pub enum DaProtocolMessage {
    DaManifestAnnounce(DaManifest),
    DaShareOffer(DaShareOffer),
    DaShareRequest(DaShareRequest),
    DaShare(DaShare),
    DaSampleRequest(DaSampleRequest),
    DaSampleResponse(DaSampleResponse),
    DaAvailabilityVote(DaAvailabilityVote),
    DaAvailabilityCertificate(DaAvailabilityCertificate),
    DaChallenge(DaChallenge),
    DaChallengeResponse(DaChallengeResponse),
    DaRepairRequest(DaRepairRequest),
    DaRepairResponse(DaRepairResponse),
}
```

Protocol rules:

- all DA messages use the existing protocol envelope versioning;
- validator DA votes are signed with a dedicated signature domain;
- share serving has bounded request size and rate limits;
- peers cannot force unbounded memory growth with repeated share requests;
- invalid shares lower peer score;
- repeated unavailable-share responses can trigger challenge records.

## 11. DA Storage

Add a local DA store under persistent node storage:

```text
da/
  manifests/
  certificates/
  shares/
  payloads/
  challenges/
  repair/
  indexes/
```

Storage requirements:

- atomic writes for manifests, certificates, and share records;
- content-addressed share storage by `(manifest_hash, share_index, share_hash)`;
- index by height, block hash, namespace, and retention class;
- retention metadata per block and namespace;
- operator metrics for disk use and missing shares;
- repair queue for shares that should be fetched from peers;
- audit root over DA import/repair/challenge records.

Retention classes:

- hot: recent finalized blocks, all validators store custody shares;
- warm: full nodes store reconstructed payloads and all shares they acquired;
- cold: archive nodes or storage providers store full payload history;
- checkpoint: snapshot payloads retained according to state-sync policy.

## 12. RPC And Client APIs

Add public RPC methods:

```text
get_da_manifest(block_hash | height)
get_da_certificate(block_hash | height)
get_da_share(manifest_hash, share_index)
get_da_payload(block_hash | height)
get_da_namespace(block_hash | height, namespace)
get_da_namespace_proof(block_hash | height, namespace)
get_da_status(block_hash | height)
get_da_node_status
get_da_repair_status
get_da_challenge(challenge_id)
```

Operator RPC methods:

```text
submit_da_challenge
submit_da_challenge_response
start_da_repair
set_da_retention_policy
get_da_storage_metrics
get_da_peer_metrics
```

Client expectations:

- wallets can verify that a transaction's block has a DA certificate;
- indexers can fetch namespace-specific data;
- auditors can reconstruct full DA payloads;
- state-sync clients can fetch checkpoint or block data from multiple peers;
- light clients can verify DA headers and sample proofs.

## 13. State Sync Integration

Current snapshot sync should become one DA consumer rather than a separate
trust path.

Required changes:

- checkpoint snapshots are encoded as DA payload records;
- snapshot manifests include DA manifest hashes and DA certificates;
- state-sync clients can reconstruct snapshots from DA shares;
- imported snapshots record the DA certificate and required metadata roots;
- snapshot import rejects payloads without valid DA commitments;
- downstream state sync preserves DA metadata roots and import audit roots.

There should be two sync modes:

- block replay sync: reconstruct block DA payloads and replay from genesis or a
  trusted checkpoint;
- checkpoint sync: reconstruct a DA-certified snapshot and then replay blocks
  after the checkpoint.

## 14. Light Client And Sampling

Light clients should verify:

- block finality certificate;
- DA certificate;
- DA manifest hash in the block header;
- sample proofs against `da_share_root`;
- optional namespace proof for relevant data.

Initial light-client sampling can request random shares from multiple peers and
verify Merkle inclusion against the share root. Later versions should use a
more rigorous DAS scheme with stronger probabilistic guarantees and compact
polynomial commitments.

## 15. Challenges And Slashing

Challenge protocol:

1. Challenger requests a share from a validator that signed DA availability.
2. Validator must serve the share plus proof before timeout.
3. If the validator fails, challenger submits signed request/timeout evidence.
4. If the validator serves an invalid share, challenger submits invalid-share
   evidence.
5. Consensus records challenge outcome and exposes slashing evidence.

Slashable DA faults:

- signed DA vote but unavailable custody share;
- invalid share served for signed custody;
- conflicting DA votes for the same block height/hash;
- proposer DA manifest equivocation;
- malformed erasure metadata in a finalized proposal.

Non-slashable operator issues:

- expired retention after the committed retention window;
- archive node not advertising custody;
- network timeout without signed custody obligation.

## 16. Resource Accounting

DA must be metered separately from execution gas.

Add:

- `data_gas_used`;
- max DA payload bytes per block;
- max namespace bytes per block;
- max aspect module source bytes;
- max share size;
- max manifest size;
- max DA requests per peer/window;
- storage reservation per retention class.

Mempool admission should estimate DA cost before accepting large transactions
or aspect deployments. Block proposal must reject transactions that exceed DA
limits even if execution units fit.

## 17. Formal Verification And Invariants

Add DA-specific theorem obligations:

- finalized blocks must have a valid DA certificate;
- DA certificates bind exactly one block hash and manifest hash;
- reconstructed payload hash equals the committed payload hash;
- any valid reconstruction from threshold shares yields the same payload;
- payload transaction list yields the committed `tx_root`;
- replay from DA payload yields the committed execution roots;
- namespace proofs are complete and deterministic;
- invalid/missing shares cannot be accepted as available;
- snapshot import from DA preserves authenticated roots;
- challenge evidence cannot slash validators that did not sign custody.

Artifacts to add:

- `models/DeTTaDataAvailability.tla`;
- DA model-checking config;
- golden DA payload fixtures;
- DA manifest hash attestations;
- proof artifact manifest entries for DA models and fixtures;
- `detta-verify` checks for DA fixture roots and theorem coverage.

## 18. Implementation Phases

### Phase 0: Specification And Compatibility

- [x] Add this plan to the production acceptance roadmap.
- [x] Define DA glossary and threat model in docs.
- [x] Add versioned DA payload schema.
- [x] Decide whether block header migration uses optional DA fields or a
  versioned header enum.
- [x] Add feature flag `experimental-da` for incremental work.
- [x] Add architecture tests that ensure DA-disabled and DA-enabled blocks are
  explicitly distinguished.

Acceptance:

- DA schemas are documented.
- Existing tests pass without DA required.
- New tests prove DA-enabled headers cannot silently omit DA commitments.

### Phase 1: `detta-da` Crate

- [x] Create `crates/detta-da`.
- [x] Implement canonical payload encoding and decoding.
- [x] Implement DA namespaces and deterministic namespace ordering.
- [x] Implement payload hashing.
- [x] Implement manifest hashing.
- [x] Implement Merkle share commitments.
- [x] Add golden fixtures for payloads, manifests, and share roots.

Acceptance:

- fixture hashes are deterministic;
- malformed payloads are rejected;
- namespace order is stable;
- manifest hash is stable across platforms.

### Phase 2: Erasure Coding And Reconstruction

- [x] Add Reed-Solomon erasure coding dependency or internal adapter.
- [x] Split payloads into original shares.
- [x] Encode redundant shares.
- [x] Reconstruct payload from threshold shares.
- [x] Reject insufficient, duplicate, wrong-index, wrong-hash, and wrong-root
  shares.
- [x] Add property-style tests over payload sizes and missing-share patterns.

Acceptance:

- any threshold-valid share set reconstructs identical payload bytes;
- corrupted shares are rejected;
- reconstruction never accepts data with the wrong payload hash.

### Phase 3: DA Store

- [x] Add durable DA store paths to `detta-storage`.
- [x] Persist manifests, certificates, shares, payloads, challenges, repair
  records, and indexes.
- [x] Persist manifests and deterministic shares.
- [x] Persist reconstructed DA payloads keyed by manifest hash.
- [x] Persist experimental DA certificates.
- [x] Persist DA challenge records and include challenge bytes in DA storage
  stats.
- [x] Persist DA repair records.
- [x] Persist manifest indexes by height and block hash.
- [x] Persist manifest indexes by namespace and derived retention class.
- [x] Persist certificate indexes by manifest hash, height, and block hash.
- [x] Add DA index rebuild and corruption checks.
- [x] Add DA store roots.
- [x] Add DA storage metrics.
- [x] Add retention policy structures.
- [x] Add non-destructive retention audit reporting for stored manifests.
- [x] Add backup/restore coverage for DA data.

Acceptance:

- DA data survives restart;
- backup/restore preserves DA manifests, shares, certificates, and indexes;
- storage metrics expose DA byte use and missing-share counts;
- manifest indexes support height, block hash, namespace, and retention-class
  lookup with corruption checks.

### Phase 4: Protocol Messages

- [x] Add DA protocol message types to `detta-protocol` for manifests, shares,
  availability votes, certificates, and share challenges.
- [x] Add DA signature domains.
- [x] Add encoding/decoding golden fixtures.
- [x] Route DA manifest/share/challenge messages through existing TCP
  envelopes.
- [x] Add peer scoring for invalid DA shares.
- [x] Add bounded request handling.
- [x] Add rate-limit hooks.

Acceptance:

- DA messages round-trip over TCP;
- invalid DA envelopes are rejected;
- peers cannot request unbounded shares in one message.

### Phase 5: Proposal And Availability Voting

- [x] Build experimental DA payload from block candidate header, transactions,
  and receipts.
- [x] Build DA manifest and deterministic chunk shares.
- [x] Gossip manifest and shares with proposal.
- [x] Assign deterministic validator custody/share sampling.
- [x] Verify shares before signing DA vote.
- [x] Aggregate DA votes into DA certificate.
- [x] Persist experimental DA certificate records.

Acceptance:

- validators do not sign DA for missing/corrupt assigned shares;
- quorum DA votes produce a certificate;
- non-quorum DA votes cannot finalize a block.

### Phase 6: Consensus Gating

- [x] Add optional experimental DA fields to block headers.
- [x] Add DA certificate checks to consensus finality verifier.
- [x] Reject blocks whose DA manifest hash does not match the header in the
  DA-certified finality verifier.
- [x] Reject blocks whose payload `tx_root` does not match execution block
  transactions.
- [x] Require DA certificate in production mode.

Acceptance:

- withheld-data proposals fail before finality;
- bad manifest proposals fail;
- valid DA-certified proposals finalize and replay.

### Phase 7: RPC And Retrieval

- [x] Add DA manifest/share/payload RPC types.
- [x] Add namespace retrieval APIs.
- [x] Add DA certificate RPC types.
- [x] Add DA storage metrics APIs.
- [x] Add broader DA node status APIs.
- [x] Add repair status APIs.
- [x] Update OpenAPI and RPC docs.
- [x] Extend `detta-client` with DA inspection commands.

Acceptance:

- external clients can fetch and verify DA manifests;
- indexers can fetch namespace data;
- auditors can reconstruct a block payload through RPC.

### Phase 8: DA-Backed State Sync

- [x] Encode checkpoint snapshots as DA payloads.
- [x] Fetch snapshot shares from multiple peers.
- [x] Reconstruct and verify DA-certified checkpoints from threshold shares.
- [x] Replay DA-certified blocks after checkpoint.
- [x] Persist DA manifest/certificate provenance in snapshot import audit records.

Acceptance:

- a new node can sync from DA shares without trusting one snapshot peer;
- corrupted checkpoint shares are rejected;
- missing shares trigger repair/retry behavior.

### Phase 9: Challenge And Slashing Evidence

- [x] Implement DA share challenge messages.
- [x] Implement challenge-response verification.
- [x] Persist DA challenge records.
- [x] Add slashing records for signed unavailable shares.
- [x] Expose challenge records over RPC.
- [x] Add governance policy for DA slashing parameters.

Acceptance:

- a validator that signed custody but cannot serve a share can be challenged;
- invalid-share responses create evidence;
- non-signers cannot be falsely slashed.

### Phase 10: Light Client Sampling

- [x] Add sampling proof APIs.
- [x] Add light-client DA verification helper.
- [x] Add sample schedule derivation from block hash and client randomness.
- [x] Verify sampled shares against DA share root.
- [x] Add namespace proof verification.

Acceptance:

- light clients can verify finality plus DA certificate plus samples;
- sample proof tampering is rejected;
- namespace absence/inclusion proofs are deterministic.

### Phase 11: Operations And Release Evidence

- [x] Add DA metrics to operator metrics.
- [x] Add DA alerts for missing shares and pending repair records.
- [x] Add DA alerts for custody-share assignment failures, repair lag, and
  challenge failures.
- [x] Add DA retention runbook.
- [x] Add DA retention audit checks to the public testnet stability drill.
- [x] Add DA incident-response drill.
- [x] Add DA stability drill.
- [x] Add DA evidence to audit-readiness and release-candidate bundles.
- [x] Add DA proof artifacts to readiness status v2 or later.

Acceptance:

- release gate exercises DA-certified block production;
- retained evidence bundles include DA manifests, certificates, and challenge
  drill reports;
- readiness verifiers reject stale DA evidence.

### Phase 12: Production Hardening

- [x] Load test DA gossip with large aspect deployments.
- [x] Load test sustained AMM/lending/staking traffic with DA certificates.
- [x] Fuzz DA payload decoding and share reconstruction.
- [x] Fuzz DA RPC request bounds.
- [x] Add long-running multi-validator retention simulation.
- [x] Add chaos tests for offline validators during retrieval.
- [x] Add archive-node reconstruction tests.

Acceptance:

- DA remains bounded under adversarial inputs;
- data can be reconstructed after minority validator failures;
- validator restart does not lose custody metadata;
- archive nodes can reconstruct historical payloads.

## 19. End-To-End Test Matrix

Required tests:

- valid DA-certified token transfer finalizes;
- valid DA-certified aspect-token deployment finalizes;
- valid DA-certified AMM liquidity and swap block finalizes;
- withheld transaction payload fails finality;
- bad DA manifest hash fails finality;
- bad share hash fails availability vote;
- insufficient shares fail reconstruction;
- threshold shares reconstruct identical payload;
- validator restart preserves custody shares;
- backup/restore preserves DA data;
- RPC payload retrieval reconstructs block transactions;
- namespace retrieval returns only requested namespace data with proof;
- DA-backed checkpoint sync imports a state snapshot;
- corrupted checkpoint share is rejected;
- DA challenge succeeds against unavailable signer;
- DA challenge fails against validator that did not sign custody;
- light client verifies finality plus DA certificate plus samples;
- release evidence bundle binds DA manifests and certificates.

## 20. Production Acceptance Criteria

DeTTa has a production-grade DA layer when all of these are true:

- every production block header contains DA payload, manifest, share, and
  certificate commitments;
- block finality is impossible without a valid DA certificate;
- DA certificates require quorum signatures from active validators;
- validators sign DA only after validating required shares/samples;
- any threshold-valid share set reconstructs the canonical payload;
- reconstructed payloads replay to the committed transaction and execution
  roots;
- aspect module source/artifacts required for replay are DA-covered;
- governance, bridge, oracle, and upgrade evidence is DA-covered;
- DA shares are durably stored according to retention policy;
- persistent nodes commit the production retention policy by default without
  overwriting operator-provided policies;
- DA storage stats expose the active retention policy and root;
- DA retention audit reports active/expired manifests and unsatisfied local
  retention obligations;
- DA manifest indexes are queryable by namespace and retention class over
  persistent-node RPC;
- RPC can retrieve manifests, certificates, shares, payloads, namespaces, and
  proofs;
- light-client DA verification is implemented and tested;
- state sync can reconstruct checkpoints or blocks from DA shares;
- unavailable signed custody is challengeable and produces slashing evidence;
- operator health, metrics, and alerts expose DA health and the active
  production DA profile;
- release gates include DA-certified block production and withheld-data tests;
- audit/readiness bundles include DA evidence and verifier checks;
- formal/proof artifacts cover DA certificate and reconstruction invariants.

## 21. Resolved Production v1 Policy Decisions

- DeTTa production DA v1 uses Merkle SHA-256 share commitments over encoded
  share hashes. KZG commitments are deferred to a future manifest/payload
  version.
- Validators sign deterministic custody assignments, and votes may additionally
  carry light-client sample indices. The v1 profile requires at least two
  custody shares and three light-client samples where those checks are used.
- Full nodes must reconstruct or locally verify full DA payloads before serving
  production payload RPC responses.
- Validator hot retention is at least 65,536 blocks. Archive and checkpoint
  retention are at least 1,048,576 blocks.
- Mandatory DA record namespaces for production coverage are `detta.aspect`,
  `detta.block`, `detta.bridge`, `detta.governance`, `detta.oracle`,
  `detta.receipt`, and `detta.tx`.
- Production block DA payloads carry transactions and receipts as payload
  records. Raw events are optional records; event integrity is otherwise
  replay-deterministic through the committed block `event_root` and receipt
  `event_root_after` values.
- DA data gas is priced in 1024-byte units by `DaProductionProfile::v1()`.
- DA slashing is governed by the timelocked validator-set governance path
  already used by `DaSlashingPolicy`.
- Archive/storage providers are treated as governance-registered storage
  providers for public-mainnet readiness; a separate open storage market can be
  added after v1.

## 22. Recommended First Implementation Slice

Start with a correctness-first local DA prototype:

1. Add `crates/detta-da`.
2. Implement canonical payload, manifest, Merkle share root, and deterministic
   chunking without erasure coding.
3. Add block-header optional DA fields behind `experimental-da`.
4. Persist manifests and chunks in the local node.
5. Add RPC methods for manifest and chunk retrieval.
6. Add tests proving withheld chunks prevent DA verification.

Then add Reed-Solomon erasure coding and consensus gating once the canonical
payload and manifest path is stable.

This keeps the first slice small enough to verify while preserving the path to
full production DA.
