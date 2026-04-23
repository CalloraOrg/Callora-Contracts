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

Emitted on each deduction — once per `deduct()` call and once per item in `batch_deduct()`.

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

---

### `withdraw`

Emitted when the vault owner withdraws to their own address.

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
| topic 1 | topics   | Address | `caller` — admin or owner who triggered pause   |
| data    | data     | ()      | empty                                            |

**Indexer Note:** After this event is emitted, `is_paused()` view function returns `true`.
The following operations are blocked until unpause: `deposit()`, `deduct()`, `batch_deduct()`.

---

### `vault_unpaused`

Emitted when the vault circuit-breaker is deactivated by admin or owner.

| Field   | Location | Type    | Description                                      |
|---------|----------|---------|--------------------------------------------------|
| topic 0 | topics   | Symbol  | `"vault_unpaused"`                               |
| topic 1 | topics   | Address | `caller` — admin or owner who triggered unpause |
| data    | data     | ()      | empty                                            |

**Indexer Note:** After this event is emitted, `is_paused()` view function returns `false`.
All vault operations are restored: `deposit()`, `deduct()`, `batch_deduct()`.

---

### View Function: `is_paused()`

The vault exposes a read-only view function for off-chain systems to query the current pause state.

**Signature:** `pub fn is_paused(env: Env) -> bool`

**Return Value:**
- `true` — Vault is currently paused (circuit-breaker active)
- `false` — Vault is operational (normal state)

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
- `vault_paused` event emitted → `is_paused()` returns `true`
- `vault_unpaused` event emitted → `is_paused()` returns `false`

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

### `set_auth_caller`

Emitted when the owner updates the authorized caller address.

| Index   | Location | Type    | Description               |
|---------|----------|---------|---------------------------|
| topic 0 | topics   | Symbol  | `"set_auth_caller"`       |
| topic 1 | topics   | Address | vault owner               |
| data    | data     | Address | new authorized caller     |

```json
{
  "topics": ["set_auth_caller", "GOWNER..."],
  "data": "GCALLER..."
}
```

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

> **Note:** `balance_credited` is never emitted when `to_pool = true`. Indexers tracking developer earnings should subscribe to this event; indexers tracking total protocol revenue should subscribe to `payment_received` with `to_pool = true`.

---

## Contract: `callora-revenue-pool` (v0.0.1)

The revenue pool receives USDC forwarded by the vault on every `deduct` / `batch_deduct`
call and lets the admin distribute those funds to developers.

### `init`

Emitted once when the revenue pool is initialized.

| Index   | Location | Type    | Description                          |
|---------|----------|---------|--------------------------------------|
| topic 0 | topics   | Symbol  | `"init"`                             |
| topic 1 | topics   | Address | `admin` — initial admin address      |
| data    | data     | Address | `usdc_token` — token contract address|

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
| topic 1 | topics   | Address | `current_admin` — the nominator          |
| data    | data     | Address | `pending_admin` — nominee who must accept|

```json
{
  "topics": ["admin_transfer_started", "GCURRENT_ADMIN..."],
  "data": "GPENDING_ADMIN..."
}
```

> Indexers should treat funds as still under `current_admin` control until
> `admin_transfer_completed` is observed.

---

### `admin_transfer_completed`

Emitted when the nominee accepts the admin role (step 2 of 2).

| Index   | Location | Type    | Description                        |
|---------|-----------|---------|------------------------------------|
| topic 0 | topics   | Symbol  | `"admin_transfer_completed"`       |
| topic 1 | topics   | Address | `new_admin` — the accepted admin   |
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

> **Note:** This is an **event-only helper** — it does not move tokens. USDC
> arrives via a direct token transfer from the vault. Call `receive_payment` to
> emit this event for indexer alignment.

| Index   | Location | Type         | Description                                     |
|---------|-----------|--------------|-------------------------------------------------|
| topic 0 | topics   | Symbol       | `"receive_payment"`                             |
| topic 1 | topics   | Address      | `caller` — typically admin                      |
| data    | data     | (i128, bool) | `(amount, from_vault)` — amount in stroops; `from_vault=true` when source is the vault |

```json
{
  "topics": ["receive_payment", "GADMIN..."],
  "data": [5000000, true]
}
```

**Example — manual top-up (not from vault):**

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
| topic 1 | topics   | Address | `to` — developer address |
| data    | data     | i128    | `amount` in stroops      |

```json
{
  "topics": ["distribute", "GDEVELOPER..."],
  "data": 2500000
}
```

> A `distribute` event guarantees the token transfer succeeded — the USDC has
> left the pool contract and arrived at `to`.

---

### `batch_distribute`

Emitted **once per payment** during a `batch_distribute()` call. If a batch has
three payments, three `batch_distribute` events are emitted in order.

| Index   | Location | Type    | Description              |
|---------|----------|---------|--------------------------|
| topic 0 | topics   | Symbol  | `"batch_distribute"`     |
| topic 1 | topics   | Address | `to` — developer address |
| data    | data     | i128    | `amount` in stroops      |

```json
{
  "topics": ["batch_distribute", "GDEVELOPER_A..."],
  "data": 1000000
}
```

**Example — 3-payment batch produces 3 events:**

```json
[
  { "topics": ["batch_distribute", "GDEV_A..."], "data": 1000000 },
  { "topics": ["batch_distribute", "GDEV_B..."], "data": 2000000 },
  { "topics": ["batch_distribute", "GDEV_C..."], "data": 500000  }
]
```

> `batch_distribute` is atomic — either all payments succeed and all events are
> emitted, or none are. Indexers can verify atomicity by checking that all events
> share the same ledger sequence number.

---

## Contract: `callora-settlement` (v0.1.0)

### `payment_received`

Emitted by `receive_payment()` for every inbound payment regardless of routing.

| Index        | Location | Type             | Description                                                              |
|--------------|----------|------------------|--------------------------------------------------------------------------|
| topic 0      | topics   | Symbol           | `"payment_received"`                                                     |
| topic 1      | topics   | Address          | `caller` — vault or admin address                                        |
| `from_vault` | data     | Address          | same as topic 1                                                          |
| `amount`     | data     | i128             | payment amount in stroops; always > 0                                    |
| `to_pool`    | data     | bool             | `true` → credited to global pool; `false` → credited to a developer     |
| `developer`  | data     | Option\<Address\>| `None` when `to_pool=true`; developer address when `to_pool=false`      |

**Example — global pool credit:**

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

**Example — developer credit:**

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

---

### `balance_credited`

Emitted by `receive_payment()` **only** when `to_pool = false`.

| Index         | Location | Type    | Description                                      |
|---------------|----------|---------|--------------------------------------------------|
| topic 0       | topics   | Symbol  | `"balance_credited"`                             |
| topic 1       | topics   | Address | `developer` — address whose balance was updated  |
| `developer`   | data     | Address | same as topic 1                                  |
| `amount`      | data     | i128    | amount credited (stroops)                        |
| `new_balance` | data     | i128    | developer's cumulative balance after this credit |

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

> `balance_credited` is never emitted when `to_pool = true`. Indexers tracking
> developer earnings should subscribe to this event; indexers tracking total
> protocol revenue should subscribe to `payment_received` with `to_pool = true`.

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
| `set_auth_caller`        | vault           | `set_authorized_caller()`                |
| `metadata_set`           | vault           | `set_metadata()`                         |
| `metadata_updated`       | vault           | `update_metadata()`                      |
| `distribute`             | vault           | `distribute()`                           |
| `init`                   | revenue-pool    | `init()`                                 |
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
| 0.0.1   | vault         | Added `set_auth_caller` event (Issue #256)                   |
| 0.0.1   | revenue-pool  | Full revenue pool event suite with JSON examples             |
| 0.1.0   | settlement    | `payment_received`, `balance_credited`                       |