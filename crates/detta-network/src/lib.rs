use detta_protocol::{
    decode_message, encode_message, PeerHello, ProtocolError, ProtocolMessage, HEADER_LEN,
    MAX_PAYLOAD_LEN,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};

pub type PeerId = String;
pub type NetworkMessage = ProtocolMessage;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub from: PeerId,
    pub to: PeerId,
    pub message: NetworkMessage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct WireEnvelope {
    from: PeerId,
    to: PeerId,
    payload: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkError {
    DuplicatePeer(PeerId),
    UnknownPeer(PeerId),
    Protocol(ProtocolError),
    Io(String),
    PeerRejected(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerConnection {
    pub hello: PeerHello,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeerConnectionManager {
    local: PeerHello,
    peers: BTreeMap<PeerId, PeerConnection>,
    max_receive_batch: usize,
}

impl PeerConnectionManager {
    pub fn new(local: PeerHello, max_receive_batch: usize) -> Self {
        Self {
            local,
            peers: BTreeMap::new(),
            max_receive_batch,
        }
    }

    pub fn local(&self) -> &PeerHello {
        &self.local
    }

    pub fn peer_count(&self) -> usize {
        self.peers.len()
    }

    pub fn peer(&self, peer_id: &str) -> Option<&PeerConnection> {
        self.peers.get(peer_id)
    }

    pub fn register_peer(&mut self, hello: PeerHello) -> Result<(), NetworkError> {
        if hello.peer_id == self.local.peer_id {
            return Err(NetworkError::PeerRejected(format!(
                "peer {} cannot connect to itself",
                hello.peer_id
            )));
        }
        if hello.network_id != self.local.network_id {
            return Err(NetworkError::PeerRejected(format!(
                "network mismatch: {}",
                hello.network_id
            )));
        }
        if hello.protocol_version != self.local.protocol_version {
            return Err(NetworkError::PeerRejected(format!(
                "protocol version mismatch: {}",
                hello.protocol_version
            )));
        }
        if self.peers.contains_key(&hello.peer_id) {
            return Err(NetworkError::DuplicatePeer(hello.peer_id));
        }

        self.peers
            .insert(hello.peer_id.clone(), PeerConnection { hello });
        Ok(())
    }

    pub fn receive_bounded(
        &self,
        transport: &mut InMemoryTransport,
        peer_id: &str,
    ) -> Result<Vec<Envelope>, NetworkError> {
        transport.drain_peer_bounded(peer_id, self.max_receive_batch)
    }
}

#[derive(Debug)]
pub struct TcpProtocolStream {
    stream: TcpStream,
}

impl TcpProtocolStream {
    pub fn connect(addr: impl ToSocketAddrs) -> Result<Self, NetworkError> {
        let stream = TcpStream::connect(addr).map_err(io_error)?;
        Ok(Self { stream })
    }

    pub fn from_stream(stream: TcpStream) -> Self {
        Self { stream }
    }

    pub fn send(&mut self, message: &NetworkMessage) -> Result<(), NetworkError> {
        write_wire_message(&mut self.stream, message)
    }

    pub fn receive(&mut self) -> Result<NetworkMessage, NetworkError> {
        read_wire_message(&mut self.stream)
    }

    pub fn handshake(&mut self, local: PeerHello) -> Result<PeerHello, NetworkError> {
        let expected_network = local.network_id.clone();
        let expected_version = local.protocol_version;
        self.send(&NetworkMessage::PeerHello(local))?;

        match self.receive()? {
            NetworkMessage::PeerHello(remote) => {
                if remote.network_id != expected_network {
                    return Err(NetworkError::PeerRejected(format!(
                        "network mismatch: {}",
                        remote.network_id
                    )));
                }
                if remote.protocol_version != expected_version {
                    return Err(NetworkError::PeerRejected(format!(
                        "protocol version mismatch: {}",
                        remote.protocol_version
                    )));
                }
                Ok(remote)
            }
            other => Err(NetworkError::PeerRejected(format!(
                "expected peer hello, got {:?}",
                other.kind()
            ))),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InMemoryTransport {
    inboxes: BTreeMap<PeerId, Vec<WireEnvelope>>,
}

impl InMemoryTransport {
    pub fn new(peers: impl IntoIterator<Item = PeerId>) -> Result<Self, NetworkError> {
        let mut transport = Self::default();
        for peer in peers {
            transport.add_peer(peer)?;
        }
        Ok(transport)
    }

    pub fn add_peer(&mut self, peer: PeerId) -> Result<(), NetworkError> {
        if self.inboxes.contains_key(&peer) {
            return Err(NetworkError::DuplicatePeer(peer));
        }
        self.inboxes.insert(peer, Vec::new());
        Ok(())
    }

    pub fn peers(&self) -> impl Iterator<Item = &PeerId> {
        self.inboxes.keys()
    }

    pub fn send(
        &mut self,
        from: impl Into<PeerId>,
        to: impl Into<PeerId>,
        message: NetworkMessage,
    ) -> Result<(), NetworkError> {
        let from = from.into();
        let to = to.into();
        self.require_peer(&from)?;
        let inbox = self
            .inboxes
            .get_mut(&to)
            .ok_or_else(|| NetworkError::UnknownPeer(to.clone()))?;
        let payload = encode_message(&message).map_err(NetworkError::Protocol)?;
        inbox.push(WireEnvelope { from, to, payload });
        Ok(())
    }

    pub fn inject_raw(
        &mut self,
        from: impl Into<PeerId>,
        to: impl Into<PeerId>,
        payload: Vec<u8>,
    ) -> Result<(), NetworkError> {
        let from = from.into();
        let to = to.into();
        self.require_peer(&from)?;
        let inbox = self
            .inboxes
            .get_mut(&to)
            .ok_or_else(|| NetworkError::UnknownPeer(to.clone()))?;
        inbox.push(WireEnvelope { from, to, payload });
        Ok(())
    }

    pub fn broadcast(
        &mut self,
        from: impl Into<PeerId>,
        message: NetworkMessage,
    ) -> Result<usize, NetworkError> {
        let from = from.into();
        self.require_peer(&from)?;
        let recipients: Vec<_> = self
            .inboxes
            .keys()
            .filter(|peer| *peer != &from)
            .cloned()
            .collect();

        let sent = recipients.len();
        for to in recipients {
            self.send(from.clone(), to, message.clone())?;
        }
        Ok(sent)
    }

    pub fn drain_peer(&mut self, peer: &str) -> Result<Vec<Envelope>, NetworkError> {
        let limit = self
            .inboxes
            .get(peer)
            .map(Vec::len)
            .ok_or_else(|| NetworkError::UnknownPeer(peer.to_string()))?;
        self.drain_peer_bounded(peer, limit)
    }

    pub fn drain_peer_bounded(
        &mut self,
        peer: &str,
        max_envelopes: usize,
    ) -> Result<Vec<Envelope>, NetworkError> {
        let inbox = self
            .inboxes
            .get_mut(peer)
            .ok_or_else(|| NetworkError::UnknownPeer(peer.to_string()))?;

        let take_count = max_envelopes.min(inbox.len());
        let wires: Vec<_> = inbox.drain(..take_count).collect();
        wires
            .into_iter()
            .map(|wire| {
                let message = decode_message(&wire.payload).map_err(NetworkError::Protocol)?;
                Ok(Envelope {
                    from: wire.from,
                    to: wire.to,
                    message,
                })
            })
            .collect()
    }

    pub fn pending_len(&self, peer: &str) -> Result<usize, NetworkError> {
        self.inboxes
            .get(peer)
            .map(Vec::len)
            .ok_or_else(|| NetworkError::UnknownPeer(peer.to_string()))
    }

    fn require_peer(&self, peer: &str) -> Result<(), NetworkError> {
        if self.inboxes.contains_key(peer) {
            Ok(())
        } else {
            Err(NetworkError::UnknownPeer(peer.to_string()))
        }
    }
}

fn write_wire_message(
    writer: &mut impl Write,
    message: &NetworkMessage,
) -> Result<(), NetworkError> {
    let bytes = encode_message(message).map_err(NetworkError::Protocol)?;
    writer.write_all(&bytes).map_err(io_error)
}

fn read_wire_message(reader: &mut impl Read) -> Result<NetworkMessage, NetworkError> {
    let mut header = [0u8; HEADER_LEN];
    reader.read_exact(&mut header).map_err(io_error)?;

    let declared = u32::from_be_bytes([header[6], header[7], header[8], header[9]]) as usize;
    if declared > MAX_PAYLOAD_LEN {
        return Err(NetworkError::Protocol(ProtocolError::PayloadTooLarge {
            actual: declared,
            max: MAX_PAYLOAD_LEN,
        }));
    }

    let mut bytes = Vec::with_capacity(HEADER_LEN + declared);
    bytes.extend_from_slice(&header);
    let mut payload = vec![0; declared];
    reader.read_exact(&mut payload).map_err(io_error)?;
    bytes.extend_from_slice(&payload);

    decode_message(&bytes).map_err(NetworkError::Protocol)
}

fn io_error(error: std::io::Error) -> NetworkError {
    NetworkError::Io(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_consensus::{EquivocationEvidence, Vote};
    use detta_core::{Argument, DeTTaState, Method, Transaction, ValidatorNode};
    use detta_protocol::{encode_message, PeerRole, ProtocolError, PROTOCOL_MAGIC};
    use std::net::TcpListener;
    use std::thread;

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

    fn tx_with_hash(tx_hash: &str) -> Transaction {
        let mut tx = transfer_tx();
        tx.tx_hash = tx_hash.into();
        tx
    }

    fn validator_hello(peer_id: &str) -> PeerHello {
        PeerHello::new(peer_id, "detta-testnet", [PeerRole::Validator])
    }

    #[test]
    fn peer_connection_manager_tracks_validated_peer_metadata() {
        let mut manager = PeerConnectionManager::new(validator_hello("validator-1"), 16);
        let remote = PeerHello::new("full-node-1", "detta-testnet", [PeerRole::FullNode]);

        manager.register_peer(remote.clone()).unwrap();

        assert_eq!(manager.peer_count(), 1);
        assert_eq!(manager.peer("full-node-1").unwrap().hello, remote);
        assert_eq!(
            manager.register_peer(PeerHello::new(
                "full-node-1",
                "detta-testnet",
                [PeerRole::FullNode]
            )),
            Err(NetworkError::DuplicatePeer("full-node-1".into()))
        );
        assert!(matches!(
            manager.register_peer(PeerHello::new(
                "validator-2",
                "wrong-net",
                [PeerRole::Validator]
            )),
            Err(NetworkError::PeerRejected(_))
        ));
        assert!(matches!(
            manager.register_peer(validator_hello("validator-1")),
            Err(NetworkError::PeerRejected(_))
        ));
    }

    #[test]
    fn bounded_receive_loop_preserves_unread_envelopes() {
        let manager = PeerConnectionManager::new(validator_hello("validator-2"), 2);
        let mut transport =
            InMemoryTransport::new(["validator-1".into(), "validator-2".into()]).unwrap();
        for index in 1..=3 {
            transport
                .send(
                    "validator-1",
                    "validator-2",
                    NetworkMessage::Transaction(tx_with_hash(&format!("tx{index}"))),
                )
                .unwrap();
        }

        let first_batch = manager
            .receive_bounded(&mut transport, "validator-2")
            .unwrap();

        assert_eq!(first_batch.len(), 2);
        assert_eq!(transport.pending_len("validator-2"), Ok(1));

        let second_batch = manager
            .receive_bounded(&mut transport, "validator-2")
            .unwrap();

        assert_eq!(second_batch.len(), 1);
        assert_eq!(transport.pending_len("validator-2"), Ok(0));
    }

    #[test]
    fn transport_sends_transaction_to_peer() {
        let mut transport =
            InMemoryTransport::new(["v1".into(), "v2".into(), "v3".into()]).unwrap();

        transport
            .send("v1", "v2", NetworkMessage::Transaction(transfer_tx()))
            .unwrap();

        assert_eq!(transport.pending_len("v2"), Ok(1));
        let envelopes = transport.drain_peer("v2").unwrap();
        assert_eq!(envelopes[0].from, "v1");
        assert_eq!(envelopes[0].to, "v2");
        assert!(matches!(
            envelopes[0].message,
            NetworkMessage::Transaction(_)
        ));
        assert_eq!(transport.pending_len("v2"), Ok(0));
    }

    #[test]
    fn transport_broadcasts_block_without_loopback() {
        let mut transport =
            InMemoryTransport::new(["v1".into(), "v2".into(), "v3".into()]).unwrap();
        let proposer = ValidatorNode::new("v1", seeded_state());
        let block = proposer.propose_block(1, vec![transfer_tx()], 1_000);

        let sent = transport
            .broadcast("v1", NetworkMessage::Block(Box::new(block.clone())))
            .unwrap();

        assert_eq!(sent, 2);
        assert_eq!(transport.pending_len("v1"), Ok(0));
        for peer in ["v2", "v3"] {
            let envelope = transport.drain_peer(peer).unwrap().pop().unwrap();
            assert_eq!(envelope.from, "v1");
            assert_eq!(envelope.to, peer);
            assert_eq!(
                envelope.message,
                NetworkMessage::Block(Box::new(block.clone()))
            );
        }
    }

    #[test]
    fn transport_rejects_unknown_or_duplicate_peers() {
        let mut transport = InMemoryTransport::new(["v1".into()]).unwrap();

        assert_eq!(
            transport.add_peer("v1".into()),
            Err(NetworkError::DuplicatePeer("v1".into()))
        );
        assert_eq!(
            transport.send("v1", "missing", NetworkMessage::Transaction(transfer_tx()),),
            Err(NetworkError::UnknownPeer("missing".into()))
        );
        assert_eq!(
            transport.broadcast("missing", NetworkMessage::Transaction(transfer_tx()),),
            Err(NetworkError::UnknownPeer("missing".into()))
        );
    }

    #[test]
    fn transport_rejects_corrupt_protocol_envelope() {
        let mut transport = InMemoryTransport::new(["v1".into(), "v2".into()]).unwrap();
        let mut payload = encode_message(&NetworkMessage::Transaction(transfer_tx())).unwrap();
        payload[0] = b'X';

        transport.inject_raw("v1", "v2", payload).unwrap();

        assert_eq!(
            transport.drain_peer("v2"),
            Err(NetworkError::Protocol(ProtocolError::BadMagic {
                actual: [
                    b'X',
                    PROTOCOL_MAGIC[1],
                    PROTOCOL_MAGIC[2],
                    PROTOCOL_MAGIC[3]
                ]
            }))
        );
    }

    #[test]
    fn transport_carries_consensus_control_messages() {
        let mut transport = InMemoryTransport::new(["v1".into(), "v2".into()]).unwrap();
        let vote = Vote {
            validator_id: "v1".into(),
            height: 1,
            block_hash: "block-a".into(),
        };
        let evidence = EquivocationEvidence {
            validator_id: "v1".into(),
            height: 1,
            first_block_hash: "block-a".into(),
            second_block_hash: "block-b".into(),
        };

        transport
            .send("v1", "v2", NetworkMessage::Vote(vote.clone()))
            .unwrap();
        transport
            .send(
                "v1",
                "v2",
                NetworkMessage::EquivocationEvidence(evidence.clone()),
            )
            .unwrap();

        let envelopes = transport.drain_peer("v2").unwrap();
        assert_eq!(envelopes[0].message, NetworkMessage::Vote(vote));
        assert_eq!(
            envelopes[1].message,
            NetworkMessage::EquivocationEvidence(evidence)
        );
    }

    #[test]
    fn tcp_protocol_stream_round_trips_over_local_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut server = TcpProtocolStream::from_stream(stream);
            let message = server.receive().unwrap();
            assert!(matches!(message, NetworkMessage::Transaction(_)));
            server
                .send(&NetworkMessage::Vote(Vote {
                    validator_id: "validator-1".into(),
                    height: 1,
                    block_hash: "block-a".into(),
                }))
                .unwrap();
        });

        let mut client = TcpProtocolStream::connect(addr).unwrap();
        client
            .send(&NetworkMessage::Transaction(transfer_tx()))
            .unwrap();
        let response = client.receive().unwrap();

        assert_eq!(
            response,
            NetworkMessage::Vote(Vote {
                validator_id: "validator-1".into(),
                height: 1,
                block_hash: "block-a".into(),
            })
        );
        server.join().unwrap();
    }

    #[test]
    fn tcp_protocol_stream_exchanges_peer_hello() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut server = TcpProtocolStream::from_stream(stream);
            let remote = server
                .handshake(PeerHello::new(
                    "validator-1",
                    "detta-testnet",
                    [PeerRole::Validator],
                ))
                .unwrap();

            assert_eq!(remote.peer_id, "full-node-1");
            assert!(remote.roles.contains(&PeerRole::FullNode));
        });

        let mut client = TcpProtocolStream::connect(addr).unwrap();
        let remote = client
            .handshake(PeerHello::new(
                "full-node-1",
                "detta-testnet",
                [PeerRole::FullNode],
            ))
            .unwrap();

        assert_eq!(remote.peer_id, "validator-1");
        assert!(remote.roles.contains(&PeerRole::Validator));
        server.join().unwrap();
    }
}
