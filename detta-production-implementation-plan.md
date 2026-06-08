# DeTTa Production Implementation Plan

**Project name:** DeTTa  
**Meaning:** Distributed Transactional Atomspace  
**Target:** Production distributed DeFi Atomspace  
**Inputs:** `secured-contract-spaces.md`, `scs-formal.md`,
`detta-implementation-plan.md`

# 0. Implementation Progress

Updated: 2026-06-08

Completed production-plan slices:

- [x] Production implementation plan created.
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

Next implementation slices:

- [ ] Add peer-scored reconnect queues and retry metrics for long-running
  validator processes.

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

# 9. Open Production Decisions

The following decisions must be closed before M1 or M2:

- consensus protocol selection;
- networking stack;
- validator key type and signature scheme;
- canonical binary encoding;
- authenticated tree design;
- storage engine;
- RPC protocol;
- gas/resource pricing;
- evaluator implementation strategy;
- invariant language;
- bridge finality proof format;
- governance authority model;
- production deployment topology.

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
