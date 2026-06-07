# Secured Contract Spaces for MeTTa/PeTTa DeFi

**Status:** Draft specification v0.3  
**Format:** Markdown  
**Abbreviation:** SCS

# Status and Scope

## Status

This document is a draft target-runtime specification. It describes the desired security architecture for DeFi contracts implemented in MeTTa/PeTTa-like systems. It is not a statement that current PeTTa already implements these controls.

Current ordinary PeTTa spaces are not sufficient as a DeFi security boundary. In the normal space implementation, adding an atom ultimately asserts into a Prolog predicate, removing retracts, and matching or enumeration queries the backing predicate directly. Current PeTTa also exposes powerful mutation and interoperation primitives, including Python and Prolog escape hatches, which must not be exposed directly to adversarial contract code. See References: PeTTa `spaces.pl` and PeTTa `metta.pl`.

## Scope

SCS defines a secured contract execution model for:

- token contracts;

- automated market makers;

- lending vaults;

- staking systems;

- bridges;

- oracle adapters;

- governance-controlled DeFi contracts.

A deployed contract instance is modeled as a secured Module Space: a uniform space-like object with private storage, public methods, a guarded authorization surface, and deterministic state transition semantics. The Module Space framing is inspired by HyperClaw, which treats modules as uniform spaces behind a common orchestration interface. See References: HyperClaw.

## Out of scope

SCS does not define:

- a blockchain consensus protocol;

- a final ASI:Chain, Hyperon, MeTTa, or PeTTa syntax;

- hidden-balance cryptography;

- zero-knowledge proof systems;

- a general-purpose operating-system sandbox;

- retroactive safety for ordinary public PeTTa spaces.

Transparent balances are allowed. Private balances require additional mechanisms such as commitments, nullifiers, encrypted state, or zero-knowledge transition proofs.

## Existing PeTTa `sealed` is not security sealing

SCS uses “secured” and “guarded” in a security sense. Current PeTTa has a form named `sealed`, but it is not a sealed storage or authorization primitive. The current translator handles it through Prolog term copying/scoping mechanics, not through access-control enforcement. Therefore, SCS **MUST NOT** rely on existing `sealed` syntax as a DeFi storage boundary. See References: PeTTa `translator.pl`.

# Executive Summary

A Secured Contract Space is a runtime-owned object representing one deployed smart-contract instance. It contains:

    SecuredContractSpace
        contract_id
        code_hash
        public ABI / exported methods
        private state subspace
        public view interface
        append-only event space
        method policy registry
        live capability registry
        enforcement module
        guarded storage kernel
        invariant set
        transaction engine hooks

The central rule is:

    External callers do not mutate state.
    External callers invoke methods.
    Methods are authorized by functional access-control policy.
    Authority is checked by live registry lookup.
    State writes are allowed only inside the method's authorized write scope.
    All state and registry effects commit or revert atomically.

The default authorization path is not “pass a capability token into every method.” Instead:

    call method
        -> runtime derives current caller from transaction context
        -> enforcement module looks up method policy
        -> enforcement module derives required grants from method + args + caller
        -> capability registry is queried live
        -> authorization succeeds or fails
        -> private implementation runs only if authorized

Signed capability certificates are allowed only as exceptions, usually through adapter methods such as `permit`, `executeSignedOrder`, `submitOracleAttestation`, or `redeemBridgeMessage`.

## Primary safety invariant

The primary safety invariant is:

    No balance changes merely because code can mention a balance atom.
    No balance changes merely because code can mention a capability-shaped term.
    Balances change only through an authorized method whose policy passes live registry enforcement and whose transition satisfies storage and protocol invariants.

# Normative Language and Goals

The keywords **MUST**, **MUST NOT**, **SHOULD**, **SHOULD NOT**, and **MAY** are normative.

This specification distinguishes between:

    contract-level convention:
        a pattern written in MeTTa source code

    runtime enforcement:
        a check that cannot be bypassed by malicious MeTTa code

For DeFi safety, all critical protections **MUST** be runtime-enforced.

## Goals

SCS **MUST** provide:

1.  one secured state space per deployed contract instance;

2.  public method calls through a contract dispatcher;

3.  deny-by-default method authorization;

4.  functional access control based on method policy;

5.  live capability-registry lookup as the default authority check;

6.  no routine capability-token passing by callers;

7.  guarded grant, revoke, update, and consume operations for registry capabilities;

8.  runtime-derived caller identity;

9.  method-scoped write authority;

10. storage-level rejection of unauthorized writes;

11. atomic commit/revert for state, registry, and events;

12. invariant enforcement before commit;

13. deterministic execution;

14. restricted PeTTa evaluator mode for contract code;

15. auditable authorization decisions and state transitions.

## Design bias

The design deliberately prefers simple, live authorization over portable bearer authority:

    Default:
        method policy + current caller + live registry lookup

    Exception:
        signed certificate adapter at protocol edges

This keeps ordinary contract calls simple and makes revocation, spend tracking, roles, and governance easier to reason about.

# Threat Model

SCS assumes adversaries may:

1.  submit arbitrary external transactions;

2.  deploy malicious contracts;

3.  call public methods in unexpected orders;

4.  trigger cross-contract calls;

5.  attempt reentrancy;

6.  attempt raw Atomspace mutation;

7.  attempt raw Atomspace enumeration;

8.  attempt to forge caller identity;

9.  attempt to bypass method policies;

10. attempt to mutate capability registry records;

11. attempt to exploit nondeterminism;

12. attempt to leak private state through errors, events, traces, or logs;

13. attempt to call PeTTa, Prolog, Python, filesystem, process, or network escape hatches;

14. attempt replay attacks using nonces or signed certificates;

15. attempt to create duplicate or contradictory balance facts.

SCS assumes the following components are part of the trusted computing base:

- contract dispatcher;

- method policy registry;

- enforcement module;

- live capability registry;

- guarded storage kernel;

- atomic transaction engine;

- restricted evaluator implementation;

- deterministic serialization and hashing machinery.

SCS does not trust ordinary contract code to enforce its own security by convention.

# Core Architecture

SCS uses a layered enforcement architecture.

    External transaction
        |
        v
    Contract Dispatcher
        |
        v
    Method Policy Registry
        |
        v
    Enforcement Module
        |
        v
    Live Capability Registry
        |
        v
    Authorized Execution Frame
        |
        v
    Private Method Implementation
        |
        v
    Guarded Storage Kernel
        |
        v
    Atomic Transaction Engine
        |
        v
    Committed State + Events

The runtime has five main enforcement layers:

- **Dispatcher:** controls which methods are callable.
- **Method policy registry:** declares required authority and allowed effects.
- **Enforcement module:** checks current caller against live registry state.
- **Guarded storage kernel:** rejects raw mutation and writes outside method scope.
- **Atomic transaction engine:** commits all registry, state, and event effects together.



## Ordinary call flow

A normal method call follows this shape:

    (call-contract! tx TokenA transferFrom Alice Bob USDC 10)

The caller does not pass a capability token. Instead, the runtime binds the transaction context, finds the method policy for `TokenA.transferFrom`, derives the required allowance from the method arguments, and checks the live capability registry.

## State transition boundary

The private implementation may request state reads and writes through runtime primitives. The storage kernel accepts a write only if:

- an authorized execution frame is active;

- the write targets the current contract instance;

- the key matches the active method’s write scope;

- the value satisfies schema and kernel invariants;

- the transaction engine can stage the write deterministically.

# Component Model

## Secured Contract Space

A deployed contract instance **MUST** be represented by a SCS object.

Conceptual structure:

    (SecuredContractSpace
      (contract-id TokenA)
      (code-hash Hash-abc)
      (state-space TokenA.state)
      (view-space TokenA.views)
      (event-space TokenA.events)
      (method-policy-registry TokenA.method-policies)
      (capability-registry TokenA.capabilities)
      (invariants TokenA.invariants)
      (dispatcher TokenA.dispatcher))

The implementation **MAY** store this as Prolog facts, Rust structs, database records, or another runtime representation. It **MUST NOT** expose the raw state handle to untrusted MeTTa code.

## Contract Dispatcher

The dispatcher is the only external entrypoint into a contract. It handles:

- method resolution;

- ABI checking;

- caller-context binding;

- policy lookup;

- authorization callout;

- private implementation invocation;

- result formatting;

- error handling.

External calls **MUST** use dispatcher-mediated forms such as:

    (call-contract! tx TokenA transfer Bob USDC 10)

External callers **MUST NOT** call implementation internals such as:

    (TokenA._transferImpl Bob USDC 10)

## Method Policy Registry

The method policy registry maps:

    (contract_id, method_name, optional argument pattern)

to:

- authorization requirements;

- read effects;

- write effects;

- registry consumption effects;

- event effects;

- invariant requirements;

- reentrancy policy.

## Capability Registry

The capability registry is the default source of durable authority. It stores live grants such as roles, allowances, spend limits, oracle updater permissions, bridge validator permissions, governance rights, and session limits.

## Enforcement Module

The enforcement module evaluates method policies against:

- current transaction context;

- method arguments;

- live capability registry;

- contract state;

- system state;

- pause state;

- governance state.

## Guarded Storage Kernel

The guarded storage kernel owns contract state mutation. It **MUST** reject raw state mutation, unauthorized writes, duplicate canonical keys, schema violations, and numeric-domain violations.

## Atomic Transaction Engine

The transaction engine stages contract-state writes, registry updates, nonce consumption, event emissions, and cross-contract subcall effects. It commits them together or discards them together.

## Certificate Adapter

Signed certificates are exceptional adapters into the registry-first model. Examples include `permit`, `executeSignedOrder`, `submitOracleAttestation`, `redeemBridgeMessage`, and `openSession`.

# Contract-Space Isolation Requirements

#### SCS-ISO-001 — Per-instance state

Each deployed contract instance **MUST** own a distinct guarded state subspace.

#### SCS-ISO-002 — No ordinary public state space

Contract state **MUST NOT** be represented as an ordinary user-addressable PeTTa `&space`.

#### SCS-ISO-003 — No raw state handles

The runtime **MUST NOT** expose raw state-space handles to external callers or other contracts.

#### SCS-ISO-004 — Shared code, separate state

Multiple contract instances **MAY** share code, but their state and authorization registries **MUST** remain separate unless an explicit shared-state contract is deployed.

Example:

    ERC20 code
        TokenA.state
        TokenB.state
        TokenC.state

`TokenA.transfer` **MUST NOT** be able to write `TokenB.state`.

#### SCS-ISO-005 — Private implementation namespace

Internal functions such as `_setBalance!`, `_debit!`, `_credit!`, `_setReserve!`, `_setDebt!`, and `_consumeAllowance!` **MUST NOT** be externally callable.

#### SCS-ISO-006 — No namespace guessing

Security **MUST NOT** depend on contract-state names being secret. Even if an attacker guesses a contract’s internal state identifier, raw access **MUST** fail.

#### SCS-ISO-007 — Explicit shared state

If multiple contracts intentionally share state, the shared state **MUST** be modeled as its own secured contract or secured shared resource with explicit policies and invariants.

# Transaction Context

Every call executes with a runtime-created context. Required fields are:

    chain_id
    block_height or slot
    block_timestamp or logical time
    tx_hash
    tx_sender
    msg_sender
    current_contract
    current_method
    call_depth
    call_stack
    nonce
    gas_or_step_budget
    read_set
    write_set
    registry_delta
    event_buffer
    reentrancy_locks
    authorization_result

## Caller identity

The runtime **MUST** distinguish:

- **`tx_sender`:** original external transaction signer.
- **`msg_sender`:** immediate caller of the current contract.
- **`current_contract`:** contract currently executing.



Example:

    Alice calls Dex.swap
    Dex calls TokenA.transferFrom

    Inside TokenA:
        tx_sender        = Alice
        msg_sender       = Dex
        current_contract = TokenA

`transferFrom` should check whether `Dex` has authority to spend Alice’s funds.

## Context requirements

#### SCS-TX-001 — Authenticated sender

Every state-changing external call **MUST** have an authenticated origin.

#### SCS-TX-002 — Runtime-created context

The transaction context **MUST** be created by the runtime, not by user code.

#### SCS-TX-003 — Sender distinction

The runtime **MUST** distinguish `tx_sender`, `msg_sender`, and `current_contract`.

#### SCS-TX-004 — Replay protection

State-changing calls **MUST** consume a valid nonce or equivalent replay-prevention mechanism.

#### SCS-TX-005 — Context integrity

User code **MUST NOT** be able to forge or mutate `tx_sender`, `msg_sender`, `current_contract`, `current_method`, `call_depth`, nonce, block context, authorization result, or write scope.

#### SCS-TX-006 — Consensus determinism

All context values used by contracts **MUST** be deterministic under consensus execution.

#### SCS-TX-007 — Resource budget

The context **MUST** include a gas, step, or resource budget.

# Functional Access Control

Functional access control is the core SCS authorization model. Callers normally do not pass capability tokens. Methods declare required authority. The enforcement module derives the required authority from:

    contract_id
    method_name
    method_arguments
    msg_sender
    tx_sender
    current state
    current registry

## Default call shape

Preferred:

    (call-contract! tx TokenA transferFrom Alice Bob USDC 10)

Avoid by default:

    (call-contract! tx TokenA transferFrom (CapRef Cap-123) Alice Bob USDC 10)

Capability references **MAY** appear in specialized methods, but they are not the ordinary programming model.

## Method policy structure

Conceptual policy form:

    (method-policy TokenA transferFrom
      (principal msg_sender)

      (requires
        AuthenticatedCaller
        ContractNotPaused
        (RegistryGrant
          (registry TokenA.capabilities)
          (holder msg_sender)
          (resource (Allowance TokenA $from $asset))
          (right transfer-from)
          (amount $amount)
          (consume $amount)))

      (reads
        (Balance TokenA $from $asset)
        (Balance TokenA $to $asset)
        (Allowance TokenA $from msg_sender $asset))

      (writes
        (Balance TokenA $from $asset)
        (Balance TokenA $to $asset)
        (Allowance TokenA $from msg_sender $asset))

      (emits Transfer)

      (invariants
        NoNegativeBalances
        TotalSupplyConserved)

      (reentrancy nonreentrant))

## Functional access-control requirements

#### SCS-FAC-001 — Policy per public method

Every public state-changing method **MUST** have a registered method policy.

#### SCS-FAC-002 — Deny by default

A method with no registered policy **MUST** be unavailable to external callers.

#### SCS-FAC-003 — Runtime-derived principal

The authorization principal **MUST** be derived from transaction context, not from a user-provided method argument.

#### SCS-FAC-004 — Default principal

The default authorization principal **SHOULD** be `msg_sender`.

#### SCS-FAC-005 — Live registry lookup

The enforcement module **MUST** check required durable authority through live capability-registry lookup.

#### SCS-FAC-006 — No routine capability arguments

Normal methods **SHOULD NOT** require callers to pass capability tokens or capability references.

#### SCS-FAC-007 — Argument-dependent policies

Method policies **MAY** bind requirements to method arguments. For example, `transferFrom(from, to, asset, amount)` requires an allowance grant from `from` to `msg_sender` for `asset` and at least `amount`.

#### SCS-FAC-008 — Guarded policy registration

Only deployment, governance, or authorized admin mechanisms **MAY** register or update method policies.

#### SCS-FAC-009 — Dispatcher enforcement

The dispatcher **MUST** enforce method policy before private implementation execution.

#### SCS-FAC-010 — Private implementation protection

The private implementation behind a public method **MUST NOT** be externally callable.

#### SCS-FAC-011 — Atomic authorization effects

Any registry effects caused by authorization, such as allowance consumption or nonce consumption, **MUST** commit or revert atomically with the contract call.

#### SCS-FAC-012 — Method-scoped write set

A method policy **SHOULD** declare the state classes or keys the method may write.

#### SCS-FAC-013 — Storage-level write-scope enforcement

The storage kernel **MUST** reject writes outside the active method’s authorized write scope.

#### SCS-FAC-014 — View policies

Read-only methods **SHOULD** have policies too, even if the policy is simply `PublicRead`.

#### SCS-FAC-015 — Auditability

Authorization success, authorization failure, and registry consumption **SHOULD** be auditable.

## Two-layer authorization

SCS separates method access from state effect control:

    method access control:
        who may call this method?

    effect control:
        what may this method mutate?

Both layers are required. A caller may be authorized to call `transferFrom`, but the implementation must still be prevented from mutating unrelated state such as minter roles, upgrade admin records, or total supply unless the policy explicitly permits those writes.

# Capability Registry

The capability registry is the default authority substrate. A capability is normally a live grant in registry state, not a token passed into every method.

## Registry grants

Example registry records:

    (RoleGrant
      (contract TokenA)
      (role Minter)
      (holder BridgeA)
      (admin Governance)
      (active True))

    (AllowanceGrant
      (token TokenA)
      (owner Alice)
      (spender Dex)
      (asset USDC)
      (limit 1000)
      (spent 250)
      (expiry Block-900000)
      (revoked False))

    (OracleGrant
      (oracle PriceOracleA)
      (feed ETH-USD)
      (holder OracleBot7)
      (right update-price)
      (active True))

## Registry-first authority

The normal authorization path is:

    method call
      -> policy lookup
      -> derive grant key from method arguments and caller
      -> live registry lookup
      -> authorize or reject

For example, `transferFrom(Alice, Bob, USDC, 10)` called by `Dex` derives the canonical lookup:

    AllowanceGrant(TokenA, owner=Alice, spender=Dex, asset=USDC)

The method succeeds only if that grant exists, is active, is not expired or revoked, and has sufficient remaining amount.

## Registry requirements

#### SCS-REG-001 — Registry-first authority

The default capability certification mechanism **MUST** be live lookup in an authoritative capability registry.

#### SCS-REG-002 — Contract-scoped registries

Each contract **MAY** own its own capability registry. A runtime **MAY** also provide a global registry, but authority records **MUST** be explicitly scoped by contract and resource.

#### SCS-REG-003 — Guarded registry mutation

Only authorized methods **MAY** create, update, consume, revoke, or delete registry grants.

#### SCS-REG-004 — Use-time lookup

Protected operations **MUST** check the registry at use time.

#### SCS-REG-005 — No authority from syntax

A MeTTa term that merely resembles a grant **MUST NOT** authorize anything unless it exists in the authoritative registry or is accepted by an approved certificate adapter.

#### SCS-REG-006 — Canonical lookup keys

Registries **SHOULD** use canonical keys.

Examples:

    Role:
        (contract, role, holder)

    Allowance:
        (token, owner, spender, asset)

    Oracle:
        (oracle, feed, updater)

    Governance:
        (contract, action, holder)

#### SCS-REG-007 — Atomic consumption

If a grant use consumes allowance, quota, nonce, or spend limit, that consumption **MUST** be staged and committed atomically with the protected operation.

#### SCS-REG-008 — Revocation visibility

A revoked grant **MUST** fail once the revocation is visible in the current execution state.

#### SCS-REG-009 — No stale authorization cache

A cached authorization result **MUST NOT** authorize a later operation after the underlying grant has changed, unless snapshot semantics explicitly permit it.

#### SCS-REG-010 — Deterministic lookup

Registry lookup **MUST** be deterministic under consensus execution.

#### SCS-REG-011 — Registry schema

Capability registries **SHOULD** define schemas for each grant class.

#### SCS-REG-012 — Registry invariants

The registry **MUST** enforce grant-class invariants such as nonnegative limits, valid expiry domains, valid holder identifiers, valid resource identifiers, and valid right names.

## Registry consumption

For spend-like grants, authorization has a side effect. For `transferFrom`, the runtime must stage:

    AllowanceGrant.spent:= AllowanceGrant.spent + amount

That staged update **MUST** commit only if the corresponding balance transfer also commits.

# Signed Certificate Exceptions

Signed capability certificates are allowed, but they are not the default authority mechanism. They are adapters at the edges of the registry-first model.

## Appropriate uses

Signed certificates **SHOULD** be limited to:

- permit-style approvals;

- off-chain orders;

- relayed transactions;

- cross-chain attestations;

- oracle signatures;

- temporary sessions;

- batch authorization.

## Certificate adapter pattern

A certificate adapter takes a signed object and turns it into registry state or consumes registry state.

Example:

    (call-contract! tx TokenA permit Alice Dex USDC 1000 signedPermit)

`permit` does:

1.  verify signature;

2.  verify domain separation;

3.  verify nonce;

4.  create or update registry allowance;

5.  consume nonce;

6.  emit `Approval`.

Then future calls use the normal registry path:

    (call-contract! tx TokenA transferFrom Alice Bob USDC 10)

## Certificate requirements

#### SCS-CERT-001 — Exceptional use

Signed certificates **MAY** be supported but **SHOULD NOT** be the normal authorization mechanism.

#### SCS-CERT-002 — Canonical payload

A signed certificate **MUST** have a canonical serialization.

#### SCS-CERT-003 — Domain separation

A certificate **MUST** bind the signature to chain, runtime, contract, resource, operation class, and relevant nonce domain.

#### SCS-CERT-004 — Issuer authority

The verifier **MUST** check that the signer is authorized to issue the certified authority.

#### SCS-CERT-005 — Non-malleability

Changing holder, resource, rights, limit, expiry, nonce, or delegation policy **MUST** invalidate the certificate.

#### SCS-CERT-006 — Replay protection

A state-changing certificate **MUST** consume a nonce, certificate identifier, sequence number, or equivalent registry record.

#### SCS-CERT-007 — Registry integration

Where practical, successful certificate verification **SHOULD** create, update, or consume registry authority.

#### SCS-CERT-008 — Short validity

Certificate-only authority **SHOULD** be short-lived.

#### SCS-CERT-009 — No direct state authority

A signed certificate **MUST NOT** by itself bypass method-policy enforcement or guarded storage.

## Hybrid mode

If a signed certificate requires revocation, spend tracking, or nonce tracking, it becomes hybrid:

    signature proves issuance
    registry proves freshness / non-revocation / unused nonce / remaining spend

This hybrid mode is preferred for most DeFi certificate flows.

# Public ABI and Method Visibility

## ABI declaration

A contract **MUST** declare its public methods.

Example:

    (contract TokenA
      (exports
        balanceOf
        allowance
        transfer
        approve
        transferFrom
        permit))

## Visibility classes

SCS **SHOULD** support at least:

- **public:** callable externally through dispatcher.
- **view:** externally callable and read-only.
- **internal:** callable only by same contract implementation.
- **private:** callable only by implementation frame, not cross-contract.
- **kernel:** callable only by runtime or storage kernel.



## ABI requirements

#### SCS-ABI-001 — Exported methods only

External callers **MUST** interact with contracts only through exported public or view methods.

#### SCS-ABI-002 — No direct private calls

External callers **MUST NOT** invoke private/internal implementation methods.

#### SCS-ABI-003 — Type checking

The dispatcher **SHOULD** type-check method arguments before authorization and execution.

#### SCS-ABI-004 — View read-only guarantee

View methods **MUST NOT** receive write authority and **MUST NOT** mutate state, registry, or event logs.

#### SCS-ABI-005 — Raw match forbidden

Public reads **MUST** go through view methods, not raw pattern matching over private state.

#### SCS-ABI-006 — ABI stability

Public ABI changes **SHOULD** require governance or deployment-time authority.

#### SCS-ABI-007 — Method selector determinism

Method resolution **MUST** be deterministic and unambiguous.

# Guarded State Model and Storage Kernel

Contract state **MUST** use canonical storage records, not loose contradictory facts.

## State schema examples

A token contract might declare:

    (state-schema TokenA
      (Balance Account Asset UInt)
      (Allowance Account Spender Asset UInt)
      (TotalSupply Asset UInt)
      (Paused Bool)
      (Nonce Account UInt))

An AMM might declare:

    (state-schema PoolA
      (Reserve Asset UInt)
      (LiquidityPosition Account UInt)
      (FeeBps UInt)
      (Paused Bool))

## Canonical keys

Balances **SHOULD** be stored by canonical key:

    key   = (BalanceKey TokenA Alice USDC)
    value = 100

not as loose duplicate atoms:

    (Balance Alice USDC 100)
    (Balance Alice USDC 999)

## State requirements

#### SCS-STATE-001 — Canonical records

State records representing unique entities **MUST** have canonical keys.

#### SCS-STATE-002 — No duplicate balances

The storage layer **MUST** prevent multiple canonical balances for the same `(contract, account, asset)` key.

#### SCS-STATE-003 — Integer amounts

Token amounts **MUST** be represented as integers, not floating-point numbers.

#### SCS-STATE-004 — Nonnegative UInt fields

Fields declared as `UInt` **MUST** reject negative values.

#### SCS-STATE-005 — Schema validation

State writes **SHOULD** be validated against the declared schema.

#### SCS-STATE-006 — Keyed replacement

State updates **SHOULD** use keyed replacement semantics.

Preferred:

    (state-set! (BalanceKey TokenA Alice USDC) 90)

Avoid:

    (remove-atom &state (Balance Alice USDC 100))
    (add-atom &state (Balance Alice USDC 90))

#### SCS-STATE-007 — No raw state enumeration

Raw private-state enumeration **MUST** be forbidden unless the caller has an explicit auditor policy.

#### SCS-STATE-008 — State root

The runtime **SHOULD** compute a deterministic state root after each committed transition.

## Guarded storage kernel

The storage kernel is the last line of defense. Even if a method implementation is buggy, the kernel **MUST** enforce:

- active authorized method;

- active execution frame;

- active write scope;

- valid state schema;

- canonical key constraints;

- numeric constraints;

- contract instance isolation.

## Conceptual kernel API

    (state-get ctx key)
    (state-set! ctx key value)
    (state-delete! ctx key)
    (state-exists? ctx key)

The context argument is not a user-supplied value. It is the runtime execution context.

## Storage requirements

#### SCS-STOR-001 — Active frame required

State writes **MUST** fail unless an authorized contract execution frame is active.

#### SCS-STOR-002 — Contract match

A method executing in `TokenA` **MUST NOT** write `TokenB` state unless a declared cross-contract protocol explicitly authorizes it.

#### SCS-STOR-003 — Write-scope check

Each write **MUST** match the active method’s authorized write set.

#### SCS-STOR-004 — Registry-state separation

Registry mutation **MUST** be handled as registry delta, not arbitrary state mutation.

#### SCS-STOR-005 — No bypass primitives

Raw `add-atom`, `remove-atom`, `get-atoms`, raw `match`, Prolog assertion/retraction, Python calls, filesystem access, process execution, and network access **MUST NOT** bypass guarded storage.

#### SCS-STOR-006 — Kernel-enforced schema

The kernel **SHOULD** enforce state schema constraints that are cheap and deterministic.

#### SCS-STOR-007 — Audit hooks

The kernel **SHOULD** expose auditable read/write sets without leaking unauthorized private state.

# Atomic Commit and Revert

All state-changing calls execute as staged transitions.

## Staged transition components

    contract_state_delta
    capability_registry_delta
    nonce_delta
    event_buffer
    cross_contract_subcall_deltas
    invariant_check_results

## Commit algorithm

1.  Validate transaction.

2.  Resolve contract and method.

3.  Bind runtime context.

4.  Look up method policy.

5.  Authorize via enforcement module.

6.  Stage registry consumption effects.

7.  Execute private implementation.

8.  Stage state writes.

9.  Stage events.

10. Run invariant checks.

11. If all checks pass, commit registry delta, state delta, events, nonce update, and state root.

12. Otherwise, discard all staged effects.

13. Revoke the execution frame.

14. Return result or error.

## Atomicity requirements

#### SCS-ATOMIC-001 — All-or-nothing

A state-changing method **MUST** commit all effects or none.

#### SCS-ATOMIC-002 — No partial transfer

A transfer **MUST NOT** commit a debit without the corresponding credit.

#### SCS-ATOMIC-003 — Registry and state atomicity

Authorization-consumption effects, such as allowance usage, **MUST** commit atomically with balance changes.

#### SCS-ATOMIC-004 — Event rollback

Events emitted by reverted calls **MUST** be discarded.

#### SCS-ATOMIC-005 — Revert conditions

The runtime **MUST** revert on authorization failure, policy failure, forbidden operation, type error, arithmetic overflow, insufficient balance, nonce failure, capability registry failure, invariant violation, out-of-gas or out-of-steps, unhandled exception, reentrancy violation, and storage-scope violation.

#### SCS-ATOMIC-006 — Nested subtransactions

Nested contract calls **MUST** execute in subtransactions. Child effects merge into the parent only if the child succeeds.

#### SCS-ATOMIC-007 — Read-your-writes

A contract **SHOULD** read its own staged writes within the same transition.

#### SCS-ATOMIC-008 — Deterministic rollback

Rollback semantics **MUST** be deterministic and independent of host-language side effects.

# Invariant Enforcement

SCS **MUST** enforce DeFi-critical invariants before commit.

## Generic invariants

- no negative balances;

- one canonical balance per account and asset;

- total supply matches the sum of balances;

- allowance cannot be overspent;

- nonce cannot be replayed;

- registry grants cannot be mutated without authority;

- events correspond to committed state changes;

- state schema is respected.

## Token invariants

- **Transfer:** total supply is conserved.
- **Mint:** minter authority is required, total supply increases by minted amount, and recipient balance increases by minted amount.
- **Burn:** burn authority is required, total supply decreases by burned amount, and owner balance decreases by burned amount.
- **transferFrom:** allowance is consumed atomically and spender is `msg_sender`.



## AMM invariants

- reserves are nonnegative;

- swap respects pricing formula;

- slippage bound is enforced;

- fees are accounted for;

- pool accounting matches token transfers;

- LP shares match pool ownership model.

## Lending invariants

- debt is nonnegative;

- collateral is nonnegative;

- borrow respects loan-to-value limits;

- oracle price is fresh;

- liquidation improves or preserves solvency;

- protocol reserves remain nonnegative.

## Oracle invariants

- price update comes from authorized updater;

- timestamp is fresh;

- deviation bounds are enforced where applicable;

- quorum rules are satisfied where applicable.

## Invariant requirements

#### SCS-INV-001 — Declared invariants

Each contract **MUST** declare critical invariants.

#### SCS-INV-002 — Runtime enforcement

Mandatory invariants **MUST** be checked by runtime or trusted kernel code before commit.

#### SCS-INV-003 — Pure invariant functions

Invariant functions **MUST** be deterministic and side-effect-free.

#### SCS-INV-004 — Storage-level invariants

Basic invariants such as canonical key uniqueness and `UInt` nonnegativity **SHOULD** be enforced by the storage kernel, not only by contract code.

#### SCS-INV-005 — Failure reverts

Invariant failure **MUST** revert the full transition.

#### SCS-INV-006 — Invariant coverage

Invariant declarations **SHOULD** identify which methods can affect each invariant.

# DeFi Method Examples

This section gives illustrative method policies. The exact syntax is not normative; the security obligations are normative.

## Token transfer

Call:

    (call-contract! tx TokenA transfer Bob USDC 10)

Policy:

    (method-policy TokenA transfer
      (principal msg_sender)

      (requires
        AuthenticatedCaller
        ContractNotPaused
        (> $amount 0)
        (BalanceAtLeast TokenA msg_sender $asset $amount))

      (writes
        (Balance TokenA msg_sender $asset)
        (Balance TokenA $to $asset))

      (emits Transfer)

      (invariants
        NoNegativeBalances
        TotalSupplyConserved)

      (reentrancy nonreentrant))

Implementation effect:

    debit msg_sender
    credit recipient
    emit Transfer

No allowance registry lookup is needed because the caller spends its own balance.

## Approve

Call:

    (call-contract! tx TokenA approve Dex USDC 1000)

Policy:

    (method-policy TokenA approve
      (principal msg_sender)

      (requires
        AuthenticatedCaller
        ContractNotPaused)

      (registry-writes
        (AllowanceGrant TokenA msg_sender $spender $asset))

      (emits Approval)

      (reentrancy nonreentrant))

Effect:

    set AllowanceGrant(TokenA, owner=msg_sender, spender=Dex, asset=USDC, limit=1000)
    emit Approval

## transferFrom

Call:

    (call-contract! tx TokenA transferFrom Alice Bob USDC 10)

Policy:

    (method-policy TokenA transferFrom
      (principal msg_sender)

      (requires
        AuthenticatedCaller
        ContractNotPaused
        (RegistryGrant
          (kind AllowanceGrant)
          (token TokenA)
          (owner $from)
          (spender msg_sender)
          (asset $asset)
          (right transfer-from)
          (amount $amount)
          (consume $amount))
        (BalanceAtLeast TokenA $from $asset $amount))

      (registry-consumes
        (AllowanceGrant TokenA $from msg_sender $asset $amount))

      (writes
        (Balance TokenA $from $asset)
        (Balance TokenA $to $asset))

      (emits Transfer)

      (invariants
        NoNegativeBalances
        TotalSupplyConserved)

      (reentrancy nonreentrant))

Effects:

    consume allowance
    debit Alice
    credit Bob
    emit Transfer

## Mint

Call:

    (call-contract! tx TokenA mint Bob USDC 1000)

Policy:

    (method-policy TokenA mint
      (principal msg_sender)

      (requires
        AuthenticatedCaller
        (RegistryGrant
          (kind RoleGrant)
          (contract TokenA)
          (role Minter)
          (holder msg_sender)
          (right use-role)))

      (writes
        (Balance TokenA $to $asset)
        (TotalSupply TokenA $asset))

      (emits Mint Transfer)

      (invariants
        NoNegativeBalances
        TotalSupplyEqualsSumBalances)

      (reentrancy nonreentrant))

Access-control libraries for smart contracts commonly frame privileged actions, such as minting or pausing, as explicit role-governed permissions.

## Permit

Call:

    (call-contract! tx TokenA permit Alice Dex USDC 1000 signedPermit)

Policy:

    (method-policy TokenA permit
      (principal tx_sender)

      (requires
        ValidSignedPermit
        NonceUnused)

      (registry-writes
        (AllowanceGrant TokenA Alice Dex USDC)
        (Nonce TokenA Alice $nonce))

      (emits Approval)

      (reentrancy nonreentrant))

Effect:

    verify signature
    consume nonce
    create/update registry allowance
    emit Approval

Future `transferFrom` calls use live registry lookup, not the certificate argument.

# Cross-Contract Calls, Reentrancy, and Events

## Cross-contract call stack

SCS **MUST** support composability while controlling reentrancy.

Example call stack:

    call_stack = [
      Frame(tx_sender=Alice, msg_sender=Alice, current_contract=Dex, method=swap),
      Frame(tx_sender=Alice, msg_sender=Dex, current_contract=TokenA, method=transferFrom)
    ]

## Cross-contract requirements

#### SCS-CALL-001 — Explicit call stack

The runtime **MUST** maintain an explicit call stack.

#### SCS-CALL-002 — Correct sender semantics

Nested calls **MUST** update `msg_sender` to the immediate caller while preserving `tx_sender`.

#### SCS-CALL-003 — Reentrancy policy

Each state-changing method **MUST** have a reentrancy policy. The default **SHOULD** be `nonreentrant`.

#### SCS-CALL-004 — Checks-effects-interactions

Where external calls are necessary, contract code **SHOULD** follow checks-effects-interactions: complete checks, apply local state effects, and only then interact externally. Solidity’s security guidance recommends this ordering to reduce reentrancy risk.

#### SCS-CALL-005 — No caller write-scope leakage

A caller’s write scope **MUST NOT** be passed to a callee.

#### SCS-CALL-006 — Callee-owned authorization

A callee **MUST** authorize its own method based on its own policy and registry.

#### SCS-CALL-007 — Revalidate after external calls

A contract **MUST NOT** rely on assumptions about another contract’s state after an external call unless those assumptions are revalidated.

#### SCS-CALL-008 — Cross-contract subtransactions

Cross-contract calls **MUST** compose through nested subtransactions with deterministic merge or revert semantics.

## Events and audit log

Each contract **SHOULD** have an append-only event space.

Example event:

    (Event
      (contract TokenA)
      (tx Tx-123)
      (index 0)
      (type Transfer)
      (args Alice Bob USDC 10))

## Event requirements

#### SCS-EVT-001 — Append-only committed events

Committed events **MUST** be append-only.

#### SCS-EVT-002 — Event rollback

Events from reverted calls **MUST NOT** be committed.

#### SCS-EVT-003 — Event authority

Only the currently executing contract frame **MAY** emit events for that contract.

#### SCS-EVT-004 — No secret leakage

Events **MUST NOT** contain private runtime context, hidden storage handles, internal kernel authority, or sensitive debug data.

#### SCS-EVT-005 — Deterministic order

Event ordering within a transaction **MUST** be deterministic.

#### SCS-EVT-006 — Authorization audit

The runtime **SHOULD** produce an audit trail for policy checks and registry consumption.

# Restricted PeTTa Contract Evaluator

A PeTTa DeFi runtime **MUST** run contract code in a restricted evaluator mode. MeTTa is designed for reflective and self-modifying computation; the Hyperon paper describes Atomspace add/remove operations and notes that MeTTa programs can rewrite their own code. That is valuable for AGI experimentation, but adversarial DeFi contract execution needs a stricter sandbox.

## Forbidden by default

The restricted evaluator **MUST NOT** expose these to contract code unless explicitly wrapped by trusted, runtime-checked primitives:

    add-atom
    remove-atom
    get-atoms
    raw match over guarded state
    import!
    py-call
    callPredicate
    assertzPredicate
    assertaPredicate
    retractPredicate
    translatePredicate
    add-translator-rule!
    remove-translator-rule!
    host filesystem access
    process execution
    network access
    unmetered randomness
    unmetered current time
    debug traces containing private state

## Allowed safe primitives

The restricted evaluator **MAY** expose:

    require
    assert-eq
    safe-add
    safe-sub
    safe-mul
    safe-div
    state-get
    state-set!
    registry-get
    registry-set!
    registry-consume!
    emit!
    call-contract!
    view-contract
    tx-sender
    msg-sender
    current-contract
    block-height
    block-timestamp

All mutation primitives in this list **MUST** be runtime-guarded.

## PeTTa storage recommendation

For PeTTa, guarded contract state **SHOULD** be stored outside ordinary user-addressable spaces. One possible representation is:

    contract_state(ContractId, Key, Value).
    contract_registry(ContractId, GrantKey, GrantValue).
    contract_event(ContractId, EventIndex, Event).

These predicates **MUST NOT** be exposed through raw PeTTa `match`, `get-atoms`, `assertzPredicate`, or `retractPredicate`.

## Evaluator requirements

#### SCS-SBX-001 — Restricted evaluator

Untrusted contract code **MUST** run under a restricted evaluator.

#### SCS-SBX-002 — Dangerous primitives forbidden

Raw mutation, host interop, and nondeterministic primitives **MUST** be forbidden by default.

#### SCS-SBX-003 — Guarded replacements

Any exposed replacement for mutation or interop **MUST** enforce method policy, runtime context, and storage scope.

#### SCS-SBX-004 — No arbitrary imports

Contracts **MUST NOT** import arbitrary local files, Python modules, Prolog predicates, or remote resources at execution time.

#### SCS-SBX-005 — Trace filtering

Debug traces **MUST NOT** reveal private state, registry internals, or sensitive runtime context to unauthorized users.

# Deployment, Governance, Determinism, and Errors

## Deployment

Deployment **MUST** register:

- contract identifier;

- code hash;

- initial state schema;

- public ABI;

- method policies;

- initial capability registry;

- invariant set;

- upgrade policy;

- admin policy.

#### SCS-GOV-001 — Guarded deployment

Deployment **MUST** initialize method policies and registry state atomically with contract creation.

#### SCS-GOV-002 — Guarded policy updates

Method policy updates **MUST** require governance or authorized admin authority.

#### SCS-GOV-003 — Timelocks for critical changes

Critical changes **SHOULD** be timelocked. Examples include implementation upgrades, method policy changes, fee changes, oracle-source changes, loan-to-value changes, minter grants, and bridge validator-set changes.

#### SCS-GOV-004 — Pause authority

Emergency pause **MAY** exist, but pause authority **MUST NOT** automatically imply mint, upgrade, withdraw, or registry-admin authority.

#### SCS-GOV-005 — Least privilege

Roles **SHOULD** be narrow and composable.

## Determinism and resource bounds

#### SCS-DET-001 — Deterministic execution

Given the same initial state and transaction, validators **MUST** compute the same result, events, registry state, and state root.

#### SCS-DET-002 — Bounded execution

Each call **MUST** have bounded computation, memory, recursion depth, event output, and cross-contract call depth.

#### SCS-DET-003 — No nondeterministic host calls

Contract code **MUST NOT** call nondeterministic host functions unless the result is provided by consensus context.

#### SCS-DET-004 — Metering

The evaluator **MUST** meter execution steps or gas.

#### SCS-DET-005 — Canonical serialization

State roots, event roots, signed payloads, and audit traces **SHOULD** use canonical serialization.

## Error semantics

SCS errors **SHOULD** be structured.

Example:

    (Error
      (code InsufficientBalance)
      (contract TokenA)
      (method transfer)
      (tx Tx-123))

Required error classes include:

    Unauthorized
    MethodNotExported
    PolicyMissing
    PolicyRejected
    RegistryGrantMissing
    RegistryGrantExpired
    RegistryGrantRevoked
    AllowanceExceeded
    NonceInvalid
    InsufficientBalance
    InvalidAmount
    WriteScopeViolation
    ForbiddenPrimitive
    InvariantViolation
    ReentrancyViolation
    OutOfGas
    TypeError
    ArithmeticOverflow
    ContractPaused

Errors **MUST NOT** leak private state beyond the caller’s authorized view.

# Minimal Acceptance Tests

A conforming SCS implementation **MUST** pass at least these tests.

## Raw state mutation blocked

Attempt:

    (add-atom &TokenA.state (Balance Attacker USDC 1000000000))

Expected:

    failure: ForbiddenPrimitive or InaccessibleState
    state unchanged
    no events

## Raw state match blocked

Attempt:

    (match &TokenA.state (Balance $who USDC $amount) ($who $amount))

Expected:

    failure or empty result unless an auditor view policy authorizes it

## Missing method policy denied

Attempt:

    (call-contract! tx TokenA undocumentedMethod...)

Expected:

    failure: PolicyMissing or MethodNotExported

## Caller cannot forge identity

Attempt:

    (call-contract! tx TokenA transferFrom ClaimedCaller Alice Bob USDC 10)

Expected:

    runtime ignores claimed caller unless it is a normal method argument
    authorization uses msg_sender from context

## Transfer preserves supply

Initial state:

    Alice: 100
    Bob: 50
    TotalSupply: 150

Call:

    (call-contract! tx TokenA transfer Bob USDC 10)

Expected:

    Alice: 90
    Bob: 60
    TotalSupply: 150
    Transfer event committed

## Insufficient balance reverts

Initial:

    Alice: 5
    Bob: 0

Call:

    (call-contract! tx TokenA transfer Bob USDC 10)

Expected:

    failure: InsufficientBalance
    Alice remains 5
    Bob remains 0
    no Transfer event

## transferFrom requires live registry allowance

Initial registry:

    AllowanceGrant(TokenA, Alice, Dex, USDC) = 100

Call from Dex:

    (call-contract! tx TokenA transferFrom Alice Bob USDC 10)

Expected:

    allowance remaining or spent updated by 10
    Alice debited 10
    Bob credited 10
    all effects atomic

Call from Mallory:

    (call-contract! tx TokenA transferFrom Alice Bob USDC 10)

Expected:

    failure: RegistryGrantMissing
    state unchanged

## Allowance consumption reverts with failed transfer

If allowance is consumed but the transfer later fails, expected:

    allowance unchanged
    balances unchanged
    events discarded

## Method write-scope enforced

`transferFrom` attempts to write:

    RoleGrant(TokenA, Minter, Attacker)

Expected:

    failure: WriteScopeViolation
    full transition reverted

## Reentrancy blocked

Malicious callee reenters `TokenA.transferFrom` during active execution.

Expected:

    failure: ReentrancyViolation
    or allowed only if method has explicit safe reentrancy policy

## Signed permit is only adapter

Call:

    (call-contract! tx TokenA permit Alice Dex USDC 1000 signedPermit)

Expected:

    signature verified
    nonce consumed
    registry allowance created
    Approval event emitted

Later:

    (call-contract! tx TokenA transferFrom Alice Bob USDC 10)

Expected:

    uses live registry lookup, not certificate argument

## Forbidden PeTTa escape hatch blocked

Attempt:

    (py-call...)
    (assertzPredicate...)
    (import!...)

Expected:

    failure: ForbiddenPrimitive
    state unchanged

## Determinism

Same initial state and same transaction run on two replicas.

Expected:

    same result
    same final state root
    same event sequence
    same registry state

# Recommended Implementation Phases and Reference Algorithms

## Implementation phases

### Phase 1 — Minimal secured token

Implement:

- secured contract dispatcher;

- private state storage;

- method policy registry;

- capability registry;

- live registry enforcement;

- `transfer`;

- `approve`;

- `transferFrom`;

- atomic commit/revert;

- event log;

- restricted PeTTa evaluator;

- basic invariants.

### Phase 2 — Method-scoped storage enforcement

Add:

- write-scope derivation;

- storage-level write rejection;

- read/write sets;

- state schema validation;

- canonical state roots.

### Phase 3 — Cross-contract DeFi

Add:

- call stack;

- `tx_sender` / `msg_sender` semantics;

- nested subtransactions;

- reentrancy guards;

- AMM pool contract;

- lending vault contract;

- oracle adapter contract.

### Phase 4 — Governance and upgrades

Add:

- role registry;

- timelocks;

- pause mechanism;

- policy updates;

- upgrade control;

- audit trail.

### Phase 5 — Signed certificate adapters

Add:

- permit;

- signed orders;

- oracle attestations;

- bridge messages;

- session grants;

- certificate nonce registry.

### Phase 6 — Verification tooling

Add:

- property tests;

- symbolic execution;

- invariant checking;

- policy linting;

- write-scope analysis;

- state-root proof generation;

- event-root proof generation.

Formalization note: a companion abstract transition model and proof-obligation
outline is maintained in [scs-formal.md](scs-formal.md).

## Reference dispatcher

    call_contract(tx, contract_id, method, args):
        ctx = authenticate_and_build_context(tx, contract_id, method)

        contract = resolve_contract(contract_id)
        abi_entry = contract.abi.lookup(method)
        require abi_entry.exported

        policy = method_policy_registry.lookup(contract_id, method, args)
        require policy.exists

        begin_subtransaction(ctx)

        auth_result = enforcement.authorize(ctx, policy, args)
        require auth_result.allowed

        ctx.write_scope = auth_result.write_scope
        ctx.registry_delta = auth_result.registry_delta

        result = execute_private_impl(ctx, contract, method, args)

        require invariants_hold(ctx, contract, policy)

        commit_subtransaction(ctx)
        return result

## Reference enforcement module

    authorize(ctx, policy, args):
        principal = derive_principal(ctx, policy.principal)

        for requirement in policy.requires:
            evaluate_static_requirement(requirement, ctx, args)

        for grant_req in policy.registry_requirements:
            grant = registry.lookup(grant_req.key(ctx, args, principal))
            require grant.active
            require grant.rights include grant_req.right
            require grant.scope matches ctx
            require grant.remaining >= grant_req.amount

            if grant_req.consume:
                stage_registry_consumption(grant, grant_req.amount)

        write_scope = derive_write_scope(policy.writes, ctx, args)
        return Authorized(write_scope, staged_registry_delta)

## Reference storage kernel

    state_set(ctx, key, value):
        require ctx.active_authorized_frame
        require key.contract_id == ctx.current_contract
        require key in ctx.write_scope
        require schema_accepts(key, value)
        require kernel_invariants_accept(key, value)
        stage_state_write(key, value)

## Reference registry consumption

    consume_allowance(ctx, token, owner, spender, asset, amount):
        key = AllowanceGrantKey(token, owner, spender, asset)
        grant = registry.lookup(key)

        require grant.active
        require not grant.revoked
        require grant.expiry >= ctx.block_height
        require grant.limit - grant.spent >= amount

        stage_registry_write(key, grant.spent:= grant.spent + amount)

# Final Design Principle

The final SCS model is:

    A smart contract is a secured Module Space.

    Its public surface is a method ABI.
    Its authority model is functional access control.
    Its durable permissions live in a live capability registry.
    Its state is private guarded storage.
    Its methods receive no routine capability arguments.
    Its dispatcher enforces method policy before implementation execution.
    Its storage kernel enforces method-scoped writes during execution.
    Its registry, state, and events commit or revert atomically.

The most important security invariant is:

    No balance changes merely because code can mention a balance atom.
    No balance changes merely because code can mention a capability-shaped term.
    Balances change only through an authorized method whose policy passes live registry enforcement and whose transition satisfies storage and protocol invariants.

For PeTTa, this requires an added contract runtime, guarded storage backend, and restricted evaluator. Ordinary PeTTa spaces and ordinary reflective mutation primitives are too open for adversarial DeFi balances.

# References

PeTTa `spaces.pl` source. https://raw.githubusercontent.com/trueagi-io/PeTTa/main/src/spaces.pl

PeTTa `metta.pl` source. https://raw.githubusercontent.com/trueagi-io/PeTTa/main/src/metta.pl

PeTTa `translator.pl` source. https://raw.githubusercontent.com/trueagi-io/PeTTa/main/src/translator.pl

HyperClaw: A Cognitive Orchestration Layer for the Road to AGI. https://singularitynet.io/hyperclaw-a-cognitive-orchestration-layer-for-the-road-to-agi/

Hyperon: A framework for AGI at the human level and beyond. https://ar5iv.labs.arxiv.org/html/2310.18318

OpenZeppelin Contracts: Access Control. https://docs.openzeppelin.com/contracts/5.x/access-control

Solidity Documentation: Security Considerations. https://docs.soliditylang.org/en/latest/security-considerations.html
