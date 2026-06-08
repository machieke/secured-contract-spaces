use detta_core::{
    Amount, AssetId, Block, BlockError, ContractId, ContractRecord, Event, GrantKey, MempoolError,
    OutboxMessageProof, Principal, Receipt, RegistryNonInclusionProof, RegistryProof, StateKey,
    StateSnapshot, StorageNonInclusionProof, StorageProof, Transaction, ValidatorNode,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RpcError {
    Mempool(MempoolError),
    Block(BlockError),
    BlockNotFound,
    ReceiptNotFound,
    TransactionNotFound,
    ContractNotFound,
    ProofNotFound,
}

pub struct RpcService {
    node: ValidatorNode,
}

impl RpcService {
    pub fn new(node: ValidatorNode) -> Self {
        Self { node }
    }

    pub fn node(&self) -> &ValidatorNode {
        &self.node
    }

    pub fn submit_transaction(&mut self, tx: Transaction) -> Result<(), RpcError> {
        self.node.submit_transaction(tx).map_err(RpcError::Mempool)
    }

    pub fn produce_block(&mut self, height: u64, timestamp: u64) -> Result<Block, RpcError> {
        let block = self.node.propose_pending_block(height, timestamp);
        self.node
            .validate_and_apply(&block)
            .map_err(RpcError::Block)?;
        Ok(block)
    }

    pub fn import_block(&mut self, block: &Block) -> Result<(), RpcError> {
        self.node.validate_and_apply(block).map_err(RpcError::Block)
    }

    pub fn get_transaction(&self, tx_hash: &str) -> Result<Transaction, RpcError> {
        self.node
            .get_transaction(tx_hash)
            .cloned()
            .ok_or(RpcError::TransactionNotFound)
    }

    pub fn get_receipt(&self, tx_hash: &str) -> Result<Receipt, RpcError> {
        self.node
            .get_receipt(tx_hash)
            .cloned()
            .ok_or(RpcError::ReceiptNotFound)
    }

    pub fn get_block(&self, height: u64) -> Result<Block, RpcError> {
        self.node
            .get_block(height)
            .cloned()
            .ok_or(RpcError::BlockNotFound)
    }

    pub fn get_state_root(&self) -> String {
        self.node.state().global_state_root()
    }

    pub fn snapshot(&self) -> StateSnapshot {
        self.node.state().snapshot()
    }

    pub fn call_balance_view(
        &self,
        contract: impl Into<ContractId>,
        owner: impl Into<Principal>,
        asset: impl Into<AssetId>,
    ) -> Amount {
        self.node.state().balance(contract, owner, asset)
    }

    pub fn call_total_supply_view(
        &self,
        contract: impl Into<ContractId>,
        asset: impl Into<AssetId>,
    ) -> Amount {
        self.node.state().total_supply(contract, asset)
    }

    pub fn get_storage_proof(&self, key: &StateKey) -> Result<StorageProof, RpcError> {
        self.node
            .state()
            .storage_proof(key)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_storage_non_inclusion_proof(
        &self,
        key: &StateKey,
    ) -> Result<StorageNonInclusionProof, RpcError> {
        self.node
            .state()
            .storage_non_inclusion_proof(key)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_registry_proof(&self, key: &GrantKey) -> Result<RegistryProof, RpcError> {
        self.node
            .state()
            .registry_proof(key)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_registry_non_inclusion_proof(
        &self,
        key: &GrantKey,
    ) -> Result<RegistryNonInclusionProof, RpcError> {
        self.node
            .state()
            .registry_non_inclusion_proof(key)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_outbox_message_proof(&self, index: usize) -> Result<OutboxMessageProof, RpcError> {
        self.node
            .state()
            .outbox_message_proof(index)
            .ok_or(RpcError::ProofNotFound)
    }

    pub fn get_events(&self) -> Vec<Event> {
        self.node.state().events().to_vec()
    }

    pub fn get_contract(
        &self,
        contract: impl Into<ContractId>,
    ) -> Result<ContractRecord, RpcError> {
        self.node
            .state()
            .contract(contract)
            .cloned()
            .ok_or(RpcError::ContractNotFound)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use detta_core::{Argument, ContractInvariant, DeTTaState, Method, TxStatus};

    fn seeded_rpc() -> RpcService {
        let mut state = DeTTaState::new("detta-local");
        state
            .deploy_token(
                "TokenA",
                "USDC",
                vec![("Alice".into(), 100), ("Bob".into(), 50)],
            )
            .unwrap();
        RpcService::new(ValidatorNode::new("validator-1", state))
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
    fn rpc_submits_transaction_produces_block_and_returns_receipt() {
        let mut rpc = seeded_rpc();
        rpc.submit_transaction(transfer_tx()).unwrap();

        let block = rpc.produce_block(1, 1_000).unwrap();
        let receipt = rpc.get_receipt("tx1").unwrap();

        assert_eq!(block.transactions.len(), 1);
        assert_eq!(receipt.status, TxStatus::Committed);
        assert_eq!(rpc.call_balance_view("TokenA", "Alice", "USDC"), 90);
        assert_eq!(rpc.call_balance_view("TokenA", "Bob", "USDC"), 60);
        assert_eq!(rpc.get_transaction("tx1").unwrap().tx_hash, "tx1");
        assert_eq!(rpc.get_block(1).unwrap().header.height, 1);
    }

    #[test]
    fn rpc_returns_storage_proof_and_contract_descriptor() {
        let mut rpc = seeded_rpc();
        rpc.submit_transaction(transfer_tx()).unwrap();
        let block = rpc.produce_block(1, 1_000).unwrap();

        let key = StateKey::Balance {
            contract: "TokenA".into(),
            owner: "Bob".into(),
            asset: "USDC".into(),
        };
        let proof = rpc.get_storage_proof(&key).unwrap();
        let contract = rpc.get_contract("TokenA").unwrap();

        assert!(proof.verify());
        assert_eq!(proof.proof.root, block.header.storage_root);
        assert_eq!(contract.contract_id, "TokenA");
        assert!(contract.method_policy(&Method::Transfer).is_some());
        assert!(contract
            .declared_invariants()
            .contains(&ContractInvariant::TokenSupplyMatchesBalances));
        assert_eq!(rpc.get_state_root(), block.header.global_state_root);
    }
}
