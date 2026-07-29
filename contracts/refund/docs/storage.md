# Refund Contract Storage Documentation

This document outlines the storage keys used by the Refund contract, detailing their Soroban storage tiers and the rationale behind their selection.

## Storage Keys Hierarchy

### `DataKey::Admin`
- **Tier:** **Instance**
- **Rationale:** The administrator address dictates access control for the entire contract. Because it is globally applicable and must survive as long as the contract is active, the `Instance` tier is used. This ensures the admin data shares the same Time-To-Live (TTL) as the contract instance itself, preventing access lockouts.

### `DataKey::PendingAdmin`
- **Tier:** **Instance**
- **Data Type:** `Address` (Optional / Nullable)
- **Rationale:** Used during a secure two-step admin transfer process to hold the nominated successor before they accept the role. Because this is a singleton configuration state tied directly to the contract's administration lifecycle, instance storage is appropriate.

### `DataKey::RefundRequest(Address)`
- **Tier:** **Persistent**
- **Rationale:** This key stores the refund request state for a specific user address, including the amount requested, timestamps, and approval status. Since refund requests must survive across contract invocations and are user-scoped, `Persistent` storage ensures the data is durably stored and cannot be arbitrarily dropped without an explicit archival process.

### `DataKey::ProcessedRefund(Address, u64)`
- **Tier:** **Persistent**
- **Rationale:** Tracks completed refunds keyed by user address and a transaction nonce. This serves as an audit trail and replay-prevention mechanism. `Persistent` storage guarantees high availability for off-chain indexers and monitors.

### `DataKey::RateLimitCounter(Address)`
- **Tier:** **Temporary**
- **Rationale:** Tracks the number of refund requests initiated by a user within a rolling time window for rate-limiting purposes. Since this data becomes irrelevant after the window expires, it uses `Temporary` storage to significantly reduce state bloat and protocol fees.

### `DataKey::Paused`
- **Tier:** **Instance**
- **Data Type:** `bool`
- **Rationale:** Circuit-breaker flag that globally halts refund processing when set. As a singleton contract configuration value, it shares the contract instance's lifecycle and TTL.

## Storage Tiers Overview

- **Instance Storage**: Stored in the contract instance ledger entry. Automatically extended when the contract instance is invoked. Ideal for small, frequently accessed configuration data and global contract state.
- **Persistent Storage**: Stored as independent ledger entries with their own Time-To-Live (TTL). Ideal for user-specific or data-heavy entries that require explicit TTL management.
- **Temporary Storage**: Short-lived entries intended for transient data (e.g., rate-limit counters, short-lived scratchpads).
