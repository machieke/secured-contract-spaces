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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EquivocationEvidence {
    pub validator_id: String,
    pub height: u64,
    pub first_block_hash: String,
    pub second_block_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SlashingRecord {
    pub validator_id: String,
    pub slashed_at_height: u64,
    pub evidence: EquivocationEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ValidatorSetUpdate {
    pub update_id: String,
    pub add_validators: Vec<(String, DeTTaState)>,
    pub remove_validators: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConsensusError {
    UnknownProposer,
    UnknownValidator(String),
    SlashedValidator(String),
    DuplicateValidator(String),
    EmptyValidatorSet,
    ValidatorSetWouldBeEmpty,
    ValidatorSetUpdateAlreadyApplied(String),
    QuorumNotReached {
        accepted: usize,
        required: usize,
    },
    InvalidBlock {
        validator: String,
        error: BlockError,
    },
    Equivocation(EquivocationEvidence),
}

pub struct ConsensusCluster {
    validators: BTreeMap<String, ValidatorNode>,
    slashing_records: BTreeMap<String, SlashingRecord>,
    applied_validator_set_updates: BTreeSet<String>,
}

impl ConsensusCluster {
    pub fn new(validators: Vec<(String, DeTTaState)>) -> Result<Self, ConsensusError> {
        if validators.is_empty() {
            return Err(ConsensusError::EmptyValidatorSet);
        }

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
            slashing_records: BTreeMap::new(),
            applied_validator_set_updates: BTreeSet::new(),
        })
    }

    pub fn validator_count(&self) -> usize {
        self.validators.len()
    }

    pub fn active_validator_count(&self) -> usize {
        self.validators.len() - self.slashing_records.len()
    }

    pub fn quorum(&self) -> usize {
        quorum_for(self.active_validator_count())
    }

    pub fn validator(&self, validator_id: &str) -> Option<&ValidatorNode> {
        self.validators.get(validator_id)
    }

    pub fn is_slashed(&self, validator_id: &str) -> bool {
        self.slashing_records.contains_key(validator_id)
    }

    pub fn slashing_record(&self, validator_id: &str) -> Option<&SlashingRecord> {
        self.slashing_records.get(validator_id)
    }

    pub fn slashing_records(&self) -> impl Iterator<Item = &SlashingRecord> {
        self.slashing_records.values()
    }

    pub fn has_applied_validator_set_update(&self, update_id: &str) -> bool {
        self.applied_validator_set_updates.contains(update_id)
    }

    pub fn record_equivocation(
        &mut self,
        evidence: EquivocationEvidence,
    ) -> Result<&SlashingRecord, ConsensusError> {
        if !self.validators.contains_key(&evidence.validator_id) {
            return Err(ConsensusError::UnknownValidator(evidence.validator_id));
        }
        let validator_id = evidence.validator_id.clone();
        self.slashing_records
            .entry(validator_id.clone())
            .or_insert_with(|| SlashingRecord {
                validator_id: validator_id.clone(),
                slashed_at_height: evidence.height,
                evidence,
            });
        Ok(self
            .slashing_records
            .get(&validator_id)
            .expect("record inserted above"))
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
        if self.is_slashed(proposer_id) {
            return Err(ConsensusError::SlashedValidator(proposer_id.to_string()));
        }
        Ok(proposer.propose_block(height, transactions, timestamp))
    }

    pub fn finalize_block(&mut self, block: &Block) -> Result<FinalityCertificate, ConsensusError> {
        if self.is_slashed(&block.header.proposer) {
            return Err(ConsensusError::SlashedValidator(
                block.header.proposer.clone(),
            ));
        }

        let mut votes = Vec::new();
        let block_hash = block.block_hash();

        for (validator_id, validator) in &mut self.validators {
            if self.slashing_records.contains_key(validator_id) {
                continue;
            }
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

        self.certificate_from_active_votes(block.header.height, &block_hash, votes)
    }

    pub fn certificate_from_active_votes(
        &mut self,
        height: u64,
        block_hash: &str,
        votes: Vec<Vote>,
    ) -> Result<FinalityCertificate, ConsensusError> {
        if let Some(evidence) = Self::detect_equivocation(&votes) {
            self.record_equivocation(evidence.clone())?;
            return Err(ConsensusError::Equivocation(evidence));
        }

        let active_votes: Vec<_> = votes
            .into_iter()
            .filter(|vote| !self.is_slashed(&vote.validator_id))
            .collect();
        Self::certificate_from_votes(height, block_hash, active_votes, self.quorum())
    }

    pub fn apply_validator_set_update(
        &mut self,
        update: ValidatorSetUpdate,
        certificate: &FinalityCertificate,
    ) -> Result<(), ConsensusError> {
        if self
            .applied_validator_set_updates
            .contains(&update.update_id)
        {
            return Err(ConsensusError::ValidatorSetUpdateAlreadyApplied(
                update.update_id,
            ));
        }

        self.require_active_quorum(&certificate.signers)?;
        self.validate_validator_set_update(&update)?;

        for validator_id in &update.remove_validators {
            self.validators.remove(validator_id);
            self.slashing_records.remove(validator_id);
        }
        for (validator_id, state) in update.add_validators {
            self.validators.insert(
                validator_id.clone(),
                ValidatorNode::new(validator_id, state),
            );
        }
        self.applied_validator_set_updates.insert(update.update_id);
        Ok(())
    }

    fn require_active_quorum(&self, signers: &[String]) -> Result<(), ConsensusError> {
        let mut accepted = BTreeSet::new();
        for signer in signers {
            if !self.validators.contains_key(signer) {
                return Err(ConsensusError::UnknownValidator(signer.clone()));
            }
            if self.is_slashed(signer) {
                return Err(ConsensusError::SlashedValidator(signer.clone()));
            }
            accepted.insert(signer.clone());
        }

        let required = self.quorum();
        if accepted.len() < required {
            return Err(ConsensusError::QuorumNotReached {
                accepted: accepted.len(),
                required,
            });
        }
        Ok(())
    }

    fn validate_validator_set_update(
        &self,
        update: &ValidatorSetUpdate,
    ) -> Result<(), ConsensusError> {
        let mut add_ids = BTreeSet::new();
        for (validator_id, _) in &update.add_validators {
            if !add_ids.insert(validator_id.clone()) || self.validators.contains_key(validator_id) {
                return Err(ConsensusError::DuplicateValidator(validator_id.clone()));
            }
        }

        let mut remove_ids = BTreeSet::new();
        for validator_id in &update.remove_validators {
            if !remove_ids.insert(validator_id.clone())
                || !self.validators.contains_key(validator_id)
            {
                return Err(ConsensusError::UnknownValidator(validator_id.clone()));
            }
        }

        let next_validator_count =
            self.validators.len() + update.add_validators.len() - update.remove_validators.len();
        if next_validator_count == 0 {
            return Err(ConsensusError::ValidatorSetWouldBeEmpty);
        }

        Ok(())
    }

    pub fn certificate_from_votes(
        height: u64,
        block_hash: &str,
        votes: Vec<Vote>,
        quorum: usize,
    ) -> Result<FinalityCertificate, ConsensusError> {
        if let Some(evidence) = Self::detect_equivocation(&votes) {
            return Err(ConsensusError::Equivocation(evidence));
        }

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

    pub fn detect_equivocation(votes: &[Vote]) -> Option<EquivocationEvidence> {
        let mut seen = BTreeMap::<(&str, u64), &str>::new();
        for vote in votes {
            let key = (vote.validator_id.as_str(), vote.height);
            if let Some(previous_hash) = seen.get(&key) {
                if *previous_hash != vote.block_hash.as_str() {
                    let (first_block_hash, second_block_hash) =
                        ordered_hash_pair(previous_hash, &vote.block_hash);
                    return Some(EquivocationEvidence {
                        validator_id: vote.validator_id.clone(),
                        height: vote.height,
                        first_block_hash,
                        second_block_hash,
                    });
                }
            } else {
                seen.insert(key, vote.block_hash.as_str());
            }
        }
        None
    }
}

fn ordered_hash_pair(left: &str, right: &str) -> (String, String) {
    if left <= right {
        (left.to_string(), right.to_string())
    } else {
        (right.to_string(), left.to_string())
    }
}

pub fn quorum_for(active_validators: usize) -> usize {
    if active_validators == 0 {
        0
    } else {
        (active_validators * 2 / 3) + 1
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
        assert_eq!(cluster.validator_count(), 3);
        assert_eq!(cluster.active_validator_count(), 3);
        assert_eq!(cluster.quorum(), 3);
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

    #[test]
    fn detects_validator_equivocation_at_same_height() {
        let votes = vec![
            Vote {
                validator_id: "v1".into(),
                height: 7,
                block_hash: "hash-b".into(),
            },
            Vote {
                validator_id: "v2".into(),
                height: 7,
                block_hash: "hash-b".into(),
            },
            Vote {
                validator_id: "v1".into(),
                height: 7,
                block_hash: "hash-a".into(),
            },
        ];

        assert_eq!(
            ConsensusCluster::detect_equivocation(&votes),
            Some(EquivocationEvidence {
                validator_id: "v1".into(),
                height: 7,
                first_block_hash: "hash-a".into(),
                second_block_hash: "hash-b".into(),
            })
        );
    }

    #[test]
    fn certificate_rejects_equivocating_votes_before_quorum() {
        let votes = vec![
            Vote {
                validator_id: "v1".into(),
                height: 1,
                block_hash: "hash-a".into(),
            },
            Vote {
                validator_id: "v1".into(),
                height: 1,
                block_hash: "hash-b".into(),
            },
            Vote {
                validator_id: "v2".into(),
                height: 1,
                block_hash: "hash-a".into(),
            },
        ];

        let error = ConsensusCluster::certificate_from_votes(1, "hash-a", votes, 2).unwrap_err();

        assert_eq!(
            error,
            ConsensusError::Equivocation(EquivocationEvidence {
                validator_id: "v1".into(),
                height: 1,
                first_block_hash: "hash-a".into(),
                second_block_hash: "hash-b".into(),
            })
        );
    }

    #[test]
    fn cluster_records_slashing_for_equivocation() {
        let mut cluster = ConsensusCluster::new(vec![
            ("v1".into(), seeded_state()),
            ("v2".into(), seeded_state()),
            ("v3".into(), seeded_state()),
        ])
        .unwrap();
        let votes = vec![
            Vote {
                validator_id: "v1".into(),
                height: 1,
                block_hash: "hash-a".into(),
            },
            Vote {
                validator_id: "v1".into(),
                height: 1,
                block_hash: "hash-b".into(),
            },
            Vote {
                validator_id: "v2".into(),
                height: 1,
                block_hash: "hash-a".into(),
            },
        ];

        let error = cluster
            .certificate_from_active_votes(1, "hash-a", votes)
            .unwrap_err();

        assert!(matches!(error, ConsensusError::Equivocation(_)));
        assert!(cluster.is_slashed("v1"));
        assert_eq!(cluster.active_validator_count(), 2);
        assert_eq!(cluster.quorum(), 2);
        assert_eq!(
            cluster.slashing_record("v1").unwrap(),
            &SlashingRecord {
                validator_id: "v1".into(),
                slashed_at_height: 1,
                evidence: EquivocationEvidence {
                    validator_id: "v1".into(),
                    height: 1,
                    first_block_hash: "hash-a".into(),
                    second_block_hash: "hash-b".into(),
                },
            }
        );
    }

    #[test]
    fn slashed_validator_cannot_propose_or_count_toward_finality() {
        let mut cluster = ConsensusCluster::new(vec![
            ("v1".into(), seeded_state()),
            ("v2".into(), seeded_state()),
            ("v3".into(), seeded_state()),
            ("v4".into(), seeded_state()),
        ])
        .unwrap();
        cluster
            .record_equivocation(EquivocationEvidence {
                validator_id: "v1".into(),
                height: 1,
                first_block_hash: "hash-a".into(),
                second_block_hash: "hash-b".into(),
            })
            .unwrap();

        let propose = cluster.propose_block("v1", 1, vec![transfer_tx()], 1_000);
        assert_eq!(
            propose.unwrap_err(),
            ConsensusError::SlashedValidator("v1".into())
        );

        let insufficient_votes = vec![
            Vote {
                validator_id: "v1".into(),
                height: 2,
                block_hash: "hash-c".into(),
            },
            Vote {
                validator_id: "v2".into(),
                height: 2,
                block_hash: "hash-c".into(),
            },
            Vote {
                validator_id: "v3".into(),
                height: 2,
                block_hash: "hash-c".into(),
            },
        ];
        let error = cluster
            .certificate_from_active_votes(2, "hash-c", insufficient_votes)
            .unwrap_err();
        assert_eq!(
            error,
            ConsensusError::QuorumNotReached {
                accepted: 2,
                required: 3,
            }
        );

        let enough_active_votes = vec![
            Vote {
                validator_id: "v2".into(),
                height: 2,
                block_hash: "hash-c".into(),
            },
            Vote {
                validator_id: "v3".into(),
                height: 2,
                block_hash: "hash-c".into(),
            },
            Vote {
                validator_id: "v4".into(),
                height: 2,
                block_hash: "hash-c".into(),
            },
        ];
        let certificate = cluster
            .certificate_from_active_votes(2, "hash-c", enough_active_votes)
            .unwrap();
        assert_eq!(certificate.signers, vec!["v2", "v3", "v4"]);
    }

    #[test]
    fn quorum_authorizes_validator_set_update() {
        let mut cluster = ConsensusCluster::new(vec![
            ("v1".into(), seeded_state()),
            ("v2".into(), seeded_state()),
            ("v3".into(), seeded_state()),
        ])
        .unwrap();
        let certificate = FinalityCertificate {
            height: 1,
            block_hash: "hash-a".into(),
            signers: vec!["v1".into(), "v2".into(), "v3".into()],
        };
        let update = ValidatorSetUpdate {
            update_id: "validator-update-1".into(),
            add_validators: vec![("v4".into(), seeded_state())],
            remove_validators: vec!["v3".into()],
        };

        cluster
            .apply_validator_set_update(update.clone(), &certificate)
            .unwrap();

        assert!(cluster.has_applied_validator_set_update("validator-update-1"));
        assert_eq!(cluster.validator_count(), 3);
        assert!(cluster.validator("v3").is_none());
        assert!(cluster.validator("v4").is_some());

        let replay = cluster
            .apply_validator_set_update(update, &certificate)
            .unwrap_err();
        assert_eq!(
            replay,
            ConsensusError::ValidatorSetUpdateAlreadyApplied("validator-update-1".into())
        );
    }

    #[test]
    fn validator_set_update_requires_current_quorum() {
        let mut cluster = ConsensusCluster::new(vec![
            ("v1".into(), seeded_state()),
            ("v2".into(), seeded_state()),
            ("v3".into(), seeded_state()),
        ])
        .unwrap();
        let certificate = FinalityCertificate {
            height: 1,
            block_hash: "hash-a".into(),
            signers: vec!["v1".into(), "v2".into()],
        };
        let update = ValidatorSetUpdate {
            update_id: "validator-update-1".into(),
            add_validators: vec![("v4".into(), seeded_state())],
            remove_validators: vec![],
        };

        let error = cluster
            .apply_validator_set_update(update, &certificate)
            .unwrap_err();

        assert_eq!(
            error,
            ConsensusError::QuorumNotReached {
                accepted: 2,
                required: 3,
            }
        );
        assert!(cluster.validator("v4").is_none());
    }

    #[test]
    fn validator_set_update_rejects_invalid_membership_changes() {
        let mut cluster = ConsensusCluster::new(vec![
            ("v1".into(), seeded_state()),
            ("v2".into(), seeded_state()),
            ("v3".into(), seeded_state()),
        ])
        .unwrap();
        let certificate = FinalityCertificate {
            height: 1,
            block_hash: "hash-a".into(),
            signers: vec!["v1".into(), "v2".into(), "v3".into()],
        };

        let duplicate = ValidatorSetUpdate {
            update_id: "validator-update-1".into(),
            add_validators: vec![("v3".into(), seeded_state())],
            remove_validators: vec![],
        };
        assert_eq!(
            cluster
                .apply_validator_set_update(duplicate, &certificate)
                .unwrap_err(),
            ConsensusError::DuplicateValidator("v3".into())
        );

        let empty = ValidatorSetUpdate {
            update_id: "validator-update-2".into(),
            add_validators: vec![],
            remove_validators: vec!["v1".into(), "v2".into(), "v3".into()],
        };
        assert_eq!(
            cluster
                .apply_validator_set_update(empty, &certificate)
                .unwrap_err(),
            ConsensusError::ValidatorSetWouldBeEmpty
        );
    }
}
