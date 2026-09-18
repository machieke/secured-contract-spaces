# DeTTa Helm chart

This chart runs a small DeTTa cluster on k3s or any standard Kubernetes cluster.
It mirrors the Docker Compose topology: four BFT validators, one persistent RPC
full node, a deterministic demo genesis, and TCP JSON-RPC health probes.

## What gets installed

- One Secret containing the shared BFT consensus seed, unless
  `consensus.seed.existingSecret` is set.
- One single-replica StatefulSet and Service per validator.
- One single-replica StatefulSet and Service for the RPC/full node.
- Per-pod genesis initContainers that run `detta-node write-genesis`.
- PersistentVolumeClaims for every validator and the RPC node.
- A Helm test pod that runs `detta-client state-root` against the RPC service.

The chart intentionally avoids a shared genesis PVC. Every pod writes the same
deterministic genesis into an `emptyDir` before starting the node, so the default
setup works on k3s `local-path` storage without requiring ReadWriteMany volumes.

Validator peer addresses prefer Kubernetes service IP environment variables and
fall back to service DNS names. Validator 1 also delays startup briefly and does
bounded RPC readiness checks against the other validators before starting. The
chart uses a 20 second BFT round by default so small k3s clusters have a clean
bootstrap window before view-change.

## Build or publish the image

For a single-node k3s development cluster:

```sh
docker build -f docker/Dockerfile -t detta-node:local .
docker save detta-node:local -o /tmp/detta-node-local.tar
sudo k3s ctr images import /tmp/detta-node-local.tar
```

For a multi-node k3s cluster, push the image to a registry reachable by every
node and override `image.repository`, `image.tag`, and optionally
`image.registry`.

## Install

```sh
helm upgrade --install detta charts/detta \
  --namespace detta \
  --create-namespace
```

With a registry image:

```sh
helm upgrade --install detta charts/detta \
  --namespace detta \
  --create-namespace \
  --set image.registry=registry.example.com \
  --set image.repository=detta-node \
  --set image.tag=v0.1.0
```

Expose the RPC node locally:

```sh
kubectl -n detta port-forward svc/detta-rpc 8080:8080
detta-client state-root --rpc 127.0.0.1:8080
```

Run the Helm test:

```sh
helm test detta -n detta
```

## k3s storage

k3s normally ships with the `local-path` storage class. The chart leaves
`storageClass` empty by default so Kubernetes uses the default class.

To pin the class explicitly:

```sh
helm upgrade --install detta charts/detta \
  --namespace detta \
  --create-namespace \
  --set validators.persistence.storageClass=local-path \
  --set rpcNode.persistence.storageClass=local-path
```

For ephemeral development runs:

```sh
helm upgrade --install detta charts/detta \
  --namespace detta \
  --create-namespace \
  --set validators.persistence.enabled=false \
  --set rpcNode.persistence.enabled=false
```

## Consensus seed

The default seed is a demo value. For any non-throwaway network, create your own
Secret and point the chart at it:

```sh
kubectl -n detta create secret generic detta-consensus-seed \
  --from-literal=consensus-seed='replace-with-a-long-random-secret'

helm upgrade --install detta charts/detta \
  --namespace detta \
  --create-namespace \
  --set consensus.seed.existingSecret=detta-consensus-seed
```

## Validator count

The default is four validators, which gives f=1 BFT fault tolerance. The chart
can render a different initial validator count:

```sh
helm upgrade --install detta charts/detta \
  --namespace detta \
  --create-namespace \
  --set validators.count=5
```

Treat validator membership as an initial-launch parameter. The current DeTTa
node uses a static full-mesh roster from environment variables; live membership
changes still require a coordinated operational procedure outside this chart.

## Useful commands

```sh
kubectl -n detta get pods,svc,pvc
kubectl -n detta logs -f statefulset/detta-validator-1
kubectl -n detta logs -f statefulset/detta-rpc
helm uninstall detta -n detta
```
