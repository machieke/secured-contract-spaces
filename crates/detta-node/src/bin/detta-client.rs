use detta_core::{Argument, Block, Method, Receipt, Transaction};
use detta_rpc::{RpcRequest, RpcResponse, RpcResult};
use detta_storage::DaRetentionClass;
use serde::Serialize;
use std::collections::BTreeMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpStream;
use std::process;

const DEFAULT_RPC_ADDR: &str = "127.0.0.1:8080";
const DEFAULT_CHAIN_ID: &str = "detta-local";
const DEFAULT_FACTORY_CONTRACT: &str = "FactoryA";
const DEFAULT_POOL_CONTRACT: &str = "PoolAB";
const DEFAULT_SENDER: &str = "Alice";
const DEFAULT_BUDGET: u64 = 1_000_000;
const DEFAULT_DA_SHARE_SIZE_BYTES: u32 = 128;

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
    if matches!(command, "-h" | "--help" | "help") {
        println!("{}", usage());
        return Ok(());
    }

    let options = ParsedOptions::parse(&args[2..])?;
    let mut client = TcpRpcClient::connect(&options.value_or("rpc", DEFAULT_RPC_ADDR))?;

    match command {
        "deploy-token" => {
            submit_and_maybe_finalize(&mut client, deploy_token_tx(&options)?, &options)
        }
        "deploy-pool" => {
            submit_and_maybe_finalize(&mut client, deploy_pool_tx(&options)?, &options)
        }
        "add-liquidity" => {
            submit_and_maybe_finalize(&mut client, add_liquidity_tx(&options)?, &options)
        }
        "swap" => submit_and_maybe_finalize(&mut client, swap_tx(&options)?, &options),
        "produce-block" => {
            let height = parse_u64(&options.required("height")?, "height")?;
            let timestamp = options
                .optional("timestamp")
                .map(|value| parse_u64(&value, "timestamp"))
                .transpose()?
                .unwrap_or(height * 1_000);
            let result = client.ok(RpcRequest::ProduceBlock { height, timestamp })?;
            print_json(&result)
        }
        "produce-da-block" => {
            let height = parse_u64(&options.required("height")?, "height")?;
            let timestamp = options
                .optional("timestamp")
                .map(|value| parse_u64(&value, "timestamp"))
                .transpose()?
                .unwrap_or(height * 1_000);
            let share_size_bytes = options
                .optional("share-size")
                .map(|value| parse_u32(&value, "share-size"))
                .transpose()?
                .unwrap_or(DEFAULT_DA_SHARE_SIZE_BYTES);
            let result = client.ok(RpcRequest::ProduceDaBlock {
                height,
                timestamp,
                share_size_bytes,
            })?;
            print_json(&result)
        }
        "receipt" => {
            let result = client.ok(RpcRequest::GetReceipt {
                tx_hash: options.required("tx-hash")?,
            })?;
            print_json(&result)
        }
        "da-manifest" => {
            let result = client.ok(RpcRequest::GetDaManifest {
                manifest_hash: options.required("manifest-hash")?,
            })?;
            print_json(&result)
        }
        "da-share" => {
            let result = client.ok(RpcRequest::GetDaShare {
                manifest_hash: options.required("manifest-hash")?,
                index: parse_u32(&options.required("index")?, "index")?,
            })?;
            print_json(&result)
        }
        "da-certificate" => {
            let result = client.ok(RpcRequest::GetDaCertificate {
                certificate_hash: options.required("certificate-hash")?,
            })?;
            print_json(&result)
        }
        "da-challenge" => {
            let result = client.ok(RpcRequest::GetDaChallengeRecord {
                challenge_id: options.required("challenge-id")?,
            })?;
            print_json(&result)
        }
        "da-payload" => {
            let result = client.ok(RpcRequest::GetDaPayload {
                manifest_hash: options.required("manifest-hash")?,
            })?;
            print_json(&result)
        }
        "da-namespace" => {
            let result = client.ok(RpcRequest::GetDaNamespace {
                manifest_hash: options.required("manifest-hash")?,
                namespace: options.required("namespace")?,
            })?;
            print_json(&result)
        }
        "da-sample-proofs" => {
            let result = client.ok(RpcRequest::GetDaSampleProofs {
                manifest_hash: options.required("manifest-hash")?,
                client_randomness: options.required("client-randomness")?,
                sample_count: parse_u32(&options.required("sample-count")?, "sample-count")?,
                namespaces: parse_csv_option(options.optional("namespaces")),
            })?;
            print_json(&result)
        }
        "da-status" => {
            let result = client.ok(RpcRequest::GetDaStatus {
                manifest_hash: options.required("manifest-hash")?,
            })?;
            print_json(&result)
        }
        "da-repair-status" => {
            let result = client.ok(RpcRequest::GetDaRepairStatus {
                manifest_hash: options.required("manifest-hash")?,
            })?;
            print_json(&result)
        }
        "da-coding-fraud-proof" => {
            let result = client.ok(RpcRequest::GetDaCodingFraudProof {
                manifest_hash: options.required("manifest-hash")?,
            })?;
            print_json(&result)
        }
        "da-stats" => {
            let result = client.ok(RpcRequest::GetDaStorageStats)?;
            print_json(&result)
        }
        "da-retention-audit" => {
            let result = client.ok(RpcRequest::GetDaRetentionAudit)?;
            print_json(&result)
        }
        "da-retention-prune-plan" => {
            let result = client.ok(RpcRequest::GetDaRetentionPrunePlan)?;
            print_json(&result)
        }
        "da-manifest-index-by-height" => {
            let result = client.ok(RpcRequest::GetDaManifestIndexByHeight {
                height: parse_u64(&options.required("height")?, "height")?,
            })?;
            print_json(&result)
        }
        "da-manifest-index-by-block" => {
            let result = client.ok(RpcRequest::GetDaManifestIndexByBlockHash {
                block_hash: options.required("block-hash")?,
            })?;
            print_json(&result)
        }
        "da-manifest-index-by-namespace" => {
            let result = client.ok(RpcRequest::GetDaManifestIndexByNamespace {
                namespace: options.required("namespace")?,
            })?;
            print_json(&result)
        }
        "da-manifest-index-by-retention" => {
            let result = client.ok(RpcRequest::GetDaManifestIndexByRetentionClass {
                class: parse_da_retention_class(&options.required("class")?)?,
            })?;
            print_json(&result)
        }
        "da-certificate-index-by-manifest" => {
            let result = client.ok(RpcRequest::GetDaCertificateIndexByManifest {
                manifest_hash: options.required("manifest-hash")?,
            })?;
            print_json(&result)
        }
        "da-certificate-index-by-height" => {
            let result = client.ok(RpcRequest::GetDaCertificateIndexByHeight {
                height: parse_u64(&options.required("height")?, "height")?,
            })?;
            print_json(&result)
        }
        "da-certificate-index-by-block" => {
            let result = client.ok(RpcRequest::GetDaCertificateIndexByBlockHash {
                block_hash: options.required("block-hash")?,
            })?;
            print_json(&result)
        }
        "state-root" => {
            let result = client.ok(RpcRequest::GetStateRoot)?;
            print_json(&result)
        }
        other => Err(format!("unknown command: {other}\n\n{}", usage())),
    }
}

fn deploy_token_tx(options: &ParsedOptions) -> Result<Transaction, String> {
    let contract = options.required("contract")?;
    let sender = options.value_or("sender", DEFAULT_SENDER);
    let nonce = parse_u64(&options.required("nonce")?, "nonce")?;
    base_tx(
        options,
        "deploy-token",
        &sender,
        nonce,
        options.value_or("factory", DEFAULT_FACTORY_CONTRACT),
        Method::DeployToken,
        vec![
            Argument::Text(contract.clone()),
            Argument::Asset(options.required("asset")?),
            Argument::Principal(options.value_or("initial-holder", &sender)),
            Argument::Amount(parse_u128(
                &options.required("initial-supply")?,
                "initial-supply",
            )?),
        ],
    )
}

fn deploy_pool_tx(options: &ParsedOptions) -> Result<Transaction, String> {
    let contract = options.required("contract")?;
    let sender = options.value_or("sender", DEFAULT_SENDER);
    let nonce = parse_u64(&options.required("nonce")?, "nonce")?;
    base_tx(
        options,
        "deploy-pool",
        &sender,
        nonce,
        options.value_or("factory", DEFAULT_FACTORY_CONTRACT),
        Method::DeployAmmPool,
        vec![
            Argument::Text(contract),
            Argument::Asset(options.required("asset-a")?),
            Argument::Asset(options.required("asset-b")?),
        ],
    )
}

fn add_liquidity_tx(options: &ParsedOptions) -> Result<Transaction, String> {
    let sender = options.value_or("sender", DEFAULT_SENDER);
    let nonce = parse_u64(&options.required("nonce")?, "nonce")?;
    base_tx(
        options,
        "add-liquidity",
        &sender,
        nonce,
        options.value_or("pool", DEFAULT_POOL_CONTRACT),
        Method::AddLiquidity,
        vec![
            Argument::Amount(parse_u128(
                &options.required("asset-a-amount")?,
                "asset-a-amount",
            )?),
            Argument::Amount(parse_u128(
                &options.required("asset-b-amount")?,
                "asset-b-amount",
            )?),
        ],
    )
}

fn swap_tx(options: &ParsedOptions) -> Result<Transaction, String> {
    let sender = options.value_or("sender", DEFAULT_SENDER);
    let nonce = parse_u64(&options.required("nonce")?, "nonce")?;
    base_tx(
        options,
        "swap",
        &sender,
        nonce,
        options.value_or("pool", DEFAULT_POOL_CONTRACT),
        Method::Swap,
        vec![
            Argument::Asset(options.required("input-asset")?),
            Argument::Amount(parse_u128(&options.required("amount-in")?, "amount-in")?),
            Argument::Amount(parse_u128(&options.required("min-output")?, "min-output")?),
        ],
    )
}

fn base_tx(
    options: &ParsedOptions,
    prefix: &str,
    sender: &str,
    nonce: u64,
    target: String,
    method: Method,
    args: Vec<Argument>,
) -> Result<Transaction, String> {
    Ok(Transaction {
        chain_id: options.value_or("chain-id", DEFAULT_CHAIN_ID),
        tx_hash: options
            .optional("tx-hash")
            .unwrap_or_else(|| format!("{prefix}-{sender}-{nonce}")),
        sender: sender.into(),
        nonce,
        valid_until_height: options
            .optional("valid-until-height")
            .map(|value| parse_u64(&value, "valid-until-height"))
            .transpose()?,
        target,
        method,
        args,
        signature_ok: true,
        budget: options
            .optional("budget")
            .map(|value| parse_u64(&value, "budget"))
            .transpose()?
            .unwrap_or(DEFAULT_BUDGET),
    })
}

fn submit_and_maybe_finalize(
    client: &mut TcpRpcClient,
    transaction: Transaction,
    options: &ParsedOptions,
) -> Result<(), String> {
    let tx_hash = transaction.tx_hash.clone();
    let submitted = client.ok(RpcRequest::SubmitTransaction { transaction })?;
    let produced_block = if let Some(height) = options.optional("produce-height") {
        let height = parse_u64(&height, "produce-height")?;
        let timestamp = options
            .optional("produce-timestamp")
            .map(|value| parse_u64(&value, "produce-timestamp"))
            .transpose()?
            .unwrap_or(height * 1_000);
        match client.ok(RpcRequest::ProduceBlock { height, timestamp })? {
            RpcResult::Block(block) => Some(BlockSummary::from_block(&block)),
            result => {
                return Err(format!(
                    "expected produce-block to return block, got {result:?}"
                ));
            }
        }
    } else {
        None
    };
    let receipt = if produced_block.is_some() {
        match client.ok(RpcRequest::GetReceipt { tx_hash })? {
            RpcResult::Receipt(receipt) => Some(*receipt),
            result => return Err(format!("expected receipt result, got {result:?}")),
        }
    } else {
        None
    };

    print_json(&ClientCommandReport {
        submitted,
        produced_block,
        receipt,
    })
}

#[derive(Serialize)]
struct ClientCommandReport {
    submitted: RpcResult,
    produced_block: Option<BlockSummary>,
    receipt: Option<Receipt>,
}

#[derive(Serialize)]
struct BlockSummary {
    height: u64,
    block_hash: String,
    global_state_root: String,
    storage_root: String,
    receipt_root: String,
    transaction_count: usize,
}

impl BlockSummary {
    fn from_block(block: &Block) -> Self {
        Self {
            height: block.header.height,
            block_hash: block.block_hash(),
            global_state_root: block.header.global_state_root.clone(),
            storage_root: block.header.storage_root.clone(),
            receipt_root: block.header.receipt_root.clone(),
            transaction_count: block.transactions.len(),
        }
    }
}

struct TcpRpcClient {
    stream: TcpStream,
    reader: BufReader<TcpStream>,
}

impl TcpRpcClient {
    fn connect(addr: &str) -> Result<Self, String> {
        let stream =
            TcpStream::connect(addr).map_err(|error| format!("RPC connect failed: {error}"))?;
        stream
            .set_nodelay(true)
            .map_err(|error| format!("failed to configure RPC stream: {error}"))?;
        let reader = BufReader::new(
            stream
                .try_clone()
                .map_err(|error| format!("failed to clone RPC stream: {error}"))?,
        );
        Ok(Self { stream, reader })
    }

    fn request(&mut self, request: RpcRequest) -> Result<RpcResponse, String> {
        serde_json::to_writer(&mut self.stream, &request)
            .map_err(|error| format!("failed to encode RPC request: {error}"))?;
        self.stream
            .write_all(b"\n")
            .map_err(|error| format!("failed to write RPC request: {error}"))?;
        self.stream
            .flush()
            .map_err(|error| format!("failed to flush RPC request: {error}"))?;

        let mut line = String::new();
        self.reader
            .read_line(&mut line)
            .map_err(|error| format!("failed to read RPC response: {error}"))?;
        if line.is_empty() {
            return Err("empty RPC response".into());
        }
        serde_json::from_str(&line)
            .map_err(|error| format!("failed to decode RPC response: {error}"))
    }

    fn ok(&mut self, request: RpcRequest) -> Result<RpcResult, String> {
        match self.request(request)? {
            RpcResponse::Ok(result) => Ok(result),
            RpcResponse::Error(error) => {
                Err(format!("RPC error {}: {}", error.code, error.message))
            }
        }
    }
}

struct ParsedOptions {
    values: BTreeMap<String, String>,
}

impl ParsedOptions {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut values = BTreeMap::new();
        let mut index = 0;
        while index < args.len() {
            let raw = &args[index];
            let option = raw
                .strip_prefix("--")
                .ok_or_else(|| format!("unexpected positional argument: {raw}"))?;
            if option.is_empty() {
                return Err("empty option name".into());
            }
            if let Some((key, value)) = option.split_once('=') {
                if key.is_empty() {
                    return Err("empty option name".into());
                }
                values.insert(key.to_string(), value.to_string());
                index += 1;
                continue;
            }
            if matches!(args.get(index + 1), Some(next) if !next.starts_with("--")) {
                values.insert(option.to_string(), args[index + 1].clone());
                index += 2;
            } else {
                return Err(format!("missing value for --{option}"));
            }
        }
        Ok(Self { values })
    }

    fn required(&self, key: &str) -> Result<String, String> {
        self.values
            .get(key)
            .cloned()
            .ok_or_else(|| format!("missing required option --{key}"))
    }

    fn optional(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn value_or(&self, key: &str, default: &str) -> String {
        self.optional(key).unwrap_or_else(|| default.into())
    }
}

fn parse_u64(value: &str, label: &str) -> Result<u64, String> {
    value
        .parse()
        .map_err(|error| format!("invalid --{label}: {error}"))
}

fn parse_u32(value: &str, label: &str) -> Result<u32, String> {
    value
        .parse()
        .map_err(|error| format!("invalid --{label}: {error}"))
}

fn parse_u128(value: &str, label: &str) -> Result<u128, String> {
    value
        .parse()
        .map_err(|error| format!("invalid --{label}: {error}"))
}

fn parse_da_retention_class(value: &str) -> Result<DaRetentionClass, String> {
    match value.to_ascii_lowercase().as_str() {
        "hot" => Ok(DaRetentionClass::Hot),
        "warm" => Ok(DaRetentionClass::Warm),
        "cold" => Ok(DaRetentionClass::Cold),
        "checkpoint" => Ok(DaRetentionClass::Checkpoint),
        _ => Err(format!(
            "invalid --class: expected hot, warm, cold, or checkpoint, got {value}"
        )),
    }
}

fn parse_csv_option(value: Option<String>) -> Vec<String> {
    value
        .into_iter()
        .flat_map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn print_json<T: Serialize>(value: &T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn usage() -> String {
    r#"Usage:
  detta-client deploy-token --contract <id> --asset <symbol> --initial-holder <principal> --initial-supply <amount> --nonce <n> [--produce-height <h>]
  detta-client deploy-pool --contract <id> --asset-a <symbol> --asset-b <symbol> --nonce <n> [--produce-height <h>]
  detta-client add-liquidity --pool <id> --asset-a-amount <amount> --asset-b-amount <amount> --nonce <n> [--produce-height <h>]
  detta-client swap --pool <id> --input-asset <symbol> --amount-in <amount> --min-output <amount> --nonce <n> [--produce-height <h>]
  detta-client produce-block --height <h> [--timestamp <t>]
  detta-client produce-da-block --height <h> [--timestamp <t>] [--share-size <bytes>]
  detta-client receipt --tx-hash <hash>
  detta-client da-manifest --manifest-hash <hash>
  detta-client da-share --manifest-hash <hash> --index <i>
  detta-client da-certificate --certificate-hash <hash>
  detta-client da-challenge --challenge-id <hash>
  detta-client da-payload --manifest-hash <hash>
  detta-client da-namespace --manifest-hash <hash> --namespace <name>
  detta-client da-sample-proofs --manifest-hash <hash> --client-randomness <bytes> --sample-count <n> [--namespaces <csv>]
  detta-client da-status --manifest-hash <hash>
  detta-client da-repair-status --manifest-hash <hash>
  detta-client da-coding-fraud-proof --manifest-hash <hash>
  detta-client da-stats
  detta-client da-retention-audit
  detta-client da-retention-prune-plan
  detta-client da-manifest-index-by-height --height <h>
  detta-client da-manifest-index-by-block --block-hash <hash>
  detta-client da-manifest-index-by-namespace --namespace <name>
  detta-client da-manifest-index-by-retention --class <hot|warm|cold|checkpoint>
  detta-client da-certificate-index-by-manifest --manifest-hash <hash>
  detta-client da-certificate-index-by-height --height <h>
  detta-client da-certificate-index-by-block --block-hash <hash>
  detta-client state-root

Common options:
  --rpc <host:port>               TCP RPC address, default 127.0.0.1:8080
  --chain-id <id>                 Chain id, default detta-local
  --sender <principal>            Transaction sender, default Alice
  --tx-hash <hash>                Transaction hash/id
  --factory <id>                  Factory contract, default FactoryA
  --budget <units>                Transaction budget, default 1000000
  --valid-until-height <height>   Optional expiry height
  --produce-timestamp <time>      Timestamp for --produce-height
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deploy_token_command_builds_factory_transaction() {
        let options = ParsedOptions::parse(&[
            "--contract".into(),
            "ClientToken".into(),
            "--asset".into(),
            "CLT".into(),
            "--initial-holder".into(),
            "Alice".into(),
            "--initial-supply".into(),
            "1000".into(),
            "--sender".into(),
            "Issuer".into(),
            "--nonce".into(),
            "7".into(),
            "--tx-hash".into(),
            "deploy-token-1".into(),
        ])
        .unwrap();
        let tx = deploy_token_tx(&options).unwrap();
        assert_eq!(tx.tx_hash, "deploy-token-1");
        assert_eq!(tx.sender, "Issuer");
        assert_eq!(tx.target, DEFAULT_FACTORY_CONTRACT);
        assert_eq!(tx.method, Method::DeployToken);
        assert_eq!(
            tx.args,
            vec![
                Argument::Text("ClientToken".into()),
                Argument::Asset("CLT".into()),
                Argument::Principal("Alice".into()),
                Argument::Amount(1000)
            ]
        );
    }

    #[test]
    fn swap_command_builds_pool_transaction() {
        let options = ParsedOptions::parse(&[
            "--pool".into(),
            "ClientPool".into(),
            "--input-asset".into(),
            "CLT".into(),
            "--amount-in".into(),
            "15".into(),
            "--min-output".into(),
            "8".into(),
            "--sender".into(),
            "Bob".into(),
            "--nonce".into(),
            "2".into(),
        ])
        .unwrap();
        let tx = swap_tx(&options).unwrap();
        assert_eq!(tx.sender, "Bob");
        assert_eq!(tx.target, "ClientPool");
        assert_eq!(tx.method, Method::Swap);
        assert_eq!(
            tx.args,
            vec![
                Argument::Asset("CLT".into()),
                Argument::Amount(15),
                Argument::Amount(8)
            ]
        );
    }

    #[test]
    fn parses_da_retention_class_names() {
        assert_eq!(
            parse_da_retention_class("hot").unwrap(),
            DaRetentionClass::Hot
        );
        assert_eq!(
            parse_da_retention_class("Checkpoint").unwrap(),
            DaRetentionClass::Checkpoint
        );
        assert!(parse_da_retention_class("archive").is_err());
    }

    #[test]
    fn parses_optional_csv_namespaces() {
        assert_eq!(
            parse_csv_option(Some("detta.tx, detta.receipt,,detta.block".into())),
            vec![
                "detta.tx".to_string(),
                "detta.receipt".to_string(),
                "detta.block".to_string()
            ]
        );
        assert!(parse_csv_option(None).is_empty());
    }
}
