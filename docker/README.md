# DeTTa local network (Docker)

Containerized DeTTa node + a Compose topology that spins up a small fleet of
nodes sharing one genesis: four validators and one RPC/full node.

## What this is

The Compose project runs **Byzantine-fault-tolerant consensus** across four
validators:

1. The **proposer** (`validator-1`) produces a block each round, signs it, and
   sends the signed proposal to every peer.
2. Each **voting validator** (`validator-2..4`) verifies the proposer's
   signature, **re-executes** the block (`import_block` checks every root), and
   replies with its own **signed vote**.
3. The proposer aggregates a **quorum** of cryptographically verified votes into
   a **finality certificate** and broadcasts it; each validator verifies the
   certificate against the active set and persists it.

A non-validating `rpc` node follows the chain over RPC for client queries. All
nodes share one genesis. Validator keys are **derived deterministically** from a
shared `DETTA_CONSENSUS_SEED` and the roster, so no key files are distributed —
any node can recompute every validator's public key.

Verified end to end: all validators converge on the same `state_root`, each
independently persists every finality certificate (4-of-4 signers), and stopping
one validator does not stop finality — the proposer keeps finalizing with a
3-of-4 quorum (`signers=[validator-1, validator-2, validator-3]`).

### Consensus model and its bounds — be precise

- **Safety is Byzantine fault tolerant** for f < N/3 (here N=4, f=1, quorum=3):
  an invalid block cannot collect a quorum of honest votes, and every proposal
  and vote is signature-checked, so honest validators never finalize a bad or
  conflicting block.
- **Liveness is single-proposer.** There is a fixed proposer and **no leader
  rotation / view-change**: if the proposer crashes, finality halts until it
  returns. A restarted *follower* that missed blocks does not auto-catch-up in
  BFT mode (no block backfill on the consensus path). These are deliberate scope
  cuts for a demo, not BFT safety gaps.

For a plainer crash-fault-tolerant (non-BFT) mode — a single leader that
auto-produces and followers that pull+verify-import over RPC — leave
`DETTA_BFT` unset and use `DETTA_PRODUCE_INTERVAL_SECS` / `DETTA_SYNC_PEER`
instead.

### Consensus environment

| Env var | Who | Meaning |
| --- | --- | --- |
| `DETTA_BFT` | all validators | `1` to enable BFT consensus |
| `DETTA_VALIDATORS` | all | comma-separated validator roster |
| `DETTA_PROPOSER` | all | which validator proposes (default: first in roster) |
| `DETTA_CONSENSUS_SEED` | all | shared secret; per-validator keys = `H(seed, id)` |
| `DETTA_NETWORK_ID` | all | signed-message domain separator |
| `DETTA_CONSENSUS_LISTEN` | voters | `host:port` to receive proposals on (e.g. `0.0.0.0:9080`) |
| `DETTA_CONSENSUS_PEERS` | proposer | comma-separated voter consensus addresses |
| `DETTA_PRODUCE_INTERVAL_SECS` | proposer | seconds per consensus round |
| `DETTA_QUORUM` | all | override quorum (default `2N/3 + 1`) |

## Layout

| File | Purpose |
| --- | --- |
| `Dockerfile` | Multi-stage build of `detta-node` + `detta-client` (build context = repo root). |
| `entrypoint.sh` | Env-driven launcher; bootstraps from shared genesis, resumes from storage on restart. |
| `docker-compose.yml` | genesis-init + 4 validators + 1 RPC node + a `client` driver (under the `tools` profile). |

## Quick start

From the repository root:

```sh
# Build images and start the network in the background.
docker compose -f docker/docker-compose.yml up --build -d

# Watch health (nodes become "healthy" once their RPC answers).
docker compose -f docker/docker-compose.yml ps
```

Ports on the host:

| Service | Host port | In-cluster address |
| --- | --- | --- |
| rpc (full node) | `8080` | `rpc:8080` |
| validator-1 | `8081` | `validator-1:8080` |
| validator-2 | `8082` | `validator-2:8080` |
| validator-3 | `8083` | `validator-3:8080` |
| validator-4 | `8084` | `validator-4:8080` |

## Driving the network

The leader produces blocks on its own, so the chain advances and followers
converge without intervention. Use the bundled client via the `tools` profile
(it joins the Compose network, so address nodes by service name):

```sh
# Inspect DA + state on any node.
docker compose -f docker/docker-compose.yml run --rm client da-stats --rpc validator-1:8080
docker compose -f docker/docker-compose.yml run --rm client state-root --rpc rpc:8080

# Produce an extra DA-committed block on demand (in addition to the auto loop).
docker compose -f docker/docker-compose.yml run --rm client \
  produce-da-block --height 1 --rpc validator-1:8080
```

### Verify consensus convergence

Snapshot every node's state root in one client container (fast, so the sample is
taken within a single block interval); all nodes should report the same root:

```sh
docker compose -f docker/docker-compose.yml run --rm --no-deps --entrypoint sh client -c '
for n in validator-1 validator-2 validator-3 validator-4 rpc; do
  detta-client state-root --rpc "$n":8080
done'
```

Watch finality form and propagate:

```sh
docker compose -f docker/docker-compose.yml logs -f validator-1
# [validator-1] finalized height=1 signers=["validator-1","validator-2","validator-3","validator-4"]
docker compose -f docker/docker-compose.yml logs -f validator-2
# [validator-2] persisted finality height=1 signers=[...]
```

Test fault tolerance — stop one validator and confirm finality continues with a
quorum (N=4 tolerates f=1):

```sh
docker compose -f docker/docker-compose.yml stop validator-4
docker compose -f docker/docker-compose.yml logs -f validator-1
# [validator-1] peer validator-4:9080 unreachable this round
# [validator-1] finalized height=N signers=["validator-1","validator-2","validator-3"]
```

From the host you can target the published ports instead (TCP JSON-RPC):

```sh
target/release/detta-client state-root --rpc 127.0.0.1:8080   # the rpc node
```

## Lifecycle

```sh
# Stop, keep data volumes.
docker compose -f docker/docker-compose.yml down

# Stop and wipe all node state + the shared genesis (fresh network next up).
docker compose -f docker/docker-compose.yml down -v

# Tail logs.
docker compose -f docker/docker-compose.yml logs -f validator-1
```

## Notes

- **Genesis:** `genesis-init` writes `/genesis/genesis.json` (chain `detta-local`,
  preset `defi-demo`) into the shared `genesis` volume once; all nodes mount it
  read-only and bootstrap from it. To change chain id / preset, edit the
  `genesis-init` command and recreate with `down -v`.
- **Persistence:** each node has its own `*-data` volume holding blocks,
  receipts, snapshots, DA objects, and consensus-signing records. `serve`
  resumes from it on restart; `--genesis` is only used on first boot.
- **Transport:** nodes serve `--transport tcp` so the bundled `detta-client`
  (which speaks the TCP RPC framing) can drive and health-check them. The health
  check runs `detta-client state-root` against the node's own RPC.
- **Scaling:** add another validator by copying a `validator-N` service block
  (new `DETTA_VALIDATOR_ID`, `DETTA_CONSENSUS_LISTEN: 0.0.0.0:9080`, a new data
  volume, a new host port), adding it to `DETTA_VALIDATORS` on **every** node, and
  adding `validator-N:9080` to the proposer's `DETTA_CONSENSUS_PEERS`. The quorum
  (`2N/3 + 1`) updates automatically. Keep a single `DETTA_PROPOSER`; this build
  has no leader rotation.
