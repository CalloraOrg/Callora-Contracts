# Event Schema (Vault Contract)

Events emitted by the Callora vault contract for indexers and frontends. All topic/data types refer to Soroban/Stellar XDR values.

## Contract: Callora Vault

### `init`

Emitted when the vault is initialized.

| Field   | Location | Type   | Description           |
|---------|----------|--------|-----------------------|
| topic 0 | topics   | Symbol | `"init"`              |
| topic 1 | topics   | Address| vault owner           |
| data    | data     | i128   | initial balance       |

---

### `deposit`

Emitted when balance is increased via `deposit(amount)`.

| Field   | Location | Type   | Description   |
|---------|----------|--------|---------------|
| topic 0 | topics   | Symbol | `"deposit"`   |
| data    | data     | (i128, i128) | (amount, new_balance) |

---

### `deduct`

Emitted on each deduction: single `deduct(amount)` or each item in `batch_deduct(items)`.

| Field   | Location | Type   | Description   |
|---------|----------|--------|---------------|
| topic 0 | topics   | Symbol | `"deduct"`    |
| topic 1 | topics   | Address| caller        |
| topic 2 | topics   | Symbol | optional request_id (empty symbol if none) |
| data    | data     | (i128, i128) | (amount, new_balance) |

---

### `withdraw`

Emitted when the owner withdraws via `withdraw(amount)`.

| Field   | Location | Type   | Description   |
|---------|----------|--------|---------------|
| topic 0 | topics   | Symbol | `"withdraw"`  |
| topic 1 | topics   | Address| vault owner   |
| data    | data     | (i128, i128) | (amount, new_balance) |

---

### `withdraw_to`

Emitted when the owner withdraws to a designated address via `withdraw_to(to, amount)`.

| Field   | Location | Type   | Description   |
|---------|----------|--------|---------------|
| topic 0 | topics   | Symbol | `"withdraw_to"` |
| topic 1 | topics   | Address| vault owner   |
| topic 2 | topics   | Address| recipient `to` |
| data    | data     | (i128, i128) | (amount, new_balance) |

---

### `metadata_set`

Emitted when metadata is set for an offering via `set_metadata(offering_id, metadata)`.

| Field   | Location | Type   | Description   |
|---------|----------|--------|---------------|
| topic 0 | topics   | Symbol | `"metadata_set"` |
| topic 1 | topics   | String | offering_id   |
| topic 2 | topics   | Address| caller (owner/issuer) |
| data    | data     | String | metadata (IPFS CID or URI) |

---

### `metadata_updated`

Emitted when existing metadata is updated via `update_metadata(offering_id, metadata)`.

| Field   | Location | Type   | Description   |
|---------|----------|--------|---------------|
| topic 0 | topics   | Symbol | `"metadata_updated"` |
| topic 1 | topics   | String | offering_id   |
| topic 2 | topics   | Address| caller (owner/issuer) |
| data    | data     | (String, String) | (old_metadata, new_metadata) |

---

### `ownership_nominated`
| Field   | Location | Type   | Description   |
|---------|----------|--------|---------------|
| topic 0 | topics   | Symbol | `"ownership_nominated"` |
| topic 1 | topics   | Address| current owner |
| topic 2 | topics   | Address| nominee       |
| data    | data     | ()     | empty         |

---

### `ownership_accepted`
| Field   | Location | Type   | Description   |
|---------|----------|--------|---------------|
| topic 0 | topics   | Symbol | `"ownership_accepted"` |
| topic 1 | topics   | Address| old owner     |
| topic 2 | topics   | Address| new owner     |
| data    | data     | ()     | empty         |

---

### `admin_nominated`
| Field   | Location | Type   | Description   |
|---------|----------|--------|---------------|
| topic 0 | topics   | Symbol | `"admin_nominated"` |
| topic 1 | topics   | Address| current admin |
| topic 2 | topics   | Address| nominee       |
| data    | data     | ()     | empty         |

- **Pause**: not present in current vault; would indicate pause state change.

---

## Contract: Callora Settlement (`callora-settlement` v0.1.0)

### `payment_received`

Emitted by `receive_payment()` for every inbound payment regardless of `to_pool` mode.

| Field   | Location | Type    | Description                                                                 |
|---------|----------|---------|-----------------------------------------------------------------------------|
| topic 0 | topics   | Symbol  | `"payment_received"`                                                        |
| topic 1 | topics   | Address | `caller` — the vault or admin address that sent the payment                 |
| `from_vault` | data | Address | same as topic 1 (vault/admin caller)                                   |
| `amount`     | data | i128    | payment amount in USDC micro-units (stroops); always > 0                |
| `to_pool`    | data | bool    | `true` → credited to global pool; `false` → credited to a developer    |
| `developer`  | data | Option\<Address\> | `None` when `to_pool=true`; developer address when `to_pool=false` |

**Example — `to_pool = true` (global pool credit):**

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

**Example — `to_pool = false` (developer credit):**

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

Emitted by `receive_payment()` **only** when `to_pool = false`. Follows the `payment_received` event for the same call.

| Field         | Location | Type    | Description                                          |
|---------------|----------|---------|------------------------------------------------------|
| topic 0       | topics   | Symbol  | `"balance_credited"`                                 |
| topic 1       | topics   | Address | `developer` — the address whose balance was updated  |
| `developer`   | data     | Address | same as topic 1                                      |
| `amount`      | data     | i128    | amount credited in this call (USDC micro-units)      |
| `new_balance` | data     | i128    | developer's cumulative balance after this credit     |

**Example:**

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

> **Note:** `balance_credited` is never emitted when `to_pool = true`. Indexers tracking developer earnings should subscribe to this event; indexers tracking total protocol revenue should subscribe to `payment_received` with `to_pool = true`.

---

## Contract: Callora Revenue Pool (`callora-revenue-pool` v0.0.1)

### `init`

Emitted when the revenue pool is initialized.

| Field   | Location | Type    | Description                                      |
|---------|----------|---------|--------------------------------------------------|
| topic 0 | topics   | Symbol  | `"init"`                                         |
| topic 1 | topics   | Address | `admin` — initial admin address                 |
| data    | data     | Address | `usdc_token` — token contract address used      |

---

### `receive_payment`

Emitted by `receive_payment(caller, amount, from_vault)`.

| Field   | Location | Type    | Description                                      |
|---------|----------|---------|--------------------------------------------------|
| topic 0 | topics   | Symbol  | `"receive_payment"`                              |
| topic 1 | topics   | Address | `caller` — typically admin or vault              |
| data    | data     | (i128, bool) | (amount, from_vault)                        |

---

### `distribute`

Emitted when USDC is distributed to a single developer.

| Field   | Location | Type    | Description                                      |
|---------|----------|---------|--------------------------------------------------|
| topic 0 | topics   | Symbol  | `"distribute"`                                   |
| topic 1 | topics   | Address | `to` — developer address                        |
| data    | data     | i128    | `amount` distributed                             |

---

### `batch_distribute`

Emitted for every individual distribution during a `batch_distribute(payments)` call.

| Field   | Location | Type    | Description                                      |
|---------|----------|---------|--------------------------------------------------|
| topic 0 | topics   | Symbol  | `"batch_distribute"`                             |
| topic 1 | topics   | Address | `to` — developer address                        |
| data    | data     | i128    | `amount` distributed                             |

---

### `admin_transfer_started`

Emitted when the current admin nominates a successor.

| Field   | Location | Type    | Description                                      |
|---------|----------|---------|--------------------------------------------------|
| topic 0 | topics   | Symbol  | `"admin_transfer_started"`                       |
| topic 1 | topics   | Address | `current_admin` — the nominator                 |
| data    | data     | Address | `pending_admin` — the nominee who must accept   |

---

### `admin_transfer_completed`

Emitted when the nominee accepts the admin role.

| Field   | Location | Type    | Description                                      |
|---------|----------|---------|--------------------------------------------------|
| topic 0 | topics   | Symbol  | `"admin_transfer_completed"`                     |
| topic 1 | topics   | Address | `new_admin` — the nominee who is now admin      |
| data    | data     | ()      | empty                                            |

---

### Version notes

| Version | Change |
|---------|--------|
| 0.1.0   | Initial settlement events: `payment_received`, `balance_credited` |
| 0.0.1   | Added Revenue Pool events and admin rotation audit trail events |

> If events structs gain new fields in future versions, a new row will be added here with the crate semver and a description of the added/changed fields.
