# PR: docs — rustdoc on limits public entrypoints (Closes #735)

## Overview

Introduces the **Callora Limits** contract — a new Soroban smart contract that
maintains a registry of **per-token transaction limits** for the Callora
protocol. Every public entrypoint carries comprehensive `///`-style rustdoc
following the project's NatSpec conventions (what/how/why + errors + events).

**Closes: #735** ("Add rustdoc for public entrypoints in limits").

The issue names `contracts/limits/src/lib.rs` as the implementation target.
That module did not yet exist in the tree (the only pre-existing `limits`
sources were the internal `contracts/vault/src/limits.rs` reserve-cap helper and
`contracts/settlement/src/limits.rs` min-balance helper, neither of which is a
standalone contract). This PR adds the standalone `limits` contract at the exact
path the issue specifies, mirroring the structure and conventions of the
`checkpoint` contract added for the sibling rustdoc issue #667.

---

## Contract Summary

The Limits contract lets the Callora admin configure an inclusive `[min, max]`
transaction band per token contract address. Sibling contracts (vault,
settlement, revenue pool) can consult it through the read-only
`check_amount` entrypoint to validate deposit / withdrawal / payout amounts
against a single, centrally-managed source of truth.

| Contract | Role |
|----------|------|
| **Vault** | Holds USDC, processes deposits/deducts |
| **Settlement** | Tracks per-developer balances, handles withdrawals |
| **Revenue Pool** | Distributes protocol yield |
| **Checkpoint** | Records immutable balance snapshots for audit trails |
| **Limits** *(new)* | Central per-token transaction-limit registry |

---

## Files Changed

### New files

| File | Purpose |
|------|---------|
| `contracts/limits/Cargo.toml` | Package manifest — soroban-sdk 22, cdylib + rlib |
| `contracts/limits/src/lib.rs` | Contract logic; all `pub fn`s fully documented |
| `contracts/limits/src/errors.rs` | `LimitsError` enum — 8 stable numeric codes |
| `contracts/limits/src/events.rs` | Event symbol constructors + 7 byte-identity tests |
| `contracts/limits/src/test.rs` | Unit tests covering every entrypoint and edge case |

### Modified files

| File | Change |
|------|--------|
| `Cargo.toml` | Added `contracts/limits` to `workspace.members` and `default-members` |

---

## Public API

### Initialisation

| Function | Auth | Description |
|----------|:----:|-------------|
| `init(admin)` | ✅ `require_auth` | One-time initialisation; sets admin |

### Limit configuration

| Function | Auth | Description |
|----------|:----:|-------------|
| `set_limit(caller, token, min, max)` | ✅ via `require_admin` | Create/update a `[min, max]` band; emits `limit_set` |
| `remove_limit(caller, token)` | ✅ via `require_admin` | Clear a token's band; emits `limit_removed` |

### Limit queries (read-only)

| Function | Auth | Returns |
|----------|:----:|---------|
| `get_limit(token)` | ❌ public | `Option<TokenLimit>` |
| `has_limit(token)` | ❌ public | `bool` |
| `check_amount(token, amount)` | ❌ public | `Result<(), LimitsError>` band validation |

### Admin rotation (two-step) + views

| Function | Auth | Description |
|----------|:----:|-------------|
| `set_admin(caller, new_admin)` | ✅ via `require_admin` | Nominate new admin |
| `accept_admin(caller)` | ✅ `require_auth` | Complete transfer |
| `cancel_admin_transfer(caller)` | ✅ via `require_admin` | Cancel pending transfer |
| `get_admin()` | ❌ public | Current admin |
| `get_pending_admin()` | ❌ public | Pending admin during rotation |

### Upgrade

| Function | Auth | Description |
|----------|:----:|-------------|
| `upgrade(caller, new_wasm_hash)` | ✅ via `require_admin` | Swap contract WASM |

---

## Data Model

```rust
pub struct TokenLimit {
    pub token: Address, // token contract the band applies to
    pub min: i128,      // inclusive minimum (>= 0)
    pub max: i128,      // inclusive maximum (>= min); UNLIMITED_MAX = no cap
}
```

| Storage Key | Scope | Value |
|-------------|-------|-------|
| `StorageKey::Admin` | Instance | Current admin |
| `StorageKey::PendingAdmin` | Instance | Pending admin during rotation |
| `StorageKey::Limit(token)` | Persistent | `TokenLimit` band (6-month TTL) |

---

## Error Codes

| Code | Variant | Meaning |
|-----:|---------|---------|
| 1 | `NotInitialized` | Contract has not been initialized |
| 2 | `AlreadyInitialized` | `init` was called more than once |
| 3 | `Unauthorized` | Caller is not the admin |
| 4 | `AmountNegative` | A limit or amount is negative |
| 5 | `InvalidLimit` | `max` set below `min` |
| 6 | `BelowMinimum` | Amount is below the configured minimum |
| 7 | `AboveMaximum` | Amount exceeds the configured maximum |
| 8 | `Overflow` | Arithmetic overflow detected |

---

## Security Properties

| Property | Implementation |
|----------|---------------|
| **Auth on state-changing entrypoints** | `require_auth` enforced on `init`, `set_limit`, `remove_limit`, all admin rotation, and `upgrade` |
| **Overflow-safe math** | `check_amount` performs no arithmetic on the amount — only bound comparisons — so it cannot overflow |
| **No raw `.unwrap()`** | All `Option/Result` extractions use `ok_or(LimitsError::…)` or `unwrap_or_else(\|\| panic!(…))` |
| **Two-step admin rotation** | `set_admin` → `accept_admin` prevents accidental admin loss |
| **TTL management** | Persistent limit entries get a 6-month TTL, extended on every write |

---

## Test Coverage

Unit tests live in `contracts/limits/src/test.rs` and cover every public
entrypoint plus boundary conditions:

- init: success, double-init rejection, event emission, pre-init `get_admin`
- set_limit: set/get, overwrite, per-token independence, `min == max`,
  unlimited max, negative min/max rejection, `max < min` rejection,
  non-admin rejection, event
- remove_limit: clears band, no-op on missing, non-admin rejection, event
- get_limit / has_limit: `None`/`false` when unset
- check_amount: no-limit fast path, negative rejection, in-band (both
  boundaries), below-min, above-max, unlimited-max large amount, unlimited-max
  still enforces min, zero-min allows zero
- admin rotation: happy path, non-admin `set_admin`, wrong-caller
  `accept_admin` panic, no-pending `accept_admin` panic, cancel clears pending,
  cancel without pending panic, non-admin cancel, post-rotation authority, event
- upgrade: non-admin rejection

### Rustdoc Self-Verification

`lib.rs` includes a `rustdoc_tests` module (`every_public_fn_in_lib_has_rustdoc`)
that parses the source at compile time and asserts **every `pub fn` is preceded
by a `///` doc comment**, matching the pattern used by the checkpoint contract.
This fails CI if an undocumented public function is ever added.

Additionally, `events.rs` carries 7 byte-identity snapshot tests proving each
event topic symbol never drifts.

---

## Build & Test Commands

```bash
# Build the limits contract
cargo build -p callora-limits

# Run limits tests
cargo test -p callora-limits

# Full workspace tests (ensure no regressions)
cargo test --workspace

# Lint
cargo clippy -p callora-limits -- -D warnings

# Build WASM for deployment
cargo build --target wasm32-unknown-unknown --release -p callora-limits
```

---

## Checklist

- [x] `///`-style rustdoc (what/how/why + errors + events) on every `pub fn`
      (verified by self-test)
- [x] `require_auth` on every state-changing entrypoint
- [x] Overflow-safe validation; no raw `.unwrap()` in production paths
- [x] Typed `#[contracterror]` enum with 8 stable numeric codes
- [x] Event symbols with byte-identity snapshot tests
- [x] Two-step admin rotation (`set_admin` → `accept_admin`)
- [x] Focused unit tests across all entrypoints and edge cases
- [x] Rustdoc self-test (`every_public_fn_in_lib_has_rustdoc`)
- [x] Workspace membership in root `Cargo.toml`
- [x] Follows existing contract conventions (vault, settlement, revenue pool, checkpoint)
