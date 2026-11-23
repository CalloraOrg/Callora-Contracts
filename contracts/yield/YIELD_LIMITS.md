# Per-Account Yield Limits (Issue #842 / task b#017)

This document describes the new per-account state-cap surface added to the
`callora-yield` crate for the GrantFox FWC26 audit metric campaign (Issue #842,
task b#017).

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
| `init(admin)`                           | 1-shot        | One-time setup; rejects with `AlreadyInitialized` after first call.   |
| `set_admin(caller, new_admin)`          | admin         | Two-step transfer, step 1.                                             |
| `accept_admin(caller)`                  | pending-admin | Two-step transfer, step 2.                                            |
| `cancel_admin_transfer(caller)`         | admin         | Drops a pending transfer.                                              |
| `upgrade(caller, new_wasm_hash)`        | admin         | Admin-only WASM swap.                                                  |

### Limits configuration (admin only)

| Entrypoint                                                    | Effect                                                  |
|---------------------------------------------------------------|---------------------------------------------------------|
| `set_default_limits(caller, max_b, max_p, max_s)`             | Replace the global defaults.                             |
| `set_account_limits(caller, account, max_b, max_p, max_s)`   | Override the defaults for a single account.             |
| `clear_account_limits(caller, account)`                      | Revert the account to the global defaults.              |
| `set_window_ledgers(caller, window_ledgers)`                  | Set the rolling-window length in ledgers (`0` disables).|
| `set_window_limits(caller, max_b, max_p, max_s)`             | Set the per-window open caps.                           |

### User counter mutators (caller-authenticated)

| Entrypoint                  | Effect                                                          |
|-----------------------------|-----------------------------------------------------------------|
| `place_bet(caller)`         | Increment caller's open-bet counter; rejects at cap.            |
| `clear_bet(caller)`         | Decrement caller's open-bet counter; rejects on zero.           |
| `open_position(caller)`     | Increment caller's open-position counter; rejects at cap.       |
| `close_position(caller)`    | Decrement caller's open-position counter; rejects on zero.      |
| `subscribe(caller)`         | Increment caller's active-subscription counter; rejects at cap. |
| `unsubscribe(caller)`       | Decrement caller's active-subscription counter; rejects on zero.|

### Read-only views (no auth)

| Entrypoint                                                | Effect                                                          |
|-----------------------------------------------------------|-----------------------------------------------------------------|
| `get_admin()`                                             | Current admin address or `NotInitialized`.                      |
| `get_default_limits()`                                    | Global cap defaults (fallback to `DEFAULT_LIMITS`).             |
| `get_account_limits(account)`                             | Effective caps for the account (per-account override → default). |
| `get_account_state(account)`                              | Live counters for the account (zeroed if absent).               |
| `can_place_bet(account)` / `can_open_position(account)` / |                                                              |
| `can_subscribe(account)`                                  | Dry-run gate checks (concurrent **and** window aware).         |
| `get_window_ledgers()`                                    | Rolling-window length in ledgers (`0` = disabled).             |
| `get_window_limits()`                                     | Global per-window caps.                                        |
| `get_window_state(account)`                               | Stored per-window counters (zeroed if absent; does not roll).  |

## Rolling time windows (Issue #1522)

The concurrent caps above never reset. An optional rolling window adds a
second, time-scoped layer that bounds how much an account can **open** inside
a configurable number of ledgers:

- `window_ledgers` is a global admin setting. `0` (the default) disables
  windows and restores the pre-#1522 behaviour exactly: no window storage is
  read, written, or enforced. Non-zero values must be within
  `MIN_WINDOW_LEDGERS..=MAX_WINDOW_LEDGERS` (`1..=1_000_000`).
- `WindowLimits { max_bets, max_positions, max_subscriptions }` caps how many
  of each kind may be opened per window. It defaults to
  `DEFAULT_WINDOW_LIMITS`, i.e. every dimension at `MAX_CAP`, so merely
  enabling windows does not change behaviour until caps are configured.
- Every account has independent `WindowState { window_start, bets_opened,
  positions_opened, subscriptions_added }` counters. The window rolls
  **lazily** on the next successful mutation when
  `ledger.sequence() >= window_start + window_ledgers`; no keeper is needed.
  The addition saturates at `u32::MAX` so a sequence near the type ceiling
  cannot wrap into a spurious reset.
- Window caps are enforced by `place_bet`, `open_position`, and `subscribe`
  **in addition to** the concurrent caps, in that order. A rejection returns
  `BetsWindowCap` / `PositionsWindowCap` / `SubscriptionsWindowCap` and does
  not consume capacity or persist a roll.
- Decrements (`clear_bet`, `close_position`, `unsubscribe`) never reduce
  window counters: closing a bet does not refund window capacity. Only the
  window rolling over resets the counters.
- A brand-new account anchors its first window at the ledger of its first
  successful open.

```text
ledger:            0    5    7         14        21
window (W = 7):  [0,7)     [7,14)    [14,21)   [21,28)
opens at:           2     1         3         1
```

## Storage layout

- `DefaultLimits` and `AccountLimits(Address)` are stored in **instance**
  storage so they participate in the same TTL extension window as the rest of
  the contract's configuration.
- `AccountState(Address)` is stored in **persistent** storage and bumped to
  `STATE_BUMP_AMOUNT` (≈30 days) on every read / write so accounts that go
  quiet do not silently archive.
- `WindowLedgers` and `DefaultWindowLimits` are new **instance** storage keys
  using the existing `INSTANCE_BUMP_*` TTL policy.
- `WindowState(Address)` is a new **persistent** key using the existing
  `STATE_BUMP_*` TTL policy. It is written only while windows are enabled and
  the account successfully opens something.

### Migration note (Issue #1522)

The window feature is **purely additive** at the storage level: no existing
key was reused or reshaped, and `AccountState` / `AccountLimits` are byte-for-
byte unchanged. An upgraded contract therefore reads all pre-existing accounts
without migration. Windows are disabled by default (`window_ledgers == 0`), so
the behaviour and gas profile of an upgraded-but-unconfigured contract match
the previous version. Because this crate has no on-chain layout descriptor or
`STORAGE_MIGRATION_VERSION` constant, none needed bumping; this note is the
migration record. Operators that later enable windows should expect one extra
persistent read/write for accounts that open bets, positions, or subscriptions.

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
| 11   | `BetsWindowCap`       | Per-window bet cap reached.                            |
| 12   | `PositionsWindowCap`  | Per-window position cap reached.                       |
| 13   | `SubscriptionsWindowCap` | Per-window subscription cap reached.                |

Codes 11-13 are only returned while `window_ledgers > 0`. Invalid window
configuration (length outside the bounds or a window cap above `MAX_CAP`)
returns the existing `InvalidLimit` (code 4).

## Events

`contracts/yield/src/events.rs` centralizes every topic as a `Symbol::new`
call and pins each to its byte representation in `#[cfg(test)]` snapshot
tests. Topic vocabulary (`init`, `admin_*`, `default_limits_set`,
`account_limits_set`, `account_limits_cleared`, `bet_placed`, `bet_cleared`,
`position_opened`, `position_closed`, `subscription_added`,
`subscription_removed`, `upgraded`) follows the past-tense-verb pattern
documented in `CONTRIBUTING.md`.

Rolling windows add three topics, each with a byte-snapshot test:

| Topic                 | Emitted when                                         | Payload (data)                  |
|-----------------------|------------------------------------------------------|---------------------------------|
| `window_ledgers_set`  | Admin updates the window length.                     | new `window_ledgers`            |
| `window_limits_set`   | Admin updates per-window caps.                       | `WindowLimits`                  |
| `window_rolled`       | Lazy roll on the first successful mutation after expiry. | `(new_window_start, window_ledgers)` |

Existing event topics and payload shapes are unchanged by this feature.

## Backward compatibility

- `contracts/yield/src/lib.rs` continues to re-export
  `RevenuePool` / `RevenuePoolClient` unchanged.
- No prior type, storage key, or event topic was renamed.
- No prior `callora-yield` consumer is broken (only additive additions to the
  public surface).

## Verification

```bash
# Format, build, run unit tests & auth-snap integration suite.
cargo fmt  -p callora-yield
cargo test -p callora-yield --lib              # 40+ unit tests
cargo test -p callora-yield --test auth_snap   # 15 mutator + 10 view assertions

# Build for the wasm32 target so WASM-bound assets do compile.
cargo build -p callora-yield --target wasm32-unknown-unknown --release
```

All unit tests must pass, all 15 mutator assertions in `auth_snap.rs` must
report `res.is_err()`, and all 10 view assertions in `auth_snap.rs` must run
without `require_auth`.

## Coverage summary

- 60+ unit tests for caps, defaults, storage helpers, dry-run checks, admin
  rotation, and rolling windows (boundary/exact-ledger/skipped-window/
  per-account/decrement-no-refund/overflow cases).
- A proptest comparing randomised open/close/ledger-advance sequences against
  a simple model of the concurrent and window caps.
- 15 authentication-snapshot mutator tests (`require_auth` enforcement).
- 10 authentication-snapshot view tests (no-auth callability).
- 16 byte-snapshot tests pinning event-topic symbol identity.
