# Vault Storage Layout

This document describes the storage layout of the Callora Vault contract, including all `DataKey` variants, their value types, and upgrade implications.

## Storage Overview

All contract state is stored in Soroban **instance storage** under a single unified `DataKey` enum. Using a typed enum prevents key collisions, enables exhaustive auditing, and matches the idiomatic Soroban storage pattern used across this workspace.

## DataKey Enum

```rust
#[contracttype]
pub enum DataKey {
    Meta,
    AllowedDepositors,
    Admin,
    UsdcToken,
    Settlement,
    RevenuePool,
    MaxDeduct,
    Metadata(String),
}
```

## Storage Key Reference

| Variant | Value Type | Description | Set by | Required |
|---------|-----------|-------------|--------|----------|
| `DataKey::Meta` | `VaultMeta` | Primary vault state: owner, balance, authorized_caller, min_deposit | `init`, `deposit`, `deduct`, `batch_deduct`, `withdraw`, `withdraw_to`, `transfer_ownership`, `set_authorized_caller` | Yes |
| `DataKey::AllowedDepositors` | `Vec<Address>` | Ordered list of addresses permitted to deposit. Absent when no depositors are set. | `set_allowed_depositor` | No |
| `DataKey::Admin` | `Address` | Administrative address for privileged operations (distribute, set_settlement). Set to owner at init. | `init`, `set_admin` | Yes |
| `DataKey::UsdcToken` | `Address` | USDC token contract address used for all token transfers. | `init` | Yes |
| `DataKey::Settlement` | `Address` | Optional settlement contract address. When present, deduct transfers USDC here (takes priority over RevenuePool). | `set_settlement` | No |
| `DataKey::RevenuePool` | `Address` | Optional revenue pool address. Receives USDC on deduct when Settlement is not set. | `init` | No |
| `DataKey::MaxDeduct` | `i128` | Maximum amount per single deduct call. Defaults to `i128::MAX` (no cap) if not set at init. | `init` | Yes (defaults) |
| `DataKey::Metadata(String)` | `String` | Per-offering metadata reference (IPFS CID or HTTPS URI). Keyed by `offering_id`. | `set_metadata`, `update_metadata` | No |

## Data Structures

### VaultMeta

```rust
#[contracttype]
#[derive(Clone)]
pub struct VaultMeta {
    pub owner: Address,       // Vault owner; always permitted to deposit
    pub balance: i128,        // Current tracked balance in USDC micro-units
    pub authorized_caller: Option<Address>, // Optional address permitted to call deduct
    pub min_deposit: i128,    // Minimum deposit amount (0 = no minimum)
}
```

## Storage Operations

### Write Operations

| Function | Keys Written |
|----------|-------------|
| `init` | `Meta`, `UsdcToken`, `Admin`, optionally `RevenuePool`, `MaxDeduct` |
| `deposit` | `Meta` (balance update) |
| `deduct` | `Meta` (balance update) |
| `batch_deduct` | `Meta` (balance update) |
| `withdraw` | `Meta` (balance update) |
| `withdraw_to` | `Meta` (balance update) |
| `transfer_ownership` | `Meta` (owner update) |
| `set_authorized_caller` | `Meta` (authorized_caller update) |
| `set_admin` | `Admin` |
| `set_settlement` | `Settlement` |
| `set_allowed_depositor(Some(addr))` | `AllowedDepositors` |
| `set_allowed_depositor(None)` | removes `AllowedDepositors` |
| `set_metadata` | `Metadata(offering_id)` |
| `update_metadata` | `Metadata(offering_id)` |

### Read Operations

| Function | Keys Read |
|----------|----------|
| `get_meta` | `Meta` |
| `balance` | `Meta` |
| `get_admin` | `Admin` |
| `get_settlement` | `Settlement` |
| `get_max_deduct` | `MaxDeduct` |
| `get_metadata` | `Metadata(offering_id)` |
| `is_authorized_depositor` | `Meta`, `AllowedDepositors` |
| `distribute` | `Admin`, `UsdcToken` |
| `deduct` / `batch_deduct` | `Meta`, `MaxDeduct`, `Settlement`, `RevenuePool`, `UsdcToken` |

## Deduct Transfer Priority

When a deduct or batch_deduct is executed, USDC is transferred according to this priority:

1. If `DataKey::Settlement` is set → transfer to settlement contract
2. Else if `DataKey::RevenuePool` is set → transfer to revenue pool
3. Else → USDC remains in the vault contract

## AllowedDepositors: Vec vs Map

The allowed depositors list uses `Vec<Address>` (not `Map`) intentionally:

- **Stable ordering**: Vec preserves insertion order; iteration is deterministic
- **Deduplication**: `set_allowed_depositor` checks for duplicates before appending
- **Clear semantics**: `set_allowed_depositor(None)` removes the entire key
- **O(n) lookup**: Acceptable for small depositor lists (typically 1–5 addresses)

## Upgrade Implications

### Adding Fields to VaultMeta

Create a `VaultMetaV2` struct and migrate during a one-time upgrade call:

```rust
// Read old, transform, write new
let old: VaultMeta = inst.get(&DataKey::Meta).unwrap();
let new = VaultMetaV2 { ..old, new_field: default_value };
inst.set(&DataKey::Meta, &new);
```

### Adding New DataKey Variants

Adding new variants to `DataKey` is non-breaking. Existing stored values are unaffected.

### Renaming DataKey Variants

Renaming a variant changes its on-chain discriminant. A migration function must:
1. Read from the old key
2. Write to the new key
3. Remove the old key

## Security Considerations

- All storage writes are gated behind authorization checks in contract functions
- `DataKey::Admin` and `DataKey::Meta.owner` are separate roles — admin controls distribution and settlement; owner controls deposits and withdrawals
- Balance operations use `assert!` to prevent underflow
- No direct external storage access is possible in Soroban

## Version History

| Version | Change |
|---------|--------|
| 1.0 | Initial `StorageKey` enum with `Meta`, `AllowedDepositors`, `Admin`, `UsdcToken`, `Settlement`, `RevenuePool`, `MaxDeduct`, `Metadata(String)` |
| 1.1 | Renamed `StorageKey` → `DataKey`; added doc comments to all variants; removed stale `// Replaced by StorageKey enum variants` comment; updated STORAGE.md |
