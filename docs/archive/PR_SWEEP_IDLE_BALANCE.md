# feat(vault): add owner-only `sweep_idle_balance` with settlement/revenue_pool routing

## Summary

Adds a dedicated `sweep_idle_balance(env, owner, to: SweepDestination, amount)` entrypoint to `CalloraVault` that lets operators move surplus USDC out of the vault into a configured sibling contract (settlement or revenue pool) without going through `deduct` (which encodes per-call business logic, rate-limiting, and settlement notifications).

## Changes

### `contracts/vault/src/lib.rs`
- Added `SweepDestination` `#[contracttype]` enum with `Settlement` and `RevenuePool` variants.
- Added `pub fn sweep_idle_balance(env, owner, to: SweepDestination, amount) -> Result<i128, VaultError>` entrypoint.
  - Owner-only auth via `owner.require_auth()` + owner identity check.
  - Blocked when paused (`VaultError::Paused`).
  - Rejects `amount <= 0` (`VaultError::AmountNotPositive`).
  - Rejects `amount > meta.balance` (`VaultError::InsufficientBalance`).
  - Rejects unconfigured destination: `Settlement` → `VaultError::SettlementNotSet`; `RevenuePool` → `VaultError::NotInitialized`.
  - Follows CEI: state written and event emitted before the external token transfer.
  - Bumps instance TTL on success.

### `contracts/vault/src/events.rs`
- Added `pub fn event_swept(env: &Env) -> Symbol` topic constructor (`"swept"`).
- Added snapshot test `test_event_swept_bytes` verifying byte identity.

### `contracts/vault/src/test.rs`
- Added `setup_sweep_vault` helper.
- 12 new tests covering:
  - Happy paths: sweep to settlement, sweep to revenue pool.
  - Event schema: `swept` topics and `(amount, new_balance)` data payload.
  - Failure paths: paused, unauthorized, zero amount, negative amount, insufficient balance.
  - Edge cases: sweep full balance, settlement not configured, revenue pool not configured, partial sweeps, balance unchanged on failure.

### `EVENT_SCHEMA.md`
- Added `swept` event schema section with topic/data table and JSON example.
- Added `swept` to the event index table.
- Added version history entry `0.2.0`.

### `contracts/vault/STORAGE.md`
- Added `sweep_idle_balance` row to the Core Vault Operations table.
- Added version history entry `1.3`.

## Acceptance Criteria

| Criterion | Status |
|-----------|--------|
| Owner-only auth verified by test | ✅ `sweep_fails_unauthorized` |
| Emits `swept` event with `(destination, amount, new_balance)` payload | ✅ `sweep_emits_swept_event` |
| Updates `meta.balance` consistently with the on-ledger USDC move | ✅ `sweep_to_settlement_succeeds`, `sweep_partial_amount_correct` |
| Blocked when paused | ✅ `sweep_fails_when_paused` |
| Rejects zero / negative amount | ✅ `sweep_fails_zero_amount`, `sweep_fails_negative_amount` |
| Rejects amount > balance | ✅ `sweep_fails_insufficient_balance` |
| Rejects unconfigured settlement destination | ✅ `sweep_to_settlement_fails_when_not_configured` |
| Rejects unconfigured revenue pool destination | ✅ `sweep_to_revenue_pool_fails_when_not_configured` |
| No `unwrap()` in production paths | ✅ All errors use `?` / `ok_or` |
| NatSpec-style `///` doc comments | ✅ Full doc on `sweep_idle_balance` and `SweepDestination` |
| Documented in `EVENT_SCHEMA.md` and `STORAGE.md` | ✅ |

closes #415
