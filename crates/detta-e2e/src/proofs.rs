use detta_core::{EventProof, OutboxMessageProof, ReceiptProof, RegistryProof, StorageProof};

pub fn assert_storage_proof_matches_root(proof: &StorageProof, expected_root: &str) {
    assert!(proof.verify(), "storage proof failed local verification");
    assert_eq!(proof.proof.root, expected_root);
}

pub fn assert_registry_proof_matches_root(proof: &RegistryProof, expected_root: &str) {
    assert!(proof.verify(), "registry proof failed local verification");
    assert_eq!(proof.proof.root, expected_root);
}

pub fn assert_receipt_proof_matches_root(proof: &ReceiptProof, expected_root: &str) {
    assert!(proof.verify(), "receipt proof failed local verification");
    assert_eq!(proof.proof.root, expected_root);
}

pub fn assert_event_proof_matches_root(proof: &EventProof, expected_root: &str) {
    assert!(proof.verify(), "event proof failed local verification");
    assert_eq!(proof.proof.root, expected_root);
}

pub fn assert_outbox_proof_matches_root(proof: &OutboxMessageProof, expected_root: &str) {
    assert!(proof.verify(), "outbox proof failed local verification");
    assert_eq!(proof.proof.root, expected_root);
}
