------------------------- MODULE DeTTaDataAvailability -------------------------
EXTENDS Naturals, FiniteSets

\* Abstract DeTTa data-availability model.
\*
\* This model intentionally abstracts cryptographic hashing, signatures,
\* erasure-code arithmetic, and concrete wire formats. It captures the DA
\* safety boundary required by the runtime: validators can vote only after
\* validating assigned custody shares, certificates require quorum signatures
\* for the committed manifest, reconstructed payloads require threshold-valid
\* shares, and finalized blocks must be replayable from the DA payload roots.

CONSTANTS
    Block,
    GoodManifest,
    BadManifest,
    GoodPayload,
    BadPayload,
    GoodShareA,
    GoodShareB,
    BadShare,
    V1,
    V2,
    V3,
    TxRootGood,
    TxRootBad,
    ExecutionRootGood,
    ExecutionRootBad,
    Quorum,
    Threshold

Blocks == {Block}
Manifests == {GoodManifest, BadManifest}
Payloads == {GoodPayload, BadPayload}
Shares == {GoodShareA, GoodShareB, BadShare}
Validators == {V1, V2, V3}
TxRoots == {TxRootGood, TxRootBad}
ExecutionRoots == {ExecutionRootGood, ExecutionRootBad}

VARIABLES
    activeValidators,
    blockManifest,
    blockPayload,
    blockTxRoot,
    blockExecutionRoot,
    manifestPayload,
    payloadTxRoot,
    payloadExecutionRoot,
    shareManifest,
    sharePayload,
    validShares,
    assignedShares,
    custodyValidated,
    daVotes,
    daCertificates,
    reconstructions,
    finalizedBlocks

vars ==
    << activeValidators,
       blockManifest,
       blockPayload,
       blockTxRoot,
       blockExecutionRoot,
       manifestPayload,
       payloadTxRoot,
       payloadExecutionRoot,
       shareManifest,
       sharePayload,
       validShares,
       assignedShares,
       custodyValidated,
       daVotes,
       daCertificates,
       reconstructions,
       finalizedBlocks >>

VoteType ==
    [ validator : Validators,
      block     : Blocks,
      manifest  : Manifests ]

CertificateType ==
    [ block    : Blocks,
      manifest : Manifests,
      signers  : SUBSET Validators ]

ReconstructionType ==
    [ block    : Blocks,
      manifest : Manifests,
      payload  : Payloads,
      shares   : SUBSET Shares ]

ValidatorTypeOK == activeValidators \subseteq Validators
BlockTypeOK ==
    /\ blockManifest \in [Blocks -> Manifests]
    /\ blockPayload \in [Blocks -> Payloads]
    /\ blockTxRoot \in [Blocks -> TxRoots]
    /\ blockExecutionRoot \in [Blocks -> ExecutionRoots]
PayloadTypeOK ==
    /\ manifestPayload \in [Manifests -> Payloads]
    /\ payloadTxRoot \in [Payloads -> TxRoots]
    /\ payloadExecutionRoot \in [Payloads -> ExecutionRoots]
ShareTypeOK ==
    /\ shareManifest \in [Shares -> Manifests]
    /\ sharePayload \in [Shares -> Payloads]
    /\ validShares \subseteq Shares
    /\ assignedShares \in [Validators -> SUBSET Shares]
DAEvidenceTypeOK ==
    /\ custodyValidated \subseteq (Validators \X Manifests)
    /\ daVotes \subseteq VoteType
    /\ daCertificates \subseteq CertificateType
    /\ reconstructions \subseteq ReconstructionType
    /\ finalizedBlocks \subseteq Blocks

TypeOK ==
    /\ ValidatorTypeOK
    /\ BlockTypeOK
    /\ PayloadTypeOK
    /\ ShareTypeOK
    /\ DAEvidenceTypeOK

CustodySharesValid(v, m) ==
    /\ v \in activeValidators
    /\ m \in Manifests
    /\ \A s \in assignedShares[v] :
        /\ s \in validShares
        /\ shareManifest[s] = m
        /\ sharePayload[s] = manifestPayload[m]

VoteRecord(v, b) ==
    [ validator |-> v,
      block     |-> b,
      manifest  |-> blockManifest[b] ]

CertificateRecord(b, signers) ==
    [ block    |-> b,
      manifest |-> blockManifest[b],
      signers  |-> signers ]

ReconstructionRecord(b, shares) ==
    [ block    |-> b,
      manifest |-> blockManifest[b],
      payload  |-> manifestPayload[blockManifest[b]],
      shares   |-> shares ]

CertificateValid(cert) ==
    /\ cert \in CertificateType
    /\ cert.manifest = blockManifest[cert.block]
    /\ cert.signers \subseteq activeValidators
    /\ Cardinality(cert.signers) >= Quorum
    /\ \A v \in cert.signers :
        /\ VoteRecord(v, cert.block) \in daVotes
        /\ CustodySharesValid(v, cert.manifest)

ReconstructionValid(rec) ==
    /\ rec \in ReconstructionType
    /\ rec.manifest = blockManifest[rec.block]
    /\ rec.payload = manifestPayload[rec.manifest]
    /\ Cardinality(rec.shares) >= Threshold
    /\ \A s \in rec.shares :
        /\ s \in validShares
        /\ shareManifest[s] = rec.manifest
        /\ sharePayload[s] = rec.payload

ReplayRootsMatch(b, payload) ==
    /\ payloadTxRoot[payload] = blockTxRoot[b]
    /\ payloadExecutionRoot[payload] = blockExecutionRoot[b]

ValidateCustody(v, b) ==
    /\ v \in activeValidators
    /\ b \in Blocks
    /\ CustodySharesValid(v, blockManifest[b])
    /\ custodyValidated' = custodyValidated \cup {<<v, blockManifest[b]>>}
    /\ UNCHANGED << activeValidators,
                    blockManifest,
                    blockPayload,
                    blockTxRoot,
                    blockExecutionRoot,
                    manifestPayload,
                    payloadTxRoot,
                    payloadExecutionRoot,
                    shareManifest,
                    sharePayload,
                    validShares,
                    assignedShares,
                    daVotes,
                    daCertificates,
                    reconstructions,
                    finalizedBlocks >>

VoteDA(v, b) ==
    /\ v \in activeValidators
    /\ b \in Blocks
    /\ <<v, blockManifest[b]>> \in custodyValidated
    /\ daVotes' = daVotes \cup {VoteRecord(v, b)}
    /\ UNCHANGED << activeValidators,
                    blockManifest,
                    blockPayload,
                    blockTxRoot,
                    blockExecutionRoot,
                    manifestPayload,
                    payloadTxRoot,
                    payloadExecutionRoot,
                    shareManifest,
                    sharePayload,
                    validShares,
                    assignedShares,
                    custodyValidated,
                    daCertificates,
                    reconstructions,
                    finalizedBlocks >>

AggregateCertificate(b, signers) ==
    /\ b \in Blocks
    /\ signers \subseteq activeValidators
    /\ Cardinality(signers) >= Quorum
    /\ \A v \in signers : VoteRecord(v, b) \in daVotes
    /\ daCertificates' = daCertificates \cup {CertificateRecord(b, signers)}
    /\ UNCHANGED << activeValidators,
                    blockManifest,
                    blockPayload,
                    blockTxRoot,
                    blockExecutionRoot,
                    manifestPayload,
                    payloadTxRoot,
                    payloadExecutionRoot,
                    shareManifest,
                    sharePayload,
                    validShares,
                    assignedShares,
                    custodyValidated,
                    daVotes,
                    reconstructions,
                    finalizedBlocks >>

ReconstructPayload(b, shares) ==
    /\ b \in Blocks
    /\ shares \subseteq Shares
    /\ LET rec == ReconstructionRecord(b, shares) IN
        /\ ReconstructionValid(rec)
        /\ ReplayRootsMatch(b, rec.payload)
        /\ reconstructions' = reconstructions \cup {rec}
    /\ UNCHANGED << activeValidators,
                    blockManifest,
                    blockPayload,
                    blockTxRoot,
                    blockExecutionRoot,
                    manifestPayload,
                    payloadTxRoot,
                    payloadExecutionRoot,
                    shareManifest,
                    sharePayload,
                    validShares,
                    assignedShares,
                    custodyValidated,
                    daVotes,
                    daCertificates,
                    finalizedBlocks >>

FinalizeBlock(b) ==
    /\ b \in Blocks
    /\ \E cert \in daCertificates :
        /\ cert.block = b
        /\ CertificateValid(cert)
    /\ \E rec \in reconstructions :
        /\ rec.block = b
        /\ rec.payload = blockPayload[b]
        /\ ReconstructionValid(rec)
        /\ ReplayRootsMatch(b, rec.payload)
    /\ finalizedBlocks' = finalizedBlocks \cup {b}
    /\ UNCHANGED << activeValidators,
                    blockManifest,
                    blockPayload,
                    blockTxRoot,
                    blockExecutionRoot,
                    manifestPayload,
                    payloadTxRoot,
                    payloadExecutionRoot,
                    shareManifest,
                    sharePayload,
                    validShares,
                    assignedShares,
                    custodyValidated,
                    daVotes,
                    daCertificates,
                    reconstructions >>

Init ==
    /\ activeValidators = Validators
    /\ blockManifest = [b \in Blocks |-> GoodManifest]
    /\ blockPayload = [b \in Blocks |-> GoodPayload]
    /\ blockTxRoot = [b \in Blocks |-> TxRootGood]
    /\ blockExecutionRoot = [b \in Blocks |-> ExecutionRootGood]
    /\ manifestPayload =
        [m \in Manifests |-> IF m = GoodManifest THEN GoodPayload ELSE BadPayload]
    /\ payloadTxRoot =
        [p \in Payloads |-> IF p = GoodPayload THEN TxRootGood ELSE TxRootBad]
    /\ payloadExecutionRoot =
        [p \in Payloads |->
            IF p = GoodPayload THEN ExecutionRootGood ELSE ExecutionRootBad]
    /\ shareManifest =
        [s \in Shares |-> IF s = BadShare THEN BadManifest ELSE GoodManifest]
    /\ sharePayload =
        [s \in Shares |-> IF s = BadShare THEN BadPayload ELSE GoodPayload]
    /\ validShares = {GoodShareA, GoodShareB}
    /\ assignedShares =
        [v \in Validators |->
            IF v = V1 THEN {GoodShareA}
            ELSE IF v = V2 THEN {GoodShareB}
            ELSE {BadShare}]
    /\ custodyValidated = {}
    /\ daVotes = {}
    /\ daCertificates = {}
    /\ reconstructions = {}
    /\ finalizedBlocks = {}

Next ==
    \/ \E v \in Validators, b \in Blocks : ValidateCustody(v, b)
    \/ \E v \in Validators, b \in Blocks : VoteDA(v, b)
    \/ \E b \in Blocks, signers \in SUBSET Validators :
        AggregateCertificate(b, signers)
    \/ \E b \in Blocks, shares \in SUBSET Shares :
        ReconstructPayload(b, shares)
    \/ \E b \in Blocks : FinalizeBlock(b)

Spec == Init /\ [][Next]_vars

\* Validators only sign after validating their required custody/sample shares.
DASignatureCustodySoundness ==
    [](\A vote \in daVotes :
        /\ <<vote.validator, vote.manifest>> \in custodyValidated
        /\ CustodySharesValid(vote.validator, vote.manifest))

\* A DA certificate is bound to the committed manifest and active-validator
\* quorum for that block.
DAQuorumCertificateSoundness ==
    [](\A cert \in daCertificates : CertificateValid(cert))

\* Block finality requires a valid DA certificate for the committed manifest.
DAFinalityRequiresCertificate ==
    [](\A b \in finalizedBlocks :
        \E cert \in daCertificates :
            /\ cert.block = b
            /\ cert.manifest = blockManifest[b]
            /\ CertificateValid(cert))

\* Every accepted reconstruction uses threshold-valid shares for the committed
\* manifest and reconstructs the canonical payload for that manifest.
DAReconstructionSoundness ==
    [](\A rec \in reconstructions :
        /\ ReconstructionValid(rec)
        /\ ReplayRootsMatch(rec.block, rec.payload))

\* Finalized blocks can be replayed from the reconstructed DA payload roots.
FinalizedPayloadReplaySoundness ==
    [](\A b \in finalizedBlocks :
        \E rec \in reconstructions :
            /\ rec.block = b
            /\ rec.payload = blockPayload[b]
            /\ ReconstructionValid(rec)
            /\ ReplayRootsMatch(b, rec.payload))

=============================================================================
