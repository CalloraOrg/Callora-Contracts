# Revenue Pool Batch Transfer (`batch_distribute`)

`batch_distribute` performs an atomic multi-leg USDC transfer. All validation — including duplicate-recipient detection — runs before any external call to the token contract, so either every transfer in the batch succeeds or none do.

---

## Four-Phase Execution Model

### Phase 0: Authorization
- Verifies the caller is the current admin via `require_auth()` and an explicit admin address check.

### Phase 1: Precomputation, Validation & Duplicate Detection
- Rejects empty batches and batches exceeding `MAX_BATCH_SIZE` with typed errors (`RevenuePoolError::BatchEmpty` / `RevenuePoolError::BatchTooLarge`).
- Iterates all payments once, tracking seen addresses in a `Map<Address, bool>` (O(n log n) in `no_std` Soroban).
- Rejects the first duplicate recipient encountered with `RevenuePoolError::DuplicateRecipient`.
- Validates each amount is strictly positive and within `max_distribute`, and that no recipient is the pool contract itself.
- Accumulates the batch total with `checked_add` (overflow-safe).
- **No external calls occur in this phase.**

### Phase 2: Balance Check
- Performs a single read of the USDC token contract balance.
- Fails with `RevenuePoolError::InsufficientBalance` if `balance < total`.
- **One external read, no writes.**

### Phase 3: Execution
- Transfers each leg and emits one `batch_distribute` event per leg.
- Soroban's transaction model guarantees full revert on any failure.

All validation (Phases 0–2) completes before any state-changing external call. If any check fails, no transfers occur and no `batch_distribute` events are emitted.

---

## Duplicate Recipient Policy

**Duplicates are rejected.** If the same `Address` appears more than once in `payments`, the call fails with `RevenuePoolError::DuplicateRecipient` and no tokens are moved.

To pay the same address for multiple items (e.g., two milestones), aggregate the amounts off-chain before submitting:

```rust
// Instead of:
payments.push_back((developer, 1_000)); // milestone 1
payments.push_back((developer, 1_500)); // milestone 2 <- rejected

// Do:
payments.push_back((developer, 2_500)); // aggregated
```

---

## Errors

All contract-owned validation failures use typed `#[contracterror] RevenuePoolError` variants:

| Error | Code | Trigger |
|---|---|---|
| `RevenuePoolError::BatchEmpty` | `1` | `payments` is empty |
| `RevenuePoolError::BatchTooLarge` | `2` | `payments.len() > MAX_BATCH_SIZE` |
| `RevenuePoolError::NotInitialized` | `3` | Required pool configuration is missing |
| `RevenuePoolError::Unauthorized` | `5` | Caller is not authorized for the operation |
| `RevenuePoolError::Paused` | `6` | Pool is paused |
| `RevenuePoolError::AmountNotPositive` | `12` | A payment amount is not positive |
| `RevenuePoolError::AmountExceedsMaxDistribute` | `13` | A payment exceeds the per-leg cap |
| `RevenuePoolError::InvalidRecipient` | `14` | A recipient is the pool contract |
| `RevenuePoolError::InsufficientBalance` | `15` | Pool balance is below the batch total |
| `RevenuePoolError::DuplicateRecipient` | `16` | A recipient appears more than once |
| `RevenuePoolError::Overflow` | `17` | The checked batch total overflows `i128` |

---

## Event Schema

One `batch_distribute` event is emitted per payment leg after all validation passes:

```text
topics: ("batch_distribute", recipient: Address)
data:   amount: i128
```

Because duplicates are rejected, each recipient address appears at most once across all events in a successful batch.

---

## Batch Size Policy & Chunking

- **Hard cap:** `MAX_BATCH_SIZE = 50` entries per call.
- **Minimum:** 1 entry.

`batch_distribute` returns `Result<(), RevenuePoolError>`. From the generated client, call `try_batch_distribute(...)` to handle `BatchEmpty` / `BatchTooLarge` without a host panic.

To pay more than `MAX_BATCH_SIZE` recipients, pre-chunk the payout list and submit one `batch_distribute` call per chunk.

### On-chain helper: `chunk_iter`

`chunk_iter(env, payments, chunk_size)` splits an ordered `Vec<(Address, i128)>` into consecutive chunks of at most `chunk_size` legs, preserving order. An empty input or `chunk_size == 0` yields no chunks. It is a pure, read-only helper with no storage access, auth, or token movement.

```rust
use callora_revenue_pool::{chunk_iter, MAX_BATCH_SIZE};

for chunk in chunk_iter(&env, payments, MAX_BATCH_SIZE).iter() {
    pool.batch_distribute(&admin, &chunk);
}
```

### Off-chain (TypeScript) chunking

```ts
const MAX_BATCH_SIZE = 50; // must match the contract constant

function chunkPayments<T>(payments: T[], size: number = MAX_BATCH_SIZE): T[][] {
  if (size <= 0) return [];
  const chunks: T[][] = [];
  for (let i = 0; i < payments.length; i += size) {
    chunks.push(payments.slice(i, i + size));
  }
  return chunks;
}

for (const chunk of chunkPayments(payments)) {
  await pool.batch_distribute({ caller: admin, payments: chunk });
}
```

> **Atomicity note:** Each chunk is its own transaction. A failure in one chunk reverts only that chunk, not previously submitted chunks.

---

## TTL Window and Archival Risk Mitigation

Soroban archives ledger entries after prolonged inactivity. The revenue pool extends instance storage TTL on state-modifying operations to prevent instance state from being archived:

| Constant | Value | Approximate Duration | Purpose |
|---|---|---|---|
| `BUMP_AMOUNT` | `10_000` | ~16 days | Ledger extension amount when TTL is extended |
| `LIFETIME_THRESHOLD` | `1_000` | ~1.5 days | Minimum TTL before extension is triggered |

State-modifying functions (`init`, `set_admin`, `accept_admin`, `distribute`, `batch_distribute`) extend instance storage TTL. Read-only view functions (`get_admin`, `get_usdc_token`, `balance`) intentionally do not extend TTL.
