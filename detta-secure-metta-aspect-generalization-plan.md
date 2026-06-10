# DeTTa Secure MeTTa Aspect Generalization Plan

**Project:** DeTTa  
**Goal:** Replace hard-coded DeFi behavior with secure, deployable,
taxonomy-aligned MeTTa aspect bundles.  
**Primary inputs:**

- `secured-contract-spaces.md`
- `scs-formal.md`
- `detta-restricted-evaluator-subset.md`
- `detta-production-implementation-plan.md`
- `../MeTTa-DeFi-Token-Aspect-Taxonomy/MeTTa-DeFi-Token-Aspect-Taxonomy.md`

## 0. Problem Statement

DeTTa currently proves that a Secured Contract Space style runtime can execute
and replicate DeFi-like state transitions deterministically. However, most
DeFi behavior is encoded directly in Rust:

- contract classes are represented by `ContractKind` variants;
- callable behavior is represented by fixed `Method` variants;
- policies and invariants are constructed by Rust helper functions;
- executor dispatch is a Rust `match` over contract kind and method;
- token, AMM, oracle, bridge, lending, staking, governance, and account
  registry semantics are implemented as native Rust methods.

That is useful as a secure baseline, but it does not yet realize the original
Secured Contract Spaces vision: DeFi behavior should be describable by
restricted MeTTa/PeTTa contract modules, including the aspect and bundle model
from the MeTTa DeFi Token Aspect Taxonomy.

The target architecture is:

```text
taxonomy MeTTa source
    -> canonical aspect package
    -> typed aspect IR
    -> static policy/effect/invariant analysis
    -> proof-carrying deployed module
    -> deterministic restricted evaluator
    -> guarded SCS kernel
    -> consensus-replicated state transition
```

Rust remains the trusted kernel. DeFi business behavior moves into restricted,
auditable, canonical MeTTa aspect modules.

## 1. Design Principle

DeTTa should not generalize by making the runtime execute arbitrary MeTTa.
That would recreate the same raw Atomspace mutation risks that SCS was designed
to avoid.

The secure generalization is a two-layer system:

```text
Trusted Rust kernel:
  consensus, signatures, mempool, metering, storage roots, guarded storage,
  capability registry, state proofs, restricted evaluator host, canonical
  hashing, invariant runner, and deployment admission.

Untrusted but checked MeTTa modules:
  aspect declarations, storage schemas, actions, projections, method bodies,
  preconditions, postconditions, hooks, constraints, and invariants.
```

The Rust kernel enforces:

- no raw state mutation;
- no raw registry mutation;
- no raw event emission;
- no raw cross-contract or cross-shard effects;
- method-scoped write authority;
- effect typing;
- deterministic arithmetic;
- resource limits;
- atomic commit or revert;
- invariant validation before commit;
- canonical source and IR roots;
- deployment-time and upgrade-time verification.

The MeTTa module describes:

- which aspects are included;
- what storage the aspects own;
- what actions they provide;
- what external API projections they expose;
- what method bodies execute;
- what authority each method requires;
- what effects each method can perform;
- what invariants must hold.

## 2. Target User Story

An end user or protocol author should be able to define a token behavior as a
bundle of semantic aspects:

```metta
(bundle MyFeeToken)
(bundle-includes MyFeeToken StaticBalanceAspect)
(bundle-includes MyFeeToken TransferableBalanceAspect)
(bundle-includes MyFeeToken ApprovalAspect)
(bundle-includes MyFeeToken SpendAllowanceAspect)
(bundle-includes MyFeeToken FeeTransferAspect)
(bundle-includes MyFeeToken ObservableTransferAspect)

(projection MyFeeToken ERC20-transfer
  (= (ERC20.transfer $to $amount)
     (transferFromSelf tx.sender $to $amount)))
```

Deployment should produce a secured contract whose behavior is defined by the
bundle and projections, not by a native Rust `Token` branch.

The runtime should reject the deployment if:

- the aspect set is incomplete;
- an aspect conflict is present;
- a projection targets an unknown action;
- a method lacks policy;
- a write targets storage not owned by the contract or aspect;
- an effect is undeclared;
- an invariant is missing, unexecutable, or unsupported;
- the code uses forbidden primitives;
- static analysis cannot bound execution;
- canonical roots do not match submitted artifacts.

## 3. Non-Goals

This plan does not attempt to:

- support arbitrary Hyperon/MeTTa execution;
- expose raw Atomspace mutation to contracts;
- support arbitrary imports at execution time;
- support Python, Prolog, filesystem, process, or network access;
- support nondeterministic wall-clock or randomness primitives;
- replace the consensus/runtime kernel with MeTTa;
- make unverified user contracts mainnet-safe immediately;
- implement zero-knowledge proofs for private balances.

## 4. Current Hard-Coded Surface To Generalize

The existing native runtime should be treated as the reference implementation
for the first standard library modules.

Current hard-coded concepts to replace or wrap:

- `ContractKind::Token`
- `ContractKind::AmmPool`
- `ContractKind::LendingVault`
- `ContractKind::Staking`
- `ContractKind::Router`
- token transfer/approve/transferFrom/permit methods
- AMM add-liquidity/swap methods
- lending deposit/borrow/liquidate methods
- staking stake/unstake/unbond/reward methods
- Rust-built method policy manifests
- Rust-built declared invariant sets
- Rust executor dispatch by native contract kind

Current native concepts to keep as trusted kernel facilities:

- transaction validation;
- nonces and replay protection;
- signature verification;
- account signer registry;
- guarded state and registry storage;
- event root and receipt root generation;
- proof APIs;
- consensus/finality;
- state sync;
- deterministic serialization;
- release gates;
- static verifier and proof artifact validation.

## 5. Architecture Overview

```text
                 +------------------------------------+
                 | MeTTa aspect source package        |
                 | taxonomy, aspects, bundle, methods |
                 +------------------+-----------------+
                                    |
                                    v
                 +------------------------------------+
                 | detta-aspect parser/canonicalizer  |
                 | restricted grammar, source root     |
                 +------------------+-----------------+
                                    |
                                    v
                 +------------------------------------+
                 | typed aspect IR                    |
                 | aspects, storage, actions, effects |
                 +------------------+-----------------+
                                    |
                                    v
                 +------------------------------------+
                 | static verifier                    |
                 | type, effect, policy, invariant    |
                 +------------------+-----------------+
                                    |
                                    v
                 +------------------------------------+
                 | deployable module artifact          |
                 | IR root, proof roots, ABI, schema   |
                 +------------------+-----------------+
                                    |
                                    v
             +----------------------+----------------------+
             |                                             |
             v                                             v
 +---------------------------+               +---------------------------+
 | restricted evaluator      |               | SCS guarded kernel        |
 | deterministic action code |-------------->| storage/registry/events   |
 +---------------------------+               +---------------------------+
             |                                             |
             +----------------------+----------------------+
                                    |
                                    v
                 +------------------------------------+
                 | atomic receipt, roots, proofs      |
                 +------------------------------------+
```

## 6. New Workspace Components

### 6.1 `crates/detta-aspects`

New crate responsible for:

- parsing the accepted taxonomy-aligned MeTTa source subset;
- canonicalizing source;
- validating declarations;
- lowering source to typed aspect IR;
- computing source, IR, ABI, policy, invariant, and module roots;
- producing deterministic JSON fixtures.

Primary public API:

```rust
pub fn parse_aspect_package(source: &str) -> Result<AspectPackageAst, AspectError>;
pub fn canonical_aspect_source(ast: &AspectPackageAst) -> String;
pub fn lower_to_ir(ast: &AspectPackageAst) -> Result<AspectModuleIr, AspectError>;
pub fn verify_module(ir: &AspectModuleIr) -> Result<VerifiedAspectModule, AspectError>;
pub fn module_artifact(module: &VerifiedAspectModule) -> AspectModuleArtifact;
```

### 6.2 `crates/detta-aspect-stdlib`

New crate or `models/aspects/stdlib/` package containing standard aspect
definitions:

- `AccountAspect`
- `ErrorAspect`
- `EventAspect`
- `NamedSymbolAspect`
- `StaticBalanceAspect`
- `TransferableBalanceAspect`
- `SelfTransferAspect`
- `ApprovalAspect`
- `SpendAllowanceAspect`
- `DelegatedTransferAspect`
- `PermitApprovalAspect`
- `FeeTransferAspect`
- `PausableTransferAspect`
- `RestrictedTransferAspect`
- `MintableBalanceAspect`
- `BurnableBalanceAspect`
- `ObservableTransferAspect`
- `ObservableMintAspect`
- `ObservableBurnAspect`
- `VotableBalanceAspect`
- `SnapshotBalanceAspect`
- `WrappedBalanceAspect`
- `VaultShareBalanceAspect`
- `StakeBalanceAspect`
- `RewardedStakeBalanceAspect`

The first production-grade target should be a standard-library equivalent of
the current native token contract:

```text
ERC20ConformantToken =
  NamedSymbolAspect
  + StaticBalanceAspect
  + TransferableBalanceAspect
  + SelfTransferAspect
  + ApprovalAspect
  + SpendAllowanceAspect
  + DelegatedTransferAspect
  + PermitApprovalAspect
  + ObservableTransferAspect
  + ObservableApprovalAspect
```

### 6.3 `crates/detta-evaluator`

Extend the current restricted evaluator from a proof-trace fixture evaluator
into a real executable action evaluator for verified aspect modules.

Required capabilities:

- function/action invocation by canonical symbol;
- lexical argument binding;
- deterministic expression evaluation;
- checked arithmetic;
- boolean conditions;
- `require`;
- guarded `state-get`;
- guarded `state-set!`;
- guarded `registry-get`;
- guarded `registry-set!`;
- guarded `registry-consume!`;
- guarded `emit!`;
- guarded `call-contract!`;
- deterministic trace emission;
- meter charging per instruction and per host kernel call;
- static maximum stack depth or no recursion.

### 6.4 `crates/detta-core`

Generalize contract records and execution:

- add a programmable module contract descriptor;
- store module artifacts or references in state;
- dispatch programmable methods through the verified aspect evaluator;
- keep native contracts as precompiles during migration;
- add a compatibility path to compare native and aspect-defined behavior.

Potential direction:

```rust
pub enum ContractRuntime {
    Native(NativeContractKind),
    AspectModule {
        module_hash: String,
        bundle_id: String,
        abi_root: String,
        policy_root: String,
        invariant_root: String,
    },
}
```

Eventually, `ContractKind` should stop being the primary semantic dispatcher.
It can remain as metadata or as native precompile identifiers.

### 6.5 `crates/detta-verify`

Add verification tooling for:

- aspect module artifact validation;
- standard-library fixture root checks;
- native-vs-aspect differential tests;
- symbolic write-scope analysis;
- invariant coverage checks;
- proof obligation manifests;
- taxonomy conformance checks.

### 6.6 RPC and Client

Extend RPC with module-aware deployment and inspection:

- `submit_aspect_module`
- `deploy_aspect_contract`
- `get_aspect_module`
- `get_aspect_module_artifact`
- `get_aspect_contract_schema`
- `get_aspect_contract_abi`
- `get_aspect_contract_policy`
- `get_aspect_contract_invariants`
- `evaluate_aspect_view`

Existing transaction submission should continue to call methods normally once
a contract is deployed.

## 7. Accepted MeTTa Source Subset

The deployable aspect language should be a taxonomy-aligned subset, not full
MeTTa.

### 7.1 Declaration Forms

Accepted top-level declarations:

```metta
(: Symbol Type)
(aspect AspectId)
(abstract-aspect AspectId)
(bundle BundleId)
(layer-of AspectId LayerId)
(extends AspectId ParentAspectId)
(conflicts AspectId OtherAspectId)
(owns AspectId StateId TypeId)
(registry-owns AspectId RegistryId TypeId)
(provides AspectId FacetId)
(requires AspectId FacetId)
(action AspectId ActionId)
(derived AspectId ActionId Expr)
(local-invariant AspectId InvariantId Expr)
(bundle-includes BundleId AspectId)
(bundle-extends BundleId ParentBundleId)
(bundle-constraint BundleId ConstraintId Expr)
(projection BundleId ProjectionId Expr)
(method-abi BundleId ProjectionId ArgsExpr ReturnTypeExpr)
(method-policy BundleId ProjectionId AuthorityExpr EffectsExpr InvariantsExpr)
(cross-constraint ConstraintId Expr)
```

### 7.2 Executable Expression Forms

Accepted executable forms for action and projection bodies:

```metta
(let $name Expr Expr)
(if Condition ThenExpr ElseExpr)
(require Condition ErrorCode)
(and Expr Expr)
(or Expr Expr)
(not Expr)
(= Expr Expr)
(< Expr Expr)
(<= Expr Expr)
(> Expr Expr)
(>= Expr Expr)
(safe-add Expr Expr)
(safe-sub Expr Expr)
(safe-mul Expr Expr)
(safe-div Expr Expr)
(state-get StateRef)
(state-set! StateRef Expr)
(registry-get RegistryRef)
(registry-set! RegistryRef Expr)
(registry-consume! RegistryRef Expr)
(emit! EventExpr)
(call-contract! ContractId MethodId Args)
(tx-sender)
(msg-sender)
(current-contract)
(block-height)
(block-timestamp)
```

### 7.3 Forbidden Forms

Always forbidden in deployable modules:

- `add-atom`
- `remove-atom`
- `get-atoms`
- unrestricted `match` over guarded state;
- `import!`
- `py-call`
- `callPredicate`
- `assertzPredicate`
- `assertaPredicate`
- `retractPredicate`
- `translatePredicate`
- translator rule mutation;
- filesystem access;
- process execution;
- network access;
- wall-clock access;
- unmetered randomness;
- reflection over private state;
- unbounded recursion;
- unbounded loops;
- dynamic imports;
- dynamic method construction for mutable calls.

## 8. Typed Aspect IR

Lower accepted MeTTa to a canonical IR before deployment. Consensus execution
must use the canonical IR, not ad hoc source interpretation.

### 8.1 Core IR Sketch

```rust
pub struct AspectModuleIr {
    pub module_id: String,
    pub source_root: String,
    pub taxonomy_version: String,
    pub aspects: BTreeMap<AspectId, AspectDef>,
    pub bundles: BTreeMap<BundleId, BundleDef>,
    pub projections: BTreeMap<ProjectionId, ProjectionDef>,
    pub actions: BTreeMap<ActionId, ActionDef>,
    pub storage_schema: BTreeMap<StateId, StateSchema>,
    pub event_schema: BTreeMap<EventId, EventSchema>,
    pub policies: BTreeMap<MethodId, MethodPolicyIr>,
    pub invariants: BTreeMap<InvariantId, InvariantDef>,
}

pub struct ActionDef {
    pub action_id: ActionId,
    pub args: Vec<TypedArg>,
    pub return_type: TypeId,
    pub body: ExprIr,
    pub declared_effects: BTreeSet<EffectKind>,
    pub write_scope: BTreeSet<StatePattern>,
    pub required_authority: AuthorityExpr,
    pub required_invariants: BTreeSet<InvariantId>,
}
```

### 8.2 Type System

Required primitive types:

- `Bool`
- `UInt8`
- `UInt64`
- `Amount`
- `Address`
- `Asset`
- `Contract`
- `Method`
- `Role`
- `Bytes`
- `String`
- `Time`
- `Height`
- `Event`
- `StateRef<T>`
- `RegistryRef<T>`
- `Option<T>`

Rules:

- no implicit numeric narrowing;
- all arithmetic on `Amount` and unsigned integers is checked;
- account, asset, contract, method, and role identifiers are distinct types;
- state references must resolve to schema-owned keys;
- events must match declared event schema;
- method arguments must match ABI schema.

### 8.3 Effect System

Each action and projection must statically declare effects:

- `ReadState`
- `WriteState`
- `ReadRegistry`
- `WriteRegistry`
- `ConsumeRegistryGrant`
- `EmitEvent`
- `CallContract`
- `DeployContract`
- `ScheduleUpgrade`
- `ExecuteUpgrade`
- `CrossShardOutboxAppend`
- `Abort`

The verifier rejects:

- undeclared effects;
- effects not permitted by method policy;
- storage writes outside the action write scope;
- registry writes outside the registry authority scope;
- cross-contract calls without declared call targets or interface constraints;
- call graphs with forbidden reentrancy.

### 8.4 Authority Model

Method authority should be expressible in IR, but evaluated by the kernel:

```text
TxSender
MsgSender
RoleGrant(role, account)
Allowance(owner, spender, asset)
PermitCertificate(domain)
OracleUpdaterGrant(asset)
BridgeCertificate(source_chain)
GovernanceAdminGrant(contract)
CustomRegistryGrant(schema, key)
```

MeTTa code may request authority through declarative policy expressions, but
only the kernel can read, grant, consume, or enforce authority.

## 9. Deployment Artifact

A deployable module artifact should be fully deterministic:

```json
{
  "schema": "detta.aspect-module-artifact.v1",
  "module_id": "ERC20ConformantToken",
  "taxonomy_version": "NormalizedBalanceFirst.v1",
  "source_root": "...",
  "canonical_source": "...",
  "ir_root": "...",
  "abi_root": "...",
  "policy_root": "...",
  "storage_schema_root": "...",
  "registry_schema_root": "...",
  "event_schema_root": "...",
  "invariant_root": "...",
  "proof_obligation_root": "...",
  "verifier_version": "detta-aspects-0",
  "accepted_language": "detta-aspect-metta.v1"
}
```

Deployment transaction flow:

```text
SubmitAspectModule
  -> decode artifact
  -> verify source root
  -> parse canonical source
  -> lower to IR
  -> recompute roots
  -> static verify
  -> store module by module_hash

DeployAspectContract
  -> reference stored module_hash
  -> provide constructor args
  -> initialize owned state
  -> register exported methods
  -> register method policies
  -> register invariant set
  -> emit deployment event
```

## 10. Execution Model

### 10.1 Method Call Path

```text
transaction
  -> signature and nonce validation
  -> contract lookup
  -> method/projection lookup
  -> policy lookup
  -> authority verification by kernel
  -> evaluator context creation
  -> execute canonical action IR
  -> guarded kernel calls for effects
  -> invariant checks
  -> atomic commit or revert
  -> receipt, event root, storage root
```

### 10.2 Evaluator Context

The evaluator receives a read-only context:

- chain id;
- block height;
- block timestamp;
- transaction sender;
- current message sender;
- current contract id;
- method id;
- typed method args;
- remaining budget;
- allowed effects;
- allowed write scope;
- declared invariant set.

It does not receive direct access to raw state maps, registry maps, event logs,
network sockets, host files, system time, or random sources.

### 10.3 Guarded Host Calls

All stateful operations go through the kernel:

```text
state-get(key)
  -> key must match contract schema or public view rule

state-set!(key, value)
  -> effect must be declared
  -> method policy must permit storage write
  -> key must be inside write scope
  -> value must match schema
  -> write is staged, not committed

registry-consume!(key, amount)
  -> effect must be declared
  -> authority policy must permit consumption
  -> grant must be live
  -> consumption is staged, not committed

emit!(event)
  -> effect must be declared
  -> event must match schema
  -> event is staged, not committed
```

### 10.4 Commit Rules

The transaction commits only if:

- evaluator returns success;
- all staged writes are schema-valid;
- all registry changes are policy-valid;
- all emitted events are schema-valid;
- all declared method invariants pass;
- all contract-level invariants pass when touched state requires them;
- block resource limits are not exceeded.

Any failure reverts all staged writes, registry changes, events, outbox
messages, and nested call effects.

## 11. Aspect Composition Semantics

### 11.1 Aspect Inclusion

For each bundle:

- included aspects are closed over `extends`;
- required facets must be provided by included aspects;
- conflicting aspects cannot coexist;
- abstract aspects cannot be included as concrete providers unless satisfied
  by concrete descendants;
- every owned state key must have one owning aspect;
- shared state must be modeled as explicit shared resource state.

### 11.2 Action Composition

Actions are composed from:

- base action bodies;
- preconditions from `around-requires`;
- transformations from `around-transform`;
- post-hooks from `after`;
- event hooks;
- cross-aspect constraints.

The compiler should lower composition into a canonical action graph:

```text
projection method
  -> before requirements
  -> authority checks
  -> composed action body
  -> after hooks
  -> invariant checks
```

Ordering must be deterministic. Where taxonomy declarations do not impose a
total order, the compiler must either:

- derive a stable order from explicit dependency edges; or
- reject the bundle as ambiguous.

### 11.3 Example: Fee Transfer

`FeeTransferAspect` should compile into a transform on `transferBalance`:

```text
gross debit from sender
net credit to receiver
fee credit to recipient
assert gross = net + fee
emit events according to bundle policy
```

The kernel must verify:

- `feeBps <= BPS`;
- fee recipient is a valid account when fees are enabled;
- writes touch only balance keys owned by the contract;
- total balance is preserved for fee transfers;
- events match schema.

The result is programmable behavior with runtime-enforced conservation.

## 12. Standard Library Migration Strategy

### 12.1 Phase A: Native Baseline Freezing

Before replacing hard-coded behavior:

- freeze native token semantics with golden tests;
- export native policy manifests as JSON fixtures;
- export native invariant fixtures;
- export native transaction trace fixtures;
- export native receipt/event/storage-root fixtures.

### 12.2 Phase B: Aspect Standard Library

Implement MeTTa aspect modules equivalent to native contracts:

1. ERC20-like token bundle;
2. fee token bundle;
3. pausable token bundle;
4. restricted transfer token bundle;
5. capped mintable token bundle;
6. burnable token bundle;
7. staking balance bundle;
8. vault share bundle.

The first equivalence target is the existing native token:

```text
native Token transfer/approve/transferFrom/permit
  == aspect-defined ERC20ConformantToken behavior
```

### 12.3 Phase C: Differential Runtime

Run both implementations in tests:

```text
same genesis
same transaction sequence
native contract result
aspect contract result
compare receipts, balances, allowances, events, roots
```

Required differential coverage:

- successful transfer;
- insufficient balance transfer;
- approve;
- transferFrom;
- allowance underflow;
- permit success;
- permit replay;
- paused transfer rejection;
- fee transfer;
- restricted account rejection;
- mint cap rejection;
- burn success;
- event emission order;
- invariant failure rollback.

### 12.4 Phase D: Precompile Compatibility

After equivalence is stable:

- keep native contracts as optional precompiles;
- require precompiles to publish the same ABI, policy, invariant, and proof
  roots as their aspect modules;
- treat precompiles as optimized implementations, not separate semantics.

### 12.5 Phase E: Programmable Default

New deployments should use aspect modules by default. Native paths remain only
for genesis system contracts or explicitly audited precompiles.

## 13. Formal Verification Plan

### 13.1 Abstract Theorems

Extend `scs-formal.md` with programmable module concepts:

- module artifact;
- aspect-owned state schema;
- action IR;
- effect typing;
- write-scope theorem;
- projection-to-action refinement;
- aspect composition soundness;
- invariant preservation by checked modules.

Core theorem target:

```text
If a module passes static verification, and every evaluator host call is
mediated by the guarded kernel, then no method can mutate state outside its
declared write scope, bypass policy authority, or commit with violated
declared invariants.
```

### 13.2 Proof Obligations Per Module

Each deployed module produces proof obligations:

- all method policies exist;
- all effects are declared;
- all writes are in scope;
- all registry operations are authorized;
- all action calls terminate within bounded resources;
- all arithmetic is checked;
- all local invariants are executable or proved;
- all cross-aspect constraints are discharged;
- all projection methods route through declared actions;
- all public methods preserve required global invariants.

### 13.3 Machine-Checked Artifacts

Add checked artifacts under `models/`:

- `models/aspects/stdlib/*.metta`
- `models/aspects/stdlib/*.artifact.json`
- `models/aspects/stdlib/*.proof-obligations.json`
- `models/aspects/stdlib/*.trace.json`
- `models/aspects/stdlib/*.sha256`
- TLA+ or equivalent model extensions for programmable dispatch;
- symbolic verifier reports for write scopes and invariant coverage.

### 13.4 Verification Crate Gates

`detta-verify` should fail if:

- standard-library source roots drift;
- artifact roots drift;
- proof obligation files are missing;
- an invariant lacks a checker or proof reference;
- a method lacks policy;
- a module has an undeclared effect;
- a native precompile claims equivalence without differential evidence.

## 14. Testing Plan

### 14.1 Parser and Canonicalization

- parse every accepted taxonomy declaration form;
- reject forbidden forms;
- reject unknown top-level forms;
- canonicalize whitespace and atom ordering where allowed;
- round-trip AST to canonical source;
- root fixtures remain stable.

### 14.2 Type and Effect Checker

- reject unknown aspects;
- reject unresolved requirements;
- reject conflicts;
- reject duplicate state ownership;
- reject undeclared writes;
- reject wrong value types;
- reject missing method policy;
- reject undeclared effects;
- reject ambiguous hook ordering;
- reject unbounded recursion or loops.

### 14.3 Evaluator

- deterministic execution for the same IR/context;
- budget exhaustion reverts staged effects;
- arithmetic overflow reverts staged effects;
- forbidden primitive rejection before host effects;
- trace roots stable;
- nested calls cannot inherit caller write scope;
- read-only views do not alter roots.

### 14.4 Runtime Integration

- deploy aspect module;
- deploy aspect contract instance;
- call projection methods;
- verify receipts;
- verify storage proofs;
- verify events;
- verify rollback;
- verify state sync with aspect contracts;
- verify validator replay determinism;
- verify cross-node consensus roots.

### 14.5 Differential Tests

For native-token equivalence:

- native token and aspect token start from equivalent state;
- generated transaction sequences are applied to both;
- final balances, allowances, total supply, events, receipts, and roots match
  where roots are expected to match;
- semantic deltas are explicitly documented where storage encoding differs.

### 14.6 Adversarial Tests

- malicious module tries raw `add-atom`;
- malicious module writes another contract's balance;
- malicious module emits undeclared event;
- malicious module consumes unauthorized allowance;
- malicious module calls itself reentrantly;
- malicious module causes step exhaustion after staged write;
- malicious module uses ambiguous aspect hook ordering;
- malicious module hides supply inflation in after hook;
- malicious module declares invariant but omits checker;
- malicious module tampers with canonical source root.

## 15. RPC and Client Workflow

### 15.1 Submit Module

```json
{
  "method": "submit_aspect_module",
  "params": {
    "artifact": {
      "schema": "detta.aspect-module-artifact.v1",
      "module_id": "ERC20ConformantToken",
      "canonical_source": "...",
      "source_root": "...",
      "ir_root": "...",
      "abi_root": "...",
      "policy_root": "...",
      "invariant_root": "..."
    }
  }
}
```

### 15.2 Deploy Contract

```json
{
  "method": "submit_transaction",
  "params": {
    "transaction": {
      "target": "AspectFactoryA",
      "method": "DeployAspectContract",
      "args": [
        {"Text": "MyToken"},
        {"Text": "ERC20ConformantToken"},
        {"Text": "<module-hash>"},
        {"Text": "<constructor-args-root>"}
      ]
    }
  }
}
```

### 15.3 Call Contract

After deployment, clients call methods normally:

```json
{
  "method": "submit_signed_transaction",
  "params": {
    "signed": {
      "transaction": {
        "target": "MyToken",
        "method": "Other",
        "args": [
          {"Text": "ERC20.transfer"},
          {"Principal": "Bob"},
          {"Amount": 10}
        ]
      },
      "public_key_hex": "...",
      "signature_hex": "..."
    }
  }
}
```

Long term, the RPC wire model should support string method IDs directly so
aspect-defined projections do not have to be tunneled through `Method::Other`.

## 16. Security Gates

An aspect module cannot be admitted unless all gates pass:

- `canonical_source_root_matches`
- `ir_root_matches`
- `grammar_allowed`
- `forbidden_primitives_absent`
- `types_resolved`
- `aspect_dependencies_closed`
- `aspect_conflicts_absent`
- `storage_ownership_unique`
- `projection_targets_exist`
- `method_policies_complete`
- `authority_expressions_kernel_supported`
- `effects_declared`
- `write_scopes_sound`
- `registry_scopes_sound`
- `call_graph_bounded`
- `reentrancy_policy_sound`
- `invariants_registered`
- `invariant_checkers_present`
- `constructor_initializes_required_state`
- `resource_bounds_known`
- `proof_obligations_recorded`

Release gates should fail on missing or bypassed checks.

## 17. Governance and Upgrades

Programmable aspect modules need stricter upgrade rules than native code:

- module submission stores immutable module artifacts by hash;
- contract instances reference immutable module hashes;
- upgrades create a new module hash and schedule migration;
- migrations are themselves restricted aspect modules or audited native
  migration precompiles;
- governance timelocks apply to module changes;
- upgrade rehearsal runs the new module on forked state;
- invariant comparison must pass before execution;
- client and indexer APIs expose current and scheduled module hashes.

Upgrade acceptance criteria:

- old module hash remains auditable;
- new module artifact is retrievable;
- migration has proof obligations;
- rehearsal report lists changed ABI, storage schema, policy, and invariants;
- storage schema migration is deterministic and reversible in rehearsal;
- emergency pause can prevent upgrade execution.

## 18. Storage and State Root Compatibility

Aspect-defined contracts need canonical storage layout:

```text
state_key =
  contract_id
  + aspect_id
  + state_id
  + typed_key_arguments
```

Example:

```text
Balance(MyToken, StaticBalanceAspect, owner=Alice, asset=CLT)
Allowance(MyToken, ApprovalAspect, owner=Alice, spender=Bob, asset=CLT)
```

Rules:

- storage keys are derived from schema, not arbitrary strings;
- every state key belongs to exactly one aspect or explicit shared resource;
- schema roots are part of the module artifact;
- schema changes require upgrade/migration;
- proofs include schema metadata so clients can verify meaning, not just bytes.

## 19. Invariant Strategy

Invariant declarations from taxonomy modules must become executable or proved
runtime obligations.

Classes:

- local executable invariant: checked directly against staged state;
- bounded symbolic invariant: checked by verifier over action IR;
- cross-aspect invariant: checked over composed action graph;
- audit-only theorem: requires external proof artifact before release;
- unsupported invariant: deployment rejected.

Examples:

```text
StaticBalanceAspect:
  totalSupply equals sum of balances

TransferableBalanceAspect:
  transferBalance preserves totalBalance

FeeTransferAspect:
  sender debit equals receiver net credit plus fee

ApprovalAspect:
  spendAllowance cannot underflow

DelegatedTransferAspect:
  allowance is consumed before balance movement
```

The first production version should support executable and bounded symbolic
invariants only. Audit-only theorem references are acceptable for research
artifacts, but not for admitting unaudited mainnet modules.

## 20. Phased Implementation Plan

### Phase 0: Specification Alignment

- [x] Add this plan.
- [x] Add an SCS appendix for programmable aspect modules.
- [x] Add a restricted aspect language spec.
- [x] Mark current native DeFi contracts as baseline precompiles.
- [x] Define taxonomy version pinning.

Progress note: the plan itself is checked in, `secured-contract-spaces.md`
now has an appendix for programmable aspect modules, and
`detta-aspect-language-subset.md` documents accepted source forms, forbidden
full-MeTTa behavior, admission gates, native precompiles as baselines, and
taxonomy version pinning.

Acceptance criteria:

- docs clearly distinguish kernel enforcement from programmable behavior;
- forbidden full-MeTTa behavior is listed;
- module admission gates are documented.

### Phase 1: Aspect Parser and Canonicalizer

- [x] Create `crates/detta-aspects`.
- [x] Parse accepted declaration forms.
- [x] Reject forbidden top-level and executable forms.
- [x] Render canonical source.
- [x] Compute source roots.
- [x] Add fixtures for selected taxonomy snippets.

Acceptance criteria:

- parser accepts initial standard-library token aspects;
- parser rejects raw mutation and host escape forms;
- canonical source roots are stable under whitespace changes;
- `cargo test -p detta-aspects` passes.

### Phase 2: Typed IR and Static Verification

- [x] Define `AspectModuleIr`.
- [x] Lower AST to IR.
- [x] Implement type resolution.
- [x] Implement aspect dependency closure.
- [x] Implement conflict checking.
- [x] Implement storage ownership checking.
- [x] Implement projection target checking.
- [x] Implement method policy completeness checking.
- [x] Emit module artifacts.

Acceptance criteria:

- valid ERC20-like bundle lowers to IR;
- invalid conflicts are rejected;
- duplicate state ownership is rejected;
- every exported projection has policy and ABI entries;
- artifact roots are deterministic.

### Phase 3: Effect and Authority Checker

- [x] Add effect inference.
- [x] Add declared-vs-inferred effect comparison.
- [x] Add write-scope checker.
- [x] Add registry-scope checker.
- [x] Add authority expression support.
- [x] Add call graph and reentrancy checker.

Progress note: the first checker slice parses typed authority/effect policy
metadata, rejects undeclared inferred state and registry effects, verifies
state writes against bundle-owned state, verifies registry reads, writes, and
consumes against bundle-owned registry schema, and rejects cyclic action
graphs.

Acceptance criteria:

- undeclared state writes fail verification;
- undeclared registry consumption fails verification;
- unauthorized cross-contract calls fail verification;
- reentrant call graphs fail unless explicitly supported and proven safe;
- verifier emits proof obligation reports.

### Phase 4: Executable Restricted Aspect Evaluator

- [x] Extend evaluator expression AST.
- [x] Add lexical bindings and typed arguments.
- [x] Add deterministic conditionals.
- [x] Add `require`.
- [x] Add checked arithmetic.
- [x] Add guarded state/registry/event host calls.
- [x] Add deterministic trace roots.
- [x] Add meter charging.

Progress note: the first executable evaluator slice runs verified aspect action
expressions into deterministic host-call traces with lexical argument binding,
checked arithmetic, boolean conditions, `require`, step metering, stack-depth
limits, and trace roots. Aspect state writes, events, permit verification,
bridge verification, and restricted token-settlement `call-contract!` traces
are now wired into the SCS guarded storage kernel and block executor. The
aspect action evaluator has been split into a core-independent
`detta-aspect-runtime` crate so the block executor can integrate it without
depending on the higher-level proof/evaluator crate.

Acceptance criteria:

- evaluator can execute simple transfer action IR;
- staged writes revert on failure;
- forbidden primitives never reach host calls;
- trace fixtures are stable;
- evaluator tests cover overflow and step exhaustion.

### Phase 5: Programmable Contract Runtime

- [x] Add module registry state.
- [x] Add programmable contract descriptor.
- [x] Add module submission path.
- [x] Add aspect contract deployment path.
- [x] Dispatch programmable methods through evaluator.
- [x] Integrate invariant checks.
- [x] Add receipt/proof support for programmable modules.

Progress note: the first runtime slice stores authenticated aspect module
records by deterministic module hash, includes them in the global state root,
admits modules through a consensus-replayed factory transaction, exposes
module inspection over RPC, and deploys fail-closed programmable contract
descriptors that reference a registered module hash and bundle id. The first
dispatch slice admits source-verified modules, stores canonical executable IR,
exports `Method::Other(projection)` methods from bundle projections, executes
projection IR through `detta-aspect-runtime`, and applies supported `state-set!`
and `emit!` host calls through the guarded kernel. The invariant slice resolves
method-policy invariant references against the deployed bundle closure, exposes
aspect-local invariant obligations in the method policy manifest, evaluates
required invariant expressions after programmable host writes, and fails closed
on missing, ambiguous, false, non-boolean, or mutating invariant checks.
The proof slice adds authenticated aspect-module Merkle proofs, standard-library
artifact and proof-obligation manifests, THM-016 formal coverage, and existing
receipt, event, and storage proofs for programmable method calls and
aspect-owned state. Registry-backed host calls, cross-contract calls, and richer
state-backed invariant reads are future host-capability extensions outside the
current token-aspect acceptance slice.

Acceptance criteria:

- a verified aspect module can be submitted;
- an aspect contract can be deployed;
- an aspect-defined transfer can commit;
- invalid aspect deployment reverts atomically;
- validator replay is deterministic.

### Phase 6: Token Standard Library Equivalence

- [x] Implement static balance aspects.
- [x] Implement transferable balance aspects.
- [x] Implement approval and delegated transfer aspects.
- [x] Implement permit approval aspect or a kernel adapter for permits.
- [x] Implement observable transfer/approval events.
- [x] Define `ERC20ConformantToken` bundle.
- [x] Differential-test against native token.

Progress note: the minimal standard-library transfer fixture is now executable.
It uses keyed `balanceOf` and `allowanceOf` state reads and writes, guarded by
bundle-scoped aspect storage authorization, and a core differential test
verifies transfer, approve, transferFrom, permit, permit replay rejection, and
permit-authorized transferFrom state transitions for the `ERC20ConformantToken`
bundle against the native token baseline. Transfer and approval actions emit
concrete aspect-contract event payloads through the guarded kernel event path.
Factory aspect deployment can invoke a verified MeTTa initializer projection,
and the differential ERC20 test now initializes supply through
`ERC20-initialize` instead of direct storage seeding. The differential test
also normalizes native `Transfer`/`Approval` events and aspect
`(Transfer ...)`/`(Approval ...)` event payloads into shared ERC20 semantics and
compares them directly.

Acceptance criteria:

- native token and aspect token match on transfer, approve, transferFrom, and
  permit flows;
- receipts and events match semantically;
- all token invariants are checked;
- no token behavior requires native Rust branches except trusted adapters.

### Phase 7: Extended Token Aspects

- [x] Add fee transfer aspect.
- [x] Add pausable transfer aspect.
- [x] Add restricted transfer aspect.
- [x] Add locked transfer aspect.
- [x] Add mintable and burnable aspects.
- [x] Add capped mintable aspect.
- [x] Add observable mint/burn aspects.

Progress note: `FeeTransferAspect` is now part of the executable token
standard-library fixture. A `FeeToken` bundle deploys through the aspect module
factory, initializes supply through `ERC20-initialize`, configures fee basis
points through `Fee-setConfig`, and executes `ERC20-transfer` through a
MeTTa-defined atomic debit/credit action that emits concrete fee and net
transfer events. The current fixture uses a standard-library `FeeTreasury`
atom for the recipient while aspect persistent state is still numeric-only.
`PausableTransferAspect` and the `PausableToken` bundle are also executable:
the bundle initializes through the same MeTTa initializer, transfers while
unpaused, stores pause state through `Pause-setPaused`, and rejects
`ERC20-transfer` while paused with no balance mutation.
`RestrictedTransferAspect` and the `RestrictedToken` bundle store per-address
blocked flags through `Restriction-setBlocked`, allow unblocked transfers, and
reject transfers involving blocked accounts before balance mutation.
`LockedTransferAspect` and the `LockedToken` bundle store per-address unlock
heights and reject transfers before the deterministic block-height context
reaches the configured unlock height.
`MintableBalanceAspect`, `BurnableBalanceAspect`, and `CappedMintableAspect`
now maintain `totalSupply`, update balances with checked arithmetic, emit
zero-address mint/burn transfer events, and reject cap overflow through a
MeTTa-defined `Mint-mint` projection.

Acceptance criteria:

- fee token deploys from MeTTa aspect bundle;
- pausable token rejects transfer while paused;
- restricted token rejects blocked accounts;
- capped mint rejects cap overflow;
- burnable token reduces supply;
- all behavior is defined by aspect modules.

### Phase 8: DeFi Composability Aspects

- [x] Add votable balance aspect.
- [x] Add snapshot balance aspect.
- [x] Add wrapped balance aspect.
- [x] Add vault share balance aspect.
- [x] Add stake and rewarded stake balance aspects.
- [x] Define bridge mint/burn aspect with certificate adapter.

Progress note: `VotableBalanceAspect` and `VotableToken` expose
`Votes-getVotes` as a read-only projection over aspect-owned balances.
`SnapshotBalanceAspect` and `SnapshotToken` can capture keyed historical
balance and supply snapshots and read them back after later transfers.
`VaultShareBalanceAspect` and `VaultShareToken` support checked
deposit/redeem share math, return minted/redeemed amounts, maintain reserve,
share supply, and share balances, and settle supplied native token custody
through restricted host calls.
`WrappedBalanceAspect` and `WrappedToken` support 1:1 wrap, transfer, and
unwrap accounting over aspect-owned balances, supply, and reserve state while
moving supplied native token custody through restricted host calls.
`StakeBalanceAspect`, `RewardedStakeBalanceAspect`, and `RewardedStakeToken`
support stake, unstake, deterministic block-height reward accrual, and reward
claim flows through MeTTa projections. Stake, unstake, reward funding, and
reward claims settle supplied native token custody through the token
`transferFrom`/`transfer` host-call adapter, and reward claims require a
prefunded aspect reward reserve.
`BridgeMintBurnAspect` and `BridgeMintBurnToken` use a `bridge-verify!` host
adapter for kernel-verified inbound bridge certificates, store consumed bridge
message ids in aspect-owned replay state, mint through MeTTa-defined balance
logic, and burn bridged supply locally. Outbound cross-shard outbox emission for
burn/release flows is a future kernel host-capability extension beyond the
current inbound mint and local burn acceptance slice.

Acceptance criteria:

- vault share bundle supports deposit/redeem with checked share math;
- staking bundle supports stake/unstake/reward flows;
- bridge mint/burn requires kernel-verified bridge certificate;
- composed bundles produce complete policy and invariant manifests.

### Phase 9: RPC, Client, and Documentation

- [x] Document aspect module RPC.
- [x] Add OpenAPI schema updates.
- [x] Add client guide for deploying an aspect token.
- [x] Add E2E client tests for submit/deploy/call/query.
- [x] Add module artifact inspection RPCs.

Progress note: RPC now exposes `get_aspect_module_artifacts`, a read-only
artifact report with authenticated roots plus bundle IDs, ABI, method policy,
storage schema, registry schema, and invariant maps for source-backed verified
modules. `detta-rpc-openapi.json` includes the method tag, `detta-rpc-api.md`
documents the aspect module workflow and proof expectations, and
`detta-client-aspect-token-guide.md` gives an end-user flow for submitting the
MeTTa source artifact, inspecting it, deploying an `ERC20ConformantToken`
aspect contract, transferring tokens, and verifying storage proofs. The
`aspect_module_client_flows` E2E test exercises submit, module listing, module
proof, artifact inspection, deployment, projected transfer, event query, and
aspect-state proof verification over the TCP client.

Acceptance criteria:

- client can submit aspect module artifact;
- client can deploy aspect token;
- client can transfer aspect token;
- client can query ABI, policy, schema, and invariants;
- docs explain production signing and proof verification.

### Phase 10: Formal and Release Gates

- [x] Extend TLA+/formal model for programmable dispatch.
- [x] Add proof obligation manifests.
- [x] Add `detta-verify` module artifact validation.
- [x] Add release-gate checks for aspect fixtures.
- [x] Add audit finding categories for module verifier.

Progress note: `models/DeTTaBlockExecution.tla` now models verified module
artifacts, aspect contract bindings, verified module admission, aspect-owned
key sets, and `ProgrammableModuleSoundness`. The checked-in
`minimal-transfer-token.artifact.json` records the standard-library module
roots and IR counts, and `minimal-transfer-token.proof-obligations.json`
records the required programmable-module obligations. `detta-verify` validates
the artifact against the parsed source, rejects stale roots, rejects missing
proof obligations, pins THM-016 coverage, and validates audit categories for
`aspect_parser`, `aspect_verifier`, `aspect_evaluator`, and
`kernel_host_calls`. The release gate now runs the aspect module client E2E
flow and verifies the aspect source, artifact, and proof-obligation checksum
attestations.

Acceptance criteria:

- release gate fails on invalid standard-library artifact;
- release gate fails on missing proof obligations;
- formal docs cover programmable module soundness;
- external audit scope includes parser, verifier, evaluator, and kernel host
  calls.

## 21. Production Acceptance Criteria

The secure generalization is not complete until all of these are true:

- DeTTa can deploy a token whose behavior is defined by taxonomy-aligned MeTTa
  aspects, not native Rust token branches.
- The deployed module is parsed, canonicalized, lowered, and verified before
  state admission.
- The evaluator executes canonical IR only.
- All mutation goes through guarded kernel host calls.
- Every method has policy, authority, effects, write scope, and invariants.
- The release gate rejects forbidden primitives and undeclared effects.
- The standard ERC20-like aspect bundle is differentially tested against the
  native token baseline.
- Extended token aspects such as fee, pause, restricted transfer, mint, burn,
  cap, and observable events are deployable without new Rust business logic.
- Receipts, events, storage proofs, and state roots work for aspect contracts.
- Validator replay and multi-node consensus are deterministic for aspect
  contracts.
- Formal artifacts explain why verified modules cannot bypass SCS security.
- Documentation shows how a user deploys and calls an aspect-defined token.

## 22. Main Risks

### Parser Ambiguity

Risk: source accepted by one node is interpreted differently by another.

Mitigation:

- canonical AST;
- deterministic renderer;
- root fixtures;
- strict versioning;
- reject ambiguous syntax.

### Aspect Composition Ambiguity

Risk: two aspects both transform the same action and ordering changes behavior.

Mitigation:

- explicit dependency edges;
- deterministic action graph;
- reject unresolved ordering.

### Invariant Under-Specification

Risk: modules declare weak invariants and still pass deployment.

Mitigation:

- standard-library invariant requirements;
- bundle-level minimum invariant profiles;
- verifier warnings as hard errors for production profiles.

### Evaluator Escape

Risk: restricted evaluator accidentally exposes host behavior.

Mitigation:

- allowlist-only primitives;
- forbidden primitive fixtures;
- no dynamic imports;
- no raw Atomspace operations;
- audit all host calls.

### Native/Aspect Divergence

Risk: precompile behavior differs from aspect behavior.

Mitigation:

- differential tests;
- shared ABI/policy/invariant roots;
- precompile equivalence manifests.

## 23. Open Design Decisions

- Whether aspect modules should be submitted as source only, artifact only, or
  source plus artifact.
- Whether method IDs should become first-class strings in `Transaction` instead
  of `Method::Other(String)`.
- Whether standard-library taxonomy files live in this repo or remain imported
  from the sibling taxonomy project with pinned hashes.
- Whether invariant checking should initially be runtime-only, symbolic-only,
  or hybrid.
- How much of AMM behavior should be represented as taxonomy aspects versus a
  separate DeFi primitive module family.
- Which governance threshold is required to bless a module as standard library.

## 24. Recommended First PR Sequence

1. Add `detta-aspects` crate with parser/canonicalizer for declarations only.
2. Add standard-library source fixture for a minimal static transferable token.
3. Add typed IR structs and artifact roots.
4. Add static checks for dependencies, conflicts, state ownership, and policy
   completeness.
5. Add evaluator support for executable transfer action.
6. Add programmable contract descriptor behind a feature-gated runtime path.
7. Add E2E deployment of an aspect-defined token in a single-node test.
8. Add differential test against native token transfer.
9. Expand to approval and transferFrom.
10. Add release-gate checks for aspect artifacts.

This sequence keeps the existing hard-coded DeFi runtime working while moving
one narrow behavior slice at a time into secure, taxonomy-aligned MeTTa.
