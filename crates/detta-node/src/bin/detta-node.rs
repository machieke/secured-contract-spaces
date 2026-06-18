use detta_core::{Argument, DeTTaState, Method, StateSnapshot, Transaction};
use detta_node::PersistentValidatorNode;
use detta_rpc::{
    HttpJsonRpcServer, JsonRpcHandler, JsonRpcServer, RpcRequest, RpcResponse, RpcResult,
    RpcTransportError,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

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
