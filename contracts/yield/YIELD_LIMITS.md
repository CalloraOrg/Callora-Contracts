# Per-Account Yield Limits (Issue #842 / task b#017, #1414)

This document describes the per-account state-cap surface of the
`callora-yield` crate (Issue #842, task b#017) and the operator-gated
enforcement model introduced by Issue #1414.

## Problem

A single Stellar account can repeatedly call yield-bearing entrypoints (deposit
yield → open bets / positions / subscriptions) to grief indexers, drain fees
via tiny repeated operations, or game the audit metrics by inflating the
yield-bearing UX without producing meaningful economic activity. The issue
requires per-account caps on the *number* of open bets, open positions, and
active subscriptions a single address may hold at any one time.

## Surface

The new contract `CalloraYieldLimits` (in `contracts/yield/src/limits.rs`)
exposes the following entrypoints. Re-exports via `contracts/yield/src/lib.rs`:

```rust
pub use limits::{
    AccountLimits, AccountState, CalloraYieldLimits, CalloraYieldLimitsClient,
};
pub use errors::YieldLimitError;
```

### Construction & admin

| Entrypoint                              | Auth          | Notes                                                                 |
|-----------------------------------------|---------------|-----------------------------------------------------------------------|
| `init(admin)`                           | `admin` signs | One-time setup; rejects with `AlreadyInitialized` after first call.   |
| `set_admin(caller, new_admin)`          | admin         | Two-step transfer, step 1.                                             |
| `accept_admin(caller)`                  | pending-admin | Two-step transfer, step 2.                                            |
| `cancel_admin_transfer(caller)`         | admin         | Drops a pending transfer.                                              |
| `upgrade(caller, new_wasm_hash)`        | admin         | Admin-only WASM swap.                                                  |
| `set_operator(caller, operator)`        | admin         | Register an address that may mutate per-account counters.             |
| `remove_operator(caller, operator)`     | admin         | Revoke a registration; unknown operators are a no-op.                 |

`init` calls `admin.require_auth()` **before** inspecting or writing storage.
Without that check the first caller between deploy and initialization could
install itself as admin and take over every admin entrypoint, `upgrade`
included. `init` therefore fails with an auth error (leaving no admin stored)
when the admin address has not signed.

### Limits configuration (admin only)

| Entrypoint                                                    | Effect                                                  |
|---------------------------------------------------------------|---------------------------------------------------------|
| `set_default_limits(caller, max_b, max_p, max_s)`             | Replace the global defaults.                             |
| `set_account_limits(caller, account, max_b, max_p, max_s)`   | Override the defaults for a single account.             |
| `clear_account_limits(caller, account)`                      | Revert the account to the global defaults.              |

### Counter mutators (operator-gated)

| Entrypoint                        | Effect                                                            |
|-----------------------------------|-------------------------------------------------------------------|
| `place_bet(operator, account)`    | Increment `account`'s open-bet counter; rejects at cap.           |
| `clear_bet(operator, account)`    | Decrement `account`'s open-bet counter; rejects on zero.          |
| `open_position(operator, account)`| Increment `account`'s open-position counter; rejects at cap.      |
| `close_position(operator, account)`| Decrement `account`'s open-position counter; rejects on zero.     |
| `subscribe(operator, account)`    | Increment `account`'s active-subscription counter; rejects at cap.|
| `unsubscribe(operator, account)`  | Decrement `account`'s active-subscription counter; rejects on zero.|

`operator` must sign the call and be registered with `set_operator`. The cap is
always enforced on `account`, which the operator names explicitly. Error
precedence is fixed: operator `require_auth`, then operator registration
(`Unauthorized`), then the cap (`BetsAtCap` / `PositionsAtCap` /
`SubscriptionsAtCap`), then the counter-underflow check.

Why this changed: counters used to be incremented by the account itself, so a
capped account could call `clear_bet` and then `place_bet` again forever — the
caps limited only honest accounts. With the operator model an account cannot
reset (or fake) its own counters: only a trusted, admin-registered backend that
observes real bets/positions/subscriptions can move them. `clear_*` is
decrement-only, so it cannot be used to mint headroom either.

### Read-only views (no auth)

| Entrypoint                                                | Effect                                                          |
|-----------------------------------------------------------|-----------------------------------------------------------------|
| `get_admin()`                                             | Current admin address or `NotInitialized`.                      |
| `get_default_limits()`                                    | Global cap defaults (fallback to `DEFAULT_LIMITS`).             |
| `get_account_limits(account)`                             | Effective caps for the account (per-account override → default). |
| `get_account_state(account)`                              | Live counters for the account (zeroed if absent).               |
| `get_operator(operator)`                                  | `true` when the address is a registered operator.                |
| `can_place_bet(account)` / `can_open_position(account)` / |                                                              |
| `can_subscribe(account)`                                  | Dry-run gate checks.                                           |

## Storage layout

- `Admin`, `PendingAdmin`, `DefaultLimits` and the migration markers stay in
  **instance** storage — they are read by every admin call and bounded in size.
- `AccountLimits(Address)` overrides live in **persistent** storage and are
  bumped with `CAPS_BUMP_THRESHOLD` / `CAPS_BUMP_AMOUNT` (≈7 days / ≈30 days)
  on every read and write. Per-account caps used to share the instance entry,
  which is loaded on every call and size-capped; configuring caps for many
  accounts grew that entry without bound and could evict other accounts' caps.
- `AccountState(Address)` is stored in **persistent** storage and bumped to
  `STATE_BUMP_AMOUNT` (≈30 days) on every read / write so accounts that go
  quiet do not silently archive.
- `Operator(Address)` is a **persistent** `bool` flag with the same
  `CAPS_BUMP_*` policy, refreshed whenever it is written or read by an
  authorized mutation.

### Storage migration (v1 → v2)

`STORAGE_MIGRATION_VERSION` is `2` and `STORAGE_LAYOUT_DESCRIPTOR` includes
`Operator`. Moving `AccountLimits` from instance to persistent storage does not
need a dedicated entrypoint: `read_account_limits` still checks the legacy
instance slot and, when the persistent slot is missing but the legacy one is
present, copies the value to persistent storage, extends its TTL, and removes
the instance copy. Writes and `clear_account_limits` remove both slots, so the
two layouts can never disagree. `set_default_limits`, `AccountState` and the
admin keys are unchanged.

### Defaults (`DEFAULT_LIMITS`)

```rust
pub const DEFAULT_LIMITS: AccountLimits = AccountLimits {
    max_bets: 100,
    max_positions: 50,
    max_subscriptions: 20,
};
```

These defaults are intentionally conservative so the contract is safe-by-default
even before admin sets `set_default_limits`. The admin may broadcast any
post-init override.

### Cap ceiling (`MAX_CAP`)

`MAX_CAP = 100_0000` (`1_000_000`). Any individual cap exceeds this triggers
`InvalidLimit` (code 4) instead of being persisted.

## Typed errors

`YieldLimitError` (in `contracts/yield/src/errors.rs`) is `#[contracterror]
#[repr(u32)]`. Numeric codes are stable and integrate with off-chain SDKs:

| Code | Variant               | Meaning                                                |
|------|-----------------------|--------------------------------------------------------|
| 1    | `NotInitialized`      | `init` was not called yet.                             |
| 2    | `AlreadyInitialized`  | `init` was called twice.                               |
| 3    | `Unauthorized`        | Caller is not permitted for this operation.            |
| 4    | `InvalidLimit`        | A configured cap exceeds `MAX_CAP`.                    |
| 5    | `BetsAtCap`           | Account's open-bet counter is at the cap.              |
| 6    | `PositionsAtCap`      | Account's open-position counter is at the cap.         |
| 7    | `SubscriptionsAtCap`  | Account's active-subscription counter is at the cap.   |
| 8    | `CounterUnderflow`    | `clear_*` called when the corresponding counter is 0.  |
| 9    | `Overflow`            | `u32` counter overflow during increment.               |

## Events

`contracts/yield/src/events.rs` centralizes every topic as a `Symbol::new`
call and pins each to its byte representation in `#[cfg(test)]` snapshot
tests. Topic vocabulary (`init`, `admin_*`, `operator_set`, `operator_removed`,
`default_limits_set`, `account_limits_set`, `account_limits_cleared`,
`bet_placed`, `bet_cleared`, `position_opened`, `position_closed`,
`subscription_added`, `subscription_removed`, `upgraded`) follows the
past-tense-verb pattern documented in `CONTRIBUTING.md`.

Counter events carry `(topic, operator, account)` as the first topic element so
indexers can attribute every counter change to the operator that signed it.

## Backward compatibility

- `contracts/yield/src/lib.rs` continues to re-export
  `RevenuePool` / `RevenuePoolClient` unchanged.
- No prior type, storage key, or event topic was renamed. `StorageKey::Operator`
  was appended last, so every pre-existing discriminant is stable.
- The counter mutators are a **breaking signature change** on purpose (#1414):
  they now take `(operator, account)` instead of `(caller)`. A consumer that
  calls them must be registered as an operator; the old caller-only form is
  gone because it allowed self-reset of the caps.
- Existing instance-stored caps keep working through the read-time fallback
  described above, so an upgrade does not drop configured overrides.

## Verification

```bash
# Format, build, run unit tests & auth-snap integration suite.
cargo fmt  -p callora-yield
cargo test -p callora-yield --lib              # 70+ unit tests
cargo test -p callora-yield --test auth_snap   # operator, admin and view assertions

# Build for the wasm32 target so WASM-bound assets do compile.
cargo build -p callora-yield --target wasm32-unknown-unknown --release
```

All unit tests must pass, every mutator assertion in `auth_snap.rs` must report
`res.is_err()`, and the view assertions must run without `require_auth`.

## Coverage summary

- ~80 unit tests for caps, defaults, storage helpers, dry-run checks, admin
  rotation, operator registration and the instance → persistent cap fallback.
- Authentication-snapshot tests covering `init`, the admin setters, the
  operator registry, every counter mutator, `upgrade`, and the no-auth views.
- Byte-snapshot tests pinning event-topic symbol identity.
