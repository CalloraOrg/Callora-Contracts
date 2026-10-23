# BatchClaim Contract Storage Documentation

This document outlines the storage keys used by the BatchClaim smart contract, detailing their Soroban storage tiers and the rationale behind their selection in accordance with Soroban best practices.

## Storage Tiers Overview

- **Instance Storage**: Stored in the contract instance ledger entry. Automatically extended when the contract instance is invoked. Ideal for small, frequently accessed configuration data and global contract state.
- Persistent Storage**: Stored as independent ledger entries with their own Time-To-Live (TTL). Ideal for user-specific or data-heavy entries that require explicit TTL management.
- Temporary Storage**: Short-lived entries intended for transient data (e.g., operation locks, nonce tracking).

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

### `DataKey::ClaimIdOwner(BytesN<32>)`
- **Tier**: **Persistent**
- **Data Type**: `Address`
- **Rationale**: Records the address that first reserved a given claim ID. This is the replay-protection backbone from issue #1044: once a `claim_id` is reserved it must not be reusable by a different claimant. The reservation is therefore given the long-lived `CONSUMED_TOMBSTONE_*` TTL budget (approximately six months), not the short persistent budget used for ordinary claim records. See "Reservation TTL semantics" below for what happens when the entry is archived versus expired.

---

## TTL Management Strategy

TTL durations are expressed in ledgers. Stellar ledgers close at a fixed cadence of one every five seconds, so there are `LEGERS_PER_DAY = 17_280` ledgers per day. This constant is the single source of truth for all TTL conversions in this crate (and is matched by `contracts/distribute` and `contracts/settlement`).

| Constant | Ledgers | Days | Purpose |
|----------|--------|------|---------|
| `BUMP_AMOUNT` | `LEGERS_PER_DAY * 60` = 1 |036 800 | 60 | Instance TTL extension on every invocation | 60 | Instance TTL extension on every invocation |
| `LIFETIME_THRESHOLD` | `LEGERS_PER_DAY * 30` = 518 400 | 30 | Instance TTL bump threshold |
| `PERSISTENT_BUMP` | `LEDGERS_PER_DAY * 120` = 2 073 600 | 120 | Persistent TTL extension on claim records |
| `PERSISTENT_THRESHOLD` | `LEDGERS_PER_DAY * 30` = 518 400 | 30 | Persistent TTL bump threshold |
| `CONSUMED_TOMBSTONE_BUMP` | `LEDGERS_PER_DAY * 180` | ~180 | Reservation / consumed-tombstone TTL extension |
| `CONSUMED_TOMBSTONE_THRESHOLD` | `LEDGERS_PER_DAY * 30` | 30 | Reservation / consumed-tombstone TTL bump threshold |

All bump amounts are claimed against the network `max_entry_ttl` (a 30-day maximum on the Stellar mainnet at the time of writing). The contract asserts at compile time that no constant exceeds the documented network limit.

| Key | Tier | TTL Strategy |
|-----|------|-------------|
| `Admin` | Instance | Extended on every entrypoint invocation, including read-only views via `bump_instance` |
| `PendingAdmin` | Instance | Extended on every entrypoint invocation, including read-only views via `bump_instance` |
| `Paused` | Instance | Extended on every entrypoint invocation, including read-only views via `bump_instance` |
| `BatchWindow` | Instance | Extended on every entrypoint invocation, including read-only views via `bump_instance` |
| `ClaimRoot` | Persistent | Bumped on every `set_claim_root` call (`PERSISTENT_THRESHOLD`/`PERSISTENT_BUMP`) |
| `WhitelistMerkleRoot` | Persistent | Bumped on every rotation (`PERSISTENT_THRESHOLD`/`PERSISTENT_BUMP`) |
| `Claimed(Address, BytesN <32>)` | Persistent | Bumped on every claim (`PERSISTENT_THRESHOLD`/`PERSISTENT_BUMP`) |
| `ClaimIdOwner(BytesN<32>)` | Persistent | Bumped on reservation and on every reservation read (`CONSUMED_TOMBSTONE_THRESHOLD`/`CONSUMED_TOMBSTONE_BUMP`) |
| `ClaimConsumed(BytesN <32>)` | Persistent | Bumped on consumption (`CONSUMED_TOMBSTONE_THRESHOLD`/`CONSUMED_TOMBSTONE_BUMP`) |
| `ClaimNonce(Address)` | Temporary | Bumped on every write, moderate TTL (e.g., 7 days) |

---

## Reservation TTL semantics

`ClaimIdOwner(claim_id)` is the replay-protection record for a reserved claim ID. The reservation is documented as "reserved to one claimant forever", but Soroban persistent entries cannot actually live forever — they are extended in fixed increments and eventually expire. The contract therefore gives the reservation the longest budget it uses (`CONSUMED_TOMBSTONE_BUMP", approximately six months) and refreshes it on every reservation read. There are two distinct failure modes to be aware of:

1. **Archived reservation (TTL lagging behind the current ledger, but the entry still exists in the ledger archive).** The entry is restorable. Any subsequent contract invocation that touches the key (e.g., a `claim_id_owner` view or a `batch_claim` attempt) will restore it from the archive and the reservation remains binding on the original owner. The contract bumps the restored entry to the consumed-tombstone TTL on the same invocation, so the reservation is quickly re-heathed.

2. **Expired reservation (TTL reaches zero and the entry is pruned from the ledger archive).** The entry is gone forever. At that point the contract cannot distinguish a pruned reservation from a never-reserved claim ID, so the claim ID becomes reusable by a different claimant. This is the residual risk that the long tombstone TTC is intended to minimize; it is not closed entirely because Soroban entries cannot live forever. To keep the reservation alive, clients should read it (e.g., via `claim_id_owner`) at least once per `CONSUMED_TOMBSTONE_THRESHOLD` window (about 30 days).

---

## Design Rationale

The tier selection balances three constraints:

1. **Correctness** — Claimed status and claim roots *must* survive until the claim window closes; using Temporary would risk premature eviction and double-claim vulnerabilities. Hence Instance and Persistent. The long tombstone TTC on `ClaimIdOwner` and `ClaimConsumed` is part of this correctness argument.
2. **Cost efficiency** — Transient data like per-address nonces that are consumed and rotated use Temporary storage to reduce state fees. The extra `bump_instance` calls on read-only views are deliberately cheap (a single instance TTL extension with no additional storage writes) and are necessary to keep the admin, claim records and reservations alive during periods of inactivity.
3. **Access frequency** — Admin config and the pause flag are read on nearly every entrypoint, so Instance storage minimizes read latency and avoids TTL management overhead.
