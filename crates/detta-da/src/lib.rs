use detta_core::{AspectModuleRecord, BlockHeader, ChainId, Event, Receipt, Transaction};
use reed_solomon_erasure::galois_8::ReedSolomon;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const DA_MANIFEST_SCHEMA: &str = "detta.da-manifest.v1";
pub const DA_CERTIFICATE_SCHEMA: &str = "detta.da-certificate.v1";
pub const DA_CHALLENGE_SCHEMA: &str = "detta.da-share-challenge.v1";
pub const DA_CHALLENGE_EVIDENCE_SCHEMA: &str = "detta.da-challenge-evidence.v1";
pub const DA_SAMPLE_PROOF_SCHEMA: &str = "detta.da-sample-proof.v1";
pub const DA_PAYLOAD_SCHEMA: &str = "detta.da-payload.v1";
pub const DA_PAYLOAD_VERSION: u32 = 1;
pub const REED_SOLOMON_MAX_SHARES: u32 = 256;

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct DaNamespace(pub String);

impl DaNamespace {
    pub fn new(value: impl Into<String>) -> Result<Self, DaError> {
        let value = value.into();
        if value.is_empty()
            || value.starts_with('.')
            || value.ends_with('.')
            || value.contains("..")
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.')
        {
            return Err(DaError::InvalidNamespace(value));
        }
        Ok(Self(value))
    }
}

impl From<DaNamespace> for String {
    fn from(namespace: DaNamespace) -> Self {
        namespace.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaPayload {
    pub chain_id: ChainId,
    pub height: u64,
    pub previous_block_hash: String,
    pub block_payload_version: u32,
    pub namespaces: Vec<DaNamespaceSection>,
}

impl DaPayload {
    pub fn new(
        chain_id: impl Into<ChainId>,
        height: u64,
        previous_block_hash: impl Into<String>,
        namespaces: Vec<DaNamespaceSection>,
    ) -> Result<Self, DaError> {
        let payload = Self {
            chain_id: chain_id.into(),
            height,
            previous_block_hash: previous_block_hash.into(),
            block_payload_version: DA_PAYLOAD_VERSION,
            namespaces,
        };
        payload.validate()?;
        Ok(payload.canonicalized())
    }

    pub fn canonicalized(&self) -> Self {
        let mut sections: BTreeMap<DaNamespace, Vec<DaRecord>> = BTreeMap::new();
        for section in &self.namespaces {
            sections
                .entry(section.namespace.clone())
                .or_default()
                .extend(section.records.clone());
        }
        Self {
            chain_id: self.chain_id.clone(),
            height: self.height,
            previous_block_hash: self.previous_block_hash.clone(),
            block_payload_version: self.block_payload_version,
            namespaces: sections
                .into_iter()
                .map(|(namespace, records)| DaNamespaceSection { namespace, records })
                .collect(),
        }
    }

    pub fn validate(&self) -> Result<(), DaError> {
        if self.chain_id.is_empty() {
            return Err(DaError::InvalidPayload("chain_id is empty".into()));
        }
        if self.block_payload_version != DA_PAYLOAD_VERSION {
            return Err(DaError::UnsupportedPayloadVersion {
                expected: DA_PAYLOAD_VERSION,
                actual: self.block_payload_version,
            });
        }
        for section in &self.namespaces {
            section.validate()?;
        }
        Ok(())
    }

    pub fn hash(&self) -> Result<String, DaError> {
        hash_canonical(&self.canonicalized())
    }

    pub fn namespace_root(&self) -> Result<String, DaError> {
        let ranges = namespace_ranges(&self.canonicalized());
        hash_canonical(&ranges)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaNamespaceSection {
    pub namespace: DaNamespace,
    pub records: Vec<DaRecord>,
}

impl DaNamespaceSection {
    pub fn new(namespace: DaNamespace, records: Vec<DaRecord>) -> Result<Self, DaError> {
        let section = Self { namespace, records };
        section.validate()?;
        Ok(section)
    }

    fn validate(&self) -> Result<(), DaError> {
        if self.records.is_empty() {
            return Err(DaError::InvalidPayload(format!(
                "namespace {} has no records",
                self.namespace.0
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DaRecord {
    BlockHeader(Box<BlockHeader>),
    SignedTransaction(Transaction),
    AspectArtifact(Box<AspectModuleRecord>),
    GovernancePayload {
        proposal_id: String,
        payload: String,
    },
    BridgeProof {
        message_id: String,
        proof: String,
    },
    OracleEvidence {
        asset: String,
        evidence: String,
    },
    Receipt(Receipt),
    Event(Event),
    StateDiff {
        root_before: String,
        root_after: String,
        diff: String,
    },
    SnapshotChunkReference {
        snapshot_root: String,
        manifest_hash: String,
        chunk_index: u32,
    },
    SnapshotChunkManifest {
        snapshot_root: String,
        snapshot_hash: String,
        metadata_roots: BTreeMap<String, String>,
        chunk_size: u32,
        total_bytes: u64,
        chunk_count: u32,
        chunk_hashes: Vec<String>,
        chunk_root: String,
    },
    SnapshotChunk {
        snapshot_root: String,
        manifest_hash: String,
        chunk_index: u32,
        chunk_hash: String,
        bytes: Vec<u8>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ErasureScheme {
    DeterministicChunks,
    ReedSolomonV1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaNamespaceRange {
    pub namespace: DaNamespace,
    pub section_index: u32,
    pub record_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaManifest {
    pub schema: String,
    pub schema_version: u32,
    pub chain_id: ChainId,
    pub height: u64,
    pub block_hash: String,
    pub payload_hash: String,
    pub payload_bytes: u64,
    pub namespace_root: String,
    pub share_root: String,
    pub erasure_scheme: ErasureScheme,
    pub original_share_count: u32,
    pub encoded_share_count: u32,
    pub reconstruction_threshold: u32,
    pub share_size_bytes: u32,
    pub share_hashes: Vec<String>,
    pub namespace_ranges: Vec<DaNamespaceRange>,
}

impl DaManifest {
    pub fn manifest_hash(&self) -> Result<String, DaError> {
        hash_canonical(self)
    }

    pub fn validate(&self) -> Result<(), DaError> {
        if self.schema != DA_MANIFEST_SCHEMA {
            return Err(DaError::InvalidManifest(format!(
                "unexpected schema {}",
                self.schema
            )));
        }
        if self.schema_version != 1 {
            return Err(DaError::InvalidManifest(format!(
                "unexpected schema version {}",
                self.schema_version
            )));
        }
        if self.share_size_bytes == 0 {
            return Err(DaError::InvalidChunkSize);
        }
        if self.original_share_count == 0
            || self.encoded_share_count == 0
            || self.reconstruction_threshold == 0
        {
            return Err(DaError::InvalidManifest(
                "share counts and reconstruction threshold must be positive".into(),
            ));
        }
        match self.erasure_scheme {
            ErasureScheme::DeterministicChunks => {
                if self.original_share_count != self.encoded_share_count
                    || self.reconstruction_threshold != self.encoded_share_count
                {
                    return Err(DaError::UnsupportedErasureScheme);
                }
            }
            ErasureScheme::ReedSolomonV1 => {
                if self.original_share_count >= self.encoded_share_count {
                    return Err(DaError::InvalidManifest(
                        "reed-solomon manifests must include parity shares".into(),
                    ));
                }
                if self.reconstruction_threshold != self.original_share_count {
                    return Err(DaError::InvalidManifest(
                        "reed-solomon threshold must equal original share count".into(),
                    ));
                }
                if self.encoded_share_count > REED_SOLOMON_MAX_SHARES {
                    return Err(DaError::ShareCountOverflow);
                }
                let payload_capacity =
                    u64::from(self.original_share_count) * u64::from(self.share_size_bytes);
                if payload_capacity < self.payload_bytes {
                    return Err(DaError::InvalidManifest(
                        "payload bytes exceed reed-solomon data shard capacity".into(),
                    ));
                }
            }
        }
        if self.share_hashes.len() != self.encoded_share_count as usize {
            return Err(DaError::ManifestShareCountMismatch {
                declared: self.encoded_share_count,
                actual: self.share_hashes.len() as u32,
            });
        }
        let expected_share_root = share_root(&self.share_hashes)?;
        if self.share_root != expected_share_root {
            return Err(DaError::ShareRootMismatch {
                expected: expected_share_root,
                actual: self.share_root.clone(),
            });
        }
        validate_namespace_ranges(&self.namespace_ranges)?;
        let expected_namespace_root = namespace_root_for_ranges(&self.namespace_ranges)?;
        if self.namespace_root != expected_namespace_root {
            return Err(DaError::NamespaceRootMismatch {
                expected: expected_namespace_root,
                actual: self.namespace_root.clone(),
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DaMerkleSiblingSide {
    Left,
    Right,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaMerkleSibling {
    pub side: DaMerkleSiblingSide,
    pub hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaShareInclusionProof {
    pub schema: String,
    pub schema_version: u32,
    pub manifest_hash: String,
    pub share_root: String,
    pub share_index: u32,
    pub share_hash: String,
    pub leaf_count: u32,
    pub siblings: Vec<DaMerkleSibling>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaSampleProof {
    pub share: DaShare,
    pub inclusion_proof: DaShareInclusionProof,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaSamplingSchedule {
    pub manifest_hash: String,
    pub block_hash: String,
    pub client_randomness_hash: String,
    pub requested_sample_count: u32,
    pub share_indices: Vec<u32>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaNamespaceProof {
    pub manifest_hash: String,
    pub namespace_root: String,
    pub namespace: DaNamespace,
    pub range: Option<DaNamespaceRange>,
    pub namespace_ranges: Vec<DaNamespaceRange>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaLightClientSamplingReport {
    pub manifest_hash: String,
    pub block_hash: String,
    pub client_randomness_hash: String,
    pub requested_sample_count: u32,
    pub sampled_share_indices: Vec<u32>,
    pub verified_share_count: u32,
    pub namespace_proof_count: u32,
    pub valid: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaSampleProofBundle {
    pub schedule: DaSamplingSchedule,
    pub sample_proofs: Vec<DaSampleProof>,
    pub namespace_proofs: Vec<DaNamespaceProof>,
    pub verification: DaLightClientSamplingReport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaAvailabilityVote {
    pub chain_id: ChainId,
    pub height: u64,
    pub block_hash: String,
    pub manifest_hash: String,
    pub share_root: String,
    pub validator_id: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custody_share_indices: Vec<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sampled_share_indices: Vec<u32>,
}

impl DaAvailabilityVote {
    pub fn from_manifest(
        manifest: &DaManifest,
        validator_id: impl Into<String>,
    ) -> Result<Self, DaError> {
        manifest.validate()?;
        let vote = Self {
            chain_id: manifest.chain_id.clone(),
            height: manifest.height,
            block_hash: manifest.block_hash.clone(),
            manifest_hash: manifest.manifest_hash()?,
            share_root: manifest.share_root.clone(),
            validator_id: validator_id.into(),
            custody_share_indices: Vec::new(),
            sampled_share_indices: Vec::new(),
        };
        vote.validate()?;
        Ok(vote)
    }

    pub fn from_manifest_with_custody(
        manifest: &DaManifest,
        validator_id: impl Into<String>,
        custody_share_indices: impl IntoIterator<Item = u32>,
        sampled_share_indices: impl IntoIterator<Item = u32>,
    ) -> Result<Self, DaError> {
        manifest.validate()?;
        let vote = Self {
            chain_id: manifest.chain_id.clone(),
            height: manifest.height,
            block_hash: manifest.block_hash.clone(),
            manifest_hash: manifest.manifest_hash()?,
            share_root: manifest.share_root.clone(),
            validator_id: validator_id.into(),
            custody_share_indices: canonical_share_indices(
                custody_share_indices,
                manifest.encoded_share_count,
            )?,
            sampled_share_indices: canonical_share_indices(
                sampled_share_indices,
                manifest.encoded_share_count,
            )?,
        };
        vote.validate()?;
        Ok(vote)
    }

    pub fn from_verified_custody(
        manifest: &DaManifest,
        shares: &[DaShare],
        validator_id: impl Into<String>,
        custody_share_count: u32,
    ) -> Result<Self, DaError> {
        let validator_id = validator_id.into();
        let custody_share_indices =
            assigned_custody_share_indices(manifest, &validator_id, custody_share_count)?;
        verify_custody_shares(manifest, shares, &custody_share_indices)?;
        Self::from_manifest_with_custody(manifest, validator_id, custody_share_indices, Vec::new())
    }

    pub fn vote_hash(&self) -> Result<String, DaError> {
        self.validate()?;
        hash_canonical(self)
    }

    pub fn validate(&self) -> Result<(), DaError> {
        if self.chain_id.is_empty() {
            return Err(DaError::InvalidAvailabilityVote("chain_id is empty".into()));
        }
        if self.block_hash.is_empty() {
            return Err(DaError::InvalidAvailabilityVote(
                "block_hash is empty".into(),
            ));
        }
        if self.manifest_hash.is_empty() {
            return Err(DaError::InvalidAvailabilityVote(
                "manifest_hash is empty".into(),
            ));
        }
        if self.share_root.is_empty() {
            return Err(DaError::InvalidAvailabilityVote(
                "share_root is empty".into(),
            ));
        }
        if self.validator_id.is_empty() {
            return Err(DaError::InvalidAvailabilityVote(
                "validator_id is empty".into(),
            ));
        }
        ensure_sorted_unique_indices(&self.custody_share_indices, "custody_share_indices")?;
        ensure_sorted_unique_indices(&self.sampled_share_indices, "sampled_share_indices")?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaAvailabilityCertificate {
    pub schema: String,
    pub schema_version: u32,
    pub chain_id: ChainId,
    pub height: u64,
    pub block_hash: String,
    pub manifest_hash: String,
    pub share_root: String,
    pub signers: Vec<String>,
}

impl DaAvailabilityCertificate {
    pub fn from_manifest(
        manifest: &DaManifest,
        signers: impl IntoIterator<Item = String>,
    ) -> Result<Self, DaError> {
        manifest.validate()?;
        let certificate = Self {
            schema: DA_CERTIFICATE_SCHEMA.into(),
            schema_version: 1,
            chain_id: manifest.chain_id.clone(),
            height: manifest.height,
            block_hash: manifest.block_hash.clone(),
            manifest_hash: manifest.manifest_hash()?,
            share_root: manifest.share_root.clone(),
            signers: canonical_signers(signers)?,
        };
        certificate.validate()?;
        Ok(certificate)
    }

    pub fn certificate_hash(&self) -> Result<String, DaError> {
        self.validate()?;
        hash_canonical(self)
    }

    pub fn validate(&self) -> Result<(), DaError> {
        if self.schema != DA_CERTIFICATE_SCHEMA {
            return Err(DaError::InvalidAvailabilityCertificate(format!(
                "unexpected schema {}",
                self.schema
            )));
        }
        if self.schema_version != 1 {
            return Err(DaError::InvalidAvailabilityCertificate(format!(
                "unexpected schema version {}",
                self.schema_version
            )));
        }
        if self.chain_id.is_empty()
            || self.block_hash.is_empty()
            || self.manifest_hash.is_empty()
            || self.share_root.is_empty()
        {
            return Err(DaError::InvalidAvailabilityCertificate(
                "chain_id, block_hash, manifest_hash, and share_root must be nonempty".into(),
            ));
        }
        if self.signers.is_empty() {
            return Err(DaError::InvalidAvailabilityCertificate(
                "signers must be nonempty".into(),
            ));
        }
        if self.signers != canonical_signers(self.signers.clone())? {
            return Err(DaError::InvalidAvailabilityCertificate(
                "signers must be unique and sorted".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaShareChallenge {
    pub schema: String,
    pub schema_version: u32,
    pub chain_id: ChainId,
    pub height: u64,
    pub block_hash: String,
    pub manifest_hash: String,
    pub share_root: String,
    pub availability_vote_hash: String,
    pub challenged_validator_id: String,
    pub challenger_id: String,
    pub share_index: u32,
    pub expires_at_height: u64,
}

impl DaShareChallenge {
    pub fn from_availability_vote(
        vote: &DaAvailabilityVote,
        challenger_id: impl Into<String>,
        share_index: u32,
        expires_at_height: u64,
    ) -> Result<Self, DaError> {
        vote.validate()?;
        if !vote.custody_share_indices.contains(&share_index)
            && !vote.sampled_share_indices.contains(&share_index)
        {
            return Err(DaError::InvalidChallenge(format!(
                "share {share_index} was not signed as custody or sampled by validator {}",
                vote.validator_id
            )));
        }
        let challenge = Self {
            schema: DA_CHALLENGE_SCHEMA.into(),
            schema_version: 1,
            chain_id: vote.chain_id.clone(),
            height: vote.height,
            block_hash: vote.block_hash.clone(),
            manifest_hash: vote.manifest_hash.clone(),
            share_root: vote.share_root.clone(),
            availability_vote_hash: vote.vote_hash()?,
            challenged_validator_id: vote.validator_id.clone(),
            challenger_id: challenger_id.into(),
            share_index,
            expires_at_height,
        };
        challenge.validate()?;
        Ok(challenge)
    }

    pub fn challenge_hash(&self) -> Result<String, DaError> {
        self.validate()?;
        hash_canonical(self)
    }

    pub fn validate(&self) -> Result<(), DaError> {
        if self.schema != DA_CHALLENGE_SCHEMA {
            return Err(DaError::InvalidChallenge(format!(
                "unexpected schema {}",
                self.schema
            )));
        }
        if self.schema_version != 1 {
            return Err(DaError::InvalidChallenge(format!(
                "unexpected schema version {}",
                self.schema_version
            )));
        }
        if self.chain_id.is_empty()
            || self.block_hash.is_empty()
            || self.manifest_hash.is_empty()
            || self.share_root.is_empty()
            || self.availability_vote_hash.is_empty()
            || self.challenged_validator_id.is_empty()
            || self.challenger_id.is_empty()
        {
            return Err(DaError::InvalidChallenge(
                "challenge fields must be nonempty".into(),
            ));
        }
        if self.expires_at_height <= self.height {
            return Err(DaError::InvalidChallenge(
                "challenge expiry must be after challenged height".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaShareChallengeResponse {
    pub chain_id: ChainId,
    pub height: u64,
    pub block_hash: String,
    pub manifest_hash: String,
    pub share_root: String,
    pub challenge_hash: String,
    pub validator_id: String,
    pub share_index: u32,
    pub share: Option<DaShare>,
}

impl DaShareChallengeResponse {
    pub fn from_share(challenge: &DaShareChallenge, share: DaShare) -> Result<Self, DaError> {
        Self::new(challenge, Some(share))
    }

    pub fn empty(challenge: &DaShareChallenge) -> Result<Self, DaError> {
        Self::new(challenge, None)
    }

    fn new(challenge: &DaShareChallenge, share: Option<DaShare>) -> Result<Self, DaError> {
        challenge.validate()?;
        let response = Self {
            chain_id: challenge.chain_id.clone(),
            height: challenge.height,
            block_hash: challenge.block_hash.clone(),
            manifest_hash: challenge.manifest_hash.clone(),
            share_root: challenge.share_root.clone(),
            challenge_hash: challenge.challenge_hash()?,
            validator_id: challenge.challenged_validator_id.clone(),
            share_index: challenge.share_index,
            share,
        };
        response.validate(challenge)?;
        Ok(response)
    }

    pub fn response_hash(&self) -> Result<String, DaError> {
        hash_canonical(self)
    }

    pub fn validate(&self, challenge: &DaShareChallenge) -> Result<(), DaError> {
        challenge.validate()?;
        let challenge_hash = challenge.challenge_hash()?;
        if self.chain_id != challenge.chain_id
            || self.height != challenge.height
            || self.block_hash != challenge.block_hash
            || self.manifest_hash != challenge.manifest_hash
            || self.share_root != challenge.share_root
            || self.challenge_hash != challenge_hash
            || self.validator_id != challenge.challenged_validator_id
            || self.share_index != challenge.share_index
        {
            return Err(DaError::InvalidChallengeResponse(
                "response does not match challenge".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DaChallengeFault {
    MissingResponse,
    InvalidResponse { reason: String },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaChallengeEvidence {
    pub schema: String,
    pub schema_version: u32,
    pub chain_id: ChainId,
    pub height: u64,
    pub block_hash: String,
    pub manifest_hash: String,
    pub share_root: String,
    pub challenged_validator_id: String,
    pub reporter_id: String,
    pub share_index: u32,
    pub challenge_hash: String,
    pub response_hash: Option<String>,
    pub observed_at_height: u64,
    pub fault: DaChallengeFault,
}

impl DaChallengeEvidence {
    pub fn missing_response(
        challenge: &DaShareChallenge,
        reporter_id: impl Into<String>,
        observed_at_height: u64,
    ) -> Result<Self, DaError> {
        challenge.validate()?;
        if observed_at_height <= challenge.expires_at_height {
            return Err(DaError::InvalidChallenge(
                "missing-response evidence requires an expired challenge".into(),
            ));
        }
        let evidence = Self::from_challenge(
            challenge,
            reporter_id,
            None,
            observed_at_height,
            DaChallengeFault::MissingResponse,
        )?;
        evidence.validate()?;
        Ok(evidence)
    }

    pub fn invalid_response(
        challenge: &DaShareChallenge,
        response: &DaShareChallengeResponse,
        manifest: &DaManifest,
        reporter_id: impl Into<String>,
        observed_at_height: u64,
    ) -> Result<Self, DaError> {
        challenge.validate()?;
        response.validate(challenge)?;
        verify_manifest_matches_challenge(manifest, challenge)?;
        let reason = match &response.share {
            Some(share) if share.index != challenge.share_index => {
                format!(
                    "response share index {} does not match challenged index {}",
                    share.index, challenge.share_index
                )
            }
            Some(share) => match verify_share_against_manifest(manifest, share) {
                Ok(()) => {
                    return Err(DaError::InvalidChallengeResponse(
                        "response includes a valid challenged share".into(),
                    ));
                }
                Err(error) => format!("{error:?}"),
            },
            None => "response did not include a share".into(),
        };
        let evidence = Self::from_challenge(
            challenge,
            reporter_id,
            Some(response.response_hash()?),
            observed_at_height,
            DaChallengeFault::InvalidResponse { reason },
        )?;
        evidence.validate()?;
        Ok(evidence)
    }

    fn from_challenge(
        challenge: &DaShareChallenge,
        reporter_id: impl Into<String>,
        response_hash: Option<String>,
        observed_at_height: u64,
        fault: DaChallengeFault,
    ) -> Result<Self, DaError> {
        Ok(Self {
            schema: DA_CHALLENGE_EVIDENCE_SCHEMA.into(),
            schema_version: 1,
            chain_id: challenge.chain_id.clone(),
            height: challenge.height,
            block_hash: challenge.block_hash.clone(),
            manifest_hash: challenge.manifest_hash.clone(),
            share_root: challenge.share_root.clone(),
            challenged_validator_id: challenge.challenged_validator_id.clone(),
            reporter_id: reporter_id.into(),
            share_index: challenge.share_index,
            challenge_hash: challenge.challenge_hash()?,
            response_hash,
            observed_at_height,
            fault,
        })
    }

    pub fn evidence_hash(&self) -> Result<String, DaError> {
        self.validate()?;
        hash_canonical(self)
    }

    pub fn validate(&self) -> Result<(), DaError> {
        if self.schema != DA_CHALLENGE_EVIDENCE_SCHEMA {
            return Err(DaError::InvalidChallenge(format!(
                "unexpected evidence schema {}",
                self.schema
            )));
        }
        if self.schema_version != 1 {
            return Err(DaError::InvalidChallenge(format!(
                "unexpected evidence schema version {}",
                self.schema_version
            )));
        }
        if self.chain_id.is_empty()
            || self.block_hash.is_empty()
            || self.manifest_hash.is_empty()
            || self.share_root.is_empty()
            || self.challenged_validator_id.is_empty()
            || self.reporter_id.is_empty()
            || self.challenge_hash.is_empty()
        {
            return Err(DaError::InvalidChallenge(
                "evidence fields must be nonempty".into(),
            ));
        }
        if let DaChallengeFault::InvalidResponse { reason } = &self.fault {
            if reason.is_empty() || self.response_hash.is_none() {
                return Err(DaError::InvalidChallenge(
                    "invalid-response evidence requires a reason and response hash".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaChallengeRecord {
    pub challenge: DaShareChallenge,
    pub response: Option<DaShareChallengeResponse>,
    pub evidence: Option<DaChallengeEvidence>,
}

impl DaChallengeRecord {
    pub fn challenge_id(&self) -> Result<String, DaError> {
        self.challenge.challenge_hash()
    }

    pub fn validate(&self) -> Result<(), DaError> {
        self.challenge.validate()?;
        if let Some(response) = &self.response {
            response.validate(&self.challenge)?;
        }
        if let Some(evidence) = &self.evidence {
            evidence.validate()?;
            if evidence.challenge_hash != self.challenge.challenge_hash()? {
                return Err(DaError::InvalidChallenge(
                    "evidence does not match challenge".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaShare {
    pub manifest_hash: String,
    pub index: u32,
    pub bytes: Vec<u8>,
    pub share_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DaShareSet {
    pub manifest: DaManifest,
    pub shares: Vec<DaShare>,
}

impl DaShareSet {
    pub fn from_payload(
        payload: &DaPayload,
        block_hash: impl Into<String>,
        share_size_bytes: usize,
    ) -> Result<Self, DaError> {
        build_da_share_set(payload, block_hash, share_size_bytes)
    }

    pub fn from_payload_reed_solomon(
        payload: &DaPayload,
        block_hash: impl Into<String>,
        data_share_count: u32,
        parity_share_count: u32,
    ) -> Result<Self, DaError> {
        build_reed_solomon_share_set(payload, block_hash, data_share_count, parity_share_count)
    }

    pub fn verify(&self) -> Result<(), DaError> {
        self.reconstruct_payload().map(|_| ())
    }

    pub fn reconstruct_payload(&self) -> Result<DaPayload, DaError> {
        self.manifest.validate()?;
        let shares = validated_share_map(&self.manifest, &self.shares)?;

        if shares.len() < self.manifest.reconstruction_threshold as usize {
            return Err(DaError::InsufficientShares {
                required: self.manifest.reconstruction_threshold,
                actual: shares.len() as u32,
            });
        }

        match self.manifest.erasure_scheme {
            ErasureScheme::DeterministicChunks => {
                reconstruct_deterministic_payload(&self.manifest, &shares)
            }
            ErasureScheme::ReedSolomonV1 => {
                reconstruct_reed_solomon_payload(&self.manifest, &shares)
            }
        }
    }
}

fn validated_share_map<'a>(
    manifest: &DaManifest,
    shares: &'a [DaShare],
) -> Result<BTreeMap<u32, &'a DaShare>, DaError> {
    manifest.validate()?;
    let manifest_hash = manifest.manifest_hash()?;
    let mut by_index = BTreeMap::new();

    for share in shares {
        if share.manifest_hash != manifest_hash {
            return Err(DaError::ManifestHashMismatch {
                expected: manifest_hash.clone(),
                actual: share.manifest_hash.clone(),
            });
        }
        if share.index >= manifest.encoded_share_count {
            return Err(DaError::UnexpectedShare {
                index: share.index,
                share_count: manifest.encoded_share_count,
            });
        }
        if by_index.insert(share.index, share).is_some() {
            return Err(DaError::DuplicateShare { index: share.index });
        }
        let actual_share_hash = hash_bytes(&share.bytes);
        let expected_share_hash = &manifest.share_hashes[share.index as usize];
        if share.share_hash != actual_share_hash || &share.share_hash != expected_share_hash {
            return Err(DaError::ShareHashMismatch { index: share.index });
        }
    }

    Ok(by_index)
}

fn reconstruct_deterministic_payload(
    manifest: &DaManifest,
    shares: &BTreeMap<u32, &DaShare>,
) -> Result<DaPayload, DaError> {
    let mut total_bytes = 0_u64;
    let mut payload_bytes =
        Vec::with_capacity(usize::try_from(manifest.payload_bytes).map_err(|_| {
            DaError::InvalidManifest("payload byte count does not fit this platform".into())
        })?);
    for index in 0..manifest.encoded_share_count {
        let share = shares.get(&index).ok_or(DaError::MissingShare { index })?;
        payload_bytes.extend_from_slice(&share.bytes);
        total_bytes += share.bytes.len() as u64;
    }

    if total_bytes != manifest.payload_bytes {
        return Err(DaError::TotalBytesMismatch {
            expected: manifest.payload_bytes,
            actual: total_bytes,
        });
    }

    decode_payload_bytes(manifest, &payload_bytes)
}

fn reconstruct_reed_solomon_payload(
    manifest: &DaManifest,
    shares: &BTreeMap<u32, &DaShare>,
) -> Result<DaPayload, DaError> {
    let share_size = manifest.share_size_bytes as usize;
    let mut shard_options = vec![None; manifest.encoded_share_count as usize];

    for (index, share) in shares {
        if share.bytes.len() != share_size {
            return Err(DaError::InvalidManifest(format!(
                "reed-solomon share {index} has {} bytes, expected {share_size}",
                share.bytes.len()
            )));
        }
        shard_options[*index as usize] = Some(share.bytes.clone());
    }

    reed_solomon_codec(manifest)?.reconstruct(&mut shard_options)?;

    for (index, maybe_shard) in shard_options.iter().enumerate() {
        let shard = maybe_shard.as_ref().ok_or(DaError::MissingShare {
            index: index as u32,
        })?;
        let actual_share_hash = hash_bytes(shard);
        if actual_share_hash != manifest.share_hashes[index] {
            return Err(DaError::ShareHashMismatch {
                index: index as u32,
            });
        }
    }

    let payload_capacity = usize::try_from(manifest.original_share_count)
        .map_err(|_| DaError::ShareCountOverflow)?
        .checked_mul(share_size)
        .ok_or(DaError::ShareCountOverflow)?;
    let payload_len = usize::try_from(manifest.payload_bytes).map_err(|_| {
        DaError::InvalidManifest("payload byte count does not fit this platform".into())
    })?;
    if payload_len > payload_capacity {
        return Err(DaError::InvalidManifest(
            "payload bytes exceed reed-solomon data shard capacity".into(),
        ));
    }

    let mut payload_bytes = Vec::with_capacity(payload_capacity);
    for maybe_shard in shard_options
        .iter()
        .take(manifest.original_share_count as usize)
    {
        payload_bytes.extend_from_slice(
            maybe_shard
                .as_ref()
                .expect("reed-solomon reconstruction populated all data shards"),
        );
    }
    payload_bytes.truncate(payload_len);

    decode_payload_bytes(manifest, &payload_bytes)
}

fn decode_payload_bytes(manifest: &DaManifest, payload_bytes: &[u8]) -> Result<DaPayload, DaError> {
    let payload_hash = hash_bytes(payload_bytes);
    if payload_hash != manifest.payload_hash {
        return Err(DaError::PayloadHashMismatch {
            expected: manifest.payload_hash.clone(),
            actual: payload_hash,
        });
    }

    let payload: DaPayload =
        serde_json::from_slice(payload_bytes).map_err(|_| DaError::DecodeFailed)?;
    payload.validate()?;
    let canonical = payload.canonicalized();
    if canonical != payload {
        return Err(DaError::PayloadNotCanonical);
    }
    let namespace_root = canonical.namespace_root()?;
    if namespace_root != manifest.namespace_root {
        return Err(DaError::NamespaceRootMismatch {
            expected: manifest.namespace_root.clone(),
            actual: namespace_root,
        });
    }
    Ok(canonical)
}

fn reed_solomon_codec(manifest: &DaManifest) -> Result<ReedSolomon, DaError> {
    let data_share_count =
        usize::try_from(manifest.original_share_count).map_err(|_| DaError::ShareCountOverflow)?;
    let parity_share_count =
        usize::try_from(manifest.encoded_share_count - manifest.original_share_count)
            .map_err(|_| DaError::ShareCountOverflow)?;
    ReedSolomon::new(data_share_count, parity_share_count).map_err(da_erasure_error)
}

impl From<reed_solomon_erasure::Error> for DaError {
    fn from(error: reed_solomon_erasure::Error) -> Self {
        da_erasure_error(error)
    }
}

fn da_erasure_error(error: reed_solomon_erasure::Error) -> DaError {
    DaError::ErasureCodingFailed(error.to_string())
}

pub fn build_da_share_set(
    payload: &DaPayload,
    block_hash: impl Into<String>,
    share_size_bytes: usize,
) -> Result<DaShareSet, DaError> {
    if share_size_bytes == 0 {
        return Err(DaError::InvalidChunkSize);
    }

    let payload = payload.canonicalized();
    payload.validate()?;
    let payload_bytes = canonical_bytes(&payload)?;
    let payload_hash = hash_bytes(&payload_bytes);
    let chunks: Vec<Vec<u8>> = payload_bytes
        .chunks(share_size_bytes)
        .map(|chunk| chunk.to_vec())
        .collect();
    let share_hashes: Vec<String> = chunks.iter().map(|chunk| hash_bytes(chunk)).collect();
    let encoded_share_count =
        u32::try_from(share_hashes.len()).map_err(|_| DaError::ShareCountOverflow)?;
    let manifest = DaManifest {
        schema: DA_MANIFEST_SCHEMA.into(),
        schema_version: 1,
        chain_id: payload.chain_id.clone(),
        height: payload.height,
        block_hash: block_hash.into(),
        payload_hash,
        payload_bytes: payload_bytes.len() as u64,
        namespace_root: payload.namespace_root()?,
        share_root: share_root(&share_hashes)?,
        erasure_scheme: ErasureScheme::DeterministicChunks,
        original_share_count: encoded_share_count,
        encoded_share_count,
        reconstruction_threshold: encoded_share_count,
        share_size_bytes: u32::try_from(share_size_bytes).map_err(|_| DaError::InvalidChunkSize)?,
        share_hashes,
        namespace_ranges: namespace_ranges(&payload),
    };
    manifest.validate()?;
    let manifest_hash = manifest.manifest_hash()?;
    let shares = chunks
        .into_iter()
        .enumerate()
        .map(|(index, bytes)| DaShare {
            manifest_hash: manifest_hash.clone(),
            index: index as u32,
            share_hash: hash_bytes(&bytes),
            bytes,
        })
        .collect();
    Ok(DaShareSet { manifest, shares })
}

pub fn build_reed_solomon_share_set(
    payload: &DaPayload,
    block_hash: impl Into<String>,
    data_share_count: u32,
    parity_share_count: u32,
) -> Result<DaShareSet, DaError> {
    if data_share_count == 0 || parity_share_count == 0 {
        return Err(DaError::InvalidManifest(
            "reed-solomon data and parity share counts must be positive".into(),
        ));
    }
    let encoded_share_count = data_share_count
        .checked_add(parity_share_count)
        .ok_or(DaError::ShareCountOverflow)?;
    if encoded_share_count > REED_SOLOMON_MAX_SHARES {
        return Err(DaError::ShareCountOverflow);
    }

    let payload = payload.canonicalized();
    payload.validate()?;
    let payload_bytes = canonical_bytes(&payload)?;
    let data_shards = usize::try_from(data_share_count).map_err(|_| DaError::ShareCountOverflow)?;
    let parity_shards =
        usize::try_from(parity_share_count).map_err(|_| DaError::ShareCountOverflow)?;
    let encoded_shards =
        usize::try_from(encoded_share_count).map_err(|_| DaError::ShareCountOverflow)?;
    let share_size_bytes = payload_bytes.len().div_ceil(data_shards);
    if share_size_bytes == 0 {
        return Err(DaError::InvalidChunkSize);
    }

    let mut shards = vec![vec![0_u8; share_size_bytes]; encoded_shards];
    for (index, chunk) in payload_bytes.chunks(share_size_bytes).enumerate() {
        shards[index][..chunk.len()].copy_from_slice(chunk);
    }
    ReedSolomon::new(data_shards, parity_shards)
        .map_err(da_erasure_error)?
        .encode(&mut shards)?;

    let payload_hash = hash_bytes(&payload_bytes);
    let share_hashes: Vec<String> = shards.iter().map(|shard| hash_bytes(shard)).collect();
    let manifest = DaManifest {
        schema: DA_MANIFEST_SCHEMA.into(),
        schema_version: 1,
        chain_id: payload.chain_id.clone(),
        height: payload.height,
        block_hash: block_hash.into(),
        payload_hash,
        payload_bytes: u64::try_from(payload_bytes.len()).map_err(|_| DaError::InvalidChunkSize)?,
        namespace_root: payload.namespace_root()?,
        share_root: share_root(&share_hashes)?,
        erasure_scheme: ErasureScheme::ReedSolomonV1,
        original_share_count: data_share_count,
        encoded_share_count,
        reconstruction_threshold: data_share_count,
        share_size_bytes: u32::try_from(share_size_bytes).map_err(|_| DaError::InvalidChunkSize)?,
        share_hashes,
        namespace_ranges: namespace_ranges(&payload),
    };
    manifest.validate()?;
    let manifest_hash = manifest.manifest_hash()?;
    let shares = shards
        .into_iter()
        .enumerate()
        .map(|(index, bytes)| DaShare {
            manifest_hash: manifest_hash.clone(),
            index: index as u32,
            share_hash: hash_bytes(&bytes),
            bytes,
        })
        .collect();
    Ok(DaShareSet { manifest, shares })
}

pub fn payload_hash(payload: &DaPayload) -> Result<String, DaError> {
    payload.hash()
}

pub fn hash_share_bytes(bytes: &[u8]) -> String {
    hash_bytes(bytes)
}

pub fn assigned_custody_share_indices(
    manifest: &DaManifest,
    validator_id: &str,
    custody_share_count: u32,
) -> Result<Vec<u32>, DaError> {
    manifest.validate()?;
    if validator_id.is_empty() {
        return Err(DaError::InvalidAvailabilityVote(
            "validator_id is empty".into(),
        ));
    }
    if custody_share_count == 0 {
        return Ok(Vec::new());
    }

    let manifest_hash = manifest.manifest_hash()?;
    let target = custody_share_count.min(manifest.encoded_share_count);
    let mut assigned = BTreeSet::new();
    let mut counter = 0_u64;
    while assigned.len() < target as usize {
        let mut hasher = Sha256::new();
        hasher.update(b"detta.da.custody.v1");
        hasher.update((manifest_hash.len() as u64).to_be_bytes());
        hasher.update(manifest_hash.as_bytes());
        hasher.update((validator_id.len() as u64).to_be_bytes());
        hasher.update(validator_id.as_bytes());
        hasher.update(counter.to_be_bytes());
        let digest = hasher.finalize();
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(&digest[..8]);
        assigned.insert((u64::from_be_bytes(bytes) % manifest.encoded_share_count as u64) as u32);
        counter = counter.saturating_add(1);
    }
    Ok(assigned.into_iter().collect())
}

pub fn verify_share_against_manifest(
    manifest: &DaManifest,
    share: &DaShare,
) -> Result<(), DaError> {
    manifest.validate()?;
    let manifest_hash = manifest.manifest_hash()?;
    if share.manifest_hash != manifest_hash {
        return Err(DaError::ManifestHashMismatch {
            expected: manifest_hash,
            actual: share.manifest_hash.clone(),
        });
    }
    if share.index >= manifest.encoded_share_count {
        return Err(DaError::UnexpectedShare {
            index: share.index,
            share_count: manifest.encoded_share_count,
        });
    }
    let actual_share_hash = hash_bytes(&share.bytes);
    let expected_share_hash = &manifest.share_hashes[share.index as usize];
    if share.share_hash != actual_share_hash || &share.share_hash != expected_share_hash {
        return Err(DaError::ShareHashMismatch { index: share.index });
    }
    Ok(())
}

pub fn verify_custody_shares(
    manifest: &DaManifest,
    shares: &[DaShare],
    custody_share_indices: &[u32],
) -> Result<(), DaError> {
    manifest.validate()?;
    let mut by_index = BTreeMap::new();
    for share in shares {
        verify_share_against_manifest(manifest, share)?;
        if by_index.insert(share.index, share).is_some() {
            return Err(DaError::DuplicateShare { index: share.index });
        }
    }
    for index in custody_share_indices {
        if !by_index.contains_key(index) {
            return Err(DaError::MissingShare { index: *index });
        }
    }
    Ok(())
}

pub fn verify_manifest_matches_challenge(
    manifest: &DaManifest,
    challenge: &DaShareChallenge,
) -> Result<(), DaError> {
    manifest.validate()?;
    challenge.validate()?;
    let manifest_hash = manifest.manifest_hash()?;
    if manifest_hash != challenge.manifest_hash {
        return Err(DaError::ManifestHashMismatch {
            expected: challenge.manifest_hash.clone(),
            actual: manifest_hash,
        });
    }
    if manifest.chain_id != challenge.chain_id
        || manifest.height != challenge.height
        || manifest.block_hash != challenge.block_hash
        || manifest.share_root != challenge.share_root
    {
        return Err(DaError::InvalidChallenge(
            "challenge does not match manifest".into(),
        ));
    }
    if challenge.share_index >= manifest.encoded_share_count {
        return Err(DaError::UnexpectedShare {
            index: challenge.share_index,
            share_count: manifest.encoded_share_count,
        });
    }
    Ok(())
}

pub fn derive_sample_schedule(
    manifest: &DaManifest,
    client_randomness: &[u8],
    sample_count: u32,
) -> Result<DaSamplingSchedule, DaError> {
    manifest.validate()?;
    if client_randomness.is_empty() {
        return Err(DaError::InvalidSampling(
            "client randomness must be nonempty".into(),
        ));
    }
    if sample_count == 0 {
        return Err(DaError::InvalidSampling(
            "sample_count must be positive".into(),
        ));
    }

    let manifest_hash = manifest.manifest_hash()?;
    let target = sample_count.min(manifest.encoded_share_count);
    let mut sampled = BTreeSet::new();
    let mut counter = 0_u64;
    while sampled.len() < target as usize {
        let mut hasher = Sha256::new();
        hasher.update(b"detta.da.sampling.v1");
        hasher.update((manifest_hash.len() as u64).to_be_bytes());
        hasher.update(manifest_hash.as_bytes());
        hasher.update((manifest.block_hash.len() as u64).to_be_bytes());
        hasher.update(manifest.block_hash.as_bytes());
        hasher.update((client_randomness.len() as u64).to_be_bytes());
        hasher.update(client_randomness);
        hasher.update(counter.to_be_bytes());
        let digest = hasher.finalize();
        let mut bytes = [0_u8; 8];
        bytes.copy_from_slice(&digest[..8]);
        sampled.insert((u64::from_be_bytes(bytes) % manifest.encoded_share_count as u64) as u32);
        counter = counter.saturating_add(1);
    }

    Ok(DaSamplingSchedule {
        manifest_hash,
        block_hash: manifest.block_hash.clone(),
        client_randomness_hash: hash_bytes(client_randomness),
        requested_sample_count: sample_count,
        share_indices: sampled.into_iter().collect(),
    })
}

pub fn prove_share_inclusion(
    manifest: &DaManifest,
    share_index: u32,
) -> Result<DaShareInclusionProof, DaError> {
    manifest.validate()?;
    if share_index >= manifest.encoded_share_count {
        return Err(DaError::UnexpectedShare {
            index: share_index,
            share_count: manifest.encoded_share_count,
        });
    }

    let leaves = manifest
        .share_hashes
        .iter()
        .map(merkle_leaf)
        .collect::<Result<Vec<_>, _>>()?;
    let siblings = merkle_inclusion_siblings(leaves, share_index as usize)?;
    Ok(DaShareInclusionProof {
        schema: DA_SAMPLE_PROOF_SCHEMA.into(),
        schema_version: 1,
        manifest_hash: manifest.manifest_hash()?,
        share_root: manifest.share_root.clone(),
        share_index,
        share_hash: manifest.share_hashes[share_index as usize].clone(),
        leaf_count: manifest.encoded_share_count,
        siblings,
    })
}

pub fn verify_share_inclusion_proof(
    manifest: &DaManifest,
    share: &DaShare,
    proof: &DaShareInclusionProof,
) -> Result<(), DaError> {
    manifest.validate()?;
    if proof.schema != DA_SAMPLE_PROOF_SCHEMA {
        return Err(DaError::InvalidSampling(format!(
            "unexpected sample proof schema {}",
            proof.schema
        )));
    }
    if proof.schema_version != 1 {
        return Err(DaError::InvalidSampling(format!(
            "unexpected sample proof schema version {}",
            proof.schema_version
        )));
    }
    let manifest_hash = manifest.manifest_hash()?;
    if proof.manifest_hash != manifest_hash {
        return Err(DaError::ManifestHashMismatch {
            expected: manifest_hash.clone(),
            actual: proof.manifest_hash.clone(),
        });
    }
    if proof.share_root != manifest.share_root {
        return Err(DaError::ShareRootMismatch {
            expected: manifest.share_root.clone(),
            actual: proof.share_root.clone(),
        });
    }
    if proof.leaf_count != manifest.encoded_share_count {
        return Err(DaError::ManifestShareCountMismatch {
            declared: manifest.encoded_share_count,
            actual: proof.leaf_count,
        });
    }
    if proof.share_index >= manifest.encoded_share_count || share.index != proof.share_index {
        return Err(DaError::UnexpectedShare {
            index: share.index,
            share_count: manifest.encoded_share_count,
        });
    }
    verify_share_against_manifest(manifest, share)?;
    let expected_share_hash = &manifest.share_hashes[proof.share_index as usize];
    if &proof.share_hash != expected_share_hash || share.share_hash != proof.share_hash {
        return Err(DaError::ShareHashMismatch {
            index: proof.share_index,
        });
    }

    let mut current = merkle_leaf(&proof.share_hash)?;
    for sibling in &proof.siblings {
        let sibling_hash = decode_hex_hash(&sibling.hash)?;
        current = match sibling.side {
            DaMerkleSiblingSide::Left => merkle_parent(&sibling_hash, &current),
            DaMerkleSiblingSide::Right => merkle_parent(&current, &sibling_hash),
        };
    }
    let actual_root = hex_lower(&current);
    if actual_root != proof.share_root {
        return Err(DaError::ShareRootMismatch {
            expected: proof.share_root.clone(),
            actual: actual_root,
        });
    }
    Ok(())
}

pub fn prove_namespace(
    manifest: &DaManifest,
    namespace: &DaNamespace,
) -> Result<DaNamespaceProof, DaError> {
    manifest.validate()?;
    let range = manifest
        .namespace_ranges
        .iter()
        .find(|range| &range.namespace == namespace)
        .cloned();
    Ok(DaNamespaceProof {
        manifest_hash: manifest.manifest_hash()?,
        namespace_root: manifest.namespace_root.clone(),
        namespace: namespace.clone(),
        range,
        namespace_ranges: manifest.namespace_ranges.clone(),
    })
}

pub fn verify_namespace_proof(
    manifest: &DaManifest,
    proof: &DaNamespaceProof,
) -> Result<Option<DaNamespaceRange>, DaError> {
    manifest.validate()?;
    let manifest_hash = manifest.manifest_hash()?;
    if proof.manifest_hash != manifest_hash {
        return Err(DaError::ManifestHashMismatch {
            expected: manifest_hash,
            actual: proof.manifest_hash.clone(),
        });
    }
    let expected_namespace_root = namespace_root_for_ranges(&proof.namespace_ranges)?;
    if proof.namespace_root != expected_namespace_root {
        return Err(DaError::NamespaceRootMismatch {
            expected: expected_namespace_root,
            actual: proof.namespace_root.clone(),
        });
    }
    if proof.namespace_root != manifest.namespace_root {
        return Err(DaError::NamespaceRootMismatch {
            expected: manifest.namespace_root.clone(),
            actual: proof.namespace_root.clone(),
        });
    }
    validate_namespace_ranges(&proof.namespace_ranges)?;
    if proof.namespace_ranges != manifest.namespace_ranges {
        return Err(DaError::NamespaceRootMismatch {
            expected: namespace_root_for_ranges(&manifest.namespace_ranges)?,
            actual: namespace_root_for_ranges(&proof.namespace_ranges)?,
        });
    }
    let actual_range = proof
        .namespace_ranges
        .iter()
        .find(|range| range.namespace == proof.namespace)
        .cloned();
    if proof.range != actual_range {
        return Err(DaError::InvalidSampling(
            "namespace proof range does not match committed ranges".into(),
        ));
    }
    Ok(actual_range)
}

pub fn verify_light_client_samples(
    manifest: &DaManifest,
    client_randomness: &[u8],
    sample_count: u32,
    sample_proofs: &[DaSampleProof],
    namespace_proofs: &[DaNamespaceProof],
) -> Result<DaLightClientSamplingReport, DaError> {
    let schedule = derive_sample_schedule(manifest, client_randomness, sample_count)?;
    let expected_indices = &schedule.share_indices;
    let actual_indices = sample_proofs
        .iter()
        .map(|proof| proof.share.index)
        .collect::<Vec<_>>();
    if &actual_indices != expected_indices {
        return Err(DaError::InvalidSampling(format!(
            "sample proof indices {actual_indices:?} do not match schedule {expected_indices:?}"
        )));
    }
    for proof in sample_proofs {
        verify_share_inclusion_proof(manifest, &proof.share, &proof.inclusion_proof)?;
    }
    for proof in namespace_proofs {
        verify_namespace_proof(manifest, proof)?;
    }

    Ok(DaLightClientSamplingReport {
        manifest_hash: schedule.manifest_hash,
        block_hash: schedule.block_hash,
        client_randomness_hash: schedule.client_randomness_hash,
        requested_sample_count: schedule.requested_sample_count,
        sampled_share_indices: schedule.share_indices,
        verified_share_count: actual_indices.len() as u32,
        namespace_proof_count: namespace_proofs.len() as u32,
        valid: true,
    })
}

fn namespace_ranges(payload: &DaPayload) -> Vec<DaNamespaceRange> {
    payload
        .namespaces
        .iter()
        .enumerate()
        .map(|(index, section)| DaNamespaceRange {
            namespace: section.namespace.clone(),
            section_index: index as u32,
            record_count: section.records.len() as u32,
        })
        .collect()
}

fn validate_namespace_ranges(ranges: &[DaNamespaceRange]) -> Result<(), DaError> {
    let mut previous_namespace: Option<&DaNamespace> = None;
    for (position, range) in ranges.iter().enumerate() {
        if range.section_index != position as u32 {
            return Err(DaError::InvalidManifest(
                "namespace ranges must be ordered by section index".into(),
            ));
        }
        if range.record_count == 0 {
            return Err(DaError::InvalidManifest(
                "namespace ranges must have positive record counts".into(),
            ));
        }
        if let Some(previous) = previous_namespace {
            if previous >= &range.namespace {
                return Err(DaError::InvalidManifest(
                    "namespace ranges must be unique and sorted".into(),
                ));
            }
        }
        previous_namespace = Some(&range.namespace);
    }
    Ok(())
}

fn namespace_root_for_ranges(ranges: &[DaNamespaceRange]) -> Result<String, DaError> {
    hash_canonical(&ranges)
}

fn share_root(share_hashes: &[String]) -> Result<String, DaError> {
    merkle_root(share_hashes)
}

fn canonical_signers(signers: impl IntoIterator<Item = String>) -> Result<Vec<String>, DaError> {
    let mut unique = BTreeSet::new();
    for signer in signers {
        if signer.is_empty() {
            return Err(DaError::InvalidAvailabilityCertificate(
                "signer is empty".into(),
            ));
        }
        if !unique.insert(signer.clone()) {
            return Err(DaError::DuplicateAvailabilitySigner(signer));
        }
    }
    if unique.is_empty() {
        return Err(DaError::InvalidAvailabilityCertificate(
            "signers must be nonempty".into(),
        ));
    }
    Ok(unique.into_iter().collect())
}

fn canonical_share_indices(
    indices: impl IntoIterator<Item = u32>,
    share_count: u32,
) -> Result<Vec<u32>, DaError> {
    let mut unique = BTreeSet::new();
    for index in indices {
        if index >= share_count {
            return Err(DaError::UnexpectedShare { index, share_count });
        }
        if !unique.insert(index) {
            return Err(DaError::DuplicateShare { index });
        }
    }
    Ok(unique.into_iter().collect())
}

fn ensure_sorted_unique_indices(indices: &[u32], field: &str) -> Result<(), DaError> {
    let mut previous = None;
    for index in indices {
        if let Some(previous) = previous {
            if previous >= *index {
                return Err(DaError::InvalidAvailabilityVote(format!(
                    "{field} must be sorted and unique"
                )));
            }
        }
        previous = Some(*index);
    }
    Ok(())
}

fn merkle_root<T: Serialize>(values: &[T]) -> Result<String, DaError> {
    let leaves: Result<Vec<_>, _> = values.iter().map(merkle_leaf).collect();
    Ok(hex_lower(&merkle_root_bytes(leaves?)))
}

fn merkle_leaf<T: Serialize>(value: &T) -> Result<Vec<u8>, DaError> {
    let bytes = canonical_bytes(value)?;
    let mut hasher = Sha256::new();
    hasher.update(b"detta.da.leaf.v1");
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    Ok(hasher.finalize().to_vec())
}

fn merkle_root_bytes(mut level: Vec<Vec<u8>>) -> Vec<u8> {
    if level.is_empty() {
        return Sha256::digest(b"detta.da.empty.v1").to_vec();
    }
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let left = &pair[0];
            let right = pair.get(1).unwrap_or(left);
            next.push(merkle_parent(left, right));
        }
        level = next;
    }
    level.remove(0)
}

fn merkle_inclusion_siblings(
    mut level: Vec<Vec<u8>>,
    mut index: usize,
) -> Result<Vec<DaMerkleSibling>, DaError> {
    if level.is_empty() || index >= level.len() {
        return Err(DaError::UnexpectedShare {
            index: index as u32,
            share_count: level.len() as u32,
        });
    }

    let mut siblings = Vec::new();
    while level.len() > 1 {
        let is_right = index % 2 == 1;
        let sibling_index = if is_right {
            index - 1
        } else if index + 1 < level.len() {
            index + 1
        } else {
            index
        };
        siblings.push(DaMerkleSibling {
            side: if is_right {
                DaMerkleSiblingSide::Left
            } else {
                DaMerkleSiblingSide::Right
            },
            hash: hex_lower(&level[sibling_index]),
        });

        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let left = &pair[0];
            let right = pair.get(1).unwrap_or(left);
            next.push(merkle_parent(left, right));
        }
        index /= 2;
        level = next;
    }
    Ok(siblings)
}

fn merkle_parent(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"detta.da.node.v1");
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().to_vec()
}

fn hash_canonical<T: Serialize>(value: &T) -> Result<String, DaError> {
    canonical_bytes(value).map(|bytes| hash_bytes(&bytes))
}

fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, DaError> {
    serde_json::to_vec(value).map_err(|_| DaError::EncodeFailed)
}

fn hash_bytes(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex_hash(value: &str) -> Result<Vec<u8>, DaError> {
    if value.len() != 64 || !value.len().is_multiple_of(2) {
        return Err(DaError::InvalidSampling(
            "merkle proof hash must be 32 lowercase hex bytes".into(),
        ));
    }
    let mut bytes = Vec::with_capacity(value.len() / 2);
    for chunk in value.as_bytes().chunks_exact(2) {
        let high = hex_nibble(chunk[0])?;
        let low = hex_nibble(chunk[1])?;
        bytes.push((high << 4) | low);
    }
    Ok(bytes)
}

fn hex_nibble(byte: u8) -> Result<u8, DaError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(DaError::InvalidSampling(
            "merkle proof hashes must be lowercase hex".into(),
        )),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DaError {
    InvalidNamespace(String),
    InvalidPayload(String),
    UnsupportedPayloadVersion { expected: u32, actual: u32 },
    InvalidManifest(String),
    InvalidAvailabilityVote(String),
    InvalidAvailabilityCertificate(String),
    DuplicateAvailabilitySigner(String),
    InvalidChunkSize,
    UnsupportedErasureScheme,
    ManifestShareCountMismatch { declared: u32, actual: u32 },
    ShareRootMismatch { expected: String, actual: String },
    ManifestHashMismatch { expected: String, actual: String },
    UnexpectedShare { index: u32, share_count: u32 },
    DuplicateShare { index: u32 },
    ShareHashMismatch { index: u32 },
    InsufficientShares { required: u32, actual: u32 },
    MissingShare { index: u32 },
    TotalBytesMismatch { expected: u64, actual: u64 },
    PayloadHashMismatch { expected: String, actual: String },
    NamespaceRootMismatch { expected: String, actual: String },
    PayloadNotCanonical,
    ShareCountOverflow,
    ErasureCodingFailed(String),
    InvalidChallenge(String),
    InvalidChallengeResponse(String),
    InvalidSampling(String),
    EncodeFailed,
    DecodeFailed,
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Argument, Method};

    fn tx(hash: &str, nonce: u64) -> Transaction {
        Transaction {
            chain_id: "detta-test".into(),
            tx_hash: hash.into(),
            sender: "Alice".into(),
            nonce,
            valid_until_height: Some(100),
            target: "TokenA".into(),
            method: Method::Transfer,
            args: vec![
                Argument::Principal("Bob".into()),
                Argument::Asset("USDC".into()),
                Argument::Amount(10),
            ],
            signature_ok: true,
            budget: 10_000,
        }
    }

    fn payload_with_sections(sections: Vec<DaNamespaceSection>) -> DaPayload {
        DaPayload::new("detta-test", 7, "prev-block", sections).unwrap()
    }

    fn tx_section(namespace: &str, records: Vec<DaRecord>) -> DaNamespaceSection {
        DaNamespaceSection::new(DaNamespace::new(namespace).unwrap(), records).unwrap()
    }

    fn payload_with_tx_count(count: u64) -> DaPayload {
        let records = (0..count)
            .map(|index| {
                let tx_hash = format!("tx-{index}");
                DaRecord::SignedTransaction(tx(&tx_hash, index))
            })
            .collect();
        payload_with_sections(vec![tx_section("detta.tx", records)])
    }

    #[test]
    fn share_set_reconstructs_canonical_payload() {
        let payload = payload_with_sections(vec![
            tx_section(
                "detta.receipt",
                vec![DaRecord::GovernancePayload {
                    proposal_id: "proposal-1".into(),
                    payload: "raise-da-limit".into(),
                }],
            ),
            tx_section(
                "detta.tx",
                vec![
                    DaRecord::SignedTransaction(tx("tx-1", 1)),
                    DaRecord::SignedTransaction(tx("tx-2", 2)),
                ],
            ),
        ]);

        let share_set = DaShareSet::from_payload(&payload, "block-7", 64).unwrap();

        assert!(share_set.shares.len() > 1);
        assert_eq!(share_set.manifest.chain_id, "detta-test");
        assert_eq!(share_set.manifest.height, 7);
        assert_eq!(
            share_set.reconstruct_payload().unwrap(),
            payload.canonicalized()
        );
        share_set.verify().unwrap();
    }

    #[test]
    fn reed_solomon_share_set_reconstructs_from_threshold_subset() {
        let payload = payload_with_sections(vec![
            tx_section(
                "detta.governance",
                vec![DaRecord::GovernancePayload {
                    proposal_id: "proposal-1".into(),
                    payload: "raise-da-limit".into(),
                }],
            ),
            tx_section(
                "detta.tx",
                vec![
                    DaRecord::SignedTransaction(tx("tx-1", 1)),
                    DaRecord::SignedTransaction(tx("tx-2", 2)),
                    DaRecord::SignedTransaction(tx("tx-3", 3)),
                ],
            ),
        ]);
        let share_set = DaShareSet::from_payload_reed_solomon(&payload, "block-7", 4, 2).unwrap();

        assert_eq!(
            share_set.manifest.erasure_scheme,
            ErasureScheme::ReedSolomonV1
        );
        assert_eq!(share_set.manifest.original_share_count, 4);
        assert_eq!(share_set.manifest.encoded_share_count, 6);
        assert_eq!(share_set.manifest.reconstruction_threshold, 4);

        let threshold_subset = DaShareSet {
            manifest: share_set.manifest.clone(),
            shares: share_set
                .shares
                .iter()
                .filter(|share| [0, 2, 4, 5].contains(&share.index))
                .cloned()
                .collect(),
        };

        assert_eq!(
            threshold_subset.reconstruct_payload().unwrap(),
            payload.canonicalized()
        );
        threshold_subset.verify().unwrap();
    }

    #[test]
    fn reed_solomon_rejects_insufficient_shares() {
        let payload = payload_with_tx_count(5);
        let share_set = DaShareSet::from_payload_reed_solomon(&payload, "block-7", 4, 2).unwrap();
        let partial = DaShareSet {
            manifest: share_set.manifest.clone(),
            shares: share_set.shares.iter().take(3).cloned().collect(),
        };

        assert!(matches!(
            partial.verify(),
            Err(DaError::InsufficientShares {
                required: 4,
                actual: 3
            })
        ));
    }

    #[test]
    fn reed_solomon_rejects_duplicate_wrong_index_and_tampered_shares() {
        let payload = payload_with_tx_count(5);
        let share_set = DaShareSet::from_payload_reed_solomon(&payload, "block-7", 4, 2).unwrap();

        let mut duplicate = DaShareSet {
            manifest: share_set.manifest.clone(),
            shares: share_set.shares.iter().take(4).cloned().collect(),
        };
        duplicate.shares.push(duplicate.shares[0].clone());
        assert!(matches!(
            duplicate.verify(),
            Err(DaError::DuplicateShare { index: 0 })
        ));

        let mut wrong_index = DaShareSet {
            manifest: share_set.manifest.clone(),
            shares: share_set.shares.iter().take(4).cloned().collect(),
        };
        wrong_index.shares[0].index = share_set.manifest.encoded_share_count;
        assert!(matches!(
            wrong_index.verify(),
            Err(DaError::UnexpectedShare { .. })
        ));

        let mut tampered = DaShareSet {
            manifest: share_set.manifest.clone(),
            shares: share_set.shares.iter().take(4).cloned().collect(),
        };
        tampered.shares[0].bytes[0] ^= 0x01;
        assert!(matches!(
            tampered.verify(),
            Err(DaError::ShareHashMismatch { index: 0 })
        ));
    }

    #[test]
    fn reed_solomon_reconstructs_over_payload_sizes_and_missing_patterns() {
        let patterns = [[0_u32, 1, 2], [0, 3, 4], [1, 4, 5], [3, 4, 5]];

        for tx_count in [1_u64, 2, 5, 13] {
            let payload = payload_with_tx_count(tx_count);
            let share_set =
                DaShareSet::from_payload_reed_solomon(&payload, "block-7", 3, 3).unwrap();

            for pattern in patterns {
                let subset = DaShareSet {
                    manifest: share_set.manifest.clone(),
                    shares: share_set
                        .shares
                        .iter()
                        .filter(|share| pattern.contains(&share.index))
                        .cloned()
                        .collect(),
                };
                assert_eq!(
                    subset.reconstruct_payload().unwrap(),
                    payload.canonicalized()
                );
            }
        }
    }

    #[test]
    fn namespace_order_is_canonical() {
        let first = payload_with_sections(vec![
            tx_section("detta.tx", vec![DaRecord::SignedTransaction(tx("tx-1", 1))]),
            tx_section(
                "detta.governance",
                vec![DaRecord::GovernancePayload {
                    proposal_id: "proposal-1".into(),
                    payload: "upgrade".into(),
                }],
            ),
        ]);
        let second = payload_with_sections(vec![
            tx_section(
                "detta.governance",
                vec![DaRecord::GovernancePayload {
                    proposal_id: "proposal-1".into(),
                    payload: "upgrade".into(),
                }],
            ),
            tx_section("detta.tx", vec![DaRecord::SignedTransaction(tx("tx-1", 1))]),
        ]);

        assert_eq!(
            payload_hash(&first).unwrap(),
            payload_hash(&second).unwrap()
        );
        assert_eq!(
            DaShareSet::from_payload(&first, "block-7", 128)
                .unwrap()
                .manifest
                .manifest_hash()
                .unwrap(),
            DaShareSet::from_payload(&second, "block-7", 128)
                .unwrap()
                .manifest
                .manifest_hash()
                .unwrap()
        );
    }

    #[test]
    fn tampered_share_is_rejected() {
        let payload = payload_with_sections(vec![tx_section(
            "detta.tx",
            vec![DaRecord::SignedTransaction(tx("tx-1", 1))],
        )]);
        let mut share_set = DaShareSet::from_payload(&payload, "block-7", 64).unwrap();
        share_set.shares[0].bytes[0] ^= 0x01;

        assert!(matches!(
            share_set.verify(),
            Err(DaError::ShareHashMismatch { index: 0 })
        ));
    }

    #[test]
    fn missing_share_is_rejected() {
        let payload = payload_with_sections(vec![tx_section(
            "detta.tx",
            vec![
                DaRecord::SignedTransaction(tx("tx-1", 1)),
                DaRecord::SignedTransaction(tx("tx-2", 2)),
            ],
        )]);
        let mut share_set = DaShareSet::from_payload(&payload, "block-7", 48).unwrap();
        assert!(share_set.shares.len() > 1);
        share_set.shares.pop();

        assert!(matches!(
            share_set.verify(),
            Err(DaError::InsufficientShares { .. })
        ));
    }

    #[test]
    fn wrong_manifest_hash_is_rejected() {
        let payload = payload_with_sections(vec![tx_section(
            "detta.tx",
            vec![DaRecord::SignedTransaction(tx("tx-1", 1))],
        )]);
        let mut share_set = DaShareSet::from_payload(&payload, "block-7", 64).unwrap();
        share_set.shares[0].manifest_hash = "bad-manifest".into();

        assert!(matches!(
            share_set.verify(),
            Err(DaError::ManifestHashMismatch { .. })
        ));
    }

    #[test]
    fn invalid_namespace_is_rejected() {
        assert!(matches!(
            DaNamespace::new("DeTTa.Tx"),
            Err(DaError::InvalidNamespace(_))
        ));
        assert!(matches!(
            DaNamespace::new("detta..tx"),
            Err(DaError::InvalidNamespace(_))
        ));
    }

    #[test]
    fn availability_vote_and_certificate_bind_manifest_commitments() {
        let payload = payload_with_sections(vec![tx_section(
            "detta.tx",
            vec![DaRecord::SignedTransaction(tx("tx-1", 1))],
        )]);
        let share_set = DaShareSet::from_payload(&payload, "block-7", 96).unwrap();
        let manifest_hash = share_set.manifest.manifest_hash().unwrap();

        let vote = DaAvailabilityVote::from_manifest(&share_set.manifest, "validator-2").unwrap();
        assert_eq!(vote.chain_id, share_set.manifest.chain_id);
        assert_eq!(vote.height, share_set.manifest.height);
        assert_eq!(vote.block_hash, "block-7");
        assert_eq!(vote.manifest_hash, manifest_hash);
        assert_eq!(vote.share_root, share_set.manifest.share_root);
        assert!(!vote.vote_hash().unwrap().is_empty());

        let first = DaAvailabilityCertificate::from_manifest(
            &share_set.manifest,
            vec!["validator-2".into(), "validator-1".into()],
        )
        .unwrap();
        let second = DaAvailabilityCertificate::from_manifest(
            &share_set.manifest,
            vec!["validator-1".into(), "validator-2".into()],
        )
        .unwrap();
        assert_eq!(
            first.signers,
            vec!["validator-1".to_string(), "validator-2".to_string()]
        );
        assert_eq!(first.certificate_hash(), second.certificate_hash());
    }

    #[test]
    fn availability_certificate_rejects_duplicate_or_unsorted_signers() {
        let payload = payload_with_sections(vec![tx_section(
            "detta.tx",
            vec![DaRecord::SignedTransaction(tx("tx-1", 1))],
        )]);
        let share_set = DaShareSet::from_payload(&payload, "block-7", 96).unwrap();

        assert!(matches!(
            DaAvailabilityCertificate::from_manifest(
                &share_set.manifest,
                vec!["validator-1".into(), "validator-1".into()]
            ),
            Err(DaError::DuplicateAvailabilitySigner(_))
        ));

        let mut certificate = DaAvailabilityCertificate::from_manifest(
            &share_set.manifest,
            vec!["validator-1".into(), "validator-2".into()],
        )
        .unwrap();
        certificate.signers.swap(0, 1);
        assert!(matches!(
            certificate.validate(),
            Err(DaError::InvalidAvailabilityCertificate(_))
        ));
    }

    #[test]
    fn custody_assignment_is_deterministic_and_bounded() {
        let payload = payload_with_sections(vec![tx_section(
            "detta.tx",
            vec![
                DaRecord::SignedTransaction(tx("tx-1", 1)),
                DaRecord::SignedTransaction(tx("tx-2", 2)),
            ],
        )]);
        let share_set = DaShareSet::from_payload(&payload, "block-7", 32).unwrap();

        let first = assigned_custody_share_indices(&share_set.manifest, "validator-1", 3).unwrap();
        let second = assigned_custody_share_indices(&share_set.manifest, "validator-1", 3).unwrap();
        let different =
            assigned_custody_share_indices(&share_set.manifest, "validator-2", 3).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.len(), 3);
        assert!(first.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(first
            .iter()
            .all(|index| *index < share_set.manifest.encoded_share_count));
        assert_eq!(different.len(), 3);
    }

    #[test]
    fn custody_vote_requires_assigned_shares_to_verify() {
        let payload = payload_with_sections(vec![tx_section(
            "detta.tx",
            vec![
                DaRecord::SignedTransaction(tx("tx-1", 1)),
                DaRecord::SignedTransaction(tx("tx-2", 2)),
            ],
        )]);
        let share_set = DaShareSet::from_payload(&payload, "block-7", 32).unwrap();
        let custody_indices =
            assigned_custody_share_indices(&share_set.manifest, "validator-1", 2).unwrap();
        let available: Vec<_> = share_set
            .shares
            .iter()
            .filter(|share| custody_indices.contains(&share.index))
            .cloned()
            .collect();

        let vote = DaAvailabilityVote::from_verified_custody(
            &share_set.manifest,
            &available,
            "validator-1",
            2,
        )
        .unwrap();

        assert_eq!(vote.custody_share_indices, custody_indices);

        let missing_one = &available[..available.len() - 1];
        assert!(matches!(
            DaAvailabilityVote::from_verified_custody(
                &share_set.manifest,
                missing_one,
                "validator-1",
                2,
            ),
            Err(DaError::MissingShare { .. })
        ));

        let mut tampered = available.clone();
        tampered[0].bytes[0] ^= 0x01;
        assert!(matches!(
            DaAvailabilityVote::from_verified_custody(
                &share_set.manifest,
                &tampered,
                "validator-1",
                2,
            ),
            Err(DaError::ShareHashMismatch { .. })
        ));
    }

    #[test]
    fn da_share_challenge_requires_signed_custody_or_sample_index() {
        let payload = payload_with_tx_count(3);
        let share_set = DaShareSet::from_payload(&payload, "block-7", 32).unwrap();
        let custody_indices =
            assigned_custody_share_indices(&share_set.manifest, "validator-1", 2).unwrap();
        let available: Vec<_> = share_set
            .shares
            .iter()
            .filter(|share| custody_indices.contains(&share.index))
            .cloned()
            .collect();
        let vote = DaAvailabilityVote::from_verified_custody(
            &share_set.manifest,
            &available,
            "validator-1",
            2,
        )
        .unwrap();

        let challenge =
            DaShareChallenge::from_availability_vote(&vote, "validator-2", custody_indices[0], 12)
                .unwrap();
        assert_eq!(challenge.challenged_validator_id, "validator-1");
        assert_eq!(challenge.challenger_id, "validator-2");
        assert_eq!(challenge.share_index, custody_indices[0]);
        assert_eq!(challenge.availability_vote_hash, vote.vote_hash().unwrap());
        assert!(!challenge.challenge_hash().unwrap().is_empty());

        let unsigned_index = (0..share_set.manifest.encoded_share_count)
            .find(|index| !custody_indices.contains(index))
            .unwrap();
        assert!(matches!(
            DaShareChallenge::from_availability_vote(&vote, "validator-2", unsigned_index, 12),
            Err(DaError::InvalidChallenge(_))
        ));
    }

    #[test]
    fn da_share_challenge_evidence_distinguishes_valid_invalid_and_missing_responses() {
        let payload = payload_with_tx_count(4);
        let share_set = DaShareSet::from_payload(&payload, "block-7", 32).unwrap();
        let custody_indices =
            assigned_custody_share_indices(&share_set.manifest, "validator-1", 1).unwrap();
        let challenged_share = share_set
            .shares
            .iter()
            .find(|share| share.index == custody_indices[0])
            .unwrap()
            .clone();
        let vote = DaAvailabilityVote::from_verified_custody(
            &share_set.manifest,
            std::slice::from_ref(&challenged_share),
            "validator-1",
            1,
        )
        .unwrap();
        let challenge = DaShareChallenge::from_availability_vote(
            &vote,
            "validator-2",
            challenged_share.index,
            12,
        )
        .unwrap();
        let valid_response =
            DaShareChallengeResponse::from_share(&challenge, challenged_share.clone()).unwrap();
        assert!(matches!(
            DaChallengeEvidence::invalid_response(
                &challenge,
                &valid_response,
                &share_set.manifest,
                "validator-2",
                10,
            ),
            Err(DaError::InvalidChallengeResponse(_))
        ));

        let mut tampered_share = challenged_share;
        tampered_share.bytes[0] ^= 0x01;
        let invalid_response =
            DaShareChallengeResponse::from_share(&challenge, tampered_share).unwrap();
        let evidence = DaChallengeEvidence::invalid_response(
            &challenge,
            &invalid_response,
            &share_set.manifest,
            "validator-2",
            10,
        )
        .unwrap();
        assert_eq!(evidence.challenged_validator_id, "validator-1");
        assert_eq!(evidence.reporter_id, "validator-2");
        assert_eq!(
            evidence.response_hash,
            Some(invalid_response.response_hash().unwrap())
        );
        assert!(matches!(
            evidence.fault,
            DaChallengeFault::InvalidResponse { .. }
        ));

        assert!(matches!(
            DaChallengeEvidence::missing_response(&challenge, "validator-2", 12),
            Err(DaError::InvalidChallenge(_))
        ));
        let missing = DaChallengeEvidence::missing_response(&challenge, "validator-2", 13).unwrap();
        assert_eq!(missing.response_hash, None);
        assert_eq!(missing.fault, DaChallengeFault::MissingResponse);
    }

    #[test]
    fn light_client_sampling_schedule_and_proofs_verify() {
        let payload = payload_with_tx_count(7);
        let share_set = DaShareSet::from_payload_reed_solomon(&payload, "block-7", 4, 2).unwrap();
        let schedule =
            derive_sample_schedule(&share_set.manifest, b"client-randomness", 3).unwrap();
        let repeated =
            derive_sample_schedule(&share_set.manifest, b"client-randomness", 3).unwrap();
        let different =
            derive_sample_schedule(&share_set.manifest, b"different-randomness", 3).unwrap();

        assert_eq!(schedule, repeated);
        assert_ne!(
            schedule.client_randomness_hash,
            different.client_randomness_hash
        );
        assert_eq!(schedule.share_indices.len(), 3);
        assert!(schedule
            .share_indices
            .windows(2)
            .all(|pair| pair[0] < pair[1]));

        let sample_proofs = schedule
            .share_indices
            .iter()
            .map(|index| DaSampleProof {
                share: share_set
                    .shares
                    .iter()
                    .find(|share| share.index == *index)
                    .unwrap()
                    .clone(),
                inclusion_proof: prove_share_inclusion(&share_set.manifest, *index).unwrap(),
            })
            .collect::<Vec<_>>();
        let namespace_proof =
            prove_namespace(&share_set.manifest, &DaNamespace::new("detta.tx").unwrap()).unwrap();

        let report = verify_light_client_samples(
            &share_set.manifest,
            b"client-randomness",
            3,
            &sample_proofs,
            std::slice::from_ref(&namespace_proof),
        )
        .unwrap();

        assert!(report.valid);
        assert_eq!(report.sampled_share_indices, schedule.share_indices);
        assert_eq!(report.verified_share_count, 3);
        assert_eq!(report.namespace_proof_count, 1);
        assert_eq!(
            verify_namespace_proof(&share_set.manifest, &namespace_proof)
                .unwrap()
                .unwrap()
                .namespace,
            DaNamespace::new("detta.tx").unwrap()
        );
    }

    #[test]
    fn light_client_sampling_rejects_missing_wrong_and_tampered_samples() {
        let payload = payload_with_tx_count(7);
        let share_set = DaShareSet::from_payload_reed_solomon(&payload, "block-7", 4, 2).unwrap();
        let schedule =
            derive_sample_schedule(&share_set.manifest, b"client-randomness", 3).unwrap();
        let sample_proofs = schedule
            .share_indices
            .iter()
            .map(|index| DaSampleProof {
                share: share_set
                    .shares
                    .iter()
                    .find(|share| share.index == *index)
                    .unwrap()
                    .clone(),
                inclusion_proof: prove_share_inclusion(&share_set.manifest, *index).unwrap(),
            })
            .collect::<Vec<_>>();

        assert!(matches!(
            verify_light_client_samples(
                &share_set.manifest,
                b"client-randomness",
                3,
                &sample_proofs[..2],
                &[],
            ),
            Err(DaError::InvalidSampling(_))
        ));

        let mut wrong_order = sample_proofs.clone();
        wrong_order.swap(0, 1);
        assert!(matches!(
            verify_light_client_samples(
                &share_set.manifest,
                b"client-randomness",
                3,
                &wrong_order,
                &[],
            ),
            Err(DaError::InvalidSampling(_))
        ));

        let mut tampered_share = sample_proofs.clone();
        tampered_share[0].share.bytes[0] ^= 0x01;
        assert!(matches!(
            verify_light_client_samples(
                &share_set.manifest,
                b"client-randomness",
                3,
                &tampered_share,
                &[],
            ),
            Err(DaError::ShareHashMismatch { .. })
        ));

        let mut tampered_proof = sample_proofs;
        tampered_proof[0].inclusion_proof.siblings[0].hash =
            "0000000000000000000000000000000000000000000000000000000000000000".into();
        assert!(matches!(
            verify_light_client_samples(
                &share_set.manifest,
                b"client-randomness",
                3,
                &tampered_proof,
                &[],
            ),
            Err(DaError::ShareRootMismatch { .. })
        ));
    }

    #[test]
    fn namespace_proof_verifies_committed_ranges() {
        let payload = payload_with_sections(vec![
            tx_section(
                "detta.governance",
                vec![DaRecord::GovernancePayload {
                    proposal_id: "proposal-1".into(),
                    payload: "upgrade".into(),
                }],
            ),
            tx_section("detta.tx", vec![DaRecord::SignedTransaction(tx("tx-1", 1))]),
        ]);
        let share_set = DaShareSet::from_payload(&payload, "block-7", 96).unwrap();
        let namespace = DaNamespace::new("detta.governance").unwrap();
        let mut proof = prove_namespace(&share_set.manifest, &namespace).unwrap();

        assert_eq!(
            verify_namespace_proof(&share_set.manifest, &proof).unwrap(),
            Some(DaNamespaceRange {
                namespace,
                section_index: 0,
                record_count: 1,
            })
        );

        proof.namespace_ranges.swap(0, 1);
        assert!(matches!(
            verify_namespace_proof(&share_set.manifest, &proof),
            Err(DaError::NamespaceRootMismatch { .. }) | Err(DaError::InvalidManifest(_))
        ));
    }

    #[test]
    fn golden_da_fixture_hashes_are_stable() {
        let payload = payload_with_sections(vec![
            tx_section("detta.tx", vec![DaRecord::SignedTransaction(tx("tx-1", 1))]),
            tx_section(
                "detta.governance",
                vec![DaRecord::GovernancePayload {
                    proposal_id: "proposal-1".into(),
                    payload: "upgrade".into(),
                }],
            ),
        ]);
        let share_set = DaShareSet::from_payload(&payload, "block-7", 96).unwrap();

        assert_eq!(
            payload_hash(&payload).unwrap(),
            "c73e1108fb52c5a6d592a527c6256763d59054b6aaee47dbe1519d9a63f1a63a"
        );
        assert_eq!(
            share_set.manifest.manifest_hash().unwrap(),
            "bed89f48658083db6f0b6142687de2c83f167de00985b11afba4be3fe23b33f8"
        );
        assert_eq!(
            share_set.manifest.share_root,
            "360419c027ff23ced0c23f4671cf595ad790bd6b4e13f3c3b83f65227d1afa04"
        );
        assert_eq!(share_set.manifest.encoded_share_count, 6);
    }
}
