# Escrow Contract — Admin Cool-off (Cooldown) for Critical Actions

Implements [issue #914](https://github.com/CalloraOrg/Callora-Contracts/issues/914):
a cool-off window between admin actions on the `escrow` contract to prevent rapid
abuse of an admin key holding escrowed funds.

## Overview

The `escrow` contract is the fund-holding surface for the Callora marketplace where
privileged admin keys perform high-impact operations on escrowed funds. Because those
keys control fund releases and other critical operations, they are high-value targets.
To bound the blast radius of a leaked or misused key, every **critical action** is
rate-limited by a per-action **cool-off window**: after an action runs, another
invocation of the *same* action is rejected until the window elapses.

The window is measured against the ledger **timestamp** (wall-clock seconds), so it
is independent of block cadence.

## Design

- **Per-action tracking.** Each critical action is identified by a short `Symbol`
  tag (`"release"`, `"pause"`, `"unpause"`, `"rotate"`). The last-execution timestamp
  is stored per tag, so cooling one action never blocks an unrelated one.
- **Configurable window.** A single global cooldown (seconds) is held in instance
  storage and bounded by `MIN_COOLDOWN_SECS`..=`MAX_COOLDOWN_SECS` (1 second .. 30
  days). The default at `init` is `DEFAULT_COOLDOWN_SECS` (1 hour).
- **Overflow-safe.** All arithmetic uses checked/saturating operations. There are no
  `unwrap()` calls on production paths.
- **Auth at the edge.** `require_auth` and the admin check run in the contract
  entrypoints; `admin.rs` is pure cool-off bookkeeping and is unit-tested in isolation.

## Entrypoints (API)

| Function | Auth | Cool-off tag | Description |
|----------|------|--------------|-------------|
| `init(admin, signer, cooldown_secs: Option<u64>)` | `admin` | — | One-time setup. `None` adopts the 1-hour default. |
| `set_cooldown(caller, secs)` | admin | — | Update the global window (validated). |
| `get_cooldown() -> u64` | — (view) | — | Current window in seconds. |
| `cooldown_remaining(action) -> u64` | — (view) | — | Seconds until `action` is available (0 = now). |
| `is_ready(action) -> bool` | — (view) | — | Whether `action` may run now. |
| `release(caller, recipient)` | admin | `"release"` | Release escrowed funds to the recipient. |
| `pause(caller)` | admin | `"pause"` | Pause the escrow contract. |
| `unpause(caller)` | admin | `"unpause"` | Unpause the escrow contract. |
| `rotate_signer(caller, new_signer)` | admin | `"rotate"` | Replace the escrow signer. |
| `set_admin(caller, new_admin)` | admin | — | Nominate a new admin (two-step). |
| `accept_admin(caller)` | pending admin | — | Complete the two-step admin transfer. |
| `get_admin() -> Address` | — (view) | — | Current admin. |
| `get_pending_admin() -> Option<Address>` | — (view) | — | Pending nominee, if any. |
| `get_signer() -> Address` | — (view) | — | Current escrow signer. |
| `is_paused() -> bool` | — (view) | — | Current pause state. |

Every state-changing entrypoint calls `require_auth` on its `caller`.

## Cool-off semantics

For a given action tag:

```
ready_at   = last_run_ts + cooldown           (saturating; 0 if never run)
remaining  = max(0, ready_at - now)
is_ready   = remaining == 0
```

`guard()` enforces `is_ready` and, on success, stamps `last_run_ts = now`, arming a
fresh window. A blocked call returns `EscrowError::CooldownActive` and makes **no**
state change.

Reducing the window via `set_cooldown` takes effect immediately for the *next*
readiness check (it is recomputed from the stored `last_run_ts`), allowing an admin to
safely shorten the cool-off in a genuine emergency.

## Error codes

| Code | Variant | Meaning |
|------|---------|---------|
| 1 | `NotInitialized` | Contract not initialized |
| 2 | `AlreadyInitialized` | `init` called twice |
| 3 | `Unauthorized` | Caller is not the required admin/pending admin |
| 4 | `CooldownActive` | Action is still inside its cool-off window |
| 5 | `InvalidCooldown` | Proposed window outside `[1s, 30d]` |
| 6 | `NoPendingAdmin` | No admin transfer pending |
| 7 | `Overflow` | Arithmetic overflow detected |

## Events

| Topic | Data | When |
|-------|------|------|
| `init` | `cooldown: u64` | On initialization (topic includes `admin`). |
| `cooldown_set` | `secs: u64` | On `set_cooldown` (topic includes `caller`). |
| `action` | `tag: Symbol` | On each successful guarded action (topic includes `caller`). |
| `admin_nominated` | `new_admin: Address` | On `set_admin`. |
| `admin_accepted` | `new_admin: Address` | On `accept_admin`. |

## Security notes

- The cool-off caps the *rate* of critical actions; it is a defense-in-depth layer
  complementing the two-step admin rotation, not a replacement for key hygiene.
- The `release` action is independently cooled from `pause` and `rotate`, ensuring
  that blocking an emergency pause never also blocks a fund release (and vice versa).
- Distinct action tags are independently cooled so that an emergency `pause` is never
  blocked by a recent `rotate`, and vice versa.
- All windows are bounded, so a configuration mistake cannot brick critical actions for
  longer than `MAX_COOLDOWN_SECS` (30 days).
- The two-step admin rotation (nominate → accept) ensures no admin key rotation happens
  accidentally or unilaterally.

## Relation to `hot` contract

The escrow cooldown follows the same design as the `hot` contract's cooldown
([`docs/HOT_ADMIN_COOLDOWN.md`](HOT_ADMIN_COOLDOWN.md)) with one addition: the
**`release`** critical action, specific to the escrow use-case, is also guarded by the
cooldown. This prevents rapid sequential fund releases from a compromised admin key.
