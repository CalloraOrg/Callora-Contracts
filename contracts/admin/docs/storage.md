# Admin Contract Storage Documentation

This document outlines the storage keys used by the Admin contract, their storage tiers (Instance, Persistent, or Temporary), and the rationale behind each choice in accordance with Soroban best practices.

## Storage Tiers Overview

- **Instance Storage**: Stored in the contract instance ledger entry. Automatically extended when the contract instance is invoked. Ideal for small, frequently accessed configuration data and global contract state.
- **Persistent Storage**: Stored as independent ledger entries with their own Time-To-Live (TTL). Ideal for user-specific or data-heavy entries that require explicit TTL management.
- **Temporary Storage**: Short-lived entries intended for transient data (e.g., reentrancy guards, short-lived scratchpads).

---

## Storage Keys

### 1. `Admin`
- **Tier**: **Instance**
- **Data Type**: `Address`
- **Rationale**: The administrator address is a core global configuration parameter required for nearly all admin-restricted actions. It must be accessible efficiently across contract invocations and shares the lifecycle of the contract instance.

### 2. `PendingAdmin`
- **Tier**: **Instance**
- **Data Type**: `Address` (Optional / Nullable)
- **Rationale**: Used during a secure two-step admin transfer process to hold the nominated successor before they accept the role. Because this is a singleton configuration state tied directly to the contract's administration lifecycle, instance storage is appropriate.
