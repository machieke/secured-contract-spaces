use bincode::Options;
use detta_consensus::{FinalityCertificate, SlashingRecord};
use detta_core::{Block, DeTTaState, SnapshotError, StateSnapshot, Transaction};
use detta_protocol::{SignedValidatorMessage, ValidatorSetMetadata};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
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
}

impl FileStorage {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let root = root.into();
        fs::create_dir_all(root.join("blocks")).map_err(io_error)?;
        fs::create_dir_all(root.join("certificates")).map_err(io_error)?;
        fs::create_dir_all(root.join("slashings")).map_err(io_error)?;
        Ok(Self { root })
    }

    pub fn root(&self) -> &Path {
        &self.root
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
        hash_bincode(roots)
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
            .map(|metrics| hash_bincode(&metrics))
            .transpose()
    }

    pub fn commit_block(&self, block: &Block) -> Result<(), StorageError> {
        write_json_atomic(&self.block_path(block.header.height), block)
    }

    pub fn load_block(&self, height: u64) -> Result<Block, StorageError> {
        read_json(&self.block_path(height))
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

    pub fn commit_slashing_record(&self, record: &SlashingRecord) -> Result<(), StorageError> {
        write_json_atomic(&self.slashing_path(&record.validator_id), record)
    }

    pub fn load_slashing_record(&self, validator_id: &str) -> Result<SlashingRecord, StorageError> {
        read_json(&self.slashing_path(validator_id))
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
        hash_bincode(&records)
    }

    pub fn append_snapshot_import_audit_record(
        &self,
        record: SnapshotImportAuditRecord,
    ) -> Result<(), StorageError> {
        let mut records = self.load_snapshot_import_audit_records()?;
        records.push(record);
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
        hash_bincode(&records)
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

    fn mempool_path(&self) -> PathBuf {
        self.root.join("mempool.bin")
    }
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), StorageError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io_error)?;
    }

    let tmp_path = path.with_extension("tmp");
    {
        let file = File::create(&tmp_path).map_err(io_error)?;
        let writer = BufWriter::new(file);
        bincode_options()
            .serialize_into(writer, value)
            .map_err(data_error)?;
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
    let reader = BufReader::new(file);
    bincode_options()
        .deserialize_from(reader)
        .map_err(data_error)
}

fn io_error(error: std::io::Error) -> StorageError {
    StorageError::Io(error.to_string())
}

fn data_error(error: bincode::Error) -> StorageError {
    StorageError::CorruptData(error.to_string())
}

fn bincode_options() -> impl Options {
    bincode::DefaultOptions::new().with_limit(MAX_ENCODED_BYTES)
}

fn hash_bincode<T: Serialize>(value: &T) -> Result<String, StorageError> {
    let bytes = bincode_options().serialize(value).map_err(data_error)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use detta_consensus::EquivocationEvidence;
    use detta_core::{Argument, Method, Transaction, ValidatorNode};
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

        storage.commit_block(&block).unwrap();
        let loaded = storage.load_block(1).unwrap();

        assert_eq!(loaded.block_hash(), block.block_hash());
        assert_eq!(loaded.header.storage_root, block.header.storage_root);
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

        storage.commit_finality_certificate(&certificate).unwrap();
        let loaded = storage.load_finality_certificate(7).unwrap();

        assert_eq!(loaded, certificate);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn persists_and_loads_slashing_record() {
        let dir = temp_dir("slashing");
        let storage = FileStorage::open(&dir).unwrap();
        let record = SlashingRecord {
            validator_id: "validator/1".into(),
            slashed_at_height: 9,
            evidence: EquivocationEvidence {
                validator_id: "validator/1".into(),
                height: 9,
                first_block_hash: "block-a".into(),
                second_block_hash: "block-b".into(),
            },
        };

        storage.commit_slashing_record(&record).unwrap();
        let loaded = storage.load_slashing_record("validator/1").unwrap();

        assert_eq!(loaded, record);
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
        };
        let second = SnapshotImportAuditRecord {
            snapshot_root: "snapshot-root-2".into(),
            manifest_hash: "manifest-hash-2".into(),
            required_metadata_roots_root: "required-roots-root-2".into(),
            required_metadata_roots_count: 1,
            manifest_metadata_roots_count: 2,
            chunk_count: 3,
            metadata_roots_verified: true,
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
        assert_eq!(
            FileStorage::open(&dir)
                .unwrap()
                .snapshot_import_audit_root()
                .unwrap(),
            populated_root
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
    fn reports_corrupt_block_json() {
        let dir = temp_dir("corrupt-block");
        let storage = FileStorage::open(&dir).unwrap();
        fs::write(storage.root().join("blocks").join("1.bin"), b"not-bincode").unwrap();

        let error = storage.load_block(1).unwrap_err();

        assert!(matches!(error, StorageError::CorruptData(_)));
        fs::remove_dir_all(dir).unwrap();
    }
}
