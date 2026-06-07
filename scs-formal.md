# Formal Verification Companion for Secured Contract Spaces

**Status:** Draft companion v0.1
**Applies to:** Secured Contract Spaces draft v0.3
**Format:** Markdown with prover-neutral pseudocode
**Abbreviation:** SCS-FV

# Purpose

This companion turns the prose SCS security architecture into a formal
verification target. It does not replace the runtime specification. It defines
the abstract state machine, safety properties, and implementation proof
obligations that a concrete SCS runtime should refine.

The intended use is:

1. encode this model in TLA+, Lean, Coq, Isabelle, K, or another proof system;
2. prove the generic SCS runtime theorems once;
3. instantiate the model for particular contracts such as tokens, AMMs, lending
   vaults, bridges, or oracles;
4. prove contract-specific invariants over the generic runtime transition
   relation;
5. test implementation traces against the abstract model.

This document intentionally avoids choosing a final MeTTa/PeTTa syntax. The
formal boundary is the runtime transition system.

# Verification Scope

The formal model covers:

- dispatcher-mediated contract calls;
- authenticated transaction context;
- functional access control;
- live registry lookup;
- guarded storage;
- method-scoped writes;
- atomic commit and revert;
- event commit semantics;
- cross-contract caller semantics;
- reentrancy policy;
- deterministic execution;
- contract-specific invariant preservation.

The model abstracts:

- consensus;
- networking;
- cryptographic signature internals;
- physical storage layout;
- gas pricing details;
- the full PeTTa evaluator implementation.

Cryptographic checks are modeled as deterministic predicates. Host execution is
modeled only through the restricted evaluator interface.

# Notation

The notation is prover-neutral.

```text
Set[T]          finite set of T
Seq[T]          finite sequence of T
Map[K, V]       finite partial map from K to V
Option[T]       None or Some(T)
Result[T]       Ok(T) or Err(Error)
TxOutcome       Rejected(Error), Reverted(SystemState, Error), or
                Committed(SystemState, ReturnValue)
UInt            nonnegative bounded integer domain
Bool            true or false
```

All maps use canonical keys. A lookup in a map returns `None` when the key is
absent.

All functions used by consensus execution MUST be deterministic.

# Abstract Sorts

The following sorts are uninterpreted unless constrained by later sections.

```text
ContractId
CodeHash
MethodName
Selector
Principal
AssetId
StateKey
StateValue
GrantKey
GrantRight
Event
Error
TxHash
Nonce
ChainId
BlockHeight
Timestamp
Budget
ReturnValue
Argument
Certificate
Hash
PolicyKey
TypeSpec
StateSchema
Invariant
InvariantId
ReentrancyPolicy
PrincipalRule
StaticRequirement
RegistryRequirement
ScopeExpr
RegistryEffect
EventSpec
GrantScope
StateKeyKind
LockKey
LockKeyExpr
```

# Core Records

## Contract Record

```text
ContractRecord =
  { contract_id       : ContractId
  , code_hash         : CodeHash
  , abi               : Map[Selector, ABIEntry]
  , policies          : Map[PolicyKey, MethodPolicy]
  , state_schema      : StateSchema
  , invariants        : Set[Invariant]
  , reentrancy_policy : ReentrancyPolicy
  }
```

## ABI Entry

```text
ABIEntry =
  { selector      : Selector
  , method        : MethodName
  , exported      : Bool
  , view          : Bool
  , argument_type : Seq[TypeSpec]
  , return_type   : TypeSpec
  }
```

## System State

`SystemState` is the state observed by the SCS transition relation.

```text
SystemState =
  { chain_id       : ChainId
  , block_height   : BlockHeight
  , timestamp      : Timestamp
  , contracts      : Map[ContractId, ContractRecord]
  , storage        : Map[StateKey, StateValue]
  , registry       : Map[GrantKey, Grant]
  , committed_log  : Seq[Event]
  , used_nonces    : Set[(Principal, Nonce)]
  , active_locks   : Set[LockKey]
  }
```

`used_nonces` represents the transaction admission anti-replay layer. A concrete
chain may charge fees or consume envelope nonces even when a contract call
reverts. Contract state, registry state, and events still obey SCS atomicity.

## Transaction

```text
Transaction =
  { tx_hash      : TxHash
  , chain_id     : ChainId
  , sender       : Principal
  , nonce        : Nonce
  , target       : ContractId
  , selector     : Selector
  , args         : Seq[Argument]
  , signature_ok : Bool
  , budget       : Budget
  }
```

## Execution Context

The runtime creates this context. Contract code cannot forge it or mutate it.

```text
Context =
  { chain_id        : ChainId
  , block_height    : BlockHeight
  , timestamp       : Timestamp
  , tx_hash         : TxHash
  , tx_sender       : Principal
  , msg_sender      : Principal
  , current_contract: ContractId
  , current_method  : MethodName
  , call_depth      : UInt
  , call_stack      : Seq[CallFrame]
  , nonce           : Nonce
  , budget          : Budget
  , read_set        : Set[StateKey]
  , write_set       : Set[StateKey]
  , registry_reads  : Set[GrantKey]
  , registry_delta  : Map[GrantKey, Grant]
  , event_buffer    : Seq[Event]
  , locks_held      : Set[LockKey]
  , auth_result     : Option[AuthorizationResult]
  }
```

## Call Frame

```text
CallFrame =
  { contract    : ContractId
  , method      : MethodName
  , msg_sender  : Principal
  , write_scope : Set[StateKey]
  }
```

## Authorization Result

```text
AuthorizationResult =
  { allowed        : Bool
  , principal      : Principal
  , read_scope     : Set[StateKey]
  , write_scope    : Set[StateKey]
  , registry_delta : Map[GrantKey, Grant]
  }
```

## Authorized Frame

```text
AuthorizedFrame =
  { authorized  : Bool
  , ctx         : Context
  , contract    : ContractId
  , method      : MethodName
  , principal   : Principal
  , read_scope  : Set[StateKey]
  , write_scope : Set[StateKey]
  }
```

## Staged Transition

```text
StagedTransition =
  { state_delta    : Map[StateKey, StateValue]
  , registry_delta : Map[GrantKey, Grant]
  , event_delta    : Seq[Event]
  , locks_delta    : Set[LockKey]
  }
```

## Method Policy

```text
MethodPolicy =
  { method                : MethodName
  , principal_rule        : PrincipalRule
  , static_requirements   : Seq[StaticRequirement]
  , registry_requirements : Seq[RegistryRequirement]
  , read_scope_expr       : ScopeExpr
  , write_scope_expr      : ScopeExpr
  , registry_effects      : Seq[RegistryEffect]
  , event_effects         : Seq[EventSpec]
  , invariants_required   : Set[InvariantId]
  , reentrancy            : ReentrancyMode
  }
```

The policy language MUST be machine-checkable. Every expression in a policy MUST
be total and deterministic over `(state, context, args)`.

## Grant

```text
Grant =
  { key          : GrantKey
  , issuer       : Principal
  , subject      : Principal
  , rights       : Set[GrantRight]
  , scope        : GrantScope
  , limit        : Option[UInt]
  , spent        : UInt
  , active       : Bool
  , revoked      : Bool
  , valid_from   : BlockHeight
  , valid_until  : Option[BlockHeight]
  }
```

A grant is usable only when `GrantUsable(state, ctx, grant, req)` holds.

```text
GrantUsable(state, ctx, grant, req) =
  grant.active = true
  and grant.revoked = false
  and req.right in grant.rights
  and ScopeMatches(grant.scope, ctx, req)
  and ctx.block_height >= grant.valid_from
  and NotExpired(ctx.block_height, grant.valid_until)
  and Remaining(grant) >= req.amount
```

```text
Remaining(grant) =
  if grant.limit = None then MAX_UINT
  else grant.limit.value - grant.spent
```

# State Key Discipline

Every state key MUST carry its owning contract.

```text
OwnerOfKey : StateKey -> ContractId
KindOfKey  : StateKey -> StateKeyKind
```

Canonical token examples:

```text
BalanceKey(token: ContractId, owner: Principal, asset: AssetId) : StateKey
TotalSupplyKey(token: ContractId, asset: AssetId)              : StateKey
ReserveKey(pool: ContractId, asset: AssetId)                   : StateKey
DebtKey(vault: ContractId, borrower: Principal, asset: AssetId): StateKey
```

Canonical registry examples:

```text
AllowanceGrantKey(token, owner, spender, asset) : GrantKey
RoleGrantKey(contract, role, subject)           : GrantKey
OracleUpdaterKey(oracle, subject)               : GrantKey
BridgeValidatorKey(bridge, subject)             : GrantKey
```

For every logical record, there MUST be exactly one canonical key constructor.

# Views and Staged State

During execution, reads observe staged writes made earlier in the same active
transaction.

```text
StorageView(state, staged_state, key) =
  if key in Domain(staged_state) then staged_state[key]
  else state.storage[key]
```

```text
RegistryView(state, staged_registry, key) =
  if key in Domain(staged_registry) then staged_registry[key]
  else state.registry[key]
```

This gives read-your-writes behavior without exposing raw mutable handles.

# Top-Level Transition

The top-level transition has two phases:

1. transaction admission;
2. atomic SCS execution.

```text
Step(state, tx) : TxOutcome =
  match Admit(state, tx):
    Err(error) ->
      Rejected(error)

    Ok(admitted_state) ->
      match ExecuteExternalCall(admitted_state, tx):
        Ok(next_state, return_value) ->
          Committed(next_state, return_value)

        Err(error) ->
          Reverted(admitted_state, error)
```

`Admit` authenticates the external sender and prevents transaction replay.

```text
Admit(state, tx) =
  require tx.chain_id = state.chain_id
  require tx.signature_ok = true
  require (tx.sender, tx.nonce) notin state.used_nonces

  Ok(state with
    used_nonces := state.used_nonces union {(tx.sender, tx.nonce)})
```

If admission fails, no part of `SystemState` changes. If admission succeeds but
SCS execution reverts, only admission-level replay metadata is guaranteed to
remain changed. Contract storage, capability registry, and committed events MUST
remain unchanged by the reverted execution.

# External Call Semantics

```text
ExecuteExternalCall(state, tx) : Result[(SystemState, ReturnValue)] =
  let contract = LookupContract(state, tx.target)
  let abi_entry = LookupABI(contract, tx.selector)

  require abi_entry.exported = true
  require TypeCheck(abi_entry.argument_type, tx.args)

  let ctx =
    BuildContext(state, tx,
      msg_sender       = tx.sender,
      current_contract = tx.target,
      current_method   = abi_entry.method,
      call_depth       = 0,
      call_stack       = [])

  ExecuteMethodAtomic(state, ctx, contract, abi_entry.method, tx.args)
```

`BuildContext` is a runtime function. No user term is accepted as caller
identity.

# Atomic Method Semantics

```text
ExecuteMethodAtomic(state, ctx, contract, method, args)
  : Result[(SystemState, ReturnValue)] =
  let policy = LookupPolicy(contract, method, args)
  require policy exists
  require ReentrancyAllowed(state, ctx, policy)

  let auth = Authorize(state, ctx, policy, args)
  require auth.allowed = true

  let frame =
    AuthorizedFrame(
      authorized  = true,
      ctx         = ctx with auth_result := Some(auth),
      contract    = contract.contract_id,
      method      = method,
      principal   = auth.principal,
      write_scope = auth.write_scope,
      read_scope  = auth.read_scope)

  let staged0 =
    StagedTransition(
      state_delta    = EmptyMap,
      registry_delta = auth.registry_delta,
      event_delta    = EmptySeq,
      locks_delta    = LocksFor(policy, ctx))

  let exec_result = EvalPrivateImpl(state, staged0, frame, args)
  require exec_result = Ok(staged1, return_value)

  require InvariantsHold(state, staged1, contract, policy)
  require EventsAuthorized(staged1.event_delta, policy, frame)

  Ok(Commit(state, staged1), return_value)
```

Every `require` failure after admission is an SCS revert. A revert discards
`state_delta`, `registry_delta`, and `event_delta`.

# Authorization Semantics

```text
Authorize(state, ctx, policy, args) =
  let principal = DerivePrincipal(ctx, policy.principal_rule)

  require StaticRequirementsHold(state, ctx, args, policy.static_requirements)

  let staged_registry = EmptyMap

  for req in policy.registry_requirements:
    let key = CanonicalGrantKey(req, ctx, args, principal)
    let grant = RegistryView(state, staged_registry, key)

    require grant exists
    require GrantUsable(state, ctx, grant, req)

    if req.consume = true:
      staged_registry[key] = ConsumeGrant(grant, req.amount)

  let read_scope  = DeriveReadScope(policy.read_scope_expr, state, ctx, args)
  let write_scope = DeriveWriteScope(policy.write_scope_expr, state, ctx, args)

  Ok(AuthorizationResult(
    allowed        = true,
    principal      = principal,
    read_scope     = read_scope,
    write_scope    = write_scope,
    registry_delta = staged_registry))
```

Authorization never succeeds from the syntactic presence of a capability-shaped
term in `args`. Authority comes from runtime context, method policy, and live
registry state.

# Storage Kernel Semantics

The private implementation cannot modify `state_delta` directly. It can only
request kernel operations.

```text
StateSet(state, staged, frame, key, value) =
  require frame.authorized = true
  require OwnerOfKey(key) = frame.contract
  require key in frame.write_scope
  require SchemaAccepts(frame.contract, key, value)
  require KernelInvariantsAccept(state, staged, key, value)

  Ok(staged with
    state_delta := staged.state_delta[key := value])
```

```text
StateGet(state, staged, frame, key) =
  require OwnerOfKey(key) = frame.contract
       or key in frame.read_scope

  Ok(StorageView(state, staged.state_delta, key))
```

There is no formal transition corresponding to raw `add-atom`, raw `remove`, raw
match over private state, Prolog assertion, Python interop, filesystem access, or
network access. Such operations are outside the restricted evaluator language.

# Registry Kernel Semantics

Registry mutation is separate from storage mutation.

```text
RegistrySet(state, staged, frame, key, grant) =
  require frame.authorized = true
  require RegistryWriteAuthorized(frame, key, grant)
  require RegistrySchemaAccepts(key, grant)
  require RegistryInvariantsAccept(state, staged, key, grant)

  Ok(staged with
    registry_delta := staged.registry_delta[key := grant])
```

Registry reads used for authorization are made at use time against
`RegistryView`. A conforming implementation MUST NOT authorize from stale
off-chain caches or caller-supplied certificates except through explicit
certificate adapter methods.

# Commit Semantics

```text
Commit(state, staged) =
  state with
    storage       := ApplyMapDelta(state.storage, staged.state_delta)
    registry      := ApplyMapDelta(state.registry, staged.registry_delta)
    committed_log := state.committed_log concat staged.event_delta
    active_locks  := ReleaseLocks(state.active_locks, staged.locks_delta)
```

`Commit` is total and deterministic when all preconditions of
`ExecuteMethodAtomic` hold.

# Cross-Contract Calls

A private implementation may request a dispatcher-mediated subcall.

```text
CrossCall(state, staged_parent, caller_frame: AuthorizedFrame,
          callee, selector, args) =
  let callee_contract = LookupContract(state, callee)
  let abi_entry = LookupABI(callee_contract, selector)

  require abi_entry.exported = true
  require TypeCheck(abi_entry.argument_type, args)

  let child_ctx =
    caller_frame.ctx with
      msg_sender       := caller_frame.contract
      current_contract := callee
      current_method   := abi_entry.method
      call_depth       := caller_frame.ctx.call_depth + 1
      call_stack       := Push(caller_frame.ctx.call_stack, caller_frame)

  ExecuteMethodAtomicOnStagedView(state, staged_parent, child_ctx,
                                  callee_contract, abi_entry.method, args)
```

The child call receives its own authorization result and its own write scope.
The caller's write scope is never inherited by the callee, and the callee's
write scope is never inherited by the caller.

Nested effects commit into the parent staged transition only if the child call
succeeds. If the child call reverts and the parent does not have an explicit
safe handling rule, the parent reverts.

# Reentrancy Semantics

Each method policy declares a reentrancy mode.

```text
ReentrancyMode =
  NoReentry
  | ContractReentryAllowed
  | MethodReentryAllowed
  | CustomLocks(Set[LockKeyExpr])
```

Default mode is `NoReentry`.

```text
ReentrancyAllowed(state, ctx, policy) =
  let requested = LocksFor(policy, ctx)
  requested intersection state.active_locks = EmptySet
```

During method evaluation, active locks are interpreted as:

```text
ActiveLocksView(state, staged) =
  state.active_locks union staged.locks_delta
```

Nested calls check reentrancy against `ActiveLocksView`, so a child call cannot
ignore locks held by its parent transition.

A concrete implementation may use finer locks, but it must refine this abstract
lock discipline or prove an equivalent noninterference property.

# Restricted Evaluator Contract

The evaluator is modeled as a deterministic trace-producing function.

```text
EvalPrivateImpl(state, staged, frame, args)
  : Result[(StagedTransition, ReturnValue)]
```

The evaluator MUST satisfy:

1. determinism: same inputs produce the same result;
2. kernel mediation: all storage writes go through `StateSet`;
3. registry mediation: all registry writes go through `RegistrySet`;
4. no raw private-state enumeration;
5. no mutation of runtime context;
6. no host nondeterminism;
7. no host side effects outside the SCS transition;
8. bounded execution.

For verification, the implementation can expose a kernel trace:

```text
KernelTrace =
  Seq[
    StateGet(key)
    | StateSet(key, value)
    | RegistryGet(key)
    | RegistrySet(key, grant)
    | Emit(event)
    | CrossCall(callee, selector, args)
  ]
```

Trace validation checks that every trace event is accepted by the abstract
kernel and that the final concrete delta equals the abstract staged delta.

# Generic Safety Theorems

The following theorems are the core proof obligations for the SCS runtime.

## THM-001 Dispatcher-Only External Mutation

For any admitted transaction `tx`, if `Step(state, tx)` returns
`Committed(next_state, return_value)` and contract storage changes between
`state` and `next_state`, then `tx` executed through `ExecuteExternalCall`, the
target ABI entry was exported, a method policy existed, and authorization
succeeded.

## THM-002 Contract Isolation

For any successful method execution with `current_contract = c`, every changed
storage key `k` satisfies:

```text
OwnerOfKey(k) = c
```

unless the changed key belongs to a separately authorized secured shared
resource.

## THM-003 Method Write-Scope Safety

For any successful method execution and every changed storage key `k`:

```text
k in AuthorizationResult.write_scope
```

## THM-004 No Authority From Syntax

If two calls differ only by replacing a capability-shaped argument with another
capability-shaped argument, and the method policy does not designate that
argument as an explicit certificate adapter input, then authorization results
are equal.

## THM-005 Live Registry Authorization

For every registry-backed authorization success, there exists a grant in
`RegistryView` at authorization time such that `GrantUsable` holds for the
required right, principal, scope, and amount.

## THM-006 Registry Consumption Atomicity

If a method stages registry consumption and later reverts, the committed
registry is unchanged by that method. If the method commits, the registry
consumption and all related storage writes commit together.

## THM-007 Event Atomicity

Events emitted by a reverted method are absent from `committed_log`. Events
emitted by a committed method appear in deterministic order after all prior
committed events.

## THM-008 Invariant Preservation

If `GlobalInvariants(state)` holds before a successful transition and the
transition commits, then `GlobalInvariants(next_state)` holds.

Contract-specific invariants are proved as instances of this theorem.

## THM-009 Determinism

For any `state` and `tx`, two conforming replicas executing `Step(state, tx)`
produce the same result, storage root, registry root, event sequence, and
admission replay state.

## THM-010 Replay Safety

If `Step(state, tx)` admits `tx`, then any later state containing
`(tx.sender, tx.nonce)` in `used_nonces` rejects the same transaction nonce at
admission.

## THM-011 Caller Integrity

For every method execution, `tx_sender`, `msg_sender`, `current_contract`,
`current_method`, `call_depth`, and `call_stack` are generated by the runtime
transition relation and cannot be chosen by contract code.

## THM-012 No Write-Scope Leakage Across Calls

For any cross-contract call from `A` to `B`, the child frame for `B` is
authorized against `B`'s policy, and no key owned by `A` can be written by `B`
unless that key belongs to an explicitly modeled secured shared resource.

## THM-013 View Read-Only Safety

If an ABI entry is marked `view = true`, then a successful call to it produces
no committed storage delta, registry delta, nonce delta other than admission
metadata, or event delta.

## THM-014 Schema Safety

For every committed storage key/value pair, `SchemaAccepts` holds. In
particular, UInt fields are nonnegative and bounded.

## THM-015 Raw Primitive Exclusion

No transition reachable through the restricted evaluator corresponds to raw
private storage mutation, raw private storage enumeration, Prolog assertion,
Python interop, filesystem access, process execution, or network access.

# Contract-Specific Token Instance

This section gives a minimal token instantiation suitable for mechanized proof.

## Token State

```text
Balance(token, owner, asset) : UInt
TotalSupply(token, asset)    : UInt
```

The canonical storage keys are:

```text
BalanceKey(token, owner, asset)
TotalSupplyKey(token, asset)
```

## Token Invariants

```text
TokenSupplyInvariant(state, token, asset) =
  Value(state, TotalSupplyKey(token, asset))
  =
  Sum({ Value(state, BalanceKey(token, owner, asset))
        | owner in KnownOwners(state, token, asset) })
```

```text
TokenNonnegativeInvariant(state, token, asset) =
  for all owner:
    Value(state, BalanceKey(token, owner, asset)) >= 0
  and
    Value(state, TotalSupplyKey(token, asset)) >= 0
```

In bounded-integer systems, every arithmetic operation MUST either prove no
overflow or revert before commit.

## transfer

Call shape:

```text
transfer(to, asset, amount)
```

Derived values:

```text
from = ctx.msg_sender
```

Policy:

```text
principal_rule      = MsgSender
static_requirements = [amount > 0]
registry_requirements = []
write_scope =
  { BalanceKey(token, from, asset)
  , BalanceKey(token, to, asset)
  }
invariants_required =
  { TokenSupplyInvariant(token, asset)
  , TokenNonnegativeInvariant(token, asset)
  }
```

Implementation precondition:

```text
Balance(from, asset) >= amount
```

Postcondition on success:

```text
Balance'(from, asset) = Balance(from, asset) - amount
Balance'(to, asset)   = Balance(to, asset) + amount
TotalSupply'(asset)   = TotalSupply(asset)
```

## approve

Call shape:

```text
approve(spender, asset, amount)
```

Derived values:

```text
owner = ctx.msg_sender
key   = AllowanceGrantKey(token, owner, spender, asset)
```

Policy:

```text
principal_rule      = MsgSender
static_requirements = [amount >= 0]
registry_effects    = [SetAllowanceGrant(key, amount)]
write_scope         = EmptySet
```

Postcondition on success:

```text
Registry'(key).subject = spender
Registry'(key).rights includes SpendAllowance
Registry'(key).limit = Some(amount)
Registry'(key).spent = 0
Registry'(key).active = true
Registry'(key).revoked = false
```

## transferFrom

Call shape:

```text
transferFrom(owner, to, asset, amount)
```

Derived values:

```text
spender = ctx.msg_sender
key     = AllowanceGrantKey(token, owner, spender, asset)
```

Policy:

```text
principal_rule = MsgSender
static_requirements = [amount > 0]
registry_requirements =
  [ RequireGrant(
      key = key,
      right = SpendAllowance,
      amount = amount,
      consume = true) ]
write_scope =
  { BalanceKey(token, owner, asset)
  , BalanceKey(token, to, asset)
  }
invariants_required =
  { TokenSupplyInvariant(token, asset)
  , TokenNonnegativeInvariant(token, asset)
  }
```

Implementation precondition:

```text
Balance(owner, asset) >= amount
```

Postcondition on success:

```text
Balance'(owner, asset) = Balance(owner, asset) - amount
Balance'(to, asset)    = Balance(to, asset) + amount
TotalSupply'(asset)    = TotalSupply(asset)
Registry'(key).spent   = Registry(key).spent + amount
```

Postcondition on revert:

```text
Balance'(owner, asset) = Balance(owner, asset)
Balance'(to, asset)    = Balance(to, asset)
Registry'(key)         = Registry(key)
No Transfer event is committed
```

## permit

Call shape:

```text
permit(owner, spender, asset, amount, certificate)
```

`permit` is a certificate adapter. The certificate proves the owner's intent,
but it does not directly authorize storage writes.

Policy:

```text
principal_rule = CertificateIssuer(certificate)
static_requirements =
  [ VerifyCertificate(certificate)
  , CertificateDomain(certificate) = (chain_id, token, "permit")
  , CertificateNonceUnused(certificate)
  , CertificateNotExpired(certificate, ctx.block_height)
  , CertificateOwner(certificate) = owner
  , CertificateSpender(certificate) = spender
  , CertificateAsset(certificate) = asset
  , CertificateAmount(certificate) = amount
  ]
registry_effects =
  [ ConsumeCertificateNonce(certificate)
  , SetAllowanceGrant(AllowanceGrantKey(token, owner, spender, asset), amount)
  ]
write_scope = EmptySet
```

Safety property:

```text
permit can create or update a live registry grant.
permit cannot debit or credit balances.
transferFrom later uses live registry lookup, not the certificate argument.
```

# AMM Proof Obligations

An AMM contract instance should add the following contract-specific obligations:

```text
ReserveNonnegative(pool, asset)
LPShareNonnegative(pool, provider)
PoolAccountingMatchesTokenTransfers(pool)
SwapRespectsPricingFormula(pool, input_asset, output_asset)
SlippageBoundEnforced(pool, min_output)
FeesAccounted(pool)
```

The exact pricing theorem depends on the AMM formula. For constant-product
markets, the model MUST specify integer rounding and fee order before proofs can
be meaningful.

# Lending Proof Obligations

A lending vault instance should add:

```text
DebtNonnegative(vault, borrower, asset)
CollateralNonnegative(vault, borrower, asset)
BorrowRespectsLTV(vault, borrower)
OraclePriceFresh(vault, asset)
LiquidationDoesNotDecreaseSolvency(vault, borrower)
ProtocolReservesNonnegative(vault)
```

The proof must define price domains, decimal scaling, rounding, liquidation
discounts, and stale-price behavior.

# Oracle Proof Obligations

An oracle adapter should add:

```text
UpdaterAuthorized(oracle, updater)
TimestampFresh(oracle, update)
DeviationWithinBound(oracle, update)
QuorumSatisfied(oracle, update)
CommittedPriceCanonical(oracle, asset)
```

# Implementation Refinement Obligations

A concrete runtime conforms to this formal model only if it proves or tests the
following refinement obligations.

## REF-001 Canonical Serialization

Every key, policy, grant, event, and state value has exactly one consensus
serialization. Equal logical values serialize identically. Distinct logical keys
do not collide.

## REF-002 Dispatcher Completeness

Every external contract entrypoint goes through the dispatcher. There is no
alternate public path to private implementation functions.

## REF-003 State Handle Non-Exposure

No untrusted code can obtain a handle that bypasses `StateGet` and `StateSet`.

## REF-004 Registry Handle Non-Exposure

No untrusted code can mutate grants without going through authorization and the
registry kernel.

## REF-005 Evaluator Subset Enforcement

The deployed contract evaluator exposes only the allowed primitive set. Forbidden
primitives are absent or trapped before execution.

## REF-006 Kernel Trace Soundness

For every concrete execution trace, the abstract kernel accepts the same
sequence of operations and produces the same staged deltas.

## REF-007 Atomic Storage Backend

The physical storage backend either commits the complete staged transition or
commits none of it.

## REF-008 Deterministic Host Boundary

All host-provided values used by contracts are consensus values in `Context`.
Filesystem, process, network, wall-clock, randomness, and foreign-language calls
are unavailable unless modeled as deterministic oracle contracts.

## REF-009 Error Noninterference

Error values, traces, logs, and events do not reveal private state outside the
authorized view policy.

## REF-010 Arithmetic Semantics

All arithmetic used by contract proofs has explicit boundedness, overflow,
division, rounding, and decimal-scaling semantics.

# Suggested Machine Encodings

## TLA+

Use TLA+ to model:

```text
VARIABLES storage, registry, committed_log, used_nonces, active_locks

Init ==
  TypeOK /\ GlobalInvariants

Next ==
  exists tx:
    Step(storage, registry, committed_log, used_nonces, active_locks, tx)

Spec ==
  Init /\ [][Next]_vars
```

Recommended properties:

```text
TypeOK
AuthorizationSafety
WriteScopeSafety
Atomicity
ReplaySafety
NoReentrancyViolation
TokenSupplyInvariant
```

## Lean or Coq

Use an inductive transition relation:

```text
inductive step : state -> transaction -> result state -> Prop
```

Prove preservation:

```text
theorem invariant_preservation :
  forall s tx s',
    GlobalInvariants s ->
    step s tx (Ok s') ->
    GlobalInvariants s'
```

Prove storage safety:

```text
theorem write_scope_safety :
  forall s tx s' k,
    step s tx (Ok s') ->
    storage_changed s s' k ->
    exists frame,
      authorized_frame frame /\
      k in frame.write_scope /\
      OwnerOfKey k = frame.contract
```

## Symbolic Execution

Use symbolic execution for contract implementations against the kernel API:

1. symbolically execute the private implementation;
2. collect kernel traces;
3. assert every `StateSet` key is in the derived write scope;
4. assert all revert paths discard staged effects;
5. assert postconditions for successful paths;
6. assert invariant functions hold before commit.

# Minimal Verification Milestone

A minimal formally verified SCS token requires:

1. a mechanized model of `SystemState`, `Transaction`, `Context`, `Policy`,
   `Grant`, and `StateKey`;
2. a mechanized `Step` relation;
3. proofs of THM-001 through THM-015 for the runtime model;
4. token instantiation for `transfer`, `approve`, `transferFrom`, and `permit`;
5. proofs of token supply preservation, nonnegative balances, live allowance
   consumption, permit nonce replay protection, and atomic revert behavior;
6. trace conformance tests from the implementation to the abstract model.

# Open Formalization Decisions

The following decisions must be fixed before a complete mechanized proof:

- exact finite integer widths;
- overflow behavior;
- account and principal canonicalization;
- event root construction;
- storage root construction;
- policy expression grammar;
- invariant expression grammar;
- cross-contract failure handling;
- whether failed admitted transactions consume only envelope nonces or also
  contract-visible nonces;
- exact view authorization and private-state noninterference model;
- exact liveness or progress properties, if any.

# Final Verification Principle

The SCS runtime should be verified as a small trusted transition kernel. Contract
code should be verified as producing only traces accepted by that kernel.

The central theorem remains:

```text
No balance or registry authority changes merely because code can mention the
right atom. Durable effects happen only through dispatcher-created,
policy-authorized, kernel-mediated, invariant-preserving, atomic transitions.
```
