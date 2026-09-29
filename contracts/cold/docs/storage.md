# Cold Contract Storage Documentation

This document outlines the storage keys used by the Cold storage feature, their storage tiers (Instance, Persistent, or Temporary), and the rationale behind each choice in accordance with Soroban best practices.

Cold accounting is implemented in `contracts/vault/src/cold_storage.rs` as an accounting partition of the vault's tracked balance. The data types defined there serve as the storage schema for the hot/cold split, auto-rebalance, and multisig cold-sweep features.

## Storage Tiers Overview

- **Instance Storage**: Stored in the contract instance ledger entry. Automatically extended when the contract instance is invoked. Ideal for small, frequently accessed configuration data and global contract state.
- **Persistent Storage**: Stored as independent ledger entries with their own Time-To-Live (TTL). Ideal for user-specific or data-heavy entries that require explicit TTL management.
- **Temporary Storage**: Short-lived entries intended for transient data (e.g., reentrancy guards, short-lived scratchpads).

---

## Storage Keys

### 1. `ColdConfig`
- **Tier**: **Instance**
- **Data Type**: `ColdConfig { hot_bps: u32, rebalance_threshold_bps: u32, cold_signers: Vec<Address>, cold_threshold: u32 }`
- **Rationale**: The hot/cold split configuration is a singleton that governs all cold-storage behavior for the vault. It is read on every deposit (to evaluate rebalance) and on every cold-sweep proposal/approval. Because it is a global singleton with a lifetime matching the contract instance, instance storage is appropriate. It shares the contract instance TTL, avoiding the need for separate TTL management.

### 2. `ColdBalances`
- **Tier**: **Instance**
- **Data Type**: `ColdBalances { hot: i128, cold: i128 }`
- **Rationale**: The current hot/cold balance partition is the core accounting state of the cold feature. It is read and written on every deposit (potential rebalance) and on every cold-sweep execution. As a singleton balance record that must remain consistent with the vault's total tracked balance (`VaultMeta.balance`), instance storage ensures it shares the contract's lifecycle and is always available when the contract is invoked.

### 3. `PendingColdSweep`
- **Tier**: **Instance**
- **Data Type**: `PendingColdSweep { amount: i128, destination: Address, approvals: Vec<Address>, proposed_at: u64 }`
- **Rationale**: A pending multisig cold-sweep request. At most one sweep can be pending at a time per vault. It exists from the moment a cold signer proposes a sweep until the threshold is met and the sweep executes (or is cancelled). Instance storage is appropriate because this is a singleton temporary state tied to the vault's lifecycle; the entry is explicitly cleared after execution, so there is no long-term accumulation.
