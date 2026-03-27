# Vault Storage Layout

This document describes the storage layout of the Callora Vault contract, updated to the idiomatic `StorageKey` enum pattern.

## Storage Overview

The Callora Vault contract uses Soroban's instance storage to persist contract state. All data is stored under exactly four typed keys defined by the `StorageKey` enum.

| Storage Key | Data Type | Storage Mode | Description |
|-------------|------------|--------------|-------------|
| `Config` | `VaultConfig` | Instance | Static configuration (owner, admin, tokens, limits) |
| `State` | `VaultState` | Instance | Dynamic/mutable state (balance) |
| `AllowedDepositors` | `Vec<Address>` | Instance | Optional list of approved backend depositors |
| `Metadata(String)` | `String` | Instance | Per-offering CID/URI metadata |

## Data Structures

### VaultConfig

Consolidates all primary configuration to minimize storage slots and simplify auditing.

```rust
#[contracttype]
pub struct VaultConfig {
    pub owner: Address,
    pub admin: Address,
    pub usdc_token: Address,
    pub revenue_pool: Option<Address>,
    pub settlement: Option<Address>,
    pub min_deposit: i128,
    pub max_deduct: i128,
    pub authorized_caller: Option<Address>,
}
```

### VaultState

Separated from configuration to minimize gas costs on frequent balance updates.

```rust
#[contracttype]
pub struct VaultState {
    pub balance: i128,
}
```

## Migration Awareness

The contract includes legacy migration logic in its internal getters (`get_config`, `get_state`). If old keys like `Symbol("meta")`, `Symbol("Admin")`, etc., are found, the contract dynamically maps them to the new structures. This ensures that existing vault data remains accessible even after the storage refactor.

## Operations

- **Update Config**: Operations like `set_admin` or `set_settlement` read the entire `VaultConfig` and write it back.
- **Update Balance**: `deposit` and `deduct` only read/write the `VaultState` slot.
- **Access Control**: `is_authorized_depositor` reads both `VaultConfig` (for owner) and `AllowedDepositors` (for the whitelist).

---

**Note**: This layout is optimized for single-tenant vaults with a managed list of depositors.
