# DeTTa local network (Docker)

Containerized DeTTa node + a Compose topology that spins up a small fleet of
nodes sharing one genesis: four validators and one RPC/full node.

## What this is

The Compose project runs **Byzantine-fault-tolerant consensus with a rotating
proposer** across four validators (full mesh):

1. For each height the proposer is `roster[(height-1+view) % N]` — the role
   **rotates every block**. The proposer builds a block, signs it, and sends the
   signed proposal to every peer. It does **not** commit it yet.
2. Each validator verifies the proposer's signature, **re-executes** the block
   without committing (`verify_block_without_commit`), signs a vote (durably
   locking it to one block at that height), buffers the block, and replies.
3. The proposer aggregates a **quorum** of cryptographically verified votes into
   a **finality certificate** and broadcasts it. Every validator commits the
   block **only** when it has a verified quorum certificate, then persists it.
4. If the proposer for a height stalls, after a **view-change timeout** the next
   validator in rotation takes over (`view=1, 2, …`) and proposes that height.

A non-validating `rpc` node and any restarted validator **catch up** by fetching
finalized blocks *with their certificates* from peers and committing them — every
commit is certificate-gated, so a Byzantine peer cannot feed a forked block.
Validator keys are **derived deterministically** from a shared
`DETTA_CONSENSUS_SEED` and the roster, so no key files are distributed.

Verified end to end (`docker compose … up`): the proposer rotates per height with
4-of-4 certificates; killing a validator triggers a **view-change** and the chain
keeps finalizing at a 3-of-4 quorum with **no fork** (all live validators stay on
one `state_root`); restarting it, it catches up via certificate-verified backfill
and rejoins at 4-of-4.

### Consensus model and its bounds — be precise

- **Safety is Byzantine fault tolerant** for f < N/3 (here N=4, f=1, quorum=3),
  and holds *regardless of rotation timing*. The single commit point is a
  verified quorum certificate, and an honest validator signs at most one block
  per height — durably (`reserve_consensus_signing_record`, persisted across
  restart; unit-tested). So at most one block per height can gather a
  certificate ⇒ at most one block per height is ever committed ⇒ **no fork**,
  even when two proposers propose for the same height during a view change.
- **Liveness under proposer failure** is provided by view-change: a crashed or
  slow proposer is replaced by the next validator after the timeout.
- **Residual:** there is **no NEW-VIEW locked-value re-proposal yet**. In the rare
  case where a proposer's votes split across validators (some lock to block X)
  *and then it crashes*, that one height can stall until enough validators are
  free — a liveness hiccup, never a fork. This is the last piece of a textbook
  PBFT view-change and is the documented next step.

For a plainer crash-fault-tolerant (non-BFT) mode — a single leader that
auto-produces and followers that pull+verify-import over RPC — leave
`DETTA_BFT` unset and use `DETTA_PRODUCE_INTERVAL_SECS` / `DETTA_SYNC_PEER`
instead.

### Consensus environment

| Env var | Who | Meaning |
| --- | --- | --- |
| `DETTA_BFT` | all | `1` to enable BFT consensus |
| `DETTA_VALIDATORS` | all | comma-separated validator roster (rotation order) |
| `DETTA_CONSENSUS_SEED` | all | shared secret; per-validator keys = `H(seed, id)` |
| `DETTA_NETWORK_ID` | all | signed-message domain separator |
| `DETTA_CONSENSUS_LISTEN` | validators | `host:port` to receive proposals on (e.g. `0.0.0.0:9080`) |
| `DETTA_CONSENSUS_PEERS` | validators | `id=host:port,…` of the **other** validators (full mesh) |
| `DETTA_RPC_PEERS` | all | `host:port,…` of peer RPCs for certificate-verified catch-up |
| `DETTA_PRODUCE_INTERVAL_SECS` | validators | round / view-change timeout in seconds |
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

Watch finality and the rotating proposer (each validator finalizes the heights
it proposes):

```sh
docker compose -f docker/docker-compose.yml logs -f validator-2
# [validator-2] finalized height=N signers=["validator-1","validator-2","validator-3","validator-4"]
# [validator-2] committed finality height=N+1 signers=[...]   # proposed by another validator
```

Test view-change — stop a validator and confirm the chain keeps finalizing (a
backup takes over its slots; N=4 tolerates f=1):

```sh
docker compose -f docker/docker-compose.yml stop validator-1
docker compose -f docker/docker-compose.yml logs -f validator-2
# [validator-2] view-change: taking over proposal for height=H view=1
# [validator-2] finalized height=H signers=["validator-2","validator-3","validator-4"]
```

Confirm **no fork** while degraded — the live validators stay on one root:

```sh
docker compose -f docker/docker-compose.yml run --rm --no-deps --entrypoint sh client -c '
for n in validator-2 validator-3 validator-4; do detta-client state-root --rpc "$n":8080; done'

# Restart it; it catches up via certificate-verified backfill and rejoins at 4-of-4:
docker compose -f docker/docker-compose.yml start validator-1
docker compose -f docker/docker-compose.yml logs -f validator-1
# [validator-1] caught up finalized height=...   then   finalized height=... signers=[all four]
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
  adding it to each other validator's `DETTA_CONSENSUS_PEERS` / `DETTA_RPC_PEERS`
  (full mesh). The quorum (`2N/3 + 1`) and the rotation schedule update
  automatically.
