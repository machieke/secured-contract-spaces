use detta_consensus::{FinalityCertificate, SlashingRecord};
use detta_core::{Block, DeTTaState, Receipt, SnapshotError, StateSnapshot, Transaction};
use detta_da::{
    payload_hash, DaAvailabilityCertificate, DaChallengeRecord, DaManifest, DaPayload, DaShare,
    DaShareSet, DA_V1_ARCHIVE_MIN_RETENTION_BLOCKS, DA_V1_VALIDATOR_MIN_RETENTION_BLOCKS,
};
use detta_protocol::{SignedValidatorMessage, ValidatorSetMetadata, ValidatorSignatureDomain};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

const MAX_ENCODED_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageError {
    Io(String),
    CorruptData(String),
    InvalidSnapshot(SnapshotError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileStorage {
    root: PathBuf,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ValidatorSetMetadataAuditOutcome {
    Applied,
    Pruned,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidatorSetMetadataAuditRecord {
    pub update_id: String,
    pub outcome: ValidatorSetMetadataAuditOutcome,
    pub height: u64,
    pub signers: Vec<String>,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotImportAuditRecord {
    pub snapshot_root: String,
    pub manifest_hash: String,
    pub required_metadata_roots_root: String,
    pub required_metadata_roots_count: usize,
    pub manifest_metadata_roots_count: usize,
    pub chunk_count: u32,
    pub metadata_roots_verified: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub da_manifest_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub da_certificate_hash: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotImportAuditConfig {
    pub max_records: usize,
    pub max_page_size: usize,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConsensusSigningRecord {
    pub validator_id: String,
    pub domain: ValidatorSignatureDomain,
    pub height: u64,
    pub block_hash: String,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaStorageStats {
    pub manifest_count: u64,
    pub expected_share_count: u64,
    pub stored_share_count: u64,
    pub missing_share_count: u64,
    pub payload_count: u64,
    pub certificate_count: u64,
    pub challenge_count: u64,
    pub repair_record_count: u64,
    pub manifest_bytes: u64,
    pub share_bytes: u64,
    pub payload_bytes: u64,
    pub certificate_bytes: u64,
    pub challenge_bytes: u64,
    pub repair_record_bytes: u64,
    pub index_file_count: u64,
    pub index_bytes: u64,
    pub retention_policy_root: Option<String>,
    pub retention_policy_bytes: u64,
    pub retention_policy: Option<DaRetentionPolicyConfig>,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaRetentionAuditEntry {
    pub manifest_hash: String,
    pub chain_id: String,
    pub height: u64,
    pub block_hash: String,
    pub class: DaRetentionClass,
    pub age_blocks: u64,
    pub retention_expires_at_height: Option<u64>,
    pub expired: bool,
    pub policy_present: bool,
    pub retain_payloads: bool,
    pub retain_all_shares: bool,
    pub min_retention_blocks: Option<u64>,
    pub max_payload_bytes: Option<u64>,
    pub payload_bytes: u64,
    pub payload_within_policy_limit: bool,
    pub payload_present: bool,
    pub expected_share_count: u32,
    pub stored_share_count: u32,
    pub missing_share_count: u32,
    pub payload_retention_satisfied: bool,
    pub share_retention_satisfied: bool,
    pub retention_satisfied: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaRetentionAuditReport {
    pub current_height: u64,
    pub policy_root: Option<String>,
    pub policy: Option<DaRetentionPolicyConfig>,
    pub manifest_count: u64,
    pub expired_manifest_count: u64,
    pub active_manifest_count: u64,
    pub missing_policy_class_count: u64,
    pub unsatisfied_manifest_count: u64,
    pub entries: Vec<DaRetentionAuditEntry>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum DaRetentionClass {
    Hot,
    Warm,
    Cold,
    Checkpoint,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaRetentionPolicy {
    pub class: DaRetentionClass,
    pub retain_payloads: bool,
    pub retain_all_shares: bool,
    pub min_retention_blocks: u64,
    pub max_payload_bytes: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaRetentionPolicyConfig {
    pub policies: Vec<DaRetentionPolicy>,
}

impl DaRetentionPolicyConfig {
    pub fn production_default() -> Self {
        Self {
            policies: vec![
                DaRetentionPolicy {
                    class: DaRetentionClass::Hot,
                    retain_payloads: true,
                    retain_all_shares: true,
                    min_retention_blocks: DA_V1_VALIDATOR_MIN_RETENTION_BLOCKS,
                    max_payload_bytes: Some(MAX_ENCODED_BYTES),
                },
                DaRetentionPolicy {
                    class: DaRetentionClass::Warm,
                    retain_payloads: true,
                    retain_all_shares: false,
                    min_retention_blocks: DA_V1_VALIDATOR_MIN_RETENTION_BLOCKS * 4,
                    max_payload_bytes: Some(MAX_ENCODED_BYTES),
                },
                DaRetentionPolicy {
                    class: DaRetentionClass::Cold,
                    retain_payloads: true,
                    retain_all_shares: true,
                    min_retention_blocks: DA_V1_ARCHIVE_MIN_RETENTION_BLOCKS,
                    max_payload_bytes: None,
                },
                DaRetentionPolicy {
                    class: DaRetentionClass::Checkpoint,
                    retain_payloads: true,
                    retain_all_shares: true,
                    min_retention_blocks: DA_V1_ARCHIVE_MIN_RETENTION_BLOCKS,
                    max_payload_bytes: None,
                },
            ],
        }
    }

    pub fn validate(&self) -> Result<(), StorageError> {
        validate_da_retention_policy(self)
    }

    pub fn policy_for_class(&self, class: DaRetentionClass) -> Option<&DaRetentionPolicy> {
        self.policies.iter().find(|policy| policy.class == class)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaRepairRecord {
    pub manifest_hash: String,
    pub missing_share_indices: Vec<u32>,
    pub recorded_at_height: u64,
    pub reason: String,
    pub completed: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaManifestIndexEntry {
    pub manifest_hash: String,
    pub chain_id: String,
    pub height: u64,
    pub block_hash: String,
    pub payload_hash: String,
    pub share_root: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaCertificateIndexEntry {
    pub certificate_hash: String,
    pub chain_id: String,
    pub height: u64,
    pub block_hash: String,
    pub manifest_hash: String,
    pub share_root: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DaStoreRoots {
    pub manifest_root: String,
    pub share_root: String,
    pub payload_root: String,
    pub certificate_root: String,
    pub challenge_root: String,
    pub repair_root: String,
    pub index_root: String,
    pub retention_policy_root: Option<String>,
    pub root: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct StorageBackupManifest {
    pub file_count: usize,
    pub total_bytes: u64,
    pub snapshot_root: Option<String>,
    pub highest_block_height: Option<u64>,
    pub highest_finality_certificate_height: Option<u64>,
}

impl FileStorage {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let root = root.into();
        fs::create_dir_all(root.join("blocks")).map_err(io_error)?;
        fs::create_dir_all(root.join("certificates")).map_err(io_error)?;
        fs::create_dir_all(root.join("slashings")).map_err(io_error)?;
        fs::create_dir_all(root.join("consensus_signing_records")).map_err(io_error)?;
        fs::create_dir_all(root.join("da").join("manifests")).map_err(io_error)?;
        fs::create_dir_all(root.join("da").join("shares")).map_err(io_error)?;
        fs::create_dir_all(root.join("da").join("payloads")).map_err(io_error)?;
        fs::create_dir_all(root.join("da").join("certificates")).map_err(io_error)?;
        fs::create_dir_all(root.join("da").join("challenges")).map_err(io_error)?;
        fs::create_dir_all(root.join("da").join("repairs")).map_err(io_error)?;
        fs::create_dir_all(root.join("da").join("indexes").join("manifests_by_height"))
            .map_err(io_error)?;
        fs::create_dir_all(
            root.join("da")
                .join("indexes")
                .join("manifests_by_block_hash"),
        )
        .map_err(io_error)?;
        fs::create_dir_all(
            root.join("da")
                .join("indexes")
                .join("certificates_by_manifest"),
        )
        .map_err(io_error)?;
        fs::create_dir_all(
            root.join("da")
                .join("indexes")
                .join("certificates_by_height"),
        )
        .map_err(io_error)?;
        fs::create_dir_all(
            root.join("da")
                .join("indexes")
                .join("certificates_by_block_hash"),
        )
        .map_err(io_error)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn storage_bytes(&self) -> Result<u64, StorageError> {
        directory_size_bytes(&self.root)
    }

    pub fn da_storage_stats(&self) -> Result<DaStorageStats, StorageError> {
        let mut stats = DaStorageStats::default();
        let manifests_dir = self.root.join("da").join("manifests");
        let shares_dir = self.root.join("da").join("shares");
        let payloads_dir = self.root.join("da").join("payloads");
        let certificates_dir = self.root.join("da").join("certificates");
        let challenges_dir = self.root.join("da").join("challenges");
        let repairs_dir = self.root.join("da").join("repairs");
        let indexes_dir = self.root.join("da").join("indexes");

        for entry in fs::read_dir(&manifests_dir).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("bin") {
                continue;
            }
            let metadata = fs::metadata(&path).map_err(io_error)?;
            if !metadata.is_file() {
                continue;
            }
            let manifest: DaManifest = read_json(&path)?;
            manifest.validate().map_err(da_error)?;
            let manifest_hash = manifest.manifest_hash().map_err(da_error)?;
            stats.manifest_count += 1;
            stats.expected_share_count = stats
                .expected_share_count
                .saturating_add(manifest.encoded_share_count as u64);
            stats.manifest_bytes = stats.manifest_bytes.saturating_add(metadata.len());
            for index in 0..manifest.encoded_share_count {
                if !self.da_share_path(&manifest_hash, index).exists() {
                    stats.missing_share_count += 1;
                }
            }
        }

        let share_file_stats = directory_file_stats(&shares_dir)?;
        stats.stored_share_count = share_file_stats.file_count;
        stats.share_bytes = share_file_stats.total_bytes;
        let payload_file_stats = directory_file_stats(&payloads_dir)?;
        stats.payload_count = payload_file_stats.file_count;
        stats.payload_bytes = payload_file_stats.total_bytes;
        let certificate_file_stats = directory_file_stats(&certificates_dir)?;
        stats.certificate_count = certificate_file_stats.file_count;
        stats.certificate_bytes = certificate_file_stats.total_bytes;
        let challenge_file_stats = directory_file_stats(&challenges_dir)?;
        stats.challenge_count = challenge_file_stats.file_count;
        stats.challenge_bytes = challenge_file_stats.total_bytes;
        let repair_file_stats = directory_file_stats(&repairs_dir)?;
        stats.repair_record_count = repair_file_stats.file_count;
        stats.repair_record_bytes = repair_file_stats.total_bytes;
        let index_file_stats = directory_file_stats(&indexes_dir)?;
        stats.index_file_count = index_file_stats.file_count;
        stats.index_bytes = index_file_stats.total_bytes;
        if let Some(retention_policy) = self.maybe_load_da_retention_policy()? {
            let retention_policy_path = self.da_retention_policy_path();
            let metadata = fs::metadata(&retention_policy_path).map_err(io_error)?;
            if metadata.is_file() {
                stats.retention_policy_bytes = metadata.len();
            }
            stats.retention_policy_root = Some(hash_canonical_json(&retention_policy)?);
            stats.retention_policy = Some(retention_policy);
        }
        stats.total_bytes = stats
            .manifest_bytes
            .saturating_add(stats.share_bytes)
            .saturating_add(stats.payload_bytes)
            .saturating_add(stats.certificate_bytes)
            .saturating_add(stats.challenge_bytes)
            .saturating_add(stats.repair_record_bytes)
            .saturating_add(stats.index_bytes)
            .saturating_add(stats.retention_policy_bytes);
        Ok(stats)
    }

    pub fn da_retention_audit(
        &self,
        current_height: u64,
    ) -> Result<DaRetentionAuditReport, StorageError> {
        let policy = self.maybe_load_da_retention_policy()?;
        let policy_root = policy.as_ref().map(hash_canonical_json).transpose()?;
        let mut entries = Vec::new();

        for path in sorted_bin_paths(&self.root.join("da").join("manifests"))? {
            let manifest: DaManifest = read_json(&path)?;
            manifest.validate().map_err(da_error)?;
            let manifest_hash = manifest.manifest_hash().map_err(da_error)?;
            let expected_file_name = format!("{}.bin", file_safe_id(&manifest_hash));
            if path.file_name().and_then(|name| name.to_str()) != Some(expected_file_name.as_str())
            {
                return Err(StorageError::CorruptData(
                    "DA manifest filename does not match manifest hash".into(),
                ));
            }

            let class = da_manifest_retention_class(&manifest);
            let class_policy = policy
                .as_ref()
                .and_then(|config| config.policy_for_class(class));
            let age_blocks = current_height.saturating_sub(manifest.height);
            let retention_expires_at_height = class_policy
                .map(|policy| manifest.height.saturating_add(policy.min_retention_blocks));
            let expired =
                retention_expires_at_height.is_some_and(|height| current_height >= height);
            let retain_payloads = class_policy.is_some_and(|policy| policy.retain_payloads);
            let retain_all_shares = class_policy.is_some_and(|policy| policy.retain_all_shares);
            let max_payload_bytes = class_policy.and_then(|policy| policy.max_payload_bytes);
            let payload_within_policy_limit =
                max_payload_bytes.is_none_or(|max_bytes| manifest.payload_bytes <= max_bytes);
            let payload_present = self.da_payload_path(&manifest_hash).exists();
            let mut stored_share_count = 0_u32;
            for index in 0..manifest.encoded_share_count {
                if self.da_share_path(&manifest_hash, index).exists() {
                    stored_share_count = stored_share_count.saturating_add(1);
                }
            }
            let missing_share_count = manifest
                .encoded_share_count
                .saturating_sub(stored_share_count);
            let payload_retention_required = retain_payloads && !expired;
            let share_retention_required = retain_all_shares && !expired;
            let payload_retention_satisfied =
                !payload_retention_required || (payload_present && payload_within_policy_limit);
            let share_retention_satisfied = !share_retention_required || missing_share_count == 0;
            let retention_satisfied = class_policy.is_some()
                && payload_within_policy_limit
                && payload_retention_satisfied
                && share_retention_satisfied;

            entries.push(DaRetentionAuditEntry {
                manifest_hash,
                chain_id: manifest.chain_id,
                height: manifest.height,
                block_hash: manifest.block_hash,
                class,
                age_blocks,
                retention_expires_at_height,
                expired,
                policy_present: class_policy.is_some(),
                retain_payloads,
                retain_all_shares,
                min_retention_blocks: class_policy.map(|policy| policy.min_retention_blocks),
                max_payload_bytes,
                payload_bytes: manifest.payload_bytes,
                payload_within_policy_limit,
                payload_present,
                expected_share_count: manifest.encoded_share_count,
                stored_share_count,
                missing_share_count,
                payload_retention_satisfied,
                share_retention_satisfied,
                retention_satisfied,
            });
        }

        entries.sort_by(|left, right| {
            left.height
                .cmp(&right.height)
                .then_with(|| left.manifest_hash.cmp(&right.manifest_hash))
        });

        let manifest_count = entries.len() as u64;
        let expired_manifest_count = entries.iter().filter(|entry| entry.expired).count() as u64;
        let missing_policy_class_count =
            entries.iter().filter(|entry| !entry.policy_present).count() as u64;
        let unsatisfied_manifest_count = entries
            .iter()
            .filter(|entry| !entry.retention_satisfied)
            .count() as u64;

        Ok(DaRetentionAuditReport {
            current_height,
            policy_root,
            policy,
            manifest_count,
            expired_manifest_count,
            active_manifest_count: manifest_count.saturating_sub(expired_manifest_count),
            missing_policy_class_count,
            unsatisfied_manifest_count,
            entries,
        })
    }

    pub fn backup_to(
        &self,
        destination: impl AsRef<Path>,
    ) -> Result<StorageBackupManifest, StorageError> {
        let destination = destination.as_ref();
        fs::create_dir_all(destination).map_err(io_error)?;
        let mut manifest = StorageBackupManifest {
            file_count: 0,
            total_bytes: 0,
            snapshot_root: if self.snapshot_path().exists() {
                Some(self.load_snapshot()?.global_state_root)
            } else {
                None
            },
            highest_block_height: self
                .load_blocks()?
                .into_iter()
                .map(|block| block.header.height)
                .max(),
            highest_finality_certificate_height: self.highest_finality_certificate_height()?,
        };
        copy_directory_contents(&self.root, destination, &mut manifest)?;
        Ok(manifest)
    }

    pub fn restore_from_backup(
        backup_root: impl AsRef<Path>,
        restore_root: impl Into<PathBuf>,
    ) -> Result<Self, StorageError> {
        let restore_root = restore_root.into();
        fs::create_dir_all(&restore_root).map_err(io_error)?;
        let mut manifest = StorageBackupManifest {
            file_count: 0,
            total_bytes: 0,
            snapshot_root: None,
            highest_block_height: None,
            highest_finality_certificate_height: None,
        };
        copy_directory_contents(backup_root.as_ref(), &restore_root, &mut manifest)?;
        Self::open(restore_root)
    }

    pub fn commit_snapshot(&self, snapshot: &StateSnapshot) -> Result<(), StorageError> {
        DeTTaState::from_snapshot(snapshot.clone()).map_err(StorageError::InvalidSnapshot)?;
        write_json_atomic(&self.snapshot_path(), snapshot)
    }

    pub fn load_snapshot(&self) -> Result<StateSnapshot, StorageError> {
        let snapshot: StateSnapshot = read_json(&self.snapshot_path())?;
        DeTTaState::from_snapshot(snapshot.clone()).map_err(StorageError::InvalidSnapshot)?;
        Ok(snapshot)
    }

    pub fn commit_snapshot_metadata_roots(
        &self,
        roots: &BTreeMap<String, String>,
    ) -> Result<(), StorageError> {
        write_json_atomic(&self.snapshot_metadata_roots_path(), roots)
    }

    pub fn load_snapshot_metadata_roots(&self) -> Result<BTreeMap<String, String>, StorageError> {
        let path = self.snapshot_metadata_roots_path();
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        read_json(&path)
    }

    pub fn commit_required_snapshot_metadata_roots(
        &self,
        roots: &BTreeMap<String, String>,
    ) -> Result<(), StorageError> {
        write_json_atomic(&self.required_snapshot_metadata_roots_path(), roots)
    }

    pub fn load_required_snapshot_metadata_roots(
        &self,
    ) -> Result<BTreeMap<String, String>, StorageError> {
        let path = self.required_snapshot_metadata_roots_path();
        if !path.exists() {
            return Ok(BTreeMap::new());
        }
        read_json(&path)
    }

    pub fn required_snapshot_metadata_roots_root(&self) -> Result<String, StorageError> {
        let roots = self.load_required_snapshot_metadata_roots()?;
        Self::required_snapshot_metadata_roots_root_for(&roots)
    }

    pub fn required_snapshot_metadata_roots_root_for(
        roots: &BTreeMap<String, String>,
    ) -> Result<String, StorageError> {
        hash_canonical_json(roots)
    }

    pub fn commit_snapshot_sync_client_metrics<T: Serialize>(
        &self,
        metrics: &T,
    ) -> Result<(), StorageError> {
        write_json_atomic(&self.snapshot_sync_client_metrics_path(), metrics)
    }

    pub fn load_snapshot_sync_client_metrics<T: DeserializeOwned>(
        &self,
    ) -> Result<Option<T>, StorageError> {
        let path = self.snapshot_sync_client_metrics_path();
        if !path.exists() {
            return Ok(None);
        }
        read_json(&path).map(Some)
    }

    pub fn snapshot_sync_client_metrics_root<T: DeserializeOwned + Serialize>(
        &self,
    ) -> Result<Option<String>, StorageError> {
        self.load_snapshot_sync_client_metrics::<T>()?
            .map(|metrics| Self::snapshot_sync_client_metrics_root_for(&metrics))
            .transpose()
    }

    pub fn snapshot_sync_client_metrics_root_for<T: Serialize>(
        metrics: &T,
    ) -> Result<String, StorageError> {
        hash_canonical_json(metrics)
    }

    pub fn commit_block(&self, block: &Block) -> Result<(), StorageError> {
        write_json_atomic(&self.block_path(block.header.height), block)
    }

    pub fn load_block(&self, height: u64) -> Result<Block, StorageError> {
        read_json(&self.block_path(height))
    }

    pub fn maybe_load_block(&self, height: u64) -> Result<Option<Block>, StorageError> {
        let path = self.block_path(height);
        if !path.exists() {
            return Ok(None);
        }
        read_json(&path).map(Some)
    }

    pub fn load_blocks(&self) -> Result<Vec<Block>, StorageError> {
        let mut heights = Vec::new();
        for entry in fs::read_dir(self.root.join("blocks")).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("bin") {
                continue;
            }
            let Some(height) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.parse::<u64>().ok())
            else {
                continue;
            };
            heights.push(height);
        }
        heights.sort_unstable();
        heights
            .into_iter()
            .map(|height| self.load_block(height))
            .collect()
    }

    fn highest_finality_certificate_height(&self) -> Result<Option<u64>, StorageError> {
        let mut highest = None;
        for entry in fs::read_dir(self.root.join("certificates")).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("bin") {
                continue;
            }
            let Some(height) = path
                .file_stem()
                .and_then(|stem| stem.to_str())
                .and_then(|stem| stem.parse::<u64>().ok())
            else {
                continue;
            };
            highest = Some(highest.map_or(height, |current: u64| current.max(height)));
        }
        Ok(highest)
    }

    pub fn find_transaction(&self, tx_hash: &str) -> Result<Option<Transaction>, StorageError> {
        for block in self.load_blocks()? {
            if let Some(transaction) = block
                .transactions
                .into_iter()
                .find(|transaction| transaction.tx_hash == tx_hash)
            {
                return Ok(Some(transaction));
            }
        }
        Ok(None)
    }

    pub fn find_receipt(&self, tx_hash: &str) -> Result<Option<Receipt>, StorageError> {
        for block in self.load_blocks()? {
            if let Some(receipt) = block
                .receipts
                .into_iter()
                .find(|receipt| receipt.tx_hash == tx_hash)
            {
                return Ok(Some(receipt));
            }
        }
        Ok(None)
    }

    pub fn commit_finality_certificate(
        &self,
        certificate: &FinalityCertificate,
    ) -> Result<(), StorageError> {
        write_json_atomic(&self.certificate_path(certificate.height), certificate)
    }

    pub fn load_finality_certificate(
        &self,
        height: u64,
    ) -> Result<FinalityCertificate, StorageError> {
        read_json(&self.certificate_path(height))
    }

    pub fn maybe_load_finality_certificate(
        &self,
        height: u64,
    ) -> Result<Option<FinalityCertificate>, StorageError> {
        let path = self.certificate_path(height);
        if !path.exists() {
            return Ok(None);
        }
        read_json(&path).map(Some)
    }

    pub fn commit_slashing_record(&self, record: &SlashingRecord) -> Result<(), StorageError> {
        write_json_atomic(&self.slashing_path(&record.validator_id), record)
    }

    pub fn load_slashing_record(&self, validator_id: &str) -> Result<SlashingRecord, StorageError> {
        read_json(&self.slashing_path(validator_id))
    }

    pub fn maybe_load_slashing_record(
        &self,
        validator_id: &str,
    ) -> Result<Option<SlashingRecord>, StorageError> {
        let path = self.slashing_path(validator_id);
        if !path.exists() {
            return Ok(None);
        }
        read_json(&path).map(Some)
    }

    pub fn load_slashing_records(&self) -> Result<Vec<SlashingRecord>, StorageError> {
        let mut paths = Vec::new();
        for entry in fs::read_dir(self.root.join("slashings")).map_err(io_error)? {
            let path = entry.map_err(io_error)?.path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("bin") {
                paths.push(path);
            }
        }
        paths.sort();
        paths.into_iter().map(|path| read_json(&path)).collect()
    }

    pub fn commit_validator_set_metadata(
        &self,
        metadata: &ValidatorSetMetadata,
    ) -> Result<(), StorageError> {
        write_json_atomic(&self.validator_set_path(), metadata)
    }

    pub fn load_validator_set_metadata(&self) -> Result<ValidatorSetMetadata, StorageError> {
        read_json(&self.validator_set_path())
    }

    pub fn maybe_load_validator_set_metadata(
        &self,
    ) -> Result<Option<ValidatorSetMetadata>, StorageError> {
        let path = self.validator_set_path();
        if !path.exists() {
            return Ok(None);
        }
        read_json(&path).map(Some)
    }

    pub fn commit_pending_validator_set_metadata_authorizations(
        &self,
        authorizations: &[SignedValidatorMessage],
    ) -> Result<(), StorageError> {
        write_json_atomic(
            &self.pending_validator_set_metadata_authorizations_path(),
            &authorizations,
        )
    }

    pub fn load_pending_validator_set_metadata_authorizations(
        &self,
    ) -> Result<Vec<SignedValidatorMessage>, StorageError> {
        let path = self.pending_validator_set_metadata_authorizations_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        read_json(&path)
    }

    pub fn append_validator_set_metadata_audit_record(
        &self,
        record: ValidatorSetMetadataAuditRecord,
    ) -> Result<(), StorageError> {
        self.append_validator_set_metadata_audit_record_with_retention(record, usize::MAX)
    }

    pub fn append_validator_set_metadata_audit_record_with_retention(
        &self,
        record: ValidatorSetMetadataAuditRecord,
        max_records: usize,
    ) -> Result<(), StorageError> {
        let mut records = self.load_validator_set_metadata_audit_records()?;
        records.push(record);
        if records.len() > max_records {
            let remove_count = records.len() - max_records;
            records.drain(0..remove_count);
        }
        write_json_atomic(&self.validator_set_metadata_audit_path(), &records)
    }

    pub fn load_validator_set_metadata_audit_records(
        &self,
    ) -> Result<Vec<ValidatorSetMetadataAuditRecord>, StorageError> {
        let path = self.validator_set_metadata_audit_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        read_json(&path)
    }

    pub fn load_validator_set_metadata_audit_records_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<ValidatorSetMetadataAuditRecord>, StorageError> {
        Ok(self
            .load_validator_set_metadata_audit_records()?
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect())
    }

    pub fn validator_set_metadata_audit_root(&self) -> Result<String, StorageError> {
        let records = self.load_validator_set_metadata_audit_records()?;
        hash_canonical_json(&records)
    }

    pub fn append_snapshot_import_audit_record(
        &self,
        record: SnapshotImportAuditRecord,
    ) -> Result<(), StorageError> {
        self.append_snapshot_import_audit_record_with_retention(record, usize::MAX)
    }

    pub fn append_snapshot_import_audit_record_with_retention(
        &self,
        record: SnapshotImportAuditRecord,
        max_records: usize,
    ) -> Result<(), StorageError> {
        let mut records = self.load_snapshot_import_audit_records()?;
        records.push(record);
        if records.len() > max_records {
            let remove_count = records.len() - max_records;
            records.drain(0..remove_count);
        }
        write_json_atomic(&self.snapshot_import_audit_path(), &records)
    }

    pub fn load_snapshot_import_audit_records(
        &self,
    ) -> Result<Vec<SnapshotImportAuditRecord>, StorageError> {
        let path = self.snapshot_import_audit_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        read_json(&path)
    }

    pub fn load_snapshot_import_audit_records_page(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Vec<SnapshotImportAuditRecord>, StorageError> {
        Ok(self
            .load_snapshot_import_audit_records()?
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect())
    }

    pub fn snapshot_import_audit_root(&self) -> Result<String, StorageError> {
        let records = self.load_snapshot_import_audit_records()?;
        Self::snapshot_import_audit_root_for(&records)
    }

    pub fn snapshot_import_audit_root_for(
        records: &[SnapshotImportAuditRecord],
    ) -> Result<String, StorageError> {
        hash_canonical_json(&records)
    }

    pub fn commit_snapshot_import_audit_config(
        &self,
        config: &SnapshotImportAuditConfig,
    ) -> Result<(), StorageError> {
        write_json_atomic(&self.snapshot_import_audit_config_path(), config)
    }

    pub fn maybe_load_snapshot_import_audit_config(
        &self,
    ) -> Result<Option<SnapshotImportAuditConfig>, StorageError> {
        let path = self.snapshot_import_audit_config_path();
        if !path.exists() {
            return Ok(None);
        }
        read_json(&path).map(Some)
    }

    pub fn snapshot_import_audit_config_root(&self) -> Result<Option<String>, StorageError> {
        self.maybe_load_snapshot_import_audit_config()?
            .map(|config| Self::snapshot_import_audit_config_root_for(&config))
            .transpose()
    }

    pub fn snapshot_import_audit_config_root_for(
        config: &SnapshotImportAuditConfig,
    ) -> Result<String, StorageError> {
        hash_canonical_json(config)
    }

    pub fn commit_mempool(&self, transactions: &[Transaction]) -> Result<(), StorageError> {
        write_json_atomic(&self.mempool_path(), &transactions)
    }

    pub fn load_mempool(&self) -> Result<Vec<Transaction>, StorageError> {
        let path = self.mempool_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        read_json(&path)
    }

    pub fn commit_da_manifest(&self, manifest: &DaManifest) -> Result<String, StorageError> {
        manifest.validate().map_err(da_error)?;
        let manifest_hash = manifest.manifest_hash().map_err(da_error)?;
        write_json_atomic(&self.da_manifest_path(&manifest_hash), manifest)?;
        self.upsert_da_manifest_indexes(&manifest_hash, manifest)?;
        Ok(manifest_hash)
    }

    pub fn maybe_load_da_manifest(
        &self,
        manifest_hash: &str,
    ) -> Result<Option<DaManifest>, StorageError> {
        let path = self.da_manifest_path(manifest_hash);
        if !path.exists() {
            return Ok(None);
        }
        self.load_da_manifest(manifest_hash).map(Some)
    }

    pub fn load_da_manifest(&self, manifest_hash: &str) -> Result<DaManifest, StorageError> {
        let manifest: DaManifest = read_json(&self.da_manifest_path(manifest_hash))?;
        manifest.validate().map_err(da_error)?;
        let actual_hash = manifest.manifest_hash().map_err(da_error)?;
        if actual_hash != manifest_hash {
            return Err(StorageError::CorruptData(format!(
                "DA manifest hash mismatch: expected {manifest_hash}, got {actual_hash}"
            )));
        }
        Ok(manifest)
    }

    pub fn load_da_manifest_index_by_height(
        &self,
        height: u64,
    ) -> Result<Vec<DaManifestIndexEntry>, StorageError> {
        self.load_da_manifest_index_entries(
            &self.da_manifests_by_height_index_path(height),
            Some(height),
            None,
        )
    }

    pub fn load_da_manifest_index_by_block_hash(
        &self,
        block_hash: &str,
    ) -> Result<Vec<DaManifestIndexEntry>, StorageError> {
        self.load_da_manifest_index_entries(
            &self.da_manifests_by_block_hash_index_path(block_hash),
            None,
            Some(block_hash),
        )
    }

    pub fn commit_da_certificate(
        &self,
        certificate: &DaAvailabilityCertificate,
    ) -> Result<String, StorageError> {
        certificate.validate().map_err(da_error)?;
        let certificate_hash = certificate.certificate_hash().map_err(da_error)?;
        write_json_atomic(&self.da_certificate_path(&certificate_hash), certificate)?;
        self.upsert_da_certificate_indexes(&certificate_hash, certificate)?;
        Ok(certificate_hash)
    }

    pub fn maybe_load_da_certificate(
        &self,
        certificate_hash: &str,
    ) -> Result<Option<DaAvailabilityCertificate>, StorageError> {
        let path = self.da_certificate_path(certificate_hash);
        if !path.exists() {
            return Ok(None);
        }
        self.load_da_certificate(certificate_hash).map(Some)
    }

    pub fn load_da_certificate(
        &self,
        certificate_hash: &str,
    ) -> Result<DaAvailabilityCertificate, StorageError> {
        let certificate: DaAvailabilityCertificate =
            read_json(&self.da_certificate_path(certificate_hash))?;
        certificate.validate().map_err(da_error)?;
        let actual_hash = certificate.certificate_hash().map_err(da_error)?;
        if actual_hash != certificate_hash {
            return Err(StorageError::CorruptData(format!(
                "DA certificate hash mismatch: expected {certificate_hash}, got {actual_hash}"
            )));
        }
        Ok(certificate)
    }

    pub fn load_da_certificate_index_by_manifest_hash(
        &self,
        manifest_hash: &str,
    ) -> Result<Vec<DaCertificateIndexEntry>, StorageError> {
        self.load_da_certificate_index_entries(
            &self.da_certificates_by_manifest_index_path(manifest_hash),
            Some(manifest_hash),
            None,
            None,
        )
    }

    pub fn load_da_certificate_index_by_height(
        &self,
        height: u64,
    ) -> Result<Vec<DaCertificateIndexEntry>, StorageError> {
        self.load_da_certificate_index_entries(
            &self.da_certificates_by_height_index_path(height),
            None,
            Some(height),
            None,
        )
    }

    pub fn load_da_certificate_index_by_block_hash(
        &self,
        block_hash: &str,
    ) -> Result<Vec<DaCertificateIndexEntry>, StorageError> {
        self.load_da_certificate_index_entries(
            &self.da_certificates_by_block_hash_index_path(block_hash),
            None,
            None,
            Some(block_hash),
        )
    }

    pub fn rebuild_da_indexes(&self) -> Result<(), StorageError> {
        let indexes_path = self.da_indexes_path();
        if indexes_path.exists() {
            fs::remove_dir_all(&indexes_path).map_err(io_error)?;
        }
        self.create_da_index_dirs()?;

        for path in sorted_bin_paths(&self.root.join("da").join("manifests"))? {
            let manifest: DaManifest = read_json(&path)?;
            manifest.validate().map_err(da_error)?;
            let manifest_hash = manifest.manifest_hash().map_err(da_error)?;
            let expected_file_name = format!("{}.bin", file_safe_id(&manifest_hash));
            if path.file_name().and_then(|name| name.to_str()) != Some(expected_file_name.as_str())
            {
                return Err(StorageError::CorruptData(
                    "DA manifest filename does not match manifest hash".into(),
                ));
            }
            self.upsert_da_manifest_indexes(&manifest_hash, &manifest)?;
        }

        for path in sorted_bin_paths(&self.root.join("da").join("certificates"))? {
            let certificate: DaAvailabilityCertificate = read_json(&path)?;
            certificate.validate().map_err(da_error)?;
            let certificate_hash = certificate.certificate_hash().map_err(da_error)?;
            let expected_file_name = format!("{}.bin", file_safe_id(&certificate_hash));
            if path.file_name().and_then(|name| name.to_str()) != Some(expected_file_name.as_str())
            {
                return Err(StorageError::CorruptData(
                    "DA certificate filename does not match certificate hash".into(),
                ));
            }
            self.upsert_da_certificate_indexes(&certificate_hash, &certificate)?;
        }
        Ok(())
    }

    fn upsert_da_manifest_indexes(
        &self,
        manifest_hash: &str,
        manifest: &DaManifest,
    ) -> Result<(), StorageError> {
        let entry = DaManifestIndexEntry {
            manifest_hash: manifest_hash.into(),
            chain_id: manifest.chain_id.clone(),
            height: manifest.height,
            block_hash: manifest.block_hash.clone(),
            payload_hash: manifest.payload_hash.clone(),
            share_root: manifest.share_root.clone(),
        };
        self.upsert_da_manifest_index_entry(
            &self.da_manifests_by_height_index_path(manifest.height),
            Some(manifest.height),
            None,
            &entry,
        )?;
        self.upsert_da_manifest_index_entry(
            &self.da_manifests_by_block_hash_index_path(&manifest.block_hash),
            None,
            Some(&manifest.block_hash),
            &entry,
        )
    }

    fn upsert_da_manifest_index_entry(
        &self,
        path: &Path,
        expected_height: Option<u64>,
        expected_block_hash: Option<&str>,
        entry: &DaManifestIndexEntry,
    ) -> Result<(), StorageError> {
        let mut entries =
            self.load_da_manifest_index_entries(path, expected_height, expected_block_hash)?;
        entries.retain(|existing| existing.manifest_hash != entry.manifest_hash);
        entries.push(entry.clone());
        entries.sort_by(|left, right| left.manifest_hash.cmp(&right.manifest_hash));
        write_json_atomic(path, &entries)
    }

    fn load_da_manifest_index_entries(
        &self,
        path: &Path,
        expected_height: Option<u64>,
        expected_block_hash: Option<&str>,
    ) -> Result<Vec<DaManifestIndexEntry>, StorageError> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let entries: Vec<DaManifestIndexEntry> = read_json(path)?;
        let mut previous_manifest_hash: Option<&str> = None;
        for entry in &entries {
            if entry.manifest_hash.is_empty()
                || entry.chain_id.is_empty()
                || entry.block_hash.is_empty()
                || entry.payload_hash.is_empty()
                || entry.share_root.is_empty()
            {
                return Err(StorageError::CorruptData(
                    "DA manifest index entry contains an empty required field".into(),
                ));
            }
            if let Some(previous) = previous_manifest_hash {
                if previous >= entry.manifest_hash.as_str() {
                    return Err(StorageError::CorruptData(
                        "DA manifest index entries must be sorted and unique".into(),
                    ));
                }
            }
            previous_manifest_hash = Some(entry.manifest_hash.as_str());
            if expected_height.is_some_and(|height| entry.height != height) {
                return Err(StorageError::CorruptData(
                    "DA manifest index height does not match index path".into(),
                ));
            }
            if expected_block_hash.is_some_and(|block_hash| entry.block_hash.as_str() != block_hash)
            {
                return Err(StorageError::CorruptData(
                    "DA manifest index block hash does not match index path".into(),
                ));
            }
            let manifest = self.load_da_manifest(&entry.manifest_hash)?;
            if entry.chain_id.as_str() != manifest.chain_id.as_str()
                || entry.height != manifest.height
                || entry.block_hash.as_str() != manifest.block_hash.as_str()
                || entry.payload_hash.as_str() != manifest.payload_hash.as_str()
                || entry.share_root.as_str() != manifest.share_root.as_str()
            {
                return Err(StorageError::CorruptData(
                    "DA manifest index entry does not match stored manifest".into(),
                ));
            }
        }
        Ok(entries)
    }

    fn upsert_da_certificate_indexes(
        &self,
        certificate_hash: &str,
        certificate: &DaAvailabilityCertificate,
    ) -> Result<(), StorageError> {
        let entry = DaCertificateIndexEntry {
            certificate_hash: certificate_hash.into(),
            chain_id: certificate.chain_id.clone(),
            height: certificate.height,
            block_hash: certificate.block_hash.clone(),
            manifest_hash: certificate.manifest_hash.clone(),
            share_root: certificate.share_root.clone(),
        };
        self.upsert_da_certificate_index_entry(
            &self.da_certificates_by_manifest_index_path(&certificate.manifest_hash),
            Some(&certificate.manifest_hash),
            None,
            None,
            &entry,
        )?;
        self.upsert_da_certificate_index_entry(
            &self.da_certificates_by_height_index_path(certificate.height),
            None,
            Some(certificate.height),
            None,
            &entry,
        )?;
        self.upsert_da_certificate_index_entry(
            &self.da_certificates_by_block_hash_index_path(&certificate.block_hash),
            None,
            None,
            Some(&certificate.block_hash),
            &entry,
        )
    }

    fn upsert_da_certificate_index_entry(
        &self,
        path: &Path,
        expected_manifest_hash: Option<&str>,
        expected_height: Option<u64>,
        expected_block_hash: Option<&str>,
        entry: &DaCertificateIndexEntry,
    ) -> Result<(), StorageError> {
        let mut entries = self.load_da_certificate_index_entries(
            path,
            expected_manifest_hash,
            expected_height,
            expected_block_hash,
        )?;
        entries.retain(|existing| existing.certificate_hash != entry.certificate_hash);
        entries.push(entry.clone());
        entries.sort_by(|left, right| left.certificate_hash.cmp(&right.certificate_hash));
        write_json_atomic(path, &entries)
    }

    fn load_da_certificate_index_entries(
        &self,
        path: &Path,
        expected_manifest_hash: Option<&str>,
        expected_height: Option<u64>,
        expected_block_hash: Option<&str>,
    ) -> Result<Vec<DaCertificateIndexEntry>, StorageError> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let entries: Vec<DaCertificateIndexEntry> = read_json(path)?;
        let mut previous_certificate_hash: Option<&str> = None;
        for entry in &entries {
            if entry.certificate_hash.is_empty()
                || entry.chain_id.is_empty()
                || entry.block_hash.is_empty()
                || entry.manifest_hash.is_empty()
                || entry.share_root.is_empty()
            {
                return Err(StorageError::CorruptData(
                    "DA certificate index entry contains an empty required field".into(),
                ));
            }
            if let Some(previous) = previous_certificate_hash {
                if previous >= entry.certificate_hash.as_str() {
                    return Err(StorageError::CorruptData(
                        "DA certificate index entries must be sorted and unique".into(),
                    ));
                }
            }
            previous_certificate_hash = Some(entry.certificate_hash.as_str());
            if expected_manifest_hash
                .is_some_and(|manifest_hash| entry.manifest_hash.as_str() != manifest_hash)
            {
                return Err(StorageError::CorruptData(
                    "DA certificate index manifest hash does not match index path".into(),
                ));
            }
            if expected_height.is_some_and(|height| entry.height != height) {
                return Err(StorageError::CorruptData(
                    "DA certificate index height does not match index path".into(),
                ));
            }
            if expected_block_hash.is_some_and(|block_hash| entry.block_hash.as_str() != block_hash)
            {
                return Err(StorageError::CorruptData(
                    "DA certificate index block hash does not match index path".into(),
                ));
            }
            let certificate = self.load_da_certificate(&entry.certificate_hash)?;
            if entry.chain_id.as_str() != certificate.chain_id.as_str()
                || entry.height != certificate.height
                || entry.block_hash.as_str() != certificate.block_hash.as_str()
                || entry.manifest_hash.as_str() != certificate.manifest_hash.as_str()
                || entry.share_root.as_str() != certificate.share_root.as_str()
            {
                return Err(StorageError::CorruptData(
                    "DA certificate index entry does not match stored certificate".into(),
                ));
            }
        }
        Ok(entries)
    }

    pub fn commit_da_challenge_record(
        &self,
        record: &DaChallengeRecord,
    ) -> Result<String, StorageError> {
        record.validate().map_err(da_error)?;
        let challenge_id = record.challenge_id().map_err(da_error)?;
        write_json_atomic(&self.da_challenge_record_path(&challenge_id), record)?;
        Ok(challenge_id)
    }

    pub fn maybe_load_da_challenge_record(
        &self,
        challenge_id: &str,
    ) -> Result<Option<DaChallengeRecord>, StorageError> {
        let path = self.da_challenge_record_path(challenge_id);
        if !path.exists() {
            return Ok(None);
        }
        self.load_da_challenge_record(challenge_id).map(Some)
    }

    pub fn load_da_challenge_record(
        &self,
        challenge_id: &str,
    ) -> Result<DaChallengeRecord, StorageError> {
        let record: DaChallengeRecord = read_json(&self.da_challenge_record_path(challenge_id))?;
        record.validate().map_err(da_error)?;
        let actual_id = record.challenge_id().map_err(da_error)?;
        if actual_id != challenge_id {
            return Err(StorageError::CorruptData(format!(
                "DA challenge record hash mismatch: expected {challenge_id}, got {actual_id}"
            )));
        }
        Ok(record)
    }

    pub fn load_da_challenge_records(&self) -> Result<Vec<DaChallengeRecord>, StorageError> {
        let mut records = Vec::new();
        for path in sorted_bin_paths(&self.root.join("da").join("challenges"))? {
            let record: DaChallengeRecord = read_json(&path)?;
            record.validate().map_err(da_error)?;
            let challenge_id = record.challenge_id().map_err(da_error)?;
            let expected_file_name = format!("{}.bin", file_safe_id(&challenge_id));
            if path.file_name().and_then(|name| name.to_str()) != Some(expected_file_name.as_str())
            {
                return Err(StorageError::CorruptData(
                    "DA challenge record filename does not match challenge hash".into(),
                ));
            }
            records.push(record);
        }
        Ok(records)
    }

    pub fn commit_da_payload(
        &self,
        manifest_hash: &str,
        payload: &DaPayload,
    ) -> Result<(), StorageError> {
        self.validate_da_payload(manifest_hash, payload)?;
        write_json_atomic(
            &self.da_payload_path(manifest_hash),
            &payload.canonicalized(),
        )
    }

    pub fn maybe_load_da_payload(
        &self,
        manifest_hash: &str,
    ) -> Result<Option<DaPayload>, StorageError> {
        let path = self.da_payload_path(manifest_hash);
        if !path.exists() {
            return Ok(None);
        }
        self.load_da_payload(manifest_hash).map(Some)
    }

    pub fn load_da_payload(&self, manifest_hash: &str) -> Result<DaPayload, StorageError> {
        let payload: DaPayload = read_json(&self.da_payload_path(manifest_hash))?;
        self.validate_da_payload(manifest_hash, &payload)?;
        Ok(payload.canonicalized())
    }

    fn validate_da_payload(
        &self,
        manifest_hash: &str,
        payload: &DaPayload,
    ) -> Result<(), StorageError> {
        let manifest = self.load_da_manifest(manifest_hash)?;
        let actual_payload_hash = payload_hash(payload).map_err(da_error)?;
        if actual_payload_hash != manifest.payload_hash {
            return Err(StorageError::CorruptData(format!(
                "DA payload hash mismatch: expected {}, got {actual_payload_hash}",
                manifest.payload_hash
            )));
        }
        let actual_namespace_root = payload.namespace_root().map_err(da_error)?;
        if actual_namespace_root != manifest.namespace_root {
            return Err(StorageError::CorruptData(format!(
                "DA payload namespace root mismatch: expected {}, got {actual_namespace_root}",
                manifest.namespace_root
            )));
        }
        Ok(())
    }

    pub fn commit_da_share(&self, share: &DaShare) -> Result<(), StorageError> {
        let actual_hash = detta_da::hash_share_bytes(&share.bytes);
        if actual_hash != share.share_hash {
            return Err(StorageError::CorruptData(format!(
                "DA share hash mismatch for index {}: expected {}, got {}",
                share.index, share.share_hash, actual_hash
            )));
        }
        write_json_atomic(
            &self.da_share_path(&share.manifest_hash, share.index),
            share,
        )
    }

    pub fn load_da_share(&self, manifest_hash: &str, index: u32) -> Result<DaShare, StorageError> {
        let share: DaShare = read_json(&self.da_share_path(manifest_hash, index))?;
        if share.manifest_hash != manifest_hash {
            return Err(StorageError::CorruptData(format!(
                "DA share manifest hash mismatch for index {index}: expected {manifest_hash}, got {}",
                share.manifest_hash
            )));
        }
        if share.index != index {
            return Err(StorageError::CorruptData(format!(
                "DA share index mismatch: expected {index}, got {}",
                share.index
            )));
        }
        let actual_hash = detta_da::hash_share_bytes(&share.bytes);
        if actual_hash != share.share_hash {
            return Err(StorageError::CorruptData(format!(
                "DA share hash mismatch for index {index}: expected {}, got {actual_hash}",
                share.share_hash
            )));
        }
        Ok(share)
    }

    pub fn maybe_load_da_share(
        &self,
        manifest_hash: &str,
        index: u32,
    ) -> Result<Option<DaShare>, StorageError> {
        let path = self.da_share_path(manifest_hash, index);
        if !path.exists() {
            return Ok(None);
        }
        self.load_da_share(manifest_hash, index).map(Some)
    }

    pub fn commit_da_share_set(&self, share_set: &DaShareSet) -> Result<String, StorageError> {
        let payload = share_set.reconstruct_payload().map_err(da_error)?;
        let manifest_hash = self.commit_da_manifest(&share_set.manifest)?;
        for share in &share_set.shares {
            if share.manifest_hash != manifest_hash {
                return Err(StorageError::CorruptData(format!(
                    "DA share set manifest hash mismatch: expected {manifest_hash}, got {}",
                    share.manifest_hash
                )));
            }
            self.commit_da_share(share)?;
        }
        self.commit_da_payload(&manifest_hash, &payload)?;
        Ok(manifest_hash)
    }

    pub fn load_da_share_set(&self, manifest_hash: &str) -> Result<DaShareSet, StorageError> {
        let manifest = self.load_da_manifest(manifest_hash)?;
        let mut shares = Vec::new();
        for index in 0..manifest.encoded_share_count {
            shares.push(self.load_da_share(manifest_hash, index)?);
        }
        let share_set = DaShareSet { manifest, shares };
        share_set.verify().map_err(da_error)?;
        Ok(share_set)
    }

    pub fn commit_da_repair_record(&self, record: &DaRepairRecord) -> Result<String, StorageError> {
        let manifest = self.load_da_manifest(&record.manifest_hash)?;
        ensure_sorted_unique_u32(&record.missing_share_indices, "missing_share_indices")?;
        if record
            .missing_share_indices
            .iter()
            .any(|index| *index >= manifest.encoded_share_count)
        {
            return Err(StorageError::CorruptData(
                "DA repair record references an out-of-range share".into(),
            ));
        }
        if record.reason.is_empty() {
            return Err(StorageError::CorruptData(
                "DA repair record reason must be nonempty".into(),
            ));
        }
        let record_id = hash_canonical_json(record)?;
        write_json_atomic(&self.da_repair_record_path(&record_id), record)?;
        Ok(record_id)
    }

    pub fn load_da_repair_record(&self, record_id: &str) -> Result<DaRepairRecord, StorageError> {
        let record: DaRepairRecord = read_json(&self.da_repair_record_path(record_id))?;
        let actual_id = hash_canonical_json(&record)?;
        if actual_id != record_id {
            return Err(StorageError::CorruptData(format!(
                "DA repair record hash mismatch: expected {record_id}, got {actual_id}"
            )));
        }
        Ok(record)
    }

    pub fn load_da_repair_records(&self) -> Result<Vec<DaRepairRecord>, StorageError> {
        let mut records = Vec::new();
        for path in sorted_bin_paths(&self.root.join("da").join("repairs"))? {
            let record: DaRepairRecord = read_json(&path)?;
            let record_id = hash_canonical_json(&record)?;
            let expected_file_name = format!("{}.bin", file_safe_id(&record_id));
            if path.file_name().and_then(|name| name.to_str()) != Some(expected_file_name.as_str())
            {
                return Err(StorageError::CorruptData(
                    "DA repair record filename does not match repair record hash".into(),
                ));
            }
            records.push(record);
        }
        Ok(records)
    }

    pub fn commit_da_retention_policy(
        &self,
        config: &DaRetentionPolicyConfig,
    ) -> Result<String, StorageError> {
        validate_da_retention_policy(config)?;
        let root = hash_canonical_json(config)?;
        write_json_atomic(&self.da_retention_policy_path(), config)?;
        Ok(root)
    }

    pub fn maybe_load_da_retention_policy(
        &self,
    ) -> Result<Option<DaRetentionPolicyConfig>, StorageError> {
        let path = self.da_retention_policy_path();
        if !path.exists() {
            return Ok(None);
        }
        self.load_da_retention_policy().map(Some)
    }

    pub fn load_da_retention_policy(&self) -> Result<DaRetentionPolicyConfig, StorageError> {
        let config: DaRetentionPolicyConfig = read_json(&self.da_retention_policy_path())?;
        validate_da_retention_policy(&config)?;
        Ok(config)
    }

    pub fn da_store_roots(&self) -> Result<DaStoreRoots, StorageError> {
        let manifest_root = directory_content_root(&self.root.join("da").join("manifests"))?;
        let share_root = directory_content_root(&self.root.join("da").join("shares"))?;
        let payload_root = directory_content_root(&self.root.join("da").join("payloads"))?;
        let certificate_root = directory_content_root(&self.root.join("da").join("certificates"))?;
        let challenge_root = directory_content_root(&self.root.join("da").join("challenges"))?;
        let repair_root = directory_content_root(&self.root.join("da").join("repairs"))?;
        let index_root = directory_content_root(&self.da_indexes_path())?;
        let retention_policy_root = self
            .maybe_load_da_retention_policy()?
            .map(|config| hash_canonical_json(&config))
            .transpose()?;
        let root = hash_canonical_json(&(
            &manifest_root,
            &share_root,
            &payload_root,
            &certificate_root,
            &challenge_root,
            &repair_root,
            &index_root,
            &retention_policy_root,
        ))?;
        Ok(DaStoreRoots {
            manifest_root,
            share_root,
            payload_root,
            certificate_root,
            challenge_root,
            repair_root,
            index_root,
            retention_policy_root,
            root,
        })
    }

    pub fn commit_consensus_signing_record_if_absent(
        &self,
        record: &ConsensusSigningRecord,
    ) -> Result<Option<ConsensusSigningRecord>, StorageError> {
        let path =
            self.consensus_signing_record_path(&record.validator_id, record.domain, record.height);
        if path.exists() {
            return read_json(&path).map(Some);
        }

        match write_json_create_new(&path, record) {
            Ok(()) => Ok(None),
            Err(StorageError::Io(error)) => {
                if path.exists() {
                    read_json(&path).map(Some)
                } else {
                    Err(StorageError::Io(error))
                }
            }
            Err(error) => Err(error),
        }
    }

    pub fn maybe_load_consensus_signing_record(
        &self,
        validator_id: &str,
        domain: ValidatorSignatureDomain,
        height: u64,
    ) -> Result<Option<ConsensusSigningRecord>, StorageError> {
        let path = self.consensus_signing_record_path(validator_id, domain, height);
        if !path.exists() {
            return Ok(None);
        }
        read_json(&path).map(Some)
    }

    fn snapshot_path(&self) -> PathBuf {
        self.root.join("latest_snapshot.bin")
    }

    fn snapshot_metadata_roots_path(&self) -> PathBuf {
        self.root.join("latest_snapshot_metadata_roots.bin")
    }

    fn required_snapshot_metadata_roots_path(&self) -> PathBuf {
        self.root
            .join("latest_required_snapshot_metadata_roots.bin")
    }

    fn snapshot_sync_client_metrics_path(&self) -> PathBuf {
        self.root.join("latest_snapshot_sync_client_metrics.bin")
    }

    fn block_path(&self, height: u64) -> PathBuf {
        self.root.join("blocks").join(format!("{height}.bin"))
    }

    fn certificate_path(&self, height: u64) -> PathBuf {
        self.root.join("certificates").join(format!("{height}.bin"))
    }

    fn slashing_path(&self, validator_id: &str) -> PathBuf {
        self.root
            .join("slashings")
            .join(format!("{}.bin", file_safe_id(validator_id)))
    }

    fn validator_set_path(&self) -> PathBuf {
        self.root.join("validator_set.bin")
    }

    fn pending_validator_set_metadata_authorizations_path(&self) -> PathBuf {
        self.root
            .join("pending_validator_set_metadata_authorizations.bin")
    }

    fn validator_set_metadata_audit_path(&self) -> PathBuf {
        self.root.join("validator_set_metadata_audit.bin")
    }

    fn snapshot_import_audit_path(&self) -> PathBuf {
        self.root.join("snapshot_import_audit.bin")
    }

    fn snapshot_import_audit_config_path(&self) -> PathBuf {
        self.root.join("snapshot_import_audit_config.bin")
    }

    fn mempool_path(&self) -> PathBuf {
        self.root.join("mempool.bin")
    }

    fn consensus_signing_record_path(
        &self,
        validator_id: &str,
        domain: ValidatorSignatureDomain,
        height: u64,
    ) -> PathBuf {
        self.root
            .join("consensus_signing_records")
            .join(validator_signature_domain_path(domain))
            .join(file_safe_id(validator_id))
            .join(format!("{height}.bin"))
    }

    fn da_manifest_path(&self, manifest_hash: &str) -> PathBuf {
        self.root
            .join("da")
            .join("manifests")
            .join(format!("{}.bin", file_safe_id(manifest_hash)))
    }

    fn da_certificate_path(&self, certificate_hash: &str) -> PathBuf {
        self.root
            .join("da")
            .join("certificates")
            .join(format!("{}.bin", file_safe_id(certificate_hash)))
    }

    fn da_challenge_record_path(&self, challenge_id: &str) -> PathBuf {
        self.root
            .join("da")
            .join("challenges")
            .join(format!("{}.bin", file_safe_id(challenge_id)))
    }

    fn da_payload_path(&self, manifest_hash: &str) -> PathBuf {
        self.root
            .join("da")
            .join("payloads")
            .join(format!("{}.bin", file_safe_id(manifest_hash)))
    }

    fn da_repair_record_path(&self, record_id: &str) -> PathBuf {
        self.root
            .join("da")
            .join("repairs")
            .join(format!("{}.bin", file_safe_id(record_id)))
    }

    fn da_retention_policy_path(&self) -> PathBuf {
        self.root.join("da").join("retention_policy.bin")
    }

    fn da_share_path(&self, manifest_hash: &str, index: u32) -> PathBuf {
        self.root
            .join("da")
            .join("shares")
            .join(file_safe_id(manifest_hash))
            .join(format!("{index}.bin"))
    }

    fn da_indexes_path(&self) -> PathBuf {
        self.root.join("da").join("indexes")
    }

    fn create_da_index_dirs(&self) -> Result<(), StorageError> {
        fs::create_dir_all(self.da_indexes_path().join("manifests_by_height")).map_err(io_error)?;
        fs::create_dir_all(self.da_indexes_path().join("manifests_by_block_hash"))
            .map_err(io_error)?;
        fs::create_dir_all(self.da_indexes_path().join("certificates_by_manifest"))
            .map_err(io_error)?;
        fs::create_dir_all(self.da_indexes_path().join("certificates_by_height"))
            .map_err(io_error)?;
        fs::create_dir_all(self.da_indexes_path().join("certificates_by_block_hash"))
            .map_err(io_error)
    }

    fn da_manifests_by_height_index_path(&self, height: u64) -> PathBuf {
        self.da_indexes_path()
            .join("manifests_by_height")
            .join(format!("{height}.bin"))
    }

    fn da_manifests_by_block_hash_index_path(&self, block_hash: &str) -> PathBuf {
        self.da_indexes_path()
            .join("manifests_by_block_hash")
            .join(format!("{}.bin", file_safe_id(block_hash)))
    }

    fn da_certificates_by_manifest_index_path(&self, manifest_hash: &str) -> PathBuf {
        self.da_indexes_path()
            .join("certificates_by_manifest")
            .join(format!("{}.bin", file_safe_id(manifest_hash)))
    }

    fn da_certificates_by_height_index_path(&self, height: u64) -> PathBuf {
        self.da_indexes_path()
            .join("certificates_by_height")
            .join(format!("{height}.bin"))
    }

    fn da_certificates_by_block_hash_index_path(&self, block_hash: &str) -> PathBuf {
        self.da_indexes_path()
            .join("certificates_by_block_hash")
            .join(format!("{}.bin", file_safe_id(block_hash)))
    }
}

fn validate_da_retention_policy(config: &DaRetentionPolicyConfig) -> Result<(), StorageError> {
    if config.policies.is_empty() {
        return Err(StorageError::CorruptData(
            "DA retention policy config must include at least one policy".into(),
        ));
    }
    let mut classes = BTreeMap::new();
    for policy in &config.policies {
        if classes.insert(policy.class, ()).is_some() {
            return Err(StorageError::CorruptData(format!(
                "duplicate DA retention policy class {:?}",
                policy.class
            )));
        }
        if policy.min_retention_blocks == 0 {
            return Err(StorageError::CorruptData(format!(
                "DA retention policy {:?} must retain data for at least one block",
                policy.class
            )));
        }
        if policy.max_payload_bytes == Some(0) {
            return Err(StorageError::CorruptData(format!(
                "DA retention policy {:?} max payload bytes must be positive when set",
                policy.class
            )));
        }
        if !policy.retain_payloads && !policy.retain_all_shares {
            return Err(StorageError::CorruptData(format!(
                "DA retention policy {:?} must retain payloads or shares",
                policy.class
            )));
        }
    }
    Ok(())
}

fn da_manifest_retention_class(manifest: &DaManifest) -> DaRetentionClass {
    if manifest
        .namespace_ranges
        .iter()
        .any(|range| range.namespace.0 == "detta.snapshot")
    {
        DaRetentionClass::Checkpoint
    } else {
        DaRetentionClass::Hot
    }
}

fn ensure_sorted_unique_u32(values: &[u32], field: &str) -> Result<(), StorageError> {
    if !values.windows(2).all(|window| window[0] < window[1]) {
        return Err(StorageError::CorruptData(format!(
            "{field} must be sorted and unique"
        )));
    }
    Ok(())
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), StorageError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let bytes = encode_json(value)?;

    let tmp_path = path.with_extension("tmp");
    {
        let file = File::create(&tmp_path).map_err(io_error)?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&bytes).map_err(io_error)?;
        writer.flush().map_err(io_error)?;
    }

    let file = File::options()
        .read(true)
        .open(&tmp_path)
        .map_err(io_error)?;
    file.sync_all().map_err(io_error)?;
    fs::rename(&tmp_path, path).map_err(io_error)?;
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<T, StorageError> {
    let file = File::open(path).map_err(io_error)?;
    let encoded_len = file.metadata().map_err(io_error)?.len();
    if encoded_len > MAX_ENCODED_BYTES {
        return Err(StorageError::CorruptData(format!(
            "encoded storage value exceeds {MAX_ENCODED_BYTES} bytes"
        )));
    }
    let reader = BufReader::new(file);
    serde_json::from_reader(reader).map_err(data_error)
}

fn write_json_create_new<T: Serialize>(path: &Path, value: &T) -> Result<(), StorageError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }
    let bytes = encode_json(value)?;

    let mut file = File::options()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(&bytes).map_err(io_error)?;
    file.sync_all().map_err(io_error)
}

fn io_error(error: std::io::Error) -> StorageError {
    StorageError::Io(error.to_string())
}

fn data_error(error: serde_json::Error) -> StorageError {
    StorageError::CorruptData(error.to_string())
}

fn da_error(error: detta_da::DaError) -> StorageError {
    StorageError::CorruptData(format!("data availability error: {error:?}"))
}

fn encode_json<T: Serialize>(value: &T) -> Result<Vec<u8>, StorageError> {
    let bytes = serde_json::to_vec(value).map_err(data_error)?;
    if bytes.len() as u64 > MAX_ENCODED_BYTES {
        return Err(StorageError::CorruptData(format!(
            "encoded storage value exceeds {MAX_ENCODED_BYTES} bytes"
        )));
    }
    Ok(bytes)
}

fn hash_canonical_json<T: Serialize>(value: &T) -> Result<String, StorageError> {
    let bytes = encode_json(value)?;
    let mut hasher = Sha256::new();
    hasher.update(b"detta-storage-root");
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
    Ok(hex_lower(&hasher.finalize()))
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

fn file_safe_id(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(value.len() * 2);
    for byte in value.as_bytes() {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn validator_signature_domain_path(domain: ValidatorSignatureDomain) -> &'static str {
    match domain {
        ValidatorSignatureDomain::BlockProposal => "block_proposal",
        ValidatorSignatureDomain::Vote => "vote",
        ValidatorSignatureDomain::DaAvailabilityVote => "da_availability_vote",
        ValidatorSignatureDomain::DaShareChallenge => "da_share_challenge",
        ValidatorSignatureDomain::DaShareChallengeResponse => "da_share_challenge_response",
        ValidatorSignatureDomain::DaChallengeEvidence => "da_challenge_evidence",
        ValidatorSignatureDomain::FinalityCertificate => "finality_certificate",
        ValidatorSignatureDomain::DaAvailabilityCertificate => "da_availability_certificate",
        ValidatorSignatureDomain::ValidatorSetUpdate => "validator_set_update",
        ValidatorSignatureDomain::EquivocationEvidence => "equivocation_evidence",
        ValidatorSignatureDomain::ValidatorSetMetadataUpdate => "validator_set_metadata_update",
    }
}

fn directory_size_bytes(path: &Path) -> Result<u64, StorageError> {
    let metadata = fs::metadata(path).map_err(io_error)?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        return Ok(0);
    }

    let mut total = 0_u64;
    for entry in fs::read_dir(path).map_err(io_error)? {
        total = total.saturating_add(directory_size_bytes(&entry.map_err(io_error)?.path())?);
    }
    Ok(total)
}

fn sorted_bin_paths(path: &Path) -> Result<Vec<PathBuf>, StorageError> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(path).map_err(io_error)? {
        let path = entry.map_err(io_error)?.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("bin") {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct DirectoryFileStats {
    file_count: u64,
    total_bytes: u64,
}

fn directory_file_stats(path: &Path) -> Result<DirectoryFileStats, StorageError> {
    if !path.exists() {
        return Ok(DirectoryFileStats::default());
    }
    let metadata = fs::metadata(path).map_err(io_error)?;
    if metadata.is_file() {
        return Ok(DirectoryFileStats {
            file_count: 1,
            total_bytes: metadata.len(),
        });
    }
    if !metadata.is_dir() {
        return Ok(DirectoryFileStats::default());
    }

    let mut stats = DirectoryFileStats::default();
    for entry in fs::read_dir(path).map_err(io_error)? {
        let child = directory_file_stats(&entry.map_err(io_error)?.path())?;
        stats.file_count = stats.file_count.saturating_add(child.file_count);
        stats.total_bytes = stats.total_bytes.saturating_add(child.total_bytes);
    }
    Ok(stats)
}

fn directory_content_root(path: &Path) -> Result<String, StorageError> {
    let mut entries = Vec::new();
    collect_directory_content_roots(path, path, &mut entries)?;
    entries.sort();
    hash_canonical_json(&entries)
}

fn collect_directory_content_roots(
    root: &Path,
    path: &Path,
    entries: &mut Vec<(String, String)>,
) -> Result<(), StorageError> {
    if !path.exists() {
        return Ok(());
    }
    let metadata = fs::metadata(path).map_err(io_error)?;
    if metadata.is_file() {
        let relative_path = path
            .strip_prefix(root)
            .map_err(|error| StorageError::CorruptData(error.to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        let bytes = fs::read(path).map_err(io_error)?;
        if bytes.len() as u64 > MAX_ENCODED_BYTES {
            return Err(StorageError::CorruptData(format!(
                "encoded storage value exceeds {MAX_ENCODED_BYTES} bytes"
            )));
        }
        entries.push((relative_path, bytes_hash(&bytes)));
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(path).map_err(io_error)? {
        collect_directory_content_roots(root, &entry.map_err(io_error)?.path(), entries)?;
    }
    Ok(())
}

fn bytes_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

fn copy_directory_contents(
    source: &Path,
    destination: &Path,
    manifest: &mut StorageBackupManifest,
) -> Result<(), StorageError> {
    fs::create_dir_all(destination).map_err(io_error)?;
    for entry in fs::read_dir(source).map_err(io_error)? {
        let source_path = entry.map_err(io_error)?.path();
        let destination_path = destination.join(
            source_path
                .file_name()
                .ok_or_else(|| StorageError::Io("backup path has no filename".into()))?,
        );
        let metadata = fs::metadata(&source_path).map_err(io_error)?;
        if metadata.is_dir() {
            copy_directory_contents(&source_path, &destination_path, manifest)?;
        } else if metadata.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(io_error)?;
            }
            let bytes = fs::copy(&source_path, &destination_path).map_err(io_error)?;
            manifest.file_count += 1;
            manifest.total_bytes = manifest.total_bytes.saturating_add(bytes);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_consensus::{EquivocationEvidence, SlashingEvidence};
    use detta_core::{Argument, Method, Transaction, ValidatorNode};
    use detta_da::{
        DaAvailabilityVote, DaChallengeEvidence, DaNamespace, DaNamespaceSection, DaPayload,
        DaRecord, DaShareChallenge, DaShareChallengeResponse,
    };
    use detta_protocol::{
        ProtocolMessage, ValidatorPublicKey, ValidatorSetMetadataUpdate, ValidatorSignatureDomain,
    };
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("detta-storage-{name}-{nonce}"))
    }

    fn seeded_state() -> DeTTaState {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_token(
                "TokenA",
                "USDC",
                vec![("Alice".into(), 100), ("Bob".into(), 50)],
            )
            .unwrap();
        state
    }

    fn transfer_tx() -> Transaction {
        Transaction {
            chain_id: "detta-local".into(),
            tx_hash: "tx1".into(),
            sender: "Alice".into(),
            nonce: 1,
            valid_until_height: None,
            target: "TokenA".into(),
            method: Method::Transfer,
            args: vec![
                Argument::Principal("Bob".into()),
                Argument::Asset("USDC".into()),
                Argument::Amount(10),
            ],
            signature_ok: true,
            budget: 1_000_000,
        }
    }

    fn da_payload() -> DaPayload {
        DaPayload::new(
            "detta-local",
            1,
            "previous-block",
            vec![DaNamespaceSection::new(
                DaNamespace::new("detta.tx").unwrap(),
                vec![DaRecord::SignedTransaction(transfer_tx())],
            )
            .unwrap()],
        )
        .unwrap()
    }

    fn snapshot_da_payload() -> DaPayload {
        DaPayload::new(
            "detta-local",
            2,
            "previous-snapshot-block",
            vec![DaNamespaceSection::new(
                DaNamespace::new("detta.snapshot").unwrap(),
                vec![DaRecord::SnapshotChunkManifest {
                    snapshot_root: "snapshot-root".into(),
                    snapshot_hash: "snapshot-hash".into(),
                    metadata_roots: BTreeMap::new(),
                    chunk_size: 64,
                    total_bytes: 64,
                    chunk_count: 1,
                    chunk_hashes: vec!["chunk-hash".into()],
                    chunk_root: "chunk-root".into(),
                }],
            )
            .unwrap()],
        )
        .unwrap()
    }

    #[test]
    fn persists_and_verifies_snapshot() {
        let dir = temp_dir("snapshot");
        let storage = FileStorage::open(&dir).unwrap();
        let state = seeded_state();
        let snapshot = state.snapshot();

        storage.commit_snapshot(&snapshot).unwrap();
        let loaded = storage.load_snapshot().unwrap();

        assert_eq!(loaded.global_state_root, snapshot.global_state_root);
        assert_eq!(loaded.storage_root, snapshot.storage_root);
        assert_eq!(loaded.policy_root, snapshot.policy_root);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persists_snapshot_metadata_roots() {
        let dir = temp_dir("snapshot-metadata-roots");
        let storage = FileStorage::open(&dir).unwrap();
        let mut roots = BTreeMap::new();
        roots.insert(
            "validator_set_metadata_audit_root".into(),
            "audit-root-1".into(),
        );
        roots.insert(
            "state_sync_client_metrics_root".into(),
            "metrics-root-1".into(),
        );

        assert_eq!(
            storage.load_snapshot_metadata_roots().unwrap(),
            BTreeMap::new()
        );
        assert_eq!(
            storage.load_required_snapshot_metadata_roots().unwrap(),
            BTreeMap::new()
        );
        let empty_required_roots_root = storage.required_snapshot_metadata_roots_root().unwrap();
        storage.commit_snapshot_metadata_roots(&roots).unwrap();
        storage
            .commit_required_snapshot_metadata_roots(&roots)
            .unwrap();
        let required_roots_root = storage.required_snapshot_metadata_roots_root().unwrap();
        assert_ne!(required_roots_root, empty_required_roots_root);
        assert_eq!(storage.load_snapshot_metadata_roots().unwrap(), roots);
        assert_eq!(
            storage.load_required_snapshot_metadata_roots().unwrap(),
            roots
        );
        let mut same_roots_different_insertion_order = BTreeMap::new();
        same_roots_different_insertion_order.insert(
            "state_sync_client_metrics_root".into(),
            "metrics-root-1".into(),
        );
        same_roots_different_insertion_order.insert(
            "validator_set_metadata_audit_root".into(),
            "audit-root-1".into(),
        );
        storage
            .commit_required_snapshot_metadata_roots(&same_roots_different_insertion_order)
            .unwrap();
        assert_eq!(
            storage.required_snapshot_metadata_roots_root().unwrap(),
            required_roots_root
        );
        assert_eq!(
            FileStorage::open(&dir)
                .unwrap()
                .load_snapshot_metadata_roots()
                .unwrap(),
            roots
        );
        assert_eq!(
            FileStorage::open(&dir)
                .unwrap()
                .load_required_snapshot_metadata_roots()
                .unwrap(),
            roots
        );
        assert_eq!(
            FileStorage::open(&dir)
                .unwrap()
                .required_snapshot_metadata_roots_root()
                .unwrap(),
            required_roots_root
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
    struct TestSnapshotSyncMetrics {
        requests_sent: u32,
        metadata_roots_verified: bool,
    }

    #[test]
    fn persists_snapshot_sync_client_metrics() {
        let dir = temp_dir("snapshot-sync-client-metrics");
        let storage = FileStorage::open(&dir).unwrap();
        let metrics = TestSnapshotSyncMetrics {
            requests_sent: 3,
            metadata_roots_verified: true,
        };

        assert_eq!(
            storage
                .load_snapshot_sync_client_metrics::<TestSnapshotSyncMetrics>()
                .unwrap(),
            None
        );
        assert_eq!(
            storage
                .snapshot_sync_client_metrics_root::<TestSnapshotSyncMetrics>()
                .unwrap(),
            None
        );
        storage
            .commit_snapshot_sync_client_metrics(&metrics)
            .unwrap();
        let metrics_root = storage
            .snapshot_sync_client_metrics_root::<TestSnapshotSyncMetrics>()
            .unwrap()
            .unwrap();
        assert_eq!(
            metrics_root,
            FileStorage::snapshot_sync_client_metrics_root_for(&metrics).unwrap()
        );
        assert_eq!(
            FileStorage::open(&dir)
                .unwrap()
                .load_snapshot_sync_client_metrics::<TestSnapshotSyncMetrics>()
                .unwrap(),
            Some(metrics)
        );
        assert_eq!(
            FileStorage::open(&dir)
                .unwrap()
                .snapshot_sync_client_metrics_root::<TestSnapshotSyncMetrics>()
                .unwrap(),
            Some(metrics_root)
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_snapshot_with_bad_root() {
        let dir = temp_dir("bad-snapshot");
        let storage = FileStorage::open(&dir).unwrap();
        let mut snapshot = seeded_state().snapshot();
        snapshot.storage_root = "bad-root".into();

        let error = storage.commit_snapshot(&snapshot).unwrap_err();

        assert!(matches!(
            error,
            StorageError::InvalidSnapshot(SnapshotError::StorageRootMismatch)
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persists_and_loads_block() {
        let dir = temp_dir("block");
        let storage = FileStorage::open(&dir).unwrap();
        let proposer = ValidatorNode::new("validator-1", seeded_state());
        let block = proposer.propose_block(1, vec![transfer_tx()], 1_000);

        assert_eq!(storage.maybe_load_block(1).unwrap(), None);
        storage.commit_block(&block).unwrap();
        let loaded = storage.load_block(1).unwrap();
        let maybe_loaded = storage.maybe_load_block(1).unwrap();

        assert_eq!(loaded.block_hash(), block.block_hash());
        assert_eq!(maybe_loaded.unwrap().block_hash(), block.block_hash());
        assert_eq!(loaded.header.storage_root, block.header.storage_root);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persists_and_loads_da_share_set() {
        let dir = temp_dir("da-share-set");
        let backup_dir = temp_dir("da-share-set-backup");
        let restore_dir = temp_dir("da-share-set-restore");
        let storage = FileStorage::open(&dir).unwrap();
        let share_set = DaShareSet::from_payload(&da_payload(), "block-1", 64).unwrap();
        let manifest_hash = share_set.manifest.manifest_hash().unwrap();

        assert_eq!(
            storage.maybe_load_da_manifest(&manifest_hash).unwrap(),
            None
        );
        assert_eq!(
            storage.commit_da_share_set(&share_set).unwrap(),
            manifest_hash
        );

        let loaded_manifest = storage
            .maybe_load_da_manifest(&manifest_hash)
            .unwrap()
            .unwrap();
        assert_eq!(loaded_manifest, share_set.manifest);
        let loaded_share_set = storage.load_da_share_set(&manifest_hash).unwrap();
        assert_eq!(loaded_share_set.manifest, share_set.manifest);
        assert_eq!(loaded_share_set.shares, share_set.shares);
        assert_eq!(
            loaded_share_set.reconstruct_payload().unwrap(),
            da_payload().canonicalized()
        );
        assert_eq!(
            storage.maybe_load_da_payload(&manifest_hash).unwrap(),
            Some(da_payload().canonicalized())
        );
        assert_eq!(
            storage.load_da_payload(&manifest_hash).unwrap(),
            da_payload().canonicalized()
        );
        assert_eq!(
            storage.load_da_manifest_index_by_height(1).unwrap(),
            vec![DaManifestIndexEntry {
                manifest_hash: manifest_hash.clone(),
                chain_id: share_set.manifest.chain_id.clone(),
                height: share_set.manifest.height,
                block_hash: share_set.manifest.block_hash.clone(),
                payload_hash: share_set.manifest.payload_hash.clone(),
                share_root: share_set.manifest.share_root.clone(),
            }]
        );
        assert_eq!(
            storage
                .load_da_manifest_index_by_block_hash("block-1")
                .unwrap(),
            storage.load_da_manifest_index_by_height(1).unwrap()
        );
        assert!(storage
            .load_da_manifest_index_by_block_hash("missing-block")
            .unwrap()
            .is_empty());

        let manifest = storage.backup_to(&backup_dir).unwrap();
        assert!(manifest.file_count > share_set.shares.len());
        let restored = FileStorage::restore_from_backup(&backup_dir, &restore_dir).unwrap();
        assert_eq!(
            restored.load_da_share_set(&manifest_hash).unwrap().shares,
            share_set.shares
        );
        assert_eq!(
            restored.load_da_manifest_index_by_height(1).unwrap(),
            storage.load_da_manifest_index_by_height(1).unwrap()
        );
        assert_eq!(
            restored.load_da_payload(&manifest_hash).unwrap(),
            da_payload().canonicalized()
        );

        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(backup_dir).unwrap();
        fs::remove_dir_all(restore_dir).unwrap();
    }

    #[test]
    fn persists_and_loads_da_challenge_record() {
        let dir = temp_dir("da-challenge-record");
        let storage = FileStorage::open(&dir).unwrap();
        let share_set = DaShareSet::from_payload(&da_payload(), "block-1", 64).unwrap();
        let vote = DaAvailabilityVote::from_manifest_with_custody(
            &share_set.manifest,
            "validator-1",
            [0],
            [],
        )
        .unwrap();
        let challenge =
            DaShareChallenge::from_availability_vote(&vote, "validator-2", 0, 9).unwrap();
        let mut invalid_share = share_set.shares[0].clone();
        invalid_share.bytes[0] ^= 0x01;
        let response = DaShareChallengeResponse::from_share(&challenge, invalid_share).unwrap();
        let evidence = DaChallengeEvidence::invalid_response(
            &challenge,
            &response,
            &share_set.manifest,
            "validator-2",
            8,
        )
        .unwrap();
        let record = DaChallengeRecord {
            challenge,
            response: Some(response),
            evidence: Some(evidence),
        };
        let challenge_id = record.challenge_id().unwrap();

        assert_eq!(
            storage
                .maybe_load_da_challenge_record(&challenge_id)
                .unwrap(),
            None
        );
        assert_eq!(
            storage.commit_da_challenge_record(&record).unwrap(),
            challenge_id
        );
        assert_eq!(
            storage
                .maybe_load_da_challenge_record(&challenge_id)
                .unwrap(),
            Some(record.clone())
        );
        assert_eq!(
            storage.load_da_challenge_record(&challenge_id).unwrap(),
            record
        );
        let stats = storage.da_storage_stats().unwrap();
        assert_eq!(stats.challenge_count, 1);
        assert!(stats.challenge_bytes > 0);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persists_da_repair_records_retention_policy_and_store_roots() {
        let dir = temp_dir("da-store-roots");
        let storage = FileStorage::open(&dir).unwrap();
        let share_set = DaShareSet::from_payload(&da_payload(), "block-7", 64).unwrap();
        let manifest_hash = storage.commit_da_share_set(&share_set).unwrap();
        let empty_roots = storage.da_store_roots().unwrap();
        assert_ne!(empty_roots.manifest_root, empty_roots.share_root);

        let repair_record = DaRepairRecord {
            manifest_hash: manifest_hash.clone(),
            missing_share_indices: vec![0],
            recorded_at_height: 7,
            reason: "test repair".into(),
            completed: false,
        };
        let repair_id = storage.commit_da_repair_record(&repair_record).unwrap();
        assert_eq!(
            storage.load_da_repair_record(&repair_id).unwrap(),
            repair_record
        );
        assert!(matches!(
            storage.commit_da_repair_record(&DaRepairRecord {
                manifest_hash: manifest_hash.clone(),
                missing_share_indices: vec![1, 1],
                recorded_at_height: 7,
                reason: "duplicate".into(),
                completed: false,
            }),
            Err(StorageError::CorruptData(_))
        ));

        let retention_policy = DaRetentionPolicyConfig {
            policies: vec![
                DaRetentionPolicy {
                    class: DaRetentionClass::Hot,
                    retain_payloads: true,
                    retain_all_shares: true,
                    min_retention_blocks: 128,
                    max_payload_bytes: Some(1024 * 1024),
                },
                DaRetentionPolicy {
                    class: DaRetentionClass::Checkpoint,
                    retain_payloads: true,
                    retain_all_shares: false,
                    min_retention_blocks: 1024,
                    max_payload_bytes: None,
                },
            ],
        };
        let retention_root = storage
            .commit_da_retention_policy(&retention_policy)
            .unwrap();
        assert_eq!(
            storage.maybe_load_da_retention_policy().unwrap(),
            Some(retention_policy.clone())
        );
        assert_eq!(
            storage.load_da_retention_policy().unwrap(),
            retention_policy
        );

        let roots = storage.da_store_roots().unwrap();
        assert_ne!(roots.root, empty_roots.root);
        assert_eq!(roots.retention_policy_root, Some(retention_root.clone()));
        assert_ne!(roots.repair_root, empty_roots.repair_root);
        let stats = storage.da_storage_stats().unwrap();
        assert_eq!(stats.retention_policy_root, Some(retention_root));
        assert_eq!(stats.retention_policy, Some(retention_policy));
        assert!(stats.retention_policy_bytes > 0);
        assert_eq!(stats.payload_count, 1);
        assert_eq!(stats.repair_record_count, 1);
        assert!(stats.payload_bytes > 0);
        assert!(stats.repair_record_bytes > 0);
        assert!(stats.index_file_count > 0);
        assert!(stats.index_bytes > 0);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn production_da_retention_policy_defaults_are_stable_and_valid() {
        let policy = DaRetentionPolicyConfig::production_default();

        policy.validate().unwrap();
        assert_eq!(
            policy.policies,
            vec![
                DaRetentionPolicy {
                    class: DaRetentionClass::Hot,
                    retain_payloads: true,
                    retain_all_shares: true,
                    min_retention_blocks: DA_V1_VALIDATOR_MIN_RETENTION_BLOCKS,
                    max_payload_bytes: Some(MAX_ENCODED_BYTES),
                },
                DaRetentionPolicy {
                    class: DaRetentionClass::Warm,
                    retain_payloads: true,
                    retain_all_shares: false,
                    min_retention_blocks: DA_V1_VALIDATOR_MIN_RETENTION_BLOCKS * 4,
                    max_payload_bytes: Some(MAX_ENCODED_BYTES),
                },
                DaRetentionPolicy {
                    class: DaRetentionClass::Cold,
                    retain_payloads: true,
                    retain_all_shares: true,
                    min_retention_blocks: DA_V1_ARCHIVE_MIN_RETENTION_BLOCKS,
                    max_payload_bytes: None,
                },
                DaRetentionPolicy {
                    class: DaRetentionClass::Checkpoint,
                    retain_payloads: true,
                    retain_all_shares: true,
                    min_retention_blocks: DA_V1_ARCHIVE_MIN_RETENTION_BLOCKS,
                    max_payload_bytes: None,
                },
            ]
        );
    }

    #[test]
    fn da_retention_audit_reports_active_expired_and_unsatisfied_manifests() {
        let dir = temp_dir("da-retention-audit");
        let storage = FileStorage::open(&dir).unwrap();
        let hot_policy = DaRetentionPolicy {
            class: DaRetentionClass::Hot,
            retain_payloads: true,
            retain_all_shares: true,
            min_retention_blocks: 10,
            max_payload_bytes: Some(1024 * 1024),
        };
        let checkpoint_policy = DaRetentionPolicy {
            class: DaRetentionClass::Checkpoint,
            retain_payloads: true,
            retain_all_shares: true,
            min_retention_blocks: 2,
            max_payload_bytes: None,
        };
        let policy = DaRetentionPolicyConfig {
            policies: vec![hot_policy.clone(), checkpoint_policy.clone()],
        };
        let policy_root = storage.commit_da_retention_policy(&policy).unwrap();

        let hot_share_set = DaShareSet::from_payload(&da_payload(), "block-hot", 64).unwrap();
        let hot_manifest_hash = storage.commit_da_manifest(&hot_share_set.manifest).unwrap();
        storage
            .commit_da_payload(&hot_manifest_hash, &da_payload())
            .unwrap();

        let checkpoint_share_set =
            DaShareSet::from_payload(&snapshot_da_payload(), "block-checkpoint", 64).unwrap();
        let checkpoint_manifest_hash = storage.commit_da_share_set(&checkpoint_share_set).unwrap();

        let report = storage.da_retention_audit(5).unwrap();

        assert_eq!(report.current_height, 5);
        assert_eq!(report.policy_root, Some(policy_root));
        assert_eq!(report.policy, Some(policy));
        assert_eq!(report.manifest_count, 2);
        assert_eq!(report.active_manifest_count, 1);
        assert_eq!(report.expired_manifest_count, 1);
        assert_eq!(report.missing_policy_class_count, 0);
        assert_eq!(report.unsatisfied_manifest_count, 1);

        let hot_entry = report
            .entries
            .iter()
            .find(|entry| entry.manifest_hash == hot_manifest_hash)
            .unwrap();
        assert_eq!(hot_entry.class, DaRetentionClass::Hot);
        assert_eq!(hot_entry.min_retention_blocks, Some(10));
        assert_eq!(hot_entry.retention_expires_at_height, Some(11));
        assert!(!hot_entry.expired);
        assert!(hot_entry.payload_present);
        assert_eq!(hot_entry.stored_share_count, 0);
        assert_eq!(
            hot_entry.missing_share_count,
            hot_entry.expected_share_count
        );
        assert!(hot_entry.payload_retention_satisfied);
        assert!(!hot_entry.share_retention_satisfied);
        assert!(!hot_entry.retention_satisfied);

        let checkpoint_entry = report
            .entries
            .iter()
            .find(|entry| entry.manifest_hash == checkpoint_manifest_hash)
            .unwrap();
        assert_eq!(checkpoint_entry.class, DaRetentionClass::Checkpoint);
        assert_eq!(checkpoint_entry.min_retention_blocks, Some(2));
        assert_eq!(checkpoint_entry.retention_expires_at_height, Some(4));
        assert!(checkpoint_entry.expired);
        assert_eq!(
            checkpoint_entry.stored_share_count,
            checkpoint_entry.expected_share_count
        );
        assert!(checkpoint_entry.retention_satisfied);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn da_retention_policy_rejects_zero_windows_and_zero_payload_limits() {
        let zero_window = DaRetentionPolicyConfig {
            policies: vec![DaRetentionPolicy {
                class: DaRetentionClass::Hot,
                retain_payloads: true,
                retain_all_shares: true,
                min_retention_blocks: 0,
                max_payload_bytes: Some(MAX_ENCODED_BYTES),
            }],
        };
        assert!(matches!(
            zero_window.validate(),
            Err(StorageError::CorruptData(message))
                if message.contains("must retain data for at least one block")
        ));

        let zero_payload_limit = DaRetentionPolicyConfig {
            policies: vec![DaRetentionPolicy {
                class: DaRetentionClass::Hot,
                retain_payloads: true,
                retain_all_shares: true,
                min_retention_blocks: 1,
                max_payload_bytes: Some(0),
            }],
        };
        assert!(matches!(
            zero_payload_limit.validate(),
            Err(StorageError::CorruptData(message))
                if message.contains("max payload bytes must be positive")
        ));
    }

    #[test]
    fn reports_da_storage_stats_and_missing_shares() {
        let dir = temp_dir("da-storage-stats");
        let storage = FileStorage::open(&dir).unwrap();
        let share_set = DaShareSet::from_payload(&da_payload(), "block-7", 64).unwrap();
        let manifest_hash = storage.commit_da_share_set(&share_set).unwrap();

        let stats = storage.da_storage_stats().unwrap();
        assert_eq!(stats.manifest_count, 1);
        assert_eq!(
            stats.expected_share_count,
            share_set.manifest.encoded_share_count as u64
        );
        assert_eq!(stats.stored_share_count, share_set.shares.len() as u64);
        assert_eq!(stats.missing_share_count, 0);
        assert_eq!(stats.payload_count, 1);
        assert_eq!(stats.certificate_count, 0);
        assert_eq!(stats.challenge_count, 0);
        assert_eq!(stats.repair_record_count, 0);
        assert_eq!(stats.index_file_count, 2);
        assert!(stats.manifest_bytes > 0);
        assert!(stats.share_bytes > 0);
        assert!(stats.payload_bytes > 0);
        assert_eq!(stats.certificate_bytes, 0);
        assert_eq!(stats.challenge_bytes, 0);
        assert_eq!(stats.repair_record_bytes, 0);
        assert!(stats.index_bytes > 0);
        assert_eq!(stats.retention_policy_root, None);
        assert_eq!(stats.retention_policy_bytes, 0);
        assert_eq!(stats.retention_policy, None);
        assert_eq!(
            stats.total_bytes,
            stats.manifest_bytes
                + stats.share_bytes
                + stats.payload_bytes
                + stats.certificate_bytes
                + stats.challenge_bytes
                + stats.repair_record_bytes
                + stats.index_bytes
                + stats.retention_policy_bytes
        );

        fs::remove_file(storage.da_share_path(&manifest_hash, 0)).unwrap();
        let stats = storage.da_storage_stats().unwrap();
        assert_eq!(stats.stored_share_count, share_set.shares.len() as u64 - 1);
        assert_eq!(stats.missing_share_count, 1);

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persists_and_loads_da_certificate() {
        let dir = temp_dir("da-certificate");
        let storage = FileStorage::open(&dir).unwrap();
        let share_set = DaShareSet::from_payload(&da_payload(), "block-7", 64).unwrap();
        let manifest_hash = storage.commit_da_share_set(&share_set).unwrap();
        let roots_before_certificate = storage.da_store_roots().unwrap();
        let certificate = DaAvailabilityCertificate::from_manifest(
            &share_set.manifest,
            vec!["validator-1".into(), "validator-2".into()],
        )
        .unwrap();
        let certificate_hash = certificate.certificate_hash().unwrap();

        assert_eq!(
            storage.maybe_load_da_certificate(&certificate_hash),
            Ok(None)
        );
        assert_eq!(
            storage.commit_da_certificate(&certificate).unwrap(),
            certificate_hash
        );
        assert_eq!(
            storage
                .maybe_load_da_certificate(&certificate_hash)
                .unwrap()
                .unwrap(),
            certificate
        );
        assert_eq!(
            storage.load_da_certificate(&certificate_hash).unwrap(),
            certificate
        );
        let certificate_index_entry = DaCertificateIndexEntry {
            certificate_hash: certificate_hash.clone(),
            chain_id: certificate.chain_id.clone(),
            height: certificate.height,
            block_hash: certificate.block_hash.clone(),
            manifest_hash: certificate.manifest_hash.clone(),
            share_root: certificate.share_root.clone(),
        };
        assert_eq!(
            storage
                .load_da_certificate_index_by_manifest_hash(&manifest_hash)
                .unwrap(),
            vec![certificate_index_entry.clone()]
        );
        assert_eq!(
            storage.load_da_certificate_index_by_height(1).unwrap(),
            vec![certificate_index_entry.clone()]
        );
        assert_eq!(
            storage
                .load_da_certificate_index_by_block_hash("block-7")
                .unwrap(),
            vec![certificate_index_entry]
        );
        let reopened = FileStorage::open(&dir).unwrap();
        assert_eq!(
            reopened
                .load_da_certificate_index_by_manifest_hash(&manifest_hash)
                .unwrap(),
            storage
                .load_da_certificate_index_by_manifest_hash(&manifest_hash)
                .unwrap()
        );
        fs::remove_dir_all(storage.da_indexes_path()).unwrap();
        storage.rebuild_da_indexes().unwrap();
        assert_eq!(
            storage
                .load_da_certificate_index_by_manifest_hash(&manifest_hash)
                .unwrap(),
            reopened
                .load_da_certificate_index_by_manifest_hash(&manifest_hash)
                .unwrap()
        );
        let stats = storage.da_storage_stats().unwrap();
        assert_eq!(stats.certificate_count, 1);
        assert!(stats.certificate_bytes > 0);
        assert_eq!(stats.index_file_count, 5);
        assert!(stats.index_bytes > 0);
        let roots_after_certificate = storage.da_store_roots().unwrap();
        assert_ne!(
            roots_after_certificate.certificate_root,
            roots_before_certificate.certificate_root
        );
        assert_ne!(
            roots_after_certificate.index_root,
            roots_before_certificate.index_root
        );

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_corrupt_da_index_entries() {
        let dir = temp_dir("da-corrupt-index");
        let storage = FileStorage::open(&dir).unwrap();
        let share_set = DaShareSet::from_payload(&da_payload(), "block-7", 64).unwrap();
        storage.commit_da_share_set(&share_set).unwrap();
        let mut entries = storage.load_da_manifest_index_by_height(1).unwrap();
        entries[0].payload_hash = "wrong-payload-hash".into();
        write_json_atomic(&storage.da_manifests_by_height_index_path(1), &entries).unwrap();

        assert!(matches!(
            storage.load_da_manifest_index_by_height(1),
            Err(StorageError::CorruptData(_))
        ));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_corrupt_da_share_on_load() {
        let dir = temp_dir("da-corrupt-share");
        let storage = FileStorage::open(&dir).unwrap();
        let share_set = DaShareSet::from_payload(&da_payload(), "block-1", 64).unwrap();
        let manifest_hash = storage.commit_da_share_set(&share_set).unwrap();
        let mut corrupt = storage.load_da_share(&manifest_hash, 0).unwrap();
        corrupt.bytes[0] ^= 0x01;
        write_json_atomic(&storage.da_share_path(&manifest_hash, 0), &corrupt).unwrap();

        assert!(matches!(
            storage.load_da_share(&manifest_hash, 0),
            Err(StorageError::CorruptData(_))
        ));
        assert!(matches!(
            storage.load_da_share_set(&manifest_hash),
            Err(StorageError::CorruptData(_))
        ));

        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn indexes_transactions_and_receipts_from_durable_blocks() {
        let dir = temp_dir("block-history-index");
        let storage = FileStorage::open(&dir).unwrap();
        let proposer = ValidatorNode::new("validator-1", seeded_state());
        let block = proposer.propose_block(1, vec![transfer_tx()], 1_000);

        storage.commit_block(&block).unwrap();

        let blocks = storage.load_blocks().unwrap();
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block_hash(), block.block_hash());
        assert_eq!(
            storage.find_transaction("tx1").unwrap(),
            Some(block.transactions[0].clone())
        );
        assert_eq!(
            storage.find_receipt("tx1").unwrap(),
            Some(block.receipts[0].clone())
        );
        assert_eq!(storage.find_transaction("missing").unwrap(), None);
        assert_eq!(storage.find_receipt("missing").unwrap(), None);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persists_and_loads_finality_certificate() {
        let dir = temp_dir("certificate");
        let storage = FileStorage::open(&dir).unwrap();
        let certificate = FinalityCertificate {
            height: 7,
            block_hash: "block-hash-7".into(),
            signers: vec!["validator-1".into(), "validator-2".into()],
        };

        assert_eq!(storage.maybe_load_finality_certificate(7).unwrap(), None);
        storage.commit_finality_certificate(&certificate).unwrap();
        let loaded = storage.load_finality_certificate(7).unwrap();
        let maybe_loaded = storage.maybe_load_finality_certificate(7).unwrap();

        assert_eq!(loaded, certificate);
        assert_eq!(maybe_loaded, Some(certificate));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persists_and_loads_slashing_record() {
        let dir = temp_dir("slashing");
        let storage = FileStorage::open(&dir).unwrap();
        let record = SlashingRecord {
            validator_id: "validator/1".into(),
            slashed_at_height: 9,
            evidence: SlashingEvidence::Equivocation(EquivocationEvidence {
                validator_id: "validator/1".into(),
                height: 9,
                first_block_hash: "block-a".into(),
                second_block_hash: "block-b".into(),
            }),
        };

        storage.commit_slashing_record(&record).unwrap();
        let loaded = storage.load_slashing_record("validator/1").unwrap();
        let maybe_loaded = storage.maybe_load_slashing_record("validator/1").unwrap();

        assert_eq!(loaded, record);
        assert_eq!(maybe_loaded, Some(record));
        assert_eq!(storage.load_slashing_records().unwrap().len(), 1);
        assert_eq!(
            storage.maybe_load_slashing_record("validator/2").unwrap(),
            None
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persists_and_optionally_loads_validator_set_metadata() {
        let dir = temp_dir("validator-set");
        let storage = FileStorage::open(&dir).unwrap();
        let metadata = ValidatorSetMetadata {
            network_id: "detta-testnet".into(),
            chain_id: "detta-local".into(),
            validators: vec![
                ValidatorPublicKey {
                    validator_id: "validator-1".into(),
                    key_id: "consensus-key-1".into(),
                    public_key_hex: "aa".repeat(32),
                },
                ValidatorPublicKey {
                    validator_id: "validator-2".into(),
                    key_id: "consensus-key-1".into(),
                    public_key_hex: "bb".repeat(32),
                },
            ],
            applied_updates: vec!["genesis-validator-set".into()],
        };

        assert_eq!(storage.maybe_load_validator_set_metadata().unwrap(), None);
        storage.commit_validator_set_metadata(&metadata).unwrap();

        assert_eq!(storage.load_validator_set_metadata().unwrap(), metadata);
        assert!(storage
            .maybe_load_validator_set_metadata()
            .unwrap()
            .is_some());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persists_pending_validator_set_metadata_authorizations() {
        let dir = temp_dir("pending-validator-set-authorizations");
        let storage = FileStorage::open(&dir).unwrap();
        let update = ValidatorSetMetadataUpdate {
            update_id: "validator-set-update-1".into(),
            add_validators: vec![ValidatorPublicKey {
                validator_id: "validator-3".into(),
                key_id: "consensus-key-1".into(),
                public_key_hex: "cc".repeat(32),
            }],
            remove_validators: vec![],
            expires_at_height: None,
        };
        let authorization = SignedValidatorMessage {
            signer: "validator-1".into(),
            key_id: "consensus-key-1".into(),
            network_id: "detta-testnet".into(),
            chain_id: "detta-local".into(),
            domain: ValidatorSignatureDomain::ValidatorSetMetadataUpdate,
            message: Box::new(ProtocolMessage::ValidatorSetMetadataUpdate(update)),
            signature_hex: "aa".repeat(64),
        };

        assert_eq!(
            storage
                .load_pending_validator_set_metadata_authorizations()
                .unwrap(),
            vec![]
        );
        storage
            .commit_pending_validator_set_metadata_authorizations(std::slice::from_ref(
                &authorization,
            ))
            .unwrap();

        assert_eq!(
            storage
                .load_pending_validator_set_metadata_authorizations()
                .unwrap(),
            vec![authorization]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn appends_validator_set_metadata_audit_records() {
        let dir = temp_dir("validator-set-audit");
        let storage = FileStorage::open(&dir).unwrap();
        let applied = ValidatorSetMetadataAuditRecord {
            update_id: "validator-set-update-1".into(),
            outcome: ValidatorSetMetadataAuditOutcome::Applied,
            height: 3,
            signers: vec!["validator-1".into(), "validator-2".into()],
            reason: "applied".into(),
        };
        let rejected = ValidatorSetMetadataAuditRecord {
            update_id: "validator-set-update-2".into(),
            outcome: ValidatorSetMetadataAuditOutcome::Rejected,
            height: 4,
            signers: vec!["validator-3".into()],
            reason: "expired".into(),
        };
        let pruned = ValidatorSetMetadataAuditRecord {
            update_id: "validator-set-update-3".into(),
            outcome: ValidatorSetMetadataAuditOutcome::Pruned,
            height: 5,
            signers: vec!["validator-4".into()],
            reason: "retained".into(),
        };

        assert_eq!(
            storage.load_validator_set_metadata_audit_records().unwrap(),
            vec![]
        );
        let empty_root = storage.validator_set_metadata_audit_root().unwrap();
        storage
            .append_validator_set_metadata_audit_record(applied.clone())
            .unwrap();
        storage
            .append_validator_set_metadata_audit_record(rejected.clone())
            .unwrap();

        assert_eq!(
            storage.load_validator_set_metadata_audit_records().unwrap(),
            vec![applied, rejected.clone()]
        );
        let populated_root = storage.validator_set_metadata_audit_root().unwrap();
        assert_ne!(populated_root, empty_root);
        assert_eq!(
            FileStorage::open(&dir)
                .unwrap()
                .validator_set_metadata_audit_root()
                .unwrap(),
            populated_root
        );
        storage
            .append_validator_set_metadata_audit_record_with_retention(pruned.clone(), 2)
            .unwrap();

        assert_eq!(
            storage.load_validator_set_metadata_audit_records().unwrap(),
            vec![rejected.clone(), pruned.clone()]
        );
        assert_eq!(
            storage
                .load_validator_set_metadata_audit_records_page(1, 1)
                .unwrap(),
            vec![pruned]
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn appends_snapshot_import_audit_records() {
        let dir = temp_dir("snapshot-import-audit");
        let storage = FileStorage::open(&dir).unwrap();
        let first = SnapshotImportAuditRecord {
            snapshot_root: "snapshot-root-1".into(),
            manifest_hash: "manifest-hash-1".into(),
            required_metadata_roots_root: "required-roots-root-1".into(),
            required_metadata_roots_count: 2,
            manifest_metadata_roots_count: 3,
            chunk_count: 4,
            metadata_roots_verified: true,
            da_manifest_hash: None,
            da_certificate_hash: None,
        };
        let second = SnapshotImportAuditRecord {
            snapshot_root: "snapshot-root-2".into(),
            manifest_hash: "manifest-hash-2".into(),
            required_metadata_roots_root: "required-roots-root-2".into(),
            required_metadata_roots_count: 1,
            manifest_metadata_roots_count: 2,
            chunk_count: 3,
            metadata_roots_verified: true,
            da_manifest_hash: None,
            da_certificate_hash: None,
        };
        let third = SnapshotImportAuditRecord {
            snapshot_root: "snapshot-root-3".into(),
            manifest_hash: "manifest-hash-3".into(),
            required_metadata_roots_root: "required-roots-root-3".into(),
            required_metadata_roots_count: 3,
            manifest_metadata_roots_count: 4,
            chunk_count: 5,
            metadata_roots_verified: true,
            da_manifest_hash: Some("da-manifest-hash-3".into()),
            da_certificate_hash: Some("da-certificate-hash-3".into()),
        };

        assert_eq!(
            storage.load_snapshot_import_audit_records().unwrap(),
            vec![]
        );
        let empty_root = storage.snapshot_import_audit_root().unwrap();
        storage
            .append_snapshot_import_audit_record(first.clone())
            .unwrap();
        storage
            .append_snapshot_import_audit_record(second.clone())
            .unwrap();

        assert_eq!(
            storage.load_snapshot_import_audit_records().unwrap(),
            vec![first.clone(), second.clone()]
        );
        assert_eq!(
            storage
                .load_snapshot_import_audit_records_page(1, 1)
                .unwrap(),
            vec![second.clone()]
        );
        let populated_root = storage.snapshot_import_audit_root().unwrap();
        assert_ne!(populated_root, empty_root);
        assert_eq!(
            FileStorage::snapshot_import_audit_root_for(&[first.clone(), second.clone()]).unwrap(),
            populated_root
        );
        storage
            .append_snapshot_import_audit_record_with_retention(third.clone(), 2)
            .unwrap();
        assert_eq!(
            storage.load_snapshot_import_audit_records().unwrap(),
            vec![second.clone(), third.clone()]
        );
        let retained_root = FileStorage::snapshot_import_audit_root_for(&[second, third]).unwrap();
        assert_eq!(storage.snapshot_import_audit_root().unwrap(), retained_root);
        assert_eq!(
            FileStorage::open(&dir)
                .unwrap()
                .snapshot_import_audit_root()
                .unwrap(),
            retained_root
        );
        let config = SnapshotImportAuditConfig {
            max_records: 2,
            max_page_size: 1,
        };
        assert_eq!(
            storage.maybe_load_snapshot_import_audit_config().unwrap(),
            None
        );
        assert_eq!(storage.snapshot_import_audit_config_root().unwrap(), None);
        storage
            .commit_snapshot_import_audit_config(&config)
            .unwrap();
        let config_root = FileStorage::snapshot_import_audit_config_root_for(&config).unwrap();
        assert_ne!(config_root, retained_root);
        assert_eq!(
            storage.snapshot_import_audit_config_root().unwrap(),
            Some(config_root)
        );
        assert_eq!(
            FileStorage::open(&dir)
                .unwrap()
                .maybe_load_snapshot_import_audit_config()
                .unwrap(),
            Some(config)
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persists_and_loads_mempool() {
        let dir = temp_dir("mempool");
        let storage = FileStorage::open(&dir).unwrap();
        let tx = transfer_tx();

        assert_eq!(storage.load_mempool().unwrap(), vec![]);
        storage.commit_mempool(std::slice::from_ref(&tx)).unwrap();

        let loaded = storage.load_mempool().unwrap();

        assert_eq!(loaded, vec![tx]);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn storage_byte_count_tracks_durable_files() {
        let dir = temp_dir("storage-bytes");
        let storage = FileStorage::open(&dir).unwrap();
        let before = storage.storage_bytes().unwrap();

        storage.commit_mempool(&[transfer_tx()]).unwrap();
        let after = storage.storage_bytes().unwrap();

        assert!(after > before);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn backup_and_restore_copy_durable_storage_files() {
        let dir = temp_dir("backup-source");
        let backup_dir = temp_dir("backup-copy");
        let restore_dir = temp_dir("backup-restore");
        let storage = FileStorage::open(&dir).unwrap();
        let proposer = ValidatorNode::new("validator-1", seeded_state());
        let block = proposer.propose_block(1, vec![transfer_tx()], 1_000);
        let snapshot = proposer.state().snapshot();
        let certificate = FinalityCertificate {
            height: 1,
            block_hash: block.block_hash(),
            signers: vec!["validator-1".into()],
        };
        storage.commit_snapshot(&snapshot).unwrap();
        storage.commit_block(&block).unwrap();
        storage.commit_finality_certificate(&certificate).unwrap();

        let manifest = storage.backup_to(&backup_dir).unwrap();
        let restored = FileStorage::restore_from_backup(&backup_dir, &restore_dir).unwrap();

        assert!(manifest.file_count >= 3);
        assert!(manifest.total_bytes > 0);
        assert_eq!(
            manifest.snapshot_root,
            Some(snapshot.global_state_root.clone())
        );
        assert_eq!(manifest.highest_block_height, Some(1));
        assert_eq!(manifest.highest_finality_certificate_height, Some(1));
        assert_eq!(restored.load_block(1).unwrap(), block);
        assert_eq!(restored.load_finality_certificate(1).unwrap(), certificate);
        assert_eq!(restored.load_snapshot().unwrap(), snapshot);

        fs::remove_dir_all(dir).unwrap();
        fs::remove_dir_all(backup_dir).unwrap();
        fs::remove_dir_all(restore_dir).unwrap();
    }

    #[test]
    fn consensus_signing_record_preserves_first_height_domain_claim() {
        let dir = temp_dir("consensus-signing-record");
        let storage = FileStorage::open(&dir).unwrap();
        let first = ConsensusSigningRecord {
            validator_id: "validator/1".into(),
            domain: ValidatorSignatureDomain::Vote,
            height: 7,
            block_hash: "block-a".into(),
        };
        let conflicting = ConsensusSigningRecord {
            block_hash: "block-b".into(),
            ..first.clone()
        };

        assert_eq!(
            storage
                .maybe_load_consensus_signing_record(
                    "validator/1",
                    ValidatorSignatureDomain::Vote,
                    7,
                )
                .unwrap(),
            None
        );
        assert_eq!(
            storage
                .commit_consensus_signing_record_if_absent(&first)
                .unwrap(),
            None
        );
        assert_eq!(
            storage
                .maybe_load_consensus_signing_record(
                    "validator/1",
                    ValidatorSignatureDomain::Vote,
                    7,
                )
                .unwrap(),
            Some(first.clone())
        );
        assert_eq!(
            storage
                .commit_consensus_signing_record_if_absent(&conflicting)
                .unwrap(),
            Some(first.clone())
        );
        assert_eq!(
            FileStorage::open(&dir)
                .unwrap()
                .maybe_load_consensus_signing_record(
                    "validator/1",
                    ValidatorSignatureDomain::Vote,
                    7,
                )
                .unwrap(),
            Some(first)
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn reports_corrupt_block_json() {
        let dir = temp_dir("corrupt-block");
        let storage = FileStorage::open(&dir).unwrap();
        fs::write(storage.root().join("blocks").join("1.bin"), b"not-json").unwrap();

        let error = storage.load_block(1).unwrap_err();

        assert!(matches!(error, StorageError::CorruptData(_)));
        fs::remove_dir_all(dir).unwrap();
    }
}
