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
pub const DA_CODING_FRAUD_PROOF_SCHEMA: &str = "detta.da-coding-fraud-proof.v1";
pub const DA_PAYLOAD_SCHEMA: &str = "detta.da-payload.v1";
pub const DA_PRODUCTION_PROFILE_SCHEMA: &str = "detta.da-production-profile.v1";
pub const DA_APPLICATION_PROFILE_SCHEMA: &str = "detta.da-application-profile.v1";
pub const APPLICATION_DA_PAYLOAD_SCHEMA: &str = "detta.application-da-payload.v1";
pub const APPLICATION_DA_MANIFEST_SCHEMA: &str = "detta.application-da-manifest.v1";
pub const DA_APPLICATION_VALIDATION_REPORT_SCHEMA: &str =
    "detta.da-application-validation-report.v1";
pub const APPLICATION_DA_CODING_FRAUD_PROOF_SCHEMA: &str =
    "detta.application-da-coding-fraud-proof.v1";
pub const DA_PAYLOAD_VERSION: u32 = 1;
pub const REED_SOLOMON_MAX_SHARES: u32 = 256;
pub const DA_V1_MIN_CUSTODY_SHARE_COUNT: u32 = 2;
pub const DA_V1_MIN_LIGHT_CLIENT_SAMPLE_COUNT: u32 = 3;
pub const DA_V1_DATA_GAS_BYTES_PER_UNIT: u64 = 1024;
pub const DA_V1_VALIDATOR_MIN_RETENTION_BLOCKS: u64 = 65_536;
pub const DA_V1_ARCHIVE_MIN_RETENTION_BLOCKS: u64 = 1_048_576;
pub const DA_APPLICATION_ID_MAX_BYTES: usize = 128;
pub const DA_APPLICATION_STREAM_ID_MAX_BYTES: usize = 128;
pub const DA_APPLICATION_PROFILE_NAME_MAX_BYTES: usize = 128;
pub const DA_APPLICATION_ROOT_NAME_MAX_BYTES: usize = 128;
pub const DA_RECORD_SCHEMA_ID_MAX_BYTES: usize = 128;
pub const DA_RECORD_CONTENT_TYPE_MAX_BYTES: usize = 128;

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

    pub fn validate(&self) -> Result<(), DaError> {
        DaNamespace::new(self.0.clone()).map(|_| ())
    }
}

impl From<DaNamespace> for String {
    fn from(namespace: DaNamespace) -> Self {
        namespace.0
    }
}

fn namespace_unchecked(value: &str) -> DaNamespace {
    DaNamespace::new(value).expect("checked-in DA namespace constants must be valid")
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
    Application(DaRecordEnvelope),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DaCommitmentScheme {
    MerkleSha256V1,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DaCustodyMode {
    DeterministicCustodyWithLightClientSampling,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DaEventAvailabilityMode {
    RegeneratedFromExecutionRoots,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DaSlashingGovernanceMode {
    TimelockedValidatorSetGovernance,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DaArchiveIncentiveMode {
    GovernanceRegisteredStorageProviders,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaProductionProfile {
    pub schema: String,
    pub schema_version: u32,
    pub commitment_scheme: DaCommitmentScheme,
    pub erasure_scheme: ErasureScheme,
    pub custody_mode: DaCustodyMode,
    pub min_custody_share_count: u32,
    pub min_light_client_sample_count: u32,
    pub full_payload_required_for_rpc: bool,
    pub receipts_are_payload_records: bool,
    pub event_availability_mode: DaEventAvailabilityMode,
    pub validator_min_retention_blocks: u64,
    pub archive_min_retention_blocks: u64,
    pub data_gas_bytes_per_unit: u64,
    pub slashing_governance_mode: DaSlashingGovernanceMode,
    pub archive_incentive_mode: DaArchiveIncentiveMode,
    pub mandatory_da_namespaces: Vec<DaNamespace>,
}

impl DaProductionProfile {
    pub fn v1() -> Self {
        Self {
            schema: DA_PRODUCTION_PROFILE_SCHEMA.into(),
            schema_version: 1,
            commitment_scheme: DaCommitmentScheme::MerkleSha256V1,
            erasure_scheme: ErasureScheme::ReedSolomonV1,
            custody_mode: DaCustodyMode::DeterministicCustodyWithLightClientSampling,
            min_custody_share_count: DA_V1_MIN_CUSTODY_SHARE_COUNT,
            min_light_client_sample_count: DA_V1_MIN_LIGHT_CLIENT_SAMPLE_COUNT,
            full_payload_required_for_rpc: true,
            receipts_are_payload_records: true,
            event_availability_mode: DaEventAvailabilityMode::RegeneratedFromExecutionRoots,
            validator_min_retention_blocks: DA_V1_VALIDATOR_MIN_RETENTION_BLOCKS,
            archive_min_retention_blocks: DA_V1_ARCHIVE_MIN_RETENTION_BLOCKS,
            data_gas_bytes_per_unit: DA_V1_DATA_GAS_BYTES_PER_UNIT,
            slashing_governance_mode: DaSlashingGovernanceMode::TimelockedValidatorSetGovernance,
            archive_incentive_mode: DaArchiveIncentiveMode::GovernanceRegisteredStorageProviders,
            mandatory_da_namespaces: vec![
                namespace_unchecked("detta.aspect"),
                namespace_unchecked("detta.block"),
                namespace_unchecked("detta.bridge"),
                namespace_unchecked("detta.governance"),
                namespace_unchecked("detta.oracle"),
                namespace_unchecked("detta.receipt"),
                namespace_unchecked("detta.tx"),
            ],
        }
    }

    pub fn validate(&self) -> Result<(), DaError> {
        if self.schema != DA_PRODUCTION_PROFILE_SCHEMA {
            return Err(DaError::InvalidManifest(format!(
                "unexpected DA production profile schema {}",
                self.schema
            )));
        }
        if self.schema_version != 1 {
            return Err(DaError::InvalidManifest(format!(
                "unexpected DA production profile version {}",
                self.schema_version
            )));
        }
        if self.commitment_scheme != DaCommitmentScheme::MerkleSha256V1 {
            return Err(DaError::UnsupportedErasureScheme);
        }
        if self.erasure_scheme != ErasureScheme::ReedSolomonV1 {
            return Err(DaError::UnsupportedErasureScheme);
        }
        if self.custody_mode != DaCustodyMode::DeterministicCustodyWithLightClientSampling {
            return Err(DaError::InvalidAvailabilityVote(
                "unsupported DA custody mode".into(),
            ));
        }
        if self.min_custody_share_count == 0 || self.min_light_client_sample_count == 0 {
            return Err(DaError::InvalidSampling(
                "custody and sample counts must be positive".into(),
            ));
        }
        if !self.full_payload_required_for_rpc {
            return Err(DaError::InvalidPayload(
                "production DA RPC requires full verified payloads".into(),
            ));
        }
        if !self.receipts_are_payload_records {
            return Err(DaError::InvalidPayload(
                "production DA requires receipts as payload records".into(),
            ));
        }
        if self.event_availability_mode != DaEventAvailabilityMode::RegeneratedFromExecutionRoots {
            return Err(DaError::InvalidPayload(
                "unsupported production event availability mode".into(),
            ));
        }
        if self.validator_min_retention_blocks == 0
            || self.archive_min_retention_blocks < self.validator_min_retention_blocks
        {
            return Err(DaError::InvalidManifest(
                "archive retention must be at least validator retention".into(),
            ));
        }
        if self.data_gas_bytes_per_unit == 0 {
            return Err(DaError::InvalidManifest(
                "DA data gas bytes per unit must be positive".into(),
            ));
        }
        validate_sorted_unique_namespaces(&self.mandatory_da_namespaces)?;
        for required in [
            "detta.block",
            "detta.tx",
            "detta.receipt",
            "detta.aspect",
            "detta.governance",
            "detta.bridge",
            "detta.oracle",
        ] {
            if !self
                .mandatory_da_namespaces
                .iter()
                .any(|namespace| namespace.0 == required)
            {
                return Err(DaError::InvalidManifest(format!(
                    "production DA profile is missing required namespace {required}"
                )));
            }
        }
        Ok(())
    }

    pub fn data_gas_for_bytes(&self, payload_bytes: u64) -> Result<u64, DaError> {
        self.validate()?;
        Ok(payload_bytes.div_ceil(self.data_gas_bytes_per_unit))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct DaApplicationId(pub String);

impl DaApplicationId {
    pub fn new(value: impl Into<String>) -> Result<Self, DaError> {
        let value = value.into();
        validate_application_id(&value)?;
        Ok(Self(value))
    }

    pub fn validate(&self) -> Result<(), DaError> {
        validate_application_id(&self.0)
    }
}

impl From<DaApplicationId> for String {
    fn from(application_id: DaApplicationId) -> Self {
        application_id.0
    }
}

fn application_id_unchecked(value: &str) -> DaApplicationId {
    DaApplicationId::new(value).expect("checked-in DA application ids must be valid")
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaApplicationCoordinate {
    pub application_id: DaApplicationId,
    pub stream_id: String,
    pub sequence: u64,
    pub epoch: Option<u64>,
    pub parent_hash: Option<String>,
    pub subject_hash: Option<String>,
}

impl DaApplicationCoordinate {
    pub fn validate(&self, policy: &DaCoordinatePolicy) -> Result<(), DaError> {
        self.application_id.validate()?;
        validate_stream_id(&self.stream_id, policy.max_stream_id_bytes as usize)?;
        if !policy.allow_epoch && self.epoch.is_some() {
            return Err(DaError::InvalidManifest(
                "application coordinate carries an epoch but the profile forbids epochs".into(),
            ));
        }
        if policy.require_parent_hash && self.parent_hash.is_none() {
            return Err(DaError::InvalidManifest(
                "application coordinate is missing required parent_hash".into(),
            ));
        }
        if policy.require_subject_hash && self.subject_hash.is_none() {
            return Err(DaError::InvalidManifest(
                "application coordinate is missing required subject_hash".into(),
            ));
        }
        validate_optional_hash("parent_hash", self.parent_hash.as_deref())?;
        validate_optional_hash("subject_hash", self.subject_hash.as_deref())?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum DaPayloadKind {
    Block,
    Batch,
    Checkpoint,
    Snapshot,
    MediaManifest,
    ModerationLog,
    IndexDelta,
    ApplicationDefined(String),
}

impl DaPayloadKind {
    pub fn validate(&self) -> Result<(), DaError> {
        if let Self::ApplicationDefined(value) = self {
            validate_schema_id("application payload kind", value)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum DaRecordEncoding {
    CanonicalJson,
    OpaqueBytes,
    EncryptedBytes,
    ExternalContentAddress,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum DaNamespaceRequirement {
    Required,
    Optional,
    Forbidden,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum DaApplicationRetentionClass {
    Hot,
    Warm,
    Cold,
    Archive,
    Checkpoint,
    Custom(String),
}

impl DaApplicationRetentionClass {
    pub fn validate(&self) -> Result<(), DaError> {
        if let Self::Custom(value) = self {
            validate_schema_id("application retention class", value)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DaApplicationValidationMode {
    OpaqueBytes,
    SchemaDecodableRecords,
    ApplicationAdapterVerified,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DaApplicationPrivacyMode {
    Public,
    Encrypted,
    CommitmentOnly,
    MixedExplicit,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaNamespacePolicy {
    pub namespace: DaNamespace,
    pub requirement: DaNamespaceRequirement,
    pub allowed_record_schemas: Vec<String>,
    pub min_records: u32,
    pub max_records: u32,
    pub retention_class: DaApplicationRetentionClass,
}

impl DaNamespacePolicy {
    pub fn validate(&self) -> Result<(), DaError> {
        self.namespace.validate()?;
        self.retention_class.validate()?;
        match self.requirement {
            DaNamespaceRequirement::Forbidden => {
                if self.min_records != 0
                    || self.max_records != 0
                    || !self.allowed_record_schemas.is_empty()
                {
                    return Err(DaError::InvalidManifest(format!(
                        "forbidden namespace {} cannot allow records",
                        self.namespace.0
                    )));
                }
            }
            DaNamespaceRequirement::Required | DaNamespaceRequirement::Optional => {
                if self.max_records == 0 || self.min_records > self.max_records {
                    return Err(DaError::InvalidManifest(format!(
                        "namespace {} has invalid record bounds",
                        self.namespace.0
                    )));
                }
                if self.requirement == DaNamespaceRequirement::Required && self.min_records == 0 {
                    return Err(DaError::InvalidManifest(format!(
                        "required namespace {} must require at least one record",
                        self.namespace.0
                    )));
                }
                validate_sorted_unique_schema_ids(
                    "namespace allowed_record_schemas",
                    &self.allowed_record_schemas,
                )?;
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaRecordPolicy {
    pub schema: String,
    pub schema_version: u32,
    pub allowed_namespaces: Vec<DaNamespace>,
    pub allowed_encodings: Vec<DaRecordEncoding>,
    pub max_record_bytes: u64,
    pub require_content_hash: bool,
    pub require_signer: bool,
}

impl DaRecordPolicy {
    pub fn validate(&self) -> Result<(), DaError> {
        validate_schema_id("record schema", &self.schema)?;
        if self.schema_version == 0 {
            return Err(DaError::InvalidManifest(format!(
                "record schema {} has version 0",
                self.schema
            )));
        }
        if self.max_record_bytes == 0 {
            return Err(DaError::InvalidManifest(format!(
                "record schema {} has max_record_bytes 0",
                self.schema
            )));
        }
        if self.allowed_namespaces.is_empty() {
            return Err(DaError::InvalidManifest(format!(
                "record schema {} has no allowed namespaces",
                self.schema
            )));
        }
        validate_sorted_unique_namespaces(&self.allowed_namespaces)?;
        if self.allowed_encodings.is_empty() {
            return Err(DaError::InvalidManifest(format!(
                "record schema {} has no allowed encodings",
                self.schema
            )));
        }
        validate_sorted_unique_record_encodings(&self.allowed_encodings)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaCoordinatePolicy {
    pub max_stream_id_bytes: u32,
    pub allow_epoch: bool,
    pub require_parent_hash: bool,
    pub require_subject_hash: bool,
}

impl DaCoordinatePolicy {
    pub fn validate(&self) -> Result<(), DaError> {
        if self.max_stream_id_bytes == 0
            || self.max_stream_id_bytes as usize > DA_APPLICATION_STREAM_ID_MAX_BYTES
        {
            return Err(DaError::InvalidManifest(
                "coordinate max_stream_id_bytes is outside supported bounds".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaApplicationRootPolicy {
    pub name: String,
    pub required: bool,
}

impl DaApplicationRootPolicy {
    pub fn validate(&self) -> Result<(), DaError> {
        validate_root_name(&self.name)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaNamespaceRetentionPolicy {
    pub namespace: DaNamespace,
    pub retention_class: DaApplicationRetentionClass,
}

impl DaNamespaceRetentionPolicy {
    pub fn validate(&self) -> Result<(), DaError> {
        self.namespace.validate()?;
        self.retention_class.validate()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaPayloadKindRetentionPolicy {
    pub payload_kind: DaPayloadKind,
    pub retention_class: DaApplicationRetentionClass,
}

impl DaPayloadKindRetentionPolicy {
    pub fn validate(&self) -> Result<(), DaError> {
        self.payload_kind.validate()?;
        self.retention_class.validate()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaApplicationRetentionPolicy {
    pub default_class: DaApplicationRetentionClass,
    pub namespace_overrides: Vec<DaNamespaceRetentionPolicy>,
    pub payload_kind_overrides: Vec<DaPayloadKindRetentionPolicy>,
}

impl DaApplicationRetentionPolicy {
    pub fn validate(&self) -> Result<(), DaError> {
        self.default_class.validate()?;
        validate_sorted_unique_namespace_retention_policies(&self.namespace_overrides)?;
        for override_policy in &self.namespace_overrides {
            override_policy.validate()?;
        }
        validate_sorted_unique_payload_kind_retention_policies(&self.payload_kind_overrides)?;
        for override_policy in &self.payload_kind_overrides {
            override_policy.validate()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaApplicationProfile {
    pub schema: String,
    pub schema_version: u32,
    pub application_id: DaApplicationId,
    pub profile_version: u32,
    pub profile_name: String,
    pub da_profile: DaProductionProfile,
    pub namespace_policies: Vec<DaNamespacePolicy>,
    pub record_policies: Vec<DaRecordPolicy>,
    pub coordinate_policy: DaCoordinatePolicy,
    pub root_bindings: Vec<DaApplicationRootPolicy>,
    pub retention_policy: DaApplicationRetentionPolicy,
    pub validation_mode: DaApplicationValidationMode,
    pub privacy_mode: DaApplicationPrivacyMode,
    pub max_payload_bytes: u64,
    pub max_records_per_payload: u32,
}

impl DaApplicationProfile {
    pub fn profile_hash(&self) -> Result<String, DaError> {
        self.validate()?;
        hash_canonical(self)
    }

    pub fn profile_id(&self) -> Result<String, DaError> {
        self.profile_hash()
    }

    pub fn canonicalized(&self) -> Self {
        let mut profile = self.clone();
        profile
            .namespace_policies
            .sort_by(|left, right| left.namespace.cmp(&right.namespace));
        for policy in &mut profile.namespace_policies {
            policy.allowed_record_schemas.sort();
        }
        profile.record_policies.sort_by(|left, right| {
            left.schema
                .cmp(&right.schema)
                .then(left.schema_version.cmp(&right.schema_version))
        });
        for policy in &mut profile.record_policies {
            policy.allowed_namespaces.sort();
            policy.allowed_encodings.sort();
        }
        profile
            .root_bindings
            .sort_by(|left, right| left.name.cmp(&right.name));
        profile
            .retention_policy
            .namespace_overrides
            .sort_by(|left, right| left.namespace.cmp(&right.namespace));
        profile
            .retention_policy
            .payload_kind_overrides
            .sort_by(|left, right| left.payload_kind.cmp(&right.payload_kind));
        profile
    }

    pub fn validate(&self) -> Result<(), DaError> {
        if self.schema != DA_APPLICATION_PROFILE_SCHEMA {
            return Err(DaError::InvalidManifest(format!(
                "unexpected application DA profile schema {}",
                self.schema
            )));
        }
        if self.schema_version != 1 {
            return Err(DaError::InvalidManifest(format!(
                "unexpected application DA profile version {}",
                self.schema_version
            )));
        }
        if &self.canonicalized() != self {
            return Err(DaError::InvalidManifest(
                "application DA profile must be canonicalized before hashing".into(),
            ));
        }
        self.application_id.validate()?;
        if self.profile_version == 0 {
            return Err(DaError::InvalidManifest(
                "application DA profile_version must be positive".into(),
            ));
        }
        validate_label(
            "application profile_name",
            &self.profile_name,
            DA_APPLICATION_PROFILE_NAME_MAX_BYTES,
        )?;
        self.da_profile.validate()?;
        if self.max_payload_bytes == 0 || self.max_records_per_payload == 0 {
            return Err(DaError::InvalidManifest(
                "application DA payload bounds must be positive".into(),
            ));
        }
        validate_sorted_unique_namespace_policies(&self.namespace_policies)?;
        validate_sorted_unique_record_policies(&self.record_policies)?;
        validate_sorted_unique_root_policies(&self.root_bindings)?;
        self.coordinate_policy.validate()?;
        self.retention_policy.validate()?;

        let mut known_namespaces: BTreeMap<DaNamespace, DaNamespaceRequirement> = BTreeMap::new();
        let mut referenced_schema_ids = BTreeSet::new();
        let mut required_namespace_count = 0_u32;
        for policy in &self.namespace_policies {
            policy.validate()?;
            if policy.requirement == DaNamespaceRequirement::Required {
                required_namespace_count += 1;
            }
            if policy.requirement != DaNamespaceRequirement::Forbidden {
                for schema in &policy.allowed_record_schemas {
                    referenced_schema_ids.insert(schema.clone());
                }
            }
            known_namespaces.insert(policy.namespace.clone(), policy.requirement.clone());
        }
        if required_namespace_count == 0 {
            return Err(DaError::InvalidManifest(
                "application DA profile must declare at least one required namespace".into(),
            ));
        }

        let mut known_record_schema_ids = BTreeSet::new();
        for policy in &self.record_policies {
            policy.validate()?;
            known_record_schema_ids.insert(policy.schema.clone());
            for namespace in &policy.allowed_namespaces {
                match known_namespaces.get(namespace) {
                    Some(DaNamespaceRequirement::Required)
                    | Some(DaNamespaceRequirement::Optional) => {}
                    Some(DaNamespaceRequirement::Forbidden) => {
                        return Err(DaError::InvalidManifest(format!(
                            "record schema {} allows forbidden namespace {}",
                            policy.schema, namespace.0
                        )));
                    }
                    None => {
                        return Err(DaError::InvalidManifest(format!(
                            "record schema {} allows unknown namespace {}",
                            policy.schema, namespace.0
                        )));
                    }
                }
            }
        }
        if self.validation_mode != DaApplicationValidationMode::OpaqueBytes
            && known_record_schema_ids.is_empty()
        {
            return Err(DaError::InvalidManifest(
                "non-opaque application DA profiles must declare record policies".into(),
            ));
        }
        if self.validation_mode != DaApplicationValidationMode::OpaqueBytes {
            for schema in &referenced_schema_ids {
                if !known_record_schema_ids.contains(schema) {
                    return Err(DaError::InvalidManifest(format!(
                        "namespace policy references unknown record schema {schema}"
                    )));
                }
            }
            for schema in &known_record_schema_ids {
                if !referenced_schema_ids.contains(schema) {
                    return Err(DaError::InvalidManifest(format!(
                        "record schema {schema} is not allowed by any namespace policy"
                    )));
                }
            }
        }

        for policy in &self.record_policies {
            for namespace in &policy.allowed_namespaces {
                let namespace_policy = self
                    .namespace_policies
                    .iter()
                    .find(|candidate| candidate.namespace == *namespace)
                    .ok_or_else(|| {
                        DaError::InvalidManifest(format!(
                            "record schema {} allows unknown namespace {}",
                            policy.schema, namespace.0
                        ))
                    })?;
                if !namespace_policy
                    .allowed_record_schemas
                    .iter()
                    .any(|schema| schema == &policy.schema)
                {
                    return Err(DaError::InvalidManifest(format!(
                        "namespace {} does not allow record schema {}",
                        namespace.0, policy.schema
                    )));
                }
            }
        }
        for override_policy in &self.retention_policy.namespace_overrides {
            if !matches!(
                known_namespaces.get(&override_policy.namespace),
                Some(DaNamespaceRequirement::Required) | Some(DaNamespaceRequirement::Optional)
            ) {
                return Err(DaError::InvalidManifest(format!(
                    "retention override targets unknown or forbidden namespace {}",
                    override_policy.namespace.0
                )));
            }
        }

        Ok(())
    }

    pub fn detta_defi_v1() -> Self {
        Self {
            schema: DA_APPLICATION_PROFILE_SCHEMA.into(),
            schema_version: 1,
            application_id: application_id_unchecked("detta.defi"),
            profile_version: 1,
            profile_name: "DeTTa DeFi DA v1".into(),
            da_profile: DaProductionProfile::v1(),
            namespace_policies: vec![
                namespace_policy(
                    "detta.aspect",
                    DaNamespaceRequirement::Optional,
                    vec!["detta.aspect.artifact"],
                    0,
                    4096,
                    DaApplicationRetentionClass::Archive,
                ),
                namespace_policy(
                    "detta.block",
                    DaNamespaceRequirement::Required,
                    vec!["detta.block.header"],
                    1,
                    1,
                    DaApplicationRetentionClass::Archive,
                ),
                namespace_policy(
                    "detta.bridge",
                    DaNamespaceRequirement::Optional,
                    vec!["detta.bridge.proof"],
                    0,
                    4096,
                    DaApplicationRetentionClass::Archive,
                ),
                namespace_policy(
                    "detta.event",
                    DaNamespaceRequirement::Optional,
                    vec!["detta.event"],
                    0,
                    50_000,
                    DaApplicationRetentionClass::Warm,
                ),
                namespace_policy(
                    "detta.governance",
                    DaNamespaceRequirement::Optional,
                    vec!["detta.governance.payload"],
                    0,
                    4096,
                    DaApplicationRetentionClass::Archive,
                ),
                namespace_policy(
                    "detta.oracle",
                    DaNamespaceRequirement::Optional,
                    vec!["detta.oracle.evidence"],
                    0,
                    4096,
                    DaApplicationRetentionClass::Archive,
                ),
                namespace_policy(
                    "detta.receipt",
                    DaNamespaceRequirement::Optional,
                    vec!["detta.receipt"],
                    0,
                    50_000,
                    DaApplicationRetentionClass::Archive,
                ),
                namespace_policy(
                    "detta.tx",
                    DaNamespaceRequirement::Optional,
                    vec!["detta.tx.signed"],
                    0,
                    50_000,
                    DaApplicationRetentionClass::Archive,
                ),
            ],
            record_policies: vec![
                record_policy(
                    "detta.aspect.artifact",
                    vec!["detta.aspect"],
                    vec![DaRecordEncoding::CanonicalJson],
                    512 * 1024,
                    false,
                ),
                record_policy(
                    "detta.block.header",
                    vec!["detta.block"],
                    vec![DaRecordEncoding::CanonicalJson],
                    64 * 1024,
                    false,
                ),
                record_policy(
                    "detta.bridge.proof",
                    vec!["detta.bridge"],
                    vec![DaRecordEncoding::CanonicalJson],
                    512 * 1024,
                    false,
                ),
                record_policy(
                    "detta.event",
                    vec!["detta.event"],
                    vec![DaRecordEncoding::CanonicalJson],
                    128 * 1024,
                    false,
                ),
                record_policy(
                    "detta.governance.payload",
                    vec!["detta.governance"],
                    vec![DaRecordEncoding::CanonicalJson],
                    512 * 1024,
                    false,
                ),
                record_policy(
                    "detta.oracle.evidence",
                    vec!["detta.oracle"],
                    vec![DaRecordEncoding::CanonicalJson],
                    512 * 1024,
                    false,
                ),
                record_policy(
                    "detta.receipt",
                    vec!["detta.receipt"],
                    vec![DaRecordEncoding::CanonicalJson],
                    128 * 1024,
                    false,
                ),
                record_policy(
                    "detta.tx.signed",
                    vec!["detta.tx"],
                    vec![DaRecordEncoding::CanonicalJson],
                    128 * 1024,
                    true,
                ),
            ],
            coordinate_policy: DaCoordinatePolicy {
                max_stream_id_bytes: DA_APPLICATION_STREAM_ID_MAX_BYTES as u32,
                allow_epoch: true,
                require_parent_hash: false,
                require_subject_hash: false,
            },
            root_bindings: vec![
                root_policy("detta.block.hash", true),
                root_policy("detta.global.state.root", true),
                root_policy("detta.receipt.root", true),
                root_policy("detta.tx.root", true),
            ],
            retention_policy: DaApplicationRetentionPolicy {
                default_class: DaApplicationRetentionClass::Archive,
                namespace_overrides: vec![DaNamespaceRetentionPolicy {
                    namespace: namespace_unchecked("detta.event"),
                    retention_class: DaApplicationRetentionClass::Warm,
                }],
                payload_kind_overrides: vec![DaPayloadKindRetentionPolicy {
                    payload_kind: DaPayloadKind::Block,
                    retention_class: DaApplicationRetentionClass::Archive,
                }],
            },
            validation_mode: DaApplicationValidationMode::ApplicationAdapterVerified,
            privacy_mode: DaApplicationPrivacyMode::Public,
            max_payload_bytes: 16 * 1024 * 1024,
            max_records_per_payload: 150_000,
        }
    }

    pub fn social_demo_v1() -> Self {
        Self {
            schema: DA_APPLICATION_PROFILE_SCHEMA.into(),
            schema_version: 1,
            application_id: application_id_unchecked("social.demo"),
            profile_version: 1,
            profile_name: "Social Demo DA v1".into(),
            da_profile: DaProductionProfile::v1(),
            namespace_policies: vec![
                namespace_policy(
                    "social.feed",
                    DaNamespaceRequirement::Required,
                    vec!["social.post"],
                    1,
                    10_000,
                    DaApplicationRetentionClass::Warm,
                ),
                namespace_policy(
                    "social.media",
                    DaNamespaceRequirement::Optional,
                    vec!["social.media.reference"],
                    0,
                    10_000,
                    DaApplicationRetentionClass::Cold,
                ),
                namespace_policy(
                    "social.moderation",
                    DaNamespaceRequirement::Optional,
                    vec!["social.moderation.action"],
                    0,
                    10_000,
                    DaApplicationRetentionClass::Archive,
                ),
                namespace_policy(
                    "social.private",
                    DaNamespaceRequirement::Optional,
                    vec!["social.private.message"],
                    0,
                    10_000,
                    DaApplicationRetentionClass::Cold,
                ),
            ],
            record_policies: vec![
                record_policy(
                    "social.media.reference",
                    vec!["social.media"],
                    vec![DaRecordEncoding::ExternalContentAddress],
                    16 * 1024,
                    false,
                ),
                record_policy(
                    "social.moderation.action",
                    vec!["social.moderation"],
                    vec![DaRecordEncoding::CanonicalJson],
                    64 * 1024,
                    true,
                ),
                record_policy(
                    "social.post",
                    vec!["social.feed"],
                    vec![DaRecordEncoding::CanonicalJson],
                    256 * 1024,
                    true,
                ),
                record_policy(
                    "social.private.message",
                    vec!["social.private"],
                    vec![DaRecordEncoding::EncryptedBytes],
                    256 * 1024,
                    true,
                ),
            ],
            coordinate_policy: DaCoordinatePolicy {
                max_stream_id_bytes: DA_APPLICATION_STREAM_ID_MAX_BYTES as u32,
                allow_epoch: true,
                require_parent_hash: false,
                require_subject_hash: false,
            },
            root_bindings: vec![root_policy("social.event.log.root", true)],
            retention_policy: DaApplicationRetentionPolicy {
                default_class: DaApplicationRetentionClass::Warm,
                namespace_overrides: vec![
                    DaNamespaceRetentionPolicy {
                        namespace: namespace_unchecked("social.media"),
                        retention_class: DaApplicationRetentionClass::Cold,
                    },
                    DaNamespaceRetentionPolicy {
                        namespace: namespace_unchecked("social.moderation"),
                        retention_class: DaApplicationRetentionClass::Archive,
                    },
                    DaNamespaceRetentionPolicy {
                        namespace: namespace_unchecked("social.private"),
                        retention_class: DaApplicationRetentionClass::Cold,
                    },
                ],
                payload_kind_overrides: vec![
                    DaPayloadKindRetentionPolicy {
                        payload_kind: DaPayloadKind::Batch,
                        retention_class: DaApplicationRetentionClass::Warm,
                    },
                    DaPayloadKindRetentionPolicy {
                        payload_kind: DaPayloadKind::MediaManifest,
                        retention_class: DaApplicationRetentionClass::Cold,
                    },
                    DaPayloadKindRetentionPolicy {
                        payload_kind: DaPayloadKind::ModerationLog,
                        retention_class: DaApplicationRetentionClass::Archive,
                    },
                ],
            },
            validation_mode: DaApplicationValidationMode::SchemaDecodableRecords,
            privacy_mode: DaApplicationPrivacyMode::MixedExplicit,
            max_payload_bytes: 16 * 1024 * 1024,
            max_records_per_payload: 100_000,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DaApplicationProfileStatus {
    Active,
    Deprecated,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaApplicationProfileRegistration {
    pub profile_id: String,
    pub profile: DaApplicationProfile,
    pub status: DaApplicationProfileStatus,
    pub activated_at_sequence: Option<u64>,
    pub deprecated_at_sequence: Option<u64>,
}

impl DaApplicationProfileRegistration {
    pub fn active(profile: DaApplicationProfile) -> Result<Self, DaError> {
        let profile_id = profile.profile_id()?;
        let registration = Self {
            profile_id,
            profile,
            status: DaApplicationProfileStatus::Active,
            activated_at_sequence: None,
            deprecated_at_sequence: None,
        };
        registration.validate()?;
        Ok(registration)
    }

    pub fn validate(&self) -> Result<(), DaError> {
        self.profile.validate()?;
        if self.profile_id != self.profile.profile_id()? {
            return Err(DaError::InvalidManifest(
                "application profile registration id does not match profile hash".into(),
            ));
        }
        match self.status {
            DaApplicationProfileStatus::Active => {
                if self.deprecated_at_sequence.is_some() {
                    return Err(DaError::InvalidManifest(
                        "active application profile registration has deprecation sequence".into(),
                    ));
                }
            }
            DaApplicationProfileStatus::Deprecated => {
                if self.deprecated_at_sequence.is_none() {
                    return Err(DaError::InvalidManifest(
                        "deprecated application profile registration lacks deprecation sequence"
                            .into(),
                    ));
                }
            }
        }
        Ok(())
    }

    pub fn is_active(&self) -> bool {
        self.status == DaApplicationProfileStatus::Active
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct DaApplicationProfileRegistry {
    profiles_by_id: BTreeMap<String, DaApplicationProfileRegistration>,
    profile_ids_by_application_version: BTreeMap<(DaApplicationId, u32), String>,
}

impl DaApplicationProfileRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_builtin_profiles() -> Result<Self, DaError> {
        let mut registry = Self::new();
        registry.register(DaApplicationProfile::detta_defi_v1())?;
        registry.register(DaApplicationProfile::social_demo_v1())?;
        Ok(registry)
    }

    pub fn from_registrations(
        registrations: Vec<DaApplicationProfileRegistration>,
    ) -> Result<Self, DaError> {
        let mut registry = Self::new();
        for registration in registrations {
            registry.insert_registration(registration)?;
        }
        Ok(registry)
    }

    pub fn register(&mut self, profile: DaApplicationProfile) -> Result<String, DaError> {
        let registration = DaApplicationProfileRegistration::active(profile)?;
        let profile_id = registration.profile_id.clone();
        self.insert_registration(registration)?;
        Ok(profile_id)
    }

    pub fn deprecate(
        &mut self,
        profile_id: &str,
        deprecated_at_sequence: u64,
    ) -> Result<(), DaError> {
        let registration = self.profiles_by_id.get_mut(profile_id).ok_or_else(|| {
            DaError::InvalidManifest(format!("unknown application profile id {profile_id}"))
        })?;
        registration.status = DaApplicationProfileStatus::Deprecated;
        registration.deprecated_at_sequence = Some(deprecated_at_sequence);
        registration.validate()
    }

    pub fn get(&self, profile_id: &str) -> Result<&DaApplicationProfileRegistration, DaError> {
        self.profiles_by_id.get(profile_id).ok_or_else(|| {
            DaError::InvalidManifest(format!("unknown application profile id {profile_id}"))
        })
    }

    pub fn get_profile(&self, profile_id: &str) -> Result<&DaApplicationProfile, DaError> {
        Ok(&self.get(profile_id)?.profile)
    }

    pub fn get_profile_version(
        &self,
        application_id: &DaApplicationId,
        profile_version: u32,
    ) -> Result<&DaApplicationProfileRegistration, DaError> {
        let profile_id = self
            .profile_ids_by_application_version
            .get(&(application_id.clone(), profile_version))
            .ok_or_else(|| {
                DaError::InvalidManifest(format!(
                    "unknown application profile {} version {profile_version}",
                    application_id.0
                ))
            })?;
        self.get(profile_id)
    }

    pub fn latest_active_profile(
        &self,
        application_id: &DaApplicationId,
    ) -> Option<&DaApplicationProfileRegistration> {
        self.profile_ids_by_application_version
            .iter()
            .filter(|((candidate, _), _)| candidate == application_id)
            .filter_map(|(_, profile_id)| self.profiles_by_id.get(profile_id))
            .filter(|registration| registration.is_active())
            .last()
    }

    pub fn profiles_for_application(
        &self,
        application_id: &DaApplicationId,
    ) -> Vec<&DaApplicationProfileRegistration> {
        self.profile_ids_by_application_version
            .iter()
            .filter(|((candidate, _), _)| candidate == application_id)
            .filter_map(|(_, profile_id)| self.profiles_by_id.get(profile_id))
            .collect()
    }

    pub fn registrations(&self) -> Vec<DaApplicationProfileRegistration> {
        self.profiles_by_id.values().cloned().collect()
    }

    pub fn validate_payload(&self, payload: &ApplicationDaPayload) -> Result<(), DaError> {
        let registration = self.get(&payload.profile_id)?;
        if !registration.is_active() {
            return Err(DaError::InvalidManifest(format!(
                "application profile id {} is not active",
                registration.profile_id
            )));
        }
        payload.validate(&registration.profile)
    }

    pub fn validate_historical_payload(
        &self,
        payload: &ApplicationDaPayload,
    ) -> Result<(), DaError> {
        let registration = self.get(&payload.profile_id)?;
        payload.validate(&registration.profile)
    }

    fn insert_registration(
        &mut self,
        registration: DaApplicationProfileRegistration,
    ) -> Result<(), DaError> {
        registration.validate()?;
        let profile_id = registration.profile_id.clone();
        let version_key = (
            registration.profile.application_id.clone(),
            registration.profile.profile_version,
        );
        if let Some(existing) = self.profiles_by_id.get(&profile_id) {
            if existing != &registration {
                return Err(DaError::InvalidManifest(format!(
                    "conflicting application profile registration for id {profile_id}"
                )));
            }
            return Ok(());
        }
        if let Some(existing_profile_id) = self.profile_ids_by_application_version.get(&version_key)
        {
            if existing_profile_id != &profile_id {
                return Err(DaError::InvalidManifest(format!(
                    "application profile {} version {} is already registered",
                    version_key.0 .0, version_key.1
                )));
            }
        }
        self.profile_ids_by_application_version
            .insert(version_key, profile_id.clone());
        self.profiles_by_id.insert(profile_id, registration);
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaRecordEnvelope {
    pub schema: String,
    pub schema_version: u32,
    pub content_type: String,
    pub encoding: DaRecordEncoding,
    pub bytes: Vec<u8>,
    pub content_hash: String,
    pub signer: Option<String>,
    pub signature: Option<String>,
}

impl DaRecordEnvelope {
    pub fn new(
        schema: impl Into<String>,
        schema_version: u32,
        content_type: impl Into<String>,
        encoding: DaRecordEncoding,
        bytes: Vec<u8>,
        signer: Option<String>,
        signature: Option<String>,
    ) -> Result<Self, DaError> {
        let envelope = Self {
            schema: schema.into(),
            schema_version,
            content_type: content_type.into(),
            encoding,
            content_hash: hash_bytes(&bytes),
            bytes,
            signer,
            signature,
        };
        envelope.validate_structure()?;
        Ok(envelope)
    }

    pub fn validate_structure(&self) -> Result<(), DaError> {
        validate_schema_id("record envelope schema", &self.schema)?;
        if self.schema_version == 0 {
            return Err(DaError::InvalidPayload(format!(
                "record envelope schema {} has version 0",
                self.schema
            )));
        }
        validate_label(
            "record envelope content_type",
            &self.content_type,
            DA_RECORD_CONTENT_TYPE_MAX_BYTES,
        )?;
        if self.bytes.is_empty() {
            return Err(DaError::InvalidPayload(format!(
                "record envelope {} carries no bytes",
                self.schema
            )));
        }
        if self.content_hash != hash_bytes(&self.bytes) {
            return Err(DaError::PayloadHashMismatch {
                expected: hash_bytes(&self.bytes),
                actual: self.content_hash.clone(),
            });
        }
        validate_optional_label("record envelope signer", self.signer.as_deref(), 256)?;
        validate_optional_label("record envelope signature", self.signature.as_deref(), 1024)?;
        Ok(())
    }

    pub fn validate_against_policy(&self, policy: &DaRecordPolicy) -> Result<(), DaError> {
        self.validate_structure()?;
        policy.validate()?;
        if self.schema != policy.schema || self.schema_version != policy.schema_version {
            return Err(DaError::InvalidPayload(format!(
                "record envelope {}@{} does not match policy {}@{}",
                self.schema, self.schema_version, policy.schema, policy.schema_version
            )));
        }
        if !policy
            .allowed_encodings
            .iter()
            .any(|encoding| encoding == &self.encoding)
        {
            return Err(DaError::InvalidPayload(format!(
                "record envelope {} uses unsupported encoding {:?}",
                self.schema, self.encoding
            )));
        }
        if self.bytes.len() as u64 > policy.max_record_bytes {
            return Err(DaError::InvalidPayload(format!(
                "record envelope {} exceeds max_record_bytes",
                self.schema
            )));
        }
        if policy.require_content_hash && self.content_hash.is_empty() {
            return Err(DaError::InvalidPayload(format!(
                "record envelope {} is missing content_hash",
                self.schema
            )));
        }
        if policy.require_signer
            && (self.signer.as_deref().unwrap_or_default().is_empty()
                || self.signature.as_deref().unwrap_or_default().is_empty())
        {
            return Err(DaError::InvalidPayload(format!(
                "record envelope {} requires signer and signature",
                self.schema
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaApplicationRoot {
    pub name: String,
    pub hash: String,
}

impl DaApplicationRoot {
    pub fn new(name: impl Into<String>, hash: impl Into<String>) -> Result<Self, DaError> {
        let root = Self {
            name: name.into(),
            hash: hash.into(),
        };
        root.validate_structure()?;
        Ok(root)
    }

    pub fn validate_structure(&self) -> Result<(), DaError> {
        validate_root_name(&self.name)?;
        validate_sha256_hex("application root hash", &self.hash)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplicationDaNamespaceSection {
    pub namespace: DaNamespace,
    pub records: Vec<DaRecordEnvelope>,
}

impl ApplicationDaNamespaceSection {
    pub fn new(namespace: DaNamespace, records: Vec<DaRecordEnvelope>) -> Result<Self, DaError> {
        let section = Self { namespace, records };
        section.validate_structure()?;
        Ok(section)
    }

    pub fn validate_structure(&self) -> Result<(), DaError> {
        self.namespace.validate()?;
        if self.records.is_empty() {
            return Err(DaError::InvalidPayload(format!(
                "application namespace {} has no records",
                self.namespace.0
            )));
        }
        for record in &self.records {
            record.validate_structure()?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplicationDaPayload {
    pub schema: String,
    pub schema_version: u32,
    pub application_id: DaApplicationId,
    pub profile_id: String,
    pub coordinate: DaApplicationCoordinate,
    pub payload_kind: DaPayloadKind,
    pub previous_payload_hash: Option<String>,
    pub application_roots: Vec<DaApplicationRoot>,
    pub namespaces: Vec<ApplicationDaNamespaceSection>,
}

impl ApplicationDaPayload {
    pub fn new(
        profile: &DaApplicationProfile,
        coordinate: DaApplicationCoordinate,
        payload_kind: DaPayloadKind,
        previous_payload_hash: Option<String>,
        application_roots: Vec<DaApplicationRoot>,
        namespaces: Vec<ApplicationDaNamespaceSection>,
    ) -> Result<Self, DaError> {
        let payload = Self {
            schema: APPLICATION_DA_PAYLOAD_SCHEMA.into(),
            schema_version: 1,
            application_id: profile.application_id.clone(),
            profile_id: profile.profile_id()?,
            coordinate,
            payload_kind,
            previous_payload_hash,
            application_roots,
            namespaces,
        }
        .canonicalized();
        payload.validate(profile)?;
        Ok(payload)
    }

    pub fn canonicalized(&self) -> Self {
        let mut namespaces: BTreeMap<DaNamespace, Vec<DaRecordEnvelope>> = BTreeMap::new();
        for section in &self.namespaces {
            namespaces
                .entry(section.namespace.clone())
                .or_default()
                .extend(section.records.clone());
        }
        let mut application_roots = self.application_roots.clone();
        application_roots.sort_by(|left, right| left.name.cmp(&right.name));
        Self {
            schema: self.schema.clone(),
            schema_version: self.schema_version,
            application_id: self.application_id.clone(),
            profile_id: self.profile_id.clone(),
            coordinate: self.coordinate.clone(),
            payload_kind: self.payload_kind.clone(),
            previous_payload_hash: self.previous_payload_hash.clone(),
            application_roots,
            namespaces: namespaces
                .into_iter()
                .map(|(namespace, records)| ApplicationDaNamespaceSection { namespace, records })
                .collect(),
        }
    }

    pub fn validate_structure(&self) -> Result<(), DaError> {
        if self.schema != APPLICATION_DA_PAYLOAD_SCHEMA {
            return Err(DaError::InvalidPayload(format!(
                "unexpected application DA payload schema {}",
                self.schema
            )));
        }
        if self.schema_version != 1 {
            return Err(DaError::InvalidPayload(format!(
                "unexpected application DA payload version {}",
                self.schema_version
            )));
        }
        self.application_id.validate()?;
        validate_sha256_hex("application profile_id", &self.profile_id)?;
        self.payload_kind.validate()?;
        validate_optional_sha256_hex(
            "previous_payload_hash",
            self.previous_payload_hash.as_deref(),
        )?;
        validate_sorted_unique_application_roots(&self.application_roots)?;
        if self.namespaces.is_empty() {
            return Err(DaError::InvalidPayload(
                "application DA payload must contain at least one namespace".into(),
            ));
        }
        validate_sorted_unique_namespaces(
            &self
                .namespaces
                .iter()
                .map(|section| section.namespace.clone())
                .collect::<Vec<_>>(),
        )?;
        for section in &self.namespaces {
            section.validate_structure()?;
        }
        if &self.canonicalized() != self {
            return Err(DaError::PayloadNotCanonical);
        }
        Ok(())
    }

    pub fn validate(&self, profile: &DaApplicationProfile) -> Result<(), DaError> {
        profile.validate()?;
        self.validate_structure()?;
        if self.application_id != profile.application_id {
            return Err(DaError::InvalidPayload(format!(
                "payload application_id {} does not match profile application_id {}",
                self.application_id.0, profile.application_id.0
            )));
        }
        if self.profile_id != profile.profile_id()? {
            return Err(DaError::InvalidPayload(
                "payload profile_id does not match profile hash".into(),
            ));
        }
        if self.coordinate.application_id != self.application_id {
            return Err(DaError::InvalidPayload(
                "payload coordinate application_id does not match payload application_id".into(),
            ));
        }
        self.coordinate.validate(&profile.coordinate_policy)?;

        let payload_bytes = canonical_bytes(&self.canonicalized())?;
        if payload_bytes.len() as u64 > profile.max_payload_bytes {
            return Err(DaError::InvalidPayload(
                "application DA payload exceeds profile max_payload_bytes".into(),
            ));
        }

        let namespace_policies = profile
            .namespace_policies
            .iter()
            .map(|policy| (policy.namespace.clone(), policy))
            .collect::<BTreeMap<_, _>>();
        let record_policies = profile
            .record_policies
            .iter()
            .map(|policy| (policy.schema.clone(), policy))
            .collect::<BTreeMap<_, _>>();
        let sections = self
            .namespaces
            .iter()
            .map(|section| (section.namespace.clone(), section))
            .collect::<BTreeMap<_, _>>();

        let total_record_count: u64 = self
            .namespaces
            .iter()
            .map(|section| section.records.len() as u64)
            .sum();
        if total_record_count > profile.max_records_per_payload as u64 {
            return Err(DaError::InvalidPayload(
                "application DA payload exceeds profile max_records_per_payload".into(),
            ));
        }

        for policy in &profile.namespace_policies {
            match policy.requirement {
                DaNamespaceRequirement::Required => {
                    let section = sections.get(&policy.namespace).ok_or_else(|| {
                        DaError::InvalidPayload(format!(
                            "application DA payload missing required namespace {}",
                            policy.namespace.0
                        ))
                    })?;
                    validate_application_namespace_record_count(policy, section.records.len())?;
                }
                DaNamespaceRequirement::Optional => {
                    if let Some(section) = sections.get(&policy.namespace) {
                        validate_application_namespace_record_count(policy, section.records.len())?;
                    }
                }
                DaNamespaceRequirement::Forbidden => {
                    if sections.contains_key(&policy.namespace) {
                        return Err(DaError::InvalidPayload(format!(
                            "application DA payload includes forbidden namespace {}",
                            policy.namespace.0
                        )));
                    }
                }
            }
        }

        for section in &self.namespaces {
            let namespace_policy = namespace_policies.get(&section.namespace).ok_or_else(|| {
                DaError::InvalidPayload(format!(
                    "application DA payload uses unknown namespace {}",
                    section.namespace.0
                ))
            })?;
            if namespace_policy.requirement == DaNamespaceRequirement::Forbidden {
                return Err(DaError::InvalidPayload(format!(
                    "application DA payload includes forbidden namespace {}",
                    section.namespace.0
                )));
            }
            validate_application_namespace_record_count(namespace_policy, section.records.len())?;
            for record in &section.records {
                if !namespace_policy
                    .allowed_record_schemas
                    .iter()
                    .any(|schema| schema == &record.schema)
                {
                    return Err(DaError::InvalidPayload(format!(
                        "namespace {} does not allow record schema {}",
                        section.namespace.0, record.schema
                    )));
                }
                if profile.validation_mode == DaApplicationValidationMode::OpaqueBytes {
                    record.validate_structure()?;
                    continue;
                }
                let record_policy = record_policies.get(&record.schema).ok_or_else(|| {
                    DaError::InvalidPayload(format!(
                        "application DA payload uses unknown record schema {}",
                        record.schema
                    ))
                })?;
                record.validate_against_policy(record_policy)?;
                if !record_policy
                    .allowed_namespaces
                    .iter()
                    .any(|namespace| namespace == &section.namespace)
                {
                    return Err(DaError::InvalidPayload(format!(
                        "record schema {} is not allowed in namespace {}",
                        record.schema, section.namespace.0
                    )));
                }
            }
        }

        validate_application_roots_against_profile(self, profile)?;
        Ok(())
    }

    pub fn hash(&self) -> Result<String, DaError> {
        let canonical = self.canonicalized();
        canonical.validate_structure()?;
        hash_canonical(&canonical)
    }

    pub fn namespace_root(&self) -> Result<String, DaError> {
        let canonical = self.canonicalized();
        canonical.validate_structure()?;
        namespace_root_for_ranges(&application_namespace_ranges(&canonical))
    }
}

pub fn application_payload_hash(payload: &ApplicationDaPayload) -> Result<String, DaError> {
    payload.hash()
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaApplicationValidationReport {
    pub schema: String,
    pub schema_version: u32,
    pub validator_id: String,
    pub application_id: DaApplicationId,
    pub profile_id: String,
    pub payload_hash: String,
    pub manifest_hash: Option<String>,
    pub payload_kind: DaPayloadKind,
    pub sequence: u64,
    pub namespace_count: u32,
    pub record_count: u32,
    pub application_root: Option<String>,
    pub accepted: bool,
}

impl DaApplicationValidationReport {
    pub fn report_hash(&self) -> Result<String, DaError> {
        self.validate()?;
        hash_canonical(self)
    }

    pub fn validate(&self) -> Result<(), DaError> {
        if self.schema != DA_APPLICATION_VALIDATION_REPORT_SCHEMA {
            return Err(DaError::InvalidManifest(format!(
                "unexpected application validation report schema {}",
                self.schema
            )));
        }
        if self.schema_version != 1 {
            return Err(DaError::InvalidManifest(format!(
                "unexpected application validation report version {}",
                self.schema_version
            )));
        }
        validate_label("application validator_id", &self.validator_id, 128)?;
        self.application_id.validate()?;
        validate_sha256_hex("application validation report profile_id", &self.profile_id)?;
        validate_sha256_hex(
            "application validation report payload_hash",
            &self.payload_hash,
        )?;
        validate_optional_sha256_hex(
            "application validation report manifest_hash",
            self.manifest_hash.as_deref(),
        )?;
        self.payload_kind.validate()?;
        validate_optional_sha256_hex(
            "application validation report application_root",
            self.application_root.as_deref(),
        )?;
        if !self.accepted {
            return Err(DaError::InvalidManifest(
                "successful application validation reports must be accepted".into(),
            ));
        }
        Ok(())
    }
}

pub trait DaApplicationValidator {
    fn profile_id(&self) -> &str;

    fn validate_payload(
        &self,
        profile: &DaApplicationProfile,
        payload: &ApplicationDaPayload,
        manifest: Option<&ApplicationDaManifest>,
    ) -> Result<DaApplicationValidationReport, DaError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpaqueApplicationValidator {
    profile_id: String,
}

impl OpaqueApplicationValidator {
    pub fn new(profile: &DaApplicationProfile) -> Result<Self, DaError> {
        Ok(Self {
            profile_id: profile.profile_id()?,
        })
    }
}

impl DaApplicationValidator for OpaqueApplicationValidator {
    fn profile_id(&self) -> &str {
        &self.profile_id
    }

    fn validate_payload(
        &self,
        profile: &DaApplicationProfile,
        payload: &ApplicationDaPayload,
        manifest: Option<&ApplicationDaManifest>,
    ) -> Result<DaApplicationValidationReport, DaError> {
        validate_application_payload_with_optional_manifest(
            self.profile_id(),
            profile,
            payload,
            manifest,
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SchemaApplicationValidator {
    profile_id: String,
}

impl SchemaApplicationValidator {
    pub fn new(profile: &DaApplicationProfile) -> Result<Self, DaError> {
        Ok(Self {
            profile_id: profile.profile_id()?,
        })
    }
}

impl DaApplicationValidator for SchemaApplicationValidator {
    fn profile_id(&self) -> &str {
        &self.profile_id
    }

    fn validate_payload(
        &self,
        profile: &DaApplicationProfile,
        payload: &ApplicationDaPayload,
        manifest: Option<&ApplicationDaManifest>,
    ) -> Result<DaApplicationValidationReport, DaError> {
        let report = validate_application_payload_with_optional_manifest(
            self.profile_id(),
            profile,
            payload,
            manifest,
        )?;
        validate_application_schema_records(payload)?;
        Ok(report)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SocialDemoDaValidator {
    profile_id: String,
}

impl SocialDemoDaValidator {
    pub fn new() -> Result<Self, DaError> {
        let profile = DaApplicationProfile::social_demo_v1();
        Ok(Self {
            profile_id: profile.profile_id()?,
        })
    }
}

impl DaApplicationValidator for SocialDemoDaValidator {
    fn profile_id(&self) -> &str {
        &self.profile_id
    }

    fn validate_payload(
        &self,
        profile: &DaApplicationProfile,
        payload: &ApplicationDaPayload,
        manifest: Option<&ApplicationDaManifest>,
    ) -> Result<DaApplicationValidationReport, DaError> {
        if profile.application_id != application_id_unchecked("social.demo") {
            return Err(DaError::InvalidPayload(
                "social-demo validator requires the social.demo profile".into(),
            ));
        }
        if !matches!(
            payload.payload_kind,
            DaPayloadKind::Batch | DaPayloadKind::MediaManifest | DaPayloadKind::ModerationLog
        ) {
            return Err(DaError::InvalidPayload(
                "social-demo payload kind is not supported".into(),
            ));
        }
        if payload.coordinate.sequence == 0 {
            return Err(DaError::InvalidPayload(
                "social-demo sequence must be positive".into(),
            ));
        }
        if payload.coordinate.sequence > 1 && payload.previous_payload_hash.is_none() {
            return Err(DaError::InvalidPayload(
                "social-demo non-genesis payloads must reference previous_payload_hash".into(),
            ));
        }

        let report = validate_application_payload_with_optional_manifest(
            self.profile_id(),
            profile,
            payload,
            manifest,
        )?;
        validate_application_schema_records(payload)?;
        validate_social_demo_signatures(profile, payload)?;
        validate_social_demo_event_log_root(payload)?;
        Ok(report)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DettaDefiDaValidator {
    profile_id: String,
}

impl DettaDefiDaValidator {
    pub fn new() -> Result<Self, DaError> {
        let profile = DaApplicationProfile::detta_defi_v1();
        Ok(Self {
            profile_id: profile.profile_id()?,
        })
    }

    pub fn validate_block_payload(
        &self,
        payload: &DaPayload,
        manifest: Option<&DaManifest>,
    ) -> Result<DaApplicationValidationReport, DaError> {
        let profile = DaApplicationProfile::detta_defi_v1();
        validate_production_block_payload(payload, &profile.da_profile)?;
        if let Some(manifest) = manifest {
            verify_manifest_commits_payload(manifest, payload)?;
        }
        let canonical = payload.canonicalized();
        let payload_hash = canonical.hash()?;
        Ok(DaApplicationValidationReport {
            schema: DA_APPLICATION_VALIDATION_REPORT_SCHEMA.into(),
            schema_version: 1,
            validator_id: self.profile_id.clone(),
            application_id: profile.application_id,
            profile_id: self.profile_id.clone(),
            payload_hash,
            manifest_hash: manifest.map(DaManifest::manifest_hash).transpose()?,
            payload_kind: DaPayloadKind::Block,
            sequence: canonical.height,
            namespace_count: canonical.namespaces.len() as u32,
            record_count: canonical
                .namespaces
                .iter()
                .map(|section| section.records.len() as u32)
                .sum(),
            application_root: None,
            accepted: true,
        })
    }
}

impl DaApplicationValidator for DettaDefiDaValidator {
    fn profile_id(&self) -> &str {
        &self.profile_id
    }

    fn validate_payload(
        &self,
        profile: &DaApplicationProfile,
        payload: &ApplicationDaPayload,
        manifest: Option<&ApplicationDaManifest>,
    ) -> Result<DaApplicationValidationReport, DaError> {
        if profile.application_id != application_id_unchecked("detta.defi") {
            return Err(DaError::InvalidPayload(
                "DeTTa DeFi validator requires the detta.defi profile".into(),
            ));
        }
        if payload.payload_kind != DaPayloadKind::Block {
            return Err(DaError::InvalidPayload(
                "DeTTa DeFi application payloads must use block payload kind".into(),
            ));
        }
        validate_application_payload_with_optional_manifest(
            self.profile_id(),
            profile,
            payload,
            manifest,
        )
    }
}

pub fn social_demo_event_log_root(payload: &ApplicationDaPayload) -> Result<String, DaError> {
    let canonical = payload.canonicalized();
    application_event_log_root_for_sections(&canonical.namespaces)
}

fn namespace_policy(
    namespace: &str,
    requirement: DaNamespaceRequirement,
    allowed_record_schemas: Vec<&str>,
    min_records: u32,
    max_records: u32,
    retention_class: DaApplicationRetentionClass,
) -> DaNamespacePolicy {
    DaNamespacePolicy {
        namespace: namespace_unchecked(namespace),
        requirement,
        allowed_record_schemas: allowed_record_schemas
            .into_iter()
            .map(String::from)
            .collect(),
        min_records,
        max_records,
        retention_class,
    }
}

fn record_policy(
    schema: &str,
    allowed_namespaces: Vec<&str>,
    allowed_encodings: Vec<DaRecordEncoding>,
    max_record_bytes: u64,
    require_signer: bool,
) -> DaRecordPolicy {
    DaRecordPolicy {
        schema: schema.into(),
        schema_version: 1,
        allowed_namespaces: allowed_namespaces
            .into_iter()
            .map(namespace_unchecked)
            .collect(),
        allowed_encodings,
        max_record_bytes,
        require_content_hash: true,
        require_signer,
    }
}

fn root_policy(name: &str, required: bool) -> DaApplicationRootPolicy {
    DaApplicationRootPolicy {
        name: name.into(),
        required,
    }
}

pub fn validate_production_block_payload(
    payload: &DaPayload,
    profile: &DaProductionProfile,
) -> Result<(), DaError> {
    profile.validate()?;
    payload.validate()?;
    let canonical = payload.canonicalized();
    if &canonical != payload {
        return Err(DaError::PayloadNotCanonical);
    }

    let mut block_header: Option<&BlockHeader> = None;
    let mut tx_count = 0_usize;
    let mut receipt_count = 0_usize;
    validate_sorted_unique_namespaces(
        &payload
            .namespaces
            .iter()
            .map(|section| section.namespace.clone())
            .collect::<Vec<_>>(),
    )?;

    for section in &payload.namespaces {
        match section.namespace.0.as_str() {
            "detta.block" => {
                for record in &section.records {
                    let DaRecord::BlockHeader(header) = record else {
                        return Err(DaError::InvalidPayload(
                            "detta.block contains a non-header record".into(),
                        ));
                    };
                    if block_header.is_some() {
                        return Err(DaError::InvalidPayload(
                            "production DA payload contains multiple block headers".into(),
                        ));
                    }
                    block_header = Some(header.as_ref());
                }
            }
            "detta.tx" => {
                for record in &section.records {
                    if !matches!(record, DaRecord::SignedTransaction(_)) {
                        return Err(DaError::InvalidPayload(
                            "detta.tx contains a non-transaction record".into(),
                        ));
                    }
                    tx_count += 1;
                }
            }
            "detta.receipt" => {
                for record in &section.records {
                    if !matches!(record, DaRecord::Receipt(_)) {
                        return Err(DaError::InvalidPayload(
                            "detta.receipt contains a non-receipt record".into(),
                        ));
                    }
                    receipt_count += 1;
                }
            }
            "detta.aspect" => validate_section_records(section, |record| {
                matches!(record, DaRecord::AspectArtifact(_))
            })?,
            "detta.governance" => validate_section_records(section, |record| {
                matches!(record, DaRecord::GovernancePayload { .. })
            })?,
            "detta.bridge" => validate_section_records(section, |record| {
                matches!(record, DaRecord::BridgeProof { .. })
            })?,
            "detta.oracle" => validate_section_records(section, |record| {
                matches!(record, DaRecord::OracleEvidence { .. })
            })?,
            "detta.event" => {
                validate_section_records(section, |record| matches!(record, DaRecord::Event(_)))?
            }
            other => {
                return Err(DaError::InvalidPayload(format!(
                    "unsupported production block DA namespace {other}"
                )));
            }
        }
    }

    let header = block_header.ok_or_else(|| {
        DaError::InvalidPayload("production DA payload must contain a block header".into())
    })?;
    if payload.chain_id != header.chain_id
        || payload.height != header.height
        || payload.previous_block_hash != header.previous_block_hash
    {
        return Err(DaError::InvalidPayload(
            "production DA payload metadata does not match block header".into(),
        ));
    }
    if tx_count == 0 && receipt_count != 0 {
        return Err(DaError::InvalidPayload(
            "production DA payload has receipts without transactions".into(),
        ));
    }
    if tx_count != 0 && receipt_count != tx_count {
        return Err(DaError::InvalidPayload(format!(
            "production DA payload transaction count {tx_count} does not match receipt count {receipt_count}"
        )));
    }
    Ok(())
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
pub struct ApplicationDaManifest {
    pub schema: String,
    pub schema_version: u32,
    pub application_id: DaApplicationId,
    pub profile_id: String,
    pub coordinate: DaApplicationCoordinate,
    pub payload_kind: DaPayloadKind,
    pub payload_hash: String,
    pub payload_bytes: u64,
    pub namespace_root: String,
    pub application_root: Option<String>,
    pub share_root: String,
    pub erasure_scheme: ErasureScheme,
    pub original_share_count: u32,
    pub encoded_share_count: u32,
    pub reconstruction_threshold: u32,
    pub share_size_bytes: u32,
    pub share_hashes: Vec<String>,
    pub namespace_ranges: Vec<DaNamespaceRange>,
}

impl ApplicationDaManifest {
    pub fn manifest_hash(&self) -> Result<String, DaError> {
        hash_canonical(self)
    }

    pub fn validate_structure(&self) -> Result<(), DaError> {
        if self.schema != APPLICATION_DA_MANIFEST_SCHEMA {
            return Err(DaError::InvalidManifest(format!(
                "unexpected application DA manifest schema {}",
                self.schema
            )));
        }
        if self.schema_version != 1 {
            return Err(DaError::InvalidManifest(format!(
                "unexpected application DA manifest version {}",
                self.schema_version
            )));
        }
        self.application_id.validate()?;
        validate_sha256_hex("application manifest profile_id", &self.profile_id)?;
        self.coordinate.application_id.validate()?;
        if self.coordinate.application_id != self.application_id {
            return Err(DaError::InvalidManifest(
                "application manifest coordinate application_id does not match manifest application_id"
                    .into(),
            ));
        }
        self.payload_kind.validate()?;
        validate_sha256_hex("application manifest payload_hash", &self.payload_hash)?;
        validate_sha256_hex("application manifest namespace_root", &self.namespace_root)?;
        validate_optional_sha256_hex(
            "application manifest application_root",
            self.application_root.as_deref(),
        )?;
        validate_sha256_hex("application manifest share_root", &self.share_root)?;
        if self.payload_bytes == 0 {
            return Err(DaError::InvalidManifest(
                "application manifest payload_bytes must be positive".into(),
            ));
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
        for share_hash in &self.share_hashes {
            validate_sha256_hex("application manifest share_hash", share_hash)?;
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

    pub fn validate(&self, profile: &DaApplicationProfile) -> Result<(), DaError> {
        profile.validate()?;
        self.validate_structure()?;
        if self.application_id != profile.application_id {
            return Err(DaError::InvalidManifest(format!(
                "manifest application_id {} does not match profile application_id {}",
                self.application_id.0, profile.application_id.0
            )));
        }
        if self.profile_id != profile.profile_id()? {
            return Err(DaError::InvalidManifest(
                "manifest profile_id does not match profile hash".into(),
            ));
        }
        self.coordinate.validate(&profile.coordinate_policy)?;
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ApplicationDaShareSet {
    pub manifest: ApplicationDaManifest,
    pub shares: Vec<DaShare>,
}

impl ApplicationDaShareSet {
    pub fn from_payload(
        payload: &ApplicationDaPayload,
        profile: &DaApplicationProfile,
        share_size_bytes: usize,
    ) -> Result<Self, DaError> {
        build_application_da_share_set(payload, profile, share_size_bytes)
    }

    pub fn from_payload_reed_solomon(
        payload: &ApplicationDaPayload,
        profile: &DaApplicationProfile,
        data_share_count: u32,
        parity_share_count: u32,
    ) -> Result<Self, DaError> {
        build_application_reed_solomon_share_set(
            payload,
            profile,
            data_share_count,
            parity_share_count,
        )
    }

    pub fn verify(&self, profile: &DaApplicationProfile) -> Result<(), DaError> {
        self.reconstruct_payload(profile).map(|_| ())
    }

    pub fn reconstruct_payload(
        &self,
        profile: &DaApplicationProfile,
    ) -> Result<ApplicationDaPayload, DaError> {
        self.manifest.validate(profile)?;
        let shares = validated_application_share_map(&self.manifest, &self.shares)?;

        if shares.len() < self.manifest.reconstruction_threshold as usize {
            return Err(DaError::InsufficientShares {
                required: self.manifest.reconstruction_threshold,
                actual: shares.len() as u32,
            });
        }

        match self.manifest.erasure_scheme {
            ErasureScheme::DeterministicChunks => {
                reconstruct_deterministic_application_payload(&self.manifest, &shares, profile)
            }
            ErasureScheme::ReedSolomonV1 => {
                reconstruct_reed_solomon_application_payload(&self.manifest, &shares, profile)
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplicationDaNamespaceProof {
    pub manifest_hash: String,
    pub application_id: DaApplicationId,
    pub profile_id: String,
    pub coordinate: DaApplicationCoordinate,
    pub payload_hash: String,
    pub namespace_root: String,
    pub namespace: DaNamespace,
    pub range: Option<DaNamespaceRange>,
    pub namespace_ranges: Vec<DaNamespaceRange>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplicationDaCodingFraudProof {
    pub schema: String,
    pub schema_version: u32,
    pub application_id: DaApplicationId,
    pub profile_id: String,
    pub coordinate: DaApplicationCoordinate,
    pub payload_kind: DaPayloadKind,
    pub manifest_hash: String,
    pub share_root: String,
    pub reporter_id: String,
    pub data_shares: Vec<DaShare>,
    pub fault: DaCodingFault,
}

impl ApplicationDaCodingFraudProof {
    pub fn from_committed_data_shares(
        manifest: &ApplicationDaManifest,
        profile: &DaApplicationProfile,
        data_shares: &[DaShare],
        reporter_id: impl Into<String>,
    ) -> Result<Self, DaError> {
        let fault = detect_application_da_coding_fault(manifest, profile, data_shares)?
            .ok_or_else(|| {
                DaError::InvalidCodingFraudProof(
                    "committed data shares form a valid application DA encoding".into(),
                )
            })?;
        let proof = Self {
            schema: APPLICATION_DA_CODING_FRAUD_PROOF_SCHEMA.into(),
            schema_version: 1,
            application_id: manifest.application_id.clone(),
            profile_id: manifest.profile_id.clone(),
            coordinate: manifest.coordinate.clone(),
            payload_kind: manifest.payload_kind.clone(),
            manifest_hash: manifest.manifest_hash()?,
            share_root: manifest.share_root.clone(),
            reporter_id: reporter_id.into(),
            data_shares: data_shares.to_vec(),
            fault,
        };
        proof.validate(manifest, profile)?;
        Ok(proof)
    }

    pub fn proof_hash(&self) -> Result<String, DaError> {
        self.validate_structure()?;
        hash_canonical(self)
    }

    pub fn validate_structure(&self) -> Result<(), DaError> {
        if self.schema != APPLICATION_DA_CODING_FRAUD_PROOF_SCHEMA {
            return Err(DaError::InvalidCodingFraudProof(format!(
                "unexpected application coding fraud proof schema {}",
                self.schema
            )));
        }
        if self.schema_version != 1 {
            return Err(DaError::InvalidCodingFraudProof(format!(
                "unexpected application coding fraud proof version {}",
                self.schema_version
            )));
        }
        self.application_id.validate()?;
        validate_sha256_hex(
            "application coding fraud proof profile_id",
            &self.profile_id,
        )?;
        self.coordinate.application_id.validate()?;
        self.payload_kind.validate()?;
        validate_sha256_hex(
            "application coding fraud proof manifest_hash",
            &self.manifest_hash,
        )?;
        validate_sha256_hex(
            "application coding fraud proof share_root",
            &self.share_root,
        )?;
        validate_label(
            "application coding fraud proof reporter_id",
            &self.reporter_id,
            256,
        )?;
        Ok(())
    }

    pub fn validate(
        &self,
        manifest: &ApplicationDaManifest,
        profile: &DaApplicationProfile,
    ) -> Result<(), DaError> {
        self.validate_structure()?;
        manifest.validate(profile)?;
        if self.application_id != manifest.application_id
            || self.profile_id != manifest.profile_id
            || self.coordinate != manifest.coordinate
            || self.payload_kind != manifest.payload_kind
            || self.manifest_hash != manifest.manifest_hash()?
            || self.share_root != manifest.share_root
        {
            return Err(DaError::InvalidCodingFraudProof(
                "application coding fraud proof does not match manifest".into(),
            ));
        }
        let detected = detect_application_da_coding_fault(manifest, profile, &self.data_shares)?
            .ok_or_else(|| {
                DaError::InvalidCodingFraudProof(
                    "committed data shares form a valid application DA encoding".into(),
                )
            })?;
        if detected != self.fault {
            return Err(DaError::InvalidCodingFraudProof(
                "declared application coding fault does not match recomputed fault".into(),
            ));
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

    pub fn from_payload_reed_solomon_with_target_share_size(
        payload: &DaPayload,
        block_hash: impl Into<String>,
        target_share_size_bytes: usize,
    ) -> Result<Self, DaError> {
        build_reed_solomon_share_set_with_target_share_size(
            payload,
            block_hash,
            target_share_size_bytes,
        )
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

fn validated_application_share_map<'a>(
    manifest: &ApplicationDaManifest,
    shares: &'a [DaShare],
) -> Result<BTreeMap<u32, &'a DaShare>, DaError> {
    manifest.validate_structure()?;
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

fn reconstruct_deterministic_application_payload(
    manifest: &ApplicationDaManifest,
    shares: &BTreeMap<u32, &DaShare>,
    profile: &DaApplicationProfile,
) -> Result<ApplicationDaPayload, DaError> {
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

    decode_application_payload_bytes(manifest, &payload_bytes, profile)
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

fn reconstruct_reed_solomon_application_payload(
    manifest: &ApplicationDaManifest,
    shares: &BTreeMap<u32, &DaShare>,
    profile: &DaApplicationProfile,
) -> Result<ApplicationDaPayload, DaError> {
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

    reed_solomon_application_codec(manifest)?.reconstruct(&mut shard_options)?;

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

    decode_application_payload_bytes(manifest, &payload_bytes, profile)
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

fn decode_application_payload_bytes(
    manifest: &ApplicationDaManifest,
    payload_bytes: &[u8],
    profile: &DaApplicationProfile,
) -> Result<ApplicationDaPayload, DaError> {
    let payload_hash = hash_bytes(payload_bytes);
    if payload_hash != manifest.payload_hash {
        return Err(DaError::PayloadHashMismatch {
            expected: manifest.payload_hash.clone(),
            actual: payload_hash,
        });
    }
    if payload_bytes.len() as u64 != manifest.payload_bytes {
        return Err(DaError::TotalBytesMismatch {
            expected: manifest.payload_bytes,
            actual: payload_bytes.len() as u64,
        });
    }

    let payload: ApplicationDaPayload =
        serde_json::from_slice(payload_bytes).map_err(|_| DaError::DecodeFailed)?;
    payload.validate(profile)?;
    let canonical = payload.canonicalized();
    if canonical != payload {
        return Err(DaError::PayloadNotCanonical);
    }
    if canonical.application_id != manifest.application_id
        || canonical.profile_id != manifest.profile_id
        || canonical.coordinate != manifest.coordinate
        || canonical.payload_kind != manifest.payload_kind
    {
        return Err(DaError::ManifestPayloadMismatch {
            expected: manifest.manifest_hash()?,
            actual: canonical.hash()?,
        });
    }
    let namespace_root = canonical.namespace_root()?;
    if namespace_root != manifest.namespace_root {
        return Err(DaError::NamespaceRootMismatch {
            expected: manifest.namespace_root.clone(),
            actual: namespace_root,
        });
    }
    let application_root = application_root_commitment(&canonical)?;
    if application_root != manifest.application_root {
        return Err(DaError::ManifestPayloadMismatch {
            expected: manifest.manifest_hash()?,
            actual: canonical.hash()?,
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

fn reed_solomon_application_codec(
    manifest: &ApplicationDaManifest,
) -> Result<ReedSolomon, DaError> {
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

pub fn build_application_da_share_set(
    payload: &ApplicationDaPayload,
    profile: &DaApplicationProfile,
    share_size_bytes: usize,
) -> Result<ApplicationDaShareSet, DaError> {
    if share_size_bytes == 0 {
        return Err(DaError::InvalidChunkSize);
    }

    let payload = payload.canonicalized();
    payload.validate(profile)?;
    let payload_bytes = canonical_bytes(&payload)?;
    let payload_hash = hash_bytes(&payload_bytes);
    let chunks: Vec<Vec<u8>> = payload_bytes
        .chunks(share_size_bytes)
        .map(|chunk| chunk.to_vec())
        .collect();
    let share_hashes: Vec<String> = chunks.iter().map(|chunk| hash_bytes(chunk)).collect();
    let encoded_share_count =
        u32::try_from(share_hashes.len()).map_err(|_| DaError::ShareCountOverflow)?;
    let manifest = ApplicationDaManifest {
        schema: APPLICATION_DA_MANIFEST_SCHEMA.into(),
        schema_version: 1,
        application_id: payload.application_id.clone(),
        profile_id: payload.profile_id.clone(),
        coordinate: payload.coordinate.clone(),
        payload_kind: payload.payload_kind.clone(),
        payload_hash,
        payload_bytes: payload_bytes.len() as u64,
        namespace_root: payload.namespace_root()?,
        application_root: application_root_commitment(&payload)?,
        share_root: share_root(&share_hashes)?,
        erasure_scheme: ErasureScheme::DeterministicChunks,
        original_share_count: encoded_share_count,
        encoded_share_count,
        reconstruction_threshold: encoded_share_count,
        share_size_bytes: u32::try_from(share_size_bytes).map_err(|_| DaError::InvalidChunkSize)?,
        share_hashes,
        namespace_ranges: application_namespace_ranges(&payload),
    };
    manifest.validate(profile)?;
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
    Ok(ApplicationDaShareSet { manifest, shares })
}

pub fn build_application_reed_solomon_share_set(
    payload: &ApplicationDaPayload,
    profile: &DaApplicationProfile,
    data_share_count: u32,
    parity_share_count: u32,
) -> Result<ApplicationDaShareSet, DaError> {
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
    payload.validate(profile)?;
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
    let manifest = ApplicationDaManifest {
        schema: APPLICATION_DA_MANIFEST_SCHEMA.into(),
        schema_version: 1,
        application_id: payload.application_id.clone(),
        profile_id: payload.profile_id.clone(),
        coordinate: payload.coordinate.clone(),
        payload_kind: payload.payload_kind.clone(),
        payload_hash,
        payload_bytes: u64::try_from(payload_bytes.len()).map_err(|_| DaError::InvalidChunkSize)?,
        namespace_root: payload.namespace_root()?,
        application_root: application_root_commitment(&payload)?,
        share_root: share_root(&share_hashes)?,
        erasure_scheme: ErasureScheme::ReedSolomonV1,
        original_share_count: data_share_count,
        encoded_share_count,
        reconstruction_threshold: data_share_count,
        share_size_bytes: u32::try_from(share_size_bytes).map_err(|_| DaError::InvalidChunkSize)?,
        share_hashes,
        namespace_ranges: application_namespace_ranges(&payload),
    };
    manifest.validate(profile)?;
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
    Ok(ApplicationDaShareSet { manifest, shares })
}

pub fn build_reed_solomon_share_set_with_target_share_size(
    payload: &DaPayload,
    block_hash: impl Into<String>,
    target_share_size_bytes: usize,
) -> Result<DaShareSet, DaError> {
    if target_share_size_bytes == 0 {
        return Err(DaError::InvalidChunkSize);
    }
    let payload = payload.canonicalized();
    payload.validate()?;
    let payload_bytes = canonical_bytes(&payload)?;
    let data_share_count = payload_bytes.len().div_ceil(target_share_size_bytes).max(1);
    let max_data_share_count = (REED_SOLOMON_MAX_SHARES / 2) as usize;
    if data_share_count > max_data_share_count {
        return Err(DaError::InvalidManifest(format!(
            "reed-solomon target share size {target_share_size_bytes} would require {data_share_count} data shares, max is {max_data_share_count}"
        )));
    }
    let data_share_count =
        u32::try_from(data_share_count).map_err(|_| DaError::ShareCountOverflow)?;
    build_reed_solomon_share_set(&payload, block_hash, data_share_count, data_share_count)
}

/// Verify that a manifest's share commitment is a faithful erasure encoding of
/// the given payload.
///
/// `DaManifest::validate` only proves a manifest is *internally* consistent
/// (its `share_root` matches its `share_hashes`); it cannot, on its own, prove
/// that those shares actually reconstruct the committed payload. Without this
/// binding a malicious proposer can commit a manifest whose `payload_hash` and
/// `namespace_root` match the real payload while its `share_hashes` are not a
/// valid codeword for it, so the data is certified-available yet unrecoverable.
///
/// This recomputes the canonical share set deterministically from the payload
/// using the parameters recorded in the manifest (erasure scheme, share counts,
/// share size, block hash) and requires the rebuilt manifest to be identical.
/// Because every honest builder derives those parameters deterministically from
/// the payload, an honest manifest always round-trips, while any tampering with
/// `share_root`, `share_hashes`, `share_size_bytes`, or the share counts is
/// rejected.
pub fn verify_manifest_commits_payload(
    manifest: &DaManifest,
    payload: &DaPayload,
) -> Result<(), DaError> {
    manifest.validate()?;
    let canonical = payload.canonicalized();
    canonical.validate()?;

    let rebuilt = match manifest.erasure_scheme {
        ErasureScheme::DeterministicChunks => build_da_share_set(
            &canonical,
            manifest.block_hash.clone(),
            usize::try_from(manifest.share_size_bytes).map_err(|_| DaError::InvalidChunkSize)?,
        )?,
        ErasureScheme::ReedSolomonV1 => {
            let parity_share_count = manifest
                .encoded_share_count
                .checked_sub(manifest.original_share_count)
                .ok_or(DaError::ShareCountOverflow)?;
            build_reed_solomon_share_set(
                &canonical,
                manifest.block_hash.clone(),
                manifest.original_share_count,
                parity_share_count,
            )?
        }
    };

    if &rebuilt.manifest != manifest {
        return Err(DaError::ManifestPayloadMismatch {
            expected: rebuilt.manifest.manifest_hash()?,
            actual: manifest.manifest_hash()?,
        });
    }
    Ok(())
}

pub fn verify_application_manifest_commits_payload(
    manifest: &ApplicationDaManifest,
    payload: &ApplicationDaPayload,
    profile: &DaApplicationProfile,
) -> Result<(), DaError> {
    manifest.validate(profile)?;
    let canonical = payload.canonicalized();
    canonical.validate(profile)?;

    let rebuilt = match manifest.erasure_scheme {
        ErasureScheme::DeterministicChunks => build_application_da_share_set(
            &canonical,
            profile,
            usize::try_from(manifest.share_size_bytes).map_err(|_| DaError::InvalidChunkSize)?,
        )?,
        ErasureScheme::ReedSolomonV1 => {
            let parity_share_count = manifest
                .encoded_share_count
                .checked_sub(manifest.original_share_count)
                .ok_or(DaError::ShareCountOverflow)?;
            build_application_reed_solomon_share_set(
                &canonical,
                profile,
                manifest.original_share_count,
                parity_share_count,
            )?
        }
    };

    if &rebuilt.manifest != manifest {
        return Err(DaError::ManifestPayloadMismatch {
            expected: rebuilt.manifest.manifest_hash()?,
            actual: manifest.manifest_hash()?,
        });
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DaCodingFault {
    /// Re-encoding the committed data shares yields a parity share whose hash
    /// differs from the manifest's committed parity share hash.
    ParityMismatch { share_index: u32 },
    /// The committed data shares decode to payload bytes whose hash differs from
    /// the manifest's committed `payload_hash`.
    PayloadHashMismatch { expected: String, actual: String },
}

/// Transferable, slashable evidence that a manifest's committed shares are not a
/// valid erasure encoding of its committed payload.
///
/// Unlike [`verify_manifest_commits_payload`], which requires the asserted-correct
/// payload, this is derived purely from the proposer's own committed data shares
/// (each bound to the manifest by hash). A node that fetched the
/// `original_share_count` data shares from the network — without ever being handed
/// a "claimed" payload — can produce this proof, and any third party holding only
/// the manifest can verify it before acting on it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaCodingFraudProof {
    pub schema: String,
    pub schema_version: u32,
    pub chain_id: ChainId,
    pub height: u64,
    pub block_hash: String,
    pub manifest_hash: String,
    pub share_root: String,
    pub reporter_id: String,
    pub data_shares: Vec<DaShare>,
    pub fault: DaCodingFault,
}

impl DaCodingFraudProof {
    pub fn from_committed_data_shares(
        manifest: &DaManifest,
        data_shares: &[DaShare],
        reporter_id: impl Into<String>,
    ) -> Result<Self, DaError> {
        let fault = detect_da_coding_fault(manifest, data_shares)?.ok_or_else(|| {
            DaError::InvalidCodingFraudProof(
                "committed data shares form a valid encoding of the committed payload".into(),
            )
        })?;
        let proof = Self {
            schema: DA_CODING_FRAUD_PROOF_SCHEMA.into(),
            schema_version: 1,
            chain_id: manifest.chain_id.clone(),
            height: manifest.height,
            block_hash: manifest.block_hash.clone(),
            manifest_hash: manifest.manifest_hash()?,
            share_root: manifest.share_root.clone(),
            reporter_id: reporter_id.into(),
            data_shares: data_shares.to_vec(),
            fault,
        };
        proof.validate(manifest)?;
        Ok(proof)
    }

    pub fn proof_hash(&self) -> Result<String, DaError> {
        hash_canonical(self)
    }

    pub fn validate(&self, manifest: &DaManifest) -> Result<(), DaError> {
        if self.schema != DA_CODING_FRAUD_PROOF_SCHEMA {
            return Err(DaError::InvalidCodingFraudProof(format!(
                "unexpected coding fraud proof schema {}",
                self.schema
            )));
        }
        if self.schema_version != 1 {
            return Err(DaError::InvalidCodingFraudProof(format!(
                "unexpected coding fraud proof schema version {}",
                self.schema_version
            )));
        }
        if self.reporter_id.is_empty() {
            return Err(DaError::InvalidCodingFraudProof(
                "reporter_id is empty".into(),
            ));
        }
        let manifest_hash = manifest.manifest_hash()?;
        if self.manifest_hash != manifest_hash
            || self.chain_id != manifest.chain_id
            || self.height != manifest.height
            || self.block_hash != manifest.block_hash
            || self.share_root != manifest.share_root
        {
            return Err(DaError::InvalidCodingFraudProof(
                "coding fraud proof does not match manifest".into(),
            ));
        }
        let detected = detect_da_coding_fault(manifest, &self.data_shares)?.ok_or_else(|| {
            DaError::InvalidCodingFraudProof(
                "committed data shares form a valid encoding of the committed payload".into(),
            )
        })?;
        if detected != self.fault {
            return Err(DaError::InvalidCodingFraudProof(
                "declared fault does not match recomputed fault".into(),
            ));
        }
        Ok(())
    }
}

/// Inspect the committed data shares of a manifest and report the coding fault
/// they expose, if any. Returns `Ok(None)` when the shares are a valid encoding
/// of the committed payload.
fn detect_da_coding_fault(
    manifest: &DaManifest,
    data_shares: &[DaShare],
) -> Result<Option<DaCodingFault>, DaError> {
    manifest.validate()?;

    let mut by_index: BTreeMap<u32, &DaShare> = BTreeMap::new();
    for share in data_shares {
        verify_share_against_manifest(manifest, share)?;
        if share.index >= manifest.original_share_count {
            return Err(DaError::InvalidCodingFraudProof(
                "coding fraud proof shares must be committed data shares".into(),
            ));
        }
        if by_index.insert(share.index, share).is_some() {
            return Err(DaError::DuplicateShare { index: share.index });
        }
    }
    for index in 0..manifest.original_share_count {
        if !by_index.contains_key(&index) {
            return Err(DaError::MissingShare { index });
        }
    }

    if manifest.erasure_scheme == ErasureScheme::ReedSolomonV1 {
        let share_size = manifest.share_size_bytes as usize;
        let mut shards: Vec<Vec<u8>> =
            vec![vec![0_u8; share_size]; manifest.encoded_share_count as usize];
        for index in 0..manifest.original_share_count {
            let share = by_index[&index];
            if share.bytes.len() != share_size {
                return Err(DaError::InvalidManifest(format!(
                    "reed-solomon data share {index} has {} bytes, expected {share_size}",
                    share.bytes.len()
                )));
            }
            shards[index as usize] = share.bytes.clone();
        }
        reed_solomon_codec(manifest)?.encode(&mut shards)?;
        for index in manifest.original_share_count..manifest.encoded_share_count {
            let recomputed = hash_bytes(&shards[index as usize]);
            if recomputed != manifest.share_hashes[index as usize] {
                return Ok(Some(DaCodingFault::ParityMismatch { share_index: index }));
            }
        }
    }

    let mut payload_bytes = Vec::new();
    for index in 0..manifest.original_share_count {
        payload_bytes.extend_from_slice(&by_index[&index].bytes);
    }
    let payload_len = usize::try_from(manifest.payload_bytes).map_err(|_| {
        DaError::InvalidManifest("payload byte count does not fit this platform".into())
    })?;
    if payload_len > payload_bytes.len() {
        return Err(DaError::InvalidManifest(
            "committed payload bytes exceed data share capacity".into(),
        ));
    }
    payload_bytes.truncate(payload_len);
    let actual = hash_bytes(&payload_bytes);
    if actual != manifest.payload_hash {
        return Ok(Some(DaCodingFault::PayloadHashMismatch {
            expected: manifest.payload_hash.clone(),
            actual,
        }));
    }

    Ok(None)
}

fn detect_application_da_coding_fault(
    manifest: &ApplicationDaManifest,
    profile: &DaApplicationProfile,
    data_shares: &[DaShare],
) -> Result<Option<DaCodingFault>, DaError> {
    manifest.validate(profile)?;

    let mut by_index: BTreeMap<u32, &DaShare> = BTreeMap::new();
    for share in data_shares {
        verify_application_share_against_manifest(manifest, share)?;
        if share.index >= manifest.original_share_count {
            return Err(DaError::InvalidCodingFraudProof(
                "application coding fraud proof shares must be committed data shares".into(),
            ));
        }
        if by_index.insert(share.index, share).is_some() {
            return Err(DaError::DuplicateShare { index: share.index });
        }
    }
    for index in 0..manifest.original_share_count {
        if !by_index.contains_key(&index) {
            return Err(DaError::MissingShare { index });
        }
    }

    if manifest.erasure_scheme == ErasureScheme::ReedSolomonV1 {
        let share_size = manifest.share_size_bytes as usize;
        let mut shards: Vec<Vec<u8>> =
            vec![vec![0_u8; share_size]; manifest.encoded_share_count as usize];
        for index in 0..manifest.original_share_count {
            let share = by_index[&index];
            if share.bytes.len() != share_size {
                return Err(DaError::InvalidManifest(format!(
                    "reed-solomon application data share {index} has {} bytes, expected {share_size}",
                    share.bytes.len()
                )));
            }
            shards[index as usize] = share.bytes.clone();
        }
        reed_solomon_application_codec(manifest)?.encode(&mut shards)?;
        for index in manifest.original_share_count..manifest.encoded_share_count {
            let recomputed = hash_bytes(&shards[index as usize]);
            if recomputed != manifest.share_hashes[index as usize] {
                return Ok(Some(DaCodingFault::ParityMismatch { share_index: index }));
            }
        }
    }

    let mut payload_bytes = Vec::new();
    for index in 0..manifest.original_share_count {
        payload_bytes.extend_from_slice(&by_index[&index].bytes);
    }
    let payload_len = usize::try_from(manifest.payload_bytes).map_err(|_| {
        DaError::InvalidManifest("payload byte count does not fit this platform".into())
    })?;
    if payload_len > payload_bytes.len() {
        return Err(DaError::InvalidManifest(
            "committed payload bytes exceed data share capacity".into(),
        ));
    }
    payload_bytes.truncate(payload_len);
    let actual = hash_bytes(&payload_bytes);
    if actual != manifest.payload_hash {
        return Ok(Some(DaCodingFault::PayloadHashMismatch {
            expected: manifest.payload_hash.clone(),
            actual,
        }));
    }

    Ok(None)
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

pub fn verify_application_share_against_manifest(
    manifest: &ApplicationDaManifest,
    share: &DaShare,
) -> Result<(), DaError> {
    manifest.validate_structure()?;
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

pub fn prove_application_namespace(
    manifest: &ApplicationDaManifest,
    profile: &DaApplicationProfile,
    namespace: &DaNamespace,
) -> Result<ApplicationDaNamespaceProof, DaError> {
    manifest.validate(profile)?;
    let range = manifest
        .namespace_ranges
        .iter()
        .find(|range| &range.namespace == namespace)
        .cloned();
    Ok(ApplicationDaNamespaceProof {
        manifest_hash: manifest.manifest_hash()?,
        application_id: manifest.application_id.clone(),
        profile_id: manifest.profile_id.clone(),
        coordinate: manifest.coordinate.clone(),
        payload_hash: manifest.payload_hash.clone(),
        namespace_root: manifest.namespace_root.clone(),
        namespace: namespace.clone(),
        range,
        namespace_ranges: manifest.namespace_ranges.clone(),
    })
}

pub fn verify_application_namespace_proof(
    manifest: &ApplicationDaManifest,
    profile: &DaApplicationProfile,
    proof: &ApplicationDaNamespaceProof,
) -> Result<Option<DaNamespaceRange>, DaError> {
    manifest.validate(profile)?;
    let manifest_hash = manifest.manifest_hash()?;
    if proof.manifest_hash != manifest_hash {
        return Err(DaError::ManifestHashMismatch {
            expected: manifest_hash,
            actual: proof.manifest_hash.clone(),
        });
    }
    if proof.application_id != manifest.application_id
        || proof.profile_id != manifest.profile_id
        || proof.coordinate != manifest.coordinate
        || proof.payload_hash != manifest.payload_hash
    {
        return Err(DaError::InvalidSampling(
            "application namespace proof does not match manifest metadata".into(),
        ));
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
            "application namespace proof range does not match committed ranges".into(),
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

fn application_namespace_ranges(payload: &ApplicationDaPayload) -> Vec<DaNamespaceRange> {
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

fn application_root_commitment(payload: &ApplicationDaPayload) -> Result<Option<String>, DaError> {
    if payload.application_roots.is_empty() {
        Ok(None)
    } else {
        hash_canonical(&payload.application_roots).map(Some)
    }
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

fn validate_sorted_unique_namespaces(namespaces: &[DaNamespace]) -> Result<(), DaError> {
    if !namespaces.windows(2).all(|window| window[0] < window[1]) {
        return Err(DaError::InvalidPayload(
            "DA namespaces must be sorted and unique".into(),
        ));
    }
    Ok(())
}

fn validate_application_id(value: &str) -> Result<(), DaError> {
    if is_bounded_segmented_identifier(value, DA_APPLICATION_ID_MAX_BYTES) {
        Ok(())
    } else {
        Err(DaError::InvalidNamespace(value.into()))
    }
}

fn validate_stream_id(value: &str, max_bytes: usize) -> Result<(), DaError> {
    validate_label("application stream_id", value, max_bytes)
}

fn validate_schema_id(field: &str, value: &str) -> Result<(), DaError> {
    if is_bounded_segmented_identifier(value, DA_RECORD_SCHEMA_ID_MAX_BYTES) {
        Ok(())
    } else {
        Err(DaError::InvalidManifest(format!(
            "{field} must be a lowercase dot/hyphen identifier"
        )))
    }
}

fn validate_root_name(value: &str) -> Result<(), DaError> {
    if value.len() <= DA_APPLICATION_ROOT_NAME_MAX_BYTES
        && is_bounded_segmented_identifier(value, DA_APPLICATION_ROOT_NAME_MAX_BYTES)
    {
        Ok(())
    } else {
        Err(DaError::InvalidManifest(format!(
            "application root binding {value} is invalid"
        )))
    }
}

fn validate_label(field: &str, value: &str, max_bytes: usize) -> Result<(), DaError> {
    if value.is_empty()
        || value.len() > max_bytes
        || value.trim() != value
        || value.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err(DaError::InvalidManifest(format!(
            "{field} must be nonempty, bounded, and free of control characters"
        )));
    }
    Ok(())
}

fn validate_optional_label(
    field: &str,
    value: Option<&str>,
    max_bytes: usize,
) -> Result<(), DaError> {
    if let Some(value) = value {
        validate_label(field, value, max_bytes)?;
    }
    Ok(())
}

fn validate_optional_hash(field: &str, value: Option<&str>) -> Result<(), DaError> {
    validate_optional_label(field, value, 256)
}

fn validate_sha256_hex(field: &str, value: &str) -> Result<(), DaError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DaError::InvalidManifest(format!(
            "{field} must be a 32-byte hex hash"
        )));
    }
    if value.bytes().any(|byte| byte.is_ascii_uppercase()) {
        return Err(DaError::InvalidManifest(format!(
            "{field} must use lowercase hex"
        )));
    }
    Ok(())
}

fn validate_optional_sha256_hex(field: &str, value: Option<&str>) -> Result<(), DaError> {
    if let Some(value) = value {
        validate_sha256_hex(field, value)?;
    }
    Ok(())
}

fn validate_sorted_unique_schema_ids(field: &str, values: &[String]) -> Result<(), DaError> {
    if values.is_empty() {
        return Err(DaError::InvalidManifest(format!(
            "{field} must be nonempty"
        )));
    }
    for value in values {
        validate_schema_id(field, value)?;
    }
    if !values.windows(2).all(|window| window[0] < window[1]) {
        return Err(DaError::InvalidManifest(format!(
            "{field} must be sorted and unique"
        )));
    }
    Ok(())
}

fn validate_sorted_unique_application_roots(roots: &[DaApplicationRoot]) -> Result<(), DaError> {
    for root in roots {
        root.validate_structure()?;
    }
    if !roots
        .windows(2)
        .all(|window| window[0].name < window[1].name)
    {
        return Err(DaError::InvalidPayload(
            "application roots must be sorted and unique".into(),
        ));
    }
    Ok(())
}

fn validate_application_namespace_record_count(
    policy: &DaNamespacePolicy,
    actual: usize,
) -> Result<(), DaError> {
    if actual < policy.min_records as usize || actual > policy.max_records as usize {
        return Err(DaError::InvalidPayload(format!(
            "namespace {} record count {actual} is outside profile bounds {}..={}",
            policy.namespace.0, policy.min_records, policy.max_records
        )));
    }
    Ok(())
}

fn validate_application_roots_against_profile(
    payload: &ApplicationDaPayload,
    profile: &DaApplicationProfile,
) -> Result<(), DaError> {
    let payload_roots = payload
        .application_roots
        .iter()
        .map(|root| (&root.name, root))
        .collect::<BTreeMap<_, _>>();
    for policy in &profile.root_bindings {
        let root = payload_roots.get(&policy.name);
        if policy.required && root.is_none() {
            return Err(DaError::InvalidPayload(format!(
                "application DA payload is missing required root {}",
                policy.name
            )));
        }
    }
    for root in &payload.application_roots {
        if !profile
            .root_bindings
            .iter()
            .any(|policy| policy.name == root.name)
        {
            return Err(DaError::InvalidPayload(format!(
                "application DA payload carries unknown root {}",
                root.name
            )));
        }
    }
    Ok(())
}

fn validate_application_payload_with_optional_manifest(
    validator_id: &str,
    profile: &DaApplicationProfile,
    payload: &ApplicationDaPayload,
    manifest: Option<&ApplicationDaManifest>,
) -> Result<DaApplicationValidationReport, DaError> {
    if validator_id != profile.profile_id()? {
        return Err(DaError::InvalidManifest(
            "application validator profile id does not match supplied profile".into(),
        ));
    }
    payload.validate(profile)?;
    if let Some(manifest) = manifest {
        verify_application_manifest_commits_payload(manifest, payload, profile)?;
    }
    application_validation_report(validator_id, payload, manifest)
}

fn application_validation_report(
    validator_id: &str,
    payload: &ApplicationDaPayload,
    manifest: Option<&ApplicationDaManifest>,
) -> Result<DaApplicationValidationReport, DaError> {
    let canonical = payload.canonicalized();
    let report = DaApplicationValidationReport {
        schema: DA_APPLICATION_VALIDATION_REPORT_SCHEMA.into(),
        schema_version: 1,
        validator_id: validator_id.into(),
        application_id: canonical.application_id.clone(),
        profile_id: canonical.profile_id.clone(),
        payload_hash: canonical.hash()?,
        manifest_hash: manifest
            .map(ApplicationDaManifest::manifest_hash)
            .transpose()?,
        payload_kind: canonical.payload_kind.clone(),
        sequence: canonical.coordinate.sequence,
        namespace_count: canonical.namespaces.len() as u32,
        record_count: canonical
            .namespaces
            .iter()
            .map(|section| section.records.len() as u32)
            .sum(),
        application_root: application_root_commitment(&canonical)?,
        accepted: true,
    };
    report.validate()?;
    Ok(report)
}

fn validate_application_schema_records(payload: &ApplicationDaPayload) -> Result<(), DaError> {
    for section in &payload.canonicalized().namespaces {
        for record in &section.records {
            match record.encoding {
                DaRecordEncoding::CanonicalJson | DaRecordEncoding::ExternalContentAddress => {
                    serde_json::from_slice::<serde_json::Value>(&record.bytes).map_err(|_| {
                        DaError::InvalidPayload(format!(
                            "record schema {} is not valid JSON",
                            record.schema
                        ))
                    })?;
                }
                DaRecordEncoding::OpaqueBytes | DaRecordEncoding::EncryptedBytes => {}
            }
        }
    }
    Ok(())
}

fn validate_social_demo_signatures(
    profile: &DaApplicationProfile,
    payload: &ApplicationDaPayload,
) -> Result<(), DaError> {
    let record_policies = profile
        .record_policies
        .iter()
        .map(|policy| (policy.schema.as_str(), policy))
        .collect::<BTreeMap<_, _>>();
    for section in &payload.canonicalized().namespaces {
        for record in &section.records {
            let Some(policy) = record_policies.get(record.schema.as_str()) else {
                continue;
            };
            if !policy.require_signer {
                continue;
            }
            let signer = record.signer.as_deref().ok_or_else(|| {
                DaError::InvalidPayload(format!(
                    "social-demo record {} is missing signer",
                    record.schema
                ))
            })?;
            let expected_signature = format!("sig-{signer}");
            if record.signature.as_deref() != Some(expected_signature.as_str()) {
                return Err(DaError::InvalidPayload(format!(
                    "social-demo record {} has invalid signature",
                    record.schema
                )));
            }
        }
    }
    Ok(())
}

fn validate_social_demo_event_log_root(payload: &ApplicationDaPayload) -> Result<(), DaError> {
    let expected = social_demo_event_log_root(payload)?;
    let actual = payload
        .canonicalized()
        .application_roots
        .iter()
        .find(|root| root.name == "social.event.log.root")
        .map(|root| root.hash.clone())
        .ok_or_else(|| {
            DaError::InvalidPayload("social-demo payload is missing event log root".into())
        })?;
    if actual != expected {
        return Err(DaError::InvalidPayload(
            "social-demo event log root does not match record content hashes".into(),
        ));
    }
    Ok(())
}

fn application_event_log_root_for_sections(
    sections: &[ApplicationDaNamespaceSection],
) -> Result<String, DaError> {
    let mut record_hashes = Vec::new();
    for section in sections {
        section.validate_structure()?;
        for record in &section.records {
            record_hashes.push(record.content_hash.clone());
        }
    }
    if record_hashes.is_empty() {
        return Err(DaError::InvalidPayload(
            "application event log root requires at least one record".into(),
        ));
    }
    merkle_root(&record_hashes)
}

fn validate_sorted_unique_record_encodings(encodings: &[DaRecordEncoding]) -> Result<(), DaError> {
    if !encodings.windows(2).all(|window| window[0] < window[1]) {
        return Err(DaError::InvalidManifest(
            "record encodings must be sorted and unique".into(),
        ));
    }
    Ok(())
}

fn validate_sorted_unique_namespace_policies(
    policies: &[DaNamespacePolicy],
) -> Result<(), DaError> {
    if policies.is_empty() {
        return Err(DaError::InvalidManifest(
            "application DA profile must declare namespace policies".into(),
        ));
    }
    if !policies
        .windows(2)
        .all(|window| window[0].namespace < window[1].namespace)
    {
        return Err(DaError::InvalidManifest(
            "namespace policies must be sorted and unique".into(),
        ));
    }
    Ok(())
}

fn validate_sorted_unique_record_policies(policies: &[DaRecordPolicy]) -> Result<(), DaError> {
    if !policies
        .windows(2)
        .all(|window| window[0].schema < window[1].schema)
    {
        return Err(DaError::InvalidManifest(
            "record policies must be sorted and unique by schema".into(),
        ));
    }
    Ok(())
}

fn validate_sorted_unique_root_policies(
    policies: &[DaApplicationRootPolicy],
) -> Result<(), DaError> {
    for policy in policies {
        policy.validate()?;
    }
    if !policies
        .windows(2)
        .all(|window| window[0].name < window[1].name)
    {
        return Err(DaError::InvalidManifest(
            "application root policies must be sorted and unique".into(),
        ));
    }
    Ok(())
}

fn validate_sorted_unique_namespace_retention_policies(
    policies: &[DaNamespaceRetentionPolicy],
) -> Result<(), DaError> {
    if !policies
        .windows(2)
        .all(|window| window[0].namespace < window[1].namespace)
    {
        return Err(DaError::InvalidManifest(
            "namespace retention overrides must be sorted and unique".into(),
        ));
    }
    Ok(())
}

fn validate_sorted_unique_payload_kind_retention_policies(
    policies: &[DaPayloadKindRetentionPolicy],
) -> Result<(), DaError> {
    if !policies
        .windows(2)
        .all(|window| window[0].payload_kind < window[1].payload_kind)
    {
        return Err(DaError::InvalidManifest(
            "payload kind retention overrides must be sorted and unique".into(),
        ));
    }
    Ok(())
}

fn is_bounded_segmented_identifier(value: &str, max_bytes: usize) -> bool {
    !value.is_empty()
        && value.len() <= max_bytes
        && !value.starts_with(['.', '-'])
        && !value.ends_with(['.', '-'])
        && !value.contains("..")
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.' || byte == b'-'
        })
}

fn validate_section_records(
    section: &DaNamespaceSection,
    allowed: impl Fn(&DaRecord) -> bool,
) -> Result<(), DaError> {
    if section.records.iter().all(allowed) {
        Ok(())
    } else {
        Err(DaError::InvalidPayload(format!(
            "{} contains an unsupported record type",
            section.namespace.0
        )))
    }
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
    ManifestPayloadMismatch { expected: String, actual: String },
    InvalidCodingFraudProof(String),
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
    use detta_core::{Argument, BlockHeader, Event, EventPayload, Method, Receipt, TxStatus};

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

    fn block_header() -> BlockHeader {
        BlockHeader {
            chain_id: "detta-test".into(),
            height: 7,
            previous_block_hash: "prev-block".into(),
            tx_root: "tx-root".into(),
            receipt_root: "receipt-root".into(),
            global_state_root: "global-root".into(),
            storage_root: "storage-root".into(),
            registry_root: "registry-root".into(),
            policy_root: "policy-root".into(),
            event_root: "event-root".into(),
            nonce_root: "nonce-root".into(),
            outbox_root: "outbox-root".into(),
            timestamp: 1,
            proposer: "validator-1".into(),
            consensus_certificate: "cert".into(),
            data_availability: None,
        }
    }

    fn receipt(tx_hash: &str) -> Receipt {
        Receipt {
            tx_hash: tx_hash.into(),
            status: TxStatus::Committed,
            error: None,
            return_value: None,
            resource_units_used: 1,
            storage_root_after: "storage-root".into(),
            registry_root_after: "registry-root".into(),
            policy_root_after: "policy-root".into(),
            event_root_after: "event-root".into(),
            nonce_root_after: "nonce-root".into(),
            global_state_root_after: "global-root".into(),
        }
    }

    fn event(tx_hash: &str) -> Event {
        Event {
            contract: "TokenA".into(),
            tx_hash: tx_hash.into(),
            index: 0,
            payload: EventPayload::Transfer {
                from: "Alice".into(),
                to: "Bob".into(),
                asset: "USDC".into(),
                amount: 1,
            },
        }
    }

    fn social_coordinate(sequence: u64) -> DaApplicationCoordinate {
        DaApplicationCoordinate {
            application_id: application_id_unchecked("social.demo"),
            stream_id: "global".into(),
            sequence,
            epoch: None,
            parent_hash: None,
            subject_hash: None,
        }
    }

    fn application_root(name: &str, seed: &[u8]) -> DaApplicationRoot {
        DaApplicationRoot::new(name, hash_bytes(seed)).unwrap()
    }

    fn application_record(
        schema: &str,
        encoding: DaRecordEncoding,
        bytes: &[u8],
        signer: Option<&str>,
    ) -> DaRecordEnvelope {
        DaRecordEnvelope::new(
            schema,
            1,
            "application/json",
            encoding,
            bytes.to_vec(),
            signer.map(String::from),
            signer.map(|value| format!("sig-{value}")),
        )
        .unwrap()
    }

    fn application_section(
        namespace: &str,
        records: Vec<DaRecordEnvelope>,
    ) -> ApplicationDaNamespaceSection {
        ApplicationDaNamespaceSection::new(DaNamespace::new(namespace).unwrap(), records).unwrap()
    }

    fn social_demo_payload() -> ApplicationDaPayload {
        let profile = DaApplicationProfile::social_demo_v1();
        let sections = vec![
            application_section(
                "social.feed",
                vec![application_record(
                    "social.post",
                    DaRecordEncoding::CanonicalJson,
                    br#"{"author":"alice","post_id":"post-1","text":"hello"}"#,
                    Some("alice"),
                )],
            ),
            application_section(
                "social.media",
                vec![application_record(
                    "social.media.reference",
                    DaRecordEncoding::ExternalContentAddress,
                    br#"{"content_hash":"media-root","uri":"ipfs://example"}"#,
                    None,
                )],
            ),
        ];
        let event_log_root = application_event_log_root_for_sections(&sections).unwrap();
        ApplicationDaPayload::new(
            &profile,
            social_coordinate(1),
            DaPayloadKind::Batch,
            None,
            vec![DaApplicationRoot::new("social.event.log.root", event_log_root).unwrap()],
            sections,
        )
        .unwrap()
    }

    fn forged_deterministic_share_set_from_payload_bytes(
        reference_payload: &DaPayload,
        payload_bytes: &[u8],
        share_size_bytes: usize,
    ) -> DaShareSet {
        assert!(!payload_bytes.is_empty());
        assert!(share_size_bytes > 0);
        let chunks: Vec<Vec<u8>> = payload_bytes
            .chunks(share_size_bytes)
            .map(|chunk| chunk.to_vec())
            .collect();
        let share_hashes: Vec<String> = chunks.iter().map(|chunk| hash_bytes(chunk)).collect();
        let encoded_share_count = u32::try_from(share_hashes.len()).unwrap();
        let manifest = DaManifest {
            schema: DA_MANIFEST_SCHEMA.into(),
            schema_version: 1,
            chain_id: reference_payload.chain_id.clone(),
            height: reference_payload.height,
            block_hash: "block-7".into(),
            payload_hash: hash_bytes(payload_bytes),
            payload_bytes: payload_bytes.len() as u64,
            namespace_root: reference_payload.namespace_root().unwrap(),
            share_root: share_root(&share_hashes).unwrap(),
            erasure_scheme: ErasureScheme::DeterministicChunks,
            original_share_count: encoded_share_count,
            encoded_share_count,
            reconstruction_threshold: encoded_share_count,
            share_size_bytes: u32::try_from(share_size_bytes).unwrap(),
            share_hashes,
            namespace_ranges: namespace_ranges(reference_payload),
        };
        manifest.validate().unwrap();
        let manifest_hash = manifest.manifest_hash().unwrap();
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
        DaShareSet { manifest, shares }
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
    fn reed_solomon_target_share_size_builds_profile_share_set() {
        let payload = payload_with_tx_count(7);
        let share_set =
            DaShareSet::from_payload_reed_solomon_with_target_share_size(&payload, "block-7", 128)
                .unwrap();

        assert_eq!(
            share_set.manifest.erasure_scheme,
            ErasureScheme::ReedSolomonV1
        );
        assert!(share_set.manifest.share_size_bytes <= 128);
        assert_eq!(
            share_set.manifest.encoded_share_count,
            share_set.manifest.original_share_count * 2
        );
        assert_eq!(
            share_set.manifest.reconstruction_threshold,
            share_set.manifest.original_share_count
        );
        assert_eq!(
            share_set.reconstruct_payload().unwrap(),
            payload.canonicalized()
        );
    }

    #[test]
    fn reed_solomon_target_share_size_rejects_unbounded_share_count() {
        let payload = payload_with_tx_count(200);

        assert!(matches!(
            DaShareSet::from_payload_reed_solomon_with_target_share_size(&payload, "block-7", 1),
            Err(DaError::InvalidManifest(message))
                if message.contains("would require") && message.contains("data shares")
        ));
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
    fn payload_decode_fuzz_smoke_rejects_malformed_reconstructed_bytes() {
        let reference_payload = payload_with_tx_count(2);
        let noncanonical_payload = DaPayload {
            chain_id: "detta-test".into(),
            height: 7,
            previous_block_hash: "prev-block".into(),
            block_payload_version: DA_PAYLOAD_VERSION,
            namespaces: vec![
                tx_section("detta.tx", vec![DaRecord::SignedTransaction(tx("tx-1", 1))]),
                tx_section(
                    "detta.governance",
                    vec![DaRecord::GovernancePayload {
                        proposal_id: "proposal-1".into(),
                        payload: "upgrade".into(),
                    }],
                ),
            ],
        };
        let unsupported_version = serde_json::json!({
            "chain_id": "detta-test",
            "height": 7,
            "previous_block_hash": "prev-block",
            "block_payload_version": DA_PAYLOAD_VERSION + 1,
            "namespaces": [{
                "namespace": "detta.tx",
                "records": [{"SignedTransaction": tx("tx-1", 1)}]
            }]
        });
        let corpus = vec![
            b"null".to_vec(),
            b"[]".to_vec(),
            b"{".to_vec(),
            b"{\"chain_id\":\"detta-test\"}".to_vec(),
            serde_json::to_vec(&unsupported_version).unwrap(),
            canonical_bytes(&noncanonical_payload).unwrap(),
        ];

        for payload_bytes in corpus {
            let share_set = forged_deterministic_share_set_from_payload_bytes(
                &reference_payload,
                &payload_bytes,
                17,
            );
            assert!(
                share_set.verify().is_err(),
                "malformed payload bytes unexpectedly verified: {payload_bytes:?}"
            );
        }
    }

    #[test]
    fn share_reconstruction_fuzz_smoke_rejects_mutated_shares_and_manifests() {
        let payload = payload_with_tx_count(16);
        let share_set = DaShareSet::from_payload_reed_solomon(&payload, "block-7", 4, 2).unwrap();

        for seed in 0..72_u32 {
            let mut selected = std::collections::BTreeSet::new();
            let mut offset = 0;
            while selected.len() < share_set.manifest.reconstruction_threshold as usize {
                selected.insert((seed + offset) % share_set.manifest.encoded_share_count);
                offset += 1;
            }
            let mut candidate = DaShareSet {
                manifest: share_set.manifest.clone(),
                shares: share_set
                    .shares
                    .iter()
                    .filter(|share| selected.contains(&share.index))
                    .cloned()
                    .collect(),
            };

            match seed % 6 {
                0 => {
                    assert_eq!(
                        candidate.reconstruct_payload().unwrap(),
                        payload.canonicalized()
                    );
                }
                1 => {
                    candidate.shares.pop();
                    assert!(matches!(
                        candidate.verify(),
                        Err(DaError::InsufficientShares { .. })
                    ));
                }
                2 => {
                    candidate.shares.push(candidate.shares[0].clone());
                    assert!(matches!(
                        candidate.verify(),
                        Err(DaError::DuplicateShare { .. })
                    ));
                }
                3 => {
                    candidate.shares[0].index = candidate.manifest.encoded_share_count;
                    assert!(matches!(
                        candidate.verify(),
                        Err(DaError::UnexpectedShare { .. })
                    ));
                }
                4 => {
                    candidate.shares[0].bytes[0] ^= 0x01;
                    assert!(matches!(
                        candidate.verify(),
                        Err(DaError::ShareHashMismatch { .. })
                    ));
                }
                _ => {
                    candidate.manifest.payload_hash = "00".repeat(32);
                    let manifest_hash = candidate.manifest.manifest_hash().unwrap();
                    for share in &mut candidate.shares {
                        share.manifest_hash = manifest_hash.clone();
                    }
                    assert!(matches!(
                        candidate.verify(),
                        Err(DaError::PayloadHashMismatch { .. })
                    ));
                }
            }
        }
    }

    #[test]
    fn verify_manifest_commits_payload_accepts_honest_and_rejects_forged_commitments() {
        let payload = payload_with_tx_count(5);

        for share_set in [
            DaShareSet::from_payload(&payload, "block-7", 48).unwrap(),
            DaShareSet::from_payload_reed_solomon(&payload, "block-7", 4, 2).unwrap(),
        ] {
            // Honest manifests round-trip against their payload.
            verify_manifest_commits_payload(&share_set.manifest, &payload).unwrap();

            // A manifest whose payload_hash/namespace_root match the real payload
            // but whose share commitment encodes something else is rejected, even
            // though the manifest is internally consistent.
            let other_payload = payload_with_tx_count(6);
            let forged_shares =
                DaShareSet::from_payload_reed_solomon(&other_payload, "block-7", 4, 2)
                    .unwrap()
                    .manifest;
            let mut forged = share_set.manifest.clone();
            forged.share_size_bytes = forged_shares.share_size_bytes;
            forged.original_share_count = forged_shares.original_share_count;
            forged.encoded_share_count = forged_shares.encoded_share_count;
            forged.reconstruction_threshold = forged_shares.reconstruction_threshold;
            forged.erasure_scheme = forged_shares.erasure_scheme.clone();
            forged.share_hashes = forged_shares.share_hashes.clone();
            forged.share_root = forged_shares.share_root.clone();
            // Internally consistent forgery, but does not commit `payload`.
            forged.validate().unwrap();
            assert!(matches!(
                verify_manifest_commits_payload(&forged, &payload),
                Err(DaError::ManifestPayloadMismatch { .. })
            ));
        }
    }

    #[test]
    fn coding_fraud_proof_detects_parity_and_payload_inconsistency() {
        let payload = payload_with_tx_count(5);
        let honest = DaShareSet::from_payload_reed_solomon(&payload, "block-7", 4, 2).unwrap();
        let data_shares: Vec<DaShare> = honest
            .shares
            .iter()
            .filter(|share| share.index < honest.manifest.original_share_count)
            .cloned()
            .collect();

        // An honest manifest is a valid encoding: no fraud proof can be built.
        assert!(matches!(
            DaCodingFraudProof::from_committed_data_shares(
                &honest.manifest,
                &data_shares,
                "reporter-1"
            ),
            Err(DaError::InvalidCodingFraudProof(_))
        ));

        // Forge a parity share hash: the committed shares are no longer a valid
        // codeword for the data shares.
        let mut parity_forged = honest.manifest.clone();
        let parity_index = parity_forged.original_share_count as usize;
        parity_forged.share_hashes[parity_index] = "00".repeat(32);
        parity_forged.share_root = share_root(&parity_forged.share_hashes).unwrap();
        let manifest_hash = parity_forged.manifest_hash().unwrap();
        let rebound: Vec<DaShare> = data_shares
            .iter()
            .cloned()
            .map(|mut share| {
                share.manifest_hash = manifest_hash.clone();
                share
            })
            .collect();
        let proof =
            DaCodingFraudProof::from_committed_data_shares(&parity_forged, &rebound, "reporter-1")
                .unwrap();
        assert!(matches!(proof.fault, DaCodingFault::ParityMismatch { .. }));
        proof.validate(&parity_forged).unwrap();
        // The proof does not verify against the honest manifest.
        assert!(proof.validate(&honest.manifest).is_err());

        // Forge the committed payload hash: the data shares decode to a payload
        // whose hash differs from the commitment.
        let mut payload_forged = honest.manifest.clone();
        payload_forged.payload_hash = "11".repeat(32);
        let payload_manifest_hash = payload_forged.manifest_hash().unwrap();
        let payload_rebound: Vec<DaShare> = data_shares
            .iter()
            .cloned()
            .map(|mut share| {
                share.manifest_hash = payload_manifest_hash.clone();
                share
            })
            .collect();
        let payload_proof = DaCodingFraudProof::from_committed_data_shares(
            &payload_forged,
            &payload_rebound,
            "reporter-1",
        )
        .unwrap();
        assert!(matches!(
            payload_proof.fault,
            DaCodingFault::PayloadHashMismatch { .. }
        ));
        payload_proof.validate(&payload_forged).unwrap();
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
    fn application_id_validation_rejects_invalid_values() {
        assert!(DaApplicationId::new("social.demo").is_ok());
        assert!(DaApplicationId::new("marketplace-orders.v1").is_ok());

        for value in [
            "",
            ".social",
            "social.",
            "-social",
            "social-",
            "social..demo",
            "Social.Demo",
            "social_demo",
        ] {
            assert!(matches!(
                DaApplicationId::new(value),
                Err(DaError::InvalidNamespace(_))
            ));
        }

        let oversized = "a".repeat(DA_APPLICATION_ID_MAX_BYTES + 1);
        assert!(matches!(
            DaApplicationId::new(oversized),
            Err(DaError::InvalidNamespace(_))
        ));
    }

    #[test]
    fn application_profiles_validate_and_hash_stably() {
        let defi = DaApplicationProfile::detta_defi_v1();
        defi.validate().unwrap();
        assert_eq!(defi.application_id, application_id_unchecked("detta.defi"));
        assert_eq!(defi.da_profile, DaProductionProfile::v1());
        assert_eq!(defi.profile_hash().unwrap(), defi.profile_id().unwrap());

        let social = DaApplicationProfile::social_demo_v1();
        social.validate().unwrap();
        let social_hash = social.profile_hash().unwrap();
        assert_eq!(social_hash.len(), 64);
        assert_eq!(social_hash, social.clone().profile_hash().unwrap());

        let mut reordered = social.clone();
        reordered.namespace_policies.swap(0, 1);
        assert!(matches!(
            reordered.validate(),
            Err(DaError::InvalidManifest(_))
        ));
        assert_eq!(reordered.canonicalized(), social);
    }

    #[test]
    fn application_profile_rejects_duplicate_policy_keys() {
        let mut duplicate_namespace = DaApplicationProfile::social_demo_v1();
        duplicate_namespace
            .namespace_policies
            .insert(1, duplicate_namespace.namespace_policies[0].clone());
        assert!(matches!(
            duplicate_namespace.validate(),
            Err(DaError::InvalidManifest(_))
        ));

        let mut duplicate_record = DaApplicationProfile::social_demo_v1();
        duplicate_record
            .record_policies
            .insert(1, duplicate_record.record_policies[0].clone());
        assert!(matches!(
            duplicate_record.validate(),
            Err(DaError::InvalidManifest(_))
        ));
    }

    #[test]
    fn application_profile_rejects_unknown_schema_and_forbidden_namespace() {
        let mut unknown_schema = DaApplicationProfile::social_demo_v1();
        unknown_schema.namespace_policies[0].allowed_record_schemas[0] = "social.unknown".into();
        assert!(matches!(
            unknown_schema.validate(),
            Err(DaError::InvalidManifest(_))
        ));

        let mut forbidden_namespace = DaApplicationProfile::social_demo_v1();
        forbidden_namespace
            .namespace_policies
            .push(namespace_policy(
                "social.spam",
                DaNamespaceRequirement::Forbidden,
                vec![],
                0,
                0,
                DaApplicationRetentionClass::Archive,
            ));
        forbidden_namespace.record_policies.push(record_policy(
            "social.spam.record",
            vec!["social.spam"],
            vec![DaRecordEncoding::CanonicalJson],
            1024,
            false,
        ));
        assert!(matches!(
            forbidden_namespace.validate(),
            Err(DaError::InvalidManifest(_))
        ));
    }

    #[test]
    fn application_profile_registry_registers_builtins_and_rejects_unknown_payloads() {
        let registry = DaApplicationProfileRegistry::with_builtin_profiles().unwrap();
        let social_profile = DaApplicationProfile::social_demo_v1();
        let social_id = social_profile.profile_id().unwrap();
        assert_eq!(registry.get_profile(&social_id).unwrap(), &social_profile);
        assert_eq!(
            registry
                .latest_active_profile(&application_id_unchecked("social.demo"))
                .unwrap()
                .profile_id,
            social_id
        );
        assert!(registry
            .get_profile(&DaApplicationProfile::detta_defi_v1().profile_id().unwrap())
            .is_ok());

        let empty_registry = DaApplicationProfileRegistry::new();
        assert!(matches!(
            empty_registry.validate_payload(&social_demo_payload()),
            Err(DaError::InvalidManifest(_))
        ));
    }

    #[test]
    fn application_profile_registry_keeps_deprecated_versions_for_history() {
        let mut registry = DaApplicationProfileRegistry::new();
        let v1 = DaApplicationProfile::social_demo_v1();
        let v1_id = registry.register(v1.clone()).unwrap();
        let v1_payload = social_demo_payload();

        let mut v2 = v1.clone();
        v2.profile_version = 2;
        v2.profile_name = "Social Demo DA v2".into();
        let v2_id = registry.register(v2.clone()).unwrap();
        assert_ne!(v1_id, v2_id);

        registry.deprecate(&v1_id, 42).unwrap();
        assert_eq!(
            registry
                .get_profile_version(&application_id_unchecked("social.demo"), 1)
                .unwrap()
                .status,
            DaApplicationProfileStatus::Deprecated
        );
        assert_eq!(
            registry
                .latest_active_profile(&application_id_unchecked("social.demo"))
                .unwrap()
                .profile_id,
            v2_id
        );
        assert!(matches!(
            registry.validate_payload(&v1_payload),
            Err(DaError::InvalidManifest(_))
        ));
        registry.validate_historical_payload(&v1_payload).unwrap();

        let v2_payload = ApplicationDaPayload::new(
            &v2,
            social_coordinate(2),
            DaPayloadKind::Batch,
            Some(v1_payload.hash().unwrap()),
            vec![application_root(
                "social.event.log.root",
                b"social-event-log-root-2",
            )],
            vec![application_section(
                "social.feed",
                vec![application_record(
                    "social.post",
                    DaRecordEncoding::CanonicalJson,
                    br#"{"author":"bob","post_id":"post-2","text":"next"}"#,
                    Some("bob"),
                )],
            )],
        )
        .unwrap();
        registry.validate_payload(&v2_payload).unwrap();

        let reloaded =
            DaApplicationProfileRegistry::from_registrations(registry.registrations()).unwrap();
        assert_eq!(
            reloaded
                .get_profile_version(&application_id_unchecked("social.demo"), 1)
                .unwrap()
                .status,
            DaApplicationProfileStatus::Deprecated
        );
        reloaded.validate_historical_payload(&v1_payload).unwrap();
        reloaded.validate_payload(&v2_payload).unwrap();
    }

    #[test]
    fn detta_application_profile_preserves_da_v1_payload_hashes() {
        let profile = DaApplicationProfile::detta_defi_v1();
        profile.validate().unwrap();

        let payload = payload_with_sections(vec![
            tx_section(
                "detta.block",
                vec![DaRecord::BlockHeader(Box::new(block_header()))],
            ),
            tx_section("detta.tx", vec![DaRecord::SignedTransaction(tx("tx-1", 1))]),
            tx_section("detta.receipt", vec![DaRecord::Receipt(receipt("tx-1"))]),
        ]);
        let hash_before = payload.hash().unwrap();
        validate_production_block_payload(&payload, &profile.da_profile).unwrap();
        assert_eq!(payload.hash().unwrap(), hash_before);
    }

    #[test]
    fn record_envelope_binds_content_hash_and_policy() {
        let policy = record_policy(
            "social.post",
            vec!["social.feed"],
            vec![DaRecordEncoding::CanonicalJson],
            256 * 1024,
            true,
        );
        let envelope = DaRecordEnvelope::new(
            "social.post",
            1,
            "application/json",
            DaRecordEncoding::CanonicalJson,
            br#"{"author":"alice","text":"hello"}"#.to_vec(),
            Some("alice".into()),
            Some("sig-alice".into()),
        )
        .unwrap();
        envelope.validate_against_policy(&policy).unwrap();

        let mut tampered = envelope.clone();
        tampered.content_hash = "bad-hash".into();
        assert!(matches!(
            tampered.validate_structure(),
            Err(DaError::PayloadHashMismatch { .. })
        ));

        let unsigned = DaRecordEnvelope::new(
            "social.post",
            1,
            "application/json",
            DaRecordEncoding::CanonicalJson,
            br#"{"author":"alice","text":"hello"}"#.to_vec(),
            None,
            None,
        )
        .unwrap();
        assert!(matches!(
            unsigned.validate_against_policy(&policy),
            Err(DaError::InvalidPayload(_))
        ));
    }

    #[test]
    fn application_payload_canonicalizes_reordered_namespaces() {
        let profile = DaApplicationProfile::social_demo_v1();
        let profile_id = profile.profile_id().unwrap();
        let payload = ApplicationDaPayload {
            schema: APPLICATION_DA_PAYLOAD_SCHEMA.into(),
            schema_version: 1,
            application_id: application_id_unchecked("social.demo"),
            profile_id,
            coordinate: social_coordinate(1),
            payload_kind: DaPayloadKind::Batch,
            previous_payload_hash: None,
            application_roots: vec![application_root(
                "social.event.log.root",
                b"social-event-log-root-1",
            )],
            namespaces: vec![
                application_section(
                    "social.moderation",
                    vec![application_record(
                        "social.moderation.action",
                        DaRecordEncoding::CanonicalJson,
                        br#"{"action":"hide","post_id":"post-1"}"#,
                        Some("moderator"),
                    )],
                ),
                application_section(
                    "social.feed",
                    vec![application_record(
                        "social.post",
                        DaRecordEncoding::CanonicalJson,
                        br#"{"author":"alice","post_id":"post-1","text":"hello"}"#,
                        Some("alice"),
                    )],
                ),
            ],
        };

        assert!(matches!(
            payload.validate(&profile),
            Err(DaError::InvalidPayload(_)) | Err(DaError::PayloadNotCanonical)
        ));
        let canonical = payload.canonicalized();
        canonical.validate(&profile).unwrap();
        assert_eq!(
            canonical
                .namespaces
                .iter()
                .map(|section| section.namespace.0.as_str())
                .collect::<Vec<_>>(),
            vec!["social.feed", "social.moderation"]
        );
    }

    #[test]
    fn application_payload_enforces_required_and_forbidden_namespaces() {
        let profile = DaApplicationProfile::social_demo_v1();
        let missing_required = ApplicationDaPayload::new(
            &profile,
            social_coordinate(1),
            DaPayloadKind::Batch,
            None,
            vec![application_root(
                "social.event.log.root",
                b"social-event-log-root-1",
            )],
            vec![application_section(
                "social.media",
                vec![application_record(
                    "social.media.reference",
                    DaRecordEncoding::ExternalContentAddress,
                    br#"{"uri":"ipfs://example"}"#,
                    None,
                )],
            )],
        );
        assert!(matches!(missing_required, Err(DaError::InvalidPayload(_))));

        let mut profile_with_forbidden = DaApplicationProfile::social_demo_v1();
        profile_with_forbidden
            .namespace_policies
            .push(namespace_policy(
                "social.spam",
                DaNamespaceRequirement::Forbidden,
                vec![],
                0,
                0,
                DaApplicationRetentionClass::Archive,
            ));
        profile_with_forbidden.validate().unwrap();
        let forbidden_payload = ApplicationDaPayload::new(
            &profile_with_forbidden,
            social_coordinate(1),
            DaPayloadKind::Batch,
            None,
            vec![application_root(
                "social.event.log.root",
                b"social-event-log-root-1",
            )],
            vec![
                application_section(
                    "social.feed",
                    vec![application_record(
                        "social.post",
                        DaRecordEncoding::CanonicalJson,
                        br#"{"author":"alice","post_id":"post-1","text":"hello"}"#,
                        Some("alice"),
                    )],
                ),
                application_section(
                    "social.spam",
                    vec![application_record(
                        "social.post",
                        DaRecordEncoding::CanonicalJson,
                        br#"{"author":"mallory","post_id":"spam-1"}"#,
                        Some("mallory"),
                    )],
                ),
            ],
        );
        assert!(matches!(forbidden_payload, Err(DaError::InvalidPayload(_))));
    }

    #[test]
    fn application_payload_rejects_bad_content_hash_unknown_schema_and_size() {
        let profile = DaApplicationProfile::social_demo_v1();
        let mut tampered_hash = social_demo_payload();
        tampered_hash.namespaces[0].records[0].content_hash = "bad-hash".into();
        assert!(matches!(
            tampered_hash.validate(&profile),
            Err(DaError::PayloadHashMismatch { .. })
        ));

        let unknown_schema = ApplicationDaPayload::new(
            &profile,
            social_coordinate(1),
            DaPayloadKind::Batch,
            None,
            vec![application_root(
                "social.event.log.root",
                b"social-event-log-root-1",
            )],
            vec![application_section(
                "social.feed",
                vec![application_record(
                    "social.unknown",
                    DaRecordEncoding::CanonicalJson,
                    br#"{"author":"alice","post_id":"post-1"}"#,
                    Some("alice"),
                )],
            )],
        );
        assert!(matches!(unknown_schema, Err(DaError::InvalidPayload(_))));

        let mut tiny_payload_profile = DaApplicationProfile::social_demo_v1();
        tiny_payload_profile.max_payload_bytes = 1;
        let payload = social_demo_payload();
        assert!(matches!(
            payload.validate(&tiny_payload_profile),
            Err(DaError::InvalidPayload(_))
        ));
    }

    #[test]
    fn social_demo_application_payload_fixture_hash_is_stable() {
        let payload = social_demo_payload();
        let payload_hash = application_payload_hash(&payload).unwrap();
        let namespace_root = payload.namespace_root().unwrap();

        assert_eq!(
            payload_hash,
            "a5908058f7142cb7c843007ac7be55b564a3bb39cdfb9d19341cfe78dd48b291"
        );
        assert_eq!(
            namespace_root,
            "8c8899bbfd003d4f945c883519edfa43bd3e862eba7df96bfac701e2cc9e6b92"
        );
    }

    #[test]
    fn application_reed_solomon_share_set_reconstructs_threshold_payload() {
        let profile = DaApplicationProfile::social_demo_v1();
        let payload = social_demo_payload();
        let share_set =
            ApplicationDaShareSet::from_payload_reed_solomon(&payload, &profile, 3, 2).unwrap();

        share_set.verify(&profile).unwrap();
        assert_eq!(share_set.reconstruct_payload(&profile).unwrap(), payload);

        let threshold_share_set = ApplicationDaShareSet {
            manifest: share_set.manifest.clone(),
            shares: vec![
                share_set.shares[0].clone(),
                share_set.shares[2].clone(),
                share_set.shares[4].clone(),
            ],
        };
        assert_eq!(
            threshold_share_set.reconstruct_payload(&profile).unwrap(),
            payload
        );
    }

    #[test]
    fn application_manifest_hash_binds_application_metadata() {
        let profile = DaApplicationProfile::social_demo_v1();
        let payload = social_demo_payload();
        let share_set =
            ApplicationDaShareSet::from_payload_reed_solomon(&payload, &profile, 3, 2).unwrap();
        let base_hash = share_set.manifest.manifest_hash().unwrap();

        let mut changed_application = share_set.manifest.clone();
        changed_application.application_id = application_id_unchecked("social.other");
        assert_ne!(changed_application.manifest_hash().unwrap(), base_hash);

        let mut changed_profile = share_set.manifest.clone();
        changed_profile.profile_id = hash_bytes(b"other-profile");
        assert_ne!(changed_profile.manifest_hash().unwrap(), base_hash);

        let mut changed_coordinate = share_set.manifest.clone();
        changed_coordinate.coordinate.sequence += 1;
        assert_ne!(changed_coordinate.manifest_hash().unwrap(), base_hash);

        let mut changed_payload = share_set.manifest.clone();
        changed_payload.payload_hash = hash_bytes(b"other-payload");
        assert_ne!(changed_payload.manifest_hash().unwrap(), base_hash);

        let mut changed_namespace_root = share_set.manifest.clone();
        changed_namespace_root.namespace_root = hash_bytes(b"other-namespace-root");
        assert_ne!(changed_namespace_root.manifest_hash().unwrap(), base_hash);

        let mut changed_share_root = share_set.manifest.clone();
        changed_share_root.share_root = hash_bytes(b"other-share-root");
        assert_ne!(changed_share_root.manifest_hash().unwrap(), base_hash);
    }

    #[test]
    fn application_manifest_rejects_bad_roots_and_wrong_profile() {
        let profile = DaApplicationProfile::social_demo_v1();
        let payload = social_demo_payload();
        let share_set =
            ApplicationDaShareSet::from_payload_reed_solomon(&payload, &profile, 3, 2).unwrap();

        let mut bad_share_root = share_set.manifest.clone();
        bad_share_root.share_root = hash_bytes(b"bad-share-root");
        assert!(matches!(
            bad_share_root.validate(&profile),
            Err(DaError::ShareRootMismatch { .. })
        ));

        let wrong_profile = DaApplicationProfile::detta_defi_v1();
        assert!(matches!(
            share_set.manifest.validate(&wrong_profile),
            Err(DaError::InvalidManifest(_))
        ));

        let mut bad_payload = payload.clone();
        bad_payload.profile_id = hash_bytes(b"wrong-profile");
        assert!(matches!(
            bad_payload.validate(&profile),
            Err(DaError::InvalidPayload(_))
        ));
    }

    #[test]
    fn application_manifest_commits_payload_and_namespace_proofs() {
        let profile = DaApplicationProfile::social_demo_v1();
        let payload = social_demo_payload();
        let share_set =
            ApplicationDaShareSet::from_payload_reed_solomon(&payload, &profile, 3, 2).unwrap();

        verify_application_manifest_commits_payload(&share_set.manifest, &payload, &profile)
            .unwrap();

        let proof = prove_application_namespace(
            &share_set.manifest,
            &profile,
            &DaNamespace::new("social.media").unwrap(),
        )
        .unwrap();
        let range =
            verify_application_namespace_proof(&share_set.manifest, &profile, &proof).unwrap();
        assert_eq!(
            range.map(|range| range.namespace),
            Some(DaNamespace::new("social.media").unwrap())
        );

        let missing = prove_application_namespace(
            &share_set.manifest,
            &profile,
            &DaNamespace::new("social.private").unwrap(),
        )
        .unwrap();
        assert!(
            verify_application_namespace_proof(&share_set.manifest, &profile, &missing)
                .unwrap()
                .is_none()
        );

        let mut tampered_proof = proof.clone();
        tampered_proof.namespace_ranges[0].record_count += 1;
        assert!(matches!(
            verify_application_namespace_proof(&share_set.manifest, &profile, &tampered_proof),
            Err(DaError::NamespaceRootMismatch { .. })
        ));
    }

    #[test]
    fn application_coding_fraud_proof_detects_parity_and_payload_inconsistency() {
        let profile = DaApplicationProfile::social_demo_v1();
        let payload = social_demo_payload();
        let honest =
            ApplicationDaShareSet::from_payload_reed_solomon(&payload, &profile, 4, 2).unwrap();
        let data_shares: Vec<DaShare> = honest
            .shares
            .iter()
            .filter(|share| share.index < honest.manifest.original_share_count)
            .cloned()
            .collect();

        assert!(matches!(
            ApplicationDaCodingFraudProof::from_committed_data_shares(
                &honest.manifest,
                &profile,
                &data_shares,
                "reporter-1"
            ),
            Err(DaError::InvalidCodingFraudProof(_))
        ));

        let mut parity_forged = honest.manifest.clone();
        let parity_index = parity_forged.original_share_count as usize;
        parity_forged.share_hashes[parity_index] = "00".repeat(32);
        parity_forged.share_root = share_root(&parity_forged.share_hashes).unwrap();
        let manifest_hash = parity_forged.manifest_hash().unwrap();
        let rebound: Vec<DaShare> = data_shares
            .iter()
            .cloned()
            .map(|mut share| {
                share.manifest_hash = manifest_hash.clone();
                share
            })
            .collect();
        let proof = ApplicationDaCodingFraudProof::from_committed_data_shares(
            &parity_forged,
            &profile,
            &rebound,
            "reporter-1",
        )
        .unwrap();
        assert!(matches!(proof.fault, DaCodingFault::ParityMismatch { .. }));
        proof.validate(&parity_forged, &profile).unwrap();
        assert!(proof.validate(&honest.manifest, &profile).is_err());

        let mut payload_forged = honest.manifest.clone();
        payload_forged.payload_hash = "11".repeat(32);
        let payload_manifest_hash = payload_forged.manifest_hash().unwrap();
        let payload_rebound: Vec<DaShare> = data_shares
            .iter()
            .cloned()
            .map(|mut share| {
                share.manifest_hash = payload_manifest_hash.clone();
                share
            })
            .collect();
        let payload_proof = ApplicationDaCodingFraudProof::from_committed_data_shares(
            &payload_forged,
            &profile,
            &payload_rebound,
            "reporter-1",
        )
        .unwrap();
        assert!(matches!(
            payload_proof.fault,
            DaCodingFault::PayloadHashMismatch { .. }
        ));
        payload_proof.validate(&payload_forged, &profile).unwrap();
    }

    #[test]
    fn application_validators_produce_reports_and_schema_validator_rejects_bad_json() {
        let profile = DaApplicationProfile::social_demo_v1();
        let payload = social_demo_payload();
        let share_set =
            ApplicationDaShareSet::from_payload_reed_solomon(&payload, &profile, 3, 2).unwrap();
        let schema_validator = SchemaApplicationValidator::new(&profile).unwrap();

        let report = schema_validator
            .validate_payload(&profile, &payload, Some(&share_set.manifest))
            .unwrap();
        report.validate().unwrap();
        assert_eq!(
            report,
            schema_validator
                .validate_payload(&profile, &payload, Some(&share_set.manifest))
                .unwrap()
        );
        assert_eq!(report.report_hash().unwrap().len(), 64);

        let mut bad_json = payload.clone();
        bad_json.namespaces[0].records[0].bytes = b"{bad-json".to_vec();
        bad_json.namespaces[0].records[0].content_hash =
            hash_bytes(&bad_json.namespaces[0].records[0].bytes);
        let opaque_validator = OpaqueApplicationValidator::new(&profile).unwrap();
        opaque_validator
            .validate_payload(&profile, &bad_json, None)
            .unwrap();
        assert!(matches!(
            schema_validator.validate_payload(&profile, &bad_json, None),
            Err(DaError::InvalidPayload(_))
        ));
    }

    #[test]
    fn social_demo_validator_rejects_bad_signature_sequence_gap_and_wrong_root() {
        let profile = DaApplicationProfile::social_demo_v1();
        let validator = SocialDemoDaValidator::new().unwrap();
        let payload = social_demo_payload();
        let share_set =
            ApplicationDaShareSet::from_payload_reed_solomon(&payload, &profile, 3, 2).unwrap();

        validator
            .validate_payload(&profile, &payload, Some(&share_set.manifest))
            .unwrap();

        let mut bad_signature = payload.clone();
        bad_signature.namespaces[0].records[0].signature = Some("not-the-signature".into());
        assert!(matches!(
            validator.validate_payload(&profile, &bad_signature, None),
            Err(DaError::InvalidPayload(_))
        ));

        let sections = vec![application_section(
            "social.feed",
            vec![application_record(
                "social.post",
                DaRecordEncoding::CanonicalJson,
                br#"{"author":"bob","post_id":"post-2","text":"gap"}"#,
                Some("bob"),
            )],
        )];
        let event_log_root = application_event_log_root_for_sections(&sections).unwrap();
        let sequence_gap = ApplicationDaPayload::new(
            &profile,
            social_coordinate(2),
            DaPayloadKind::Batch,
            None,
            vec![DaApplicationRoot::new("social.event.log.root", event_log_root).unwrap()],
            sections,
        )
        .unwrap();
        assert!(matches!(
            validator.validate_payload(&profile, &sequence_gap, None),
            Err(DaError::InvalidPayload(_))
        ));

        let mut wrong_root = payload.clone();
        wrong_root.application_roots[0].hash = hash_bytes(b"wrong-social-root");
        assert!(matches!(
            validator.validate_payload(&profile, &wrong_root, None),
            Err(DaError::InvalidPayload(_))
        ));
    }

    #[test]
    fn detta_defi_validator_accepts_v1_payload_and_rejects_tampered_evidence() {
        let validator = DettaDefiDaValidator::new().unwrap();
        let payload = payload_with_sections(vec![
            tx_section(
                "detta.block",
                vec![DaRecord::BlockHeader(Box::new(block_header()))],
            ),
            tx_section("detta.tx", vec![DaRecord::SignedTransaction(tx("tx-1", 1))]),
            tx_section("detta.receipt", vec![DaRecord::Receipt(receipt("tx-1"))]),
        ]);
        let share_set = DaShareSet::from_payload_reed_solomon(&payload, "block-7", 3, 2).unwrap();
        let report = validator
            .validate_block_payload(&payload, Some(&share_set.manifest))
            .unwrap();
        report.validate().unwrap();
        assert_eq!(
            report.application_id,
            application_id_unchecked("detta.defi")
        );
        assert_eq!(report.payload_kind, DaPayloadKind::Block);

        let tampered_evidence = payload_with_sections(vec![
            tx_section(
                "detta.block",
                vec![DaRecord::BlockHeader(Box::new(block_header()))],
            ),
            tx_section("detta.oracle", vec![DaRecord::Event(event("tx-1"))]),
        ]);
        assert!(matches!(
            validator.validate_block_payload(&tampered_evidence, None),
            Err(DaError::InvalidPayload(_))
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
    fn production_profile_v1_answers_open_da_policy_questions() {
        let profile = DaProductionProfile::v1();

        profile.validate().unwrap();
        assert_eq!(
            profile.commitment_scheme,
            DaCommitmentScheme::MerkleSha256V1
        );
        assert_eq!(profile.erasure_scheme, ErasureScheme::ReedSolomonV1);
        assert_eq!(
            profile.custody_mode,
            DaCustodyMode::DeterministicCustodyWithLightClientSampling
        );
        assert!(profile.full_payload_required_for_rpc);
        assert!(profile.receipts_are_payload_records);
        assert_eq!(
            profile.event_availability_mode,
            DaEventAvailabilityMode::RegeneratedFromExecutionRoots
        );
        assert_eq!(profile.data_gas_for_bytes(0).unwrap(), 0);
        assert_eq!(profile.data_gas_for_bytes(1).unwrap(), 1);
        assert_eq!(
            profile
                .data_gas_for_bytes(DA_V1_DATA_GAS_BYTES_PER_UNIT + 1)
                .unwrap(),
            2
        );
        assert!(profile.archive_min_retention_blocks >= profile.validator_min_retention_blocks);
        assert_eq!(
            profile.mandatory_da_namespaces,
            vec![
                namespace_unchecked("detta.aspect"),
                namespace_unchecked("detta.block"),
                namespace_unchecked("detta.bridge"),
                namespace_unchecked("detta.governance"),
                namespace_unchecked("detta.oracle"),
                namespace_unchecked("detta.receipt"),
                namespace_unchecked("detta.tx"),
            ]
        );
    }

    #[test]
    fn production_block_payload_policy_accepts_transactions_receipts_and_optional_events() {
        let payload = payload_with_sections(vec![
            tx_section(
                "detta.block",
                vec![DaRecord::BlockHeader(Box::new(block_header()))],
            ),
            tx_section("detta.tx", vec![DaRecord::SignedTransaction(tx("tx-1", 1))]),
            tx_section("detta.receipt", vec![DaRecord::Receipt(receipt("tx-1"))]),
            tx_section("detta.event", vec![DaRecord::Event(event("tx-1"))]),
        ]);

        validate_production_block_payload(&payload, &DaProductionProfile::v1()).unwrap();
    }

    #[test]
    fn production_block_payload_policy_rejects_receipt_count_mismatch() {
        let payload = payload_with_sections(vec![
            tx_section(
                "detta.block",
                vec![DaRecord::BlockHeader(Box::new(block_header()))],
            ),
            tx_section(
                "detta.tx",
                vec![
                    DaRecord::SignedTransaction(tx("tx-1", 1)),
                    DaRecord::SignedTransaction(tx("tx-2", 2)),
                ],
            ),
            tx_section("detta.receipt", vec![DaRecord::Receipt(receipt("tx-1"))]),
        ]);

        assert!(matches!(
            validate_production_block_payload(&payload, &DaProductionProfile::v1()),
            Err(DaError::InvalidPayload(message))
                if message.contains("transaction count 2 does not match receipt count 1")
        ));
    }

    #[test]
    fn production_block_payload_policy_rejects_unsupported_namespace_records() {
        let wrong_receipt_section = payload_with_sections(vec![
            tx_section(
                "detta.block",
                vec![DaRecord::BlockHeader(Box::new(block_header()))],
            ),
            tx_section("detta.tx", vec![DaRecord::SignedTransaction(tx("tx-1", 1))]),
            tx_section(
                "detta.receipt",
                vec![DaRecord::SignedTransaction(tx("tx-1", 1))],
            ),
        ]);
        assert!(matches!(
            validate_production_block_payload(&wrong_receipt_section, &DaProductionProfile::v1()),
            Err(DaError::InvalidPayload(message))
                if message.contains("detta.receipt contains a non-receipt record")
        ));

        let unsupported_namespace = payload_with_sections(vec![
            tx_section(
                "detta.block",
                vec![DaRecord::BlockHeader(Box::new(block_header()))],
            ),
            tx_section("detta.debug", vec![DaRecord::Event(event("tx-1"))]),
        ]);
        assert!(matches!(
            validate_production_block_payload(&unsupported_namespace, &DaProductionProfile::v1()),
            Err(DaError::InvalidPayload(message))
                if message.contains("unsupported production block DA namespace detta.debug")
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
