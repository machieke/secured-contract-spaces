# DeTTa Operator Manual

This manual describes how to operate DeTTa and its data availability layer.
It is intended for validator, full-node, RPC, archive, and release operators.
Use it together with:

- `detta-operator-runbook.md` for detailed release, monitoring, governance, and
  validator procedures;
- `detta-da-operator-runbook.md` for DA retention, repair, and evidence
  procedures;
- `detta-data-availability-layer.md` for the DA v1 vocabulary, threat model,
  and schema;
- `detta-rpc-api.md` and `detta-rpc-openapi.json` for the RPC surface;
- `ops/detta-public-testnet-readiness.json` and
  `ops/detta-mainnet-candidate-readiness.json` for launch readiness status.

Status note: the current implementation is a production-track candidate. It is
not audited, not a public mainnet, and must not be used to secure real funds.

## 1. High-Level Architecture

DeTTa is a consensus-replicated secured Atomspace runtime for DeFi. It combines
deterministic secured contract execution, validator consensus, durable storage,
state sync, RPC, governance, and data availability.

```text
clients / wallets / operators
        |
        v
HTTP or TCP JSON-RPC
        |
        v
persistent DeTTa node
        |
        +-- transaction admission and persistent mempool
        +-- deterministic secured-contract execution
        +-- receipts, events, roots, and proofs
        +-- validator proposal/vote/finality logic
        +-- validator-set metadata and governance updates
        +-- snapshot state sync and backup/restore storage
        +-- DA block payloads, manifests, shares, certificates, and repair
```

Recommended production topology:

- run at least four independently hosted validators so the default quorum is
  3-of-4;
- run non-validator full nodes for public RPC, indexing, and state sync;
- run archive/DA nodes with extended retention for audit and reconstruction;
- keep validator peer networking allowlisted;
- keep operator RPC behind authentication and rate limits;
- expose public RPC only from dedicated bounded RPC nodes.

### DeTTa Core Components

- `detta-core`: deterministic secured-contract execution, roots, proofs, and
  DeFi contract methods.
- `detta-consensus`: finality certificates, DA-certified finality, validator
  set updates, and slashing.
- `detta-protocol`: signed network envelopes, validator messages, DA messages,
  and state-sync wire messages.
- `detta-storage`: durable blocks, receipts, mempool, snapshots, metadata,
  DA objects, indexes, repair records, and audit records.
- `detta-node`: persistent validator/full-node orchestration and RPC serving.
- `detta-da`: DA payloads, manifests, Reed-Solomon shares, reconstruction,
  custody assignment, sampling proofs, challenges, and certificates.

### Data Availability Layer

For DA v1, finalized production blocks are expected to have:

- a canonical DA payload containing replay/audit data;
- a DA manifest binding payload hash, namespace root, share root, share counts,
  erasure scheme, and namespace ranges;
- Reed-Solomon v1 encoded shares committed by a Merkle SHA-256 share root;
- validator DA votes signed only after deterministic custody shares verify;
- a quorum DA certificate for the manifest;
- finality checks that bind the block header DA commitment, DA certificate,
  reconstructed payload roots, and execution roots.

Persistent nodes store DA manifests, shares, reconstructed payloads,
certificates, challenge evidence, repair records, and indexes. DA status and
repair RPCs report whether data is locally reconstructable and whether
retention obligations are satisfied.

## 2. Roles And Responsibilities

Validator operators:

- run validator nodes with persistent storage and signing keys;
- keep consensus-signing records intact across restarts;
- monitor finality, DA custody, slashing, storage, and RPC health;
- participate in validator-set metadata updates and governance procedures;
- retain evidence for releases, incidents, and audits.

Full-node/RPC operators:

- serve bounded JSON-RPC for clients and indexers;
- keep state, block history, proofs, and snapshots available;
- avoid exposing validator keys or privileged operator RPC publicly.

Archive/DA operators:

- retain DA manifests, certificates, payloads, and shares beyond validator hot
  windows;
- verify reconstruction and sampling periodically;
- provide repair sources for missing shares;
- retain audit evidence for governance, bridge, oracle, and aspect deployment
  data.

Release operators:

- run release gates, drills, packaging, signing, and readiness verification;
- publish artifact checksums and detached signatures;
- retain release-candidate evidence bundles.

## 3. Initial Launch Configuration

### 3.1 Preflight

Before launch, every operator should verify:

- source branch and commit are agreed by the validator set;
- readiness manifests do not claim readiness while blockers remain;
- release artifacts are generated from a clean source state for final
  publication;
- validator identities, network id, chain id, quorum, RPC addresses, and
  storage paths are fixed in the launch record;
- all operators have the same genesis artifact and checksum;
- every validator has a dedicated persistent storage volume;
- backup and restore procedures have been rehearsed;
- DA retention policy is accepted by the operator group;
- release, genesis, governance, validator onboarding, packaged client, DA
  stability, audit readiness, and signing drills have retained reports.

Run the release gate before any public launch candidate:

```sh
DETTA_E2E_FULL=1 DETTA_REQUIRE_DEP_AUDIT=1 scripts/detta-release-gate.sh
```

For final publication packaging, require a clean source tree:

```sh
DETTA_REQUIRE_CLEAN_RELEASE_SOURCE=1 scripts/detta-package-release.sh
```

Then sign generated checksum attestations and verify signatures:

```sh
for checksum in dist/*.sha256; do
  gpg --armor --output "$checksum.sig" --detach-sign "$checksum"
done

DETTA_RELEASE_SIGNER_FINGERPRINT=<fingerprint> \
  scripts/detta-verify-release-signatures.sh dist/detta-release-<version>.json
```

### 3.2 Genesis

Generate the launch genesis from the agreed binary and preset:

```sh
cargo build --release --bin detta-node --bin detta-client

target/release/detta-node write-genesis \
  --output dist/genesis.json \
  --chain-id <chain-id> \
  --preset defi-demo
```

Finalize launch genesis with an operator drill:

```sh
DETTA_FINAL_GENESIS_VALIDATORS=validator-1,validator-2,validator-3,validator-4 \
  scripts/detta-genesis-finalization-drill.sh
```

Retain:

- genesis file and checksum;
- launch validator ids and public keys;
- expected quorum;
- initial chain id and network id;
- faucet/sample-client artifacts if published;
- genesis-finalization report and checksum.

### 3.3 Validator Startup

Start each validator from the same genesis on an independent host:

```sh
target/release/detta-node serve \
  --storage /var/lib/detta/validator-1 \
  --genesis /etc/detta/genesis.json \
  --validator-id validator-1 \
  --rpc 127.0.0.1:8080 \
  --transport tcp \
  --max-connections 0
```

Notes:

- `serve` restarts from `--storage` when `latest_snapshot.bin` exists;
  `--genesis` is used only on first boot.
- Use different storage directories per validator identity.
- Do not reuse a validator signing key across active nodes.
- Keep the validator data directory durable; it includes consensus-signing
  records that prevent conflicting signatures after crash or partition.

### 3.4 Full Node And RPC Startup

Start full/RPC nodes from the same genesis or from a verified snapshot. Use
`--transport tcp` for nodes driven by the packaged `detta-client`; use
`--transport http` for HTTP JSON-RPC nodes behind a reverse proxy.

```sh
target/release/detta-node serve \
  --storage /var/lib/detta/fullnode-1 \
  --genesis /etc/detta/genesis.json \
  --validator-id fullnode-1 \
  --rpc 127.0.0.1:9080 \
  --transport http \
  --max-connections 1024
```

For public RPC:

- put the HTTP endpoint behind a reverse proxy with TLS;
- apply request size, rate, and connection limits;
- expose public read/write client methods only;
- keep operator RPC methods on a private network or behind bearer-token
  authentication.

### 3.5 DA Launch Configuration

The production DA v1 profile uses:

- Merkle SHA-256 share commitments;
- Reed-Solomon v1 erasure coding;
- deterministic validator custody assignments;
- optional light-client sample indices;
- production default retention policy unless an operator-provided policy
  already exists;
- DA namespaces for block, transaction, receipt, aspect, governance, bridge,
  and oracle evidence.

After first DA block production, verify:

```sh
target/release/detta-client da-stats --rpc 127.0.0.1:8080
target/release/detta-client da-retention-audit --rpc 127.0.0.1:8080
target/release/detta-client da-retention-prune-plan --rpc 127.0.0.1:8080
```

Expected launch conditions:

- active DA production profile is `detta.da-production-profile.v1`;
- retention policy root is present;
- `missing_share_count == 0` for local hot data;
- `unsatisfied_manifest_count == 0`;
- no DA custody, repair, or challenge alerts are active.

## 4. Adding Nodes

### 4.1 Add A Full/RPC Node

1. Provision host, storage, firewall rules, and monitoring.
2. Install the verified release artifact.
3. Copy genesis or fetch a verified snapshot from a healthy peer.
4. Start with a new storage directory.
5. Verify health:

```sh
target/release/detta-client state-root --rpc <node-rpc>
```

Also query:

- `get_node_health`;
- `get_operator_metrics`;
- `get_snapshot_metadata_root_status`;
- `get_da_storage_stats`;
- `get_da_retention_audit`.

6. Compare state root, latest height, DA profile, and metadata roots against
   at least two existing healthy nodes using the root-comparison procedure in
   §11.2. All seven roots must match the majority at equal height.
7. Add the node to load balancers only after roots and RPC checks match.

### 4.2 Add A Validator

Validator membership is controlled by signed validator-set metadata updates.

1. Generate the new validator identity and public key offline.
2. Provision host, storage, networking, monitoring, and backups.
3. Sync as a non-voting full node first.
4. Verify roots and DA status against existing validators.
5. Create a validator-set metadata update that adds the new validator key.
6. Collect current-validator quorum authorizations.
7. Submit authorizations through `propose_validator_set_metadata_update`.
8. Monitor `get_validator_set_metadata_update_status` until applied.
9. Confirm `get_validator_set_metadata_audit_records` contains the update.
10. Restart one non-critical node and verify the keyring reloads.
11. Enable the validator key only after the update is active and audited.

Reject the onboarding if:

- validator id or key is duplicated;
- chain id or network id is wrong;
- update expiry height is stale;
- signer quorum is insufficient;
- root or DA status mismatches existing validators;
- the new node cannot pass restart and backup checks.

### 4.3 Add An Archive/DA Node

1. Start from genesis or a verified DA-backed checkpoint.
2. Configure storage with archive-capacity retention.
3. Fetch and retain hot/checkpoint DA manifests, certificates, shares, and
   payloads.
4. Run reconstruction checks for recent and historical manifests.
5. Verify namespace and certificate indexes:

```sh
target/release/detta-client da-manifest-index-by-height --height <h> --rpc <node-rpc>
target/release/detta-client da-manifest-index-by-namespace --namespace detta.tx --rpc <node-rpc>
target/release/detta-client da-certificate-index-by-height --height <h> --rpc <node-rpc>
```

6. Register the archive role through the deployment's governance/process before
   relying on it for mainnet audit retention.

## 5. Removing Nodes

### 5.1 Remove A Full/RPC Node

1. Drain client traffic from the load balancer.
2. Stop accepting new RPC requests.
3. Verify no unique archive/DA data exists only on this node.
4. Take a final backup if the node has useful audit evidence.
5. Stop the service.
6. Remove from monitoring and peer allowlists.

### 5.2 Remove A Validator

1. Confirm the remaining validator set will still have quorum.
2. Create a validator-set metadata update that removes the validator key.
3. Collect quorum authorizations from the current validator set.
4. Submit the update and monitor until applied.
5. Confirm audit records include the removal.
6. Stop the validator process.
7. Quarantine or destroy the removed validator's signing key according to the
   key-management policy.
8. Preserve the data directory until finality, slashing, DA challenge, and
   audit windows have expired.

Emergency removal follows the same governed path, but operators should also:

- quarantine the suspected key immediately;
- preserve logs, signing records, finality certificates, and slashing evidence;
- increase monitoring on finality lag and peer isolation;
- run incident-response drills against the retained evidence.

### 5.3 Remove Or Degrade A DA/Archive Node

Before removal:

- run `get_da_retention_audit`;
- verify no active manifest is uniquely satisfied by this node;
- verify checkpoint data is replicated elsewhere;
- export DA manifests, certificates, challenge records, indexes, and audit
  records needed for the retention window;
- run reconstruction from an independent node after removal.

Do not prune manifests, certificates, indexes, challenge records, or audit
records as part of ordinary DA retirement.

## 6. Maintenance Procedures

### 6.1 Daily Health Checks

Check every validator and public RPC node:

- `get_node_health`: chain id, height, validator key count, roots;
- `get_operator_metrics`: finality height, lag, peers, mempool, storage bytes,
  DA profile, DA counters;
- `get_operator_alerts`: stalled consensus, root mismatch, excessive reverts,
  peer isolation, slashing evidence, disk pressure, RPC overload, mempool
  saturation, DA missing shares, DA repair lag, DA custody failures, DA
  challenge failures;
- `get_mempool_status`: pending count, per-sender pressure, admission limits;
- `get_finality_certificate`: latest finalized signer set;
- `get_da_storage_stats`: active retention policy, root, stored/missing shares;
- `get_da_retention_audit`: active/expired manifests and unsatisfied
  obligations.

Alert immediately on:

- height lag against validator majority;
- missing or stale finality certificates;
- any root mismatch;
- any slashing record;
- repeated state-sync failures;
- nonzero active DA missing-share count;
- DA repair lag beyond the policy window;
- disk pressure near retention limits;
- unexpected DA production profile changes.

### 6.2 Backups

Back up each node's storage directory regularly. The backup must preserve:

- blocks, transactions, receipts, finality certificates;
- mempool records;
- snapshots and metadata roots;
- validator-set metadata, keyring metadata, pending authorizations, and audit
  records;
- consensus-signing records;
- slashing records;
- DA manifests, shares, payloads, certificates, repair records, challenge
  records, indexes, and DA storage roots.

After backup:

1. Restore to an isolated test directory.
2. Start a node from the restored storage.
3. Verify latest block, finality certificate, state root, snapshot roots, and
   DA store roots — compare all roots against a healthy peer per §11.2.
4. Run DA payload reconstruction for representative hot and checkpoint
   manifests per §11.3 (`payload_reconstructable == true`).
5. Record backup manifest, root checks, and restore result.

### 6.3 DA Retention And Pruning

Before pruning:

```sh
target/release/detta-client da-stats --rpc <node-rpc>
target/release/detta-client da-retention-audit --rpc <node-rpc>
target/release/detta-client da-retention-prune-plan --rpc <node-rpc>
target/release/detta-client da-repair-status --manifest-hash <hash> --rpc <node-rpc>
```

Proceed only if:

- active manifest retention is satisfied;
- prune plan candidates are expired under the active policy;
- no repair or custody alerts are active;
- independent archive/DA nodes can reconstruct representative payloads;
- before/after evidence will be retained.

After pruning:

- rerun stats and audit;
- reconstruct representative retained payloads;
- run state sync from a DA checkpoint if checkpoint shares were touched;
- rebuild DA indexes if manual repair or disk maintenance touched `da/indexes`;
- retain before/after reports.

### 6.4 Repairs

When DA repair alerts fire, follow the step-by-step share-repair procedure in
§11.4. In summary:

1. Identify the manifest and exact missing indices with
   `da-repair-status --manifest-hash <hash>` (`missing_share_indices`,
   `repair_needed`).
2. Fetch each missing share from at least two independent peers with
   `da-share --manifest-hash <hash> --index <i>`.
3. Shares are hash-verified against the committed `share_hash`/`share_root` on
   ingest; non-matching shares are rejected — confirm via `da-status` that the
   index left `missing_share_indices`.
4. Once `stored_share_count` reaches the threshold
   (`expected_share_count / 2` for v1), the payload reconstructs and persists.
5. Verify `da-status` reports `payload_reconstructable == true` and
   `reconstruction_error == null`, then re-check retention (§11.5).
6. Record any peer that served a non-matching share for possible isolation or
   challenge (§11.8).

### 6.5 Upgrades

All code and policy upgrades must be governed, rehearsed, and auditable:

1. Schedule the upgrade through governance.
2. Wait for timelock maturity.
3. Fetch scheduled upgrade or policy update RPC records.
4. Fetch `get_upgrade_rehearsal_report`.
5. Require no invariant failures.
6. Execute the upgrade.
7. Verify post-upgrade roots, receipts, events, and DA evidence.
8. Restart one non-critical node first, then roll the rest.

Do not manually edit state, policy roots, contract code, validator metadata, or
DA policy files to bypass governance.

### 6.6 Release Evidence

For every release candidate, retain:

- release gate output;
- release package manifest and checksums;
- detached signatures;
- source-state report;
- readiness status report;
- genesis finalization report;
- launch rehearsal report;
- governance bootstrap report;
- validator onboarding report;
- packaged client flow report;
- stability drill report;
- DA incident and DA stability reports;
- audit-readiness package and verifier output;
- proof artifact manifest and TLA model-checking output.

Use:

```sh
scripts/detta-release-candidate-evidence-bundle.sh
scripts/detta-verify-release-candidate-evidence-bundle.sh <bundle>
```

## 7. Disaster Recovery

### 7.1 Process Crash Or Host Reboot

1. Restart the node with the same storage directory and validator id.
2. Verify `get_node_health`, `get_mempool_status`, latest block, latest
   finality certificate, and state root.
3. Verify consensus-signing records exist before allowing the validator key to
   sign again.
4. Verify DA stats and retention audit.
5. Compare roots with two healthy peers before returning to service.

### 7.2 Corrupted Local Storage

1. Stop the node.
2. Preserve the corrupted storage directory for evidence.
3. Restore the latest verified backup to a fresh directory.
4. Rebuild DA indexes if needed.
5. Start the node from restored storage.
6. Verify state root, block height, finality certificates, snapshot metadata
   roots, and DA store roots (compare all roots per §11.2), then verify DA
   reconstruction for a representative recent and checkpoint manifest (§11.3)
   and retention obligations (§11.5).
7. Rejoin only after roots match healthy peers (§11.2).

If no trustworthy backup exists, bootstrap from a verified snapshot or
DA-backed checkpoint instead of trying to repair state manually.

### 7.3 Lost DA Shares Or Payloads

Run the detailed share-repair procedure in §11.4 for each affected manifest. In
summary:

1. Query `da-repair-status --manifest-hash <hash>` for affected manifests and
   read `missing_share_indices`.
2. Fetch shares from multiple independent peers (§11.4 step 2); invalid shares
   are rejected on ingest — record the serving peer as evidence.
3. Reconstruct from threshold-valid shares and confirm
   `payload_reconstructable == true` (§11.3).
4. Re-verify namespace and certificate indexes:

   ```sh
   target/release/detta-client da-manifest-index-by-block --block-hash <hash> --rpc <node-rpc>
   target/release/detta-client da-certificate-index-by-manifest --manifest-hash <hash> --rpc <node-rpc>
   ```

5. Re-run a light-client sample-proof check (§11.6).
6. If signed custody cannot be served, follow the DA challenge/evidence path
   (§11.8) and the DA operator runbook.

### 7.4 Validator Key Compromise

1. Isolate the validator host and stop signing immediately.
2. Preserve data directory, logs, signing records, certificates, and network
   messages.
3. Check for equivocation or DA custody failures.
4. Prepare a validator-set metadata update removing the compromised key.
5. Collect quorum authorizations and apply the removal.
6. Add a replacement validator only after it syncs, verifies roots, and passes
   onboarding checks.
7. Publish an incident report with affected heights, signer ids, evidence
   hashes, and remediation actions.

### 7.5 Chain Halt Or Finality Stall

1. Freeze validator membership changes except emergency removals.
2. Compare latest block, state root, finality certificate, DA manifest, and DA
   certificate across validators.
3. Identify whether the halt is caused by missing quorum, bad DA manifest,
   missing DA certificate, unavailable custody shares, network partition, or
   invalid block execution — use the operator-metrics decision tree in §11.8 and
   the certificate-binding check in §11.7.
4. Preserve all conflicting proposals, votes, DA manifests, and certificates.
5. Remove or quarantine faulty validators through governed metadata updates.
6. Resume only after a quorum agrees on roots and DA availability.
7. Run incident-response and DA incident drills against the evidence set.

### 7.6 RPC Compromise Or Overload

1. Remove the node from public load balancers.
2. Rotate RPC authentication tokens if operator RPC may be exposed.
3. Preserve request logs and RPC error metrics.
4. Verify state roots and storage roots against validators.
5. Restore from backup or rebuild as a fresh full node if integrity is in
   doubt.
6. Reintroduce traffic gradually with stricter rate limits.

### 7.7 Data Center Or Region Loss

1. Confirm remaining validators still meet quorum.
2. Disable affected endpoints in peer allowlists and load balancers.
3. Promote full/archive nodes in other regions only after root and DA checks.
4. Restore lost nodes from verified backups in a new region.
5. Run DA reconstruction and checkpoint state-sync checks before rejoining.
6. Review whether retention copies, archive roles, and backup schedules need
   expansion.

## 8. Launch And Operations Checklists

### Launch Checklist

- Release gate passed with retained output.
- Readiness manifests verified.
- Release artifacts packaged, checksummed, signed, and verified.
- Genesis finalized and signed.
- Validator ids, public keys, chain id, network id, and quorum recorded.
- All validators boot from the same genesis and report matching roots.
- Public RPC nodes are behind TLS, rate limits, and load balancers.
- Operator RPC is private/authenticated.
- DA profile, retention policy root, and DA storage stats are present.
- DA block production, retrieval, sampling, retention, and index commands pass.
- Backup/restore rehearsal passed.
- Incident-response, governance, validator-onboarding, stability, and DA drills
  have retained reports.

### Daily Checklist

- Finality height advancing.
- Validator majority roots match.
- No root mismatch or slashing alerts.
- Mempool pressure within limits.
- DA missing-share and repair alerts clear.
- Retention audit has no unsatisfied active manifests.
- Disk usage below retention thresholds.
- Snapshot metadata roots match expectations.
- Public RPC error rate within limits.

### Weekly Checklist

- Restore a recent backup into an isolated node.
- Reconstruct representative DA block and checkpoint payloads.
- Verify DA sample proofs for recent blocks.
- Review validator-set metadata audit records.
- Review governance queue and pending timelocks.
- Review release/audit evidence retention.
- Verify archive nodes still serve historical DA payloads.

## 9. Command Reference

Node:

```sh
detta-node write-genesis --output PATH [--chain-id detta-local] [--preset defi-demo]
detta-node serve --storage DIR --genesis PATH [--validator-id validator-1] \
  [--rpc 127.0.0.1:8080] [--transport tcp|http] [--max-connections N]
detta-node faucet-tx --to ACCOUNT --amount AMOUNT [--chain-id detta-local]
```

Client DA and operations commands:

```sh
detta-client produce-da-block --height <h> [--timestamp <t>] [--share-size <bytes>]
detta-client da-manifest --manifest-hash <hash>
detta-client da-share --manifest-hash <hash> --index <i>
detta-client da-certificate --certificate-hash <hash>
detta-client da-payload --manifest-hash <hash>
detta-client da-namespace --manifest-hash <hash> --namespace <name>
detta-client da-sample-proofs --manifest-hash <hash> \
  --client-randomness <bytes> --sample-count <n> [--namespaces <csv>]
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
```

Release and evidence scripts:

```sh
scripts/detta-release-gate.sh
scripts/detta-package-release.sh
scripts/detta-verify-release-signatures.sh
scripts/detta-genesis-finalization-drill.sh
scripts/detta-operator-launch-rehearsal.sh
scripts/detta-incident-response-drill.sh
scripts/detta-da-incident-response-drill.sh
scripts/detta-governance-bootstrap-drill.sh
scripts/detta-validator-onboarding-drill.sh
scripts/detta-packaged-client-flow-drill.sh
scripts/detta-public-testnet-stability-drill.sh
scripts/detta-da-stability-drill.sh
scripts/detta-audit-readiness-package.sh
scripts/detta-release-candidate-evidence-bundle.sh
```

## 10. Escalation Rules

Escalate to the validator operator group immediately when:

- quorum is at risk;
- finality stalls;
- a validator key may be compromised;
- DA custody, missing-share, or challenge alerts are active;
- state roots differ between honest validators;
- signed artifacts or readiness evidence do not verify;
- a bridge, oracle, governance, or upgrade proof is disputed;
- retention policy would prune the only known reconstructable data.

The default response is to preserve evidence first, stop unsafe signing or
serving second, and restore service only after roots, certificates, and DA
availability are independently verified.

## 11. Detailed Verification And Repair Procedures

This section gives concrete, copy-pasteable steps for the verification and fix
actions referenced throughout the manual. Field names below are the exact
JSON keys returned by the node. Replace `<node-rpc>` with a node's RPC address
(for example `127.0.0.1:8080`) and `<operator-token>` with the operator bearer
token where a method is privileged.

### 11.1 Invoking RPC Methods

HTTP-transport nodes accept bounded JSON `POST` requests on `/` or `/rpc`:

```sh
curl -s -X POST http://<node-rpc>/rpc \
  -H 'content-type: application/json' \
  -d '{"method":"get_node_health"}' | jq .
```

Privileged operator methods take a bearer-token wrapper when the node enforces
operator authentication:

```sh
curl -s -X POST http://<node-rpc>/rpc \
  -H 'content-type: application/json' \
  -d '{"bearer_token":"<operator-token>","request":{"method":"get_da_retention_audit"}}' | jq .
```

TCP-transport nodes (driven by the packaged client) expose the same data through
`target/release/detta-client <subcommand> --rpc <node-rpc>`. `jq` is optional but
used below to extract fields.

### 11.2 Verify State And Metadata Roots Match Across Nodes

Use this wherever the manual says "compare roots" or "verify roots against
healthy peers". A node is consistent only when **every** root below is identical
to the validator majority at the **same height**.

1. Capture the health report from the node under test and from two known-healthy
   peers:

   ```sh
   for n in <node-rpc> <peer1-rpc> <peer2-rpc>; do
     echo "== $n =="
     curl -s -X POST http://$n/rpc -H 'content-type: application/json' \
       -d '{"method":"get_node_health"}' \
       | jq '{height, chain_id, global_state_root, storage_root, registry_root,
              policy_root, event_root, nonce_root, outbox_root}'
   done
   ```

2. Confirm `chain_id` matches and the nodes are at the **same** `height`. If
   heights differ, let the lagging node sync and re-sample; comparing roots at
   different heights is meaningless.
3. At equal height, all of `global_state_root`, `storage_root`, `registry_root`,
   `policy_root`, `event_root`, `nonce_root`, and `outbox_root` must match the
   majority exactly.
4. **If any root differs from the honest majority:** the node's state is
   divergent. Do not return it to service or let it sign. Stop the node, preserve
   the storage directory as evidence, and rebuild it from a verified backup
   (§7.2) or a verified snapshot/DA checkpoint. Re-run this check before
   rejoining. A persistent mismatch across multiple honest nodes is an
   escalation trigger (§10).

### 11.3 Verify A DA Payload Is Reconstructable

Use this wherever the manual says "verify reconstruction" or
"payload_reconstructable == true".

1. Resolve the manifest hash for the height of interest:

   ```sh
   target/release/detta-client da-manifest-index-by-height --height <h> --rpc <node-rpc>
   ```

   Record the `manifest_hash` from the returned `DaManifestIndexEntry`.

2. Query DA status for that manifest:

   ```sh
   target/release/detta-client da-status --manifest-hash <manifest_hash> --rpc <node-rpc>
   ```

3. Read the `DaStatusReport` fields:
   - `payload_reconstructable` must be `true`.
   - `stored_share_count` must be `>=` the manifest reconstruction threshold
     (for the v1 equal data/parity profile this is `expected_share_count / 2`).
   - `missing_share_indices` lists shares to repair; it may be non-empty even
     when `payload_reconstructable` is `true` (you still hold a threshold), but
     a retention policy with `retain_all_shares` is not satisfied until it is
     empty (§11.5).
   - `manifest_available` and, when finalized, `certificate_available` must be
     `true`; `reconstruction_error` must be `null`.
4. **If `payload_reconstructable` is `false`:** the node lacks a threshold of
   valid shares. Run the share-repair procedure in §11.4. If `reconstruction_error`
   is set, it names the failing check (for example a share-hash or
   payload-hash mismatch), which indicates corrupted local shares rather than
   merely missing ones — preserve them as evidence and re-fetch from independent
   peers.

### 11.4 Repair Missing Or Corrupted DA Shares

Use this for §6.4 (Repairs) and §7.3 (Lost DA Shares Or Payloads).

1. Identify the affected manifest and the exact missing shares:

   ```sh
   target/release/detta-client da-repair-status --manifest-hash <hash> --rpc <node-rpc>
   ```

   In the `DaRepairStatusReport`, `repair_needed`, `missing_share_indices`, and
   `pending_repair_count` define the work; `payload_reconstructable` tells you
   whether reconstruction is currently possible at all.

2. Fetch each missing share from **independent** peers (vary `--rpc` across at
   least two operators so a single bad source cannot dictate the result):

   ```sh
   target/release/detta-client da-share --manifest-hash <hash> --index <i> --rpc <peer-rpc>
   ```

3. Verify every fetched share before trusting it. The node validates a share's
   bytes against the committed `share_hash` and the manifest `share_root` on
   ingest, so a share from a peer that does not match is rejected automatically;
   confirm by re-running `da-status` and checking the share moved out of
   `missing_share_indices`. Record any peer that served a non-matching share for
   possible isolation or challenge (§11.8).
4. Once `stored_share_count` reaches the reconstruction threshold, confirm the
   payload reconstructs and is persisted:

   ```sh
   target/release/detta-client da-status --manifest-hash <hash> --rpc <node-rpc>
   ```

   Require `payload_reconstructable == true` and `reconstruction_error == null`.
5. Re-verify retention obligations (§11.5) and, if checkpoint shares were
   touched, re-run a sample-proof check (§11.6).
6. **If shares cannot be obtained from any honest peer** and signed custody
   cannot be served, escalate and follow the DA challenge/slashing path
   (§11.8 and `detta-da-operator-runbook.md`).

### 11.5 Verify DA Retention Obligations

Use this wherever the manual says "verify retention is satisfied".

```sh
target/release/detta-client da-retention-audit --rpc <node-rpc>
```

In the `DaRetentionAuditReport`:

- `unsatisfied_manifest_count` must be `0`. If non-zero, inspect `entries` for
  any item whose `retention_satisfied` is `false`.
- For each unsatisfied entry, `payload_retention_satisfied` and
  `share_retention_satisfied` localize the gap: a missing payload
  (`payload_present == false`) versus missing shares (`stored_share_count` <
  `expected_share_count` while the class sets `retain_all_shares == true`).
- `class` (`Hot`/`Warm`/`Cold`/`Checkpoint`) and `min_retention_blocks` show why
  the obligation still applies; `expired == true` means the obligation has
  lapsed and the data is a legitimate prune candidate (§6.3).

**Fix:** for a payload gap, reconstruct and persist it from shares (§11.4); for a
share gap on a `retain_all_shares` class, repair the missing shares (§11.4). Never
clear an unsatisfied obligation by pruning — confirm `retention_satisfied`
becomes `true` after repair.

### 11.6 Verify Light-Client Sample Proofs

Use this for weekly checks and after touching checkpoint shares.

```sh
target/release/detta-client da-sample-proofs --manifest-hash <hash> \
  --client-randomness <bytes> --sample-count <n> --rpc <node-rpc>
```

The response carries the deterministic `schedule` (derived from manifest hash,
block hash, and your `client-randomness`), one `sample_proof` per scheduled
index, optional `namespace_proofs`, and a `verification` report. Require
`verification.valid == true` and `verification.verified_share_count` equal to the
scheduled sample count. A failure means a sampled share did not prove inclusion
against the committed `share_root`; treat the serving node's data as suspect and
repair/re-fetch (§11.4).

### 11.7 Verify A DA Certificate Binds Its Block

Use this when diagnosing a finality stall (§7.5) or a disputed block.

1. Fetch the block header's DA commitment (`manifest_hash`, `payload_root`,
   `share_root`, `certificate_hash`) and the manifest and certificate:

   ```sh
   target/release/detta-client da-manifest --manifest-hash <hash> --rpc <node-rpc>
   target/release/detta-client da-certificate --certificate-hash <chash> --rpc <node-rpc>
   ```

2. Confirm the chain binds end to end: the manifest's `payload_hash`,
   `share_root`, `chain_id`, `height`, and `block_hash` must match the header
   commitment; the certificate's `manifest_hash`/`share_root`/`height`/
   `block_hash` must match the manifest; and the certificate `signers` must all
   be members of the active validator set and reach quorum.
3. **If any binding fails**, the certificate does not certify this block.
   Preserve the conflicting manifest/certificate, do not finalize on it, and
   escalate (§10). A manifest whose `share_root` does not encode its
   `payload_hash` is coding fraud; see §11.8.

### 11.8 Diagnose A Finality Stall And File DA Evidence

1. Quantify the stall from operator metrics on each validator:

   ```sh
   curl -s -X POST http://<node-rpc>/rpc -H 'content-type: application/json' \
     -d '{"bearer_token":"<operator-token>","request":{"method":"get_operator_metrics"}}' \
     | jq '{consensus_height, highest_finalized_height, finality_lag, peer_count,
            da_missing_share_count, da_custody_failure_count,
            da_pending_repair_record_count, da_oldest_pending_repair_age_blocks}'
   ```

2. Localize the cause:
   - `finality_lag` rising with healthy `peer_count` and zero DA counters points
     to a consensus/quorum problem, not DA — compare proposals and votes across
     validators and check for equivocation.
   - non-zero `da_missing_share_count` or `da_pending_repair_record_count`, or a
     large `da_oldest_pending_repair_age_blocks`, points to a DA availability
     stall — run §11.3/§11.4 on the stalled height's manifest.
   - non-zero `da_custody_failure_count` means a validator signed availability it
     cannot serve — identify it and prepare evidence.
3. File evidence for the relevant fault:
   - **Withheld/invalid challenged share:** open and track a share challenge.

     ```sh
     target/release/detta-client da-challenge --challenge-id <hash> --rpc <node-rpc>
     ```

     A challenge that expires without a valid response, or returns an invalid
     share, becomes slashable challenge evidence — follow
     `detta-da-operator-runbook.md`.
   - **Coding-inconsistent manifest** (its committed shares do not encode the
     committed payload): this is provable from the proposer's own data shares
     and slashes the block proposer. On a node that holds the manifest and all
     data shares, evaluate and capture the proof directly:

     ```sh
     target/release/detta-client da-coding-fraud-proof --manifest-hash <hash> --rpc <node-rpc>
     ```

     In the `DaCodingFraudReport`, `fault_detected == true` with a populated
     `proof` and `fault` (`ParityMismatch` or `PayloadHashMismatch`) is
     slashable coding fraud; retain the `proof` and submit it through the
     validator group's evidence process per `detta-da-operator-runbook.md` and
     `detta-data-availability-layer.md`. `fault_detected == false` with detail
     "manifest commits a valid encoding of its payload" clears the manifest; a
     detail noting fewer data shares than expected means coding correctness
     cannot be judged locally — fetch the remaining data shares first (§11.4).
4. Preserve all conflicting proposals, votes, manifests, and certificates before
   removing or quarantining any validator through governed metadata updates
   (§5.2). Resume only after a quorum agrees on roots and DA availability.
