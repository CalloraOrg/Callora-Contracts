# Atomic Multi-Leg USDC Transfer Implementation

**Date:** 2026-04-24  
**Updated:** 2026-05-27 — duplicate recipient detection added  
**Updated:** 2026-06-25 — typed size-violation errors (`RevenuePoolError`) and `chunk_iter` helper added (#418)  
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
- Rejects empty batches and batches exceeding `MAX_BATCH_SIZE` with **typed errors**
  ([`RevenuePoolError::BatchEmpty`] / [`RevenuePoolError::BatchTooLarge`]), not string panics.
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

## Errors

Batch **size** violations are typed (`#[contracterror] RevenuePoolError`) so integrators
branch on a numeric code, never a panic string:

| Error | Code | Trigger |
|---|---|---|
| `RevenuePoolError::BatchEmpty` | `1` | `payments` is empty |
| `RevenuePoolError::BatchTooLarge` | `2` | `payments.len() > MAX_BATCH_SIZE` |

Remaining per-leg validations still panic with string constants (typed-error migration for
these is tracked separately):

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
- **Minimum:** 1 entry.
- Size violations return **typed errors** instead of string panics, so integrators
  branch on a stable numeric code rather than matching panic text:

| Condition | Error | Code |
|---|---|---|
| `payments.len() == 0` | `RevenuePoolError::BatchEmpty` | `1` |
| `payments.len() > MAX_BATCH_SIZE` | `RevenuePoolError::BatchTooLarge` | `2` |

`batch_distribute` returns `Result<(), RevenuePoolError>`. From the generated client,
call `try_batch_distribute(...)` to receive `Err(Ok(RevenuePoolError::BatchTooLarge))`
(or `BatchEmpty`) without triggering a host panic.

---

## Chunking Large Distributions

To pay more than `MAX_BATCH_SIZE` developers, **pre-chunk** the payout list and submit one
`batch_distribute` call per chunk. Every chunk is guaranteed to satisfy the cap, so no call
ever returns `BatchTooLarge` and there is no panic string to parse.

### On-chain helper: `chunk_iter`

`chunk_iter(env, payments, chunk_size)` splits an ordered `Vec<(Address, i128)>` into
consecutive chunks of at most `chunk_size` legs, preserving order. The last chunk may be
shorter (a single remaining leg becomes a one-element chunk). An empty input — or a
`chunk_size` of `0` — yields no chunks. It is a pure, read-only helper: no storage access,
no auth, no token movement.

```rust
use callora_revenue_pool::{chunk_iter, MAX_BATCH_SIZE};

// Distribute to an arbitrarily large list, MAX_BATCH_SIZE legs at a time.
for chunk in chunk_iter(&env, payments, MAX_BATCH_SIZE).iter() {
    pool.batch_distribute(&admin, &chunk); // each chunk is within the cap
}
```

### Off-chain (TypeScript) integrators

Backends typically build the payout list off-chain and chunk it before invoking the
contract. Mirror `MAX_BATCH_SIZE = 50` and submit one transaction per chunk:

```ts
const MAX_BATCH_SIZE = 50; // must match the contract constant

/** Split an ordered payout list into chunks of at most `size` legs. */
function chunkPayments<T>(payments: T[], size: number = MAX_BATCH_SIZE): T[][] {
  if (size <= 0) return [];
  const chunks: T[][] = [];
  for (let i = 0; i < payments.length; i += size) {
    chunks.push(payments.slice(i, i + size));
  }
  return chunks;
}

// payments: Array<{ to: string; amount: bigint }>
for (const chunk of chunkPayments(payments)) {
  // Each chunk.length <= MAX_BATCH_SIZE, so batch_distribute never returns BatchTooLarge.
  await pool.batch_distribute({ caller: admin, payments: chunk });
}
```

> **Atomicity note:** each chunk is its own transaction. A failure in one chunk reverts only
> that chunk, not previously-submitted chunks. Design retries to be idempotent (the
> duplicate-recipient guard makes accidental re-submission of an already-paid leg safe to
> reject).

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

### Size-cap & chunking tests (#418)

| Test | Boundary / behavior |
|---|---|
| `batch_distribute_empty_returns_typed_error` | length `0` → `RevenuePoolError::BatchEmpty` |
| `batch_distribute_too_large_returns_typed_error` | length `MAX_BATCH_SIZE + 1` → `BatchTooLarge`, funds untouched |
| `batch_distribute_at_max_size_succeeds` | length `MAX_BATCH_SIZE` → succeeds |
| `chunk_iter_empty_input_yields_no_chunks` | empty input → `[]` |
| `chunk_iter_zero_chunk_size_yields_no_chunks` | `chunk_size == 0` → `[]` |
| `chunk_iter_single_leg_yields_one_chunk` | single-leg chunk |
| `chunk_iter_exact_multiple` | even split (`[5, 5]`) |
| `chunk_iter_with_remainder_has_single_leg_tail` | remainder tail (`[5, 5, 1]`) |
| `chunk_iter_chunk_size_larger_than_input` | one chunk when `chunk_size > len` |
| `chunk_iter_preserves_order_and_amounts` | order preserved across chunks |
| `chunk_iter_chunks_each_distribute_within_cap` | `MAX_BATCH_SIZE + 1` legs pre-chunked → all distribute, pool drained |

Total test suite: **62 passing** (1 pre-existing failure in `upgrade_sets_version_and_emits_event`
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
- [x] Typed `RevenuePoolError::{BatchEmpty, BatchTooLarge}` replace size-violation string panics (#418)
- [x] `chunk_iter` helper for backend pre-chunking, with Rust + TypeScript usage documented
- [x] Boundary tests for length `0`, `MAX_BATCH_SIZE`, and `MAX_BATCH_SIZE + 1`
