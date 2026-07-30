# Validators Contract — Storage Keys by Tier

`callora-validators` is a stateless pure-validation library consumed by other
Callora contracts.  It does **not** own a deployed contract instance on its
own.  Nevertheless, the crate defines three storage keys (all in the
**Instance** tier) for the migration and upgrade infrastructure used by any
parent contract that embeds the validators module.

This page documents every storage key, the Soroban tier it lives in, the
reason it was assigned to that tier, the TTL strategy that keeps it alive,
and — for the **Persistent** and **Temporary** tiers — why the crate
intentionally defines no keys there.

---

## Soroban Storage Tier Overview

| Tier       | Location inside the ledger                 | Typical use case                                         | TTL behaviour                                                            |
|------------|--------------------------------------------|----------------------------------------------------------|--------------------------------------------------------------------------|
| Instance   | Embedded in the contract instance entry    | Singleton configuration, small admin / version metadata | Shares the instance entry TTL; `extend_ttl` on one key bumps **all** instance keys together. |
| Persistent | Stand-alone ledger entry per key           | Per-user balances, high-cardinality mappings, BLOBs     | Each key owns its own TTL.  Pruned independently when the TTL expires.  |
| Temporary  | Scratchpad; never written to the ledger    | Intra-txn caches, reentrancy guards, short-lived flags  | **Cleared at ledger close.**  Never survives to the next transaction.   |

---

## Instance Tier

Every storage key used by `callora-validators` resides in the **Instance**
tier (`env.storage().instance()`).  The keys are all singletons — at most
one value per key for the contract's lifetime.

### 1. `Admin`

| Field          | Value                                               |
|----------------|-----------------------------------------------------|
| **Tier**       | Instance                                            |
| **Data type**  | `Address`                                           |
| **Written by** | Embedding contract's initialisation flow            |
| **Read by**    | `require_admin()` → `migrate_v1_to_v2`, `authorize_upgrade` |
| **Cardinality**| 1 per contract (singleton admin)                    |

**Rationale — Instance:**
`Admin` is a singleton configuration value consulted on every admin-gated
entrypoint.  Using Instance storage guarantees (a) it shares the contract
instance entry's TTL bump so the admin key cannot be independently pruned,
and (b) reads are zero-cost compared to a dedicated Persistent ledger entry.
A Persistent-tier admin key could be silently evicted when its separate TTL
expired, bricking every admin-gated function until the key was manually
re-sent.  Temporary is inapplicable because the admin address must survive
across ledger closes.

### 2. `StorageVersion`

| Field          | Value                                               |
|----------------|-----------------------------------------------------|
| **Tier**       | Instance                                            |
| **Data type**  | `u32` (default = `1` when key is absent)            |
| **Written by** | `migrate_v1_to_v2()` (writes `2` after idempotency check) |
| **Read by**    | `storage_version()` (bumps TTL, then reads), `migrate_v1_to_v2`, `authorize_upgrade` |
| **Cardinality**| 1 per contract                                      |

**Rationale — Instance:**
The version marker participates in the same always-hot lifecycle as the admin
key; every migration or upgrade entrypoint reads it first.  Persistent
storage would force deployers to TTL-bump a **second** ledger entry on every
admin operation, with a failure mode where admin was alive but version was
independently evicted (→ V1 default → migrations could re-run accidentally).
Temporary is inappropriate because a version marker must persist across
ledger closes to keep migrations idempotent.

### 3. `AuthorisedUpgrade`

| Field          | Value                                               |
|----------------|-----------------------------------------------------|
| **Tier**       | Instance                                            |
| **Data type**  | `(u32, BytesN<32>)` — `(target_version, wasm_hash)` |
| **Written by** | `authorize_upgrade()`                               |
| **Read by**    | Deployment tool (out-of-band) before WASM upgrade   |
| **Cardinality**| 1 per contract; overwritten on each authorisation   |

**Rationale — Instance:**
`AuthorisedUpgrade` is a short-lived deployment guard written and consumed
within a single deployment cycle.  It piggybacks on the Instance TTL bump
from `storage_version()` and never needs the dedicated ledger-entry
footprint that high-cardinality per-user data would warrant.  Persistent
storage would cost an extra write + separate TTL management for no benefit.
Temporary is inapplicable because the authorisation must survive until the
deployment tool executes the platform upgrade in a later transaction.

---

## Persistent Tier

**No Persistent-tier storage keys are defined in `callora-validators`.**

### What WOULD live in Persistent tier (for reference)

Persistent storage is the right choice when:

- You have **per-user records** (e.g. validator stakes, delegated balances,
  one slot per user-address) — cardinalities of thousands or more.
- The data needs to **survive independently** of whether the contract
  instance entry is touched (e.g. archival records that are only
  infrequently read by admin-only queries).
- You have **bulk BLOBs** or large byte arrays that exceed the size budget
  for the contract instance ledger entry.

### Why the validators crate uses none of this

The crate is a **pure validator library**: it exposes `checked_add_amount`,
`require_capability`, boolean predicates, and the migration/upgrade guards.
It has no notion of user accounts, no stakes, no balances, and no archival
records.  All three existing keys are singletons whose combined size fits
comfortably inside the Instance entry.

> **Audit guard:** If a future PR introduces per-validator or per-user
> state in `callora-validators`, that key MUST be assigned to the
> **Persistent** tier and documented here — adding it as a new `StorageKey`
> enum variant will fail the `storagekey_enum_has_exactly_three_variants`
> test until both the enum and this doc are updated.

---

## Temporary Tier

**No Temporary-tier storage keys are defined in `callora-validators`.**

### What WOULD live in Temporary tier (for reference)

Temporary storage is the right choice when:

- You need a **reentrancy guard** flag (locked during a cross-contract call,
  cleared at ledger-close even if the call panics partway through).
- You need an **intra-transaction cache** — e.g. memoising an expensive
  lookup that will be consulted multiple times in the same call.
- You need **per-call scratch values** that should never be observable to
  the next transaction.

### Why the validators crate uses none of this

All public functions are either (a) pure validators (no storage reads at all
— `checked_add_amount`, `require_capability`) or (b) short admin flows that
never re-enter the contract and perform no cross-contract calls.  There is
no reentrancy surface to guard and no repeated lookups worth caching.

> **Audit guard:** Adding a future cross-contract call to a migration path
> typically warrants a Temporary-tier reentrancy guard; document it here if
> that pattern is introduced.

---

## Full Key Inventory

| #  | Key                  | Tier       | Data type                | Written by              | Read by                   | Cardinality |
|----|----------------------|------------|--------------------------|-------------------------|---------------------------|-------------|
| 1  | `Admin`              | Instance   | `Address`                | init (embedder)         | `require_admin`           | 1           |
| 2  | `StorageVersion`     | Instance   | `u32`                    | `migrate_v1_to_v2`      | `storage_version`         | 1           |
| 3  | `AuthorisedUpgrade`  | Instance   | `(u32, BytesN<32>)`      | `authorize_upgrade`     | deployment tool (out-of-band) | 1       |
| —  | *(no Persistent keys)*  | Persistent | —                       | —                       | —                         | 0           |
| —  | *(no Temporary keys)*   | Temporary  | —                       | —                       | —                         | 0           |

---

## TTL Management Strategy

Soroban Instance-tier storage shares **one TTL across every key in the
contract instance entry**.  The bump in `storage_version()` therefore
protects `Admin`, `StorageVersion`, and `AuthorisedUpgrade` simultaneously.

| Key                 | Bump site                  | Threshold (ledgers) | Amount (ledgers) | Effective window |
|---------------------|----------------------------|---------------------|------------------|------------------|
| `Admin`             | `storage_version()` (auto) | 17 280 × 30         | 17 280 × 60      | ~30 days / ~60 days |
| `StorageVersion`    | `storage_version()` (auto) | 17 280 × 30         | 17 280 × 60      | ~30 days / ~60 days |
| `AuthorisedUpgrade` | `storage_version()` (auto) | 17 280 × 30         | 17 280 × 60      | ~30 days / ~60 days |

- **2:1 bump ratio**: `amount` is double `threshold`.  Any contract that is
  touched through an admin or upgrade path at least once within the
  30-day threshold window will keep all Instance keys alive indefinitely
  without manual TTL upkeep.
- **Idempotent reads**: `storage_version()` does not write anything when
  the key is absent; it safely returns the V1 default.

---

## Design Rationale — Why All-Instance?

Three reasons `callora-validators` deliberately avoids Persistent and
Temporary storage entirely:

1. **Cardinality = singletons.**  All three keys are one-value-per-contract.
   Instance storage exists for exactly this case.  Persistent keys pay an
   extra write cost and require independent TTL management for every key
   with no upside here.

2. **Access frequency.**  Every admin or upgrade path reads all three keys
   in the same transaction.  Instance storage co-locates them, yielding a
   single ledger fetch and a single TTL bump.

3. **Survival dependence.**  `Admin` and `StorageVersion` must never be
   independently evictable.  If `Admin` used Persistent storage and was
   pruned while the instance entry was still alive, every admin-gated
   function would panic with "admin not set" until an admin manually
   re-sent the key.  Instance storage ties their fates together.

---

## Security Notes

1. **Admin must be set before migration.**  Calling `migrate_v1_to_v2` or
   `authorize_upgrade` before the embedding contract writes `StorageKey::Admin`
   panics with `"require_admin: admin not set"`.  This is the expected
   initialisation ordering: init admin → then migrate.

2. **`StorageVersion` default semantics.**  Absence of the key is
   semantically identical to `STORAGE_VERSION_V1`.  Code paths must not
   treat "absent" differently from "= 1".  The production function
   `storage_version()` enforces this via `.unwrap_or(STORAGE_VERSION_V1)`.

3. **`AuthorisedUpgrade` is a guard only.**  Writing to this key does NOT
   perform the actual WASM upgrade.  A separate deployment transaction must
   run `env.update_current_contract_wasm(wasm_hash)`; the guard can be
   verified in that transaction's preconditions.
