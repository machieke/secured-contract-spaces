# DeTTa local network (Docker)

Containerized DeTTa node + a Compose topology that spins up a small fleet of
nodes sharing one genesis: four validators and one RPC/full node.

## What this is (and isn't)

The `detta-node serve` daemon runs the TCP JSON-RPC surface plus two optional
background consensus threads, selected by environment:

- **Leader** (`DETTA_PRODUCE_INTERVAL_SECS=N`): produces a block every N seconds
  from its mempool, advancing the chain.
- **Follower** (`DETTA_SYNC_PEER=host:port`): polls a peer, fetches each missing
  block, and **verify-imports** it (`import_block` re-executes the block and
  checks every root, so a divergent or invalid block is rejected).

This Compose project wires one leader (`validator-1`) and four followers
(`validator-2..4`, `rpc`) so the network **reaches consensus automatically**: the
leader proposes, every follower independently verifies and adopts the same block
sequence, and all nodes converge on the same height and state root. Verified end
to end — at a synchronized instant all five nodes report an identical
`state_root`.

So this Compose project gives you:

- a reproducible image with `detta-node` and `detta-client`;
- a shared genesis written once into a volume, so every node starts identical;
- a self-driving fleet that converges on one chain, with health checks;
- a client driver to submit work and inspect DA/state over RPC.

**Consensus model — be precise about it.** This is single-leader, crash-fault
-tolerant, *verified* replication: followers re-execute and root-check every
block, so they cannot be made to adopt an invalid chain. It is **not** full BFT —
there is no vote gossip, quorum finality certificate, leader rotation, or
view-change in the daemon (those primitives exist in `detta-consensus` /
`detta-network` but are not wired into this loop). A crashed leader halts block
production until it restarts; a single equivocating leader is not automatically
replaced. Treat it as a converging dev/demo network, not a mainnet validator set.

### Tuning consensus

| Env var | Role | Meaning |
| --- | --- | --- |
| `DETTA_PRODUCE_INTERVAL_SECS` | leader | seconds between produced blocks (unset = no production) |
| `DETTA_SYNC_PEER` | follower | `host:port` of the node to pull blocks from (unset = no sync) |
| `DETTA_SYNC_INTERVAL_SECS` | follower | poll interval, default 2s |

A node may be a leader, a follower, both, or neither.

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

Watch a follower adopt the leader's blocks:

```sh
docker compose -f docker/docker-compose.yml logs -f validator-2
# [validator-2] imported block height=1
# [validator-2] imported block height=2 ...
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
- **Scaling:** add another follower by copying a `validator-N` service block (new
  `DETTA_VALIDATOR_ID`, `DETTA_SYNC_PEER: validator-1:8080`, a new data volume,
  and a new host port). Move the leader role by setting
  `DETTA_PRODUCE_INTERVAL_SECS` on a different service (only run one producer at a
  time, or the followers will see two competing chains).
