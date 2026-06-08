use bincode::Options;
use detta_consensus::{FinalityCertificate, SlashingRecord};
use detta_core::{Block, DeTTaState, SnapshotError, StateSnapshot, Transaction};
use detta_protocol::{SignedValidatorMessage, ValidatorSetMetadata};
use serde::{de::DeserializeOwned, Serialize};
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
