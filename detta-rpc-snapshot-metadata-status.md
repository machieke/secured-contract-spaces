# DeTTa Snapshot Metadata Root Status RPC

`get_snapshot_metadata_root_status` is a persistent-validator-node JSON RPC
method for clients that need to audit which snapshot metadata roots a node will
serve and why.

Request:

```json
{"method":"get_snapshot_metadata_root_status"}
```

Successful response:

```json
{
  "status": "ok",
  "body": {
    "result": "snapshot_metadata_root_status",
    "data": {
      "...": "see crates/detta-rpc/src/lib.rs fixture"
    }
  }
}
```

The stable JSON fixture is covered by
`snapshot_metadata_root_status_json_fixture_is_stable` in
`crates/detta-rpc/src/lib.rs`.

## Field Semantics

Each metadata family reports three root slots:

- `<family>_root`: the effective root clients should compare against manifests
  and audit reports.
- `local_<family>_root`: the root computed from local durable node state, or
  `null` when that local material does not exist.
- `persisted_<family>_root`: the root loaded from the last imported snapshot
  manifest metadata, or `null` when no imported manifest supplied it.

Each family also reports:

- `using_imported_<family>_root`: true when the effective root is imported
  metadata rather than local material.
- `persisted_matches_local_<family>_root`: true when an imported root exists and
  matches the local root.

## Root Precedence

Validator-set audit roots are intentionally conservative: an imported
`validator_set_metadata_audit_root` remains effective until a local validator-set
metadata audit write replaces the stale imported root.

The remaining metadata roots prefer local material when present, matching
snapshot manifest emission:

- `snapshot_sync_client_metrics_root`
- `required_snapshot_metadata_roots_root`
- `snapshot_import_audit_root`
- `snapshot_import_audit_config_root`

For those roots, imported metadata is effective only when local material is
absent.

## Client Use

Light clients and indexers should use the effective `<family>_root` fields when
validating a snapshot manifest. They should use the local/persisted fields and
the booleans to explain provenance: local, imported, matching, or replaced.
