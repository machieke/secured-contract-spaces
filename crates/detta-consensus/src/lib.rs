use detta_core::{Block, BlockError, DeTTaState, Transaction, ValidatorNode};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Vote {
    pub validator_id: String,
    pub height: u64,
    pub block_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FinalityCertificate {
    pub height: u64,
    pub block_hash: String,
    pub signers: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConsensusError {
    UnknownProposer,
    UnknownValidator(String),
    DuplicateValidator(String),
    EmptyValidatorSet,
    QuorumNotReached {
        accepted: usize,
        required: usize,
    },
    InvalidBlock {
        validator: String,
        error: BlockError,
    },
}

pub struct ConsensusCluster {
    validators: BTreeMap<String, ValidatorNode>,
    quorum: usize,
}

impl ConsensusCluster {
    pub fn new(validators: Vec<(String, DeTTaState)>) -> Result<Self, ConsensusError> {
        if validators.is_empty() {
            return Err(ConsensusError::EmptyValidatorSet);
        }

        let quorum = (validators.len() * 2 / 3) + 1;
        let mut nodes = BTreeMap::new();
        for (validator_id, state) in validators {
            if nodes.contains_key(&validator_id) {
                return Err(ConsensusError::DuplicateValidator(validator_id));
            }
            nodes.insert(
                validator_id.clone(),
                ValidatorNode::new(validator_id, state),
            );
        }

        Ok(Self {
            validators: nodes,
            quorum,
        })
    }

    pub fn validator_count(&self) -> usize {
        self.validators.len()
    }

    pub fn quorum(&self) -> usize {
        self.quorum
    }

    pub fn validator(&self, validator_id: &str) -> Option<&ValidatorNode> {
        self.validators.get(validator_id)
    }

    pub fn propose_block(
        &self,
        proposer_id: &str,
        height: u64,
        transactions: Vec<Transaction>,
        timestamp: u64,
    ) -> Result<Block, ConsensusError> {
        let proposer = self
            .validators
            .get(proposer_id)
            .ok_or(ConsensusError::UnknownProposer)?;
        Ok(proposer.propose_block(height, transactions, timestamp))
    }

    pub fn finalize_block(&mut self, block: &Block) -> Result<FinalityCertificate, ConsensusError> {
        let mut votes = Vec::new();
        let block_hash = block.block_hash();

        for (validator_id, validator) in &mut self.validators {
            match validator.validate_and_apply(block) {
                Ok(()) => votes.push(Vote {
                    validator_id: validator_id.clone(),
                    height: block.header.height,
                    block_hash: block_hash.clone(),
                }),
                Err(error) => {
                    return Err(ConsensusError::InvalidBlock {
                        validator: validator_id.clone(),
                        error,
                    });
                }
            }
        }

        Self::certificate_from_votes(block.header.height, &block_hash, votes, self.quorum)
    }

    pub fn certificate_from_votes(
        height: u64,
        block_hash: &str,
        votes: Vec<Vote>,
        quorum: usize,
    ) -> Result<FinalityCertificate, ConsensusError> {
        let mut signers = BTreeSet::new();
        for vote in votes {
            if vote.height == height && vote.block_hash == block_hash {
                signers.insert(vote.validator_id);
            }
        }

        if signers.len() < quorum {
            return Err(ConsensusError::QuorumNotReached {
                accepted: signers.len(),
                required: quorum,
            });
        }

        Ok(FinalityCertificate {
            height,
            block_hash: block_hash.to_string(),
            signers: signers.into_iter().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Argument, DeTTaState, Method, Transaction};

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
    fn consensus_finalizes_block_after_quorum_replay() {
        let mut cluster = ConsensusCluster::new(vec![
            ("v1".into(), seeded_state()),
            ("v2".into(), seeded_state()),
            ("v3".into(), seeded_state()),
        ])
        .unwrap();
        let block = cluster
            .propose_block("v1", 1, vec![transfer_tx()], 1_000)
            .unwrap();

        let certificate = cluster.finalize_block(&block).unwrap();

        assert_eq!(certificate.height, 1);
        assert_eq!(certificate.block_hash, block.block_hash());
        assert_eq!(certificate.signers.len(), 3);
        assert_eq!(
            cluster
                .validator("v2")
                .unwrap()
                .state()
                .balance("TokenA", "Bob", "USDC"),
            60
        );
    }

    #[test]
    fn consensus_rejects_block_with_bad_root() {
        let mut cluster = ConsensusCluster::new(vec![
            ("v1".into(), seeded_state()),
            ("v2".into(), seeded_state()),
            ("v3".into(), seeded_state()),
        ])
        .unwrap();
        let mut block = cluster
            .propose_block("v1", 1, vec![transfer_tx()], 1_000)
            .unwrap();
        block.header.storage_root = "bad-root".into();

        let error = cluster.finalize_block(&block).unwrap_err();

        assert!(matches!(
            error,
            ConsensusError::InvalidBlock {
                error: BlockError::StorageRootMismatch,
                ..
            }
        ));
    }

    #[test]
    fn certificate_requires_quorum_matching_votes() {
        let votes = vec![
            Vote {
                validator_id: "v1".into(),
                height: 1,
                block_hash: "hash-a".into(),
            },
            Vote {
                validator_id: "v2".into(),
                height: 1,
                block_hash: "hash-b".into(),
            },
        ];

        let error = ConsensusCluster::certificate_from_votes(1, "hash-a", votes, 2).unwrap_err();

        assert_eq!(
            error,
            ConsensusError::QuorumNotReached {
                accepted: 1,
                required: 2
            }
        );
    }
}
