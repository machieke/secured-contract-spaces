#!/bin/sh
# Entrypoint for a DeTTa node container.
#
# On first boot the node bootstraps from the shared genesis snapshot; on restart
# it resumes from its own storage volume (`detta-node serve` ignores --genesis
# once latest_snapshot.bin exists). All configuration is via environment so the
# same image serves every role in docker-compose.yml.
set -eu

GENESIS="${DETTA_GENESIS:-/genesis/genesis.json}"
STORAGE="${DETTA_STORAGE:-/data}"
VALIDATOR_ID="${DETTA_VALIDATOR_ID:-validator-1}"
RPC_ADDR="${DETTA_RPC_ADDR:-0.0.0.0:8080}"

# Wait for the shared genesis to appear (the genesis-init service writes it
# once), unless this node already has local state to resume from.
attempts=0
while [ ! -f "$GENESIS" ] && [ ! -f "$STORAGE/latest_snapshot.bin" ]; do
  attempts=$((attempts + 1))
  if [ "$attempts" -gt 60 ]; then
    echo "entrypoint: genesis not found at $GENESIS after 60s" >&2
    exit 1
  fi
  sleep 1
done

echo "entrypoint: starting validator=$VALIDATOR_ID rpc=$RPC_ADDR storage=$STORAGE"
exec detta-node serve \
  --storage "$STORAGE" \
  --genesis "$GENESIS" \
  --validator-id "$VALIDATOR_ID" \
  --rpc "$RPC_ADDR" \
  --transport tcp \
  --max-connections 0
