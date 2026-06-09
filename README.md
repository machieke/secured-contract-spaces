# Secured Contract Spaces and DeTTa

This repository contains two related efforts:

1. **Secured Contract Spaces for MeTTa DeFi**: a runtime security and formal
   verification specification for DeFi contracts written for MeTTa/PeTTa-like
   Atomspace systems.
2. **DeTTa**: a Rust implementation of a consensus-replicated secured
   Atomspace runtime for DeFi.

DeTTa stands for **Distributed Transactional Atomspace**. It is the execution,
consensus, storage, RPC, and verification workspace for implementing the
Secured Contract Spaces model in a distributed setting.

Status: this is a production-track research and implementation repository. It
contains substantial runtime, networking, consensus, RPC, E2E, and verification
work, but it is not audited, not a public mainnet, and must not be used to
secure real funds.

## Secured Contract Spaces for MeTTa DeFi

`secured-contract-spaces.md` defines the Secured Contract Spaces (SCS) model.
An SCS is a contract-owned logical space with private state atoms, exported
methods, policy-controlled entrypoints, guarded storage, event emission,
capability registries, and invariant checks.

The core rule is simple:

```text
External callers do not mutate contract state directly.
External callers invoke methods.
The runtime authorizes the method call.
Only the runtime grants scoped write authority.
The transition commits atomically only if all invariants hold.
```

SCS is designed to close the gap between ordinary programmable spaces and a
DeFi-grade security boundary. In particular, DeFi safety cannot depend on
contract-level convention alone. The runtime must enforce:

- dispatcher-only mutation;
- deny-by-default method authorization;
- runtime-derived caller identity;
- live capability-registry lookup;
- method-scoped write authority;
- guarded storage writes;
- atomic commit or rollback;
- deterministic execution;
- reentrancy controls;
- restricted MeTTa/PeTTa evaluation;
- canonical roots, receipts, and proofs;
- signed certificate adapters for permits, oracle attestations, orders, and
  bridge messages.

The formal companion in `scs-formal.md` turns the prose specification into an
abstract state machine, safety properties, and implementation proof
obligations. The intended verification path is to prove generic SCS runtime
theorems once, then instantiate them for contracts such as tokens, AMMs,
lending vaults, staking systems, bridges, or oracle adapters.

## DeTTa

DeTTa is the distributed runtime built around the SCS model. It treats DeFi
contracts as secured Atomspace instances and replicates deterministic state
transitions through validator consensus.

At a high level:

```text
clients and operators
        |
        v
HTTP/TCP JSON-RPC
        |
        v
DeTTa validator node
        |
        +-- signed client transaction admission
        +-- mempool persistence and gossip
        +-- validator networking and consensus certificates
        +-- deterministic SCS executor
        +-- restricted evaluator
        +-- durable storage, snapshots, and state sync
        +-- receipts, events, roots, and proof APIs
```

The current implementation includes slices for:

- deterministic secured contract execution;
- state roots, receipts, event roots, and proof reports;
- signed client transactions and account signer registration;
- persistent mempool admission and replay protection;
- validator protocol envelopes and Ed25519-signed consensus messages;
- TCP validator networking, proposal and vote propagation, and finality
  certificate assembly;
- snapshot manifests, chunked state sync, metadata roots, and sync diagnostics;
- HTTP and TCP JSON-RPC surfaces;
- restricted evaluator fixtures and proof artifact hashing;
- token, AMM, oracle, bridge-security, lending, staking, governance, timelock,
  upgrade, and account-registry contract flows;
- E2E client tests covering deployment, liquidity, buys and sells, proofs,
  RPC coverage, adversarial cases, state sync, networking, and operator APIs.

The production roadmap and progress tracker live in
`detta-production-implementation-plan.md`.

## How The Two Layers Fit

SCS is the security and verification contract. It says what a secured DeFi
Atomspace runtime must enforce.

DeTTa is the distributed implementation effort. It provides the Rust crates,
protocol messages, storage model, RPC APIs, E2E harness, and formal artifacts
needed to make the SCS model executable and consensus replicated.

The relationship is:

```text
SCS specification
    -> formal model and proof obligations
    -> deterministic DeTTa executor
    -> replicated DeTTa validator network
    -> client-facing DeFi RPC workflows
    -> proofs, receipts, snapshots, and audit artifacts
```

## Workspace Layout

- `secured-contract-spaces.md`: SCS runtime security specification.
- `scs-formal.md`: formal verification companion for SCS.
- `detta-production-implementation-plan.md`: production DeTTa implementation
  plan and progress tracker.
- `detta-secure-metta-aspect-generalization-plan.md`: implementation plan for
  replacing hard-coded DeFi behavior with secure, restricted MeTTa aspect
  modules aligned with the token aspect taxonomy.
- `detta-aspect-language-subset.md`: accepted and forbidden forms for the
  restricted MeTTa aspect language used by DeTTa programmable modules.
- `detta-e2e-client-integration-test-plan.md`: E2E client integration plan.
- `detta-client-token-liquidity-guide.md`: end-user client guide for deploying
  a token, creating a liquidity pool, adding liquidity, and selling tokens.
- `detta-client-aspect-token-guide.md`: end-user client guide for submitting a
  verified MeTTa aspect module, inspecting its artifacts, deploying an
  aspect-backed token, and transferring it.
- `detta-rpc-api.md`: human-readable DeTTa RPC API documentation.
- `detta-rpc-openapi.json`: machine-readable RPC API schema.
- `detta-restricted-evaluator-subset.md`: restricted evaluator subset.
- `models/`: TLA+ model, proof artifact manifests, and evaluator proof traces.
- `ops/detta-public-testnet-readiness.json`: public-testnet readiness status
  and blocker manifest validated by `detta-verify`.
- `ops/detta-mainnet-candidate-readiness.json`: mainnet-candidate readiness
  status and release-signing blocker manifest validated by `detta-verify`.
- `security/detta-audit-findings.json`: release-candidate audit finding
  tracker validated by `detta-verify`.
- `scripts/detta-release-gate.sh`: release-gate verification script.
- `docs/`: LaTeX rendering of the SCS specification.

Rust workspace crates:

- `crates/detta-core`: deterministic SCS executor, contract state, roots,
  proofs, and DeFi contract methods.
- `crates/detta-consensus`: consensus, finality, validator-set, and slashing
  logic.
- `crates/detta-protocol`: versioned protocol envelopes, signatures, and
  snapshot sync wire types.
- `crates/detta-network`: validator transport, peer handshakes, retries, and
  TCP protocol streams.
- `crates/detta-storage`: durable blocks, mempool records, snapshots, metadata,
  audit records, and sync diagnostics.
- `crates/detta-aspects`: parser, canonicalizer, and source-root tooling for
  secure taxonomy-aligned MeTTa aspect modules.
- `crates/detta-node`: persistent validator node orchestration.
- `crates/detta-rpc`: HTTP/TCP JSON-RPC server and client-facing wire types.
- `crates/detta-evaluator`: restricted MeTTa/PeTTa-style evaluator subset.
- `crates/detta-verify`: replay, differential, symbolic, and proof artifact
  tooling.
- `crates/detta-e2e`: external client harness and integration tests.

## Verification And Test Commands

For the normal development gate:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test
```

For the DeTTa release gate:

```sh
DETTA_E2E_FULL=0 scripts/detta-release-gate.sh
```

For the full E2E profile:

```sh
DETTA_E2E_FULL=1 scripts/detta-release-gate.sh
```

The release gate checks formatting, clippy, workspace tests, selected or full
E2E client flows, dependency advisory and supply-chain policy through
`deny.toml`, verification crate tests, release builds, TLA+ model checking through
`scripts/detta-model-check.sh`, and proof artifact hash manifests.

Local dependency audits are run by `scripts/detta-dependency-audit.sh`. If
`cargo-deny` is unavailable locally, the script prints a warning and lets the
developer gate continue; set `DETTA_REQUIRE_DEP_AUDIT=1` to make that a hard
failure. CI always installs and runs `cargo-deny`.

Audit finding closure is represented by
`security/detta-audit-findings.json`. `detta-verify` fails if that manifest has
open or in-remediation findings, duplicate finding IDs, missing closure
evidence for closed findings, or accepted-risk findings without a rationale.

Public testnet readiness is represented by
`ops/detta-public-testnet-readiness.json`. The checked-in status intentionally
does not claim readiness until the stability window, packaging/genesis/faucet
work, and external audit gate are completed.

Mainnet-candidate readiness is represented by
`ops/detta-mainnet-candidate-readiness.json`. The checked-in status
intentionally does not claim readiness until public testnet, external audit,
finalized genesis, validator onboarding, governance bootstrapping, launch
rehearsal, incident-response drill, and release signing gates are complete.

## Formal Verification Surface

Formal verification in this repository is organized around refinement:

1. `secured-contract-spaces.md` defines the required runtime behavior.
2. `scs-formal.md` defines the abstract state machine and theorem obligations.
3. `models/DeTTaBlockExecution.tla` models block execution properties.
4. `crates/detta-core` implements deterministic transitions and roots.
5. `crates/detta-verify` checks replay, differential behavior, symbolic
   invariants, and proof artifacts.
6. E2E tests validate that external clients reach those behaviors through
   public RPC and network transports.

This is intended to support both runtime testing and proof-oriented review:
implementation traces can be compared against the abstract model, contract
invariants can be checked around committed transitions, and canonical roots can
bind receipts, events, snapshots, and audit metadata.

## DeFi Client Coverage

The E2E client harness is intended to exercise DeTTa as an external user would:

- deploy new tokens and AMM pools;
- register account signer keys;
- submit signed transactions;
- create liquidity;
- buy and sell through AMM methods;
- query balances, reserves, receipts, events, and proofs;
- exercise oracle, bridge, lending, staking, governance, timelock, and upgrade
  flows;
- validate rejection paths, replay protection, expiry checks, malformed RPC
  requests, and unauthorized signer behavior;
- restart nodes and verify persistence, state sync, and proof continuity.

See `detta-e2e-client-integration-test-plan.md` and `crates/detta-e2e` for the
client integration plan and implementation.

## Safety Notice

This repository is not a security audit result and is not a production chain
deployment. The specifications, models, tests, and implementation are meant to
make DeFi execution auditable and formally tractable, but real-value deployment
would require independent audits, adversarial testnets, operational hardening,
key-management procedures, incident response, economic analysis, and governance
review.
