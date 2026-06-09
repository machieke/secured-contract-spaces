# DeTTa Restricted MeTTa Aspect Language

DeTTa admits programmable DeFi behavior through a restricted, taxonomy-aligned
MeTTa aspect language. It is not full MeTTa and it is not a general Atomspace
mutation interface.

## Accepted Source Forms

The current production subset accepts:

- type declarations, for example `(: Amount Type)`;
- `aspect` and `abstract-aspect` declarations;
- `aspect-extends`, `aspect-conflicts`, `aspect-provides`, and
  `aspect-requires`;
- `owns` declarations for aspect-owned numeric state;
- `local-invariant` declarations;
- `action` declarations and `derived` action bodies;
- `bundle`, `bundle-includes`, and `bundle-extends`;
- `projection` declarations for callable methods;
- `method-abi` declarations;
- `method-policy` declarations with authority, effects, and invariants.

The accepted executable subset is deterministic and bounded. It supports
checked arithmetic, comparisons, conditionals, `require`, `seq`, `state-get`,
`state-set!`, `emit!`, deterministic block-height reads, and approved kernel
adapters such as `permit-verify!` and `bridge-verify!`.

## Forbidden Full-MeTTa Behavior

The runtime rejects:

- raw Atomspace add/remove/update primitives;
- arbitrary host interop;
- reflection over private contract state;
- dynamic code loading or runtime parser access;
- nondeterministic time, randomness, network, file, process, or thread access;
- unbounded recursion or loops;
- raw storage-key construction outside declared aspect schema;
- mutation effects missing from the method policy;
- authority derived from syntax alone rather than transaction context,
  registry grants, or kernel-verified certificates.

## Admission Gates

A module is admitted only if parsing, canonicalization, lowering, verifier
checks, root generation, and artifact validation all pass. The verifier checks
bundle closure, required facets, conflict exclusion, unique storage ownership,
projection ABI/policy coverage, declared effects, write scopes, and invariant
references.

## Checked Standard Library Coverage

The checked-in standard-library fixture is
`models/aspects/stdlib/minimal-transfer-token.metta`. It is broader than the
historical filename: the module currently contains 14 deployable token bundles
and covers ERC20-like transfer/approval, permit approval, configurable fees,
pauses, blocked-account restrictions, account locks, minting, burning, mint
caps, votes, snapshots, vault shares, wrapping, rewarded staking, and bridge
mint/burn through `bridge-verify!`.

`models/aspects/stdlib/minimal-transfer-token.artifact.json` records the
source, IR, ABI, policy, storage-schema, registry-schema, and invariant roots.
`models/aspects/stdlib/minimal-transfer-token.proof-obligations.json` records
the programmable-module obligations that tie the parser, verifier, evaluator,
host calls, and TLA+ model coverage together. The release gate validates the
source, artifact, and proof-obligation SHA-256 attestations.

## Baseline Precompiles

Native DeTTa DeFi contracts are baseline precompiles used for genesis,
compatibility, differential testing, and emergency migration. New programmable
token behavior should be expressed as verified aspect bundles. Adding a new
business rule to native Rust is a precompile change, not normal application
development.

## Taxonomy Version Pinning

Each accepted module records `taxonomy_version`. Deployment clients and
validators must treat the taxonomy version as part of the artifact identity.
Changing taxonomy semantics requires a new version and new artifact roots; a
module compiled under one taxonomy version must not be silently interpreted
under another.
