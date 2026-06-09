# DeTTa E2E Client Integration Test Implementation Plan

## Objective

Build a full end-to-end client integration test suite proving that an external DeTTa client can exercise the production distributed DeFi atomspace through public transports and RPC APIs.

The suite must validate full client workflows across token deployment, liquidity creation, buys and sells, oracle updates, lending, staking, bridge redemption, governance, state sync, proof verification, and operator APIs without relying on in-process state helpers after testnet genesis.

## Current Context

The implementation already contains core DeTTa primitives for deterministic execution, proof roots, validator replay, token operations, AMM operations, oracle flows, bridge security, governance, lending, staking, HTTP JSON-RPC, TCP JSON-RPC, persistent node state, finality records, metrics, alerts, and backup/restore.

The remaining E2E gap is a production-style client harness that drives those capabilities through client-facing APIs. In particular, client-visible deployment of new tokens and liquidity pools must be made explicit. If token and pool creation remain only as genesis or in-process state setup APIs, the E2E suite can validate runtime behavior but cannot prove that an external client can deploy new DeFi assets.

## Scope

The E2E suite will:

- Spawn a local multi-validator DeTTa network with real node processes, isolated data directories, validator identities, RPC ports, and consensus configuration.
- Drive all workflows through HTTP JSON-RPC and TCP JSON-RPC clients.
- Use signed client transaction envelopes wherever transaction submission supports signatures.
- Avoid direct `DeTTaState` access after genesis fixture creation.
- Verify state through RPC views, event streams, receipt queries, and proof APIs.
- Independently validate proof artifacts in the client harness.
- Cover successful paths, rejected paths, replay protection, malformed inputs, and restart/state-sync behavior.
- Run deterministically on a developer workstation and in CI without external network services.

The suite will not:

- Depend on a public testnet.
- Require browser UI automation unless a separate user-facing DeTTa wallet/client is added.
- Treat in-process unit tests as a substitute for transport-level integration tests.
- Use private implementation internals for final assertions when RPC or proof APIs are available.

## Proposed Test Package

Create a dedicated E2E package:

- `crates/detta-e2e/`
- `crates/detta-e2e/src/client.rs`
- `crates/detta-e2e/src/network.rs`
- `crates/detta-e2e/src/fixtures.rs`
- `crates/detta-e2e/src/proofs.rs`
- `crates/detta-e2e/tests/full_client_flows.rs`

If the workspace layout favors root integration tests, the same modules can live under `tests/detta_e2e/`, but a dedicated crate is preferred because the client harness, proof verifier, fixtures, and network supervisor will become reusable release-gate tooling.

## Client Harness

Implement a typed client wrapper around public DeTTa RPC methods.

Required client capabilities:

- HTTP JSON-RPC request/response handling with bounded timeouts.
- TCP JSON-RPC request/response handling with bounded timeouts.
- Transaction construction helpers for all supported contract methods.
- Account/key management for deterministic test identities.
- Nonce tracking and replay test helpers.
- Block production/finality wait helpers.
- Event subscription and paginated event query helpers.
- Proof request and local proof verification helpers.
- Snapshot, state-sync, backup, restore, and restart helpers.
- Negative RPC helpers for malformed JSON, oversized bodies, invalid params, and unauthorized requests.

The harness should expose high-level workflow helpers only where they match real client behavior. For example, `add_liquidity` may build and submit an `addLiquidity` transaction, but it must not mutate reserves directly.

## Deployment Semantics

The E2E suite needs one explicit answer for client-visible deployment.

### Option A: Public Deployment Transactions

Add client-facing deployment methods:

- `deployToken`
- `deployAmmPool`
- Optional `deployLendingMarket`
- Optional `deployStakingVault`

These should be ordinary signed transactions or governance-approved deployment transactions, produce receipts/events, update registry roots, and expose contract descriptors through RPC.

This option is required if DeTTa product requirements include arbitrary users or authorized issuers deploying new DeFi assets.

### Option B: Governed Genesis Fixtures

Keep deployment as controlled genesis/testnet fixture setup and document that production token/pool creation is governed or operator controlled.

Under this option, the E2E suite may create initial token and AMM contracts during testnet genesis, then test all runtime client operations through RPC.

This option is acceptable only if external client deployment is intentionally out of scope for production.

### Acceptance Decision

Before marking the E2E client suite complete, choose Option A or Option B and encode that decision in the tests. A hidden in-process deployment call inside an E2E test is not acceptable because it makes the client capability ambiguous.

## Test Network Topology

The default E2E topology should run:

- Four validators.
- One read-only RPC client endpoint if supported.
- Independent persistent storage directories for each node.
- Deterministic validator keys and client accounts.
- Short block times and bounded finality timeouts.
- Local-only ports allocated dynamically to avoid collisions.
- Configurable consensus quorum for positive and negative tests.

The network supervisor must support:

- Start all nodes.
- Stop one node.
- Restart one node from disk.
- Restart all nodes from disk.
- Isolate a validator if the networking layer supports it.
- Import snapshots into a fresh node.
- Collect node logs on failure.
- Clean temporary directories unless `DETTA_E2E_KEEP_TMP=1` is set.

## Functional Coverage

### 1. Client Bootstrap And RPC Discovery

Validate:

- `get_node_health`
- `get_operator_metrics`
- `get_operator_alerts`
- `get_mempool_status`
- RPC method availability from generated OpenAPI or method registry.
- HTTP and TCP transports return consistent responses for shared methods.
- Unknown method and invalid parameter errors are stable and typed.

Acceptance criteria:

- The client can discover a healthy network and identify the current chain, head, finalized height, validator set, and supported RPC surface.
- Every public RPC method is called at least once by the E2E suite, either as a positive test or an intentional negative/unsupported-path test.

### 2. Token Lifecycle

Validate:

- Token deployment through the chosen deployment model.
- Contract descriptor lookup.
- Total supply query.
- Balance query.
- `transfer`.
- `approve`.
- `transferFrom`.
- `permit`.
- Allowance decrement.
- Insufficient balance rejection.
- Insufficient allowance rejection.
- Bad signature rejection.
- Wrong-chain or wrong-domain permit rejection.
- Nonce replay rejection.
- Receipt, event, and storage proofs for token state changes.

Acceptance criteria:

- A client can create or obtain a token, move balances between accounts, delegate allowance, execute delegated transfer, and verify final balances only through RPC/proof APIs.
- All token failure modes return deterministic errors and do not change state roots.

### 3. AMM Liquidity And Buy/Sell Flows

Validate:

- AMM pool deployment through the chosen deployment model.
- Liquidity creation with `addLiquidity`.
- LP share accounting.
- Swap token A for token B.
- Swap token B for token A.
- Buy flow represented as exact-input or exact-output swap according to supported AMM semantics.
- Sell flow represented as the reverse swap direction.
- Slippage rejection.
- Expired deadline rejection if deadlines are supported.
- Insufficient reserves rejection.
- Fee accounting.
- Reserve query and proof verification.
- Event ordering for liquidity and swap events.

Acceptance criteria:

- A client can create liquidity and execute both buy and sell paths without direct state access.
- The independently calculated constant-product or configured AMM invariant matches post-swap reserves and fees.

### 4. Oracle Flows

Validate:

- Authorized `submitPrice`.
- Unauthorized price update rejection.
- Stale price rejection.
- Invalid round or timestamp rejection.
- Oracle-fed DeFi transaction success when price is fresh.
- Oracle-fed DeFi transaction rejection when price is stale.
- Oracle event and storage proofs.

Acceptance criteria:

- A client can publish valid oracle prices through RPC and all dependent DeFi contracts enforce freshness and authorization.

### 5. Lending Flows

Validate:

- Market descriptor lookup.
- Collateral deposit.
- Borrow.
- Repay.
- Withdraw.
- Interest accrual if implemented.
- Liquidation when collateralization falls below threshold.
- Healthy-account liquidation rejection.
- Stale oracle borrow rejection.
- Insufficient collateral rejection.
- Bad debt or reserve accounting edge case if supported.

Acceptance criteria:

- A client can complete the full lending lifecycle and verify collateral, debt, liquidation, and reserve state through RPC/proofs.

### 6. Staking Flows

Validate:

- Stake.
- Reward accrual.
- Claim rewards.
- Request unstake.
- Early unstake completion rejection.
- Complete unstake after delay.
- Penalty path if configured.
- Vault accounting proofs.

Acceptance criteria:

- A client can complete staking, rewards, and unstaking flows with deterministic timing and proof-backed assertions.

### 7. Governance, Timelocks, And Upgrades

Validate:

- Governance proposal or schedule transaction.
- Timelock delay enforcement.
- Non-admin rejection.
- Execute scheduled policy update.
- Execute scheduled code upgrade using a test upgrade artifact.
- Upgrade rehearsal report before execution.
- Pause and unpause behavior if supported.
- Rejected contract calls while paused.
- Post-upgrade state root continuity.

Acceptance criteria:

- Governance actions can be scheduled, proven pending, executed after delay, and rejected before delay or without authority.
- Upgrade and policy changes are visible through RPC, events, registry proofs, and rehearsal reports.

### 8. Bridge And Cross-Shard Security

Validate:

- Queue bridge message.
- Fetch outbox entry.
- Generate or fetch outbox proof.
- Fetch finality certificate.
- Client-side finality and proof verification.
- Redeem bridge message.
- Double redeem rejection.
- Tampered proof rejection.
- Lowered quorum rejection.
- Untrusted signer rejection.
- Wrong destination or wrong domain rejection.

Acceptance criteria:

- A client can bridge a message using only public proof and finality data, and all replay/tamper cases fail without state changes.

### 9. Router And Cross-Contract Calls

Validate:

- Routed transfer or swap.
- Explicit allowance requirements.
- No unintended allowance inheritance across contracts.
- Reentrancy guard rejection for crafted nested call if supported.
- Atomic rollback across multi-step route failure.
- Event and receipt linkage across routed calls.

Acceptance criteria:

- Cross-contract workflows preserve contract isolation, authorization boundaries, and atomicity.

### 10. Consensus, Mempool, And Finality

Validate:

- Submit transactions to one validator and observe mempool propagation.
- Produce blocks from pending transactions.
- Finalize blocks with quorum.
- Fetch and verify finality certificates.
- Fetch block and transaction history.
- Fetch paginated receipts/events.
- Subscribe to blocks/events if subscriptions are supported.
- Duplicate transaction rejection.
- Invalid transaction rejection before inclusion.
- Validator replay consistency across all nodes.

Acceptance criteria:

- All validators converge on identical finalized roots for the same client-submitted workload.
- The client can prove finality without private validator state.

### 11. State Sync, Restart, Backup, And Restore

Validate:

- Stop and restart one validator from disk.
- Stop and restart the full network from disk.
- Verify persisted finalized height, mempool state, finality records, slashing records, and event history.
- Export snapshot metadata.
- Import snapshot into a fresh node.
- Verify metadata root and restored state root.
- Run backup.
- Restore from backup into a fresh data directory.
- Verify restored RPC responses match pre-backup responses.

Acceptance criteria:

- A client can detect state-sync progress and verify restored nodes against proof roots and metadata roots.

### 12. Proof Verification

Validate client-side verification for:

- Storage inclusion.
- Storage non-inclusion where supported.
- Registry inclusion.
- Receipt inclusion.
- Event inclusion.
- Outbox inclusion.
- Snapshot metadata roots.
- Finality certificates.

Acceptance criteria:

- The E2E harness independently verifies all proof artifacts and fails tests if the RPC server returns internally inconsistent proof data.

### 13. Restricted MeTTa/PeTTa Evaluator

Validate:

- Accepted deterministic evaluator fixture.
- Rejected forbidden primitive.
- Rejected unbounded recursion or resource exhaustion.
- Rejected nondeterministic operation.
- Arithmetic overflow or type violation failure.
- Proof trace or evaluation receipt exposure if supported.

Acceptance criteria:

- Production restricted evaluator behavior is exercised through client-visible contract or evaluator APIs and all safety boundaries are enforced.

### 14. Operations, Metrics, And Alerts

Validate:

- Operator metrics reflect block production, transaction execution, RPC calls, reverts, and consensus status.
- Alerts are returned for induced root mismatch, peer isolation, excess reverts, disk pressure, backup failure, RPC overload, or slashing evidence when those triggers are available in local mode.
- Alert severity and labels are stable.
- Health status degrades and recovers under controlled failure conditions.

Acceptance criteria:

- Operators can observe the E2E network state through public operator APIs without inspecting process internals.

### 15. Adversarial RPC And Network Inputs

Validate:

- Malformed JSON-RPC request.
- Unknown method.
- Missing params.
- Wrong param type.
- Oversized request body.
- Batch request behavior if supported.
- TCP frame truncation or corrupt envelope.
- Unauthorized admin/operator call if authentication is enabled.
- Rate-limit behavior if configured.

Acceptance criteria:

- Bad client inputs produce bounded, typed failures and do not crash nodes or corrupt persistent state.

## Traceability Matrix

| Area | Primary E2E suite | Required result |
| --- | --- | --- |
| Token | `token_lifecycle_client_flow` | Deploy or obtain token, transfer, approve, permit, verify proofs |
| AMM | `amm_liquidity_buy_sell_client_flow` | Create liquidity, buy, sell, validate invariant |
| Oracle | `oracle_client_flow` | Authorized update succeeds, stale/unauthorized updates fail |
| Lending | `lending_client_flow` | Deposit, borrow, repay, withdraw, liquidate |
| Staking | `staking_client_flow` | Stake, claim, unstake delay, complete unstake |
| Governance | `governance_upgrade_client_flow` | Schedule, timelock, execute, prove registry changes |
| Bridge | `bridge_redeem_client_flow` | Queue, prove, redeem, reject replay/tamper |
| Router | `router_cross_contract_client_flow` | Atomic routed success and rollback failure |
| Consensus | `multi_validator_finality_client_flow` | Mempool propagation, block finality, root convergence |
| State sync | `state_sync_restart_backup_client_flow` | Restart, snapshot import, backup restore |
| Proofs | `client_proof_verification_flow` | Independent verification for all proof types |
| Evaluator | `restricted_evaluator_client_flow` | Safe programs run, unsafe programs fail |
| Operations | `operator_observability_client_flow` | Metrics and alerts reflect induced conditions |
| RPC hardening | `adversarial_rpc_client_flow` | Invalid inputs fail safely |

## Implementation Milestones

### Milestone 1: E2E Crate And Network Harness

Deliver:

- New E2E crate or test module.
- Test network supervisor.
- Temporary directory and dynamic port allocation.
- HTTP and TCP client wrappers.
- Health, metrics, alerts, and mempool smoke tests.

Acceptance criteria:

- `cargo test -p detta-e2e -- --test-threads=1` starts a local network and verifies basic RPC health over both transports.

### Milestone 2: Typed Transactions And Client Signing

Deliver:

- Deterministic test accounts and keys.
- Transaction builder helpers for every supported contract method.
- Nonce management.
- Signature/domain validation tests.
- Receipt polling and finality wait helpers.

Acceptance criteria:

- The client can submit signed transactions, wait for finality, fetch receipts, and detect replay or bad-signature failures.

### Milestone 3: Deployment Path

Deliver one of:

- Public deployment transactions for token and AMM pool creation.
- Explicit governed genesis fixture setup documented as the production deployment policy.

Acceptance criteria:

- The test suite proves the chosen deployment model and does not hide deployment behind private state mutation during runtime flows.

### Milestone 4: Token And AMM Workflows

Deliver:

- Token lifecycle E2E test.
- AMM liquidity E2E test.
- Buy and sell E2E tests.
- Proof-backed balance, allowance, reserve, and event assertions.

Acceptance criteria:

- A client can deploy or obtain tokens, create liquidity, buy, sell, and verify all resulting state through public APIs.

### Milestone 5: Oracle, Lending, And Staking

Deliver:

- Oracle update E2E test.
- Lending lifecycle E2E test.
- Staking lifecycle E2E test.
- Freshness, collateralization, reward, and delay failure tests.

Acceptance criteria:

- All core DeFi protocols are exercised through client transactions with both committed and reverted paths.

### Milestone 6: Governance, Timelocks, And Upgrades

Deliver:

- Governance schedule/execute E2E test.
- Timelock negative tests.
- Policy update test.
- Test upgrade artifact and rehearsal verification.

Acceptance criteria:

- Governance and upgrade controls are validated end to end with proof-backed registry and event assertions.

### Milestone 7: Bridge, Router, And Proofs

Deliver:

- Bridge queue/redeem E2E test.
- Tamper/replay/quorum negative tests.
- Router atomicity E2E test.
- Shared client-side proof verifier coverage.

Acceptance criteria:

- Cross-domain and cross-contract workflows are proven safe through independent client verification.

### Milestone 8: Consensus, Restart, State Sync, Backup, Restore

Deliver:

- Multi-validator mempool and finality E2E test.
- Validator restart test.
- Full network restart test.
- Snapshot import test.
- Backup/restore test.

Acceptance criteria:

- Persistent distributed behavior is validated across restarts and fresh-node sync.

### Milestone 9: Evaluator, Operations, And Adversarial Inputs

Deliver:

- Restricted evaluator E2E tests.
- Operator metrics and alerts E2E tests.
- Malformed RPC and corrupt TCP frame tests.

Acceptance criteria:

- Production safety boundaries and operator observability are covered through client-visible behavior.

### Milestone 10: Release Gate Integration

Deliver:

- Add a stable default E2E subset to `scripts/detta-release-gate.sh`.
- Add a longer opt-in profile for restart, backup, and adversarial tests.
- Document commands and environment variables.
- Store failure logs and node data when requested.

Acceptance criteria:

- The release gate runs deterministic E2E client coverage in CI.
- Developers can run the full suite locally with one command.

## Suggested Commands

Default local run:

```sh
cargo test -p detta-e2e -- --test-threads=1
```

Full local run including restart, state sync, backup, and adversarial tests:

```sh
DETTA_E2E_FULL=1 cargo test -p detta-e2e -- --test-threads=1
```

Keep node data for debugging:

```sh
DETTA_E2E_KEEP_TMP=1 DETTA_E2E_FULL=1 cargo test -p detta-e2e -- --test-threads=1
```

Release gate target:

```sh
scripts/detta-release-gate.sh
```

## Acceptance Criteria For The Full E2E Plan

- Every public RPC method is called at least once.
- Every DeFi contract method has at least one committed-path E2E test.
- Every DeFi contract method has at least one rejected-path E2E test where rejection is meaningful.
- Token deployment and AMM pool deployment are either client-visible workflows or explicitly governed genesis workflows.
- Liquidity creation, buy, and sell flows are driven by an external client.
- All assertions use RPC responses, receipts, events, or proofs unless the assertion is about test harness setup.
- All returned proof types are independently verified by the client harness.
- A four-validator network converges on the same finalized state root after the complete workload.
- Restarted and state-synced nodes return the same client-visible state as existing validators.
- Backup and restore produce a node that matches the verified state root and metadata roots.
- Governance timelocks, upgrades, and policy changes are covered by positive and negative tests.
- Restricted evaluator safety boundaries are covered through client-visible APIs.
- Metrics and alerts are checked under normal and induced-failure conditions.
- Tests allocate ports dynamically and do not require external services.
- The default suite is deterministic and bounded in runtime.
- The full suite can be run with one documented command.

## Risks And Open Design Questions

- Public deployment semantics for tokens and AMM pools must be resolved before claiming that clients can deploy new assets.
- If current transaction signatures are still represented by test-only flags, production signing must be completed before E2E signing tests are meaningful.
- Multi-process networking tests can become flaky unless timeouts, retries, and log capture are designed carefully.
- The suite must distinguish AMM buy/sell terminology from exact-input swap direction to avoid ambiguous assertions.
- Some operator alerts may require test-only fault injection hooks; those hooks must not weaken production behavior.
- Full backup, restore, and adversarial tests may be too slow for the default release gate and may need an opt-in profile.

## Definition Of Done

This plan is complete when DeTTa has a client-driven E2E suite that can stand up a distributed local network, deploy or obtain DeFi contracts according to the chosen production policy, perform all DeFi workflows, verify all proofs independently, exercise consensus/finality/state-sync behavior, validate governance and upgrades, check operator observability, and run from a documented command without external dependencies.
