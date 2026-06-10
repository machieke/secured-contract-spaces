# DeTTa Production Implementation Plan

**Project name:** DeTTa  
**Meaning:** Distributed Transactional Atomspace  
**Target:** Production distributed DeFi Atomspace  
**Inputs:** `secured-contract-spaces.md`, `scs-formal.md`,
`detta-implementation-plan.md`

# 0. Implementation Progress

Updated: 2026-06-10

Completed production-plan slices:

- [x] Production implementation plan created.
- [x] Release artifact signing verification checks the release manifest
  checksum/signature plus every packaged artifact checksum/signature.
- [x] Incident-response drill script exercises packaged-node operator health,
  metrics, alerts, mempool, metadata-root, and slashing-evidence surfaces.
- [x] Hard release gate runs packaged-node launch and incident-response drills
  from a temporary release directory.
- [x] Governance bootstrap drill exercises timelocked upgrade scheduling,
  rehearsal, early-execution rejection, execution, and policy-update execution
  against the packaged node.
- [x] Validator onboarding drill boots and restarts multiple packaged
  validator identities from the same genesis and verifies authenticated roots.
- [x] Versioned DeTTa protocol envelope crate added.
- [x] Protocol golden fixtures added for transaction and vote envelopes.
- [x] Network transport routes messages through protocol envelope encoding and
  decoding.
- [x] Corrupt protocol envelope rejection is covered by tests.
- [x] Pending mempool transactions persist across node restart.
- [x] Persistent nodes ingest decoded transaction and block network envelopes.
- [x] Blocking TCP protocol stream can round-trip DeTTa protocol messages over
  local sockets.
- [x] TCP peers exchange and validate `PeerHello` identity metadata.
- [x] Peer connection manager validates peer metadata and enforces bounded
  receive loops.
- [x] Persistent nodes gossip admitted transactions to validator peers.
- [x] Persistent nodes propagate consensus votes and finality certificates over
  network envelopes.
- [x] External line-delimited JSON RPC transport serves typed requests over TCP
  sockets.
- [x] Signed validator protocol envelopes use Ed25519 signatures with network,
  chain, protocol-version, and message-domain separation.
- [x] Chunked snapshot sync protocol messages reconstruct authenticated state
  snapshots and reject missing, duplicate, and tampered chunks.
- [x] Persistent nodes verify signed validator envelopes against a trusted
  validator keyring before ingesting the inner protocol message.
- [x] Persistent nodes serve authenticated snapshot manifests and chunk ranges
  from durable storage for state sync.
- [x] Signed block proposals propagate as verified validator envelopes, with a
  real TCP signed proposal/signed vote round trip covered by tests.
- [x] Persistent nodes collect verified signed votes from TCP validator peers
  and assemble a finality certificate at quorum.
- [x] Proposer nodes persist finality certificates and gossip signed certificate
  envelopes to validator peers.
- [x] TCP validator connections support bounded retry with delayed-peer and
  exhausted-budget coverage.
- [x] Network layer exposes retry metrics and a peer-scored reconnect queue for
  long-running validator processes.
- [x] Signed equivocation evidence can be gossiped by trusted reporters and
  persisted as durable slashing records.
- [x] Validator-set metadata persists network, chain, and validator public keys
  and reloads the node keyring on restart.
- [x] Signed validator-set metadata updates apply to durable metadata and
  replay-protected node keyrings.
- [x] Validator-set metadata updates require quorum authorization from
  independently verified current validators before mutating durable keyrings.
- [x] Quorum-authorized validator-set metadata update authorizations gossip over
  in-memory and TCP validator transports before durable application.
- [x] Pending validator-set metadata authorizations persist across restarts and
  resume quorum assembly before clearing after durable application.
- [x] RPC wire types and persistent-node handling support validator-set metadata
  update proposal authorizations and quorum status queries.
- [x] Persistent validator nodes serve validator-set metadata proposal and
  status methods over the shared TCP JSON RPC server path.
- [x] Validator-set metadata updates carry optional expiry heights, and nodes
  reject or prune stale pending authorizations from durable storage.
- [x] Pending validator-set metadata authorization pools are bounded, with
  per-validator limits to prevent one signer from occupying unbounded updates.
- [x] Signed validator-set metadata update audit records persist applied,
  pruned, and rejected lifecycle outcomes with signer sets and reasons.
- [x] Validator-set metadata audit records are exposed through the node-aware RPC
  request path.
- [x] Validator-set metadata audit records support retention-bounded appends and
  paginated node/RPC reads with a configured page-size cap.
- [x] Persistent node snapshots include a deterministic validator-set metadata
  audit root that changes with audit records and survives restart.
- [x] Persistent node snapshot metadata roots are exposed over the node-aware RPC
  path and the shared TCP JSON RPC server.
- [x] Snapshot-sync manifests include persistent node metadata roots so chunk
  verification authenticates the validator-set audit root.
- [x] Snapshot reconstruction/import can require persistent metadata roots and
  rejects mismatched validator-set audit roots before committing snapshots.
- [x] Verified snapshot metadata roots persist alongside imported snapshots and
  survive restart for later RPC/state-sync reporting.
- [x] Local validator-set audit writes replace stale imported snapshot metadata
  roots after retention pruning.
- [x] Snapshot metadata root status is exposed over node-aware RPC, including
  imported-vs-local replacement state.
- [x] TCP state-sync client flows require snapshot metadata roots derived from
  source status before accepting and importing chunk sets.
- [x] TCP state-sync clients report request, resume, chunk, and metadata-root
  verification metrics for paged imports.
- [x] TCP state-sync clients retry transient chunk-stream failures and preserve
  metadata-root verification on the successful reconnect.
- [x] TCP state-sync client metrics persist through storage and reload after
  restart for sync diagnostics.
- [x] Persisted TCP state-sync diagnostics are exposed through node-aware RPC,
  including the empty-before-sync state.
- [x] Persisted TCP state-sync diagnostics are served over the shared TCP JSON
  RPC path.
- [x] Persistent node snapshot root responses include persisted state-sync
  diagnostics when available.
- [x] Persisted state-sync diagnostics are hashed into snapshot metadata roots
  so manifests can authenticate diagnostics for audit proofs.
- [x] Verified snapshot imports can require state-sync diagnostics roots and
  reject mismatched diagnostics roots before commit.
- [x] Verified snapshot imports persist the exact required metadata-root set
  used for the import and reload it after restart.
- [x] Required snapshot metadata-root sets are exposed through node-aware RPC
  after verified import and after restart.
- [x] Required snapshot metadata-root sets are served over the shared TCP JSON
  RPC path.
- [x] Required snapshot metadata-root sets are included in persistent snapshot
  root responses for audit discovery.
- [x] Required snapshot metadata-root requirement sets have stable roots exposed
  through dedicated RPC reports and persistent snapshot root responses.
- [x] State-sync client diagnostics record the stable root of the required
  metadata-root set verified by successful sync attempts.
- [x] Snapshot manifests bind persisted required metadata-root requirement roots
  for downstream state-sync audit proofs.
- [x] Verified snapshot imports append persistent audit records with snapshot
  root, manifest hash, required-root-set root, root counts, and chunk count.
- [x] Snapshot import audit records and their stable root are exposed through
  node-aware RPC.
- [x] Snapshot import audit records and their stable root are served over the
  shared TCP JSON RPC path.
- [x] Snapshot import audit roots are included in persistent snapshot root
  responses for consolidated audit discovery.
- [x] Snapshot manifests bind snapshot import audit roots when import audit
  records exist.
- [x] Downstream verified imports can require snapshot import audit roots and
  reject mismatched import-audit roots before commit.
- [x] Snapshot import audit records support retention limits and bounded
  pagination through storage, node loaders, and RPC-backed flows.
- [x] Persistent snapshot root responses include snapshot import audit
  retention diagnostics: retained count, max records, and max page size.
- [x] Snapshot import audit retention-limit configuration persists through
  storage and reloads after node restart.
- [x] Snapshot import audit retention configuration is exposed through
  node-aware RPC.
- [x] Snapshot import audit retention configuration is served over the shared
  TCP JSON RPC path.
- [x] Snapshot manifests bind snapshot import audit configuration roots and
  downstream imports can require them.
- [x] Snapshot import audit configuration roots are included in persistent
  snapshot root responses for consolidated discovery.
- [x] Snapshot import audit configuration roots are exposed through dedicated
  node-aware and TCP JSON RPC for lightweight clients.
- [x] Snapshot metadata root status reports local, persisted, and effective
  snapshot import audit configuration roots.
- [x] Snapshot metadata root status reports local, persisted, and effective
  snapshot import audit roots.
- [x] Snapshot metadata root status reports local, persisted, and effective
  required snapshot metadata-root set roots.
- [x] Snapshot metadata root status effective-root precedence matches snapshot
  manifest metadata-root emission precedence.
- [x] Snapshot metadata root status reports local, persisted, and effective
  state-sync client metrics roots.
- [x] Expanded snapshot metadata root diagnostics are asserted over the shared
  TCP JSON RPC path.
- [x] Stable JSON client fixtures cover expanded snapshot metadata root status
  responses.
- [x] Expanded snapshot metadata root status RPC semantics are documented for
  client implementers.
- [x] Client signed transaction submission verifies Ed25519 payloads, requires
  registry-authenticated account signer grants, and exposes distinct stable
  errors for bad signatures and unregistered signer keys.
- [x] Account signer keys can be registered and revoked through a normal
  policy-governed account-registry contract, with authenticated registry proofs
  and signed-client E2E coverage.
- [x] Transactions carry an optional validity height, signed payloads bind that
  field, and expired transactions are rejected at admission with a stable RPC
  error.
- [x] Formal theorem coverage is exported as a stable checked-in proof-artifact
  manifest.
- [x] Proof-artifact manifests have stable SHA-256 roots for release
  attestations.
- [x] TLA formal model artifact roots are included in the proof-artifact
  manifest.
- [x] Formal model checking runbook documents proof-artifact checks and refresh
  workflow.
- [x] Theorem evidence references are checked against concrete TLA model
  operators.
- [x] A bounded TLC config artifact is included in the proof manifest.
- [x] Proof-artifact manifest JSON serialization has a stable schema fixture.
- [x] Proof-artifact manifest schema and version are explicit constants.
- [x] Restricted evaluator proof traces have stable golden fixtures.
- [x] Restricted evaluator forbidden primitives have stable golden fixtures.
- [x] Restricted evaluator subset and fixture schemas are documented.
- [x] Restricted evaluator golden fixtures have SHA-256 attestations.
- [x] Restricted evaluator fixture roots are bound into the proof-artifact
  manifest.
- [x] Restricted evaluator fixtures are referenced by theorem evidence.
- [x] Restricted evaluator resource exhaustion has a stable golden fixture.
- [x] Restricted evaluator resource exhaustion fixture has a SHA-256
  attestation.
- [x] Restricted evaluator resource exhaustion fixture root is bound into the
  proof-artifact manifest.
- [x] Restricted evaluator resource exhaustion fixture is referenced by theorem
  evidence.
- [x] Restricted evaluator arithmetic overflow has a stable golden fixture.
- [x] Restricted evaluator arithmetic overflow fixture has a SHA-256
  attestation.
- [x] Restricted evaluator arithmetic overflow fixture root is bound into the
  proof-artifact manifest.
- [x] Restricted evaluator arithmetic overflow fixture is referenced by theorem
  evidence.
- [x] Restricted evaluator proof trace root has a dedicated SHA-256
  attestation.
- [x] Restricted evaluator proof trace root attestation is bound into the
  proof-artifact manifest.
- [x] Proof-manifest tests ensure every checked-in evaluator attestation is
  bound.
- [x] Proof-manifest tests ensure every checked-in evaluator fixture JSON is
  bound.
- [x] Proof manifest v2 runtime artifact semantics are documented.
- [x] Proof-manifest refresh checks ensure evaluator fixture attestations match
  bound runtime artifact roots.
- [x] Proof-manifest refresh checks ensure evaluator trace-root attestations
  match fixture traces.
- [x] Formal runbook includes commands for evaluator runtime artifact
  attestations.
- [x] Release-audit checklist covers proof manifest v2 evaluator artifacts.
- [x] Stable evaluator fixture inventory is exported.
- [x] SHA-256 attestation exists for evaluator fixture inventory.
- [x] Evaluator fixture inventory is bound into the proof manifest.
- [x] Evaluator fixture inventory entries are validated against proof manifest
  roots.
- [x] Proof manifest runtime artifact paths are checked for uniqueness.
- [x] Proof manifest artifact roots are checked for lowercase SHA-256 format.
- [x] Proof manifest artifact paths are checked for the `models/` namespace.
- [x] Proof manifest artifact paths are checked against an extension allow-list.
- [x] Proof manifest model and runtime artifact paths are checked for
  disjointness.
- [x] Proof manifest model artifact paths are checked for uniqueness.
- [x] Proof manifest release attestations are checked for `sha256sum` format.
- [x] Proof manifest release attestation filenames are bound to target
  artifacts.
- [x] Proof manifest release attestation roots are checked against target
  bytes.
- [x] Proof manifest evaluator release attestations are checked against
  inventory coverage.
- [x] Proof manifest evaluator fixture JSON paths are checked against inventory
  coverage.
- [x] Evaluator fixture inventory schema, version, and evaluator identifiers are
  checked in verify.
- [x] Evaluator fixture inventory entry names are checked for uniqueness in
  verify.
- [x] Evaluator fixture inventory entry paths are checked for uniqueness in
  verify.
- [x] Evaluator fixture inventory roots are checked for lowercase SHA-256
  format in verify.
- [x] Evaluator fixture inventory trace-root metadata is checked against the
  proof-trace fixture and attestation in verify.
- [x] Evaluator fixture inventory fixture schemas are checked for uniqueness in
  verify.
- [x] Evaluator fixture inventory fixture schemas are checked for expected
  coverage in verify.
- [x] Evaluator fixture inventory fixture names are checked against expected
  schema mappings in verify.
- [x] Evaluator fixture inventory fixture names are checked against expected
  artifact path mappings in verify.
- [x] Evaluator fixture inventory fixture names are checked against expected
  artifact root mappings in verify.
- [x] Evaluator fixture inventory paths are checked for models namespace and
  expected suffixes in verify.
- [x] Evaluator fixture inventory attestation paths are checked against target
  artifact filenames in verify.
- [x] Evaluator fixture inventory trace-root metadata is checked for
  all-or-none proof-trace-only optionality in verify.
- [x] Evaluator fixture inventory fixture order is checked for deterministic
  stability in verify.
- [x] Restricted evaluator source parser and canonical instruction renderer
  round-trip the supported kernel-facing subset.
- [x] Proof manifest model artifact order is checked for deterministic
  stability in verify.
- [x] Proof manifest runtime artifact order is checked for deterministic
  stability in verify.
- [x] Proof manifest theorem coverage order is checked for deterministic
  stability in verify.
- [x] Proof manifest theorem evidence order is checked for deterministic
  stability in verify.
- [x] Proof manifest theorem evidence kinds are checked for expected coverage
  and runtime-test anchoring in verify.
- [x] Proof manifest theorem runtime-test evidence references are checked for
  expected DeTTa crate namespaces in verify.
- [x] Proof manifest theorem verifier evidence references are checked for the
  expected `detta_verify::verify_*` namespace in verify.
- [x] Proof manifest theorem model evidence references are checked for the
  expected `models/DeTTaBlockExecution.tla::*` namespace in verify.
- [x] Proof manifest theorem fixture evidence references are checked for the
  expected restricted evaluator JSON fixture namespace in verify.
- [x] Proof manifest theorem fixture evidence references are checked against
  evaluator fixture inventory coverage in verify.
- [x] Proof manifest theorem fixture evidence references are checked for
  expected evaluator fixture schema coverage in verify.
- [x] Proof manifest theorem fixture evidence references are checked for
  expected evaluator fixture name coverage in verify.
- [x] Proof manifest theorem model evidence references are checked for
  expected TLA operator coverage in verify.
- [x] Proof manifest theorem runtime-test evidence references are checked for
  expected DeTTa crate coverage in verify.
- [x] Proof manifest theorem verifier evidence references are checked for
  expected verifier function coverage in verify.
- [x] Proof manifest theorem evidence entries are checked for per-theorem
  uniqueness in verify.
- [x] Proof manifest theorem IDs are checked against expected theorem names in
  verify.
- [x] Proof manifest theorem names are checked for uniqueness in verify.
- [x] Proof manifest theorem IDs are checked for fixed format and sequence in
  verify.
- [x] Proof manifest theorem count is checked against coverage in verify.
- [x] Proof manifest project and scope metadata are checked in verify.
- [x] Proof manifest model and runtime artifact counts are checked in verify.
- [x] Proof manifest artifact roots are checked for SHA-256 hex length in
  verify.
- [x] Proof release attestation roots are checked for SHA-256 hex length in
  verify.
- [x] Proof release attestation count is checked in verify.
- [x] Proof release attestation filenames are checked for uniqueness in verify.
- [x] Proof release attestation filenames are checked for expected suffixes in
  verify.
- [x] Proof release attestation filenames are checked against the expected set
  in verify.
- [x] Proof release attestation test fixtures are consolidated behind a helper
  in verify.
- [x] Proof release attestation target paths are checked against the expected
  set in verify.
- [x] Proof release attestation target paths are checked for models namespace
  and relative path safety in verify.
- [x] Proof release attestation target paths are checked for uniqueness in
  verify.
- [x] Proof release attestation target filenames are checked for consistency in
  verify.
- [x] Proof release attestation target extensions are checked for consistency
  in verify.
- [x] Proof release attestation target paths are checked for expected models
  path depth in verify.
- [x] Proof release attestation roots are checked for lowercase hex in verify.
- [x] Proof release attestation filenames are checked to reject path
  separators in verify.
- [x] Proof release attestation files are checked for a single trailing newline
  in verify.
- [x] Proof release attestation lines are checked for a single sha256sum
  separator token in verify.
- [x] Proof release attestation lines are checked for nonempty root and
  filename parts in verify.
- [x] Proof manifest root attestation file is checked for a single trailing
  newline in verify.
- [x] Proof manifest root attestation line is checked for a single sha256sum
  separator token in verify.
- [x] Proof manifest root attestation line is checked for nonempty root and
  filename parts in verify.
- [x] Proof manifest root attestation filename is checked against the proof
  manifest artifact name in verify.
- [x] Proof manifest root attestation root is checked for SHA-256 hex length in
  verify.
- [x] Proof manifest root attestation root is checked for lowercase hex in
  verify.
- [x] Proof manifest root attestation filename is checked to reject path
  separators in verify.
- [x] Proof manifest root attestation filename is checked for the JSON suffix
  in verify.
- [x] Proof manifest root attestation root is checked against the proof
  manifest JSON bytes in verify.
- [x] Proof manifest root attestation parser is checked against direct
  sha256sum splitting in verify.
- [x] Local release gate script runs formatting, clippy, tests, verifier, and
  proof artifact attestation checks.
- [x] CI workflow runs the local release gate on pull requests and master
  pushes.
- [x] CI dependency audit checks RustSec advisories through cargo-deny.
- [x] Local release gate invokes the checked-in cargo-deny advisory policy,
  warning when cargo-deny is not installed unless `DETTA_REQUIRE_DEP_AUDIT=1`
  is set.
- [x] Release-candidate audit findings are tracked in a checked JSON manifest
  and verified to contain no unresolved findings by `detta-verify`.
- [x] Public-testnet readiness is tracked in a checked manifest that validates
  required workflow evidence while keeping external blockers explicit.
- [x] Mainnet-candidate readiness is tracked in a checked manifest that
  validates required launch gates, signed release artifact metadata, and
  dependency on public-testnet readiness.
- [x] Public-testnet, mainnet-candidate, and audit readiness manifests have a
  standalone verifier that checks schemas, blocker semantics, evidence paths,
  signed-artifact hashes, and targeted `detta-verify` readiness tests.
- [x] Readiness manifests have a deterministic status report that binds
  blocker state, manifest hashes, evidence inventory hashes, and source-state
  metadata for launch review.
- [x] Readiness status reports have a standalone verifier that checks transfer
  checksums, schema, manifest hashes, evidence inventory hashes, evidence file
  bytes, blocker/status consistency, and current manifest binding.
- [x] Local release gate performs a locked release build reproducibility smoke
  check.
- [x] Local release gate runs the deterministic generated-corpus fuzz smoke
  test explicitly.
- [x] Local release gate and CI run the checked-in TLA+ bounded simulation job.
- [x] Durable mempool admission enforces deterministic budget, byte-size,
  global-pending, and per-sender pending limits with stable RPC errors.
- [x] Peer protocol-version negotiation accepts adjacent compatible version
  windows and rejects incompatible windows over manager and TCP handshakes.
- [x] AMM pools support configurable swap fees with explicit per-asset
  collected-fee accounting and rounding tests.
- [x] Staking contracts support configured rewards and delayed unbonding with
  aggregate pending-unbond invariants and block-height tests.
- [x] Staking contracts support configured exit penalties with explicit
  collected-penalty accounting and immediate/delayed exit tests.
- [x] Node health RPC reports chain/network identity, height, mempool size,
  validator-key counts, pending validator-set updates, and authenticated roots.
- [x] Lending vaults support per-block debt interest, liquidation thresholds,
  collateral seizure, and explicit bad-debt accounting tests.
- [x] Governance upgrades expose forked-state rehearsal reports with invariant
  failures and authenticated roots before execution.
- [x] Upgrade rehearsal reports are exposed over shared RPC for operator
  pre-execution checks.
- [x] Persistent validator nodes serve upgrade rehearsal reports over the TCP
  JSON RPC transport used by production operators.
- [x] Persistent validator nodes serve finalized block, transaction, and
  receipt history from durable block storage after restart over node-aware and
  TCP JSON RPC paths.
- [x] RPC exposes independently verifiable receipt and event Merkle proofs, with
  persistent nodes serving finalized receipt proofs from durable block storage
  after restart.
- [x] Stable JSON client fixtures cover receipt-proof and event-proof RPC
  response shapes.
- [x] JSON RPC exposes opt-in operator endpoint wrappers for bearer-token
  authentication and per-connection request limiting without breaking public
  request compatibility.
- [x] JSON RPC request, response, proof, persistent-node, and operator-guard
  semantics are documented for client and operator implementers.
- [x] Validator block proposal and import enforce deterministic block-level
  resource limits, leaving overflow mempool transactions pending and returning
  stable RPC errors for oversized blocks.
- [x] RPC exposes bounded paginated event-history reads for indexer catch-up
  while preserving the existing full event-log query.
- [x] Persistent validator nodes expose stored finality certificates through
  typed RPC with stable not-found errors after restart.
- [x] Finality-certificate RPC compatibility is covered by stable JSON client
  fixtures and persistent-node TCP JSON after-restart tests.
- [x] Persistent validator nodes expose durable slashing/equivocation records
  through typed RPC, stable JSON fixtures, and TCP JSON after-restart tests.
- [x] Mempool status RPC reports pending count, per-sender pressure, admission
  limits, and block resource limits over in-memory, TCP, and restart paths.
- [x] RPC supports bounded polling subscriptions for new block, receipt, and
  event notifications with stable JSON fixtures and TCP coverage.
- [x] Finality-certificate persistence publishes bounded RPC subscription
  notifications with stable JSON fixtures and node-aware coverage.
- [x] Governance timelock queues expose scheduled code upgrades and policy
  updates through typed RPC with stable JSON fixtures.
- [x] Persistent validator nodes serve governance timelock queue RPC methods
  over the shared TCP JSON operator path.
- [x] RPC exposes bounded committed-block pages for catch-up clients, including
  durable after-restart persistent-node TCP coverage.
- [x] Light-client finality certificate verification checks height, block hash,
  active validator membership, and quorum without full node state.
- [x] RPC decoding has deterministic malformed and valid boundary corpus
  fuzz-smoke coverage through the JSON request handler.
- [x] Bridge redemption proofs are bound to authenticated trusted source-chain
  validator sets, including signer membership, quorum, and set-update tests.
- [x] External JSON RPC is available over bounded HTTP POST transport with
  stable transport error bodies and compatibility tests.
- [x] The external JSON RPC HTTP endpoint has a versioned OpenAPI schema whose
  method enum is covered by crate tests.
- [x] Deterministic generated DeFi corpus fuzz-smoke covers AMM, lending,
  staking, committed/reverted receipts, and post-replay declared invariants.
- [x] Operator runbook documents deployment, monitoring, recovery, governed
  upgrades, validator-key rotation, and incident response using current RPCs.
- [x] Durable consensus-signing records prevent validators from signing
  conflicting block proposals, votes, or finality certificates across restart
  and network-partition simulations.
- [x] Persistent operator metrics expose observed peer count, mempool size,
  consensus/finality height and lag, recent block/proof latency, storage bytes,
  and RPC error count through typed RPC and stable JSON/OpenAPI coverage.
- [x] Persistent operator alerts evaluate stalled consensus, root mismatch,
  excessive reverts, peer isolation, slashing evidence, disk pressure, RPC
  overload, and mempool saturation with typed RPC and stable fixtures.
- [x] File-storage backup and restore copies durable node state with a manifest
  covering bytes, file count, snapshot root, highest block height, and highest
  finality-certificate height, with persistent-node restore verification.
- [x] Release signing drill signs packaged release checksum attestations with
  an ephemeral GPG key, verifies them with the production release-signature
  verifier, and records a signing-drill report.
- [x] Packaged `detta-client` TCP RPC binary submits token deployment, pool
  deployment, liquidity, swap, receipt, block-production, and state-root
  commands for end-user DeFi workflows.
- [x] Packaged-client release drill unpacks the release archive, boots
  packaged `detta-node`, runs packaged `detta-client` through token, pool,
  liquidity, buy, and sell commands, and records a checksummed report.
- [x] Genesis-finalization drill binds the release manifest, generated genesis
  authenticated roots, faucet sample, validator identities, quorum, and
  checksums into a retained launch artifact report.
- [x] Public-testnet stability drill runs a packaged local node through a
  sustained multi-block DeFi workload and records health, metrics, mempool,
  root, and alert observations.
- [x] Audit-readiness package script bundles tracked source, generated release
  artifacts, proof manifests, readiness manifests, and security findings into a
  deterministic checksummed archive, and the hard release gate requires it.
- [x] Audit-readiness packages have a standalone verifier that checks transfer
  checksums, embedded report schema, inventory hashes, mandatory evidence
  files, release manifest binding, proof-manifest binding, and closed audit
  findings.
- [x] Release and audit manifests include source-state metadata for commit,
  tree, dirty-worktree status, tracked/untracked counts, and diff/status roots,
  with optional clean-source enforcement for final publication and audit
  handoff.
- [x] Retained release-candidate evidence bundle script reruns packaged
  operator drills under one version and bundles release artifacts, reports,
  checksums, signatures, audit package, signing key, readiness status report,
  and source-state metadata.
- [x] Retained release-candidate evidence bundles have a standalone verifier
  that checks archive paths, report schema, inventory hashes, evidence file
  hashes, required drill reports, source-state consistency, and the embedded
  audit-readiness package.
- [x] Retained release-candidate evidence bundles require the readiness status
  report and rerun its standalone verifier against the packaged copy.
- [x] Retained release-candidate evidence bundle verification imports the
  packaged signing public key and reruns release-signature verification against
  the packaged release artifacts and detached signatures.
- [x] Hard release gate runs the standalone readiness-manifest verifier so
  launch status metadata cannot drift from checked evidence paths.
- [x] Hard release gate generates the deterministic readiness status report so
  launch blocker reporting remains machine-checkable.
- [x] Hard release gate verifies the generated readiness status report before
  continuing to release artifact and proof checks.

Next implementation slices:

No pending implementation slices are listed. Add the next slice here when new
production acceptance work is scoped.

# 1. Production Objective

DeTTa is a production distributed Atomspace for DeFi where all durable state
changes are finalized by consensus and executed by a deterministic Secured
Contract Spaces (SCS) runtime.

The production objective is:

```text
Operate a Byzantine-fault-tolerant distributed Atomspace where balances,
permissions, markets, oracle state, governance state, bridge state, and DeFi
contract state can change only through policy-authorized, invariant-preserving,
resource-metered, atomic transitions with authenticated proofs.
```

The current prototype establishes the deterministic executor, authenticated
roots, validator replay simulation, core DeFi slices, and proof-checked bridge
message shape. This plan defines the remaining work required for a production
distributed DeFi system.

# 2. Production Scope

Production DeTTa must include:

- real validator networking;
- durable mempool and transaction gossip;
- BFT consensus or a finality protocol with accountable validator signatures;
- persistent block, state, receipt, event, and proof storage;
- state sync for full nodes and validators;
- external RPC APIs for wallets, relayers, indexers, and operators;
- a production restricted MeTTa/PeTTa evaluator;
- hardened token, AMM, oracle, lending, staking, governance, and bridge modules;
- invariant tooling, fuzzing, adversarial tests, and differential replay;
- formal proof artifacts for the generic SCS runtime obligations;
- deployment, observability, key management, incident response, and upgrade
  procedures.

# 3. Production Non-Goals

These are out of scope for the first production release unless explicitly
scheduled later:

- private balances;
- zero-knowledge execution;
- arbitrary host-language interop inside contracts;
- permissionless raw `add-atom`, `remove-atom`, or private-state `match`;
- synchronous cross-shard writes;
- speculative parallel execution as the only execution path;
- unbounded general-purpose distributed logic programming.

# 4. Production Readiness Definition

DeTTa is production-ready only when all of the following hold:

- a multi-validator network reaches finality over real network links;
- finalized blocks contain verifiable validator signatures or threshold
  certificates;
- independent nodes can join, sync, verify, and serve the latest state;
- the external RPC surface is stable, documented, authenticated where needed,
  and tested by client fixtures;
- the mempool is durable, bounded, replay-safe, DoS-resistant, and observable;
- all DeFi contracts have declared invariants and adversarial tests;
- failed transitions revert storage, registry, events, messages, and scheduled
  effects;
- all roots and proofs are canonical and cross-platform deterministic;
- the restricted evaluator is implemented against a precise language subset;
- generic SCS theorems have model-checking evidence or mechanized proof
  artifacts;
- release builds run in CI with tests, fuzzing, clippy, formatting, audit
  checks, and formal/model-checking jobs;
- operators can deploy, monitor, rotate keys, perform upgrades, and recover
  from faults using documented runbooks.

# 5. Architecture

```text
Clients / wallets / bots / indexers / relayers
        |
        v
External RPC and subscriptions
        |
        v
Admission service and durable mempool
        |
        v
P2P network
  transaction gossip
  block proposal propagation
  vote/certificate propagation
  state sync
        |
        v
Consensus and finality
        |
        v
Deterministic block executor
        |
        v
SCS runtime
  dispatcher
  policy engine
  capability registry
  guarded storage kernel
  restricted MeTTa/PeTTa evaluator
  invariant checker
        |
        v
Authenticated Atomspace storage
  contract table
  policy store
  registry store
  state store
  nonce store
  event store
  receipt store
  inbox/outbox stores
        |
        v
Roots, proofs, snapshots, indexes, archives
```

# 6. Workstreams

## 6.1 Protocol Specification

Deliver:

- canonical binary encoding for transactions, blocks, votes, certificates,
  state keys, values, events, receipts, and proofs;
- versioned protocol envelopes;
- chain ID and network ID rules;
- validator signature domain separation;
- deterministic integer, rounding, and overflow rules;
- error-code stability policy;
- genesis format and validator-set format;
- hard-fork and soft-upgrade compatibility rules.

Acceptance:

- every protocol object has golden cross-platform fixtures;
- non-canonical encodings are rejected;
- signature domains prevent cross-chain and cross-message replay;
- version negotiation is tested between adjacent protocol versions.

## 6.2 Production Atomspace Storage

Deliver:

- authenticated storage engine for atom tuples and SCS state;
- durable block, receipt, event, registry, policy, nonce, inbox, and outbox
  stores;
- canonical key schema with migration support;
- snapshot format with chunking and verification;
- archive mode and pruned mode;
- crash-consistency guarantees.

Acceptance:

- storage survives process crash at every commit boundary;
- restored nodes reproduce the exact latest global root;
- corrupt chunks, missing chunks, and wrong roots are rejected;
- archive nodes can prove historical state, events, receipts, and messages;
- pruned nodes can sync and serve current proofs.

## 6.3 Networking

Deliver:

- real peer transport, preferably QUIC or libp2p-based;
- peer identity and handshake protocol;
- peer discovery and allowlist/permissioned-validator mode;
- transaction gossip;
- block proposal propagation;
- vote and certificate propagation;
- evidence propagation;
- peer scoring and rate limiting;
- network metrics.

Acceptance:

- a local multi-process network finalizes blocks over real sockets;
- validators recover from disconnect and reconnect;
- duplicate, malformed, oversized, and stale messages are dropped;
- equivocation evidence is propagated and applied;
- network partitions do not produce two finalized conflicting blocks under the
  assumed fault threshold.

## 6.4 Durable Mempool

Deliver:

- persistent transaction pool;
- admission validation;
- fee or priority ordering policy;
- per-sender nonce queue;
- per-account and global rate limits;
- duplicate and replay filtering;
- gossip rebroadcast rules;
- eviction rules;
- mempool observability.

Acceptance:

- transactions survive node restart before block inclusion;
- invalid signature, wrong chain, replayed nonce, expired, oversized, and
  underfunded transactions are rejected at admission;
- mempool ordering is deterministic for a given policy;
- validators converge on substantially similar pending sets under gossip;
- spam load does not prevent valid transactions from being proposed.

## 6.5 Consensus and Finality

Deliver:

- selected BFT protocol or finality gadget;
- validator signing keys and key rotation;
- proposal, prevote, precommit, and finality certificate flow, or equivalent;
- durable consensus state;
- fork choice and finality rules;
- slashing/evidence integration;
- validator-set updates;
- light-client-verifiable finality certificates.

Acceptance:

- `3f + 1` validators tolerate `f` Byzantine validators in integration tests;
- finality certificates verify independently from full node state;
- nodes reject blocks with invalid roots, invalid proposer, wrong height,
  invalid previous hash, bad signatures, or wrong validator set;
- restarted validators resume consensus without double-signing;
- equivocation is detected, persisted, gossiped, and slashable;
- validator-set changes take effect only after finalized governance approval.

## 6.6 Block Execution

Deliver:

- deterministic executor separated from consensus;
- exact resource metering;
- block-level gas/resource limits;
- transaction-level gas/resource limits;
- deterministic event and receipt ordering;
- deterministic failure semantics;
- execution trace capture for verification.

Acceptance:

- all validators executing the same finalized block produce identical roots;
- root mismatches halt import and emit actionable diagnostics;
- failed transactions commit only admission metadata specified by the protocol;
- state, registry, events, messages, upgrades, and policy changes roll back on
  revert;
- execution traces can be replayed by the verifier.

## 6.7 State Sync

Deliver:

- snapshot creation and signing;
- chunked snapshot transfer;
- incremental catch-up from finalized blocks;
- proof-verified snapshot import;
- fast sync for new full nodes;
- validator resync workflow;
- archive sync workflow.

Acceptance:

- a new full node can sync from genesis or snapshot to latest finalized height;
- imported snapshots are rejected if any root or chunk proof fails;
- state sync works across process restart and peer rotation;
- catch-up cannot skip finalized validator-set changes;
- synced nodes can serve valid storage, registry, receipt, event, and message
  proofs.

## 6.8 External RPC

Deliver:

- HTTP and/or gRPC API;
- JSON-RPC compatibility if required by wallets;
- transaction submission;
- block, transaction, receipt, event, state, contract, and proof queries;
- view calls;
- subscription API for new blocks, receipts, events, and finality;
- OpenAPI or protobuf schemas;
- client SDK fixtures;
- authentication and rate limiting for operator endpoints.

Acceptance:

- external clients can submit transactions and observe finalized receipts;
- proof endpoints return independently verifiable proofs;
- view calls are read-only and do not alter nonce, event, registry, or storage
  roots;
- RPC remains compatible across patch releases;
- malformed requests produce stable errors;
- indexers can reconstruct event history from RPC.

## 6.9 Restricted MeTTa/PeTTa Evaluator

Deliver:

- precise supported language subset;
- parser and canonical AST;
- deterministic evaluator;
- allowed primitive table;
- forbidden primitive traps;
- resource accounting by step, memory, and output size;
- kernel API for guarded storage and registry access;
- trace emission;
- conformance tests against MeTTa/PeTTa fixtures.

Acceptance:

- raw atom mutation, raw private match, Prolog assertion/retraction, Python
  calls, filesystem access, process execution, network access, wall clock,
  randomness, and arbitrary imports are impossible from contract code;
- identical source produces identical AST, bytecode or IR, traces, and roots;
- every state write appears in a kernel trace;
- resource exhaustion reverts atomically;
- evaluator behavior is covered by golden fixtures and differential tests;
- supported MeTTa/PeTTa semantics are documented and versioned.

## 6.10 SCS Runtime Hardening

Deliver:

- dispatcher-only external mutation;
- method-scoped policy engine;
- capability registry;
- guarded storage kernel;
- reentrancy controls;
- nested subtransactions;
- view enforcement;
- error taxonomy;
- schema validation;
- invariant hooks;
- audit logging.

Acceptance:

- every generic theorem THM-001 through THM-015 from `scs-formal.md` has a
  corresponding test, model-checking artifact, or mechanized proof;
- forged caller context fails;
- missing policy denies execution;
- raw storage mutation is not exposed through public APIs;
- cross-contract calls do not inherit caller write scope;
- event, registry, message, upgrade, and storage effects are atomic;
- schema violations and arithmetic overflow revert before commit.

## 6.11 Production DeFi Contracts

Deliver:

- fungible token;
- AMM;
- oracle adapter;
- lending vault;
- staking/rewards;
- router;
- bridge/message adapter;
- governance/timelock controller;
- emergency pause and recovery tools.

Acceptance:

- all contracts declare invariants;
- token supply and allowance invariants hold under fuzzing;
- AMM reserve, LP, fee, and rounding invariants hold;
- oracle updates require live authorization and reject stale data;
- lending handles collateral valuation, interest, liquidation, bad debt,
  rounding, and oracle failure modes;
- staking handles rewards, slashing or penalties if configured, unbonding, and
  accounting invariants;
- governance changes require authority, finality, timelock, audit events, and
  migration invariant checks;
- bridge messages are finality-proof-checked, replay-protected, validator-set
  bound, and signature verified.

## 6.12 Cross-Shard and Bridge Protocol

Deliver:

- outbox and inbox stores;
- source-chain finality proof format;
- validator-set proof format;
- signature or threshold-certificate verification;
- relayer protocol;
- replay protection;
- message timeout and failure handling;
- bridge accounting invariants.

Acceptance:

- source messages cannot mutate remote state directly;
- destination consumption requires valid source finality plus message inclusion;
- stale, duplicate, malformed, wrong-destination, wrong-contract, wrong-asset,
  wrong-recipient, and wrong-amount proofs are rejected;
- validator-set changes are reflected in bridge proof verification;
- relayer failure does not compromise funds;
- all bridge accounting invariants hold under replay and equivocation tests.

## 6.13 Governance, Upgrades, and Operations

Deliver:

- governance proposal lifecycle;
- voting or multisig authority model;
- timelock queues;
- emergency pause;
- upgrade packages;
- migration scripts;
- migration invariant declarations;
- staged rollout and rollback process;
- operator runbooks.

Acceptance:

- critical changes require governance authority and timelock;
- migration scripts cannot write outside declared migration scope;
- upgrades preserve declared invariants;
- all governance actions emit auditable events;
- emergency pause can stop state-changing methods without corrupting state;
- upgrade rehearsals run on forked state before production execution.

## 6.14 Verification and Formal Methods

Deliver:

- TLA+ model with TLC configs and committed run artifacts;
- mechanized proof plan for SCS kernel;
- Lean, Coq, Isabelle, K, or equivalent proof artifacts for core theorem set,
  or a documented staged path to them;
- symbolic execution harness;
- invariant DSL or manifest format;
- property tests and fuzzing;
- differential replay harness;
- adversarial test suites;
- CI gates for all verification layers.

Acceptance:

- THM-001 through THM-015 are linked to checked evidence;
- the block execution model is model-checked for representative bounded
  validator, transaction, and failure sets;
- contract traces are rejected when they violate write scope or isolation;
- DeFi invariants are checked after every successful transition and in fuzz
  campaigns;
- deterministic replay runs continuously in CI;
- proof artifacts are versioned with the protocol they verify.

## 6.15 Security Hardening

Deliver:

- threat model update;
- key management and signing isolation;
- dependency audit;
- supply-chain controls;
- fuzzing targets;
- adversarial network tests;
- DoS tests;
- external audit preparation;
- incident response procedures.

Acceptance:

- validators do not double-sign across restart, crash, or network partition
  tests;
- malformed network and RPC inputs cannot crash the node;
- resource limits bound CPU, memory, disk, and network usage;
- fuzzers cover transaction decoding, block import, proof verification,
  evaluator parsing, evaluator execution, RPC decoding, and bridge proof
  validation;
- audit findings are tracked to closure before mainnet release.

## 6.16 Observability and Operations

Deliver:

- structured logs;
- metrics;
- tracing;
- health endpoints;
- validator dashboards;
- alerting rules;
- backup and restore;
- key rotation procedures;
- release and rollback runbooks.

Acceptance:

- operators can observe peer count, mempool size, consensus height, finality
  lag, block execution time, proof serving latency, disk growth, and error
  rates;
- alerts fire for stalled consensus, root mismatch, excessive reverts, peer
  isolation, slashing evidence, disk pressure, and RPC overload;
- a node can be restored from backup and verified against finalized roots.

# 7. Milestones

## M0: Production Specification Freeze

Deliver:

- protocol object specs;
- canonical encoding fixtures;
- finality model decision;
- storage format decision;
- RPC schema draft;
- evaluator subset draft.

Exit criteria:

- protocol fixtures pass in CI;
- open production decisions are tracked with owners;
- no implementation proceeds without versioned object schemas.

## M1: Multi-Process Testnet Node

Deliver:

- real peer transport;
- durable mempool;
- multi-process validators;
- block proposal and vote propagation;
- finalized block import;
- persistent node storage.

Exit criteria:

- at least four local validator processes finalize blocks over sockets;
- restart and reconnect tests pass;
- invalid blocks and votes are rejected.

## M2: State Sync and External RPC

Deliver:

- snapshot sync;
- block catch-up;
- external RPC server;
- proof APIs;
- subscriptions.

Exit criteria:

- a fresh full node syncs to latest finalized height;
- wallets and indexer fixtures can submit, query, subscribe, and verify proofs;
- RPC compatibility tests pass.

## M3: Production SCS Runtime and Evaluator

Deliver:

- production restricted MeTTa/PeTTa evaluator;
- kernel trace conformance;
- resource metering;
- runtime hardening;
- theorem coverage evidence.

Exit criteria:

- all forbidden primitive tests pass;
- all supported language fixtures pass;
- resource exhaustion and parser fuzzing pass;
- SCS theorem evidence is complete in CI.

## M4: Production DeFi Suite

Deliver:

- hardened token, AMM, oracle, lending, staking, router, bridge, and governance
  contracts;
- invariant manifests;
- fuzz and adversarial tests;
- migration tests.

Exit criteria:

- DeFi invariants pass property tests and adversarial scenarios;
- lending and staking economic edge cases are covered;
- bridge proof verification is bound to real validator-set finality.

## M5: Formal Verification Baseline

Deliver:

- TLC model-checking configs and run artifacts;
- mechanized proof artifacts or proof skeletons for the SCS kernel;
- symbolic trace verifier;
- CI verification jobs.

Exit criteria:

- bounded model checks pass;
- proof artifacts correspond to the current protocol version;
- trace conformance tests link implementation traces to the model.

## M6: Public Testnet

Deliver:

- deployable validator binaries;
- genesis tooling;
- operator documentation;
- monitoring stack;
- faucet and sample clients;
- external audit readiness package.

Exit criteria:

- public testnet runs for a defined stability window;
- no unresolved critical or high-severity security issues;
- state sync, RPC, governance, bridge, and DeFi workflows run end to end.

## M7: Mainnet Candidate

Deliver:

- audited release candidate;
- finalized genesis;
- validator onboarding;
- governance bootstrapping;
- incident response drills;
- release signing.

Exit criteria:

- all production acceptance gates pass;
- audits are closed or explicitly accepted by governance;
- operators complete launch rehearsal;
- release artifacts are reproducible.

# 8. CI and Release Gates

Every production release candidate must pass:

- `cargo fmt`;
- `cargo clippy --all-targets -- -D warnings`;
- unit tests;
- integration tests;
- multi-process network tests;
- state sync tests;
- RPC compatibility tests;
- deterministic replay tests;
- fuzz smoke tests;
- model-checking jobs;
- proof artifact checks;
- dependency audit;
- reproducible build checks.

# 9. Closed Implementation Choices And Remaining Launch Gates

The current implementation has closed the main M1/M2 architecture choices for
the in-repo DeTTa runtime:

- consensus uses signed validator proposals, votes, finality certificates,
  equivocation evidence, and quorum-authorized validator-set metadata updates;
- networking uses the version-negotiated TCP protocol and signed envelopes in
  `detta-protocol`, `detta-network`, and `detta-node`;
- validator and transaction signing use Ed25519-domain signed envelopes and
  account signer registry grants;
- deterministic serde-backed canonical roots bind storage, registry, policy,
  event, nonce, outbox, receipt, block, and global state;
- durable node state uses the `detta-storage` file-backed store with backup,
  restore, snapshot, and state-sync coverage;
- RPC uses typed JSON-RPC over HTTP/TCP with OpenAPI and stable JSON fixtures;
- resource pricing is budget-based with transaction, block, and evaluator step
  limits;
- the evaluator strategy is the restricted evaluator plus verified MeTTa aspect
  runtime, guarded host-call traces, and artifact roots;
- invariant checking is hybrid: runtime enforcement, symbolic trace checks,
  differential replay, theorem evidence, and TLA+ model simulation;
- bridge finality uses validator-set certificates, replay tracking, outbox
  proofs, and cross-shard message roots;
- governance uses registry-backed admin grants, timelocks, rehearsals, policy
  updates, upgrade execution, and audit records.

Remaining launch gates are operational rather than unresolved implementation
architecture: public testnet stability, external audit, final genesis and
validator onboarding, mainnet deployment topology, incident-response rehearsal,
release signing, and dependency audit in CI.

# 10. Final Acceptance Checklist

The full production DeTTa plan is complete only when:

- real networked validators finalize blocks under the selected fault model;
- mempool, consensus, execution, storage, RPC, and state sync survive restart;
- all roots and proofs verify independently;
- the production restricted evaluator executes the supported MeTTa/PeTTa subset;
- DeFi contracts satisfy invariant, fuzz, adversarial, and economic edge-case
  tests;
- governance upgrades are authorized, timelocked, audited, and invariant-safe;
- bridge messages are finality-proof-checked against live validator-set
  certificates;
- formal/model evidence covers the generic SCS theorem set;
- public testnet stability and audit gates are passed;
- operators can deploy, monitor, recover, upgrade, and rotate keys using
  documented procedures.
