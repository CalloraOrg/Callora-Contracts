# PR: overflow-safe math sweep in cold (Closes #686)

## Overview

Completes an **overflow-safe math sweep** of the Callora Vault's cold-storage
module (`contracts/vault/src/cold_storage.rs`). Every arithmetic operation in
the cold hot/cold-split and rebalance paths now routes through Rust's
`checked_*` operators, returning `VaultError::Overflow` on any overflow instead
of relying on debug assertions or silently wrapping in release builds.

**Closes: #686** ("Add overflow-safe math sweep in cold")

> Note on file path: issue #686 references `contracts/cold/src/lib.rs`. There is
> no standalone `cold` crate in this workspace — the cold-storage logic lives in
> the Vault contract at `contracts/vault/src/cold_storage.rs` (declared as
> `mod cold_storage` in `contracts/vault/src/lib.rs`). This PR targets that
> module, which is the canonical home of all "cold" math.

---

## What changed

The module was already largely checked-math clean (`total`, `hot_share_bps`,
`target_hot`, and the hot→cold move in `maybe_rebalance` all used `checked_*`).
The sweep closes the one remaining gap:

- **`maybe_rebalance` drift computation.** The drift between the current and
  target hot share was computed with raw `(current_share - target_share).abs()`.
  This is now `current_share.checked_sub(target_share)? .checked_abs()?`,
  mapping any overflow to `VaultError::Overflow`. `checked_abs` additionally
  guards the `i128::MIN` edge case, where `.abs()` panics.

No behaviour changes for in-range inputs — identical results, plus a defined
error return instead of a panic/wrap at the extremes.

## Safety properties

- **No raw arithmetic** remains in any non-test code path in the module.
- **No `unwrap()` in production paths** — overflow is surfaced as
  `Result<_, VaultError>` and propagated with `?`.
- **Conservation invariant** (`hot + cold == total`) is preserved by the
  overflow-safe rebalance path (asserted in tests).
- **Authorization** is unchanged: the cold-sweep multisig flow already gates
  every state-changing action behind configured cold signers and
  `require_auth`; this PR touches only pure arithmetic helpers and adds no new
  entrypoints.

## Tests

Added focused unit tests alongside the existing suite in the module's
`#[cfg(test)] mod tests`:

| Test | Covers |
|------|--------|
| `hot_share_bps_overflow_is_caught` | `checked_mul` overflow in share calc |
| `target_hot_overflow_is_caught` | `checked_mul` overflow in target calc |
| `target_hot_pub_matches_target_hot` | public wrapper parity + overflow |
| `maybe_rebalance_total_overflow_is_caught` | overflow via `total()` |
| `maybe_rebalance_drift_is_overflow_safe` | checked drift + conservation |
| `is_cold_signer_detects_membership` | signer-set membership helper |

These complement the pre-existing rebalance/validate tests, exercising every
`checked_*` branch introduced or touched by the sweep.

## Files changed

| File | Change |
|------|--------|
| `contracts/vault/src/cold_storage.rs` | overflow-safe drift + 6 new tests |
