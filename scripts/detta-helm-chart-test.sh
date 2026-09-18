#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CHART="$ROOT/charts/detta"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

if ! command -v helm >/dev/null 2>&1; then
  echo "helm is required to test the DeTTa chart" >&2
  exit 127
fi

helm lint "$CHART"

helm template detta "$CHART" --namespace detta > "$WORKDIR/default.yaml"
grep -q "name: detta-validator-1" "$WORKDIR/default.yaml"
grep -q "name: detta-validator-4" "$WORKDIR/default.yaml"
grep -q "name: detta-rpc" "$WORKDIR/default.yaml"
grep -q "DETTA_CONSENSUS_PEERS" "$WORKDIR/default.yaml"
grep -q "helm.sh/hook: test" "$WORKDIR/default.yaml"

helm template detta "$CHART" \
  --namespace detta \
  --set validators.count=5 \
  --set validators.persistence.enabled=false \
  --set rpcNode.persistence.enabled=false \
  --set rpc.service.type=NodePort \
  --set rpc.service.nodePort=30080 \
  > "$WORKDIR/ephemeral-nodeport.yaml"
grep -q "name: detta-validator-5" "$WORKDIR/ephemeral-nodeport.yaml"
grep -q "nodePort: 30080" "$WORKDIR/ephemeral-nodeport.yaml"

if helm template bad "$CHART" --set validators.count=3 >"$WORKDIR/invalid.out" 2>"$WORKDIR/invalid.err"; then
  echo "expected validators.count=3 to fail while BFT consensus is enabled" >&2
  exit 1
fi
grep -q "consensus.enabled requires validators.count >= 4" "$WORKDIR/invalid.err"

if command -v kubectl >/dev/null 2>&1 && kubectl auth can-i create pods >/dev/null 2>&1; then
  kubectl apply --dry-run=client --validate=false -f "$WORKDIR/default.yaml" >/dev/null
  kubectl apply --dry-run=client --validate=false -f "$WORKDIR/ephemeral-nodeport.yaml" >/dev/null
elif command -v kubectl >/dev/null 2>&1; then
  echo "kubectl is installed, but cluster auth is unavailable; skipping kubectl dry-run"
fi

echo "DeTTa Helm chart render tests passed"
