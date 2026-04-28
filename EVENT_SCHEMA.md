# Event Schema

Events emitted by all Callora contracts for indexers, frontends, and auditors.
All topic/data types refer to Soroban/Stellar XDR values.

## Change Note (2026-04)

The `workspace-members-dedup` hardening patch does not introduce event additions, removals, or payload shape changes.

## Contract: Callora Vault

### `init`

Emitted once when the vault is initialized.

| Index   | Location | Type    | Description         |
|---------|----------|---------|---------------------|
| topic 0 | topics   | Symbol  | `"init"`            |
| topic 1 | topics   | Address | vault owner         |
| data    | data     | i128    | initial balance     |

```json
{
  "topics": ["init", "GOWNER..."],
  "data": 1000000
}
```

---

### `deposit`

Emitted when a depositor increases the vault balance.

| Index   | Location | Type         | Description                   |
|---------|----------|--------------|-------------------------------|
| topic 0 | topics   | Symbol       | `"deposit"`                   |
| topic 1 | topics   | Address      | caller (depositor)            |
| data    | data     | (i128, i128) | (amount, new_balance)         |

```json
{
  "topics": ["deposit", "GDEPOSITOR..."],
  "data": [500000, 1500000]
}
```

---

### `deduct`

Emitted on each deduction â€” once per `deduct()` call and once per item in `batch_deduct()`.

| Index   | Location | Type         | Description                                    |
|---------|----------|--------------|------------------------------------------------|
| topic 0 | topics   | Symbol       | `"deduct"`                                     |
| topic 1 | topics   | Address      | caller                                         |
| topic 2 | topics   | Symbol       | `request_id` (empty Symbol if not provided)    |
| data    | data     | (i128, i128) | (amount, new_balance)                          |

```json
{
  "topics": ["deduct", "GCALLER...", "req_abc123"],
  "data": [100000, 900000]
}
```

**`request_id` encoding (indexer contract):**

- **Topic is always present**: the vault always emits **exactly 3 topics** for `deduct`.
- **No optional topic**: Soroban events do not carry an `Option` topic value; instead the vault uses a **sentinel**.
- **Sentinel for â€œno request_idâ€**: when the input `request_id` is `None`, topic 2 is `Symbol("")` (an empty symbol).
- **Indexer rule**: treat `Symbol("")` as â€œno request_id providedâ€.
- **Ambiguity note**: `Some(Symbol(""))` is indistinguishable from `None` on-chain. Clients **SHOULD NOT** intentionally pass an empty symbol as a real request id.

**Precondition (Issue #263):** `deduct` / `batch_deduct` require a settlement
address to be configured via `set_settlement`. If the settlement address is
not set, the call panics with `"settlement address not set"` **before** any
`deduct` event is emitted â€” indexers will therefore never observe a `deduct`
event for a call that lacked a configured settlement destination.

**Idempotency guard (Issue #249):** when `request_id` is `Some(Symbol)`, the
value is single-use across successful `deduct` and `batch_deduct` calls.
Reusing a previously accepted value, or repeating the same value twice inside
one batch, panics with `"duplicate request_id"` before any balance update,
transfer, or `deduct` event is emitted.

---

### `withdraw`

Emitted when the vault owner withdraws to their own address.

| Field         | Location | Type   | Description                                          |
|---------------|----------|--------|------------------------------------------------------|
| topic 0       | topics   | Symbol | `"withdraw"`                                         |
| topic 1       | topics   | Address| vault owner                                          |
| `amount`      | data     | i128   | amount withdrawn in USDC micro-units                 |
| `new_balance` | data     | i128   | vault balance after withdrawal                       |
| Index   | Location | Type         | Description           |
|---------|----------|--------------|-----------------------|
| topic 0 | topics   | Symbol       | `"withdraw"`          |
| topic 1 | topics   | Address      | vault owner           |
| data    | data     | (i128, i128) | (amount, new_balance) |

```json
{
  "topics": ["withdraw", "GOWNER..."],
  "data": [200000, 700000]
}
```

---

### `withdraw_to`

Emitted when the vault owner withdraws to a designated recipient.

| Field         | Location | Type   | Description                                          |
|---------------|----------|--------|------------------------------------------------------|
| topic 0       | topics   | Symbol | `"withdraw_to"`                                      |
| topic 1       | topics   | Address| vault owner                                          |
| topic 2       | topics   | Address| recipient `to`                                       |
| `amount`      | data     | i128   | amount withdrawn in USDC micro-units                 |
| `new_balance` | data     | i128   | vault balance after withdrawal                       |
| Index   | Location | Type         | Description           |
|---------|----------|--------------|-----------------------|
| topic 0 | topics   | Symbol       | `"withdraw_to"`       |
| topic 1 | topics   | Address      | vault owner           |
| topic 2 | topics   | Address      | recipient             |
| data    | data     | (i128, i128) | (amount, new_balance) |

```json
{
  "topics": ["withdraw_to", "GOWNER...", "GRECIPIENT..."],
  "data": [150000, 550000]
}
```

---

### `vault_paused`

Emitted when the vault is paused by the admin or owner.

| Index   | Location | Type    | Description          |
|---------|----------|---------|----------------------|
| topic 0 | topics   | Symbol  | `"vault_paused"`     |
| topic 1 | topics   | Address | caller (admin/owner) |
| data    | data     | ()      | empty                |

```json
{
  "topics": ["vault_paused", "GADMIN..."],
  "data": null
}
```

---

### `vault_unpaused`

Emitted when the vault is unpaused by the admin or owner.

| Index   | Location | Type    | Description          |
|---------|----------|---------|----------------------|
| topic 0 | topics   | Symbol  | `"vault_unpaused"`   |
| topic 1 | topics   | Address | caller (admin/owner) |
| data    | data     | ()      | empty                |

```json
{
  "topics": ["vault_unpaused", "GADMIN..."],
  "data": null
}
```

---

### `ownership_nominated`

Emitted when the owner starts a two-step ownership transfer.

| Index   | Location | Type    | Description   |
|---------|----------|---------|---------------|
| topic 0 | topics   | Symbol  | `"ownership_nominated"` |
| topic 1 | topics   | Address | current owner |
| topic 2 | topics   | Address | nominee       |
| data    | data     | ()      | empty         |

```json
{
  "topics": ["ownership_nominated", "GOWNER...", "GNOMINEE..."],
  "data": null
}
```

---

### `ownership_accepted`

Emitted when the nominee accepts ownership.

| Index   | Location | Type    | Description   |
|---------|----------|---------|---------------|
| topic 0 | topics   | Symbol  | `"ownership_accepted"` |
| topic 1 | topics   | Address | old owner     |
| topic 2 | topics   | Address | new owner     |
| data    | data     | ()      | empty         |

```json
{
  "topics": ["ownership_accepted", "GOWNER...", "GNEWOWNER..."],
  "data": null
}
```

---

### `admin_nominated`

Emitted when the admin starts a two-step admin transfer.

| Index   | Location | Type    | Description   |
|---------|----------|---------|---------------|
| topic 0 | topics   | Symbol  | `"admin_nominated"` |
| topic 1 | topics   | Address | current admin |
| topic 2 | topics   | Address | nominee       |
| data    | data     | ()      | empty         |

```json
{
  "topics": ["admin_nominated", "GADMIN...", "GNOMINEE..."],
  "data": null
}
```

---

### `admin_accepted`

- **OwnershipTransfer**: not present in current vault; would list old_owner, new_owner.

---

### `vault_paused`

Emitted when the vault circuit-breaker is activated by admin or owner.

| Field   | Location | Type    | Description                                      |
|---------|----------|---------|--------------------------------------------------|
| topic 0 | topics   | Symbol  | `"vault_paused"`                                 |
| topic 1 | topics   | Address | `caller` â€” admin or owner who triggered pause   |
| data    | data     | ()      | empty                                            |

**Indexer Note:** After this event is emitted, `is_paused()` view function returns `true`.
The following operations are blocked until unpause: `deposit()`, `deduct()`, `batch_deduct()`.

---

### `vault_unpaused`

Emitted when the vault circuit-breaker is deactivated by admin or owner.

| Field   | Location | Type    | Description                                      |
|---------|----------|---------|--------------------------------------------------|
| topic 0 | topics   | Symbol  | `"vault_unpaused"`                               |
| topic 1 | topics   | Address | `caller` â€” admin or owner who triggered unpause |
| data    | data     | ()      | empty                                            |

**Indexer Note:** After this event is emitted, `is_paused()` view function returns `false`.
All vault operations are restored: `deposit()`, `deduct()`, `batch_deduct()`.

---

### View Function: `is_paused()`

The vault exposes a read-only view function for off-chain systems to query the current pause state.

**Signature:** `pub fn is_paused(env: Env) -> bool`

**Return Value:**
- `true` â€” Vault is currently paused (circuit-breaker active)
- `false` â€” Vault is operational (normal state)

**Safety Guarantees:**
- **Read-only**: No state mutation or side effects
- **Deterministic**: Identical state always produces identical output
- **Non-panicking**: Never panics, even before initialization
- **Safe default**: Returns `false` when pause state is unset

**Indexer Usage:**
```javascript
// Check if vault is paused before processing transactions
const isPaused = await vault.isPaused();
if (isPaused) {
  // Vault is paused - deposits and deductions are blocked
  // Only admin/owner operations like withdraw() are allowed
} else {
  // Vault is operational - all functions available
}
```

**Consistency with Events:**
- `vault_paused` event emitted â†’ `is_paused()` returns `true`
- `vault_unpaused` event emitted â†’ `is_paused()` returns `false`

Indexers should use `is_paused()` for current state queries and subscribe to
`vault_paused`/`vault_unpaused` events for state change notifications.

---

### `set_revenue_pool`

Emitted when the admin sets a revenue pool address.

| Index   | Location | Type    | Description        |
|---------|-----------|---------|--------------------|
| topic 0 | topics   | Symbol  | `"set_revenue_pool"` |
| topic 1 | topics   | Address | caller (admin)     |
| data    | data     | Address | new revenue pool   |

```json
{
  "topics": ["set_revenue_pool", "GADMIN..."],
  "data": "GPOOL..."
}
```

---

### `clear_revenue_pool`

Emitted when the admin clears the revenue pool address.

| Index   | Location | Type    | Description    |
|---------|----------|---------|----------------|
| topic 0 | topics   | Symbol  | `"clear_revenue_pool"` |
| topic 1 | topics   | Address | caller (admin) |
| data    | data     | ()      | empty          |

```json
{
  "topics": ["clear_revenue_pool", "GADMIN..."],
  "data": null
}
```

---

### `metadata_set`

Emitted when offering metadata is stored for the first time.

| Index   | Location | Type    | Description               |
|---------|----------|---------|---------------------------|
| topic 0 | topics   | Symbol  | `"metadata_set"`          |
| topic 1 | topics   | String  | offering_id               |
| topic 2 | topics   | Address | caller (owner)            |
| data    | data     | String  | metadata (IPFS CID / URI) |

```json
{
  "topics": ["metadata_set", "offering-001", "GOWNER..."],
  "data": "ipfs://bafybeigdyrzt..."
}
```

---

### `metadata_updated`

Emitted when existing offering metadata is replaced.

| Index   | Location | Type             | Description                    |
|---------|----------|------------------|--------------------------------|
| topic 0 | topics   | Symbol           | `"metadata_updated"`           |
| topic 1 | topics   | String           | offering_id                    |
| topic 2 | topics   | Address          | caller (owner)                 |
| data    | data     | (String, String) | (old_metadata, new_metadata)   |

```json
{
  "topics": ["metadata_updated", "offering-001", "GOWNER..."],
  "data": ["ipfs://old...", "ipfs://new..."]
}
```

---

### `set_authorized_caller`

Emitted when the owner updates the authorized caller address.

| Index   | Location | Type                              | Description                                  |
|---------|----------|-----------------------------------|----------------------------------------------|
| topic 0 | topics   | Symbol                            | `"set_authorized_caller"`                   |
| topic 1 | topics   | Address                           | vault owner                                  |
| data    | data     | (Option<Address>, Option<Address>) | (old_authorized_caller, new_authorized_caller) |

```json
{
  "topics": ["set_authorized_caller", "GOWNER..."],
  "data": [null, "GCALLER..."]
}
```

---

---

### `admin_nominated`

Emitted when the current admin nominates a successor.

| Field   | Location | Type   | Description   |
|---------|----------|--------|-----------------------|
| topic 0 | topics   | Symbol | `"admin_nominated"` |
| topic 1 | topics   | Address| current admin |
| topic 2 | topics   | Address| nominee       |
| data    | data     | ()     | empty         |

---

### `admin_accepted`

Emitted when the nominee accepts the admin role.

| Field   | Location | Type   | Description   |
|---------|----------|--------|-----------------------|
| topic 0 | topics   | Symbol | `"admin_accepted"` |
| topic 1 | topics   | Address| old admin     |
| topic 2 | topics   | Address| new admin     |
| data    | data     | ()     | empty         |

---

## Contract: `callora-revenue-pool` (v0.0.1)

The revenue pool receives USDC forwarded by the vault on every `deduct` / `batch_deduct`
call and lets the admin distribute those funds to developers.

### `init`

Emitted once when the revenue pool is initialized.

| Index   | Location | Type    | Description                          |
|---------|----------|---------|--------------------------------------|
| topic 0 | topics   | Symbol  | `"init"`                             |
| topic 1 | topics   | Address | `admin` â€” initial admin address      |
| data    | data     | Address | `usdc_token` â€” token contract address|

```json
{
  "topics": ["init", "GADMIN..."],
  "data": "GUSDC_TOKEN..."
}
```

> **Security note:** `usdc_token` is immutable after `init`. Verify it matches the
> canonical Stellar USDC contract before deployment.

---

### `admin_transfer_started`

Emitted when the current admin nominates a successor (step 1 of 2).

| Index   | Location | Type    | Description                              |
|---------|----------|---------|------------------------------------------|
| topic 0 | topics   | Symbol  | `"admin_transfer_started"`               |
| topic 1 | topics   | Address | `current_admin` â€” the nominator          |
| data    | data     | Address | `pending_admin` â€” nominee who must accept|

```json
{
  "topics": ["admin_transfer_started", "GCURRENT_ADMIN..."],
  "data": "GPENDING_ADMIN..."
}
```

> Indexers should treat funds as still under `current_admin` control until
> `admin_transfer_completed` is observed.

---

### `admin_changed`

Emitted when `set_admin()` is called to record the requested admin change.
This event is emitted immediately before `admin_transfer_started`.

| Index   | Location | Type               | Description                           |
|---------|----------|--------------------|---------------------------------------|
| topic 0 | topics   | Symbol             | `"admin_changed"`                     |
| topic 1 | topics   | Address            | `current_admin` — caller/admin        |
| data    | data     | (Address, Address) | `(old_admin, new_admin)`              |

```json
{
  "topics": ["admin_changed", "GCURRENT_ADMIN..."],
  "data": ["GCURRENT_ADMIN...", "GPENDING_ADMIN..."]
}
```

---

### `admin_transfer_completed`

Emitted when the nominee accepts the admin role (step 2 of 2).

| Index   | Location | Type    | Description                        |
|---------|-----------|---------|------------------------------------|
| topic 0 | topics   | Symbol  | `"admin_transfer_completed"`       |
| topic 1 | topics   | Address | `new_admin` â€” the accepted admin   |
| data    | data     | ()      | empty                              |

```json
{
  "topics": ["admin_transfer_completed", "GNEW_ADMIN..."],
  "data": null
}
```

> After this event, only `new_admin` can call `distribute`, `batch_distribute`,
> `receive_payment`, and `set_admin`.

---

### `receive_payment`

Emitted when the admin logs an inbound payment from the vault.

> **Note:** This is an **event-only helper** â€” it does not move tokens. USDC
> arrives via a direct token transfer from the vault. Call `receive_payment` to
> emit this event for indexer alignment.

| Index   | Location | Type         | Description                                     |
|---------|-----------|--------------|-------------------------------------------------|
| topic 0 | topics   | Symbol       | `"receive_payment"`                             |
| topic 1 | topics   | Address      | `caller` â€” typically admin                      |
| data    | data     | (i128, bool) | `(amount, from_vault)` â€” amount in stroops; `from_vault=true` when source is the vault |

```json
{
  "topics": ["receive_payment", "GADMIN..."],
  "data": [5000000, true]
}
```

**Example â€” manual top-up (not from vault):**

```json
{
  "topics": ["receive_payment", "GADMIN..."],
  "data": [1000000, false]
}
```

> Indexers tracking total inflows should subscribe to this event and filter on
> `from_vault` to distinguish vault-originated payments from manual top-ups.

---

### `distribute`

Emitted when the admin distributes USDC to a single developer.

| Index   | Location | Type    | Description              |
|---------|----------|---------|--------------------------|
| topic 0 | topics   | Symbol  | `"distribute"`           |
| topic 1 | topics   | Address | `to` â€” developer address |
| data    | data     | i128    | `amount` in stroops      |

```json
{
  "topics": ["distribute", "GDEVELOPER..."],
  "data": 2500000
}
```

> A `distribute` event guarantees the token transfer succeeded â€” the USDC has
> left the pool contract and arrived at `to`.

---

### `batch_distribute`

Emitted **once per payment** during a `batch_distribute()` call. If a batch has
three payments, three `batch_distribute` events are emitted in order.

| Index   | Location | Type    | Description              |
|---------|----------|---------|--------------------------|
| topic 0 | topics   | Symbol  | `"batch_distribute"`     |
| topic 1 | topics   | Address | `to` â€” developer address |
| data    | data     | i128    | `amount` in stroops      |

```json
{
  "topics": ["batch_distribute", "GDEVELOPER_A..."],
  "data": 1000000
}
```

**Example â€” 3-payment batch produces 3 events:**

```json
[
  { "topics": ["batch_distribute", "GDEV_A..."], "data": 1000000 },
  { "topics": ["batch_distribute", "GDEV_B..."], "data": 2000000 },
  { "topics": ["batch_distribute", "GDEV_C..."], "data": 500000  }
]
```

> `batch_distribute` is atomic â€” either all payments succeed and all events are
> emitted, or none are. Indexers can verify atomicity by checking that all events
> share the same ledger sequence number.

---

## Contract: `callora-settlement` (v0.1.0)

Source: [`contracts/settlement/src/lib.rs`](contracts/settlement/src/lib.rs).

**Amount units.** All `amount` / `new_balance` fields are `i128` in USDC
micro-units (7-decimal scaled integers), matching the Stellar USDC contract.
Legacy text elsewhere in this document calls this "stroops" â€” same scalar type,
same integer semantics; the settlement contract never handles native XLM.

**Data payload encoding.** The `data` column describes the Soroban
`contracttype` struct published by `env.events().publish(...)`. On the wire
each struct is a single XDR value whose field names match the Rust struct;
the JSON examples below are the logical field view an indexer sees after
decoding, not a raw array. The struct layouts live in `lib.rs`:
`PaymentReceivedEvent` and `BalanceCreditedEvent`.

**Emit atomicity and ordering.** Both events originate inside one
`receive_payment()` call, so they share the same transaction and ledger
sequence. When `to_pool = false`, `payment_received` is always emitted
**before** `balance_credited`. If any guard panics (see "Panic modes" below)
no events are emitted and state is rolled back.

**Panic modes (no events emitted).**
- Caller is not the registered vault or admin (`require_authorized_caller`).
- `amount <= 0` â€” `"amount must be positive"`.
- `to_pool = true` with `developer = Some(_)` â€” `"developer address must be None when to_pool=true"`.
- `to_pool = false` with `developer = None` â€” `"developer address required when to_pool=false"`.
- Arithmetic overflow on pool or developer balance â€” `"pool balance overflow"` / `"developer balance overflow"`.

---

### `payment_received`

Emitted by `receive_payment()` for every successful inbound payment,
regardless of routing.

| Index        | Location | Type              | Description                                                                       |
|--------------|----------|-------------------|-----------------------------------------------------------------------------------|
| topic 0      | topics   | Symbol            | `"payment_received"`                                                              |
| topic 1      | topics   | Address           | `caller` â€” authorized vault or admin address (same as `from_vault` field)         |
| `from_vault` | data     | Address           | originator of the payment; duplicates topic 1 for indexers that key by data only  |
| `amount`     | data     | i128              | payment amount in USDC micro-units; invariant `amount > 0`                        |
| `to_pool`    | data     | bool              | `true` â†’ credited to global pool; `false` â†’ credited to an individual developer   |
| `developer`  | data     | Option\<Address\> | `None` when `to_pool = true`; `Some(address)` when `to_pool = false`              |

**Example â€” global pool credit (`to_pool = true`):**

```json
{
  "topics": ["payment_received", "GCALLER..."],
  "data": {
    "from_vault": "GCALLER...",
    "amount": 5000000,
    "to_pool": true,
    "developer": null
  }
}
```

Side effect: `GlobalPool.total_balance += amount` and
`GlobalPool.last_updated = env.ledger().timestamp()`.

**Example â€” developer credit (`to_pool = false`):**

```json
{
  "topics": ["payment_received", "GCALLER..."],
  "data": {
    "from_vault": "GCALLER...",
    "amount": 2500000,
    "to_pool": false,
    "developer": "GDEV..."
  }
}
```

Side effect: developer balance map entry for `GDEV...` is incremented by
`amount`. `GlobalPool.last_updated` is **not** touched on developer credits.

**Indexer guidance.**
- `topic 1` is always the caller; filter on it to isolate payments from a
  specific vault or admin.
- `developer` is the only field that distinguishes pool vs. developer credits
  in the data payload; the `to_pool` boolean is redundant but stable and
  cheaper to filter on.
- A `payment_received` with `to_pool = false` is always paired with exactly
  one `balance_credited` event in the same transaction.

---

### `balance_credited`

Emitted by `receive_payment()` **only** when `to_pool = false`, immediately
after the matching `payment_received` event.

| Index         | Location | Type    | Description                                                     |
|---------------|----------|---------|-----------------------------------------------------------------|
| topic 0       | topics   | Symbol  | `"balance_credited"`                                            |
| topic 1       | topics   | Address | `developer` â€” address whose balance was updated                 |
| `developer`   | data     | Address | same as topic 1; duplicated for data-only indexers              |
| `amount`      | data     | i128    | amount credited to the developer in USDC micro-units            |
| `new_balance` | data     | i128    | developer's cumulative balance after this credit (post-state)   |

```json
{
  "topics": ["balance_credited", "GDEV..."],
  "data": {
    "developer": "GDEV...",
    "amount": 2500000,
    "new_balance": 7500000
  }
}
```

**Invariants.**
- `new_balance = prior_balance + amount`, checked for `i128` overflow; overflow
  panics and rolls back both events.
- `new_balance` equals `CalloraSettlement::get_developer_balance(developer)`
  immediately after the emitting transaction.
- `amount` in `balance_credited` equals `amount` in the paired
  `payment_received`.

**Indexer guidance.**
- Track developer earnings by subscribing to `balance_credited` â€” it already
  carries the post-credit balance, so no separate read is required.
- Track total protocol inflow by summing `payment_received.amount` across
  both routing modes, or filter `to_pool = true` for pool-only inflow.
- `balance_credited` is **never** emitted when `to_pool = true`; do not wait
  for one on pool credits.

---

## Indexer quick-reference

| Event                    | Contract        | Trigger                                  |
|--------------------------|-----------------|------------------------------------------|
| `init`                   | vault           | `init()`                                 |
| `deposit`                | vault           | `deposit()`                              |
| `deduct`                 | vault           | `deduct()` / each item in `batch_deduct()`|
| `withdraw`               | vault           | `withdraw()`                             |
| `withdraw_to`            | vault           | `withdraw_to()`                          |
| `vault_paused`           | vault           | `pause()`                                |
| `vault_unpaused`         | vault           | `unpause()`                              |
| `ownership_nominated`    | vault           | `transfer_ownership()`                   |
| `ownership_accepted`     | vault           | `accept_ownership()`                     |
| `admin_nominated`        | vault           | `set_admin()`                            |
| `admin_accepted`         | vault           | `accept_admin()`                         |
| `set_revenue_pool`       | vault           | `set_revenue_pool(Some(addr))`           |
| `clear_revenue_pool`     | vault           | `set_revenue_pool(None)`                 |
| `set_max_deduct`         | vault           | `set_max_deduct()`                       |
| `set_authorized_caller` | vault           | `set_authorized_caller()`                |
| `metadata_set`           | vault           | `set_metadata()`                         |
| `metadata_updated`       | vault           | `update_metadata()`                      |
| `distribute`             | vault           | `distribute()`                           |
| `init`                   | revenue-pool    | `init()`                                 |
| `admin_changed`          | revenue-pool    | `set_admin()`                            |
| `admin_transfer_started` | revenue-pool    | `set_admin()`                            |
| `admin_transfer_completed`| revenue-pool   | `claim_admin()`                          |
| `receive_payment`        | revenue-pool    | `receive_payment()`                      |
| `distribute`             | revenue-pool    | `distribute()`                           |
| `batch_distribute`       | revenue-pool    | each payment in `batch_distribute()`     |
| `payment_received`       | settlement      | `receive_payment()`                      |
| `balance_credited`       | settlement      | `receive_payment()` with `to_pool=false` |

---

## Version history

| Version | Contract      | Change                                                       |
|---------|---------------|--------------------------------------------------------------|
| 0.0.1   | vault         | Initial vault events                                         |
| 0.0.1   | vault         | Added `set_authorized_caller` event with old/new value payload (Issue #256) |
| 0.0.1   | revenue-pool  | Full revenue pool event suite with JSON examples             |
| 0.0.1   | revenue-pool  | Added `admin_changed` event on `set_admin` for explicit old/new admin intent |
| 0.1.0   | settlement    | `payment_received`, `balance_credited`                       |
