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
    DeTTaApp,
    SocialApp,
    AppGovernanceApp,
    DeTTaProfile,
    SocialProfileV1,
    SocialProfileV2,
    GovernanceProfile,
    GoodCoordinate,
    SocialCoordinate,
    GovernanceCoordinate,
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
Applications == {DeTTaApp, SocialApp, AppGovernanceApp}
Profiles == {DeTTaProfile, SocialProfileV1, SocialProfileV2, GovernanceProfile}
Coordinates == {GoodCoordinate, SocialCoordinate, GovernanceCoordinate}
TxRoots == {TxRootGood, TxRootBad}
ExecutionRoots == {ExecutionRootGood, ExecutionRootBad}

ProfileApplicationBinding ==
    [p \in Profiles |->
        IF p = DeTTaProfile THEN DeTTaApp
        ELSE IF p = GovernanceProfile THEN AppGovernanceApp
        ELSE SocialApp]

CoordinateApplicationBinding ==
    [c \in Coordinates |->
        IF c = GoodCoordinate THEN DeTTaApp
        ELSE IF c = GovernanceCoordinate THEN AppGovernanceApp
        ELSE SocialApp]

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
    finalizedBlocks,
    manifestApplication,
    manifestProfile,
    manifestCoordinate,
    payloadApplication,
    payloadProfile,
    payloadCoordinate,
    profileApplication,
    coordinateApplication,
    activeApplicationProfiles,
    deprecatedApplicationProfiles

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
       finalizedBlocks,
       manifestApplication,
       manifestProfile,
       manifestCoordinate,
       payloadApplication,
       payloadProfile,
       payloadCoordinate,
       profileApplication,
       coordinateApplication,
       activeApplicationProfiles,
       deprecatedApplicationProfiles >>

ApplicationMetadataVars ==
    << manifestApplication,
       manifestProfile,
       manifestCoordinate,
       payloadApplication,
       payloadProfile,
       payloadCoordinate,
       profileApplication,
       coordinateApplication,
       activeApplicationProfiles,
       deprecatedApplicationProfiles >>

VoteType ==
    [ validator : Validators,
      block     : Blocks,
      manifest  : Manifests ]

CertificateType ==
    [ block    : Blocks,
      manifest : Manifests,
      application : Applications,
      profile : Profiles,
      coordinate : Coordinates,
      signers  : SUBSET Validators ]

ReconstructionType ==
    [ block    : Blocks,
      manifest : Manifests,
      payload  : Payloads,
      application : Applications,
      profile : Profiles,
      coordinate : Coordinates,
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
ApplicationMetadataTypeOK ==
    /\ manifestApplication \in [Manifests -> Applications]
    /\ manifestProfile \in [Manifests -> Profiles]
    /\ manifestCoordinate \in [Manifests -> Coordinates]
    /\ payloadApplication \in [Payloads -> Applications]
    /\ payloadProfile \in [Payloads -> Profiles]
    /\ payloadCoordinate \in [Payloads -> Coordinates]
    /\ profileApplication \in [Profiles -> Applications]
    /\ coordinateApplication \in [Coordinates -> Applications]
    /\ activeApplicationProfiles \subseteq Profiles
    /\ deprecatedApplicationProfiles \subseteq Profiles
    /\ activeApplicationProfiles \cap deprecatedApplicationProfiles = {}

TypeOK ==
    /\ ValidatorTypeOK
    /\ BlockTypeOK
    /\ PayloadTypeOK
    /\ ShareTypeOK
    /\ DAEvidenceTypeOK
    /\ ApplicationMetadataTypeOK

ManifestPayloadIdentity(m) ==
    LET p == manifestPayload[m] IN
        /\ manifestApplication[m] = payloadApplication[p]
        /\ manifestProfile[m] = payloadProfile[p]
        /\ manifestCoordinate[m] = payloadCoordinate[p]
        /\ profileApplication[manifestProfile[m]] = manifestApplication[m]
        /\ coordinateApplication[manifestCoordinate[m]] = manifestApplication[m]
        /\ coordinateApplication[payloadCoordinate[p]] = payloadApplication[p]

HistoricalProfileKnown(profile) ==
    profile \in (activeApplicationProfiles \cup deprecatedApplicationProfiles)

CustodySharesValid(v, m) ==
    /\ v \in activeValidators
    /\ m \in Manifests
    /\ ManifestPayloadIdentity(m)
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
      application |-> manifestApplication[blockManifest[b]],
      profile |-> manifestProfile[blockManifest[b]],
      coordinate |-> manifestCoordinate[blockManifest[b]],
      signers  |-> signers ]

ReconstructionRecord(b, shares) ==
    [ block    |-> b,
      manifest |-> blockManifest[b],
      payload  |-> manifestPayload[blockManifest[b]],
      application |-> manifestApplication[blockManifest[b]],
      profile |-> manifestProfile[blockManifest[b]],
      coordinate |-> manifestCoordinate[blockManifest[b]],
      shares   |-> shares ]

CertificateValid(cert) ==
    /\ cert \in CertificateType
    /\ cert.manifest = blockManifest[cert.block]
    /\ cert.application = manifestApplication[cert.manifest]
    /\ cert.profile = manifestProfile[cert.manifest]
    /\ cert.coordinate = manifestCoordinate[cert.manifest]
    /\ HistoricalProfileKnown(cert.profile)
    /\ ManifestPayloadIdentity(cert.manifest)
    /\ cert.signers \subseteq activeValidators
    /\ Cardinality(cert.signers) >= Quorum
    /\ \A v \in cert.signers :
        /\ VoteRecord(v, cert.block) \in daVotes
        /\ CustodySharesValid(v, cert.manifest)

ReconstructionValid(rec) ==
    /\ rec \in ReconstructionType
    /\ rec.manifest = blockManifest[rec.block]
    /\ rec.payload = manifestPayload[rec.manifest]
    /\ rec.application = manifestApplication[rec.manifest]
    /\ rec.profile = manifestProfile[rec.manifest]
    /\ rec.coordinate = manifestCoordinate[rec.manifest]
    /\ ManifestPayloadIdentity(rec.manifest)
    /\ HistoricalProfileKnown(rec.profile)
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
    /\ manifestProfile[blockManifest[b]] \in activeApplicationProfiles
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
                    finalizedBlocks,
                    ApplicationMetadataVars >>

VoteDA(v, b) ==
    /\ v \in activeValidators
    /\ b \in Blocks
    /\ manifestProfile[blockManifest[b]] \in activeApplicationProfiles
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
                    finalizedBlocks,
                    ApplicationMetadataVars >>

AggregateCertificate(b, signers) ==
    /\ b \in Blocks
    /\ manifestProfile[blockManifest[b]] \in activeApplicationProfiles
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
                    finalizedBlocks,
                    ApplicationMetadataVars >>

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
                    finalizedBlocks,
                    ApplicationMetadataVars >>

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
                    reconstructions,
                    ApplicationMetadataVars >>

DeprecateApplicationProfile(profile) ==
    /\ profile \in activeApplicationProfiles
    /\ activeApplicationProfiles' = activeApplicationProfiles \ {profile}
    /\ deprecatedApplicationProfiles' = deprecatedApplicationProfiles \cup {profile}
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
                    reconstructions,
                    finalizedBlocks,
                    manifestApplication,
                    manifestProfile,
                    manifestCoordinate,
                    payloadApplication,
                    payloadProfile,
                    payloadCoordinate,
                    profileApplication,
                    coordinateApplication >>

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
    /\ manifestApplication =
        [m \in Manifests |-> IF m = GoodManifest THEN DeTTaApp ELSE SocialApp]
    /\ manifestProfile =
        [m \in Manifests |->
            IF m = GoodManifest THEN DeTTaProfile ELSE SocialProfileV1]
    /\ manifestCoordinate =
        [m \in Manifests |->
            IF m = GoodManifest THEN GoodCoordinate ELSE SocialCoordinate]
    /\ payloadApplication =
        [p \in Payloads |-> IF p = GoodPayload THEN DeTTaApp ELSE SocialApp]
    /\ payloadProfile =
        [p \in Payloads |-> IF p = GoodPayload THEN DeTTaProfile ELSE SocialProfileV1]
    /\ payloadCoordinate =
        [p \in Payloads |->
            IF p = GoodPayload THEN GoodCoordinate ELSE SocialCoordinate]
    /\ profileApplication = ProfileApplicationBinding
    /\ coordinateApplication = CoordinateApplicationBinding
    /\ activeApplicationProfiles = {DeTTaProfile, SocialProfileV1, GovernanceProfile}
    /\ deprecatedApplicationProfiles = {}
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
    \/ \E profile \in Profiles : DeprecateApplicationProfile(profile)

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

\* Application manifests bind application id, profile id, and coordinate to the
\* canonical payload and to the profile/coordinate application owners.
ApplicationManifestIdentitySoundness ==
    [](\A m \in Manifests : ManifestPayloadIdentity(m))

\* Application DA certificates bind exactly one manifest identity tuple:
\* manifest hash, application id, profile id, and application coordinate.
ApplicationCertificateIdentitySoundness ==
    [](\A cert \in daCertificates :
        /\ CertificateValid(cert)
        /\ cert.application = manifestApplication[cert.manifest]
        /\ cert.profile = manifestProfile[cert.manifest]
        /\ cert.coordinate = manifestCoordinate[cert.manifest])

\* Reconstructing from threshold-valid shares returns the canonical payload for
\* the manifest and preserves the application identity tuple.
ApplicationReconstructionIdentitySoundness ==
    [](\A rec \in reconstructions :
        /\ ReconstructionValid(rec)
        /\ rec.application = payloadApplication[rec.payload]
        /\ rec.profile = payloadProfile[rec.payload]
        /\ rec.coordinate = payloadCoordinate[rec.payload])

\* Deprecated profiles remain available for historical reconstruction while new
\* certificates require the profile to be active at certificate aggregation.
ApplicationHistoricalProfileVerificationSoundness ==
    [](\A rec \in reconstructions : HistoricalProfileKnown(rec.profile))

\* Profile validation is deterministic because a profile atom has one immutable
\* application binding and cannot be active and deprecated in the same state.
ApplicationProfileValidationDeterminism ==
    [](/\ profileApplication = ProfileApplicationBinding
       /\ activeApplicationProfiles \cap deprecatedApplicationProfiles = {})

=============================================================================
