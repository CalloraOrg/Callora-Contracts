# BatchClaim Contract Storage Documentation

This document outlines the storage keys used by the BatchClaim smart contract, detailing their Soroban storage tiers and the rationale behind their selection in accordance with Soroban best practices.

## Storage Tiers Overview

- **Instance Storage**: Stored in the contract instance ledger entry. Automatically extended when the contract instance is invoked. Ideal for small, frequently accessed configuration data and global contract state.
- **Persistent Storage**: Stored as independent ledger entries with their own Time-To-Live (TTL). Ideal for user-specific or data-heavy entries that require explicit TTL management.
- **Temporary Storage**: Short-lived entries intended for transient data (e.g., operation locks, nonce tracking).

---

## Storage Keys

### `DataKey::Admin`
- **Tier**: **Instance**
- **Data Type**: `Address`
- **Rationale**: The administrator address is a core global configuration parameter required for all admin-restricted actions (pause, upgrade, configuration). It must be accessible efficiently across all contract invocations and shares the lifecycle of the contract instance. Using Instance storage guarantees the admin address is always available and cannot be pruned independently of the contract.

### `DataKey::PendingAdmin`
- **Tier**: **Instance**
- **Data Type**: `Address` (Optional)
- **Rationale**: Used during a secure two-step admin transfer process to hold the nominated successor before they accept the role. Since this is a singleton configuration state tied directly to the contract's administration lifecycle, Instance storage is appropriate. The key is removed from storage once the transfer completes.

### `DataKey::Paused`
- **Tier**: **Instance**
- **Data Type**: `bool`
- **Rationale**: The circuit-breaker flag guards all batch-claim operations. As a global boolean read on every guarded entrypoint, Instance storage ensures low latency and that the flag survives as long as the contract instance itself. This prevents a paused flag from being evicted while the contract is live.

### `DataKey::ClaimNonce(Address)`
- **Tier**: **Temporary**
- **Data Type**: `u64`
- **Rationale**: A per-address monotonic claim nonce that prevents replay attacks on signed claims. Each nonce is consumed once and then incremented. Temporary storage is chosen because nonces only need to remain available long enough to detect reuse; old nonces can safely be evicted by the network once their TTL expires, reducing state bloat. The contract bumps the TTL on each write to keep the current nonce live.

### `DataKey::ClaimRoot`
- **Tier**: **Persistent**
- **Data Type**: `BytesN<32>` (Merkle root hash)
- **Rationale**: The Merkle root represents the canonical set of all currently-valid claims. It is updated infrequently (on admin rotation) but must remain available for the entire validation lifecycle of in-flight claims. Persistent storage ensures the root cannot be dropped without explicit admin action, maintaining the integrity of the claim verification system.

### `DataKey::Claimed(Address, BytesN<32>)`
- **Tier**: **Persistent**
- **Data Type**: `bool`
- **Rationale**: Tracks whether a specific leaf (account + claim ID) has already been redeemed. This prevents double-claiming and must survive as long as the claim window is open. Persistent storage guarantees that the claimed status cannot be arbitrarily evicted, which would otherwise allow replayed claims to succeed.

### `DataKey::BatchWindow`
- **Tier**: **Instance**
- **Data Type**: `u64` (ledger timestamp)
- **Rationale**: Defines the time window during which a batch of claims may be submitted. As a global configuration parameter checked on every batch operation, Instance storage provides fast access and lifecycle alignment with the contract instance.

### `DataKey::WhitelistMerkleRoot`
- **Tier**: **Persistent**
- **Data Type**: `BytesN<32>` (Merkle root hash)
- **Rationale**: An optional Merkle root constraining which accounts may participate in a given batch claim round. Updated per round by the admin. Persistent storage is required because the whitelist root must remain available for the duration of the claim round, independent of contract instance activity.

---

## TTL Management Strategy

| Key | Tier | TTL Strategy |
|-----|------|-------------|
| `Admin` | Instance | Managed by contract instance lifecycle |
| `PendingAdmin` | Instance | Managed by contract instance lifecycle |
| `Paused` | Instance | Managed by contract instance lifecycle |
| `BatchWindow` | Instance | Managed by contract instance lifecycle |
| `ClaimRoot` | Persistent | Bumped on every `set_claim_root` call |
| `WhitelistMerkleRoot` | Persistent | Bumped on every rotation |
| `Claimed(Address, BytesN<32>)` | Persistent | Bumped on every claim |
| `ClaimNonce(Address)` | Temporary | Bumped on every write, moderate TTL (e.g., 7 days) |

---

## Design Rationale

The tier selection balances three constraints:

1. **Correctness** — Claimed status and claim roots *must* survive until the claim window closes; using Temporary would risk premature eviction and double-claim vulnerabilities. Hence Instance and Persistent.
2. **Cost efficiency** — Transient data like per-address nonces that are consumed and rotated use Temporary storage to reduce state fees.
3. **Access frequency** — Admin config and the pause flag are read on nearly every entrypoint, so Instance storage minimizes read latency and avoids TTL management overhead.
