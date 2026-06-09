# DeTTa Client Guide: Token Deployment, Liquidity, And Token Sales

This guide shows how an end user or wallet client submits DeTTa RPC requests to:

- deploy a new token;
- create an AMM liquidity pool;
- add liquidity to the pool;
- sell the new token for the paired asset.

The examples use the DeTTa JSON RPC wire format documented in
`detta-rpc-api.md`. They assume the target chain genesis already contains:

- `FactoryA`: the factory contract used to deploy token and AMM pool contracts;
- `AccountsA`: the account-registry contract used for production signer grants;
- `USDC`: an existing quote asset in the testnet or local DeFi genesis.

The example token is `ClientToken`, its asset symbol is `CLT`, and its pool is
`ClientPool` with pair `CLT/USDC`.

## Endpoint And Request Format

Set the RPC endpoint given by the validator or testnet operator:

```sh
export DETTA_RPC_URL=http://127.0.0.1:8080
```

HTTP clients send JSON to `/rpc`:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"get_state_root"}'
```

TCP clients send the same JSON object as one newline-delimited line. The typed
Rust TCP and HTTP client helpers used by the integration tests live in
`crates/detta-e2e/src/client.rs`.

Each transaction uses:

- `chain_id`: the target chain, for example `detta-local`;
- `tx_hash`: a unique client-chosen transaction ID;
- `sender`: the account submitting the transaction;
- `nonce`: the sender nonce, strictly increasing per sender;
- `valid_until_height`: optional expiration height, or `null`;
- `target`: the contract being called;
- `method`: the DeTTa method enum variant, such as `DeployToken`;
- `args`: typed method arguments;
- `budget`: maximum deterministic execution budget.

For local development and test harnesses, the examples use
`submit_transaction` with `signature_ok: true`. Production clients should submit
the same transaction through `submit_signed_transaction`; see
`Production Signing` below.

## Local Block Production

On a local development RPC service, submitted transactions sit in the mempool
until a block is produced. After each submit in this guide, run:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"produce_block","params":{"height":1,"timestamp":1000}}'
```

Increment `height` and `timestamp` for later blocks. On a real validator
network, clients do not call `produce_block`; they wait for consensus finality
and then query `get_receipt`.

## 1. Deploy The Token

The factory token deployment arguments are:

```text
DeployToken(Text(contract_id), Asset(asset_symbol), Principal(initial_holder), Amount(initial_supply))
```

Submit the deployment transaction:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{
    "method": "submit_transaction",
    "params": {
      "transaction": {
        "chain_id": "detta-local",
        "tx_hash": "client-deploy-token-1",
        "sender": "Issuer",
        "nonce": 1,
        "valid_until_height": null,
        "target": "FactoryA",
        "method": "DeployToken",
        "args": [
          {"Text": "ClientToken"},
          {"Asset": "CLT"},
          {"Principal": "Issuer"},
          {"Amount": 1000}
        ],
        "signature_ok": true,
        "budget": 1000000
      }
    }
  }'
```

For a local node, produce block 1:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"produce_block","params":{"height":1,"timestamp":1000}}'
```

Verify the deployment:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"get_receipt","params":{"tx_hash":"client-deploy-token-1"}}'

curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"get_contract","params":{"contract":"ClientToken"}}'

curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"get_total_supply","params":{"contract":"ClientToken","asset":"CLT"}}'

curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"get_balance","params":{"contract":"ClientToken","owner":"Issuer","asset":"CLT"}}'
```

The receipt should be committed, the contract should be a token contract, the
total supply should be `1000`, and `Issuer` should hold the initial `CLT`
balance.

## 2. Create The Liquidity Pool

The factory AMM deployment arguments are:

```text
DeployAmmPool(Text(pool_contract_id), Asset(asset_a), Asset(asset_b))
```

For a `CLT/USDC` pool, submit:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{
    "method": "submit_transaction",
    "params": {
      "transaction": {
        "chain_id": "detta-local",
        "tx_hash": "client-deploy-pool-1",
        "sender": "Issuer",
        "nonce": 2,
        "valid_until_height": null,
        "target": "FactoryA",
        "method": "DeployAmmPool",
        "args": [
          {"Text": "ClientPool"},
          {"Asset": "CLT"},
          {"Asset": "USDC"}
        ],
        "signature_ok": true,
        "budget": 1000000
      }
    }
  }'
```

For a local node, produce block 2:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"produce_block","params":{"height":2,"timestamp":2000}}'
```

Verify the pool contract:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"get_receipt","params":{"tx_hash":"client-deploy-pool-1"}}'

curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"get_contract","params":{"contract":"ClientPool"}}'
```

The pool contract should report AMM assets `CLT` and `USDC`.

## 3. Add Liquidity

Liquidity is added directly to the new pool. The argument order follows the
pool asset order from deployment:

```text
AddLiquidity(Amount(asset_a_amount), Amount(asset_b_amount))
```

For `ClientPool`, asset A is `CLT` and asset B is `USDC`. The sender must have
enough of both assets.

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{
    "method": "submit_transaction",
    "params": {
      "transaction": {
        "chain_id": "detta-local",
        "tx_hash": "client-add-liquidity-1",
        "sender": "Issuer",
        "nonce": 3,
        "valid_until_height": null,
        "target": "ClientPool",
        "method": "AddLiquidity",
        "args": [
          {"Amount": 100},
          {"Amount": 50}
        ],
        "signature_ok": true,
        "budget": 1000000
      }
    }
  }'
```

For a local node, produce block 3:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"produce_block","params":{"height":3,"timestamp":3000}}'
```

Verify the pool reserves:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"get_receipt","params":{"tx_hash":"client-add-liquidity-1"}}'

curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{
    "method": "get_storage_proof",
    "params": {
      "key": {"Reserve": {"contract": "ClientPool", "asset": "CLT"}}
    }
  }'

curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{
    "method": "get_storage_proof",
    "params": {
      "key": {"Reserve": {"contract": "ClientPool", "asset": "USDC"}}
    }
  }'
```

The example reserves should be `100 CLT` and `50 USDC`.

## 4. Sell Tokens

Swaps use:

```text
Swap(Asset(input_asset), Amount(amount_in), Amount(min_output))
```

To sell `CLT` for `USDC`, set `input_asset` to `CLT`. The pool derives the
output asset from the pair. `min_output` is the slippage guard; if the pool
cannot return at least that amount, the transaction reverts.

With reserves of `100 CLT` and `50 USDC`, selling `10 CLT` with the default
30 bps AMM fee returns `4 USDC` using the current integer math. Submit:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{
    "method": "submit_transaction",
    "params": {
      "transaction": {
        "chain_id": "detta-local",
        "tx_hash": "client-sell-clt-1",
        "sender": "Issuer",
        "nonce": 4,
        "valid_until_height": null,
        "target": "ClientPool",
        "method": "Swap",
        "args": [
          {"Asset": "CLT"},
          {"Amount": 10},
          {"Amount": 4}
        ],
        "signature_ok": true,
        "budget": 1000000
      }
    }
  }'
```

For a local node, produce block 4:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"produce_block","params":{"height":4,"timestamp":4000}}'
```

Verify the sale:

```sh
curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"get_receipt","params":{"tx_hash":"client-sell-clt-1"}}'

curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{"method":"get_events_page","params":{"offset":0,"limit":100}}'

curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{
    "method": "get_storage_proof",
    "params": {
      "key": {"Reserve": {"contract": "ClientPool", "asset": "CLT"}}
    }
  }'

curl -s "$DETTA_RPC_URL/rpc" \
  -H "Content-Type: application/json" \
  -d '{
    "method": "get_storage_proof",
    "params": {
      "key": {"Reserve": {"contract": "ClientPool", "asset": "USDC"}}
    }
  }'
```

After the example sale, the reserves should move from `100 CLT / 50 USDC` to
`110 CLT / 46 USDC`, and the events page should include a `Swap` event with
`input_asset` `CLT`, `output_asset` `USDC`, `amount_in` `10`, and `amount_out`
`4`.

## Production Signing

Production clients should not set `signature_ok: true` directly. They should:

1. create the same transaction object with `signature_ok: false`;
2. sign the canonical DeTTa transaction signing payload with Ed25519;
3. ensure the signer's public key is active for `sender` in `AccountsA`;
4. submit the signed envelope through `submit_signed_transaction`.

The signed request shape is:

```json
{
  "method": "submit_signed_transaction",
  "params": {
    "signed": {
      "transaction": {
        "chain_id": "detta-local",
        "tx_hash": "client-sell-clt-1",
        "sender": "Issuer",
        "nonce": 4,
        "valid_until_height": 120,
        "target": "ClientPool",
        "method": "Swap",
        "args": [
          {"Asset": "CLT"},
          {"Amount": 10},
          {"Amount": 4}
        ],
        "signature_ok": false,
        "budget": 1000000
      },
      "public_key_hex": "<32-byte-ed25519-public-key-hex>",
      "signature_hex": "<64-byte-ed25519-signature-hex>"
    }
  }
}
```

If the public key is not registered for the sender, the mempool returns
`mempool.unauthorized_signer`. A sender can register a new signer through
`AccountsA`:

```json
{
  "method": "submit_transaction",
  "params": {
    "transaction": {
      "chain_id": "detta-local",
      "tx_hash": "client-register-key-1",
      "sender": "Issuer",
      "nonce": 5,
      "valid_until_height": null,
      "target": "AccountsA",
      "method": "RegisterAccountKey",
      "args": [
        {"Text": "<32-byte-ed25519-public-key-hex>"}
      ],
      "signature_ok": true,
      "budget": 1000000
    }
  }
}
```

On production networks, this registration transaction should itself be signed
by an already active signer for the account.

## Operational Notes

- Reuse neither `tx_hash` nor `(sender, nonce)`. Duplicate or stale
  transactions are rejected by mempool admission.
- Use `valid_until_height` for production transactions so stale orders cannot
  execute later than intended.
- Keep `min_output` conservative enough to tolerate expected price movement but
  strict enough to protect the user from slippage.
- Query `get_receipt` before assuming a transaction committed. A submitted
  transaction can still revert during deterministic execution.
- Query storage proofs or event proofs when a client needs verifiable state for
  reserves, balances, receipts, or swap events.

The complete tested Rust flow for native token deployment, pool creation,
liquidity, and swaps is in `crates/detta-e2e/tests/factory_client_flows.rs`.
The corresponding aspect-token asset flow is in
`crates/detta-e2e/tests/aspect_amm_client_flows.rs`.
