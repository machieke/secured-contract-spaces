# DeTTa Implementation Plan

**Project name:** DeTTa
**Working meaning:** Distributed Transactional Atomspace
**Purpose:** A consensus-replicated secured Atomspace runtime for DeFi
**Inputs:** `secured-contract-spaces.md`, `scs-formal.md`
**Status:** Draft implementation plan v0.1

# 1. Objective

DeTTa is a distributed Atomspace where state-changing operations are not raw
atom additions or removals. Instead, consensus orders transactions, and a
deterministic secured contract runtime derives the next Atomspace state.

The core objective is:

```text
Build a consensus-replicated Atomspace where DeFi balances, permissions,
events, and contract state can change only through authorized, invariant-
preserving, atomic SCS transitions.
```

DeTTa should support:

- token contracts;
- AMMs;
- lending vaults;
- staking systems;
- oracle adapters;
- bridge/message adapters;
- governance-controlled upgrades;
- auditable state and event proofs.

# 2. Non-Goals for the Initial System

The initial DeTTa implementation should not attempt:

- hidden balances;
- zero-knowledge execution;
- general-purpose distributed Prolog/MeTTa mutation;
- arbitrary host-language interop from contracts;
- cross-shard synchronous writes;
- speculative parallel execution as the default path;
- support for untrusted raw `add-atom`, `remove-atom`, or raw private-state
  `match`.

Privacy can be added later through commitments, encrypted state, nullifiers, or
zero-knowledge transition proofs. The initial system assumes transparent state
to validators and authorized views to users.

# 3. Design Rule

The system must enforce this rule everywhere:

```text
Consensus accepts transactions.
The SCS runtime executes transactions.
The guarded storage kernel writes atoms.
External users and contract code do not directly mutate secured state.
```

In particular, the distributed Atomspace is not an eventually consistent CRDT
for DeFi balances. DeFi requires a finalized order of transitions.

# 4. System Architecture

```text
Clients / wallets / indexers
        |
        v
RPC and mempool
        |
        v
Consensus engine
        |
        v
Block executor
        |
        v
SCS runtime
  dispatcher
  policy engine
  capability registry
  guarded storage kernel
  restricted evaluator
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
        |
        v
State roots, registry roots, event roots, receipts
```

# 5. Node Roles

## Validator Node

Runs:

- consensus;
- mempool validation;
- block proposal;
- deterministic block execution;
- SCS runtime;
- full authenticated storage;
- state root verification;
- event and receipt generation.

Validators vote only for blocks whose execution roots match local execution.

## Full Node

Runs:

- block verification;
- deterministic replay;
- full state storage or pruned state storage;
- RPC queries;
- event indexing.

Full nodes do not vote unless configured as validators.

## Archive Node

Stores:

- all historical blocks;
- historical state snapshots;
- historical event logs;
- proofs for old state roots.

## Light Client

Verifies:

- block headers;
- consensus certificates;
- state proofs;
- event proofs;
- receipt proofs.

Light clients do not execute all transactions.

# 6. Consensus Requirements

DeTTa may use a BFT consensus protocol, a finality gadget, or another consensus
engine, but the SCS layer requires only the following abstract interface:

```text
ConsensusOutput =
  finalized ordered blocks
  consensus timestamp or logical time
  validator set / epoch data
  finality certificate
```

The consensus engine must provide:

- one canonical transaction order per finalized block;
- deterministic block metadata visible to contracts;
- a finalized block hash;
- byzantine fault assumptions documented at the chain layer;
- validator-set update rules;
- replay protection for block proposals and votes.

Contracts must not read wall-clock time, local randomness, node-local network
state, or local filesystem state. They may read only consensus-provided context.

# 7. Block Format

```text
BlockHeader =
  { chain_id              : ChainId
  , height                : UInt
  , previous_block_hash   : Hash
  , tx_root               : Hash
  , receipt_root          : Hash
  , global_state_root     : Hash
  , contract_table_root   : Hash
  , storage_root          : Hash
  , registry_root         : Hash
  , policy_root           : Hash
  , event_root            : Hash
  , nonce_root            : Hash
  , timestamp             : ConsensusTime
  , proposer              : ValidatorId
  , consensus_certificate : Certificate
  }
```

```text
Block =
  { header       : BlockHeader
  , transactions : Seq[Transaction]
  , receipts     : Seq[Receipt]
  }
```

The header roots are commitments to the post-block state.

# 8. Transaction Types

Initial state-changing transaction types:

```text
DeployContract
CallContract
GovernanceUpdate
RegisterValidator
UpdateValidatorSet
SubmitCertificate
SubmitOracleAttestation
SubmitBridgeMessage
```

Only state-changing transaction types enter consensus. Views are local queries
against a finalized state root and do not mutate state.

## CallContract

```text
CallContract =
  { chain_id    : ChainId
  , tx_hash     : TxHash
  , sender      : Principal
  , nonce       : Nonce
  , contract_id : ContractId
  , selector    : Selector
  , args        : Seq[Argument]
  , signature   : Signature
  , budget      : ResourceBudget
  }
```

# 9. State Model

The physical storage backend should be an authenticated key-value store. The
logical values are Atomspace records.

```text
GlobalState =
  { contract_table : Map[ContractId, ContractRecord]
  , policy_store   : Map[PolicyKey, MethodPolicy]
  , storage_store  : Map[StateKey, StateValue]
  , registry_store : Map[GrantKey, Grant]
  , nonce_store    : Map[Principal, NonceState]
  , event_store    : AppendOnlyEventLog
  , governance     : GovernanceState
  }
```

Every key must have canonical serialization.

```text
StateKey =
  BalanceKey(contract_id, owner, asset)
  | TotalSupplyKey(contract_id, asset)
  | ReserveKey(contract_id, asset)
  | DebtKey(contract_id, borrower, asset)
  | CustomContractKey(contract_id, schema_id, fields)
```

```text
GrantKey =
  AllowanceGrantKey(contract_id, owner, spender, asset)
  | RoleGrantKey(contract_id, role, subject)
  | OracleUpdaterKey(contract_id, subject)
  | BridgeValidatorKey(contract_id, subject)
  | GovernanceGrantKey(contract_id, right, subject)
```

# 10. Secured Atomspace Layout

A deployed contract has public descriptors and runtime-owned private stores.

```text
Public descriptor atoms:
  (Contract TokenA)
  (CodeHash TokenA Hash_abc)
  (ExportedMethod TokenA transfer)
  (ExportedMethod TokenA transferFrom)
  (ExportedView TokenA balanceOf)
  (StateRoot TokenA Root_s)
  (RegistryRoot TokenA Root_r)
  (EventRoot TokenA Root_e)
```

Runtime-owned state cells:

```text
(StateCell
  (BalanceKey TokenA Alice USDC)
  (UInt 100))

(StateCell
  (TotalSupplyKey TokenA USDC)
  (UInt 150))
```

Runtime-owned grants:

```text
(Grant
  (AllowanceGrantKey TokenA Alice Dex USDC)
  (Issuer Alice)
  (Subject Dex)
  (Rights (SpendAllowance))
  (Limit 100)
  (Spent 0)
  (Active true)
  (Revoked false))
```

Contract code never receives raw handles to these stores.

# 11. Block Execution

Block execution is a fold over transactions.

```text
ApplyBlock(state, block):
  require block.header.previous_block_hash = state.finalized_block_hash
  require consensus_certificate_valid(block.header)
  require tx_root(block.transactions) = block.header.tx_root

  working_state = state
  receipts = []

  for tx in block.transactions:
      receipt, working_state = ApplyTransaction(working_state, tx)
      receipts.append(receipt)

  require root(working_state.contract_table) = block.header.contract_table_root
  require root(working_state.storage_store) = block.header.storage_root
  require root(working_state.registry_store) = block.header.registry_root
  require root(working_state.policy_store) = block.header.policy_root
  require root(working_state.event_store) = block.header.event_root
  require root(working_state.nonce_store) = block.header.nonce_root
  require root(receipts) = block.header.receipt_root

  return finalized working_state
```

## Transaction Execution

```text
ApplyTransaction(state, tx):
  require signature_valid(tx)
  require nonce_valid(state.nonce_store, tx.sender, tx.nonce)
  mark_nonce_used_or_advanced()

  if tx.type = CallContract:
      return ExecuteSCSCallAtomically(state, tx)

  if tx.type = DeployContract:
      return ExecuteDeploymentAtomically(state, tx)

  if tx.type = GovernanceUpdate:
      return ExecuteGovernanceUpdateAtomically(state, tx)
```

For a reverted contract call, contract state, registry changes, and events revert.
Admission-level nonce semantics must be fixed and documented. The recommended
initial rule is that admitted transactions consume the envelope nonce even if the
contract call reverts.

# 12. SCS Runtime Execution

```text
ExecuteSCSCallAtomically(state, tx):
  ctx = build_context_from_consensus(state, tx)
  contract = resolve_contract(state.contract_table, tx.contract_id)
  abi_entry = resolve_selector(contract.abi, tx.selector)
  require abi_entry.exported
  require type_check(abi_entry, tx.args)

  policy = lookup_policy(state.policy_store, contract, abi_entry, tx.args)
  require policy exists

  auth = authorize(policy, ctx, state.registry_store, tx.args)
  require auth.allowed

  frame = create_authorized_frame(ctx, auth)
  staged = empty_transition()

  result = restricted_eval(contract.code_hash, frame, tx.args, staged)
  require result.success

  require invariants_hold(state, staged, policy)
  require event_policy_accepts(staged.events, policy)

  commit staged state, registry, and events together
```

# 13. Restricted Evaluator

The evaluator must expose only deterministic contract primitives:

```text
state_get
state_set
registry_get
registry_set_guarded
emit_event
call_contract
abort
pure arithmetic
pure comparison
pure data construction/destructuring
```

Forbidden by default:

```text
raw add-atom
raw remove-atom
raw match over private state
Prolog assertion/retraction
Python interop
filesystem access
process execution
network access
wall-clock access
randomness
arbitrary imports
```

# 14. Guarded Storage Backend

Recommended physical layout:

```text
storage_store:
  key = canonical_bytes(StateKey)
  value = canonical_bytes(StateValue)

registry_store:
  key = canonical_bytes(GrantKey)
  value = canonical_bytes(Grant)

policy_store:
  key = canonical_bytes(PolicyKey)
  value = canonical_bytes(MethodPolicy)
```

The backend must support:

- staged writes;
- rollback;
- atomic commit;
- deterministic iteration only where explicitly allowed;
- Merkle or Verkle proof generation;
- schema validation before write;
- root computation after commit.

# 15. Event and Receipt Model

Events are emitted into a staging buffer during execution and committed only if
the transaction commits.

```text
Receipt =
  { tx_hash          : TxHash
  , status           : Committed | Reverted | Rejected
  , gas_or_steps_used: UInt
  , return_value     : Option[ReturnValue]
  , error            : Option[Error]
  , event_root_delta : Hash
  , state_root_after : Hash
  }
```

Event atoms:

```text
(Event
  (Contract TokenA)
  (TxHash Tx123)
  (Index 0)
  (Payload (Transfer Alice Bob USDC 10)))
```

Events must not reveal private state beyond authorized event policy.

# 16. RPC and Query API

Initial RPC methods:

```text
submitTransaction(tx)
getTransaction(tx_hash)
getReceipt(tx_hash)
getBlock(height_or_hash)
getStateRoot(height)
callView(contract_id, selector, args, height)
getProof(key, height)
getEvents(filter, height_range)
getContract(contract_id)
```

`callView` executes against finalized state and must be read-only.

# 17. Implementation Modules

Recommended module boundaries:

```text
detta-core
  types, canonical serialization, hashes, errors

detta-storage
  authenticated key-value store, staging, roots, proofs

detta-runtime
  dispatcher, context, frames, storage kernel, registry kernel

detta-policy
  method policy language, policy evaluator, policy linter

detta-evaluator
  restricted MeTTa/PeTTa-compatible evaluator

detta-consensus
  consensus adapter interface and initial engine integration

detta-node
  validator/full-node process, block execution, mempool

detta-rpc
  client API, view calls, receipts, proofs

detta-indexer
  event indexing and query acceleration

detta-tests
  conformance, deterministic replay, adversarial tests
```

The recommended implementation language for the core runtime is Rust because
the system needs deterministic execution, memory safety, and strong type
boundaries. The contract frontend can remain MeTTa/PeTTa-like.

# 18. Implementation Phases

## Phase 0: Executable Model

Deliver:

- executable version of `scs-formal.md` state machine;
- in-memory storage;
- deterministic transaction runner;
- JSON or binary canonical transaction fixtures;
- golden tests for call, revert, event, and root behavior.

Acceptance:

- same fixture produces identical roots on repeated runs;
- raw state mutation is not part of the transition API;
- token transfer and transfer revert fixtures pass.

## Phase 1: Single-Node SCS Runtime

Deliver:

- contract table;
- ABI resolver;
- method policy registry;
- capability registry;
- guarded storage kernel;
- production restricted evaluator and aspect runtime;
- token contract with `transfer`, `approve`, `transferFrom`, `permit`;
- event log;
- receipt model.

Acceptance:

- unauthorized transfer fails;
- `transferFrom` requires live allowance;
- allowance consumption and balance changes are atomic;
- missing method policy denies execution;
- caller identity cannot be forged;
- views are read-only.

## Phase 2: Authenticated Storage and Proofs

Deliver:

- canonical serialization;
- storage root;
- registry root;
- policy root;
- event root;
- receipt root;
- inclusion and non-inclusion proofs.

Acceptance:

- state root is deterministic across machines;
- light-client proof can verify balance, grant, receipt, and event inclusion;
- duplicate canonical balance keys are impossible.

## Phase 3: Consensus-Replicated Node

Deliver:

- validator node process;
- mempool;
- block proposal;
- consensus adapter;
- deterministic block executor;
- validator root checks;
- state sync for full nodes.

Acceptance:

- multiple validators finalize the same block sequence;
- validators reject blocks with incorrect post-state roots;
- node replay from genesis reaches the same latest root;
- re-submitted nonce is rejected.

## Phase 4: Restricted DeTTa Evaluator

Deliver:

- MeTTa/PeTTa-compatible restricted evaluator;
- allowed primitive table;
- forbidden primitive traps;
- kernel trace emission;
- resource metering;
- deterministic arithmetic.

Acceptance:

- `add-atom`, raw private `match`, Prolog, Python, filesystem, and network
  escape hatches fail;
- every state write appears in a kernel trace;
- identical execution traces produce identical roots.

## Phase 5: DeFi Contract Suite

Deliver:

- fungible token;
- AMM pool;
- staking contract;
- lending vault;
- oracle adapter;
- bridge/message adapter skeleton.

Acceptance:

- token supply invariant holds;
- AMM reserve and LP accounting invariants hold;
- lending vault rejects undercollateralized borrow;
- stale oracle price is rejected;
- bridge message replay is rejected.

## Phase 6: Governance and Upgrades

Deliver:

- role registry;
- timelock contract;
- pause mechanism;
- guarded policy updates;
- guarded code upgrades;
- upgrade audit log.

Acceptance:

- policy changes require governance authority;
- critical changes respect timelock;
- paused methods reject state-changing calls;
- upgrade preserves declared migration invariants.

## Phase 7: Cross-Contract and Cross-Shard Messaging

Deliver:

- synchronous same-shard cross-contract calls;
- reentrancy locks;
- nested subtransactions;
- asynchronous cross-shard message queues;
- message receipts and proofs;
- replay-protected inbox/outbox contracts.

Acceptance:

- caller write scope is not inherited by callee;
- reentrancy fails unless explicitly allowed;
- cross-shard messages cannot directly mutate remote state;
- message consumption is one-time and proof-checked.

## Phase 8: Verification and Hardening

Deliver:

- TLA+ or equivalent block execution model;
- runtime safety proofs or mechanically checked proof sketches;
- symbolic execution harness for contract traces;
- policy linter;
- invariant checker;
- fuzzing and differential replay;
- adversarial test suite.

Acceptance:

- generic SCS theorems from `scs-formal.md` have corresponding tests or proofs;
- every DeFi contract declares invariants;
- invariant failures revert full transitions;
- deterministic replay is continuously tested.

# 19. Minimal DeFi MVP

The minimal DeTTa MVP should include:

- one shard;
- validator set with finalized ordered blocks;
- deterministic block executor;
- authenticated storage roots;
- SCS runtime;
- production restricted evaluator and aspect runtime;
- token contract;
- AMM pool;
- oracle adapter with authorized updater;
- RPC for transaction submission, receipts, views, and proofs;
- replay from genesis;
- adversarial tests for raw mutation, forged caller, missing policy, stale grant,
  failed invariant, and nondeterministic execution.

MVP is complete when three independent nodes can replay the same block sequence
and produce identical roots for token and AMM transactions.

# 20. Verification Strategy

Verification should run in layers.

## Model Checking

Use the formal transition model to check:

- authorization safety;
- atomicity;
- replay prevention;
- reentrancy locking;
- cross-contract write-scope isolation.

## Property Testing

Generate random transactions and assert:

- roots are deterministic;
- failed calls do not commit state, registry, or events;
- state schema always holds;
- token supply is preserved except mint/burn;
- registry spend never exceeds live grant limit.

## Symbolic Execution

For contract implementations:

- collect kernel traces;
- prove every `state_set` key is inside method write scope;
- prove all revert paths discard staged writes;
- prove postconditions for successful paths.

## Runtime Refinement

Prove or test that concrete execution refines the abstract model:

```text
concrete_trace(tx, state) accepted_by abstract_SCS_model
and
concrete_roots = abstract_roots
```

# 21. Security Gates

No release should ship without:

- deterministic replay test from genesis;
- block root mismatch rejection test;
- raw private-state mutation rejection test;
- forbidden primitive rejection test;
- method policy missing rejection test;
- caller forgery rejection test;
- allowance consumption atomicity test;
- event rollback test;
- nonce replay test;
- state proof verification test;
- consensus equivocation or invalid-block test.

# 22. Open Decisions

The following must be decided before production:

- consensus protocol and finality model;
- validator-set governance;
- canonical binary encoding;
- hash function;
- authenticated tree design;
- integer widths and overflow rules;
- gas or resource metering model;
- contract code packaging format;
- policy expression language;
- invariant expression language;
- oracle timestamp rules;
- upgrade migration rules;
- slashing or validator accountability;
- cross-shard message finality rules.

# 23. First Engineering Milestone

The first useful milestone is not a full blockchain. It is a deterministic
single-node DeTTa executor.

Build:

```text
Transaction -> SCS transition -> state root -> receipt
```

with token transfer, allowance, event rollback, and deterministic replay. Once
that is stable, consensus can replicate it. This keeps the hard security
boundary small: the consensus layer orders transactions, and the SCS executor
defines what those transactions mean.
