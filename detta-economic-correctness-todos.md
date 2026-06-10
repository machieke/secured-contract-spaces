# DeTTa Economic Correctness TODOs

This checklist tracks the completed economic-correctness pass across native
DeFi contracts and restricted MeTTa aspect flows.

Status: complete for the current DeTTa runtime scope. Remaining future work is
the broader cross-contract host-capability extension for aspect modules that
need to move external ledgers beyond their own aspect-owned accounting state.

## AMM Settlement

- [x] Native AMM `addLiquidity` debits provider balances for both pool assets.
- [x] Native AMM `addLiquidity` credits pool-owned balances for both pool
  assets.
- [x] Native AMM `swap` debits trader input balance and credits trader output
  balance.
- [x] Native AMM `swap` updates pool-owned balances consistently with reserves
  and collected fees.
- [x] AMM liquidity and swap failures roll back both balances and reserves.
- [x] AMM rejects unsupported assets instead of treating arbitrary strings as
  spendable tokens.
- [x] Aspect-token AMM uses the guarded internal aspect-transfer adapter for
  deployed aspect-token contracts that export `ERC20-transfer`.
- [x] E2E tests assert user balances, pool balances, reserves, fees, receipts,
  events, and proof roots.

## Lending Settlement

- [x] Collateral deposit debits borrower collateral-asset balances and credits
  vault-owned balances.
- [x] Borrow credits borrower debt-asset balances and debits vault-owned debt
  liquidity or fails if the vault lacks liquidity.
- [x] Liquidation transfers collateral and debt assets between liquidator,
  borrower, and vault, not only accounting keys.
- [x] Lending failures roll back collateral, debt, and token balances.

## Staking Settlement

- [x] Stake debits staker asset balances and credits staking-contract custody.
- [x] Unstake and complete-unbond credit released assets back to the staker and
  keep penalty custody explicit.
- [x] Staking reward claims mint rewards as explicit protocol emissions through
  the token ledger.
- [x] Staking failures roll back stake accounting and token balances.

## Bridge Settlement

- [x] Native bridge redemption mints or credits destination token balances
  through an explicit token adapter.
- [x] Native bridge outbound messages debit or lock source balances before
  queueing a message.
- [x] Aspect bridge mint/burn keeps certificate replay protection and supply
  accounting bound to the aspect token.
- [x] Aspect bridge mint/burn rejects zero-amount no-ops before replay state is
  consumed or supply accounting is touched.

## Restricted MeTTa Aspect Modules

- [x] Token-like aspects preserve supply/balance invariants across initialize,
  transfer, fee, mint, burn, cap, snapshot, wrap, stake, vault-share, and bridge
  projections.
- [x] Mint, burn, bridge mint, and bridge burn reject zero amounts so no-op
  privileged actions cannot emit misleading events or consume replay IDs.
- [x] Aspect vault/wrap/stake projections are tested and documented as
  aspect-owned accounting modules unless a kernel adapter is explicitly used.
- [x] Aspect method policies declare all economically relevant effects.
- [x] Native-vs-aspect and dedicated aspect tests compare balances, total
  supply, events, and revert roots for the supported executable bundles.

## Formal And Release Artifacts

- [x] Update release-gate E2E coverage for the economic settlement fixes.
- [x] Update docs to distinguish complete settlement from accounting-only
  projections.
- [x] Preserve proof/theorem artifacts and rerun the release gate, including
  TLA+ model checking and proof hash manifests.

## Verification Evidence

- `DETTA_E2E_FULL=0 scripts/detta-release-gate.sh` passed.
- `cargo test -p detta-core -- --test-threads=1` passed.
- `cargo test -p detta-e2e --test protocol_client_flows -- --test-threads=1`
  passed.
- `cargo test -p detta-e2e --test defi_method_edge_flows -- --test-threads=1`
  passed.
- `cargo test -p detta-e2e --test four_validator_convergence_flows --
  --test-threads=1` passed.
- `cargo test -p detta-core mint_burn_token_aspect_updates_supply_and_balances`
  passed.
- `cargo test -p detta-core bridge_mint_burn_aspect_requires_verified_bridge_certificate`
  passed.
