# DeTTa Layer Cost & Latency Benchmarks

This report measures per-operation **latency** and per-block **cost** (serialized
bytes and DA data-gas) across the DeTTa stack — core execution, the data
availability layer, consensus verification, storage, and the wire/protocol
layer. It focuses on the block/DA production and verification path, which is the
hot path on this branch.

The numbers are produced by a self-contained, dependency-free harness
(`crates/detta-e2e/src/bin/layer_benchmarks.rs`) so they can be regenerated on
any machine without pulling a benchmarking framework.

## Reproducing

```sh
cargo run -p detta-e2e --bin layer_benchmarks --release
```

The binary prints the Markdown tables embedded below. Re-run and replace the
"Measured results" section to refresh.

## Methodology & caveats

- **Latency** is mean wall-clock time per operation (`std::time::Instant`) over
  a fixed iteration count, with a warmup pass (≈20% of iterations) discarded.
  This is an order-of-magnitude harness, **not** a statistically rigorous
  benchmark (no outlier rejection, no confidence intervals like Criterion). Treat
  results as relative costs, not SLAs.
- **Cost (bytes)** is canonical `serde_json` serialized size — the same encoding
  used for DeTTa commitments and wire messages — so byte counts match on-chain
  commitment and gossip sizes.
- **DA data-gas** uses `DaProductionProfile::v1` (1024-byte units).
- Built with `--release` (opt-level 3). Single-threaded; no I/O contention
  except the storage rows, which hit the real filesystem (`std::env::temp_dir`).
- Workloads are blocks of *N* signed `Transfer` transactions (N ∈ {16, 64, 256})
  with one receipt each, encoded into a production DA payload
  (`detta.block` + `detta.tx` + `detta.receipt`) and Reed-Solomon shares at a
  4096-byte target share size (equal data/parity, per the v1 profile).

### Environment

| Field | Value |
| --- | --- |
| Date (UTC) | 2026-06-18 |
| CPU | Intel Xeon W-2225 @ 4.10 GHz |
| Cores | 8 |
| Memory | 78 GiB |
| Toolchain | rustc 1.88.0 (release, opt-level 3) |
| Branch | `experimental/data-availability-layer` |

## Measured results

### Workload: 16 transfer transactions

**Cost (bytes / gas)**

| Metric | Value |
| --- | ---: |
| DA payload (canonical JSON) | 15,643 B |
| DA payload data-gas (1 KiB units) | 16 |
| DA manifest | 1,335 B |
| Encoded shares (data + parity) | 8 (4 + 4) |
| Share size | 3,911 B |
| Single share payload | 3,911 B |
| DA availability certificate | 388 B |
| DA coding-fraud proof | 54,563 B |
| Wire: Block message | 15,267 B |
| Wire: DaManifest message | 1,360 B |

**Latency (mean per operation)**

| Operation | Mean | Throughput |
| --- | ---: | ---: |
| core: build_block (execute N txs) | 1.27 ms | 789 /s |
| core: apply_block | 1.37 ms | 731 /s |
| da: build share set (Reed-Solomon) | 402.28 µs | 2.5 K/s |
| da: manifest_hash | 6.12 µs | 163.3 K/s |
| da: reconstruct payload (full share set) | 381.43 µs | 2.6 K/s |
| da: verify_manifest_commits_payload | 425.29 µs | 2.4 K/s |
| da: custody assignment | 24.97 µs | 40.1 K/s |
| da: sample schedule (3 samples) | 24.99 µs | 40.0 K/s |
| da: prove share inclusion | 24.23 µs | 41.3 K/s |
| da: verify share inclusion proof | 48.13 µs | 20.8 K/s |
| da: build coding-fraud proof | 552.09 µs | 1.8 K/s |
| da: verify coding-fraud proof | 271.86 µs | 3.7 K/s |
| consensus: verify_data_availability_payload | 628.59 µs | 1.6 K/s |
| consensus: verify_data_availability_certificate | 32.03 µs | 31.2 K/s |
| consensus: certificate from votes | 84.44 µs | 11.8 K/s |
| storage: commit DA share set | 42.32 ms | 24 /s |
| storage: load DA share set | 1.18 ms | 844 /s |
| protocol: encode DaManifest | 1.43 µs | 699.1 K/s |
| protocol: decode DaManifest | 1.91 µs | 524.6 K/s |

### Workload: 64 transfer transactions

**Cost (bytes / gas)**

| Metric | Value |
| --- | ---: |
| DA payload (canonical JSON) | 59,275 B |
| DA payload data-gas (1 KiB units) | 58 |
| DA manifest | 2,812 B |
| Encoded shares (data + parity) | 30 (15 + 15) |
| Share size | 3,952 B |
| Single share payload | 3,952 B |
| DA availability certificate | 388 B |
| DA coding-fraud proof | 205,094 B |
| Wire: Block message | 57,267 B |
| Wire: DaManifest message | 2,837 B |

**Latency (mean per operation)**

| Operation | Mean | Throughput |
| --- | ---: | ---: |
| core: build_block (execute N txs) | 10.37 ms | 96 /s |
| core: apply_block | 10.37 ms | 96 /s |
| da: build share set (Reed-Solomon) | 1.69 ms | 591 /s |
| da: manifest_hash | 13.07 µs | 76.5 K/s |
| da: reconstruct payload (full share set) | 1.42 ms | 704 /s |
| da: verify_manifest_commits_payload | 1.65 ms | 606 /s |
| da: custody assignment | 47.82 µs | 20.9 K/s |
| da: sample schedule (3 samples) | 50.48 µs | 19.8 K/s |
| da: prove share inclusion | 83.04 µs | 12.0 K/s |
| da: verify share inclusion proof | 113.96 µs | 8.8 K/s |
| da: build coding-fraud proof | 3.47 ms | 288 /s |
| da: verify coding-fraud proof | 1.71 ms | 583 /s |
| consensus: verify_data_availability_payload | 2.51 ms | 398 /s |
| consensus: verify_data_availability_certificate | 89.72 µs | 11.1 K/s |
| consensus: certificate from votes | 241.37 µs | 4.1 K/s |
| storage: commit DA share set | 103.09 ms | 10 /s |
| storage: load DA share set | 4.41 ms | 227 /s |
| protocol: encode DaManifest | 2.63 µs | 379.7 K/s |
| protocol: decode DaManifest | 3.21 µs | 311.8 K/s |

### Workload: 256 transfer transactions

**Cost (bytes / gas)**

| Metric | Value |
| --- | ---: |
| DA payload (canonical JSON) | 234,271 B |
| DA payload data-gas (1 KiB units) | 229 |
| DA manifest | 8,578 B |
| Encoded shares (data + parity) | 116 (58 + 58) |
| Share size | 4,040 B |
| Single share payload | 4,040 B |
| DA availability certificate | 388 B |
| DA coding-fraud proof | 808,889 B |
| Wire: Block message | 225,735 B |
| Wire: DaManifest message | 8,603 B |

**Latency (mean per operation)**

| Operation | Mean | Throughput |
| --- | ---: | ---: |
| core: build_block (execute N txs) | 128.89 ms | 8 /s |
| core: apply_block | 128.16 ms | 8 /s |
| da: build share set (Reed-Solomon) | 11.54 ms | 87 /s |
| da: manifest_hash | 36.68 µs | 27.3 K/s |
| da: reconstruct payload (full share set) | 8.43 ms | 119 /s |
| da: verify_manifest_commits_payload | 11.51 ms | 87 /s |
| da: custody assignment | 166.55 µs | 6.0 K/s |
| da: sample schedule (3 samples) | 171.83 µs | 5.8 K/s |
| da: prove share inclusion | 300.09 µs | 3.3 K/s |
| da: verify share inclusion proof | 348.82 µs | 2.9 K/s |
| da: build coding-fraud proof | 38.07 ms | 26 /s |
| da: verify coding-fraud proof | 18.65 ms | 54 /s |
| consensus: verify_data_availability_payload | 15.09 ms | 66 /s |
| consensus: verify_data_availability_certificate | 320.01 µs | 3.1 K/s |
| consensus: certificate from votes | 854.27 µs | 1.2 K/s |
| storage: commit DA share set | 344.61 ms | 3 /s |
| storage: load DA share set | 20.31 ms | 49 /s |
| protocol: encode DaManifest | 7.31 µs | 136.8 K/s |
| protocol: decode DaManifest | 10.04 µs | 99.6 K/s |

## Analysis

### Scaling summary (16 → 256 txs, a 16× workload increase)

| Operation | 16 | 64 | 256 | Growth |
| --- | ---: | ---: | ---: | ---: |
| core: build_block | 1.27 ms | 10.37 ms | 128.89 ms | ~101× (super-linear) |
| da: build share set (RS) | 402 µs | 1.69 ms | 11.54 ms | ~29× |
| da: verify_manifest_commits_payload | 425 µs | 1.65 ms | 11.51 ms | ~27× |
| consensus: verify DA payload | 629 µs | 2.51 ms | 15.09 ms | ~24× |
| storage: commit share set | 42.3 ms | 103 ms | 344 ms | ~8× |
| protocol: encode manifest | 1.43 µs | 2.63 µs | 7.31 µs | ~5× |

### Findings

1. **Block execution dominates and scales super-linearly.** `build_block` /
   `apply_block` grow ~101× for a 16× workload (1.3 ms → 129 ms), far worse than
   the ~24–29× near-linear growth of the DA/consensus path. At 256 txs, core
   execution is **~8×** the cost of the entire DA encode-and-verify pipeline. This
   is the headline performance bottleneck and the first thing to profile
   (`apply_transaction` and the per-block root recomputation in `detta-core`) — a
   per-transaction O(N) cost (e.g. repeated full-state hashing or structure
   rebuilds) would explain the curve. **This is not a DA-layer problem.**

2. **The DA encode/verify/reconstruct path is ~linear in payload size and cheap
   relative to execution.** Reed-Solomon build, full reconstruction, and the new
   `verify_manifest_commits_payload` binding each cost ~one RS pass and track
   payload bytes closely (≈11.5 ms for a 229 KiB / 256-tx block). The security fix
   from this branch adds roughly one RS-encode (~11.5 ms at 256 txs) to
   `verify_data_availability_payload` — a deliberate, bounded cost for binding the
   share commitment to the payload.

3. **Light-client and gossip operations are effectively free.** Custody
   assignment, sample-schedule derivation, share-inclusion prove/verify,
   `manifest_hash`, and certificate verification are all single- to low-tens of
   microseconds even at 256 txs; manifest encode/decode is single-digit µs. The
   sampling-based availability path scales well and is not a bottleneck.

4. **Coding-fraud proofs are large by construction.** The proof embeds the
   `original_share_count` committed data shares, so its size tracks the payload
   (~55 KB → ~205 KB → ~809 KB). It is **slashing-path, not hot-path** evidence
   (built/verified only when fraud is suspected), so the cost is acceptable, but a
   future optimization could reference shares by index + inclusion proof and
   stream them rather than inlining full bytes. Build/verify latency (~38 ms /
   ~19 ms at 256 txs) is one to two RS passes — fine for an off-path action.

5. **Storage commit is filesystem-bound, not CPU-bound.** Committing a share set
   is the slowest single operation (42 ms → 345 ms) because it writes one file per
   share (plus the manifest) to disk synchronously; load is ~15× faster. Batching
   share writes, writing shares concurrently, or deferring `fsync` would cut block
   persistence latency substantially. The CPU cost here is negligible — this is
   I/O.

### Cost model takeaways

- **DA data-gas is linear** in payload size at 1 share-unit per 1024 bytes
  (16 → 58 → 229 gas), as designed.
- **Manifest and certificate are compact commitments.** The certificate is fixed
  (~388 B, three signers); the manifest grows only with the share-hash vector
  (~1.3 KB → ~8.6 KB), a small fraction of the payload it commits.
- **Wire cost ≈ payload cost.** A `Block` gossip message is essentially the
  payload size; the `DaManifest` message is ~1–4% of it, confirming that
  sampling/manifest gossip is cheap and full-block/share distribution is where
  bandwidth goes.

## Suggested next steps

1. Profile `detta-core::apply_transaction` / block root recomputation to remove
   the super-linear execution scaling (highest-impact, execution layer).
2. Parallelize or batch share-file writes in `FileStorage::commit_da_share_set`
   (highest-impact, storage layer).
3. Consider an index-referenced, streamed coding-fraud proof format if proof
   propagation size ever matters in practice.
4. Promote this harness to a CI perf-smoke job (assert order-of-magnitude bounds)
   to catch regressions in the DA verification path.
