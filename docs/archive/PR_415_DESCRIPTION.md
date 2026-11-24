# feat(vault): add owner-only sweep_idle_balance with settlement/revenue_pool routing

## Summary

Operators occasionally need to move surplus USDC out of the vault into the
settlement contract or revenue pool without going through `deduct`, which encodes
per-call business logic. This PR adds a dedicated `sweep_idle_balance` flow with
full auth, pause, and balance guards plus event coverage.

---

## Changes

### `contracts/vault/src/lib.rs`

- **`SweepDestination` enum** (`#[contracttype]`) — `Settlement` | `RevenuePool`
- **`SweptEventData` struct** (`#[contracttype]`) — `destination`, `amount`, `new_balance`
- **`sweep_idle_balance(env, owner, to, amount)`** entrypoint:
  - Owner-only (`owner.require_auth()` + explicit owner check)
  - Blocked when paused (`require_not_paused`)
  - Resolves destination address from `StorageKey::Settlement` or `StorageKey::RevenuePool`; returns `VaultError::DestinationNotConfigured` (code 37) if not set
  - Follows CEI pattern: balance decremented and event emitted **before** the external token transfer
  - Bumps instance storage TTL on every successful call
- **`VaultError::DestinationNotConfigured = 37`** — new error code

### `contracts/vault/src/test_sweep_idle_balance.rs` (new)

**13 tests** covering all acceptance criteria:

| Test | Covers |
|------|--------|
| `sweep_rejects_non_owner` | Auth — non-owner rejected with `Unauthorized` |
| `sweep_requires_owner_auth` | Auth — no mock → panics |
| `sweep_blocked_when_paused` | Pause guard |
| `sweep_settlement_not_configured` | `DestinationNotConfigured` for settlement |
| `sweep_revenue_pool_not_configured` | `DestinationNotConfigured` for revenue pool |
| `sweep_zero_amount_rejected` | `AmountNotPositive` |
| `sweep_negative_amount_rejected` | `AmountNotPositive` |
| `sweep_exceeds_balance_rejected` | `InsufficientBalance` |
| `sweep_partial_to_settlement` | Happy path — settlement, USDC transferred, balance decremented |
| `sweep_partial_to_revenue_pool` | Happy path — revenue pool |
| `sweep_full_balance_drain` | Amount == full balance → tracked balance 0 |
| `sweep_emits_event` | `swept` event shape: destination, amount, new_balance |
| `sweep_balance_consistency_after_multiple_sweeps` | Multi-sweep accounting consistency |

### `contracts/vault/src/events.rs`

- Added `event_swept_bytes` snapshot test asserting byte-level identity of `"swept"` topic

### `EVENT_SCHEMA.md`

- Added full `swept` event specification with field table, two JSON examples, and indexer notes
- Added `swept` row to the indexer quick-reference table
- Added version history entry

### `contracts/vault/STORAGE.md`

- Added `sweep_idle_balance` to the TTL-bump entrypoints list
- Added `sweep_idle_balance` row to the Core Vault Operations table
- Added version 1.3 history entry

---

## Acceptance criteria

- [x] Owner-only auth verified by test (`sweep_rejects_non_owner`, `sweep_requires_owner_auth`)
- [x] Emits `swept` event with `(destination, amount, new_balance)` payload (`sweep_emits_event`)
- [x] Updates `meta.balance` consistently with the on-ledger USDC move (`sweep_balance_consistency_after_multiple_sweeps`)

---

## Testing

```bash
cargo test -p callora-vault sweep_idle_balance
```

All 13 tests in `test_sweep_idle_balance.rs` pass.

---

closes #415
