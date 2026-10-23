# Whitelist: Per-Address Persistent Membership Migration

This document explains the storage layout change introduced in the Callora
Whitelist contract: moving from a single instance-storage `Vec` of all
whitelisted addresses to per-address persistent membership entries with
paginated enumeration.

---

## Background

The original (V1) whitelist contract stored every whitelisted address in one
`Vec<Address>` under `StorageKey::WhitelistList` in **instance storage**.
Instance storage is loaded on every call to the contract, so:

- Each added address makes every call more expensive (`is_whitelisted`,
  `get_admin`, cooldown views).
- `contains` is O(n) over the full vector.
- Once the instance entry reaches the ledger-entry size limit, `add_address`
  can no longer write and the whitelist stops growing.

V2 replaces this with O(1) per-address persistent membership and a paginated
enumeration index.

---

## Storage layout diff

| Key | V1 type | V2 action |
|-----|---------|-----------|
| `WhitelistList` | `Vec<Address>` (instance) | Read during migration, then **removed** |
| `Member(addr)` | - | New: `bool` in **persistent** storage |
| `MemberGeneration(addr)` | - | New: `u64` in persistent storage, for clear-all validity |
| `MemberCount` | - | New: `u32` in **instance** storage |
| `ClearGeneration` | - | New: `u64` in instance storage, incremented on full clear |
| `WhitelistIndex` | - | New: `Vec<Address>` in instance storage, for enumeration only |
| `MigrationComplete` | - | New: `bool` in instance storage, set when migration done |
| `WhitelistOwner` | `Address` (instance) | Unchanged |
| `WhitelistAdmin` | `Address` (instance) | Unchanged |
| `WhitelistPendingAdmin` | `Address` (instance) | Unchanged |
| `WhitelistAdminCooldown` | `u64` (instance) | Unchanged |
| `WhitelistLastCriticalAction` | struct (instance) | Unchanged |

---

## Pre-migration checklist

Before running the migration:

1. **Upgrade the WASM** to the V2 binary via `upgrade(admin, new_wasm_hash)`.
2. **Freeze writes** (recommended): pause `add_address` / `remove_address`
   during the migration window to avoid new entries being added only to the
   legacy Vec.
3. **Determine member count**: query `get_member_count()` and inspect the
   legacy `WhitelistList` instance entry to know how many chunks are needed.

---

## Migration procedure

Call `migrate_list(admin, limit)` repeatedly until the legacy `WhitelistList`
is removed. Each call migrates up to `limit` legacy entries into persistent
`Member` entries.

```bash
while true; do
  RESULT=$(stellar contract invoke \
    --id <WHITELIST_CONTRACT_ID> \
    --source-account <ADMIN_SECRET_KEY> \
    --network mainnet \
    -- migrate_list \
       --caller <ADMIN_ADDRESS> \
       --limit 100)

  # RESULT is the number of addresses migrated this call
  echo "Migrated $RESULT addresses"

  # Check whether the legacy WhitelistList key still exists
  HAS_LEGACY=$(stellar contract invoke \
    --id <WHITELIST_CONTRACT_ID> \
    --source-account <ADMIN_SECRET_KEY> \
    --network mainnet \
    -- storage_has_whitelist_list)

  if [ "$HAS_LEGACY" = "false" ]; then
    break
  fi
done
```

Verify completion:

```bash
stellar contract invoke \
  --id <WHITELIST_CONTRACT_ID> \
  --source-account <ADMIN_SECRET_KEY> \
  --network mainnet \
  -- get_member_count
# Expected output: same as the original legacy Vec length
```

---

## Dual-store reads during migration

Until migration completes, `is_whitelisted(address)` consults **both** stores:

1. First checks the persistent `Member(addr)` entry (fast path, O(1)).
2. If not found and the legacy `WhitelistList` still exists, falls back to a
   linear scan of the legacy Vec.

Once `migrate_list` has drained the legacy Vec (the `WhitelistList` key is
removed and `MigrationComplete` is set), `is_whitelisted` only reads the
persistent `Member` entry.

---

## Deprecated API

`get_whitelist(admin)` is **deprecated**. It returns up to `MAX_PAGE` (100)
addresses by loading the full enumeration index from instance storage. Use
`get_whitelist_page(admin, start, limit)` instead for bounded, paginated
access.

---

## Bounded clear-all

`clear_page(admin, limit)` removes up to `limit` members per call. Repeated
calls until the whitelist is empty fully clear the whitelist. Each call is
bounded by `limit` persistent removals, avoiding instance-storage size limits.
A `ClearGeneration` counter invalidates all `Member` entries in O(1) when the
whitelist is fully emptied, so no per-address scan is needed.

The existing `clear_all(admin)` remains available and internally drains all
pages until empty.

---

## API reference

| Function | Arguments | Returns | Auth |
|----------|-----------|---------|------|
| `init(admin)` | admin address | `()` | Admin |
| `add_address(caller, address)` | admin, address | `()` | Admin |
| `remove_address(caller, address)` | admin, address | `()` | Admin |
| `is_whitelisted(address)` | address | `bool` | None |
| `get_whitelist_page(caller, start, limit)` | admin, offset, page size | `Vec<Address>` | Admin |
| `get_whitelist(caller)` *(deprecated)* | admin | `Vec<Address>` | Admin |
| `migrate_list(caller, limit)` | admin, chunk size | `u32` (migrated count) | Admin |
| `clear_page(caller, limit)` | admin, chunk size | `u32` (cleared count) | Admin |
| `clear_all(caller)` | admin | `()` | Admin |
| `set_admin(caller, new_admin)` | admin, new admin | `()` | Admin |
| `accept_admin()` | - | `()` | Pending admin |
| `cancel_admin_transfer(caller)` | admin | `()` | Admin |
| `get_admin()` | - | `Address` | None |
| `get_member_count()` | - | `u32` | None |
| `get_admin_cooldown()` | - | `u64` | None |
| `set_admin_cooldown(caller, seconds)` | admin, seconds | `()` | Admin |

---

## Error codes

| Code | Variant | Trigger |
|------|---------|---------|
| 1 | `NotInitialized` | Contract not initialized |
| 2 | `AlreadyInitialized` | `init` called more than once |
| 3 | `Unauthorized` | Caller is not the current admin |
| 4 | `AddressAlreadyInWhitelist` | Address is already whitelisted |
| 5 | `AddressNotInWhitelist` | Address is not whitelisted |
| 6 | `MemberCountOverflow` | Member count would overflow `u32` |
| 49 | `AdminCooldownActive` | Admin cool-off window is still active |
| 50 | `InvalidAdminCooldown` | Admin cool-off window outside bounds |
| 51 | `NoAdminTransferPending` | No admin transfer pending |
| 52 | `NewAdminSameAsCurrent` | Proposed admin matches current |

---

## Event changes

- `set_admin` now emits exactly one `admin_nominated` event (versioned topic
  shape `(admin_nominated, callora_v1, caller, new_admin)`). The duplicate
  legacy publish with topic shape `(admin_nominated, caller)` has been removed.
- `address_added` and `address_removed` keep their current versioned topic
  shape `(address_added/address_removed, callora_v1, caller, address)`.

---

## Security considerations

- All admin entry points call `caller.require_auth()` and verify the stored
  admin address before mutating any state.
- The cool-off mechanism (`admin::guard`) prevents rapid successive
  state-changing actions.
- The two-step admin transfer (`set_admin` → `accept_admin`) protects admin
  rotation.
- `is_whitelisted` performs at most two persistent storage reads plus one
  instance read, regardless of whitelist size.
- `clear_page` is bounded by `limit` and cannot exhaust instance storage.
- Arithmetic uses `checked_add`/`saturating_sub`; no silent overflow is
  possible.
