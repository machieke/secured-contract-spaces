use detta_consensus::Vote;
use detta_core::{Block, Method, StateKey, StateValue};
use detta_e2e::client::TcpRpcClient;
use detta_e2e::fixtures::{
    amount, asset, defi_genesis_state, principal, temp_dir, tx_to, TOKEN_CONTRACT, USDC,
};
use detta_e2e::network::spawn_tcp_persistent_node;
use detta_e2e::proofs::assert_storage_proof_matches_root;
use detta_network::{Envelope, NetworkMessage, TcpProtocolStream};
use detta_node::{NetworkIngestOutcome, PersistentValidatorNode};
use detta_protocol::ValidatorSigningKey;
use detta_rpc::{RpcRequest, RpcResult};
use std::net::{SocketAddr, TcpListener};
use std::thread::{self, JoinHandle};

const NETWORK_ID: &str = "detta-testnet";

#[test]
fn signed_tcp_block_votes_and_finality_converge_across_four_validators() {
    let proposer_dir = temp_dir("e2e-signed-consensus-proposer");
    let peer_dirs = [
        temp_dir("e2e-signed-consensus-validator-2"),
        temp_dir("e2e-signed-consensus-validator-3"),
        temp_dir("e2e-signed-consensus-validator-4"),
    ];
    let proposer_key = validator_key("validator-1", 7);
    let peer_keys = [
        validator_key("validator-2", 8),
        validator_key("validator-3", 9),
        validator_key("validator-4", 10),
    ];

    let mut proposer = PersistentValidatorNode::bootstrap(
        "validator-1",
        defi_genesis_state(),
        proposer_dir.path(),
    )
    .unwrap();
    proposer.set_network_id(NETWORK_ID);
    proposer.trust_validator_key(proposer_key.public_key());
    for peer_key in &peer_keys {
        proposer.trust_validator_key(peer_key.public_key());
    }
    proposer
        .submit_transaction(tx_to(
            TOKEN_CONTRACT,
            "e2e-signed-consensus-transfer-1",
            "Alice",
            1,
            Method::Transfer,
            vec![principal("Bob"), asset(USDC), amount(17)],
        ))
        .unwrap();
    let block = proposer.produce_block(1, 1_000).unwrap();
    let block_hash = block.block_hash();
    let signed_block = proposer
        .sign_validator_message(
            &proposer_key,
            NetworkMessage::Block(Box::new(block.clone())),
        )
        .unwrap();

    let mut peers = Vec::new();
    for (index, peer_key) in peer_keys.into_iter().enumerate() {
        peers.push(spawn_signed_voting_peer(
            format!("validator-{}", index + 2),
            peer_key,
            proposer_key.public_key(),
            peer_dirs[index].path().to_path_buf(),
            block.clone(),
        ));
    }

    let mut signed_votes = vec![proposer
        .sign_validator_message(
            &proposer_key,
            NetworkMessage::Vote(Vote {
                validator_id: "validator-1".into(),
                height: block.header.height,
                block_hash: block_hash.clone(),
            }),
        )
        .unwrap()];

    for peer in peers {
        let mut tcp = TcpProtocolStream::connect(peer.addr).unwrap();
        tcp.send(&signed_block).unwrap();
        signed_votes.push(tcp.receive().unwrap());
        drop(tcp);
        peer.join.join().unwrap();
    }

    let certificate = proposer
        .collect_finality_certificate(block.header.height, &block_hash, &signed_votes, 3)
        .unwrap();
    assert_eq!(certificate.height, 1);
    assert_eq!(certificate.block_hash, block_hash);
    assert_eq!(
        certificate.signers,
        vec![
            "validator-1".to_string(),
            "validator-2".to_string(),
            "validator-3".to_string(),
            "validator-4".to_string(),
        ]
    );
    proposer.persist_finality_certificate(&certificate).unwrap();

    let (proposer_rpc_addr, proposer_rpc_server) = spawn_tcp_persistent_node(proposer).unwrap();
    let mut proposer_client = TcpRpcClient::connect(proposer_rpc_addr).unwrap();
    assert_eq!(
        finality_signers(&mut proposer_client, 1),
        certificate.signers
    );
    assert_eq!(balance(&mut proposer_client, "Bob"), 67);
    proposer_client.close();
    proposer_rpc_server.join().unwrap();

    for (index, peer_dir) in peer_dirs.iter().enumerate() {
        let peer =
            PersistentValidatorNode::restart(format!("validator-{}", index + 2), peer_dir.path())
                .unwrap();
        let (peer_rpc_addr, peer_rpc_server) = spawn_tcp_persistent_node(peer).unwrap();
        let mut peer_client = TcpRpcClient::connect(peer_rpc_addr).unwrap();
        assert_eq!(state_root(&mut peer_client), block.header.global_state_root);
        assert_eq!(balance(&mut peer_client, "Bob"), 67);
        assert_storage_value(
            &mut peer_client,
            StateKey::Balance {
                contract: TOKEN_CONTRACT.into(),
                owner: "Bob".into(),
                asset: USDC.into(),
            },
            67,
            &block.header.storage_root,
        );
        peer_client.close();
        peer_rpc_server.join().unwrap();
    }
}

struct VotingPeer {
    addr: SocketAddr,
    join: JoinHandle<()>,
}

fn spawn_signed_voting_peer(
    validator_id: String,
    signing_key: ValidatorSigningKey,
    proposer_public_key: detta_protocol::ValidatorPublicKey,
    storage_root: std::path::PathBuf,
    expected_block: Block,
) -> VotingPeer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let join = thread::spawn(move || {
        let mut peer =
            PersistentValidatorNode::bootstrap(&validator_id, defi_genesis_state(), &storage_root)
                .unwrap();
        peer.set_network_id(NETWORK_ID);
        peer.trust_validator_key(proposer_public_key);
        let (stream, _) = listener.accept().unwrap();
        let mut tcp = TcpProtocolStream::from_stream(stream);
        let envelope = Envelope {
            from: "validator-1".into(),
            to: validator_id.clone(),
            message: tcp.receive().unwrap(),
        };
        assert_eq!(
            peer.ingest_network_envelope(&envelope).unwrap(),
            NetworkIngestOutcome::BlockImported
        );
        assert_eq!(
            peer.load_block(expected_block.header.height)
                .unwrap()
                .block_hash(),
            expected_block.block_hash()
        );
        let vote = Vote {
            validator_id,
            height: expected_block.header.height,
            block_hash: expected_block.block_hash(),
        };
        let signed_vote = peer
            .sign_validator_message(&signing_key, NetworkMessage::Vote(vote))
            .unwrap();
        tcp.send(&signed_vote).unwrap();
    });
    VotingPeer { addr, join }
}

fn finality_signers(client: &mut TcpRpcClient, height: u64) -> Vec<String> {
    match client
        .ok(RpcRequest::GetFinalityCertificate { height })
        .unwrap()
    {
        RpcResult::FinalityCertificate(certificate) => certificate.signers,
        result => panic!("expected finality certificate, got {result:?}"),
    }
}

fn state_root(client: &mut TcpRpcClient) -> String {
    match client.ok(RpcRequest::GetStateRoot).unwrap() {
        RpcResult::StateRoot(root) => root,
        result => panic!("expected state root, got {result:?}"),
    }
}

fn balance(client: &mut TcpRpcClient, owner: &str) -> u128 {
    match client
        .ok(RpcRequest::GetBalance {
            contract: TOKEN_CONTRACT.into(),
            owner: owner.into(),
            asset: USDC.into(),
        })
        .unwrap()
    {
        RpcResult::Amount(amount) => amount,
        result => panic!("expected amount, got {result:?}"),
    }
}

fn assert_storage_value(
    client: &mut TcpRpcClient,
    key: StateKey,
    expected: u128,
    storage_root: &str,
) {
    match client.ok(RpcRequest::GetStorageProof { key }).unwrap() {
        RpcResult::StorageProof(proof) => {
            assert_eq!(proof.value, StateValue::UInt(expected));
            assert_storage_proof_matches_root(&proof, storage_root);
        }
        result => panic!("expected storage proof, got {result:?}"),
    }
}

fn validator_key(validator_id: &str, seed_byte: u8) -> ValidatorSigningKey {
    ValidatorSigningKey::from_seed(validator_id, "consensus-key-1", [seed_byte; 32])
}
