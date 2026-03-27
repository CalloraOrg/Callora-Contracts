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

## Not yet implemented

- **OwnershipTransfer**: not present in current vault; would list old_owner, new_owner.
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

### Version notes

| Version | Change |
|---------|--------|
| 0.1.0   | Initial settlement events: `payment_received`, `balance_credited` |

---

## Contract: Callora Revenue Pool (`callora-revenue-pool` v0.0.1)

### `init`

Emitted when the revenue pool is initialized.

| Field   | Location | Type   | Description           |
|---------|----------|--------|-----------------------|
| topic 0 | topics   | Symbol | `"init"`              |
| topic 1 | topics   | Address| admin address         |
| data    | data     | Address| USDC token address    |

---

### `receive_payment`

Emitted when a payment is logged as received (manually by admin).

| Field   | Location | Type   | Description           |
|---------|----------|--------|-----------------------|
| topic 0 | topics   | Symbol | `"receive_payment"`   |
| topic 1 | topics   | Address| caller (admin)        |
| data    | data     | (i128, bool) | (amount, from_vault)  |

---

### `distribute`

Emitted when USDC is distributed to a developer.

| Field   | Location | Type   | Description           |
|---------|----------|--------|-----------------------|
| topic 0 | topics   | Symbol | `"distribute"`        |
| topic 1 | topics   | Address| recipient developer   |
| data    | data     | i128   | distributed amount    |

---

### `batch_distribute`

Emitted for each payment in a batch distribution.

| Field   | Location | Type   | Description           |
|---------|----------|--------|-----------------------|
| topic 0 | topics   | Symbol | `"batch_distribute"`  |
| topic 1 | topics   | Address| recipient developer   |
| data    | data     | i128   | distributed amount    |
