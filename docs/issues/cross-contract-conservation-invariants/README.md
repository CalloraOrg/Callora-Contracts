# Cross-Contract Cumulative Conservation Invariants

**Campaign:** GrantFox  
**Scope:** `callora-vault`, `callora-settlement`, `callora-revenue-pool`  
**Type:** Smart-contract invariant hardening  

---

## Problem

Each contract has its own per-contract invariant tests, but no test verifies that
funds are conserved **across** the full vault → settlement → revenue-pool pipeline.
A bug in the routing logic (e.g. double-credit, missing transfer, misrouted amount)
could pass all three individual test suites while violating conservation end-to-end.

---

## Invariant to Enforce

```
vault.deducted_total
  == settlement.developer_balances_sum
   + settlement.global_pool.total_balance
   + revenue_pool.usdc_on_ledger
```

More precisely, for every sequence of operations:

1. **Vault conservation** — `vault.balance + Σ(deducted)` equals the initial deposit.
2. **Settlement conservation** — `Σ(receive_payment amounts)` equals
   `Σ(developer_balance[i]) + global_pool.total_balance`.
3. **Revenue-pool conservation** — `Σ(yield_deposit amounts)` equals
   `revenue_pool USDC on-ledger + Σ(distributed amounts)`.
4. **End-to-end** — `vault.deducted_total` equals the sum of all credits that
   eventually arrived in settlement or revenue pool.

---

## Implementation Plan

### 1. `callora-settlement` — add `get_total_received` view

Settlement currently tracks per-developer balances and a global pool balance but
has no running total of all funds received. Add a persistent `TotalReceived`
storage key incremented on every `receive_payment` / `batch_receive_payment` call.

```
StorageKey::TotalReceived  →  i128  (default 0)
```

- Increment in `receive_payment` using `checked_add` (panic on overflow).
- Expose as `pub fn get_total_received(env: Env) -> i128`.
- Invariant: `get_total_received() == Σ developer_balance[i] + global_pool.total_balance + Σ withdrawn`.

### 2. `callora-vault` — add `get_total_deducted` view

Vault tracks `meta.balance` (decremented on deduct) but not cumulative outflow.
Add a persistent `TotalDeducted` storage key incremented on every successful
`deduct` / `batch_deduct`.

```
StorageKey::TotalDeducted  →  i128  (default 0)
```

- Increment after `meta.balance` update using `checked_add`.
- Expose as `pub fn get_total_deducted(env: Env) -> i128`.
- Invariant: `initial_balance - meta.balance == get_total_deducted()` (ignoring withdrawals).

### 3. Cross-contract test — `test_cross_invariant.rs`

A new integration-style test module wired up in the vault crate (which already
imports `callora-settlement` as a dev-dependency) that:

- Deploys vault + settlement + revenue pool together.
- Runs a random sequence (64 seeds × 48 steps) of: deposit, deduct (→ settlement),
  batch_deduct, developer withdraw, pool distribute.
- After every step checks:
  ```
  vault.get_total_deducted()
    == settlement.get_total_received()
  ```
  and:
  ```
  settlement.get_total_received()
    == Σ settlement.get_developer_balance(dev_i)
     + settlement.get_global_pool().total_balance
     + total_withdrawn_from_settlement
  ```

---

## Files to Change

| File | Change |
|------|--------|
| `contracts/settlement/src/lib.rs` | Add `StorageKey::TotalReceived`, increment in `receive_payment`, expose `get_total_received` |
| `contracts/vault/src/lib.rs` | Add `StorageKey::TotalDeducted`, increment in `deduct`/`batch_deduct`, expose `get_total_deducted` |
| `contracts/vault/src/test_cross_invariant.rs` | New: cross-contract conservation test |
| `contracts/vault/src/lib.rs` | Add `#[cfg(test)] mod test_cross_invariant` |
| `docs/interfaces/vault.json` | Document `get_total_deducted` |
| `docs/interfaces/settlement.json` | Document `get_total_received` |

---

## Acceptance Criteria

- [ ] `get_total_deducted` view exists on vault; returns cumulative deducted amount.
- [ ] `get_total_received` view exists on settlement; returns cumulative received amount.
- [ ] `test_cross_invariant.rs` passes with ≥ 64 seeds, ≥ 48 steps each.
- [ ] All three per-contract invariant tests continue to pass.
- [ ] `cargo clippy -- -D warnings` clean.
- [ ] `cargo test --workspace` green.
- [ ] Interface JSON docs updated.

---

## Branch

```bash
git checkout -b feature/cross-contract-conservation-invariants
```

Commit message:

```
feat: cross-contract cumulative conservation invariants

- vault: add TotalDeducted storage key + get_total_deducted() view
- settlement: add TotalReceived storage key + get_total_received() view
- test_cross_invariant: 64-seed × 48-step end-to-end conservation check
- docs: update vault.json and settlement.json interface summaries

Closes #<issue>
```

---

## Security Notes

- Both new counters use `checked_add` — overflow panics loudly rather than wrapping silently.
- Both are view-only getter additions; no existing auth surface is changed.
- `TotalDeducted` / `TotalReceived` are informational — they do not gate any transfer logic.
