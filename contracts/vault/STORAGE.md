# Vault Storage Layout

This document describes the storage layout of the Callora Vault contract, including storage keys, data types, and access control implications.

## Storage Overview

The Callora Vault contract uses Soroban's instance storage to persist contract state. Data is organized using the `StorageKey` enum, providing type-safe access to contract state.

## Storage Keys

The contract defines the following storage keys:

```rust
#[contracttype]
pub enum StorageKey {
    Meta,                          // VaultMeta
    AllowedDepositors,             // Vec<Address>
    Admin,                         // Address
    UsdcToken,                     // Address
    Settlement,                    // Option<Address>
    RevenuePool,                   // Option<Address>
    MaxDeduct,                     // i128
    Metadata(String),              // String (offering metadata by offering_id)
}
```

### Storage Keys Table

| Key Variant | Value Type | Description | Usage | Access |
|-------------|-----------|-------------|-------|--------|
| `Meta` | `VaultMeta` | Primary vault metadata including owner, balance, authorized_caller, and min_deposit | Core vault state | `get_meta()`, updated by deposit/deduct/withdraw operations |
| `AllowedDepositors` | `Vec<Address>` | List of addresses allowed to deposit into the vault | Access control for deposits | `set_allowed_depositor()`, readable via `is_authorized_depositor()` |
| `Admin` | `Address` | Administrator address authorized to call `distribute()` and `set_admin()` | Access control for distributions | `get_admin()`, `set_admin()` (admin-only) |
| `UsdcToken` | `Address` | USDC token contract address | Token transfers for deposits, deducts, distributions | Set during `init()`, used by token operations |
| `Settlement` | `Option<Address>` | Settlement contract address; receives USDC on deduct operations | Deduct routing (priority over RevenuePool) | `set_settlement()`, `get_settlement()` (admin-only) |
| `RevenuePool` | `Option<Address>` | Revenue pool contract address; receives USDC on deduct if Settlement is not set | Deduct routing (fallback) | Set during `init()`, used if Settlement not configured |
| `MaxDeduct` | `i128` | Maximum USDC amount per single deduct operation | Deduct limit enforcement | Set during `init()`, read by `deduct()` and `batch_deduct()` |
| `Metadata(offering_id)` | `String` | Off-chain metadata reference (IPFS CID or URI) for a specific offering | Offering metadata | `set_metadata()`, `get_metadata()`, `update_metadata()` (owner-only) |

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

| Operation | Reads | Writes | Authorization |
|-----------|-------|--------|-----------------|
| `deposit(amount)` | Meta, AllowedDepositors | Meta (balance += amount) | Owner or AllowedDepositor |
| `deduct(amount, request_id)` | Meta, MaxDeduct, Settlement/RevenuePool | Meta (balance -= amount); transfers USDC | Owner or authorized_caller |
| `batch_deduct(items)` | Meta, MaxDeduct, Settlement/RevenuePool | Meta (balance -= total); transfers USDC | Owner or authorized_caller |
| `withdraw(amount)` | Meta, UsdcToken | Meta (balance -= amount); transfers USDC to owner | Owner only |
| `withdraw_to(to, amount)` | Meta, UsdcToken | Meta (balance -= amount); transfers USDC to `to` | Owner only |
| `balance()` | Meta | — | Public read |
| `transfer_ownership(new_owner)` | Meta | Meta (owner = new_owner) | Owner only |

### Admin Operations

| Operation | Reads | Writes | Authorization |
|-----------|-------|--------|-----------------|
| `distribute(to, amount)` | Admin, UsdcToken | — (USDC transfer only, no balance tracking) | Admin only |
| `set_admin(new_admin)` | Admin | Admin | Admin only |

### Access Control Operations

| Operation | Reads | Writes | Authorization |
|-----------|-------|--------|-----------------|
| `set_allowed_depositor(depositor)` | AllowedDepositors | AllowedDepositors (append or remove) | Owner only |
| `set_authorized_caller(caller)` | Meta | Meta (authorized_caller field) | Owner only |
| `is_authorized_depositor(caller)` | Meta, AllowedDepositors | — | Public read |

### Settlement & Routing

| Operation | Reads | Writes | Authorization |
|-----------|-------|--------|-----------------|
| `set_settlement(settlement_address)` | Admin | Settlement | Admin only |
| `get_settlement()` | Settlement | — | Public read |

**Deduct Routing Logic:**
1. If `StorageKey::Settlement` is set: transfer USDC to settlement
2. Else if `StorageKey::RevenuePool` is set: transfer USDC to revenue pool
3. Else: USDC remains in vault

### Metadata Operations

| Operation | Reads | Writes | Authorization |
|-----------|-------|--------|-----------------|
| `set_metadata(offering_id, metadata)` | Meta | Metadata(offering_id) | Owner only |
| `get_metadata(offering_id)` | Metadata(offering_id) | — | Public read |
| `update_metadata(offering_id, metadata)` | Meta, Metadata(offering_id) | Metadata(offering_id) | Owner only |

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
- **Admin-Only Operations:** `distribute()`, `set_admin()`, `set_settlement()`
- **Public Operations:** `balance()`, `get_meta()`, `get_metadata()`, `is_authorized_depositor()`, `get_settlement()` (read-only)
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

| Version | Change |
|---------|--------|
| 1.0 | Initial `StorageKey` enum with `Meta`, `AllowedDepositors`, `Admin`, `UsdcToken`, `Settlement`, `RevenuePool`, `MaxDeduct`, `Metadata(String)` |
| 1.1 | Renamed `StorageKey` → `DataKey`; added doc comments to all variants; removed stale `// Replaced by StorageKey enum variants` comment; updated STORAGE.md |
