use detta_consensus::{EquivocationEvidence, FinalityCertificate, ValidatorSetUpdate, Vote};
use detta_core::{Block, DeTTaState, SnapshotError, StateSnapshot, Transaction};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

pub const PROTOCOL_MAGIC: [u8; 4] = *b"DTTA";
pub const CURRENT_PROTOCOL_VERSION: u16 = 1;
pub const HEADER_LEN: usize = 10;
pub const MAX_PAYLOAD_LEN: usize = 16 * 1024 * 1024;
pub const VALIDATOR_SIGNATURE_PREFIX: &str = "detta.validator.protocol.v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum ValidatorSignatureDomain {
    BlockProposal,
    Vote,
    FinalityCertificate,
    ValidatorSetUpdate,
    EquivocationEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidatorPublicKey {
    pub validator_id: String,
    pub key_id: String,
    pub public_key_hex: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidatorSetMetadata {
    pub network_id: String,
    pub chain_id: String,
    pub validators: Vec<ValidatorPublicKey>,
}

#[derive(Clone, Debug)]
pub struct ValidatorSigningKey {
    validator_id: String,
    key_id: String,
    signing_key: SigningKey,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SignedValidatorMessage {
    pub signer: String,
    pub key_id: String,
    pub network_id: String,
    pub chain_id: String,
    pub domain: ValidatorSignatureDomain,
    pub message: Box<ProtocolMessage>,
    pub signature_hex: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotChunkRequest {
    pub snapshot_root: String,
    pub start_index: u32,
    pub max_chunks: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotChunkManifest {
    pub snapshot_root: String,
    pub snapshot_hash: String,
    pub chunk_size: u32,
    pub total_bytes: u64,
    pub chunk_count: u32,
    pub chunk_hashes: Vec<String>,
    pub chunk_root: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SnapshotChunk {
    pub manifest_hash: String,
    pub index: u32,
    pub bytes: Vec<u8>,
    pub chunk_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SnapshotChunkSet {
    pub manifest: SnapshotChunkManifest,
    pub chunks: Vec<SnapshotChunk>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProtocolMessage {
    Transaction(Transaction),
    Block(Box<Block>),
    Vote(Vote),
    FinalityCertificate(FinalityCertificate),
    ValidatorSetUpdate(ValidatorSetUpdate),
    EquivocationEvidence(EquivocationEvidence),
    StateSnapshot(Box<StateSnapshot>),
    PeerHello(PeerHello),
    SignedValidator(Box<SignedValidatorMessage>),
    SnapshotChunkRequest(SnapshotChunkRequest),
    SnapshotChunkManifest(SnapshotChunkManifest),
    SnapshotChunk(SnapshotChunk),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum ProtocolMessageKind {
    Transaction,
    Block,
    Vote,
    FinalityCertificate,
    ValidatorSetUpdate,
    EquivocationEvidence,
    StateSnapshot,
    PeerHello,
    SignedValidator,
    SnapshotChunkRequest,
    SnapshotChunkManifest,
    SnapshotChunk,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum PeerRole {
    Validator,
    FullNode,
    ArchiveNode,
    LightClient,
    Relayer,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerHello {
    pub peer_id: String,
    pub network_id: String,
    pub protocol_version: u16,
    pub roles: BTreeSet<PeerRole>,
}

impl PeerHello {
    pub fn new(
        peer_id: impl Into<String>,
        network_id: impl Into<String>,
        roles: impl IntoIterator<Item = PeerRole>,
    ) -> Self {
        Self {
            peer_id: peer_id.into(),
            network_id: network_id.into(),
            protocol_version: CURRENT_PROTOCOL_VERSION,
            roles: roles.into_iter().collect(),
        }
    }
}

impl ProtocolMessage {
    pub fn kind(&self) -> ProtocolMessageKind {
        match self {
            ProtocolMessage::Transaction(_) => ProtocolMessageKind::Transaction,
            ProtocolMessage::Block(_) => ProtocolMessageKind::Block,
            ProtocolMessage::Vote(_) => ProtocolMessageKind::Vote,
            ProtocolMessage::FinalityCertificate(_) => ProtocolMessageKind::FinalityCertificate,
            ProtocolMessage::ValidatorSetUpdate(_) => ProtocolMessageKind::ValidatorSetUpdate,
            ProtocolMessage::EquivocationEvidence(_) => ProtocolMessageKind::EquivocationEvidence,
            ProtocolMessage::StateSnapshot(_) => ProtocolMessageKind::StateSnapshot,
            ProtocolMessage::PeerHello(_) => ProtocolMessageKind::PeerHello,
            ProtocolMessage::SignedValidator(_) => ProtocolMessageKind::SignedValidator,
            ProtocolMessage::SnapshotChunkRequest(_) => ProtocolMessageKind::SnapshotChunkRequest,
            ProtocolMessage::SnapshotChunkManifest(_) => ProtocolMessageKind::SnapshotChunkManifest,
            ProtocolMessage::SnapshotChunk(_) => ProtocolMessageKind::SnapshotChunk,
        }
    }
}

impl ValidatorSigningKey {
    pub fn from_seed(
        validator_id: impl Into<String>,
        key_id: impl Into<String>,
        seed: [u8; 32],
    ) -> Self {
        Self {
            validator_id: validator_id.into(),
            key_id: key_id.into(),
            signing_key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn validator_id(&self) -> &str {
        &self.validator_id
    }

    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    pub fn public_key(&self) -> ValidatorPublicKey {
        ValidatorPublicKey {
            validator_id: self.validator_id.clone(),
            key_id: self.key_id.clone(),
            public_key_hex: hex_lower(&self.signing_key.verifying_key().to_bytes()),
        }
    }

    pub fn sign_message(
        &self,
        network_id: impl Into<String>,
        chain_id: impl Into<String>,
        message: ProtocolMessage,
    ) -> Result<SignedValidatorMessage, SignatureError> {
        let network_id = network_id.into();
        let chain_id = chain_id.into();
        let domain = expected_signature_domain(&message)?;
        let payload = validator_signing_payload(domain, &network_id, &chain_id, &message)?;
        let signature = self.signing_key.sign(&payload);

        Ok(SignedValidatorMessage {
            signer: self.validator_id.clone(),
            key_id: self.key_id.clone(),
            network_id,
            chain_id,
            domain,
            message: Box::new(message),
            signature_hex: hex_lower(&signature.to_bytes()),
        })
    }
}

impl SignedValidatorMessage {
    pub fn verify(
        &self,
        expected_network_id: &str,
        expected_chain_id: &str,
        public_key: &ValidatorPublicKey,
    ) -> Result<(), SignatureError> {
        if self.network_id != expected_network_id {
            return Err(SignatureError::NetworkMismatch {
                expected: expected_network_id.to_string(),
                actual: self.network_id.clone(),
            });
        }
        if self.chain_id != expected_chain_id {
            return Err(SignatureError::ChainMismatch {
                expected: expected_chain_id.to_string(),
                actual: self.chain_id.clone(),
            });
        }
        if self.signer != public_key.validator_id {
            return Err(SignatureError::SignerMismatch {
                expected: self.signer.clone(),
                actual: public_key.validator_id.clone(),
            });
        }
        if self.key_id != public_key.key_id {
            return Err(SignatureError::KeyIdMismatch {
                expected: self.key_id.clone(),
                actual: public_key.key_id.clone(),
            });
        }

        let expected_domain = expected_signature_domain(&self.message)?;
        if self.domain != expected_domain {
            return Err(SignatureError::DomainMismatch {
                expected: expected_domain,
                actual: self.domain,
            });
        }
        if let Some(claimed_signer) = claimed_message_signer(&self.message) {
            if self.signer != claimed_signer {
                return Err(SignatureError::MessageSignerMismatch {
                    expected: claimed_signer.to_string(),
                    actual: self.signer.clone(),
                });
            }
        }

        let public_key_bytes = decode_hex_array::<32>(&public_key.public_key_hex)
            .map_err(|_| SignatureError::InvalidPublicKey)?;
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes)
            .map_err(|_| SignatureError::InvalidPublicKey)?;
        let signature_bytes = decode_hex_array::<64>(&self.signature_hex)
            .map_err(|_| SignatureError::InvalidSignatureEncoding)?;
        let signature = Signature::from_bytes(&signature_bytes);
        let payload = validator_signing_payload(
            self.domain,
            &self.network_id,
            &self.chain_id,
            &self.message,
        )?;

        verifying_key
            .verify(&payload, &signature)
            .map_err(|_| SignatureError::InvalidSignature)
    }

    pub fn into_message(self) -> ProtocolMessage {
        *self.message
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SignatureError {
    UnsupportedMessageKind(ProtocolMessageKind),
    DomainMismatch {
        expected: ValidatorSignatureDomain,
        actual: ValidatorSignatureDomain,
    },
    NetworkMismatch {
        expected: String,
        actual: String,
    },
    ChainMismatch {
        expected: String,
        actual: String,
    },
    SignerMismatch {
        expected: String,
        actual: String,
    },
    KeyIdMismatch {
        expected: String,
        actual: String,
    },
    MessageSignerMismatch {
        expected: String,
        actual: String,
    },
    InvalidPublicKey,
    InvalidSignatureEncoding,
    InvalidSignature,
    EncodeFailed,
}

pub fn expected_signature_domain(
    message: &ProtocolMessage,
) -> Result<ValidatorSignatureDomain, SignatureError> {
    match message {
        ProtocolMessage::Block(_) => Ok(ValidatorSignatureDomain::BlockProposal),
        ProtocolMessage::Vote(_) => Ok(ValidatorSignatureDomain::Vote),
        ProtocolMessage::FinalityCertificate(_) => {
            Ok(ValidatorSignatureDomain::FinalityCertificate)
        }
        ProtocolMessage::ValidatorSetUpdate(_) => Ok(ValidatorSignatureDomain::ValidatorSetUpdate),
        ProtocolMessage::EquivocationEvidence(_) => {
            Ok(ValidatorSignatureDomain::EquivocationEvidence)
        }
        other => Err(SignatureError::UnsupportedMessageKind(other.kind())),
    }
}

fn claimed_message_signer(message: &ProtocolMessage) -> Option<&str> {
    match message {
        ProtocolMessage::Block(block) => Some(&block.header.proposer),
        ProtocolMessage::Vote(vote) => Some(&vote.validator_id),
        ProtocolMessage::FinalityCertificate(_)
        | ProtocolMessage::ValidatorSetUpdate(_)
        | ProtocolMessage::EquivocationEvidence(_)
        | ProtocolMessage::Transaction(_)
        | ProtocolMessage::StateSnapshot(_)
        | ProtocolMessage::PeerHello(_)
        | ProtocolMessage::SignedValidator(_)
        | ProtocolMessage::SnapshotChunkRequest(_)
        | ProtocolMessage::SnapshotChunkManifest(_)
        | ProtocolMessage::SnapshotChunk(_) => None,
    }
}

impl SnapshotChunkManifest {
    pub fn manifest_hash(&self) -> Result<String, SnapshotSyncError> {
        hash_postcard(self).map_err(|_| SnapshotSyncError::EncodeFailed)
    }
}

impl SnapshotChunkSet {
    pub fn from_snapshot(
        snapshot: &StateSnapshot,
        max_chunk_bytes: usize,
    ) -> Result<Self, SnapshotSyncError> {
        build_snapshot_chunks(snapshot, max_chunk_bytes)
    }

    pub fn verify(&self) -> Result<(), SnapshotSyncError> {
        self.reconstruct_snapshot().map(|_| ())
    }

    pub fn reconstruct_snapshot(&self) -> Result<StateSnapshot, SnapshotSyncError> {
        if self.manifest.chunk_count as usize != self.manifest.chunk_hashes.len() {
            return Err(SnapshotSyncError::ManifestChunkCountMismatch {
                declared: self.manifest.chunk_count,
                actual: self.manifest.chunk_hashes.len() as u32,
            });
        }
        let expected_chunk_root = chunk_root(&self.manifest.chunk_hashes)?;
        if self.manifest.chunk_root != expected_chunk_root {
            return Err(SnapshotSyncError::ChunkRootMismatch {
                expected: expected_chunk_root,
                actual: self.manifest.chunk_root.clone(),
            });
        }

        let manifest_hash = self.manifest.manifest_hash()?;
        let mut chunks = BTreeMap::new();
        let mut total_bytes = 0_u64;

        for chunk in &self.chunks {
            if chunk.manifest_hash != manifest_hash {
                return Err(SnapshotSyncError::ManifestHashMismatch {
                    expected: manifest_hash,
                    actual: chunk.manifest_hash.clone(),
                });
            }
            if chunk.index >= self.manifest.chunk_count {
                return Err(SnapshotSyncError::UnexpectedChunk {
                    index: chunk.index,
                    chunk_count: self.manifest.chunk_count,
                });
            }
            if chunks.insert(chunk.index, chunk).is_some() {
                return Err(SnapshotSyncError::DuplicateChunk { index: chunk.index });
            }

            let actual_hash = hash_bytes(&chunk.bytes);
            let expected_hash = &self.manifest.chunk_hashes[chunk.index as usize];
            if chunk.chunk_hash != actual_hash || &chunk.chunk_hash != expected_hash {
                return Err(SnapshotSyncError::ChunkHashMismatch { index: chunk.index });
            }
            total_bytes += chunk.bytes.len() as u64;
        }

        let mut snapshot_bytes = Vec::with_capacity(self.manifest.total_bytes as usize);
        for index in 0..self.manifest.chunk_count {
            let chunk = chunks
                .get(&index)
                .ok_or(SnapshotSyncError::MissingChunk { index })?;
            snapshot_bytes.extend_from_slice(&chunk.bytes);
        }

        if total_bytes != self.manifest.total_bytes {
            return Err(SnapshotSyncError::TotalBytesMismatch {
                expected: self.manifest.total_bytes,
                actual: total_bytes,
            });
        }

        let snapshot_hash = hash_bytes(&snapshot_bytes);
        if snapshot_hash != self.manifest.snapshot_hash {
            return Err(SnapshotSyncError::SnapshotHashMismatch {
                expected: self.manifest.snapshot_hash.clone(),
                actual: snapshot_hash,
            });
        }

        let snapshot: StateSnapshot =
            postcard::from_bytes(&snapshot_bytes).map_err(|_| SnapshotSyncError::DecodeFailed)?;
        if snapshot.global_state_root != self.manifest.snapshot_root {
            return Err(SnapshotSyncError::SnapshotRootMismatch {
                expected: self.manifest.snapshot_root.clone(),
                actual: snapshot.global_state_root,
            });
        }
        DeTTaState::from_snapshot(snapshot.clone()).map_err(SnapshotSyncError::InvalidSnapshot)?;
        Ok(snapshot)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SnapshotSyncError {
    InvalidChunkSize,
    EncodeFailed,
    DecodeFailed,
    ManifestChunkCountMismatch { declared: u32, actual: u32 },
    ManifestHashMismatch { expected: String, actual: String },
    ChunkHashMismatch { index: u32 },
    ChunkRootMismatch { expected: String, actual: String },
    DuplicateChunk { index: u32 },
    MissingChunk { index: u32 },
    UnexpectedChunk { index: u32, chunk_count: u32 },
    TotalBytesMismatch { expected: u64, actual: u64 },
    SnapshotHashMismatch { expected: String, actual: String },
    SnapshotRootMismatch { expected: String, actual: String },
    InvalidSnapshot(SnapshotError),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ProtocolError {
    TruncatedHeader { actual: usize },
    BadMagic { actual: [u8; 4] },
    UnsupportedVersion { actual: u16 },
    PayloadTooLarge { actual: usize, max: usize },
    PayloadLengthOverflow { actual: usize },
    LengthMismatch { declared: usize, actual: usize },
    EncodeFailed,
    DecodeFailed,
}

pub fn encode_message(message: &ProtocolMessage) -> Result<Vec<u8>, ProtocolError> {
    let payload = postcard::to_allocvec(message).map_err(|_| ProtocolError::EncodeFailed)?;
    encode_payload(CURRENT_PROTOCOL_VERSION, &payload)
}

pub fn decode_message(bytes: &[u8]) -> Result<ProtocolMessage, ProtocolError> {
    let payload = decode_payload(bytes)?;
    postcard::from_bytes(payload).map_err(|_| ProtocolError::DecodeFailed)
}

pub fn message_hash(message: &ProtocolMessage) -> Result<String, ProtocolError> {
    let bytes = encode_message(message)?;
    Ok(hex_lower(&Sha256::digest(bytes)))
}

pub fn build_snapshot_chunks(
    snapshot: &StateSnapshot,
    max_chunk_bytes: usize,
) -> Result<SnapshotChunkSet, SnapshotSyncError> {
    if max_chunk_bytes == 0 {
        return Err(SnapshotSyncError::InvalidChunkSize);
    }
    let chunk_size =
        u32::try_from(max_chunk_bytes).map_err(|_| SnapshotSyncError::InvalidChunkSize)?;
    let snapshot_bytes =
        postcard::to_allocvec(snapshot).map_err(|_| SnapshotSyncError::EncodeFailed)?;
    let snapshot_hash = hash_bytes(&snapshot_bytes);

    let mut raw_chunks: Vec<Vec<u8>> = snapshot_bytes
        .chunks(max_chunk_bytes)
        .map(ToOwned::to_owned)
        .collect();
    if raw_chunks.is_empty() {
        raw_chunks.push(Vec::new());
    }

    let chunk_hashes: Vec<_> = raw_chunks.iter().map(|chunk| hash_bytes(chunk)).collect();
    let chunk_root = chunk_root(&chunk_hashes)?;
    let chunk_count =
        u32::try_from(chunk_hashes.len()).map_err(|_| SnapshotSyncError::InvalidChunkSize)?;
    let manifest = SnapshotChunkManifest {
        snapshot_root: snapshot.global_state_root.clone(),
        snapshot_hash,
        chunk_size,
        total_bytes: snapshot_bytes.len() as u64,
        chunk_count,
        chunk_hashes,
        chunk_root,
    };
    let manifest_hash = manifest.manifest_hash()?;
    let chunks = raw_chunks
        .into_iter()
        .enumerate()
        .map(|(index, bytes)| SnapshotChunk {
            manifest_hash: manifest_hash.clone(),
            index: index as u32,
            chunk_hash: hash_bytes(&bytes),
            bytes,
        })
        .collect();

    Ok(SnapshotChunkSet { manifest, chunks })
}

fn hash_postcard<T: Serialize>(value: &T) -> Result<String, postcard::Error> {
    let bytes = postcard::to_allocvec(value)?;
    Ok(hash_bytes(&bytes))
}

fn chunk_root(chunk_hashes: &[String]) -> Result<String, SnapshotSyncError> {
    hash_postcard(&chunk_hashes).map_err(|_| SnapshotSyncError::EncodeFailed)
}

fn hash_bytes(bytes: &[u8]) -> String {
    hex_lower(&Sha256::digest(bytes))
}

fn validator_signing_payload(
    domain: ValidatorSignatureDomain,
    network_id: &str,
    chain_id: &str,
    message: &ProtocolMessage,
) -> Result<Vec<u8>, SignatureError> {
    let message_bytes = postcard::to_allocvec(message).map_err(|_| SignatureError::EncodeFailed)?;
    let mut payload = Vec::new();
    push_length_prefixed(&mut payload, VALIDATOR_SIGNATURE_PREFIX.as_bytes());
    payload.extend_from_slice(&CURRENT_PROTOCOL_VERSION.to_be_bytes());
    push_length_prefixed(&mut payload, signature_domain_name(domain).as_bytes());
    push_length_prefixed(&mut payload, network_id.as_bytes());
    push_length_prefixed(&mut payload, chain_id.as_bytes());
    push_length_prefixed(&mut payload, &message_bytes);
    Ok(payload)
}

fn signature_domain_name(domain: ValidatorSignatureDomain) -> &'static str {
    match domain {
        ValidatorSignatureDomain::BlockProposal => "block_proposal",
        ValidatorSignatureDomain::Vote => "vote",
        ValidatorSignatureDomain::FinalityCertificate => "finality_certificate",
        ValidatorSignatureDomain::ValidatorSetUpdate => "validator_set_update",
        ValidatorSignatureDomain::EquivocationEvidence => "equivocation_evidence",
    }
}

fn push_length_prefixed(out: &mut Vec<u8>, value: &[u8]) {
    let len = u32::try_from(value.len()).expect("validator signing component exceeds u32 length");
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(value);
}

fn encode_payload(version: u16, payload: &[u8]) -> Result<Vec<u8>, ProtocolError> {
    if payload.len() > MAX_PAYLOAD_LEN {
        return Err(ProtocolError::PayloadTooLarge {
            actual: payload.len(),
            max: MAX_PAYLOAD_LEN,
        });
    }
    let payload_len =
        u32::try_from(payload.len()).map_err(|_| ProtocolError::PayloadLengthOverflow {
            actual: payload.len(),
        })?;

    let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
    bytes.extend_from_slice(&PROTOCOL_MAGIC);
    bytes.extend_from_slice(&version.to_be_bytes());
    bytes.extend_from_slice(&payload_len.to_be_bytes());
    bytes.extend_from_slice(payload);
    Ok(bytes)
}

fn decode_payload(bytes: &[u8]) -> Result<&[u8], ProtocolError> {
    if bytes.len() < HEADER_LEN {
        return Err(ProtocolError::TruncatedHeader {
            actual: bytes.len(),
        });
    }

    let magic = [bytes[0], bytes[1], bytes[2], bytes[3]];
    if magic != PROTOCOL_MAGIC {
        return Err(ProtocolError::BadMagic { actual: magic });
    }

    let version = u16::from_be_bytes([bytes[4], bytes[5]]);
    if version != CURRENT_PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion { actual: version });
    }

    let declared = u32::from_be_bytes([bytes[6], bytes[7], bytes[8], bytes[9]]) as usize;
    if declared > MAX_PAYLOAD_LEN {
        return Err(ProtocolError::PayloadTooLarge {
            actual: declared,
            max: MAX_PAYLOAD_LEN,
        });
    }

    let actual = bytes.len() - HEADER_LEN;
    if declared != actual {
        return Err(ProtocolError::LengthMismatch { declared, actual });
    }

    Ok(&bytes[HEADER_LEN..])
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

fn decode_hex_array<const N: usize>(value: &str) -> Result<[u8; N], ()> {
    if value.len() != N * 2 {
        return Err(());
    }

    let mut bytes = [0_u8; N];
    let raw = value.as_bytes();
    for index in 0..N {
        let high = decode_hex_nibble(raw[index * 2])?;
        let low = decode_hex_nibble(raw[index * 2 + 1])?;
        bytes[index] = (high << 4) | low;
    }
    Ok(bytes)
}

fn decode_hex_nibble(value: u8) -> Result<u8, ()> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        b'A'..=b'F' => Ok(value - b'A' + 10),
        _ => Err(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Argument, DeTTaState, Method};

    const TX_ENVELOPE_HEX: &str = "44545441000100000032000b64657474612d6c6f63616c0374783105416c6963650106546f6b656e4100030003426f62010455534443020a01c0843d";
    const VOTE_ENVELOPE_HEX: &str =
        "4454544100010000001b020b76616c696461746f722d31070c626c6f636b2d686173682d31";

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

    fn validator_key(validator_id: &str, seed_byte: u8) -> ValidatorSigningKey {
        ValidatorSigningKey::from_seed(validator_id, "consensus-key-1", [seed_byte; 32])
    }

    #[test]
    fn transaction_envelope_fixture_is_stable() {
        let message = ProtocolMessage::Transaction(transfer_tx());
        let encoded = encode_message(&message).unwrap();

        assert_eq!(hex_lower(&encoded), TX_ENVELOPE_HEX);
        assert_eq!(decode_message(&encoded), Ok(message));
    }

    #[test]
    fn vote_envelope_fixture_is_stable() {
        let message = ProtocolMessage::Vote(Vote {
            validator_id: "validator-1".into(),
            height: 7,
            block_hash: "block-hash-1".into(),
        });
        let encoded = encode_message(&message).unwrap();

        assert_eq!(hex_lower(&encoded), VOTE_ENVELOPE_HEX);
        assert_eq!(decode_message(&encoded), Ok(message));
    }

    #[test]
    fn block_certificate_and_snapshot_messages_round_trip() {
        let state = seeded_state();
        let (block, next_state) =
            state.build_block(1, vec![transfer_tx()], 1_000, "validator-1", "cert-1");
        let certificate = FinalityCertificate {
            height: 1,
            block_hash: block.block_hash(),
            signers: vec!["validator-1".into(), "validator-2".into()],
        };
        let messages = [
            ProtocolMessage::Block(Box::new(block)),
            ProtocolMessage::FinalityCertificate(certificate),
            ProtocolMessage::StateSnapshot(Box::new(next_state.snapshot())),
        ];

        for message in messages {
            let encoded = encode_message(&message).unwrap();
            assert_eq!(decode_message(&encoded), Ok(message));
        }
    }

    #[test]
    fn peer_hello_message_round_trips() {
        let hello = PeerHello::new(
            "validator-1",
            "detta-testnet",
            [PeerRole::Validator, PeerRole::ArchiveNode],
        );
        let message = ProtocolMessage::PeerHello(hello.clone());
        let encoded = encode_message(&message).unwrap();

        assert_eq!(decode_message(&encoded), Ok(message));
        assert_eq!(hello.protocol_version, CURRENT_PROTOCOL_VERSION);
        assert!(hello.roles.contains(&PeerRole::Validator));
    }

    #[test]
    fn signed_validator_vote_verifies_and_round_trips() {
        let key = validator_key("validator-1", 7);
        let vote = ProtocolMessage::Vote(Vote {
            validator_id: "validator-1".into(),
            height: 12,
            block_hash: "block-hash-12".into(),
        });

        let signed = key
            .sign_message("detta-testnet", "detta-local", vote.clone())
            .unwrap();

        assert_eq!(key.validator_id(), "validator-1");
        assert_eq!(key.key_id(), "consensus-key-1");
        assert_eq!(signed.domain, ValidatorSignatureDomain::Vote);
        signed
            .verify("detta-testnet", "detta-local", &key.public_key())
            .unwrap();
        assert_eq!(signed.clone().into_message(), vote);

        let envelope = ProtocolMessage::SignedValidator(Box::new(signed));
        let encoded = encode_message(&envelope).unwrap();
        assert_eq!(decode_message(&encoded), Ok(envelope));
    }

    #[test]
    fn signed_validator_messages_reject_replay_and_tampering() {
        let key = validator_key("validator-1", 7);
        let signed = key
            .sign_message(
                "detta-testnet",
                "detta-local",
                ProtocolMessage::Vote(Vote {
                    validator_id: "validator-1".into(),
                    height: 12,
                    block_hash: "block-hash-12".into(),
                }),
            )
            .unwrap();

        let mut wrong_domain = signed.clone();
        wrong_domain.domain = ValidatorSignatureDomain::FinalityCertificate;
        assert_eq!(
            wrong_domain.verify("detta-testnet", "detta-local", &key.public_key()),
            Err(SignatureError::DomainMismatch {
                expected: ValidatorSignatureDomain::Vote,
                actual: ValidatorSignatureDomain::FinalityCertificate,
            })
        );

        let mut wrong_network = signed.clone();
        wrong_network.network_id = "wrong-net".into();
        assert_eq!(
            wrong_network.verify("detta-testnet", "detta-local", &key.public_key()),
            Err(SignatureError::NetworkMismatch {
                expected: "detta-testnet".into(),
                actual: "wrong-net".into(),
            })
        );

        let mut tampered = signed.clone();
        match tampered.message.as_mut() {
            ProtocolMessage::Vote(vote) => vote.block_hash = "block-hash-13".into(),
            other => panic!("expected signed vote, got {other:?}"),
        }
        assert_eq!(
            tampered.verify("detta-testnet", "detta-local", &key.public_key()),
            Err(SignatureError::InvalidSignature)
        );

        let other_key = validator_key("validator-2", 8);
        assert_eq!(
            signed.verify("detta-testnet", "detta-local", &other_key.public_key()),
            Err(SignatureError::SignerMismatch {
                expected: "validator-1".into(),
                actual: "validator-2".into(),
            })
        );

        let mismatched_claim = key
            .sign_message(
                "detta-testnet",
                "detta-local",
                ProtocolMessage::Vote(Vote {
                    validator_id: "validator-2".into(),
                    height: 12,
                    block_hash: "block-hash-12".into(),
                }),
            )
            .unwrap();
        assert_eq!(
            mismatched_claim.verify("detta-testnet", "detta-local", &key.public_key()),
            Err(SignatureError::MessageSignerMismatch {
                expected: "validator-2".into(),
                actual: "validator-1".into(),
            })
        );

        let reporter_key = validator_key("validator-2", 8);
        let reported_evidence = reporter_key
            .sign_message(
                "detta-testnet",
                "detta-local",
                ProtocolMessage::EquivocationEvidence(EquivocationEvidence {
                    validator_id: "validator-1".into(),
                    height: 12,
                    first_block_hash: "block-hash-12-a".into(),
                    second_block_hash: "block-hash-12-b".into(),
                }),
            )
            .unwrap();
        reported_evidence
            .verify("detta-testnet", "detta-local", &reporter_key.public_key())
            .unwrap();

        assert_eq!(
            key.sign_message(
                "detta-testnet",
                "detta-local",
                ProtocolMessage::Transaction(transfer_tx())
            ),
            Err(SignatureError::UnsupportedMessageKind(
                ProtocolMessageKind::Transaction
            ))
        );
    }

    #[test]
    fn snapshot_chunks_reconstruct_verified_snapshot() {
        let snapshot = seeded_state().snapshot();
        let chunk_set = build_snapshot_chunks(&snapshot, 64).unwrap();

        assert!(chunk_set.chunks.len() > 1);
        assert_eq!(chunk_set.manifest.snapshot_root, snapshot.global_state_root);
        assert_eq!(chunk_set.reconstruct_snapshot().unwrap(), snapshot);
        chunk_set.verify().unwrap();

        let messages = [
            ProtocolMessage::SnapshotChunkRequest(SnapshotChunkRequest {
                snapshot_root: chunk_set.manifest.snapshot_root.clone(),
                start_index: 0,
                max_chunks: 2,
            }),
            ProtocolMessage::SnapshotChunkManifest(chunk_set.manifest.clone()),
            ProtocolMessage::SnapshotChunk(chunk_set.chunks[0].clone()),
        ];
        for message in messages {
            let encoded = encode_message(&message).unwrap();
            assert_eq!(decode_message(&encoded), Ok(message));
        }
    }

    #[test]
    fn snapshot_chunks_reject_missing_duplicate_and_tampered_data() {
        let snapshot = seeded_state().snapshot();
        let chunk_set = build_snapshot_chunks(&snapshot, 64).unwrap();

        let mut tampered = chunk_set.clone();
        tampered.chunks[0].bytes.push(42);
        assert_eq!(
            tampered.reconstruct_snapshot(),
            Err(SnapshotSyncError::ChunkHashMismatch { index: 0 })
        );

        let mut missing = chunk_set.clone();
        missing.chunks.remove(0);
        assert_eq!(
            missing.reconstruct_snapshot(),
            Err(SnapshotSyncError::MissingChunk { index: 0 })
        );

        let mut duplicate = chunk_set.clone();
        duplicate.chunks.push(duplicate.chunks[0].clone());
        assert_eq!(
            duplicate.reconstruct_snapshot(),
            Err(SnapshotSyncError::DuplicateChunk { index: 0 })
        );

        let mut wrong_manifest = chunk_set;
        wrong_manifest.manifest.chunk_hashes[0] = "wrong-hash".into();
        assert!(matches!(
            wrong_manifest.reconstruct_snapshot(),
            Err(SnapshotSyncError::ChunkRootMismatch { .. })
        ));
    }

    #[test]
    fn rejects_bad_headers_and_lengths() {
        let message = ProtocolMessage::Transaction(transfer_tx());
        let mut encoded = encode_message(&message).unwrap();

        assert_eq!(
            decode_message(&encoded[..HEADER_LEN - 1]),
            Err(ProtocolError::TruncatedHeader {
                actual: HEADER_LEN - 1
            })
        );

        encoded[0] = b'X';
        assert_eq!(
            decode_message(&encoded),
            Err(ProtocolError::BadMagic {
                actual: [b'X', b'T', b'T', b'A']
            })
        );

        let mut encoded = encode_message(&message).unwrap();
        encoded[5] = 2;
        assert_eq!(
            decode_message(&encoded),
            Err(ProtocolError::UnsupportedVersion { actual: 2 })
        );

        let mut encoded = encode_message(&message).unwrap();
        let declared =
            u32::from_be_bytes([encoded[6], encoded[7], encoded[8], encoded[9]]) as usize;
        encoded.pop();
        assert_eq!(
            decode_message(&encoded),
            Err(ProtocolError::LengthMismatch {
                declared,
                actual: encoded.len() - HEADER_LEN
            })
        );
    }
}
