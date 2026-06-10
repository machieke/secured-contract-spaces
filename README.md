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
- secure restricted MeTTa aspect modules with parser, canonical roots,
  verifier, executable aspect runtime, module artifacts, and proof obligations;
- token, AMM, oracle, bridge-security, lending, staking, governance, timelock,
  upgrade, and account-registry contract flows;
- economic settlement for native token ledgers and supported aspect-token
  ledgers across AMM, lending, staking, and bridge flows;
- taxonomy-aligned programmable token bundles for ERC20-like transfer/approval,
  fees, pauses, restrictions, locks, mint/burn/caps, snapshots, vault shares,
  wrapping, rewarded staking, and bridge mint/burn adapters;
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
- `scripts/detta-verify-readiness-manifests.sh`: standalone verifier for
  public-testnet, mainnet-candidate, and audit readiness manifests plus their
  checked evidence paths.
- `scripts/detta-package-release.sh`: release artifact packaging script for
  the validator and client binaries, demo genesis, faucet sample, hashes, and
  signing manifest.
- `scripts/detta-source-state-report.sh`: release-source state reporter that
  binds packaging evidence to the current commit, tree, and dirty-worktree
  metadata.
- `scripts/detta-release-signing-drill.sh`: hermetic release signing drill
  that signs packaged checksum attestations with an ephemeral key and verifies
  them with the production verifier.
- `scripts/detta-release-candidate-evidence-bundle.sh`: retained release
  evidence runner that executes packaged operator drills and bundles their
  reports, release artifacts, audit package, signatures, and checksums.
- `scripts/detta-verify-release-candidate-evidence-bundle.sh`: standalone
  verifier for transferred retained release-candidate evidence bundles.
- `scripts/detta-genesis-finalization-drill.sh`: release-genesis drill that
  binds the generated genesis roots, release manifest, faucet sample,
  validator identities, quorum, and checksums into a finalization report.
- `scripts/detta-packaged-client-flow-drill.sh`: release-archive drill that
  boots packaged `detta-node` and drives packaged `detta-client` through token,
  liquidity, buy, and sell workflows.
- `scripts/detta-public-testnet-stability-drill.sh`: packaged local stability
  drill that runs a sustained multi-block DeFi workload and records
  health/metrics/alert evidence.
- `scripts/detta-audit-readiness-package.sh`: release-audit evidence packager
  that bundles tracked source, generated release artifacts, proof manifests,
  readiness manifests, and security findings into a deterministic archive.
- `scripts/detta-verify-audit-readiness-package.sh`: standalone verifier for
  transferred audit-readiness packages and their embedded evidence inventories.
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
- `crates/detta-node/src/bin/detta-client.rs`: packaged TCP RPC client for
  deploying tokens, creating pools, adding liquidity, swapping assets, reading
  receipts, and querying roots.
- `crates/detta-aspects`: parser, canonicalizer, verifier, IR lowering, and
  artifact tooling for secure taxonomy-aligned MeTTa aspect modules.
- `crates/detta-aspect-runtime`: deterministic executable runtime for verified
  aspect IR and guarded kernel host-call traces.
- `crates/detta-node`: persistent validator node orchestration and the
  `detta-node` operator binary.
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

For a release-candidate gate with dependency audit enforced locally:

```sh
DETTA_E2E_FULL=1 DETTA_REQUIRE_DEP_AUDIT=1 scripts/detta-release-gate.sh
```

The release gate checks formatting, clippy, workspace tests, selected or full
E2E client flows, dependency advisory and supply-chain policy through
`deny.toml`, verification crate tests, release builds, packaged-node launch,
incident-response, governance-bootstrap, validator-onboarding,
packaged-client-flow, local-stability, audit-readiness, and release-signing
drills from a temporary release directory, the standalone readiness-manifest
verifier, TLA+ model checking through `scripts/detta-model-check.sh`, and proof
artifact hash manifests.

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
does not claim readiness until the stability window, signed release
distribution, faucet/sample-client publication, and external audit gate are
completed. Recheck public-testnet, mainnet-candidate, and audit readiness
metadata with:

```sh
scripts/detta-verify-readiness-manifests.sh
```

## Operator Binary Quick Start

Build the deployable node binary:

```sh
cargo build --release --bin detta-node
```

Generate a deterministic demo DeFi genesis snapshot:

```sh
cargo run -p detta-node --bin detta-node -- \
  write-genesis --output ops/demo-genesis.json --chain-id detta-local
```

Start a persistent TCP JSON-RPC validator from that genesis:

```sh
cargo run -p detta-node --bin detta-node -- \
  serve --storage /tmp/detta-validator-1 \
  --genesis ops/demo-genesis.json \
  --validator-id validator-1 \
  --rpc 127.0.0.1:8080 \
  --transport tcp
```

Create a faucet transfer transaction that can be submitted through RPC:

```sh
cargo run -p detta-node --bin detta-node -- \
  faucet-tx --to Alice --amount 100 --nonce 1 --tx-hash faucet-alice-1
```

Package release-candidate artifacts after the hard release gate passes:

```sh
DETTA_RELEASE_VERSION=rc-1 scripts/detta-package-release.sh
```

The packaging script writes `dist/detta-release-<version>.json`, SHA-256
attestations, a deterministic archive containing `detta-node` and
`detta-client`, a demo genesis snapshot, and a sample faucet transaction.
The release manifest includes source-state metadata for the current Git commit,
tree, tracked changes, untracked files, and diff roots. For final publication,
set `DETTA_REQUIRE_CLEAN_RELEASE_SOURCE=1` so packaging and release-signature
verification fail if the source tree is dirty.
Detached signatures are still an operator release step and should use the
`*.sha256` files listed in the manifest:

```sh
for checksum in dist/*.sha256; do
  gpg --armor --output "$checksum.sig" --detach-sign "$checksum"
done
```

Verify a signed release bundle before publishing it:

```sh
DETTA_RELEASE_SIGNER_FINGERPRINT=<fingerprint> \
  scripts/detta-verify-release-signatures.sh dist/detta-release-rc-1.json
```

The verifier checks the release manifest checksum and signature, every
artifact checksum, declared source-state metadata, and every declared detached
signature before the signed artifact records are copied into readiness metadata.

Run a hermetic signing drill with a temporary GPG key before a real release
signing ceremony:

```sh
DETTA_RELEASE_VERSION=rc-1 scripts/detta-release-signing-drill.sh
```

The drill packages the release candidate, signs the manifest and artifact
checksum files with an ephemeral key, verifies the bundle through
`scripts/detta-verify-release-signatures.sh`, and writes
`dist/detta-release-signing-drill-<version>.json`. Production releases must
still be signed with the published release key.

Run the packaged client flow drill before publishing the sample client:

```sh
DETTA_RELEASE_VERSION=rc-1 scripts/detta-packaged-client-flow-drill.sh
```

The drill unpacks the release archive, boots packaged `detta-node`, runs
packaged `detta-client` through token deployment, pool deployment, liquidity,
buy, and sell commands, verifies committed receipts, and writes
`dist/detta-packaged-client-flow-drill-<version>.json`.

Run a local genesis-finalization drill before publishing launch artifacts:

```sh
DETTA_RELEASE_VERSION=rc-1 \
  DETTA_FINAL_GENESIS_VALIDATORS=validator-1,validator-2,validator-3,validator-4 \
  scripts/detta-genesis-finalization-drill.sh
```

The drill packages the release, verifies the release manifest binds the genesis
and faucet artifacts, validates authenticated genesis roots, records validator
identities and quorum, and writes
`dist/detta-genesis-finalization-<version>.json`. Production launches must
still sign and publish this report with the release key and validator-operator
acknowledgements.

Run a local packaged stability drill before starting the real public-testnet
stability window:

```sh
DETTA_RELEASE_VERSION=rc-1 DETTA_STABILITY_DRILL_BLOCKS=16 \
  scripts/detta-public-testnet-stability-drill.sh
```

The drill unpacks the release, boots packaged `detta-node`, runs packaged
`detta-client` through setup plus a sustained swap workload, verifies committed
blocks, health, metrics, mempool drain, root consistency, and alert status, and
writes `dist/detta-public-testnet-stability-drill-<version>.json`. This does
not replace the required 168-hour public testnet window.

Build an audit-readiness package for external review:

```sh
DETTA_RELEASE_VERSION=rc-1 scripts/detta-audit-readiness-package.sh
```

The package script creates the release artifacts, inventories tracked source
and generated release files with SHA-256 roots, validates the checked audit
findings manifest, binds the proof-artifact manifest, and writes
`dist/detta-audit-readiness-package-<version>.tar.gz` plus
`dist/detta-audit-readiness-<version>.json`. The audit report includes the
same source-state metadata as the release manifest and can be made fail-closed
for final reviewer handoff with `DETTA_REQUIRE_CLEAN_AUDIT_PACKAGE=1`. It also
runs the standalone package verifier. Reviewers can repeat that check after
transfer:

```sh
scripts/detta-verify-audit-readiness-package.sh \
  dist/detta-audit-readiness-package-rc-1.tar.gz
```

This is reviewer evidence; it is not a substitute for an independent audit.

Build a retained release-candidate evidence bundle after the hard gate passes:

```sh
DETTA_RELEASE_VERSION=rc-1 scripts/detta-release-candidate-evidence-bundle.sh
```

The evidence script reruns the packaged launch, genesis-finalization,
incident-response, governance-bootstrap, validator-onboarding, packaged-client,
local-stability, audit-readiness, and release-signing drills under one release
version. It writes `dist/detta-release-candidate-evidence-<version>.json`,
`dist/detta-release-candidate-evidence-<version>.jsonl`, and
`dist/detta-release-candidate-evidence-<version>.tar.gz` with SHA-256
attestations, then runs the standalone evidence-bundle verifier. Reviewers can
repeat that check after transfer:

```sh
scripts/detta-verify-release-candidate-evidence-bundle.sh \
  dist/detta-release-candidate-evidence-rc-1.tar.gz
```

This creates retained operator evidence; it does not replace the hard release
gate or the real public-testnet stability window.

Run a local operator launch rehearsal against the packaged binary:

```sh
DETTA_RELEASE_VERSION=rc-1 scripts/detta-operator-launch-rehearsal.sh
```

The rehearsal unpacks the release archive, boots the packaged `detta-node`
from the generated genesis, queries `get_state_root` over TCP RPC, verifies it
matches the genesis root, and writes `dist/detta-launch-rehearsal-<version>.json`.

Run a local incident-response drill against the packaged binary:

```sh
DETTA_RELEASE_VERSION=rc-1 scripts/detta-incident-response-drill.sh
```

The drill boots the packaged node, creates a reverted transfer, records an
expected RPC error, leaves one transaction pending, checks operator health,
metrics, alerts, and snapshot metadata roots, then writes
`dist/detta-incident-response-drill-<version>.json`.

Run a local governance bootstrap drill against the packaged binary:

```sh
DETTA_RELEASE_VERSION=rc-1 scripts/detta-governance-bootstrap-drill.sh
```

The drill schedules and rehearses a timelocked upgrade, verifies early
execution fails, executes after the timelock, then repeats the same pattern for
a method-policy update and writes
`dist/detta-governance-bootstrap-drill-<version>.json`.

Run a local validator onboarding drill against the packaged binary:

```sh
DETTA_RELEASE_VERSION=rc-1 scripts/detta-validator-onboarding-drill.sh
```

The drill boots four validator identities from the same generated genesis,
verifies their health and persistent roots, restarts each validator without
supplying genesis again, and writes
`dist/detta-validator-onboarding-drill-<version>.json`.

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

## Economic Settlement Model

Native DeFi methods are not reserve-only accounting shims. The executor routes
asset movement through a ledger adapter before committing protocol accounting:

- AMM liquidity and swaps debit/credit user and pool token balances as well as
  reserves.
- Lending collateral deposits, borrows, and liquidations move collateral and
  debt assets between users and the vault.
- Staking stake, unstake, unbond completion, penalties, and rewards update
  explicit token custody; rewards are minted as protocol emissions through the
  token ledger.
- Bridge outbound messages lock source balances before queueing the outbox
  message; inbound native redemptions mint destination balances after verified
  finality and replay checks.
- Aspect bridge burn/release projections burn bridged supply and append
  guarded outbound messages through the shared cross-shard outbox, replay-id,
  event, and proof-root path.

Aspect-token AMM integration is supported when the pool asset is the deployed
aspect-token contract and the bundle exports the guarded `ERC20-transfer`
method. Aspect vault shares, wrapping, and rewarded staking now use explicit
restricted `call-contract!` host calls to a supplied token contract and asset:
deposits, wraps, stakes, and reward funding consume the normal native token
allowance granted to the aspect contract, while redeems, unwraps, unstakes, and
reward claims transfer from aspect-contract custody. These calls are verifier
visible through the `CallContract` policy effect. Host-call argument conversion
is ABI-aware: native targets use built-in method schemas, while aspect targets
use the target bundle's checked-in `method-abi` before nested execution. Aspect
methods with `CallContract` effects must also publish `(calls ...)` allowlists,
and both module verification and runtime trace replay reject calls outside that
authenticated policy. Token settlement calls still use the native token method
policy, allowance, balance, and invariant checks.
Aspect expressions can use local `state-get` reads and explicit
`contract-state-get` reads for deterministic cross-contract aspect-state
inspection. Those reads are included in the host trace, treated as non-mutating
during invariant replay, supplied from a contract-qualified state snapshot, and
accepted only when the method policy publishes a matching `(reads ...)`
allowlist.
Privileged aspect operations that change supply or bridge replay state reject
zero amounts before emitting events, mutating supply, or consuming bridge
message IDs. Privileged aspect configuration and minting methods use the
deployment admin grant rather than public sender-only authority.

## DeFi Client Coverage

The E2E client harness is intended to exercise DeTTa as an external user would:

- deploy new tokens and AMM pools;
- submit and deploy restricted MeTTa aspect tokens;
- register account signer keys;
- submit signed transactions;
- create liquidity;
- buy and sell native and aspect-token assets through AMM methods;
- query balances, reserves, receipts, events, and proofs;
- exercise oracle, bridge, lending, staking, governance, timelock, and upgrade
  flows;
- validate rejection paths, replay protection, expiry checks, malformed RPC
  requests, and unauthorized signer behavior;
- restart nodes and verify persistence, state sync, and proof continuity.

See `detta-e2e-client-integration-test-plan.md` and `crates/detta-e2e` for the
client integration plan and implementation. The aspect-token AMM coverage is in
`crates/detta-e2e/tests/aspect_amm_client_flows.rs`; it verifies the current
AMM custody and reserve integration for an asset identifier matching a deployed
aspect-token contract.

## Safety Notice

This repository is not a security audit result and is not a production chain
deployment. The specifications, models, tests, and implementation are meant to
make DeFi execution auditable and formally tractable, but real-value deployment
would require independent audits, adversarial testnets, operational hardening,
key-management procedures, incident response, economic analysis, and governance
review.
