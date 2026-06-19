use detta_consensus::{quorum_for, verify_finality_certificate, Vote};
use detta_core::{Argument, DeTTaState, Method, StateSnapshot, Transaction};
use detta_network::{Envelope, NetworkMessage, TcpProtocolStream};
use detta_node::{NetworkIngestOutcome, PersistentValidatorNode};
use detta_protocol::ValidatorSigningKey;
use detta_rpc::{
    HttpJsonRpcServer, JsonRpcHandler, JsonRpcServer, RpcRequest, RpcResponse, RpcResult,
    RpcTransportError,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const CONSENSUS_KEY_ID: &str = "consensus-key-1";

const DEFAULT_CHAIN_ID: &str = "detta-local";
const DEFAULT_VALIDATOR_ID: &str = "validator-1";
const DEFAULT_RPC_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_PRESET: &str = "defi-demo";
const DEFAULT_TOKEN_CONTRACT: &str = "TokenA";
const DEFAULT_SECONDARY_TOKEN_CONTRACT: &str = "TokenB";
const DEFAULT_POOL_CONTRACT: &str = "PoolAB";
const DEFAULT_ORACLE_CONTRACT: &str = "OracleA";
const DEFAULT_VAULT_CONTRACT: &str = "VaultA";
const DEFAULT_STAKE_CONTRACT: &str = "StakeA";
const DEFAULT_GOVERNANCE_CONTRACT: &str = "GovA";
const DEFAULT_BRIDGE_CONTRACT: &str = "BridgeA";
const DEFAULT_ROUTER_CONTRACT: &str = "RouterA";
const DEFAULT_FACTORY_CONTRACT: &str = "FactoryA";
const DEFAULT_ACCOUNT_REGISTRY_CONTRACT: &str = "AccountsA";
const DEFAULT_USDC: &str = "USDC";
const DEFAULT_ATOM: &str = "ATOM";
const DEFAULT_FAUCET: &str = "Faucet";
const DEFAULT_ADMIN: &str = "Admin";
const DEFAULT_REPORTER: &str = "Reporter";

fn main() {
    if let Err(error) = run(env::args().collect()) {
        eprintln!("{error}");
        process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let Some(command) = args.get(1).map(String::as_str) else {
        return Err(usage());
    };
    let options = ParsedOptions::parse(&args[2..])?;

    match command {
        "write-genesis" => write_genesis(&options),
        "serve" => serve_node(&options),
        "faucet-tx" => write_faucet_transaction(&options),
        "-h" | "--help" | "help" => {
            println!("{}", usage());
            Ok(())
        }
        other => Err(format!("unknown command: {other}\n\n{}", usage())),
    }
}

fn write_genesis(options: &ParsedOptions) -> Result<(), String> {
    let output = PathBuf::from(options.required("output")?);
    let chain_id = options.value_or("chain-id", DEFAULT_CHAIN_ID);
    let preset = options.value_or("preset", DEFAULT_PRESET);
    if preset != DEFAULT_PRESET {
        return Err(format!(
            "unsupported genesis preset: {preset}; supported preset: {DEFAULT_PRESET}"
        ));
    }

    let state = demo_defi_genesis_state(&chain_id)?;
    let snapshot = state.snapshot();
    write_json_file(&output, &snapshot)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&GenesisSummary {
            output: output.display().to_string(),
            chain_id: chain_id.to_string(),
            global_state_root: snapshot.global_state_root,
            storage_root: snapshot.storage_root,
            registry_root: snapshot.registry_root,
            policy_root: snapshot.policy_root,
        })
        .map_err(|error| error.to_string())?
    );
    Ok(())
}

fn serve_node(options: &ParsedOptions) -> Result<(), String> {
    let storage = PathBuf::from(options.required("storage")?);
    let validator_id = options.value_or("validator-id", DEFAULT_VALIDATOR_ID);
    let rpc_addr = options.value_or("rpc", DEFAULT_RPC_ADDR);
    let transport = options.value_or("transport", "tcp");
    let max_connections = options
        .optional("max-connections")
        .map(|value| parse_usize(&value, "max-connections"))
        .transpose()?
        .unwrap_or(0);
    let node = load_or_bootstrap_node(&validator_id, &storage, options.optional("genesis"))?;
    let node = Arc::new(Mutex::new(node));
    let root = node
        .lock()
        .expect("node mutex poisoned")
        .rpc()
        .get_state_root();

    spawn_consensus_threads(&node, &validator_id);
    let mut handler = SharedNodeHandler(node);

    match transport.as_str() {
        "tcp" => {
            let server = JsonRpcServer::bind(&rpc_addr).map_err(|error| format!("{error:?}"))?;
            let local_addr = server.local_addr().map_err(|error| format!("{error:?}"))?;
            eprintln!(
                "detta-node serving tcp rpc on {local_addr}; validator={validator_id}; state_root={root}"
            );
            serve_tcp_connections(&server, &mut handler, max_connections)
        }
        "http" => {
            let server =
                HttpJsonRpcServer::bind(&rpc_addr).map_err(|error| format!("{error:?}"))?;
            let local_addr = server.local_addr().map_err(|error| format!("{error:?}"))?;
            eprintln!(
                "detta-node serving http rpc on {local_addr}; validator={validator_id}; state_root={root}"
            );
            serve_http_connections(&server, &mut handler, max_connections)
        }
        other => Err(format!(
            "unsupported transport: {other}; supported transports: tcp, http"
        )),
    }
}

/// Shares one node across the RPC-serving loop and the background consensus
/// threads. The mutex is locked only while a request is handled, so accepting a
/// connection never blocks the producer/sync threads.
struct SharedNodeHandler(Arc<Mutex<PersistentValidatorNode>>);

impl JsonRpcHandler for SharedNodeHandler {
    fn handle_json_request(&mut self, request: &[u8]) -> Result<Vec<u8>, RpcTransportError> {
        self.0
            .lock()
            .expect("node mutex poisoned")
            .handle_json_request(request)
    }
}

/// Start the optional background consensus threads selected by environment:
///
/// * `DETTA_PRODUCE_INTERVAL_SECS` — leader role: produce a block every N
///   seconds, advancing the chain from this node's mempool.
/// * `DETTA_SYNC_PEER` (`host:port`) + `DETTA_SYNC_INTERVAL_SECS` — follower
///   role: pull and verify-import blocks from a peer until heights match.
///
/// A node may be a leader, a follower, both, or neither (plain RPC server).
fn spawn_consensus_threads(node: &Arc<Mutex<PersistentValidatorNode>>, validator_id: &str) {
    // BFT mode replaces the plain leader/follower replication with signed
    // proposals, signed votes, and quorum finality certificates.
    if let Some(config) = BftConfig::from_env(validator_id) {
        spawn_bft_threads(node, config);
        return;
    }

    if let Some(secs) = env_u64("DETTA_PRODUCE_INTERVAL_SECS").filter(|secs| *secs > 0) {
        let node = Arc::clone(node);
        let validator_id = validator_id.to_string();
        eprintln!("[{validator_id}] leader: producing a block every {secs}s");
        thread::spawn(move || run_leader_production(node, Duration::from_secs(secs), validator_id));
    }
    if let Some(peer) = env::var("DETTA_SYNC_PEER")
        .ok()
        .filter(|peer| !peer.is_empty())
    {
        let secs = env_u64("DETTA_SYNC_INTERVAL_SECS").unwrap_or(2).max(1);
        let node = Arc::clone(node);
        let validator_id = validator_id.to_string();
        eprintln!("[{validator_id}] follower: syncing from {peer} every {secs}s");
        thread::spawn(move || {
            run_follower_sync(node, peer, Duration::from_secs(secs), validator_id)
        });
    }
}

/// Leader loop: produce a block per interval. Produces from this node's mempool
/// (empty blocks still advance height so followers stay converged).
fn run_leader_production(
    node: Arc<Mutex<PersistentValidatorNode>>,
    interval: Duration,
    validator_id: String,
) {
    loop {
        thread::sleep(interval);
        let mut guard = node.lock().expect("node mutex poisoned");
        let height = guard.current_height() + 1;
        let timestamp = height.saturating_mul(1_000);
        match guard.produce_block(height, timestamp) {
            Ok(block) => eprintln!(
                "[{validator_id}] produced block height={height} hash={}",
                block.block_hash()
            ),
            Err(error) => eprintln!("[{validator_id}] produce height={height} failed: {error:?}"),
        }
    }
}

/// Follower loop: catch up to the peer's height by fetching and verify-importing
/// each missing block. `import_block` re-executes and checks roots, so a forged
/// or divergent block from the peer is rejected.
fn run_follower_sync(
    node: Arc<Mutex<PersistentValidatorNode>>,
    peer: String,
    interval: Duration,
    validator_id: String,
) {
    loop {
        thread::sleep(interval);
        let peer_height = match peer_rpc(&peer, &RpcRequest::GetNodeHealth) {
            Ok(RpcResponse::Ok(RpcResult::NodeHealth(report))) => report.height,
            Ok(_) => continue,
            Err(error) => {
                eprintln!("[{validator_id}] sync: {peer} health failed: {error}");
                continue;
            }
        };
        loop {
            let local_height = node.lock().expect("node mutex poisoned").current_height();
            if local_height >= peer_height {
                break;
            }
            let next = local_height + 1;
            let block = match peer_rpc(&peer, &RpcRequest::GetBlock { height: next }) {
                Ok(RpcResponse::Ok(RpcResult::Block(block))) => *block,
                Ok(_) => break, // peer does not have this height yet
                Err(error) => {
                    eprintln!("[{validator_id}] sync: fetch height={next} failed: {error}");
                    break;
                }
            };
            let mut guard = node.lock().expect("node mutex poisoned");
            match guard.import_block(&block) {
                Ok(()) => eprintln!("[{validator_id}] imported block height={next}"),
                Err(error) => {
                    eprintln!("[{validator_id}] import height={next} failed: {error:?}");
                    break;
                }
            }
        }
    }
}

/// Minimal line-framed JSON-RPC client for peer calls (matches the TCP server's
/// `request + "\n"` framing). One connection per request keeps it stateless.
fn peer_rpc(addr: &str, request: &RpcRequest) -> Result<RpcResponse, String> {
    let stream = TcpStream::connect(addr).map_err(|error| format!("connect {addr}: {error}"))?;
    stream.set_nodelay(true).ok();
    let mut writer = stream
        .try_clone()
        .map_err(|error| format!("clone stream: {error}"))?;
    let mut reader = BufReader::new(stream);
    serde_json::to_writer(&mut writer, request).map_err(|error| format!("encode: {error}"))?;
    writer
        .write_all(b"\n")
        .map_err(|error| format!("write: {error}"))?;
    writer.flush().map_err(|error| format!("flush: {error}"))?;
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .map_err(|error| format!("read: {error}"))?;
    if line.is_empty() {
        return Err("empty response".into());
    }
    serde_json::from_str(&line).map_err(|error| format!("decode: {error}"))
}

fn env_u64(key: &str) -> Option<u64> {
    env::var(key).ok().and_then(|value| value.parse().ok())
}

// ---------------------------------------------------------------------------
// BFT consensus
//
// A single fixed proposer drives rounds; every validator verifies the proposer's
// signed block, re-executes it (`import_block` checks all roots), and signs a
// vote. The proposer aggregates a quorum of cryptographically verified votes into
// a finality certificate and broadcasts it; followers verify the certificate
// against the active validator set and persist it.
//
// Safety is Byzantine fault tolerant for f < N/3: an invalid block cannot reach a
// quorum of honest votes, and votes/proposals are signature-checked. Liveness is
// single-proposer (no leader rotation / view-change): a crashed proposer halts
// finality until it returns. Validator keys are derived deterministically from a
// shared seed and the roster, so no key files need to be distributed.
// ---------------------------------------------------------------------------

struct BftConfig {
    validator_id: String,
    roster: Vec<String>,
    proposer: String,
    network_id: String,
    seed: Vec<u8>,
    listen: Option<String>,
    peers: Vec<String>,
    round_interval: Duration,
    quorum: usize,
    backfill_peer: Option<String>,
    backfill_interval: Duration,
}

impl BftConfig {
    fn from_env(validator_id: &str) -> Option<Self> {
        let enabled = env::var("DETTA_BFT")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes"))
            .unwrap_or(false);
        if !enabled {
            return None;
        }
        let roster: Vec<String> = split_csv(&env::var("DETTA_VALIDATORS").unwrap_or_default());
        if roster.is_empty() {
            eprintln!("[{validator_id}] DETTA_BFT set but DETTA_VALIDATORS is empty; BFT disabled");
            return None;
        }
        let proposer = env::var("DETTA_PROPOSER").unwrap_or_else(|_| roster[0].clone());
        let quorum = env_u64("DETTA_QUORUM")
            .map(|value| value as usize)
            .unwrap_or_else(|| quorum_for(roster.len()));
        Some(Self {
            validator_id: validator_id.to_string(),
            roster,
            proposer,
            network_id: env::var("DETTA_NETWORK_ID").unwrap_or_else(|_| "detta-local-bft".into()),
            seed: env::var("DETTA_CONSENSUS_SEED")
                .unwrap_or_else(|_| "detta-bft-dev-seed".into())
                .into_bytes(),
            listen: env::var("DETTA_CONSENSUS_LISTEN")
                .ok()
                .filter(|value| !value.is_empty()),
            peers: split_csv(&env::var("DETTA_CONSENSUS_PEERS").unwrap_or_default()),
            round_interval: Duration::from_secs(
                env_u64("DETTA_PRODUCE_INTERVAL_SECS").unwrap_or(5).max(1),
            ),
            quorum,
            backfill_peer: env::var("DETTA_BACKFILL_PEER")
                .ok()
                .filter(|value| !value.is_empty()),
            backfill_interval: Duration::from_secs(
                env_u64("DETTA_SYNC_INTERVAL_SECS").unwrap_or(2).max(1),
            ),
        })
    }

    fn active_validators(&self) -> BTreeSet<String> {
        self.roster.iter().cloned().collect()
    }

    fn own_key(&self) -> ValidatorSigningKey {
        validator_key_for(&self.seed, &self.validator_id)
    }
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|entry| entry.trim().to_string())
        .filter(|entry| !entry.is_empty())
        .collect()
}

/// Deterministic per-validator key seed: `H(domain || master || validator_id)`.
/// `from_seed` derives the keypair from these 32 bytes alone, so distinct seeds
/// per validator are required, and any node with the master seed + roster can
/// recompute every validator's public key.
fn validator_key_for(master: &[u8], validator_id: &str) -> ValidatorSigningKey {
    let mut hasher = Sha256::new();
    hasher.update(b"detta.bft.key.v1");
    hasher.update((master.len() as u64).to_be_bytes());
    hasher.update(master);
    hasher.update(validator_id.as_bytes());
    let seed: [u8; 32] = hasher.finalize().into();
    ValidatorSigningKey::from_seed(validator_id.to_string(), CONSENSUS_KEY_ID, seed)
}

fn spawn_bft_threads(node: &Arc<Mutex<PersistentValidatorNode>>, config: BftConfig) {
    // Install the network id and trust every roster validator's public key.
    {
        let mut guard = node.lock().expect("node mutex poisoned");
        guard.set_network_id(config.network_id.clone());
        for validator in &config.roster {
            guard.trust_validator_key(validator_key_for(&config.seed, validator).public_key());
        }
    }
    eprintln!(
        "[{}] BFT enabled: proposer={} roster={:?} quorum={}",
        config.validator_id, config.proposer, config.roster, config.quorum
    );

    let config = Arc::new(config);
    if config.listen.is_some() {
        let node = Arc::clone(node);
        let config = Arc::clone(&config);
        thread::spawn(move || run_consensus_server(node, config));
    }
    if config.validator_id == config.proposer {
        let node = Arc::clone(node);
        let config = Arc::clone(&config);
        thread::spawn(move || run_proposer_rounds(node, config));
    }
    // Catch-up: a validator that restarts or falls behind pulls and
    // verify-imports the blocks it missed from a peer's RPC, so it can resume
    // voting on live proposals instead of stalling on a height gap. State sync
    // is independent of finality; quorum is unaffected while it catches up.
    if let Some(peer) = config.backfill_peer.clone() {
        let node = Arc::clone(node);
        let interval = config.backfill_interval;
        let validator_id = config.validator_id.clone();
        eprintln!("[{validator_id}] BFT catch-up: backfilling missed blocks from {peer}");
        thread::spawn(move || run_follower_sync(node, peer, interval, validator_id));
    }
}

/// Proposer loop: each round produce a block, sign it, collect a quorum of signed
/// votes from peers, then form, persist, and broadcast a finality certificate.
fn run_proposer_rounds(node: Arc<Mutex<PersistentValidatorNode>>, config: Arc<BftConfig>) {
    let key = config.own_key();
    loop {
        thread::sleep(config.round_interval);

        // Produce + sign under the node lock; release it for network I/O.
        let round = {
            let mut guard = node.lock().expect("node mutex poisoned");
            let height = guard.current_height() + 1;
            let block = match guard.produce_block(height, height.saturating_mul(1_000)) {
                Ok(block) => block,
                Err(error) => {
                    eprintln!(
                        "[{}] produce height={height} failed: {error:?}",
                        config.validator_id
                    );
                    continue;
                }
            };
            let block_hash = block.block_hash();
            let signed_block =
                match guard.sign_validator_message(&key, NetworkMessage::Block(Box::new(block))) {
                    Ok(message) => message,
                    Err(error) => {
                        eprintln!("[{}] sign block failed: {error:?}", config.validator_id);
                        continue;
                    }
                };
            let self_vote = match guard.sign_validator_message(
                &key,
                NetworkMessage::Vote(Vote {
                    validator_id: config.validator_id.clone(),
                    height,
                    block_hash: block_hash.clone(),
                }),
            ) {
                Ok(message) => message,
                Err(error) => {
                    eprintln!("[{}] sign vote failed: {error:?}", config.validator_id);
                    continue;
                }
            };
            (height, block_hash, signed_block, self_vote)
        };
        let (height, block_hash, signed_block, self_vote) = round;

        let mut votes = vec![self_vote];
        for peer in &config.peers {
            match TcpProtocolStream::connect(peer) {
                Ok(mut tcp) => {
                    if tcp.send(&signed_block).is_ok() {
                        match tcp.receive() {
                            Ok(vote) => votes.push(vote),
                            Err(error) => {
                                eprintln!(
                                    "[{}] no vote from {peer}: {error:?}",
                                    config.validator_id
                                )
                            }
                        }
                    }
                }
                Err(_) => eprintln!(
                    "[{}] peer {peer} unreachable this round",
                    config.validator_id
                ),
            }
        }

        let certificate = {
            let mut guard = node.lock().expect("node mutex poisoned");
            match guard.collect_finality_certificate(height, &block_hash, &votes, config.quorum) {
                Ok(certificate) => {
                    if let Err(error) = guard.persist_finality_certificate(&certificate) {
                        eprintln!(
                            "[{}] persist certificate failed: {error:?}",
                            config.validator_id
                        );
                    }
                    Some(certificate)
                }
                Err(error) => {
                    eprintln!(
                        "[{}] height={height} no quorum ({}/{} votes): {error:?}",
                        config.validator_id,
                        votes.len(),
                        config.quorum
                    );
                    None
                }
            }
        };

        if let Some(certificate) = certificate {
            eprintln!(
                "[{}] finalized height={height} signers={:?}",
                config.validator_id, certificate.signers
            );
            for peer in &config.peers {
                if let Ok(mut tcp) = TcpProtocolStream::connect(peer) {
                    let _ = tcp.send(&NetworkMessage::FinalityCertificate(certificate.clone()));
                }
            }
        }
    }
}

/// Follower consensus server: accept a connection, handle one message (a signed
/// block proposal → verify-import and reply with a signed vote; a finality
/// certificate → verify against the active set and persist).
fn run_consensus_server(node: Arc<Mutex<PersistentValidatorNode>>, config: Arc<BftConfig>) {
    let listen = config.listen.clone().expect("consensus listen address");
    let listener = match TcpListener::bind(&listen) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!(
                "[{}] consensus listen on {listen} failed: {error}",
                config.validator_id
            );
            return;
        }
    };
    eprintln!("[{}] consensus server on {listen}", config.validator_id);
    let key = config.own_key();
    let active = config.active_validators();
    for incoming in listener.incoming() {
        let stream = match incoming {
            Ok(stream) => stream,
            Err(_) => continue,
        };
        if let Err(error) = handle_consensus_connection(&node, &config, &key, &active, stream) {
            eprintln!(
                "[{}] consensus connection error: {error}",
                config.validator_id
            );
        }
    }
}

fn handle_consensus_connection(
    node: &Arc<Mutex<PersistentValidatorNode>>,
    config: &BftConfig,
    key: &ValidatorSigningKey,
    active: &BTreeSet<String>,
    stream: TcpStream,
) -> Result<(), String> {
    let mut tcp = TcpProtocolStream::from_stream(stream);
    let message = tcp
        .receive()
        .map_err(|error| format!("receive: {error:?}"))?;

    if let NetworkMessage::FinalityCertificate(certificate) = &message {
        let mut guard = node.lock().expect("node mutex poisoned");
        let matches_local = guard
            .load_block(certificate.height)
            .map(|block| block.block_hash() == certificate.block_hash)
            .unwrap_or(false);
        if matches_local
            && verify_finality_certificate(
                certificate,
                certificate.height,
                &certificate.block_hash,
                active,
                config.quorum,
            )
            .is_ok()
        {
            guard
                .persist_finality_certificate(certificate)
                .map_err(|error| format!("persist certificate: {error:?}"))?;
            eprintln!(
                "[{}] persisted finality height={} signers={:?}",
                config.validator_id, certificate.height, certificate.signers
            );
        }
        return Ok(());
    }

    // Otherwise treat the message as a signed block proposal: ingest verifies the
    // proposer's signature and re-executes the block before it is accepted.
    let envelope = Envelope {
        from: config.proposer.clone(),
        to: config.validator_id.clone(),
        message,
    };
    let outcome = {
        let mut guard = node.lock().expect("node mutex poisoned");
        match guard.ingest_network_envelope(&envelope) {
            Ok(outcome) => outcome,
            // A height gap (this validator is behind / just restarted) makes the
            // proposal unimportable right now. Skip this vote quietly; the
            // catch-up thread backfills the missed blocks and the validator
            // resumes voting on later proposals.
            Err(error) => {
                eprintln!(
                    "[{}] skipping proposal, catching up ({error:?})",
                    config.validator_id
                );
                return Ok(());
            }
        }
    };
    if outcome != NetworkIngestOutcome::BlockImported {
        return Ok(());
    }

    let signed_vote = {
        let guard = node.lock().expect("node mutex poisoned");
        let height = guard.current_height();
        let block = guard
            .load_block(height)
            .map_err(|error| format!("load_block: {error:?}"))?;
        let vote = Vote {
            validator_id: config.validator_id.clone(),
            height,
            block_hash: block.block_hash(),
        };
        guard
            .sign_validator_message(key, NetworkMessage::Vote(vote))
            .map_err(|error| format!("sign vote: {error:?}"))?
    };
    tcp.send(&signed_vote)
        .map_err(|error| format!("send vote: {error:?}"))?;
    Ok(())
}

fn write_faucet_transaction(options: &ParsedOptions) -> Result<(), String> {
    let to = options.required("to")?;
    let amount = parse_u128(&options.required("amount")?, "amount")?;
    let nonce = options
        .optional("nonce")
        .map(|value| parse_u64(&value, "nonce"))
        .transpose()?
        .unwrap_or(1);
    let tx_hash = options
        .optional("tx-hash")
        .unwrap_or_else(|| format!("faucet-{to}-{nonce}"));
    let valid_until_height = options
        .optional("valid-until-height")
        .map(|value| parse_u64(&value, "valid-until-height"))
        .transpose()?;
    let budget = options
        .optional("budget")
        .map(|value| parse_u64(&value, "budget"))
        .transpose()?
        .unwrap_or(1_000_000);

    let transaction = Transaction {
        chain_id: options.value_or("chain-id", DEFAULT_CHAIN_ID),
        tx_hash,
        sender: options.value_or("faucet", DEFAULT_FAUCET),
        nonce,
        valid_until_height,
        target: options.value_or("token", DEFAULT_TOKEN_CONTRACT),
        method: Method::Transfer,
        args: vec![
            Argument::Principal(to),
            Argument::Asset(options.value_or("asset", DEFAULT_USDC)),
            Argument::Amount(amount),
        ],
        signature_ok: true,
        budget,
    };

    println!(
        "{}",
        serde_json::to_string_pretty(&transaction).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn load_or_bootstrap_node(
    validator_id: &str,
    storage: &Path,
    genesis: Option<String>,
) -> Result<PersistentValidatorNode, String> {
    if storage.join("latest_snapshot.bin").exists() {
        return PersistentValidatorNode::restart(validator_id, storage)
            .map_err(|error| format!("failed to restart persistent node: {error:?}"));
    }

    let genesis = genesis.ok_or_else(|| {
        "missing --genesis; it is required when --storage does not contain a snapshot".to_string()
    })?;
    let snapshot: StateSnapshot = read_json_file(Path::new(&genesis))?;
    let state = DeTTaState::from_snapshot(snapshot)
        .map_err(|error| format!("invalid genesis snapshot: {error:?}"))?;
    PersistentValidatorNode::bootstrap(validator_id, state, storage)
        .map_err(|error| format!("failed to bootstrap persistent node: {error:?}"))
}

fn serve_tcp_connections(
    server: &JsonRpcServer,
    handler: &mut SharedNodeHandler,
    max_connections: usize,
) -> Result<(), String> {
    let mut served = 0;
    while max_connections == 0 || served < max_connections {
        server
            .serve_next_connection_with_handler(handler)
            .map_err(|error| format!("{error:?}"))?;
        served += 1;
    }
    Ok(())
}

fn serve_http_connections(
    server: &HttpJsonRpcServer,
    handler: &mut SharedNodeHandler,
    max_connections: usize,
) -> Result<(), String> {
    let mut served = 0;
    while max_connections == 0 || served < max_connections {
        server
            .serve_next_connection_with_handler(handler)
            .map_err(|error| format!("{error:?}"))?;
        served += 1;
    }
    Ok(())
}

fn demo_defi_genesis_state(chain_id: &str) -> Result<DeTTaState, String> {
    let mut state = DeTTaState::new(chain_id);
    state
        .deploy_token(
            DEFAULT_TOKEN_CONTRACT,
            DEFAULT_USDC,
            vec![
                ("Alice".into(), 200),
                ("Bob".into(), 50),
                (DEFAULT_FAUCET.into(), 1_000_000),
                (DEFAULT_VAULT_CONTRACT.into(), 1_000),
                ("Liquidator".into(), 1_000),
            ],
        )
        .map_err(|error| format!("failed to deploy token: {error:?}"))?;
    state
        .deploy_token(
            DEFAULT_SECONDARY_TOKEN_CONTRACT,
            DEFAULT_ATOM,
            vec![("Alice".into(), 1_000), ("Bob".into(), 1_000)],
        )
        .map_err(|error| format!("failed to deploy secondary token: {error:?}"))?;
    state
        .deploy_amm_pool(DEFAULT_POOL_CONTRACT, DEFAULT_USDC, DEFAULT_ATOM)
        .map_err(|error| format!("failed to deploy AMM pool: {error:?}"))?;
    state
        .deploy_oracle(DEFAULT_ORACLE_CONTRACT, DEFAULT_ATOM, DEFAULT_REPORTER, 20)
        .map_err(|error| format!("failed to deploy oracle: {error:?}"))?;
    state
        .deploy_lending_vault(
            DEFAULT_VAULT_CONTRACT,
            DEFAULT_ATOM,
            DEFAULT_USDC,
            DEFAULT_ORACLE_CONTRACT,
            5_000,
            20,
        )
        .map_err(|error| format!("failed to deploy lending vault: {error:?}"))?;
    state
        .deploy_staking_with_rewards(DEFAULT_STAKE_CONTRACT, DEFAULT_ATOM, 2, 1)
        .map_err(|error| format!("failed to deploy staking: {error:?}"))?;
    state
        .deploy_governance_with_timelock(
            DEFAULT_GOVERNANCE_CONTRACT,
            DEFAULT_TOKEN_CONTRACT,
            DEFAULT_ADMIN,
            2,
        )
        .map_err(|error| format!("failed to deploy governance: {error:?}"))?;
    state
        .deploy_bridge_with_validator_set(
            DEFAULT_BRIDGE_CONTRACT,
            "SourceChain",
            vec![
                "source-validator-1".into(),
                "source-validator-2".into(),
                "source-validator-3".into(),
            ],
            2,
        )
        .map_err(|error| format!("failed to deploy bridge: {error:?}"))?;
    state
        .deploy_router(DEFAULT_ROUTER_CONTRACT, DEFAULT_TOKEN_CONTRACT)
        .map_err(|error| format!("failed to deploy router: {error:?}"))?;
    state
        .deploy_factory(DEFAULT_FACTORY_CONTRACT)
        .map_err(|error| format!("failed to deploy factory: {error:?}"))?;
    state
        .deploy_account_registry(DEFAULT_ACCOUNT_REGISTRY_CONTRACT)
        .map_err(|error| format!("failed to deploy account registry: {error:?}"))?;
    Ok(state)
}

fn read_json_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, String> {
    let file =
        File::open(path).map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    serde_json::from_reader(file)
        .map_err(|error| format!("failed to decode {}: {error}", path.display()))
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let file = File::create(path)
        .map_err(|error| format!("failed to create {}: {error}", path.display()))?;
    serde_json::to_writer_pretty(file, value)
        .map_err(|error| format!("failed to encode {}: {error}", path.display()))
}

fn parse_u64(value: &str, name: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|_| format!("invalid --{name}: expected u64, got {value}"))
}

fn parse_usize(value: &str, name: &str) -> Result<usize, String> {
    value
        .parse()
        .map_err(|_| format!("invalid --{name}: expected usize, got {value}"))
}

fn parse_u128(value: &str, name: &str) -> Result<u128, String> {
    value
        .parse()
        .map_err(|_| format!("invalid --{name}: expected u128, got {value}"))
}

fn usage() -> String {
    r#"Usage:
  detta-node write-genesis --output PATH [--chain-id detta-local] [--preset defi-demo]
  detta-node serve --storage DIR --genesis PATH [--validator-id validator-1] [--rpc 127.0.0.1:8080] [--transport tcp|http] [--max-connections N]
  detta-node faucet-tx --to ACCOUNT --amount AMOUNT [--chain-id detta-local] [--token TokenA] [--asset USDC] [--faucet Faucet] [--nonce N] [--tx-hash HASH] [--valid-until-height H]

Notes:
  --max-connections 0, the default, serves forever.
  serve restarts from --storage when latest_snapshot.bin exists; --genesis is only used for first boot.
"#
    .to_string()
}

#[derive(Clone, Debug, Default)]
struct ParsedOptions {
    values: BTreeMap<String, String>,
    flags: BTreeSet<String>,
}

impl ParsedOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut parsed = ParsedOptions::default();
        let mut index = 0;
        while index < args.len() {
            let option = &args[index];
            let Some(name) = option.strip_prefix("--") else {
                return Err(format!("expected option starting with --, got {option}"));
            };
            if name.is_empty() {
                return Err("empty option name".into());
            }
            if index + 1 < args.len() && !args[index + 1].starts_with("--") {
                parsed
                    .values
                    .insert(name.to_string(), args[index + 1].clone());
                index += 2;
            } else {
                parsed.flags.insert(name.to_string());
                index += 1;
            }
        }
        Ok(parsed)
    }

    fn required(&self, name: &str) -> Result<String, String> {
        self.values
            .get(name)
            .cloned()
            .ok_or_else(|| format!("missing required --{name}"))
    }

    fn optional(&self, name: &str) -> Option<String> {
        self.values.get(name).cloned()
    }

    fn value_or(&self, name: &str, default: &str) -> String {
        self.values
            .get(name)
            .cloned()
            .unwrap_or_else(|| default.to_string())
    }
}

#[derive(Serialize)]
struct GenesisSummary {
    output: String,
    chain_id: String,
    global_state_root: String,
    storage_root: String,
    registry_root: String,
    policy_root: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_genesis_builds_valid_snapshot_with_faucet_balance() {
        let state = demo_defi_genesis_state("detta-test").unwrap();
        let snapshot = state.snapshot();
        assert_eq!(snapshot.state.chain_id(), "detta-test");
        assert!(!snapshot.global_state_root.is_empty());
        assert_eq!(
            state.balance(DEFAULT_TOKEN_CONTRACT, DEFAULT_FAUCET, DEFAULT_USDC),
            1_000_000
        );
    }

    #[test]
    fn parsed_options_reject_positional_arguments() {
        let args = vec!["positional".to_string()];
        assert!(ParsedOptions::parse(&args).is_err());
    }

    #[test]
    fn faucet_transaction_shape_is_transfer() {
        let options = ParsedOptions::parse(&[
            "--to".into(),
            "Alice".into(),
            "--amount".into(),
            "25".into(),
        ])
        .unwrap();
        let to = options.required("to").unwrap();
        let transaction = Transaction {
            chain_id: options.value_or("chain-id", DEFAULT_CHAIN_ID),
            tx_hash: "faucet-test".into(),
            sender: options.value_or("faucet", DEFAULT_FAUCET),
            nonce: 1,
            valid_until_height: None,
            target: options.value_or("token", DEFAULT_TOKEN_CONTRACT),
            method: Method::Transfer,
            args: vec![
                Argument::Principal(to),
                Argument::Asset(options.value_or("asset", DEFAULT_USDC)),
                Argument::Amount(
                    parse_u128(&options.required("amount").unwrap(), "amount").unwrap(),
                ),
            ],
            signature_ok: true,
            budget: 1_000_000,
        };

        assert_eq!(transaction.sender, DEFAULT_FAUCET);
        assert_eq!(transaction.target, DEFAULT_TOKEN_CONTRACT);
        assert_eq!(transaction.method, Method::Transfer);
    }
}
