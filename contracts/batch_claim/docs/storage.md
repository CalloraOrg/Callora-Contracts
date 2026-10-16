# BatchClaim Contract Storage Documentation

This document outlines the storage keys used by the `CalloraBatchClaim` smart contract, detailing their Soroban storage tiers and the rationale behind their selection in accordance with Soroban best practices.

## Storage Tiers Overview

- **Instance Storage**: Stored in the contract instance ledger entry. Automatically extended when the contract instance is invoked. Ideal for small, frequently accessed configuration data and global contract state.
- **Persistent Storage**: Stored as independent ledger entries with their own Time-To-Live (TTL). Ideal for user-specific or data-heavy entries that require explicit TTL management.

---

## Storage Keys

### `StorageKey::Admin`
- **Tier**: **Instance**
- **Data Type**: `Address`
- **Rationale**: The administrator address is a core global configuration parameter required for all admin-restricted actions (`add_claim`, `cancel_claim`, `set_reward_token`).

### `StorageKey::RewardToken`
- **Tier**: **Instance**
- **Data Type**: `Address`
- **Rationale**: Stores the reward token contract address configured at `init_with_token` or `set_reward_token`. Read during `batch_claim` to verify upfront contract balance and transfer each claimant's `pending_amount` after state updates.

### `StorageKey::TotalClaims`
- **Tier**: **Instance**
- **Data Type**: `u32`
- **Rationale**: Tracks the count of active/created claims in instance storage alongside contract configuration.

### `StorageKey::Claim(Address)`
- **Tier**: **Persistent**
- **Data Type**: `ClaimRecord`
- **Rationale**: Per-claimant pending reward record (`claimant`, `pending_amount`, `settled`, `claim_id`). Bumped by `PERSISTENT_BUMP` on every read and write.

### `StorageKey::ClaimConsumed(BytesN<32>)`
- **Tier**: **Persistent**
- **Data Type**: `bool`
- **Rationale**: Long-lived replay-protection tombstone indicating whether a `claim_id` has been settled or cancelled. Bumped by `CONSUMED_TOMBSTONE_BUMP` (~6 months) on every touch and via `extend_claim_consumed_ttl`.

### `StorageKey::ClaimIdOwner(BytesN<32>)`
- **Tier**: **Persistent**
- **Data Type**: `Address`
- **Rationale**: Permanent reservation binding a `claim_id` to the first claimant it was issued to, preventing cross-claimant identifier reuse.

---

## TTL Management Strategy

| Key | Tier | TTL Strategy |
|-----|------|-------------|
| `Admin` | Instance | Extended by `BUMP_AMOUNT` on mutating calls |
| `RewardToken` | Instance | Extended by `BUMP_AMOUNT` on mutating calls |
| `TotalClaims` | Instance | Extended by `BUMP_AMOUNT` on mutating calls |
| `Claim(Address)` | Persistent | Extended by `PERSISTENT_BUMP` on reads and writes |
| `ClaimConsumed(BytesN<32>)` | Persistent | Extended by `CONSUMED_TOMBSTONE_BUMP` on reads, writes, and keep-alive calls |
| `ClaimIdOwner(BytesN<32>)` | Persistent | Extended by `PERSISTENT_BUMP` on reads and writes |
