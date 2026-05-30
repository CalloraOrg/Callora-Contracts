# Atomic Multi-Leg USDC Transfer Implementation

**Date:** 2026-04-24  
**Updated:** 2026-05-27 — duplicate recipient detection added  
**Feature:** Atomic batch transfer with all-or-nothing execution and duplicate-recipient rejection

---

## Summary

`batch_distribute` performs an atomic multi-leg USDC transfer. All validation — including
duplicate-recipient detection — runs before any external call to the token contract, so either
every transfer in the batch succeeds or none do.

---

## Duplicate Recipient Policy

**Duplicates are rejected.** If the same `Address` appears more than once in the `payments`
vector, the call panics with `"duplicate recipient in batch"` and no tokens are moved.

### Rationale

A duplicate entry in a settlement payload is almost always an off-chain bug (e.g., a developer
listed twice in a CSV). Silently double-paying would:

- Drain the pool by an unintended amount.
- Be irreversible on-chain.
- Mask the upstream data error rather than surfacing it.

Rejecting the batch forces the caller to fix the payload and resubmit, which is the safe default
for a financial contract.

### If you need to pay the same address for two milestones

Aggregate the amounts off-chain before submitting:

```rust
// Instead of:
payments.push_back((developer, 1_000)); // milestone 1
payments.push_back((developer, 1_500)); // milestone 2  ← rejected

// Do:
payments.push_back((developer, 2_500)); // aggregated
```

---

## Four-Phase Execution Model

### Phase 0: Authorization
- Verifies caller is the current admin via `require_auth()` + explicit address check.

### Phase 1: Precomputation, Validation & Duplicate Detection
- Rejects empty batches and batches exceeding `MAX_BATCH_SIZE`.
- Iterates all payments once, building a `Map<Address, bool>` seen-set.
- Panics on the first duplicate address encountered.
- Validates each amount is strictly positive and within `max_distribute`.
- Accumulates total with `checked_add` (overflow-safe).
- **No external calls in this phase.**

### Phase 2: Balance Check
- Single read of the USDC token contract balance.
- Panics if `balance < total`.
- **One external read, no writes.**

### Phase 3: Execution
- Transfers and emits one `batch_distribute` event per leg.
- Soroban's transaction model guarantees full revert on any failure.

---

## Atomicity Guarantee

All validation (phases 0–2) completes before any state-changing external call. If any check
fails — including duplicate detection — no transfers occur and no `batch_distribute` events
are emitted.

---

## Duplicate Detection Implementation

```rust
let mut seen: Map<Address, bool> = Map::new(&env);

for payment in payments.iter() {
    let (to, amount) = payment;

    if seen.contains_key(to.clone()) {
        panic!("{}", ERR_DUPLICATE_RECIPIENT); // "duplicate recipient in batch"
    }
    seen.set(to.clone(), true);

    // ... amount validation ...
}
```

`Map<Address, bool>` is the only ordered, address-keyed collection available in `no_std`
Soroban. Each `contains_key` / `set` is O(log n), giving O(n log n) total for the validation
loop — well within budget for `MAX_BATCH_SIZE = 50`.

---

## Error Constants

| Constant | Value |
|---|---|
| `ERR_DUPLICATE_RECIPIENT` | `"duplicate recipient in batch"` |
| `ERR_AMOUNT_NOT_POSITIVE` | `"amount must be positive"` |
| `ERR_AMOUNT_EXCEEDS_MAX_DISTRIBUTE` | `"amount exceeds max_distribute"` |
| `ERR_INSUFFICIENT_BALANCE` | `"insufficient USDC balance"` |
| `ERR_UNAUTHORIZED` | `"unauthorized: caller is not admin"` |

---

## Event Schema

One `batch_distribute` event per payment leg, emitted only after all validation passes:

```
topics: ("batch_distribute", recipient: Address)
data:   amount: i128
```

The `amount` in each event reflects the exact amount transferred to that recipient. Because
duplicates are rejected, each recipient address appears at most once across all events in a
successful batch.

---

## Batch Size Policy

- **Hard cap:** `MAX_BATCH_SIZE = 50` entries per call.
- **Minimum:** 1 entry (empty batch panics).
- For larger distributions, split into multiple transactions off-chain.

---

## Test Coverage

Six new tests cover the duplicate-recipient feature (added to `test.rs`):

| Test | What it verifies |
|---|---|
| `batch_distribute_duplicate_recipient_panics` | Basic duplicate → panic |
| `batch_distribute_duplicate_does_not_transfer_any_funds` | Atomicity: balances unchanged on rejection |
| `batch_distribute_duplicate_does_not_emit_events` | No events emitted on rejection |
| `batch_distribute_duplicate_at_end_panics` | Duplicate at last position is still caught |
| `batch_distribute_unique_recipients_succeeds` | Valid batch still works after the change |
| `batch_distribute_duplicate_detected_before_balance_check` | Dedup fires in Phase 1, before Phase 2 balance check |

Total test suite: **54 passing** (1 pre-existing failure in `upgrade_sets_version_and_emits_event`
due to a Soroban unit-test environment limitation — WASM upload is not supported in `Env::default()`).

---

## Security Considerations

### Duplicate Recipient Attack
**Threat:** Malformed off-chain payload lists the same developer twice, causing double-payment.  
**Mitigation:** Phase 1 rejects the batch before any transfer. The pool balance is never touched.

### Authorization
**Threat:** Unauthorized caller distributes funds.  
**Mitigation:** `require_auth()` + explicit admin address check in Phase 0.

### Overflow
**Threat:** Crafted amounts overflow `i128` total, bypassing balance check.  
**Mitigation:** `checked_add` panics on overflow before reaching Phase 2.

### Reentrancy
**Threat:** Token contract re-enters `batch_distribute` mid-execution.  
**Mitigation:** Soroban's execution model prevents reentrancy at the host level.

---

## Checklist

- [x] Four-phase execution model implemented
- [x] Duplicate recipient detection in Phase 1 (before any external call)
- [x] `Map<Address, bool>` seen-set — O(n log n), no `unwrap()` in prod paths
- [x] Error constant `ERR_DUPLICATE_RECIPIENT` defined
- [x] All validation before external calls (atomicity preserved)
- [x] `MAX_BATCH_SIZE` cap preserved
- [x] Events reflect final per-recipient amount (one event per unique recipient)
- [x] 6 new tests covering duplicate cases
- [x] Pre-existing tests unaffected (54 pass)
- [x] `/// doc` comments updated on `batch_distribute`
- [x] Policy documented in this file
- [x] No `unwrap()` in production paths
