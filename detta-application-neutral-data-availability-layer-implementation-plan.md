# DeTTa Application-Neutral Data Availability Layer Implementation Plan

**Project:** DeTTa
**Branch:** `experimental/application-neutral-data-availability-layer`
**Created:** 2026-09-14
**Goal:** Generalize DeTTa's DA model from a DeFi-block-specific subsystem into
an application-neutral DA layer that can serve any deterministic or
append-only application, while preserving the existing DeTTa DeFi production DA
profile as a first-class compatibility profile.

## 1. Motivation

The current DA implementation is strong for DeTTa DeFi blocks:

- canonical payloads and manifests;
- Reed-Solomon v1 share production and reconstruction;
- namespace roots and share roots;
- DA votes and quorum DA certificates;
- deterministic custody assignments;
- light-client sampling proofs;
- durable manifests, shares, payloads, certificates, indexes, repairs, and
  challenge evidence;
- finality-time checks binding DeTTa block transactions, receipts, and
  specialized DeFi evidence.

The limitation is that the production validator currently understands one
application shape: DeTTa blocks with DeFi-specific namespaces such as
`detta.tx`, `detta.receipt`, `detta.aspect`, `detta.governance`,
`detta.bridge`, and `detta.oracle`.

To support a social network, marketplace, game, rollup, supply-chain log,
scientific data registry, governance forum, or any other application, the DA
layer must separate:

- generic DA commitments, erasure coding, custody, sampling, retrieval,
  retention, and repair;
- application-specific payload structure, namespace policy, record schemas,
  replay roots, and finality rules.

The target outcome is an application-neutral DA service where each application
registers a profile, publishes canonical namespaced payloads under that
profile, receives DA manifests and certificates, and exposes enough evidence
for its own clients and auditors to reconstruct and verify the data.

## 2. Core Design Principle

DA should not know the business logic of every application.

The generic DA layer should enforce:

- canonical bytes;
- profile identity;
- namespace and record schema policy;
- payload size and resource bounds;
- erasure coding correctness;
- share and namespace commitments;
- validator custody and sampling checks;
- quorum DA certificates;
- retrieval and repair behavior;
- durable indexes and retention policy.

Application adapters should enforce:

- whether the payload is a block, batch, checkpoint, moderation log, media
  manifest, or another application object;
- which namespaces are mandatory, optional, or forbidden;
- how record bytes decode;
- which application roots or event hashes must match;
- whether records must be publicly readable or encrypted;
- which replay/audit rules apply.

DeTTa DeFi becomes one adapter/profile, not the hard-coded DA universe.

## 3. Target Architecture

```text
application clients
        |
        v
application sequencer / validator / producer
        |
        v
application DA adapter
        |
        +-- profile lookup
        +-- record canonicalization
        +-- namespace policy validation
        +-- optional app-root validation
        |
        v
generic DeTTa DA service
        |
        +-- canonical payload envelope
        +-- manifest and namespace roots
        +-- Reed-Solomon shares
        +-- custody assignment
        +-- DA votes and certificates
        +-- sampling proofs
        +-- storage, indexes, retention, repair
        |
        v
light clients / full nodes / archive nodes / auditors
```

The generic DA service should be usable in three modes:

1. **Embedded DeTTa mode:** DA is used by DeTTa consensus and DeTTa block
   finality, preserving current behavior.
2. **Application batch mode:** an external application submits signed,
   canonical payload batches and receives DA manifests, shares, certificates,
   and retrieval proofs.
3. **Checkpoint/state-sync mode:** an application publishes checkpoints or
   snapshots that can be reconstructed from DA shares.

## 4. Non-Goals

The first generalized implementation should not try to solve:

- private data semantics beyond hash commitments and optional encrypted record
  bytes;
- application execution or consensus for every app;
- arbitrary code execution inside DA validators;
- global data markets, pricing auctions, or storage-provider payments;
- KZG or polynomial-commitment DA sampling;
- automatic content moderation, feed ranking, or app-specific policy disputes.

Those can be layered later once the generic profile, payload, certificate, and
retrieval model is stable.

## 5. Application-Neutral DA Vocabulary

Add these concepts to `detta-da`:

- **Application id:** stable lowercase dot-separated identifier, for example
  `detta.defi`, `social.demo`, `marketplace.orders`, or `game.moves`.
- **Application profile:** hashable policy object describing schemas,
  namespaces, validation mode, DA production profile, retention class mapping,
  and application anchor rules.
- **Profile id:** canonical hash or explicit versioned id for an application
  profile.
- **Application coordinate:** generic location for payload ordering and lookup:
  application id, stream id, height/sequence, optional epoch, optional parent
  hash, and optional subject hash.
- **Payload kind:** block, batch, checkpoint, snapshot, media manifest,
  moderation log, index delta, or application-defined kind.
- **Record schema id:** versioned schema name for records inside a namespace.
- **Record envelope:** record metadata plus canonical bytes. DA validates the
  envelope and delegates schema-specific decoding to profile validators.
- **Application root binding:** optional root or hash that the application
  wants DA to commit, such as a social event-log root, rollup state root,
  marketplace order-book root, or DeTTa block root.
- **Validation mode:** opaque bytes only, schema-decodable records, or
  application-adapter-verified records.

## 6. Proposed Data Model

### 6.1 Application Id

Add a new validated type:

```rust
pub struct DaApplicationId(pub String);
```

Rules:

- lowercase ASCII letters, digits, `-`, and `.`;
- cannot be empty;
- cannot start or end with `.` or `-`;
- cannot contain `..`;
- maximum length, for example 128 bytes;
- reserved prefixes:
  - `detta.*` for built-in DeTTa profiles;
  - `sys.*` for future DA system namespaces;
  - application operators use non-reserved prefixes.

### 6.2 Application Coordinate

Add:

```rust
pub struct DaApplicationCoordinate {
    pub application_id: DaApplicationId,
    pub stream_id: String,
    pub sequence: u64,
    pub epoch: Option<u64>,
    pub parent_hash: Option<String>,
    pub subject_hash: Option<String>,
}
```

Use cases:

- social network global feed: `stream_id = "global"`, `sequence = batch_number`;
- user feed: `stream_id = "user:<user_id_hash>"`;
- rollup block: `stream_id = "rollup-main"`, `sequence = block_height`;
- DeTTa block: `stream_id = chain_id`, `sequence = block_height`;
- checkpoint: `stream_id = "checkpoint:<chain_id>"`.

Coordinates are not consensus by themselves. They are addressable DA metadata
that applications can bind into their own finality or audit process.

### 6.3 Application DA Profile

Add:

```rust
pub struct DaApplicationProfile {
    pub schema: String,
    pub schema_version: u32,
    pub application_id: DaApplicationId,
    pub profile_version: u32,
    pub profile_name: String,
    pub da_profile: DaProductionProfile,
    pub namespace_policies: Vec<DaNamespacePolicy>,
    pub record_policies: Vec<DaRecordPolicy>,
    pub coordinate_policy: DaCoordinatePolicy,
    pub root_bindings: Vec<DaApplicationRootPolicy>,
    pub retention_policy: DaApplicationRetentionPolicy,
    pub validation_mode: DaApplicationValidationMode,
    pub privacy_mode: DaApplicationPrivacyMode,
    pub max_payload_bytes: u64,
    pub max_records_per_payload: u32,
}
```

Profile invariants:

- canonical and hashable;
- application id must be valid;
- namespace policies sorted by namespace;
- namespace policies unique;
- required namespace policies cannot be empty;
- record policies sorted by schema id;
- all mandatory schemas must map to an allowed namespace;
- `da_profile.validate()` must pass;
- payload bounds must be nonzero;
- retention policy must cover every required namespace and payload kind;
- privacy mode must be explicit.

### 6.4 Namespace Policy

Add:

```rust
pub enum DaNamespaceRequirement {
    Required,
    Optional,
    Forbidden,
}

pub struct DaNamespacePolicy {
    pub namespace: DaNamespace,
    pub requirement: DaNamespaceRequirement,
    pub allowed_record_schemas: Vec<String>,
    pub min_records: u32,
    pub max_records: u32,
    pub retention_class: DaRetentionClass,
}
```

Validation:

- required namespaces must be present in payloads;
- forbidden namespaces reject payloads;
- optional namespaces may be absent;
- record counts must satisfy min/max;
- every record schema in a namespace must be allowed;
- namespace order remains canonical.

### 6.5 Record Envelope

Add a new generic record variant:

```rust
pub struct DaRecordEnvelope {
    pub schema: String,
    pub schema_version: u32,
    pub content_type: String,
    pub encoding: DaRecordEncoding,
    pub bytes: Vec<u8>,
    pub content_hash: String,
    pub signer: Option<String>,
    pub signature: Option<String>,
}
```

Supported encodings:

- `CanonicalJson`;
- `CanonicalCbor` later;
- `OpaqueBytes`;
- `EncryptedBytes`;
- `ExternalContentAddress`.

Rules:

- record envelope itself is canonical JSON for v1 generalized DA;
- `content_hash` must equal the hash of `bytes`;
- encrypted records must include encryption metadata in their bytes or in an
  application-defined schema;
- external content records commit only content addresses and metadata, not the
  full external media bytes.

Existing typed `DaRecord` variants can remain for compatibility, but generic
applications should use `DaRecord::Application(DaRecordEnvelope)` or a new
`ApplicationDaRecord` type.

### 6.6 Application Payload Envelope

Introduce a v2 payload envelope:

```rust
pub struct ApplicationDaPayload {
    pub schema: String,
    pub schema_version: u32,
    pub application_id: DaApplicationId,
    pub profile_id: String,
    pub coordinate: DaApplicationCoordinate,
    pub payload_kind: DaPayloadKind,
    pub previous_payload_hash: Option<String>,
    pub application_roots: Vec<DaApplicationRoot>,
    pub namespaces: Vec<ApplicationDaNamespaceSection>,
}
```

The existing `DaPayload` remains supported as the DeTTa block/checkpoint v1
format. The generalized implementation can either:

1. add `ApplicationDaPayload` beside `DaPayload`; or
2. add a versioned enum:

```rust
pub enum DaPayloadEnvelope {
    DeTTaV1(DaPayload),
    ApplicationV1(ApplicationDaPayload),
}
```

Recommended first step: add `ApplicationDaPayload` beside `DaPayload` to reduce
risk and avoid breaking stable DeTTa DA fixtures.

### 6.7 Application Manifest

Generalize `DaManifest` without breaking existing v1 fields by introducing:

```rust
pub struct ApplicationDaManifest {
    pub schema: String,
    pub schema_version: u32,
    pub application_id: DaApplicationId,
    pub profile_id: String,
    pub coordinate: DaApplicationCoordinate,
    pub payload_kind: DaPayloadKind,
    pub payload_hash: String,
    pub payload_bytes: u64,
    pub namespace_root: String,
    pub application_root: Option<String>,
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

Do not overload `block_hash` for every application. Replace it with the
coordinate and optional `application_root`.

Compatibility:

- `DaManifest` remains `detta.da-manifest.v1`.
- `ApplicationDaManifest` starts as `detta.application-da-manifest.v1`.
- storage can support both in parallel.
- DeTTa block DA can expose an adapter that converts DeTTa block metadata into
  an application coordinate.

## 7. Application Profile Registry

Add a registry layer so validators and RPC nodes know which profile rules to
apply.

### 7.1 Local Registry

Initial implementation:

- in-memory `DaApplicationProfileRegistry`;
- explicit profile registration at node bootstrap or test setup;
- default built-in DeTTa DeFi profile registered automatically;
- social-demo profile used in tests.

### 7.2 Durable Registry

Next step:

- persist profiles in `detta-storage`;
- index by application id, profile id, profile version, and status;
- reject payloads for unknown profiles;
- reject payloads whose `profile_id` does not hash to the registered profile.

### 7.3 Governed Registry

Production step:

- DeTTa governance can schedule and activate profile registrations;
- profile changes are timelocked;
- old profile versions remain available for historical payload validation;
- profile activation height or sequence is recorded;
- profile deprecation cannot invalidate already-finalized DA evidence.

## 8. Validation Modes

Support three validation modes:

### 8.1 Opaque Bytes

The DA layer validates only:

- application id;
- profile id;
- payload canonicality;
- namespace policy;
- record count bounds;
- content hashes;
- total byte limits;
- share commitments and erasure coding.

Use for simple append-only logs, encrypted records, or external applications
that perform their own execution elsewhere.

### 8.2 Schema-Decodable Records

The DA layer additionally validates:

- each record decodes under the declared schema;
- required fields are present;
- records are canonical;
- optional signatures verify if profile requires signatures;
- schema-specific size limits.

Use for social network events, governance comments, marketplace orders, and
other record-oriented apps.

### 8.3 Application Adapter Verified

The DA layer calls a deterministic adapter:

```rust
pub trait DaApplicationValidator {
    fn profile_id(&self) -> &str;
    fn validate_payload(
        &self,
        profile: &DaApplicationProfile,
        payload: &ApplicationDaPayload,
        manifest: Option<&ApplicationDaManifest>,
    ) -> Result<DaApplicationValidationReport, DaError>;
}
```

Adapters must be:

- deterministic;
- bounded by explicit resource limits;
- side-effect-free;
- registered by profile id;
- covered by tests and fixtures.

Use for DeTTa block DA, rollup state-root checks, or application-specific
append-only sequence validation.

## 9. Social Network Example Profile

An application-neutral DA layer should support a profile like:

```text
application_id: social.demo
profile_version: 1
payload_kind: batch
namespaces:
  social.profile      optional, schema social.profile-update.v1
  social.post         optional, schema social.post.v1
  social.reaction     optional, schema social.reaction.v1
  social.graph        optional, schema social.follow.v1
  social.moderation   optional, schema social.moderation-action.v1
  social.media        optional, schema social.media-reference.v1
  social.tombstone    optional, schema social.tombstone.v1
required:
  at least one allowed namespace per batch
privacy:
  public records allowed
  private-message records must be encrypted or external-content references
roots:
  event_log_root required
retention:
  moderation and tombstone records retained longer than reactions
```

DA payloads would contain signed social actions:

- create post;
- edit post;
- delete post tombstone;
- follow or unfollow;
- like or remove like;
- profile update;
- moderation action;
- media hash/reference.

The DA layer would not rank feeds or decide moderation correctness. It would
make sure the social network's event data was published, namespaced, committed,
reconstructable, and auditable.

## 10. API And RPC Model

Add generic RPC methods beside existing DeTTa DA methods:

```text
register_da_application_profile
get_da_application_profile
get_da_application_profiles
submit_application_da_payload
produce_application_da_batch
get_application_da_manifest
get_application_da_payload
get_application_da_namespace
get_application_da_status
get_application_da_sample_proofs
get_application_da_manifest_index_by_application
get_application_da_manifest_index_by_coordinate
get_application_da_manifest_index_by_namespace
get_application_da_manifest_index_by_retention
```

Initial RPC can be operator-only and local-test oriented. Public production RPC
should require:

- bounded payload bytes;
- bounded record count;
- bounded namespace count;
- profile allowlist;
- rate limits;
- authentication for profile registration;
- clear error codes for unknown profile, invalid namespace, invalid schema,
  unsupported encoding, oversized payload, and failed adapter verification.

CLI additions:

```text
detta-client da-profile-register --profile PATH
detta-client da-profile --application <id> [--profile-id <hash>]
detta-client produce-application-da-batch --application <id> --profile-id <hash> --payload PATH
detta-client application-da-manifest --manifest-hash <hash>
detta-client application-da-payload --manifest-hash <hash>
detta-client application-da-namespace --manifest-hash <hash> --namespace <name>
detta-client application-da-status --manifest-hash <hash>
```

## 11. Storage And Indexes

Extend `detta-storage` DA storage without breaking v1 paths.

New durable objects:

- application profiles;
- application payloads;
- application manifests;
- application DA certificates;
- application repair records;
- application challenge records;
- application validation reports.

New indexes:

- manifests by application id;
- manifests by profile id;
- manifests by payload kind;
- manifests by coordinate sequence;
- manifests by stream id;
- manifests by namespace;
- manifests by retention class;
- manifests by application root;
- certificates by application manifest hash;
- profiles by application id and version.

Index requirements:

- index entries must reload and validate referenced objects;
- sorted order must be deterministic;
- duplicate entries rejected;
- stale or mismatched indexes rejected;
- index rebuild must support both DeTTa DA v1 and application DA v1;
- DA store root must include generic application DA indexes.

## 12. Consensus And Certificate Model

The generic certificate should bind:

- application id;
- profile id;
- coordinate;
- payload hash;
- namespace root;
- share root;
- erasure scheme and share counts;
- signer set;
- custody declaration policy.

Options:

1. Reuse `DaAvailabilityCertificate` with manifest hash only. This is simplest
   if the manifest is already application-aware.
2. Add `ApplicationDaAvailabilityCertificate` with explicit application fields.
   This is clearer for external DA consumers and light clients.

Recommended: add an application-specific certificate type for RPC clarity, but
share the internal validation logic with existing DA certificates.

Aggregation requirements:

- only active validators count;
- quorum is profile/network defined;
- all votes must bind the same manifest hash and share root;
- all votes must declare deterministic custody shares for that validator;
- optional sample indices must be sorted, unique, and in range;
- certificates are canonical and hashable.

## 13. DeTTa DeFi Compatibility Adapter

Preserve current behavior by implementing `detta.defi` as a built-in
application profile:

- application id: `detta.defi`;
- stream id: DeTTa chain id;
- sequence: block height;
- payload kind: `block`;
- mandatory namespaces:
  - `detta.block`;
  - `detta.tx`;
  - `detta.receipt`;
  - `detta.aspect`;
  - `detta.governance`;
  - `detta.bridge`;
  - `detta.oracle`;
- optional namespace:
  - `detta.event`;
- adapter validation:
  - exactly one block header;
  - transaction root matches block;
  - receipt root matches block;
  - typed aspect/governance/bridge/oracle records match committed
    transactions;
  - manifest commits the payload and Reed-Solomon share set.

Migration rule:

- do not change existing `detta.da-payload.v1` fixture roots;
- add generalized profile coverage beside it;
- later, expose DeTTa block DA through the generic profile API without removing
  existing DeTTa-specific RPC until clients migrate.

## 14. Application Privacy Model

Application-neutral DA is public by default.

For social or other user-facing apps, the profile must explicitly classify
record privacy:

- `PublicRecord`: raw bytes are DA-published;
- `EncryptedRecord`: encrypted bytes are DA-published;
- `CommitmentOnly`: DA publishes hash/metadata only;
- `ExternalContentAddress`: DA publishes a content address, media hash, and
  metadata, not the media bytes.

Validation:

- private-message schemas must reject `PublicRecord`;
- external media records must include content hash, byte length, media type,
  and storage URI/content id;
- encrypted records must include encryption scheme id and key-discovery
  metadata, but DA does not manage keys;
- deletion requests become tombstones, not physical removal from DA.

## 15. Resource Accounting

Generic application DA must meter:

- payload bytes;
- canonical record bytes;
- namespace count;
- record count;
- share count;
- erasure coding CPU budget;
- storage retention class;
- RPC request size;
- reconstruction request size;
- sample proof count.

Add `ApplicationDaCostReport`:

```rust
pub struct ApplicationDaCostReport {
    pub application_id: DaApplicationId,
    pub profile_id: String,
    pub payload_bytes: u64,
    pub record_count: u32,
    pub namespace_count: u32,
    pub data_gas: u64,
    pub original_share_count: u32,
    pub encoded_share_count: u32,
    pub retention_class_counts: Vec<(DaRetentionClass, u32)>,
}
```

Mempool or batch submission should reject application DA payloads that exceed
profile or node limits before erasure coding.

## 16. Implementation Phases

### Phase 0: Branch And Baseline

- [x] Create `experimental/application-neutral-data-availability-layer`.
- [x] Add this implementation plan.
- [x] Record current DeTTa DA v1 invariants that must remain stable.
- [x] Add a compatibility test inventory for existing DA fixture roots.

Current DeTTa DA v1 invariants now protected during the generalized DA work:

- `DaPayload` remains `detta.da-payload.v1` and is not widened for generic
  applications.
- `DaProductionProfile::v1()` remains the DeTTa DeFi compatibility profile
  used by consensus, node, RPC, storage, and E2E code.
- existing golden payload, manifest, share, namespace-root, and certificate
  hash tests remain unchanged.
- the generalized `DaApplicationProfile::detta_defi_v1()` wraps the existing
  production profile without mutating existing DA v1 payload serialization or
  payload hashes.

Acceptance:

- branch exists;
- plan exists;
- current DA tests still pass before implementation work begins.

### Phase 1: Generic Identifiers And Policies

- [x] Add `DaApplicationId`.
- [x] Add `DaApplicationCoordinate`.
- [x] Add `DaPayloadKind`.
- [x] Add `DaRecordEncoding`.
- [x] Add `DaNamespacePolicy`.
- [x] Add `DaRecordPolicy`.
- [x] Add `DaApplicationProfile`.
- [x] Add canonical hash and validation helpers.
- [x] Add unit tests for invalid ids, duplicate policies, unknown schemas,
  forbidden namespaces, and profile hash stability.

Acceptance:

- [x] application profiles are canonical and hash-stable;
- [x] invalid profile policies are rejected;
- [x] DeTTa built-in profile can be represented without changing current DA v1
  payload bytes.

### Phase 2: Application Payload Envelope

- [x] Add `ApplicationDaPayload`.
- [x] Add `ApplicationDaNamespaceSection`.
- [x] Add `DaRecordEnvelope`.
- [x] Add canonicalization for generic application payloads.
- [x] Add payload hash and namespace root helpers.
- [x] Add profile-based payload validation.
- [x] Add a social-demo payload fixture.

Acceptance:

- [x] payloads with reordered namespaces canonicalize deterministically;
- [x] content hashes bind record bytes;
- [x] required namespaces are enforced;
- [x] forbidden namespaces are rejected;
- [x] social-demo fixture hash is stable.

### Phase 3: Application Manifest And Share Set

- [x] Add `ApplicationDaManifest`.
- [x] Add `ApplicationDaShareSet`.
- [x] Reuse Reed-Solomon v1 encoding and reconstruction.
- [x] Bind application coordinate and profile id into manifests.
- [x] Add `verify_application_manifest_commits_payload`.
- [x] Add application namespace proofs.
- [x] Add coding-fraud proof compatibility for application manifests.

Acceptance:

- [x] any threshold-valid application share set reconstructs the canonical
  application payload;
- [x] manifest hash changes when application id, profile id, coordinate, payload,
  namespace root, or share root changes;
- [x] malformed share roots and wrong profile ids are rejected;
- [x] coding-fraud proofs work for application manifests.

### Phase 4: Profile Registry

- [x] Add in-memory `DaApplicationProfileRegistry`.
- [x] Register built-in `detta.defi` profile.
- [x] Register test `social.demo` profile.
- [ ] Add durable profile storage.
- [x] Add profile indexes by application id, profile id, and profile version.
- [x] Add profile activation/deprecation status.

Acceptance:

- [x] unknown profile payloads are rejected;
- [x] registered profile hashes are stable;
- [x] old profile versions remain loadable for historical validation;
- registry survives restart.

### Phase 5: Application Validators

- [x] Add `DaApplicationValidator` trait.
- [x] Add `OpaqueApplicationValidator`.
- [x] Add `SchemaApplicationValidator`.
- [x] Add `DettaDefiDaValidator` adapter over current DeTTa DA validation.
- [x] Add `SocialDemoDaValidator` for signed social events and event-log root
  checks.
- [x] Add resource budget enforcement for validators.

Acceptance:

- [x] DA validators are deterministic and side-effect-free;
- [x] DeTTa DeFi adapter accepts current valid block DA and rejects tampered
  DeFi evidence;
- [x] social-demo adapter rejects bad signatures, malformed records, sequence
  gaps, and wrong event-log roots.

### Phase 6: Generic DA Certificates

- [x] Add application availability vote type or extend existing vote with
  application manifest support.
- [x] Add application DA certificate type or generic certificate envelope.
- [x] Enforce deterministic custody declarations for application manifests.
- [x] Add certificate hash and validation helpers.
- [x] Add aggregation tests for matching and mismatched application manifests.

Acceptance:

- [x] quorum certificates bind one application manifest;
- [x] votes with empty or wrong custody assignments do not count;
- [x] votes for different profile ids, coordinates, or share roots do not count;
- [x] non-quorum votes cannot produce certificates.

### Phase 7: Storage And Indexes

- [ ] Persist application profiles.
- [ ] Persist application payloads.
- [ ] Persist application manifests.
- [ ] Persist application shares.
- [ ] Persist application certificates.
- [ ] Add application DA indexes.
- [ ] Include application DA indexes in DA store root.
- [ ] Add rebuild and corruption checks.
- [ ] Add backup/restore coverage.

Acceptance:

- application DA data survives restart;
- backup/restore preserves generic DA data and indexes;
- corrupt index entries are rejected;
- indexes can query by application id, profile id, coordinate, namespace,
  retention class, and application root.

### Phase 8: RPC And CLI

- [ ] Add RPC types for profile registration and lookup.
- [ ] Add RPC types for application payload submission and DA batch production.
- [ ] Add RPC retrieval for application manifests, payloads, namespaces,
  shares, certificates, status, samples, and indexes.
- [ ] Add request bounds and rate-limit hooks.
- [ ] Extend OpenAPI schema.
- [ ] Extend `detta-client`.
- [ ] Add social-demo client flow.

Acceptance:

- external apps can register or reference profiles;
- clients can submit application payloads and retrieve DA evidence;
- indexers can query application namespaces and coordinates;
- oversized or unknown-profile requests are rejected with stable error codes.

### Phase 9: Application-Neutral State Sync And Retrieval

- [ ] Generalize DA share retrieval to application manifests.
- [ ] Generalize reconstruction metrics to application payloads.
- [ ] Add application checkpoint payload support.
- [ ] Add application archive reconstruction tests.
- [ ] Add mixed DeTTa/social DA retrieval tests.

Acceptance:

- new nodes can reconstruct application payloads from threshold shares;
- corrupted application shares are rejected;
- minority peer failure does not prevent reconstruction when enough honest
  shares exist;
- DeTTa checkpoint sync remains unchanged.

### Phase 10: Retention, Repair, And Operations

- [ ] Add retention classification from application profiles.
- [ ] Add application DA status reports.
- [ ] Add application repair records.
- [ ] Add application DA alerts.
- [ ] Update operator manual and DA runbook.
- [ ] Add retention prune-plan support for application payload/share files.

Acceptance:

- operators can see per-application DA health;
- application retention obligations are auditable;
- pruning never removes manifests, certificates, indexes, challenge records,
  or audit evidence;
- social-demo retention policies can retain moderation/tombstone data longer
  than reactions.

### Phase 11: Governance And Profile Lifecycle

- [ ] Add governed profile registration plan.
- [ ] Add timelocked profile activation.
- [ ] Add profile deprecation rules.
- [ ] Add profile migration evidence records.
- [ ] Add policy for reserved application ids.

Acceptance:

- profile updates cannot silently change validation rules for historical data;
- deprecated profiles remain verifiable;
- application id ownership and reserved prefixes are enforced;
- governance events are DA-covered.

### Phase 12: Formal And Verification Artifacts

- [ ] Extend TLA+ DA model with application id/profile id/coordinate.
- [ ] Add invariants:
  - manifest binds application id and profile id;
  - certificates bind exactly one application manifest;
  - reconstruction returns canonical payload for any threshold-valid shares;
  - profile validation is deterministic;
  - historical profile verification is stable after profile upgrades.
- [ ] Add proof artifact manifest entries.
- [ ] Add runtime theorem coverage for application-neutral DA.
- [ ] Add fixture roots for social-demo payloads and profiles.

Acceptance:

- proof artifacts cover generic DA certificate and reconstruction invariants;
- verifier rejects missing application-neutral DA evidence;
- DeTTa DeFi proof artifacts remain stable.

### Phase 13: End-To-End Application Tests

Required E2E tests:

- DeTTa DeFi block still finalizes with existing DA path;
- DeTTa DeFi block can be exposed through the generic profile adapter;
- social-demo app registers profile, submits post/follow/moderation payloads,
  produces DA manifests and shares, collects DA certificate, retrieves payload,
  verifies namespace proofs, and reconstructs from threshold shares;
- social-demo encrypted/private record is accepted only in encrypted or
  commitment-only mode;
- social-demo public private-message record is rejected;
- wrong profile id is rejected;
- forbidden namespace is rejected;
- malformed schema bytes are rejected in schema validation mode;
- opaque payload mode accepts bytes but still enforces hash, size, namespace,
  and share commitments;
- application indexes survive restart and backup/restore.

Acceptance:

- generic DA can serve at least two applications in one node: `detta.defi` and
  `social.demo`;
- DeTTa-specific DA tests remain green;
- release gate includes at least one application-neutral DA flow.

## 17. Migration Strategy

1. Add generic types without changing current DA v1 serialization.
2. Add application profile registry with DeTTa profile represented in tests.
3. Add generic payload and manifest beside existing `DaPayload` and
   `DaManifest`.
4. Add generic storage paths beside existing DA paths.
5. Add generic RPC beside existing DeTTa DA RPC.
6. Implement `detta.defi` adapter and prove it accepts current DeTTa DA data.
7. Add social-demo profile and E2E tests.
8. Update release gates.
9. Only after compatibility is proven, consider unifying internal code paths.

Compatibility rules:

- do not rewrite existing fixture roots;
- do not change existing DA manifest hashes;
- do not remove current RPC until a deprecation window is documented;
- old manifests must remain verifiable indefinitely;
- profile upgrades must be append-only.

## 18. Security Requirements

Application-neutral DA must preserve these properties:

- no payload is accepted without a known profile;
- profile hash is bound into payloads and manifests;
- manifests bind application id, coordinate, payload hash, namespace root, and
  share root;
- DA certificates bind exactly one manifest;
- validators sign only after custody shares verify;
- malformed shares cannot reconstruct to an accepted payload;
- application validators are deterministic and bounded;
- unknown application schemas are rejected unless the profile explicitly allows
  opaque bytes;
- retention audit covers application DA obligations;
- private application records are not accidentally published as public bytes;
- profile changes cannot alter validation semantics for old payloads.

## 19. Operational Requirements

Operators need:

- per-application DA status;
- profile inventory;
- profile activation/deprecation status;
- per-application storage bytes;
- per-application missing share counts;
- per-application repair queues;
- per-application retention audit;
- application-index rebuild tools;
- profile export/import for archive nodes;
- evidence bundles containing generic DA profiles, manifests, certificates,
  sample proofs, and reconstruction reports.

## 20. Documentation Updates

Update:

- `detta-data-availability-layer.md` with application-neutral glossary and
  schemas;
- `detta-da-operator-runbook.md` with per-application retention and repair;
- `detta-operator-manual.md` with application profile launch procedures;
- `detta-rpc-api.md` and `detta-rpc-openapi.json` with generic DA RPC;
- `README.md` with application-neutral DA status and examples;
- client guides with a social-demo flow.

## 21. Suggested File-Level Work

Likely edit targets:

- `crates/detta-da/src/lib.rs`
  - identifiers, profiles, generic payloads, manifests, validation, shares,
    certificates, sampling, fixtures;
- `crates/detta-consensus/src/lib.rs`
  - generic DA certificate aggregation and validation;
- `crates/detta-storage/src/lib.rs`
  - durable generic DA objects and indexes;
- `crates/detta-protocol/src/lib.rs`
  - generic DA protocol envelopes;
- `crates/detta-rpc/src/lib.rs`
  - RPC request/response types and OpenAPI fixtures;
- `crates/detta-node/src/lib.rs`
  - profile registry, payload production, retrieval, status, repair,
    persistence;
- `crates/detta-node/src/bin/detta-client.rs`
  - profile and application DA CLI commands;
- `crates/detta-e2e/tests/*`
  - generic app DA client/integration flows;
- `models/DeTTaDataAvailability.tla`
  - application id/profile id/coordinate model;
- `models/detta-proof-artifact-manifest.json`
  - new proof artifacts and roots;
- `scripts/detta-release-gate.sh`
  - generic DA E2E flow.

## 22. Production Acceptance Criteria

The application-neutral DA layer is production-grade when:

- DeTTa DeFi DA behavior and fixture roots remain backward compatible;
- any application payload must name a registered profile;
- profile id is hash-bound into payloads and manifests;
- profiles define namespace, schema, retention, privacy, and validation policy;
- generic manifests bind application id, coordinate, payload hash, namespace
  root, share root, erasure scheme, and share counts;
- generic share sets reconstruct canonical application payloads from threshold
  valid shares;
- generic DA certificates require quorum custody-checked votes;
- generic RPC retrieves profiles, manifests, payloads, shares, namespaces,
  samples, status, repair state, and indexes;
- storage persists and indexes application profiles and DA evidence across
  restart and backup/restore;
- operators can audit DA health per application;
- private/encrypted/commitment-only record policy is enforced by profile;
- DeTTa DeFi and social-demo E2E flows both pass;
- formal/proof artifacts cover application profile binding, certificate
  binding, and reconstruction invariants;
- release gate includes at least one generic non-DeFi application DA scenario.

## 23. Initial Work Order

Recommended first implementation slice:

1. Add `DaApplicationId`, `DaApplicationCoordinate`, `DaPayloadKind`,
   `DaRecordEncoding`, `DaRecordEnvelope`, and `DaApplicationProfile` to
   `detta-da`.
2. Add canonical profile hashing and validation tests.
3. Add `social.demo` profile fixture.
4. Add `ApplicationDaPayload` canonicalization and hash tests.
5. Add `ApplicationDaManifest` and `ApplicationDaShareSet` using existing
   Reed-Solomon builder.
6. Add storage for profiles/manifests/payloads/shares.
7. Add minimal RPC and CLI for profile lookup and generic payload production.
8. Add E2E social-demo DA publish/retrieve/reconstruct test.
9. Add `detta.defi` adapter compatibility test.
10. Extend release gate after the focused tests are stable.

This sequence produces value quickly: a non-DeFi application can publish and
retrieve DA-certified data while the existing DeTTa DeFi DA path remains
unchanged.
