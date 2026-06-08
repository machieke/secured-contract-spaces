use detta_consensus::{EquivocationEvidence, FinalityCertificate, ValidatorSetUpdate, Vote};
use detta_core::{Block, Transaction};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type PeerId = String;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum NetworkMessage {
    Transaction(Transaction),
    Block(Box<Block>),
    Vote(Vote),
    FinalityCertificate(FinalityCertificate),
    ValidatorSetUpdate(ValidatorSetUpdate),
    EquivocationEvidence(EquivocationEvidence),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Envelope {
    pub from: PeerId,
    pub to: PeerId,
    pub message: NetworkMessage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum NetworkError {
    DuplicatePeer(PeerId),
    UnknownPeer(PeerId),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InMemoryTransport {
    inboxes: BTreeMap<PeerId, Vec<Envelope>>,
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
        inbox.push(Envelope { from, to, message });
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
        let inbox = self
            .inboxes
            .get_mut(peer)
            .ok_or_else(|| NetworkError::UnknownPeer(peer.to_string()))?;
        Ok(std::mem::take(inbox))
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

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Argument, DeTTaState, Method, ValidatorNode};

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
}
