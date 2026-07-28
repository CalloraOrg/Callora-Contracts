# Storage Tier Architecture

This document explains the storage keys and their designated tiers (Instance, Persistent, Temporary) within the `storage` contract, along with the rationale for each choice.

## Storage Keys

The `storage` contract uses three storage tiers as provided by the Soroban SDK. The choice of tier directly affects the state retention, gas costs, and scalability of the contract.

### 1. `StorageKey::Admin`
- **Tier:** **Instance**
- **Rationale:** 
  The admin address is a singular, globally shared configuration parameter. Instance storage is ideal here because the state is small and identical for all users interacting with the contract instance. By storing it as an instance key, any interaction that bumps the instance TTL automatically keeps the admin configuration alive, preventing silent archiving of critical settings.

### 2. `StorageKey::UserBalance(Address)`
- **Tier:** **Persistent**
- **Rationale:** 
  User balances represent critical, high-value financial state that must not be silently archived without an explicit bumping strategy. Persistent storage is chosen because it allows the contract to scale to an unbounded number of users without hitting the storage caps of instance storage. Each user's balance is independently managed, and interactions specific to a user bump their persistent TTL.

### 3. `StorageKey::RequestMarker(u64)`
- **Tier:** **Temporary**
- **Rationale:** 
  Request markers are utilized strictly for deduplication to enforce at-most-once semantics. Since deduplication is typically only required for a short operational window (e.g., during a pending transaction retry period), temporary storage is the perfect fit. Temporary entries cost significantly less gas and are allowed to archive cheaply without bloating the ledger permanently.

## Security and Compliance
- **Auth Checks:** All state-modifying entrypoints enforce authorization via `require_auth()` for their respective identities.
- **Safety Limits:** Integer operations are performed using checked arithmetic (e.g., `checked_add`) to prevent overflow, ensuring deterministic and secure execution without panics in production paths.
- **Validation:** Storage writes check for initialization state and duplicate keys before committing to ledger, ensuring state invariants are upheld.
