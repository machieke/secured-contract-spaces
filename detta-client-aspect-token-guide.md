# DeTTa Client Guide: Deploying A MeTTa Aspect Token

This guide shows how a wallet, deployment tool, or test client can deploy and
use a token whose behavior comes from a verified restricted MeTTa aspect module
instead of a hard-coded native token implementation.

The examples use the JSON RPC wire format documented in `detta-rpc-api.md`.
They assume genesis contains:

- `FactoryA`: the factory contract used to submit aspect modules and deploy
  aspect-backed contracts;
- `AccountsA`: the account-registry contract used for production signer grants.

The example deploys the checked-in `ERC20ConformantToken` bundle from
`models/aspects/stdlib/minimal-transfer-token.metta` as `ClientAspectToken`.
That fixture currently contains 14 verified bundles, 20 aspects, 50 callable
projections, 50 ABI entries, 50 method policies, 22 storage schema entries, and
16 invariant definitions. The checked-in artifact and proof-obligation
manifests live next to the source under `models/aspects/stdlib/`.

Available checked-in bundle IDs are:

- `MinimalTransferToken`: transfer, approval, delegated transfer.
- `ERC20ConformantToken`: ERC20-like transfer, approve, transferFrom, permit,
  and initializer projections.
- `FeeToken`: configurable basis-point transfer fee.
- `PausableToken`: admin-controlled pause gate for transfers.
- `RestrictedToken`: blocked-account transfer restriction.
- `LockedToken`: account unlock-height transfer restriction.
- `MintBurnToken`: mint and burn projections.
- `CappedMintToken`: mint cap plus mint projection.
- `VotableToken`: voting-power view backed by balances.
- `SnapshotToken`: balance and total-supply snapshots.
- `VaultShareToken`: checked vault deposit, redeem, and shares view.
- `WrappedToken`: wrap, unwrap, and transfer projections.
- `RewardedStakeToken`: stake, unstake, reward configuration, reward claim,
  and stake view.
- `BridgeMintBurnToken`: bridge certificate-verified mint and burn plus
  transfer.

## Request Format

Local tests can use `submit_transaction` with `signature_ok: true`. Production
clients should use `submit_signed_transaction`: sign the transaction payload
with the sender account key, register that public key through `AccountsA`, and
submit the signed envelope. The signed envelope prevents a node from accepting
forged sender transactions, while the account-registry grant lets keys rotate
without changing account identity.

After each local `submit_transaction`, call `produce_block` with the next
height. On a validator network, clients wait for consensus finality and then
query `get_receipt`, `get_block`, and proof RPCs.

## 1. Submit The Aspect Module

Submit the module source through the factory:

```json
{
  "method": "submit_transaction",
  "params": {
    "transaction": {
      "chain_id": "detta-local",
      "tx_hash": "client-aspect-submit-1",
      "sender": "Issuer",
      "nonce": 1,
      "valid_until_height": null,
      "target": "FactoryA",
      "method": "SubmitAspectModule",
      "args": [
        {"Text": "ERC20ConformantToken"},
        {"Text": "<contents of models/aspects/stdlib/minimal-transfer-token.metta>"}
      ],
      "signature_ok": true,
      "budget": 1000000
    }
  }
}
```

The factory parses, canonicalizes, verifies, and registers the module. A
committed receipt means the module roots were authenticated and the selected
bundle exists in the verified IR.

## 2. Inspect Artifacts And Proofs

List registered modules:

```json
{"method":"get_aspect_modules"}
```

Fetch the artifact report for the returned `module_hash`:

```json
{
  "method": "get_aspect_module_artifacts",
  "params": {"module_hash": "<module_hash>"}
}
```

The report includes:

- authenticated source, IR, ABI, policy, storage-schema, registry-schema, and
  invariant roots;
- `bundle_ids`;
- typed ABI entries;
- method policies with authority, effects, and invariant obligations;
- storage and registry schema maps;
- invariant definitions.

Fetch and verify the module inclusion proof:

```json
{
  "method": "get_aspect_module_proof",
  "params": {"module_hash": "<module_hash>"}
}
```

Clients should verify the returned Merkle proof against the current state root
or a finalized block/snapshot root before trusting artifact contents.

The integration test
`crates/detta-e2e/tests/aspect_amm_client_flows.rs` extends this workflow by
using the deployed aspect-token contract ID as an AMM asset identifier, creating
a pool, adding liquidity, and executing buy and sell swaps through public RPC.

## 3. Deploy The Aspect Token

Deploy a contract that references the registered module and bundle. The fourth
argument is an optional initializer projection; remaining arguments are passed
to that projection.

```json
{
  "method": "submit_transaction",
  "params": {
    "transaction": {
      "chain_id": "detta-local",
      "tx_hash": "client-aspect-deploy-1",
      "sender": "Issuer",
      "nonce": 2,
      "valid_until_height": null,
      "target": "FactoryA",
      "method": "DeployAspectContract",
      "args": [
        {"Text": "ClientAspectToken"},
        {"Text": "<module_hash>"},
        {"Text": "ERC20ConformantToken"},
        {"Text": "ERC20-initialize"},
        {"Principal": "Alice"},
        {"Amount": 100}
      ],
      "signature_ok": true,
      "budget": 1000000
    }
  }
}
```

Verify the deployment:

```json
{"method":"get_receipt","params":{"tx_hash":"client-aspect-deploy-1"}}
{"method":"get_contract","params":{"contract":"ClientAspectToken"}}
```

The contract descriptor should be `AspectModule` and should carry the same
`module_hash`, bundle ID, ABI root, policy root, and invariant root that were
inspected before deployment.

## 4. Transfer Tokens

Call the projected ERC20 transfer method on the deployed aspect contract:

```json
{
  "method": "submit_transaction",
  "params": {
    "transaction": {
      "chain_id": "detta-local",
      "tx_hash": "client-aspect-transfer-1",
      "sender": "Alice",
      "nonce": 1,
      "valid_until_height": null,
      "target": "ClientAspectToken",
      "method": {"Other": "ERC20-transfer"},
      "args": [
        {"Principal": "Bob"},
        {"Amount": 25}
      ],
      "signature_ok": true,
      "budget": 1000000
    }
  }
}
```

Query balances through storage proofs:

```json
{
  "method": "get_storage_proof",
  "params": {
    "key": {
      "AspectState": {
        "contract": "ClientAspectToken",
        "aspect": "StaticBalanceAspect",
        "state": "balanceOf",
        "key": ["Alice"]
      }
    }
  }
}
```

Repeat with `key: ["Bob"]`. Clients should verify the proof root against the
block containing `client-aspect-transfer-1`. For the example above, Alice should
hold `75` and Bob should hold `25`.

## Security Checks For Clients

- Verify `get_aspect_module_artifacts` roots before deployment.
- Verify `get_aspect_module_proof` inclusion against finalized state.
- Check the ABI and method policy for every method the UI exposes.
- Show policy authority and effects to operators before deployment.
- Use `submit_signed_transaction` and registered account keys in production.
- Verify receipts, storage proofs, and event proofs against finalized block or
  snapshot roots before showing settled balances.
