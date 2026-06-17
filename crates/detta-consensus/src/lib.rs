use detta_core::{
    receipt_root, transaction_root, Block, BlockError, DeTTaState, Receipt, Transaction,
    ValidatorNode,
};
use detta_da::{
    DaAvailabilityCertificate, DaAvailabilityVote, DaChallengeEvidence, DaManifest, DaPayload,
    DaRecord,
};
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalityMode {
    Legacy,
    DataAvailabilityRequired,
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
    pub evidence: SlashingEvidence,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum SlashingEvidence {
    Equivocation(EquivocationEvidence),
    DataAvailability(DaChallengeEvidence),
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
    FinalityHeightMismatch {
        expected: u64,
        actual: u64,
    },
    FinalityBlockHashMismatch {
        expected: String,
        actual: String,
    },
    MissingDataAvailabilityCommitment,
    MissingDataAvailabilityCertificate,
    MissingDataAvailabilityCertificateHash,
    DataAvailabilityInvalid(String),
    DataAvailabilityMismatch {
        field: &'static str,
        expected: String,
        actual: String,
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
    finality_mode: FinalityMode,
}

impl ConsensusCluster {
    pub fn new(validators: Vec<(String, DeTTaState)>) -> Result<Self, ConsensusError> {
        Self::new_with_finality_mode(validators, FinalityMode::Legacy)
    }

    pub fn new_with_finality_mode(
        validators: Vec<(String, DeTTaState)>,
        finality_mode: FinalityMode,
    ) -> Result<Self, ConsensusError> {
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
            finality_mode,
        })
    }

    pub fn finality_mode(&self) -> FinalityMode {
        self.finality_mode
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

    pub fn verify_finality_certificate(
        &self,
        certificate: &FinalityCertificate,
        expected_height: u64,
        expected_block_hash: &str,
    ) -> Result<(), ConsensusError> {
        for signer in &certificate.signers {
            if !self.validators.contains_key(signer) {
                return Err(ConsensusError::UnknownValidator(signer.clone()));
            }
            if self.is_slashed(signer) {
                return Err(ConsensusError::SlashedValidator(signer.clone()));
            }
        }

        let active_validators = self
            .validators
            .keys()
            .filter(|validator_id| !self.is_slashed(validator_id))
            .cloned()
            .collect();
        verify_finality_certificate(
            certificate,
            expected_height,
            expected_block_hash,
            &active_validators,
            self.quorum(),
        )
    }

    pub fn verify_data_availability_certificate(
        &self,
        block: &Block,
        manifest: &DaManifest,
        certificate: &DaAvailabilityCertificate,
    ) -> Result<(), ConsensusError> {
        for signer in &certificate.signers {
            if !self.validators.contains_key(signer) {
                return Err(ConsensusError::UnknownValidator(signer.clone()));
            }
            if self.is_slashed(signer) {
                return Err(ConsensusError::SlashedValidator(signer.clone()));
            }
        }

        let active_validators = self
            .validators
            .keys()
            .filter(|validator_id| !self.is_slashed(validator_id))
            .cloned()
            .collect();
        verify_data_availability_certificate(
            block,
            manifest,
            certificate,
            &active_validators,
            self.quorum(),
        )
    }

    pub fn verify_finality_with_data_availability(
        &self,
        block: &Block,
        finality_certificate: &FinalityCertificate,
        da_manifest: &DaManifest,
        da_certificate: &DaAvailabilityCertificate,
    ) -> Result<(), ConsensusError> {
        self.verify_finality_certificate(
            finality_certificate,
            block.header.height,
            &block.block_hash(),
        )?;
        self.verify_data_availability_certificate(block, da_manifest, da_certificate)
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
                evidence: SlashingEvidence::Equivocation(evidence),
            });
        Ok(self
            .slashing_records
            .get(&validator_id)
            .expect("record inserted above"))
    }

    pub fn record_data_availability_fault(
        &mut self,
        evidence: DaChallengeEvidence,
    ) -> Result<&SlashingRecord, ConsensusError> {
        validate_da_result(evidence.validate())?;
        if !self
            .validators
            .contains_key(&evidence.challenged_validator_id)
        {
            return Err(ConsensusError::UnknownValidator(
                evidence.challenged_validator_id,
            ));
        }
        let validator_id = evidence.challenged_validator_id.clone();
        self.slashing_records
            .entry(validator_id.clone())
            .or_insert_with(|| SlashingRecord {
                validator_id: validator_id.clone(),
                slashed_at_height: evidence.observed_at_height,
                evidence: SlashingEvidence::DataAvailability(evidence),
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
        if self.finality_mode == FinalityMode::DataAvailabilityRequired {
            if block.header.data_availability.is_none() {
                return Err(ConsensusError::MissingDataAvailabilityCommitment);
            }
            return Err(ConsensusError::MissingDataAvailabilityCertificate);
        }
        self.finalize_block_after_availability_checks(block)
    }

    fn finalize_block_after_availability_checks(
        &mut self,
        block: &Block,
    ) -> Result<FinalityCertificate, ConsensusError> {
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

    pub fn finalize_block_with_data_availability(
        &mut self,
        block: &Block,
        payload: &DaPayload,
        manifest: &DaManifest,
        certificate: &DaAvailabilityCertificate,
    ) -> Result<FinalityCertificate, ConsensusError> {
        self.verify_data_availability_certificate(block, manifest, certificate)?;
        verify_data_availability_payload(block, payload, manifest)?;
        self.finalize_block_after_availability_checks(block)
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

    pub fn data_availability_certificate_from_active_votes(
        &self,
        manifest: &DaManifest,
        votes: Vec<DaAvailabilityVote>,
    ) -> Result<DaAvailabilityCertificate, ConsensusError> {
        for vote in &votes {
            if !self.validators.contains_key(&vote.validator_id) {
                return Err(ConsensusError::UnknownValidator(vote.validator_id.clone()));
            }
            if self.is_slashed(&vote.validator_id) {
                return Err(ConsensusError::SlashedValidator(vote.validator_id.clone()));
            }
        }

        let active_votes: Vec<_> = votes
            .into_iter()
            .filter(|vote| !self.is_slashed(&vote.validator_id))
            .collect();
        Self::data_availability_certificate_from_votes(manifest, active_votes, self.quorum())
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

    pub fn data_availability_certificate_from_votes(
        manifest: &DaManifest,
        votes: Vec<DaAvailabilityVote>,
        quorum: usize,
    ) -> Result<DaAvailabilityCertificate, ConsensusError> {
        validate_da_result(manifest.validate())?;
        let manifest_hash = validate_da_result(manifest.manifest_hash())?;
        let mut signers = BTreeSet::new();

        for vote in votes {
            validate_da_result(vote.validate())?;
            if vote.chain_id == manifest.chain_id
                && vote.height == manifest.height
                && vote.block_hash == manifest.block_hash
                && vote.manifest_hash == manifest_hash
                && vote.share_root == manifest.share_root
            {
                signers.insert(vote.validator_id);
            }
        }

        if signers.len() < quorum {
            return Err(ConsensusError::QuorumNotReached {
                accepted: signers.len(),
                required: quorum,
            });
        }

        validate_da_result(DaAvailabilityCertificate::from_manifest(manifest, signers))
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

pub fn verify_finality_certificate(
    certificate: &FinalityCertificate,
    expected_height: u64,
    expected_block_hash: &str,
    active_validators: &BTreeSet<String>,
    quorum: usize,
) -> Result<(), ConsensusError> {
    if active_validators.is_empty() || quorum == 0 {
        return Err(ConsensusError::EmptyValidatorSet);
    }

    if certificate.height != expected_height {
        return Err(ConsensusError::FinalityHeightMismatch {
            expected: expected_height,
            actual: certificate.height,
        });
    }
    if certificate.block_hash != expected_block_hash {
        return Err(ConsensusError::FinalityBlockHashMismatch {
            expected: expected_block_hash.into(),
            actual: certificate.block_hash.clone(),
        });
    }

    let mut unique_signers = BTreeSet::new();
    for signer in &certificate.signers {
        if !active_validators.contains(signer) {
            return Err(ConsensusError::UnknownValidator(signer.clone()));
        }
        unique_signers.insert(signer.clone());
    }
    if unique_signers.len() < quorum {
        return Err(ConsensusError::QuorumNotReached {
            accepted: unique_signers.len(),
            required: quorum,
        });
    }

    Ok(())
}

pub fn verify_data_availability_certificate(
    block: &Block,
    manifest: &DaManifest,
    certificate: &DaAvailabilityCertificate,
    active_validators: &BTreeSet<String>,
    quorum: usize,
) -> Result<(), ConsensusError> {
    if active_validators.is_empty() || quorum == 0 {
        return Err(ConsensusError::EmptyValidatorSet);
    }

    let commitment = block
        .header
        .data_availability
        .as_ref()
        .ok_or(ConsensusError::MissingDataAvailabilityCommitment)?;
    validate_da_result(manifest.validate())?;
    validate_da_result(certificate.validate())?;

    let manifest_hash = validate_da_result(manifest.manifest_hash())?;
    let certificate_hash = validate_da_result(certificate.certificate_hash())?;
    let committed_certificate_hash = commitment
        .certificate_hash
        .as_ref()
        .ok_or(ConsensusError::MissingDataAvailabilityCertificateHash)?;

    require_da_match("chain_id", &block.header.chain_id, &manifest.chain_id)?;
    require_da_match(
        "height",
        &block.header.height.to_string(),
        &manifest.height.to_string(),
    )?;
    require_da_match("manifest_hash", &commitment.manifest_hash, &manifest_hash)?;
    require_da_match(
        "payload_root",
        &commitment.payload_root,
        &manifest.payload_hash,
    )?;
    require_da_match("share_root", &commitment.share_root, &manifest.share_root)?;
    require_da_match(
        "certificate_hash",
        committed_certificate_hash,
        &certificate_hash,
    )?;
    require_da_match(
        "execution_block_hash",
        &execution_block_hash_for_da(block),
        &manifest.block_hash,
    )?;
    require_da_match(
        "certificate_chain_id",
        &manifest.chain_id,
        &certificate.chain_id,
    )?;
    require_da_match(
        "certificate_height",
        &manifest.height.to_string(),
        &certificate.height.to_string(),
    )?;
    require_da_match(
        "certificate_block_hash",
        &manifest.block_hash,
        &certificate.block_hash,
    )?;
    require_da_match(
        "certificate_manifest_hash",
        &manifest_hash,
        &certificate.manifest_hash,
    )?;
    require_da_match(
        "certificate_share_root",
        &manifest.share_root,
        &certificate.share_root,
    )?;

    let mut unique_signers = BTreeSet::new();
    for signer in &certificate.signers {
        if !active_validators.contains(signer) {
            return Err(ConsensusError::UnknownValidator(signer.clone()));
        }
        unique_signers.insert(signer.clone());
    }
    if unique_signers.len() < quorum {
        return Err(ConsensusError::QuorumNotReached {
            accepted: unique_signers.len(),
            required: quorum,
        });
    }

    Ok(())
}

pub fn verify_data_availability_payload(
    block: &Block,
    payload: &DaPayload,
    manifest: &DaManifest,
) -> Result<(), ConsensusError> {
    validate_da_result(manifest.validate())?;
    validate_da_result(payload.validate())?;
    let canonical_payload = payload.canonicalized();
    let payload_hash = validate_da_result(canonical_payload.hash())?;
    let namespace_root = validate_da_result(canonical_payload.namespace_root())?;

    require_da_match(
        "payload_chain_id",
        &block.header.chain_id,
        &canonical_payload.chain_id,
    )?;
    require_da_match(
        "payload_height",
        &block.header.height.to_string(),
        &canonical_payload.height.to_string(),
    )?;
    require_da_match(
        "payload_previous_block_hash",
        &block.header.previous_block_hash,
        &canonical_payload.previous_block_hash,
    )?;
    require_da_match("payload_hash", &manifest.payload_hash, &payload_hash)?;
    require_da_match("namespace_root", &manifest.namespace_root, &namespace_root)?;

    let payload_transactions = payload_transactions(&canonical_payload)?;
    let payload_tx_root = transaction_root(&payload_transactions);
    require_da_match("payload_tx_root", &block.header.tx_root, &payload_tx_root)?;
    if payload_transactions != block.transactions {
        return Err(ConsensusError::DataAvailabilityInvalid(
            "payload transactions differ from block transactions".into(),
        ));
    }

    let payload_receipts = payload_receipts(&canonical_payload)?;
    let payload_receipt_root = receipt_root(&payload_receipts);
    require_da_match(
        "payload_receipt_root",
        &block.header.receipt_root,
        &payload_receipt_root,
    )?;
    if payload_receipts != block.receipts {
        return Err(ConsensusError::DataAvailabilityInvalid(
            "payload receipts differ from block receipts".into(),
        ));
    }

    Ok(())
}

fn payload_transactions(payload: &DaPayload) -> Result<Vec<Transaction>, ConsensusError> {
    let mut transactions = Vec::new();
    for section in &payload.namespaces {
        if section.namespace.0 != "detta.tx" {
            continue;
        }
        for record in &section.records {
            match record {
                DaRecord::SignedTransaction(tx) => transactions.push(tx.clone()),
                other => {
                    return Err(ConsensusError::DataAvailabilityInvalid(format!(
                        "detta.tx contains non-transaction record {other:?}"
                    )));
                }
            }
        }
    }
    Ok(transactions)
}

fn payload_receipts(payload: &DaPayload) -> Result<Vec<Receipt>, ConsensusError> {
    let mut receipts = Vec::new();
    for section in &payload.namespaces {
        if section.namespace.0 != "detta.receipt" {
            continue;
        }
        for record in &section.records {
            match record {
                DaRecord::Receipt(receipt) => receipts.push(receipt.clone()),
                other => {
                    return Err(ConsensusError::DataAvailabilityInvalid(format!(
                        "detta.receipt contains non-receipt record {other:?}"
                    )));
                }
            }
        }
    }
    Ok(receipts)
}

fn execution_block_hash_for_da(block: &Block) -> String {
    let mut execution_block = block.clone();
    execution_block.header.data_availability = None;
    execution_block.block_hash()
}

fn require_da_match(
    field: &'static str,
    expected: &str,
    actual: &str,
) -> Result<(), ConsensusError> {
    if expected != actual {
        return Err(ConsensusError::DataAvailabilityMismatch {
            field,
            expected: expected.to_string(),
            actual: actual.to_string(),
        });
    }
    Ok(())
}

fn validate_da_result<T, E: std::fmt::Debug>(result: Result<T, E>) -> Result<T, ConsensusError> {
    result.map_err(|error| ConsensusError::DataAvailabilityInvalid(format!("{error:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Argument, DataAvailabilityCommitment, DeTTaState, Method, Transaction};
    use detta_da::{
        DaAvailabilityCertificate, DaAvailabilityVote, DaChallengeEvidence, DaChallengeFault,
        DaManifest, DaNamespace, DaNamespaceSection, DaPayload, DaRecord, DaShareSet,
        DA_CHALLENGE_EVIDENCE_SCHEMA,
    };

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

    fn da_certified_block(
        signers: Vec<&str>,
    ) -> (Block, DaPayload, DaManifest, DaAvailabilityCertificate) {
        let (mut block, _) =
            seeded_state().build_block(1, vec![transfer_tx()], 1_000, "v1", "sim-cert:v1:1");
        let payload =
            da_payload_for_test_block(&block, block.transactions.clone(), block.receipts.clone());
        let (manifest, certificate) = attach_da_commitment(&mut block, &payload, signers);
        (block, payload, manifest, certificate)
    }

    fn attach_da_commitment(
        block: &mut Block,
        payload: &DaPayload,
        signers: Vec<&str>,
    ) -> (DaManifest, DaAvailabilityCertificate) {
        let mut execution_block = block.clone();
        execution_block.header.data_availability = None;
        let execution_block_hash = execution_block.block_hash();
        let share_set = DaShareSet::from_payload(payload, &execution_block_hash, 128).unwrap();
        let certificate = DaAvailabilityCertificate::from_manifest(
            &share_set.manifest,
            signers.into_iter().map(String::from),
        )
        .unwrap();
        let manifest_hash = share_set.manifest.manifest_hash().unwrap();
        let certificate_hash = certificate.certificate_hash().unwrap();
        block.header.data_availability = Some(DataAvailabilityCommitment {
            payload_root: share_set.manifest.payload_hash.clone(),
            manifest_hash,
            share_root: share_set.manifest.share_root.clone(),
            certificate_hash: Some(certificate_hash),
        });
        (share_set.manifest, certificate)
    }

    fn da_payload_for_test_block(
        block: &Block,
        transactions: Vec<Transaction>,
        receipts: Vec<Receipt>,
    ) -> DaPayload {
        DaPayload::new(
            block.header.chain_id.clone(),
            block.header.height,
            block.header.previous_block_hash.clone(),
            vec![
                DaNamespaceSection::new(
                    DaNamespace::new("detta.block").unwrap(),
                    vec![DaRecord::BlockHeader(Box::new(block.header.clone()))],
                )
                .unwrap(),
                DaNamespaceSection::new(
                    DaNamespace::new("detta.tx").unwrap(),
                    transactions
                        .into_iter()
                        .map(DaRecord::SignedTransaction)
                        .collect(),
                )
                .unwrap(),
                DaNamespaceSection::new(
                    DaNamespace::new("detta.receipt").unwrap(),
                    receipts.into_iter().map(DaRecord::Receipt).collect(),
                )
                .unwrap(),
            ],
        )
        .unwrap()
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
                evidence: SlashingEvidence::Equivocation(EquivocationEvidence {
                    validator_id: "v1".into(),
                    height: 1,
                    first_block_hash: "hash-a".into(),
                    second_block_hash: "hash-b".into(),
                }),
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
    fn light_client_verifies_finality_certificate_from_active_validator_set() {
        let active_validators =
            BTreeSet::from(["v1".to_string(), "v2".to_string(), "v3".to_string()]);
        let certificate = FinalityCertificate {
            height: 7,
            block_hash: "block-hash-7".into(),
            signers: vec!["v1".into(), "v3".into()],
        };

        assert_eq!(
            verify_finality_certificate(&certificate, 7, "block-hash-7", &active_validators, 2),
            Ok(())
        );
    }

    #[test]
    fn light_client_rejects_invalid_finality_certificates() {
        let active_validators =
            BTreeSet::from(["v1".to_string(), "v2".to_string(), "v3".to_string()]);
        let certificate = FinalityCertificate {
            height: 7,
            block_hash: "block-hash-7".into(),
            signers: vec!["v1".into(), "v2".into()],
        };

        assert_eq!(
            verify_finality_certificate(&certificate, 8, "block-hash-7", &active_validators, 2),
            Err(ConsensusError::FinalityHeightMismatch {
                expected: 8,
                actual: 7,
            })
        );
        assert_eq!(
            verify_finality_certificate(&certificate, 7, "other-hash", &active_validators, 2),
            Err(ConsensusError::FinalityBlockHashMismatch {
                expected: "other-hash".into(),
                actual: "block-hash-7".into(),
            })
        );

        let unknown = FinalityCertificate {
            signers: vec!["v1".into(), "v9".into()],
            ..certificate.clone()
        };
        assert_eq!(
            verify_finality_certificate(&unknown, 7, "block-hash-7", &active_validators, 2),
            Err(ConsensusError::UnknownValidator("v9".into()))
        );

        let duplicate = FinalityCertificate {
            signers: vec!["v1".into(), "v1".into()],
            ..certificate
        };
        assert_eq!(
            verify_finality_certificate(&duplicate, 7, "block-hash-7", &active_validators, 2),
            Err(ConsensusError::QuorumNotReached {
                accepted: 1,
                required: 2,
            })
        );

        let empty_active_validators = BTreeSet::new();
        assert_eq!(
            verify_finality_certificate(&duplicate, 7, "block-hash-7", &empty_active_validators, 0),
            Err(ConsensusError::EmptyValidatorSet)
        );
    }

    #[test]
    fn data_availability_certificate_verifier_accepts_header_bound_certificate() {
        let (block, _, manifest, da_certificate) = da_certified_block(vec!["v1", "v2", "v3"]);
        let cluster = ConsensusCluster::new(vec![
            ("v1".into(), seeded_state()),
            ("v2".into(), seeded_state()),
            ("v3".into(), seeded_state()),
        ])
        .unwrap();
        let finality_certificate = FinalityCertificate {
            height: block.header.height,
            block_hash: block.block_hash(),
            signers: vec!["v1".into(), "v2".into(), "v3".into()],
        };

        assert_eq!(
            cluster.verify_finality_with_data_availability(
                &block,
                &finality_certificate,
                &manifest,
                &da_certificate
            ),
            Ok(())
        );
    }

    #[test]
    fn production_da_finality_requires_valid_da_certificate_before_replay() {
        let (block, payload, manifest, da_certificate) = da_certified_block(vec!["v1", "v2", "v3"]);
        let mut cluster = ConsensusCluster::new_with_finality_mode(
            vec![
                ("v1".into(), seeded_state()),
                ("v2".into(), seeded_state()),
                ("v3".into(), seeded_state()),
            ],
            FinalityMode::DataAvailabilityRequired,
        )
        .unwrap();

        let finality_certificate = cluster
            .finalize_block_with_data_availability(&block, &payload, &manifest, &da_certificate)
            .unwrap();

        assert_eq!(finality_certificate.height, block.header.height);
        assert_eq!(finality_certificate.block_hash, block.block_hash());
        assert_eq!(finality_certificate.signers, vec!["v1", "v2", "v3"]);
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
    fn production_da_finality_mode_rejects_plain_finalization() {
        let mut cluster = ConsensusCluster::new_with_finality_mode(
            vec![
                ("v1".into(), seeded_state()),
                ("v2".into(), seeded_state()),
                ("v3".into(), seeded_state()),
            ],
            FinalityMode::DataAvailabilityRequired,
        )
        .unwrap();
        assert_eq!(
            cluster.finality_mode(),
            FinalityMode::DataAvailabilityRequired
        );
        let block = cluster
            .propose_block("v1", 1, vec![transfer_tx()], 1_000)
            .unwrap();

        assert_eq!(
            cluster.finalize_block(&block),
            Err(ConsensusError::MissingDataAvailabilityCommitment)
        );
        assert_eq!(
            cluster
                .validator("v2")
                .unwrap()
                .state()
                .balance("TokenA", "Bob", "USDC"),
            50
        );
    }

    #[test]
    fn production_da_finality_rejects_bad_manifest_before_replay() {
        let (mut block, payload, manifest, da_certificate) =
            da_certified_block(vec!["v1", "v2", "v3"]);
        block
            .header
            .data_availability
            .as_mut()
            .unwrap()
            .manifest_hash = "bad-manifest".into();
        let mut cluster = ConsensusCluster::new_with_finality_mode(
            vec![
                ("v1".into(), seeded_state()),
                ("v2".into(), seeded_state()),
                ("v3".into(), seeded_state()),
            ],
            FinalityMode::DataAvailabilityRequired,
        )
        .unwrap();

        assert!(matches!(
            cluster.finalize_block_with_data_availability(
                &block,
                &payload,
                &manifest,
                &da_certificate
            ),
            Err(ConsensusError::DataAvailabilityMismatch {
                field: "manifest_hash",
                ..
            })
        ));
        assert_eq!(
            cluster
                .validator("v2")
                .unwrap()
                .state()
                .balance("TokenA", "Bob", "USDC"),
            50
        );
    }

    #[test]
    fn production_da_finality_rejects_payload_transaction_root_mismatch() {
        let (mut block, _, _, _) = da_certified_block(vec!["v1", "v2", "v3"]);
        let mut execution_block = block.clone();
        execution_block.header.data_availability = None;
        let mut wrong_transaction = transfer_tx();
        wrong_transaction.tx_hash = "tx-wrong".into();
        wrong_transaction.args = vec![
            Argument::Principal("Bob".into()),
            Argument::Asset("USDC".into()),
            Argument::Amount(11),
        ];
        let payload = da_payload_for_test_block(
            &execution_block,
            vec![wrong_transaction],
            execution_block.receipts.clone(),
        );
        let (manifest, da_certificate) =
            attach_da_commitment(&mut block, &payload, vec!["v1", "v2", "v3"]);
        let mut cluster = ConsensusCluster::new_with_finality_mode(
            vec![
                ("v1".into(), seeded_state()),
                ("v2".into(), seeded_state()),
                ("v3".into(), seeded_state()),
            ],
            FinalityMode::DataAvailabilityRequired,
        )
        .unwrap();

        assert!(matches!(
            cluster.finalize_block_with_data_availability(
                &block,
                &payload,
                &manifest,
                &da_certificate
            ),
            Err(ConsensusError::DataAvailabilityMismatch {
                field: "payload_tx_root",
                ..
            })
        ));
        assert_eq!(
            cluster
                .validator("v2")
                .unwrap()
                .state()
                .balance("TokenA", "Bob", "USDC"),
            50
        );
    }

    #[test]
    fn data_availability_verifier_requires_committed_certificate_hash() {
        let (mut block, _, manifest, da_certificate) = da_certified_block(vec!["v1", "v2", "v3"]);
        block
            .header
            .data_availability
            .as_mut()
            .unwrap()
            .certificate_hash = None;
        let active_validators =
            BTreeSet::from(["v1".to_string(), "v2".to_string(), "v3".to_string()]);

        assert_eq!(
            verify_data_availability_certificate(
                &block,
                &manifest,
                &da_certificate,
                &active_validators,
                3
            ),
            Err(ConsensusError::MissingDataAvailabilityCertificateHash)
        );
    }

    #[test]
    fn data_availability_verifier_rejects_bad_manifest_commitment() {
        let (mut block, _, manifest, da_certificate) = da_certified_block(vec!["v1", "v2", "v3"]);
        block
            .header
            .data_availability
            .as_mut()
            .unwrap()
            .manifest_hash = "bad-manifest".into();
        let active_validators =
            BTreeSet::from(["v1".to_string(), "v2".to_string(), "v3".to_string()]);

        assert!(matches!(
            verify_data_availability_certificate(
                &block,
                &manifest,
                &da_certificate,
                &active_validators,
                3
            ),
            Err(ConsensusError::DataAvailabilityMismatch {
                field: "manifest_hash",
                ..
            })
        ));
    }

    #[test]
    fn da_votes_aggregate_only_matching_manifest_commitments() {
        let (_, _, manifest, _) = da_certified_block(vec!["v1", "v2", "v3"]);
        let mut wrong_manifest_vote = DaAvailabilityVote::from_manifest(&manifest, "v3").unwrap();
        wrong_manifest_vote.manifest_hash = "other-manifest".into();
        let votes = vec![
            DaAvailabilityVote::from_manifest(&manifest, "v1").unwrap(),
            DaAvailabilityVote::from_manifest(&manifest, "v2").unwrap(),
            wrong_manifest_vote,
        ];

        let certificate =
            ConsensusCluster::data_availability_certificate_from_votes(&manifest, votes.clone(), 2)
                .unwrap();
        assert_eq!(certificate.signers, vec!["v1", "v2"]);

        assert_eq!(
            ConsensusCluster::data_availability_certificate_from_votes(&manifest, votes, 3),
            Err(ConsensusError::QuorumNotReached {
                accepted: 2,
                required: 3,
            })
        );
    }

    #[test]
    fn cluster_da_certificate_verifier_uses_active_unslashed_validator_set() {
        let (block, _, manifest, da_certificate) = da_certified_block(vec!["v1", "v2", "v3"]);
        let mut cluster = ConsensusCluster::new(vec![
            ("v1".into(), seeded_state()),
            ("v2".into(), seeded_state()),
            ("v3".into(), seeded_state()),
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

        assert_eq!(
            cluster.verify_data_availability_certificate(&block, &manifest, &da_certificate),
            Err(ConsensusError::SlashedValidator("v1".into()))
        );
    }

    #[test]
    fn data_availability_challenge_fault_slashes_validator() {
        let mut cluster = ConsensusCluster::new(vec![
            ("v1".into(), seeded_state()),
            ("v2".into(), seeded_state()),
            ("v3".into(), seeded_state()),
        ])
        .unwrap();
        let evidence = DaChallengeEvidence {
            schema: DA_CHALLENGE_EVIDENCE_SCHEMA.into(),
            schema_version: 1,
            chain_id: "detta-local".into(),
            height: 9,
            block_hash: "block-9".into(),
            manifest_hash: "manifest-9".into(),
            share_root: "share-root-9".into(),
            challenged_validator_id: "v1".into(),
            reporter_id: "v2".into(),
            share_index: 3,
            challenge_hash: "challenge-9".into(),
            response_hash: None,
            observed_at_height: 13,
            fault: DaChallengeFault::MissingResponse,
        };

        let record = cluster
            .record_data_availability_fault(evidence.clone())
            .unwrap()
            .clone();

        assert!(cluster.is_slashed("v1"));
        assert_eq!(cluster.active_validator_count(), 2);
        assert_eq!(cluster.quorum(), 2);
        assert_eq!(
            record,
            SlashingRecord {
                validator_id: "v1".into(),
                slashed_at_height: 13,
                evidence: SlashingEvidence::DataAvailability(evidence),
            }
        );
    }

    #[test]
    fn cluster_finality_verifier_uses_active_unslashed_validator_set() {
        let mut cluster = ConsensusCluster::new(vec![
            ("v1".into(), seeded_state()),
            ("v2".into(), seeded_state()),
            ("v3".into(), seeded_state()),
        ])
        .unwrap();
        let certificate = FinalityCertificate {
            height: 9,
            block_hash: "block-hash-9".into(),
            signers: vec!["v1".into(), "v2".into(), "v3".into()],
        };

        assert_eq!(
            cluster.verify_finality_certificate(&certificate, 9, "block-hash-9"),
            Ok(())
        );

        cluster
            .record_equivocation(EquivocationEvidence {
                validator_id: "v1".into(),
                height: 8,
                first_block_hash: "hash-a".into(),
                second_block_hash: "hash-b".into(),
            })
            .unwrap();

        assert_eq!(
            cluster.verify_finality_certificate(&certificate, 9, "block-hash-9"),
            Err(ConsensusError::SlashedValidator("v1".into()))
        );
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
