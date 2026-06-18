# DeTTa local network (Docker)

Containerized DeTTa node + a Compose topology that spins up a small fleet of
nodes sharing one genesis: four validators and one RPC/full node.

## What this is (and isn't)

The shipped `detta-node serve` daemon is an **RPC server**: it serves the TCP
JSON-RPC surface and produces blocks when a client asks it to
(`produce-block` / `produce-da-block`). It does **not** run peer-to-peer gossip
or automatic multi-validator consensus — that path lives in the in-process test
harness (`detta-network`), not the binary.

So this Compose project gives you:

- a reproducible image with `detta-node` and `detta-client`;
- a shared genesis written once into a volume, so every node starts from
  identical state;
- a fleet of independent node replicas, each on its own RPC port, with health
  checks;
- a client driver to produce blocks and inspect DA/state over RPC.

It is a **local development / demo network**, not live BFT consensus across the
validators. Each node advances only when you drive its own RPC.

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

Use the bundled client via the `tools` profile (it joins the Compose network, so
address nodes by service name):

```sh
# Produce a DA-committed block on validator-1.
docker compose -f docker/docker-compose.yml run --rm client \
  produce-da-block --height 1 --rpc validator-1:8080

# Inspect DA + state.
docker compose -f docker/docker-compose.yml run --rm client da-stats --rpc validator-1:8080
docker compose -f docker/docker-compose.yml run --rm client state-root --rpc validator-1:8080
docker compose -f docker/docker-compose.yml run --rm client da-retention-audit --rpc validator-1:8080
```

From the host you can target the published ports instead (TCP JSON-RPC):

```sh
# If you have a local detta-client build:
target/release/detta-client state-root --rpc 127.0.0.1:8080
```

Because each node is an independent replica, produce blocks on each node you want
advanced, or point all your `produce-*` / `submit-*` calls at a single node and
treat the others as standby replicas of the shared genesis.

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
- **Scaling:** add another validator by copying a `validator-N` service block (new
  `DETTA_VALIDATOR_ID`, new data volume, new host port).
