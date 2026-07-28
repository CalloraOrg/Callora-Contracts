# Cold Contract Storage Documentation

This document outlines the foreseeable storage keys for the Cold contract, their storage tiers (Instance, Persistent, or Temporary), and the rationale behind each choice in accordance with Soroban best practices. The Cold contract is currently a read-only capability facade; the keys below represent the anticipated storage layout once the full cold-storage feature set is implemented.

## Storage Tiers Overview

- **Instance Storage**: Stored in the contract instance ledger entry. Automatically extended when the contract instance is invoked. Ideal for small, frequently accessed configuration data and global contract state.
- **Persistent Storage**: Stored as independent ledger entries with their own Time-To-Live (TTL). Ideal for user-specific or data-heavy entries that require explicit TTL management.
- **Temporary Storage**: Short-lived entries intended for transient data (e.g., reentrancy guards, short-lived scratchpads).

---

## Storage Keys

### 1. `Admin`
- **Tier**: **Instance**
- **Data Type**: `Address`
- **Rationale**: The administrator address controls access to admin-restricted actions such as `set_config` and signer rotation. It is a core global configuration parameter shared across nearly all state-changing entrypoints and must be available efficiently on every invocation.

### 2. `PendingAdmin`
- **Tier**: **Instance**
- **Data Type**: `Address` (Optional / Nullable)
- **Rationale**: Used during a secure two-step admin transfer process to hold the nominated successor before they accept the role. As a singleton configuration state tied to the contract's administration lifecycle, instance storage is appropriate.

### 3. `ColdConfig`
- **Tier**: **Instance**
- **Data Type**: `ColdConfig` (hot_bps, rebalance_threshold_bps, cold_signers, cold_threshold)
- **Rationale**: The cold-storage configuration governs the hot/cold balance split, rebalance drift tolerance, and multisig cold-sweep parameters. It is a singleton global configuration referenced on every deposit, rebalance, and sweep operation, making instance storage the natural choice.

### 4. `ColdBalances`
- **Tier**: **Instance**
- **Data Type**: `ColdBalances` (hot: i128, cold: i128)
- **Rationale**: The hot/cold balance accounting record tracks the current split of the vault's total balance. This is a singleton value mutated on every deposit (rebalance) and cold-sweep execution, and read on nearly every interaction. Instance storage ensures low-latency access and ties the balance's lifetime to the contract instance.

### 5. `PendingColdSweep`
- **Tier**: **Temporary**
- **Data Type**: `PendingColdSweep` (amount, destination, approvals, proposed_at)
- **Rationale**: A pending multisig cold-sweep request is inherently transient — it exists only during the propose/approve workflow and is consumed once executed (or expires). Temporary storage is appropriate because these entries have a natural TTL and do not need to persist beyond the sweep lifecycle. Only one sweep may be pending at a time.
