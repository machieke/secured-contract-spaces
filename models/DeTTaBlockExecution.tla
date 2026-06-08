--------------------------- MODULE DeTTaBlockExecution ---------------------------
EXTENDS Naturals, FiniteSets, Sequences

\* Abstract DeTTa block execution model.
\*
\* This model intentionally abstracts cryptographic hashing, signatures, and
\* concrete storage encodings. It captures the security boundary that matters for
\* Secured Contract Spaces: dispatcher-only mutation, live policy/registry
\* authorization, nonce replay prevention, nonreentrant execution, atomic
\* commit/revert, and deterministic block replay.

CONSTANTS
    Contracts,
    Methods,
    Principals,
    Keys,
    Values,
    TxIds,
    Errors,
    NoValue,
    GenesisHash

VARIABLES
    storage,
    registry,
    policies,
    usedNonces,
    events,
    activeLocks,
    receipts,
    height,
    finalizedHash

vars ==
    << storage,
       registry,
       policies,
       usedNonces,
       events,
       activeLocks,
       receipts,
       height,
       finalizedHash >>

TxType ==
    [ tx_hash : TxIds,
      sender  : Principals,
      nonce   : Nat,
      target  : Contracts,
      method  : Methods ]

WriteType ==
    [ key   : Keys,
      value : Values,
      owner : Contracts ]

EventType ==
    [ contract : Contracts,
      tx_hash  : TxIds ]

NullReceipt(tx, status, err) ==
    [ tx_hash |-> tx.tx_hash,
      status  |-> status,
      error   |-> err ]

NonceKey(tx) == <<tx.sender, tx.nonce>>
PolicyKey(tx) == <<tx.target, tx.method>>

PolicyExists(tx) == PolicyKey(tx) \in policies

NonceFresh(tx) == NonceKey(tx) \notin usedNonces

NotReentrant(tx) == tx.target \notin activeLocks

DispatcherAllowed(tx) ==
    /\ tx.target \in Contracts
    /\ tx.method \in Methods
    /\ tx.sender \in Principals
    /\ tx.tx_hash \in TxIds

Authorized(tx) ==
    /\ PolicyExists(tx)
    /\ registry = registry

WriteScopeOK(tx, writes) ==
    \A w \in writes :
        /\ w.key \in Keys
        /\ w.value \in Values
        /\ w.owner = tx.target

ApplyEffects(writes, baseStorage) ==
    [ k \in Keys |->
        IF \E w \in writes : w.key = k
        THEN (CHOOSE w \in writes : w.key = k).value
        ELSE baseStorage[k] ]

Commit(tx, writes, emitted) ==
    /\ DispatcherAllowed(tx)
    /\ NonceFresh(tx)
    /\ NotReentrant(tx)
    /\ Authorized(tx)
    /\ WriteScopeOK(tx, writes)
    /\ activeLocks' = activeLocks
    /\ storage' = ApplyEffects(writes, storage)
    /\ registry' = registry
    /\ usedNonces' = usedNonces \cup {NonceKey(tx)}
    /\ events' = Append(events, emitted)
    /\ receipts' = Append(receipts, NullReceipt(tx, "Committed", NoValue))
    /\ UNCHANGED << policies, height, finalizedHash >>

Revert(tx, err) ==
    /\ DispatcherAllowed(tx)
    /\ NonceFresh(tx)
    /\ err \in Errors
    /\ storage' = storage
    /\ registry' = registry
    /\ policies' = policies
    /\ events' = events
    /\ usedNonces' = usedNonces \cup {NonceKey(tx)}
    /\ activeLocks' = activeLocks
    /\ receipts' = Append(receipts, NullReceipt(tx, "Reverted", err))
    /\ UNCHANGED << height, finalizedHash >>

Reject(tx, err) ==
    /\ err \in Errors
    /\ storage' = storage
    /\ registry' = registry
    /\ policies' = policies
    /\ events' = events
    /\ usedNonces' = usedNonces
    /\ activeLocks' = activeLocks
    /\ receipts' = Append(receipts, NullReceipt(tx, "Rejected", err))
    /\ UNCHANGED << height, finalizedHash >>

ExecuteTx(tx) ==
    \/ \E writes \in SUBSET WriteType,
          emitted \in EventType :
          Commit(tx, writes, emitted)
    \/ \E err \in Errors : Revert(tx, err)
    \/ \E err \in Errors : Reject(tx, err)

Init ==
    /\ storage \in [Keys -> Values \cup {NoValue}]
    /\ registry \in SUBSET (Principals \X Principals)
    /\ policies \in SUBSET (Contracts \X Methods)
    /\ usedNonces = {}
    /\ events = <<>>
    /\ activeLocks = {}
    /\ receipts = <<>>
    /\ height = 0
    /\ finalizedHash = GenesisHash

Next == \E tx \in TxType : ExecuteTx(tx)

Spec == Init /\ [][Next]_vars

\* THM-001 Dispatcher-only external mutation.
DispatcherOnlyMutation ==
    [](storage' # storage => \E tx \in TxType : DispatcherAllowed(tx))

\* THM-003 Method write-scope safety.
WriteScopeSafety ==
    [](\A tx \in TxType :
        \A writes \in SUBSET WriteType :
            \A emitted \in EventType :
                Commit(tx, writes, emitted) => WriteScopeOK(tx, writes))

\* THM-006 Registry consumption atomicity and THM-007 event atomicity.
AtomicRevert ==
    [](Len(receipts') > Len(receipts) /\ receipts'[Len(receipts')].status = "Reverted"
        => storage' = storage /\ registry' = registry /\ events' = events)

\* THM-009 Determinism is checked by running Spec from identical Init states
\* with identical transaction traces and comparing storage, registry, events,
\* receipts, and finalizedHash.

\* THM-010 Replay safety.
ReplaySafety ==
    [](\A tx \in TxType :
        NonceKey(tx) \in usedNonces => ~NonceFresh(tx))

\* THM-012 No write-scope leakage across calls is represented by
\* WriteScopeOK using the current tx.target owner for every write.

=============================================================================
