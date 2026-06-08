use detta_consensus::{EquivocationEvidence, FinalityCertificate, ValidatorSetUpdate, Vote};
use detta_core::{Block, StateSnapshot, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const PROTOCOL_MAGIC: [u8; 4] = *b"DTTA";
pub const CURRENT_PROTOCOL_VERSION: u16 = 1;
pub const HEADER_LEN: usize = 10;
pub const MAX_PAYLOAD_LEN: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProtocolMessage {
    Transaction(Transaction),
    Block(Box<Block>),
    Vote(Vote),
    FinalityCertificate(FinalityCertificate),
    ValidatorSetUpdate(ValidatorSetUpdate),
    EquivocationEvidence(EquivocationEvidence),
    StateSnapshot(Box<StateSnapshot>),
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
        }
    }
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
