# Vault Storage Layout

This document describes the storage layout of the Callora Vault contract, including storage keys, data types, and access control implications.

## Instance Storage TTL

All critical vault state lives in instance storage. To prevent archival on infrequently-used vaults, every mutating entrypoint calls `env.storage().instance().extend_ttl(threshold, extend_to)`.

| Constant                  | Value                    | Rationale                                                    |
| ------------------------- | ------------------------ | ------------------------------------------------------------ |
| `INSTANCE_BUMP_THRESHOLD` | `17_280 * 30` (~30 days) | Bump is triggered when fewer than 30 days of TTL remain      |
| `INSTANCE_BUMP_AMOUNT`    | `17_280 * 60` (~60 days) | Each bump extends the TTL to 60 days from the current ledger |

Ledger rate assumption: **17 280 ledgers/day** (5-second close time on Stellar mainnet).

Entrypoints that bump TTL: `init`, `deposit`, `deduct`, `batch_deduct`, `withdraw`, `withdraw_to`.

Pure view functions (`get_meta`, `balance`, `get_admin`, `get_usdc_token`, `get_settlement`, `get_revenue_pool`, `get_contract_addresses`, `is_paused`, `is_authorized_depositor`, `get_metadata`, `get_max_deduct`, `get_allowed_depositors`, `is_request_processed`) do **not** bump the TTL — they are read-only and incur no write cost.

## Processed-Request Idempotency Storage (Temporary)

Idempotency markers for `deduct` and `batch_deduct` live in **temporary storage** — a separate Soroban storage tier that is automatically archived when its TTL expires, without requiring explicit deletion.

| Constant                    | Value                    | Rationale                                                    |
| --------------------------- | ------------------------ | ------------------------------------------------------------ |
| `REQUEST_ID_BUMP_THRESHOLD` | `17_280 * 7` (~7 days)   | Bump is triggered when fewer than 7 days of TTL remain       |
| `REQUEST_ID_BUMP_AMOUNT`    | `17_280 * 30` (~30 days) | Each bump extends the TTL to 30 days from the current ledger |

### Key: `StorageKey::ProcessedRequest(Symbol)`

- **Storage tier:** Temporary (auto-archived after TTL expires)
- **Value type:** `bool` (`true`); presence of the key is the authoritative signal
- **Written by:** `deduct` and `batch_deduct` on every **successful** deduction where `request_id` is `Some(id)`
- **Read by:** `deduct`, `batch_deduct` (duplicate check), `is_request_processed` (view)
- **TTL:** Set to `REQUEST_ID_BUMP_AMOUNT` (~30 days) on write; bumped on every successful re-use within the threshold window

### Retention Policy

| Scenario                      | Behaviour                                            |
| ----------------------------- | ---------------------------------------------------- |
| First deduct with `Some(id)`  | Marker written; TTL set to ~30 days                  |
| Retry within retention window | `DuplicateRequestId` error returned; no state change |
| Retry after TTL expires       | Marker archived; deduct treated as new (succeeds)    |
| Deduct with `None`            | No marker written; no deduplication                  |
| Failed deduct (any error)     | No marker written; id remains reusable               |

> **Caller guidance:** Backends should treat `VaultError::DuplicateRequestId` as a successful no-op — the original deduction already went through. Do not retry with a new `request_id` for the same logical operation.

> **Retention window:** The 30-day window is a best-effort guarantee. After expiry the marker is archived and the `request_id` can be reused. Callers requiring longer deduplication windows must implement their own off-chain tracking.

## Storage Overview

The Callora Vault contract uses Soroban's instance storage to persist contract state. Data is organized using the `StorageKey` enum, providing type-safe access to contract state.

## Storage Keys

The contract defines the following storage keys:

```rust
#[contracttype]
pub enum StorageKey {
    MetaKey,                       // VaultMeta
    Admin,                         // Address
    UsdcToken,                     // Address
    Settlement,                    // Address
    RevenuePool,                   // Option<Address>
    MaxDeduct,                     // i128
    Paused,                        // bool
    Metadata(String),              // String (offering metadata by offering_id)
    PendingOwner,                  // Address
    PendingAdmin,                  // Address
    DepositorList,                 // Vec<Address>
    ContractVersion,               // BytesN<32>
    ProcessedRequest(Symbol),      // bool — temporary storage, idempotency marker
}
```

### Storage Keys Table

| Key Variant                | Storage Tier  | Value Type        | Description                                            | Access                                                                     |
| -------------------------- | ------------- | ----------------- | ------------------------------------------------------ | -------------------------------------------------------------------------- |
| `MetaKey`                  | Instance      | `VaultMeta`       | Owner, balance, authorized_caller, min_deposit         | `get_meta()`, updated by deposit/deduct/withdraw                           |
| `Admin`                    | Instance      | `Address`         | Administrator address                                  | `get_admin()`, `set_admin()`                                               |
| `UsdcToken`                | Instance      | `Address`         | USDC token contract address                            | Set during `init()`                                                        |
| `Settlement`               | Instance      | `Address`         | Settlement contract; receives USDC on deduct           | `set_settlement()`, `get_settlement()`                                     |
| `RevenuePool`              | Instance      | `Option<Address>` | Revenue pool address (informational)                   | `set_revenue_pool()`, `get_revenue_pool()`                                 |
| `MaxDeduct`                | Instance      | `i128`            | Maximum USDC per single deduct                         | Set during `init()`, read by `deduct()` / `batch_deduct()`                 |
| `Paused`                   | Instance      | `bool`            | Circuit-breaker flag                                   | `pause()`, `unpause()`, `is_paused()`                                      |
| `Metadata(String)`         | Instance      | `String`          | Per-offering metadata (IPFS CID / URI)                 | `set_metadata()`, `get_metadata()`, `update_metadata()`                    |
| `Price(String)`            | Instance      | `String`          | Per-offering price string for the offering ID          | `set_price()`, `get_price()`, `remove_price()`                             |
| `OfferingIndex`            | Instance      | `Vec<String>`     | Ordered list of offering IDs with stored prices        | `set_price()`, `remove_price()`, `list_prices()`                           |
| `PendingOwner`             | Instance      | `Address`         | Two-step ownership transfer nominee                    | `transfer_ownership()`, `accept_ownership()`                               |
| `PendingAdmin`             | Instance      | `Address`         | Two-step admin transfer nominee                        | `set_admin()`, `accept_admin()`                                            |
| `DepositorList`            | Instance      | `Vec<Address>`    | Allowed depositor addresses                            | `set_allowed_depositor()`, `get_allowed_depositors()`                      |
| `ContractVersion`          | Instance      | `BytesN<32>`      | WASM hash set by `upgrade()`                           | `upgrade()`, `version()`                                                   |
| `ProcessedRequest(Symbol)` | **Temporary** | `bool`            | Idempotency marker for a processed deduct `request_id` | Written by `deduct()` / `batch_deduct()`; read by `is_request_processed()` |

## Data Structures

### VaultMeta

```rust
#[contracttype]
#[derive(Clone)]
pub struct VaultMeta {
    pub owner: Address,                    // Vault owner; always permitted to deposit
    pub balance: i128,                     // Current vault balance (USDC units)
    pub authorized_caller: Option<Address>, // Optional address authorized to call deduct/batch_deduct
    pub min_deposit: i128,                 // Minimum amount required per deposit
}
```

**Fields:**

- `owner`: `Address` - The vault owner; immutable except via `transfer_ownership()`; always permitted to deposit; can set allowed depositors and manage metadata
- `balance`: `i128` - Current vault balance in smallest USDC units; incremented by deposits, decremented by deducts/withdrawals
- `authorized_caller`: `Option<Address>` - Optional address permitted to trigger `deduct()` and `batch_deduct()` operations; can be set via `set_authorized_caller()`
- `min_deposit`: `i128` - Minimum required per deposit; configured at initialization; prevents dust deposits

### DeductItem

```rust
#[contracttype]
#[derive(Clone)]
pub struct DeductItem {
    pub amount: i128,
    pub request_id: Option<Symbol>,
}
```

Used in `batch_deduct()` to represent individual deduction requests.

## Storage Operations

### Initialization

**Function:** `init()`

Sets up the vault with initial state:

- `StorageKey::Meta` ← `VaultMeta { owner, balance: initial_balance, authorized_caller, min_deposit }`
- `StorageKey::UsdcToken` ← USDC token address
- `StorageKey::Admin` ← owner address (initially)
- `StorageKey::RevenuePool` ← optional revenue pool address
- `StorageKey::MaxDeduct` ← max deduct cap (or `DEFAULT_MAX_DEDUCT` if not specified)

### Core Vault Operations

| Operation                       | Reads                                                          | Writes                                                                         | Authorization              |
| ------------------------------- | -------------------------------------------------------------- | ------------------------------------------------------------------------------ | -------------------------- |
| `deposit(amount)`               | MetaKey, DepositorList                                         | MetaKey (balance += amount)                                                    | Owner or AllowedDepositor  |
| `deduct(amount, request_id)`    | MetaKey, MaxDeduct, Settlement, ProcessedRequest(id)?          | MetaKey (balance -= amount); ProcessedRequest(id) if Some; transfers USDC      | Owner or authorized_caller |
| `batch_deduct(items)`           | MetaKey, MaxDeduct, Settlement, ProcessedRequest(id)? per item | MetaKey (balance -= total); ProcessedRequest(id) per Some item; transfers USDC | Owner or authorized_caller |
| `withdraw(amount)`              | MetaKey, UsdcToken                                             | MetaKey (balance -= amount); transfers USDC to owner                           | Owner only                 |
| `withdraw_to(to, amount)`       | MetaKey, UsdcToken                                             | MetaKey (balance -= amount); transfers USDC to `to`                            | Owner only                 |
| `balance()`                     | MetaKey                                                        | —                                                                              | Public read                |
| `transfer_ownership(new_owner)` | MetaKey                                                        | PendingOwner                                                                   | Owner only                 |

### Admin Operations

| Operation                | Reads            | Writes                                      | Authorization |
| ------------------------ | ---------------- | ------------------------------------------- | ------------- |
| `distribute(to, amount)` | Admin, UsdcToken | — (USDC transfer only, no balance tracking) | Admin only    |
| `set_admin(new_admin)`   | Admin            | Admin                                       | Admin only    |

### Access Control Operations

| Operation                          | Reads                   | Writes                               | Authorization |
| ---------------------------------- | ----------------------- | ------------------------------------ | ------------- |
| `set_allowed_depositor(depositor)` | AllowedDepositors       | AllowedDepositors (append or remove) | Owner only    |
| `set_authorized_caller(caller)`    | Meta                    | Meta (authorized_caller field)       | Owner only    |
| `is_authorized_depositor(caller)`  | Meta, AllowedDepositors | —                                    | Public read   |

### Settlement & Routing

| Operation                            | Reads       | Writes      | Authorization                        |
| ------------------------------------ | ----------- | ----------- | ------------------------------------ |
| `set_settlement(settlement_address)` | Admin       | Settlement  | Admin only                           |
| `get_settlement()`                   | Settlement  | —           | Public read (view-only, no mutation) |
| `set_revenue_pool(revenue_pool)`     | Admin       | RevenuePool | Admin only                           |
| `get_revenue_pool()`                 | RevenuePool | —           | Public read (view-only, no mutation) |

**Deduct Routing Logic:**

1. If `StorageKey::Settlement` is set: transfer USDC to settlement
2. Else if `StorageKey::RevenuePool` is set: transfer USDC to revenue pool
3. Else: USDC remains in vault

**View Function Safety:**

- Both `get_settlement()` and `get_revenue_pool()` are read-only operations
- They return only final committed state, never intermediate or pending values
- Safe for external indexers and off-chain queries
- Deterministic: identical state inputs always produce identical outputs
- `get_settlement()` panics if not configured; `get_revenue_pool()` returns `None` gracefully

### Metadata Operations

| Operation                                | Reads                       | Writes                | Authorization |
| ---------------------------------------- | --------------------------- | --------------------- | ------------- |
| `set_metadata(offering_id, metadata)`    | Meta                        | Metadata(offering_id) | Owner only    |
| `get_metadata(offering_id)`              | Metadata(offering_id)       | —                     | Public read   |
| `update_metadata(offering_id, metadata)` | Meta, Metadata(offering_id) | Metadata(offering_id) | Owner only    |

**Metadata Notes:**

- Metadata is stored per offering (keyed by `offering_id`)
- Typical usage: store IPFS CID or HTTPS URI for offering details
- Maximum string length: no hard limit enforced, but should be kept reasonable
- Empty strings are allowed

### Read Operations

```
Instance Storage
├── StorageKey::Meta
│   └── VaultMeta
│       ├── owner: Address
│       ├── balance: i128
│       ├── authorized_caller: Option<Address>
│       └── min_deposit: i128
├── StorageKey::UsdcToken
│   └── Address
├── StorageKey::Admin
│   └── Address
├── StorageKey::AllowedDepositors (optional)
│   └── Vec<Address>
├── StorageKey::Settlement (optional)
│   └── Address
├── StorageKey::RevenuePool (optional)
│   └── Address
├── StorageKey::MaxDeduct
│   └── i128
└── StorageKey::Metadata(offering_id_1..N) (optional, multiple entries)
    └── String
```

## Migration and Upgrade Notes

### Post-Refactor Changes

The following changes were made in the recent refactor:

1. **VaultMeta Structure Expansion**
   - Added `authorized_caller: Option<Address>` field for designated deduct authorization
   - Added `min_deposit: i128` field for deposit minimum enforcement
   - Old deployments must migrate existing `VaultMeta` to include these new fields with appropriate defaults

2. **Storage Key Consolidation**
   - All admin-related keys (Admin, UsdcToken, Settlement, RevenuePool, MaxDeduct) now use the `StorageKey` enum
   - Previously may have used Symbol-based keys
   - Migration: read from old Symbol keys, write to new enum keys

3. **AllowedDepositors Structure Change**
   - Now `Vec<Address>` instead of single optional address
   - Allows multiple authorized depositors
   - Supports add/remove operations without replacing the entire collection

4. **Metadata System**
   - `StorageKey::Metadata(String)` replaces hardcoded offering metadata patterns
   - Enables flexible per-offering metadata storage

### Migration Strategy for Existing Deployments

If upgrading from a pre-refactor version, use the following pattern:

```rust
// 1. Read old VaultMeta (owner, balance only)
let old_meta = env.storage().instance().get(&StorageKey::Meta);

// 2. Create new VaultMeta with migrations
let new_meta = VaultMeta {
    owner: old_meta.owner,
    balance: old_meta.balance,
    authorized_caller: None,  // Set by owner post-upgrade via set_authorized_caller()
    min_deposit: 0,           // Default to 0; can be reset if needed
};

// 3. Write new structure back
env.storage().instance().set(&StorageKey::Meta, &new_meta);

// 4. Migrate other storage keys as needed
// (e.g., from Symbol("usdc") to StorageKey::UsdcToken)
```

## Security Considerations

### Access Control

- **Owner-Only Operations:** `set_allowed_depositor()`, `set_authorized_caller()`, `transfer_ownership()`, `withdraw()`, `withdraw_to()`, metadata operations
- **Admin-Only Operations:** `distribute()`, `set_admin()`, `set_settlement()`, `set_revenue_pool()`
- **Public Operations:** `balance()`, `get_meta()`, `get_metadata()`, `is_authorized_depositor()`, `get_settlement()`, `get_revenue_pool()` (all read-only)
- **Depositor Operations:** `deposit()` (owner or allowed depositor); `deduct()` and `batch_deduct()` (owner or authorized_caller)

### Data Integrity

- `VaultMeta` is updated atomically; all fields are modified together for consistency
- Balance operations include assertions to prevent underflow and enforce non-negative constraints
- Storage writes are transactional within Soroban; partial writes are not possible
- Authorization is validated before any state mutations

### Deduct Safety

- Single deduct amount capped by `StorageKey::MaxDeduct` to prevent excessive USDC transfers
- Batch deduct validates all items before applying any deductions (all-or-nothing semantics)
- Balance underflow prevention: all attempted deductions are validated before modifying state

## Testing

### Storage Access Patterns

The test suite validates:

- Initialization sets all required storage keys
- Deposit updates balance correctly
- Deduct routes to settlement/revenue pool as configured
- Batch operations update balance atomically
- Metadata operations (set, get, update) work correctly
- AllowedDepositors Vec operations (add, remove)
- Access control is enforced for owner-only and admin-only operations

### Recommended Additional Tests

- Metadata size limits and edge cases
- Settlement vs. RevenuePool routing priority
- Authorized caller deduction scenarios
- Balance overflow/underflow edge cases (max i128, min i128)
- Storage upgrade/downgrade compatibility
- Gas usage benchmarks for storage operations

## Monitoring and Debugging

### Storage Inspection

Use Soroban CLI to inspect storage:

```bash
soroban contract storage \
  --contract-id <CONTRACT_ID> \
  --key "meta" \
  --output json
```

### Event Monitoring

Monitor storage-related events:

- `init` events for vault creation
- Future events could track significant balance changes

## Version History

| Version | Change                                                                                                                                                                                                                                                                        |
| ------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1.0     | Initial `StorageKey` enum with `Meta`, `AllowedDepositors`, `Admin`, `UsdcToken`, `Settlement`, `RevenuePool`, `MaxDeduct`, `Metadata(String)`                                                                                                                                |
| 1.1     | Renamed `StorageKey` → `DataKey`; added doc comments to all variants; removed stale `// Replaced by StorageKey enum variants` comment; updated STORAGE.md                                                                                                                     |
| 1.2     | Added `StorageKey::ProcessedRequest(Symbol)` in **temporary storage** for `request_id` idempotency in `deduct` and `batch_deduct`. Added `VaultError::DuplicateRequestId` (code 28). Added `is_request_processed(request_id)` view. TTL: threshold ~7 days, bump to ~30 days. |

## Canonical Storage Keys

All storage is accessed via `StorageKey` enum.

### Keys

| Key                        | Storage Tier  | Description                                                     |
| -------------------------- | ------------- | --------------------------------------------------------------- |
| `MetaKey`                  | Instance      | Vault metadata (owner, balance, authorized_caller, min_deposit) |
| `DepositorList`            | Instance      | Authorized depositors                                           |
| `Admin`                    | Instance      | Admin address                                                   |
| `UsdcToken`                | Instance      | Token contract                                                  |
| `Settlement`               | Instance      | Settlement contract                                             |
| `RevenuePool`              | Instance      | Revenue pool                                                    |
| `MaxDeduct`                | Instance      | Deduct cap                                                      |
| `Paused`                   | Instance      | Circuit breaker                                                 |
| `Metadata(String)`         | Instance      | Offering metadata                                               |
| `PendingOwner`             | Instance      | Ownership transfer nominee                                      |
| `PendingAdmin`             | Instance      | Admin transfer nominee                                          |
| `ContractVersion`          | Instance      | WASM hash (set by `upgrade()`)                                  |
| `ProcessedRequest(Symbol)` | **Temporary** | Idempotency marker; auto-expires after ~30 days                 |

### Migration

- Removes deprecated `AllowedDepositors`
- Ensures Admin fallback from Meta.owner
- `ProcessedRequest` uses temporary storage — no manual cleanup required; markers expire automatically
