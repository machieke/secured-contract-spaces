# DeTTa: Toward Verifiable DeFi In A Secured Atomspace

Most DeFi systems rely on smart contracts to protect assets. That sounds
simple, but the hard part is not writing a transfer function or an AMM formula.
The hard part is making sure every state change happens through the right
authority path, commits atomically, can be replayed by independent nodes, and
comes with enough evidence for auditors and automated verifiers to check what
happened.

This repository explores that problem from two sides.

The first side is **Secured Contract Spaces for MeTTa DeFi**. A secured
contract space is a contract-owned Atomspace with a strict runtime boundary:
external users do not directly mutate state atoms. They call exported methods;
the runtime authenticates the call, grants only method-scoped write authority,
checks policy and capability registries, executes deterministically, and commits
only if all invariants still hold. That model is meant to make DeFi safety a
runtime property, not just a convention each contract author has to remember.

The second side is **DeTTa**, short for **Distributed Transactional
Atomspace**. DeTTa is the Rust implementation effort that turns the secured
contract space model into a replicated DeFi runtime. It includes deterministic
contract execution, validator consensus, persistent mempools, signed protocol
messages, state sync, JSON-RPC, proofs, and end-to-end client workflows. It
also includes secure programmable MeTTa aspect modules for token behavior, so
features like fees, pauses, restrictions, minting, snapshots, vault shares,
staking rewards, and bridge adapters can be described as restricted,
verifiable behavior instead of being hard-coded forever.

The repository has also grown a production-candidate data availability layer.
For DeFi, it is not enough for a block to have a valid state root. Validators,
full nodes, light clients, and auditors need the data required to reconstruct
and replay the block. DeTTa's DA layer commits canonical payloads into
namespaces, encodes them with Reed-Solomon shares, binds them through manifests
and share roots, requires custody-checked validator DA votes, and exposes
repair, sampling, retention, and audit evidence through RPC.

Why does this matter?

Because DeFi systems fail at the boundaries: an unchecked caller identity, a
missing invariant, an upgrade that bypasses governance, a bridge proof accepted
without the right certificate, or a finalized block whose data cannot be
retrieved later. This repository is trying to make those boundaries explicit
and machine-checkable. It connects a security specification, a formal model, a
deterministic executor, a distributed validator runtime, restricted MeTTa
programming, client-facing DeFi flows, and operational evidence into one
workspace.

The current implementation is still a research and production-track candidate,
not an audited mainnet. But it already demonstrates the shape of a more
defensible DeFi stack:

- contracts live inside secured runtime-enforced spaces;
- programmable token behavior is restricted and verified before deployment;
- every block can be replayed against committed roots;
- consensus finality can require data availability certificates;
- data needed for audit and state sync is retained, indexed, and repairable;
- release, readiness, incident, and operator procedures are part of the system,
  not an afterthought.

The larger idea is that DeFi should not have to choose between expressive
contract logic and rigorous operational safety. A secured Atomspace runtime can
give developers a programmable model while giving validators, users, and
auditors stronger guarantees about authority, determinism, invariants, and
availability.

That is what this repository is building toward: a DeFi runtime where the
important promises are not just written down, but enforced, replicated,
replayed, and eventually proven.
