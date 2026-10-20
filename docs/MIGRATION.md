# Settlement V1 -> V2 Storage Migration

This document explains the storage layout change introduced between the V1 and
V2 Callora Settlement contract and describes the on-chain migration procedure.

---

## Background

The original (V1) settlement contract stored all developer balances as a flat,
single-token mapping:

```
DeveloperBalanceV1(developer: Address) -> i128
```

V2 introduces explicit per-token accounting so the same contract can handle
settlements in multiple token denominations (USDC, USDT, etc.):

```
DeveloperBalance(developer: Address, token: Address) -> i128
```

All V1 balances are denominated in the USDC token configured via
`StorageKey::Usdc`. The migration therefore converts every
`DeveloperBalanceV1(addr)` slot into `DeveloperBalance(addr, usdc_token)`.

---

## Storage layout diff

| Key | V1 type | V2 action |
|-----|---------|-----------|
| `DeveloperBalanceV1(addr)` | `i128` | Read, merged into V2 key, then **removed** |
| `DeveloperBalance(addr, usdc_token)` | - | Written during migration |
| `StorageKey::StorageVersion` | absent | Set to `2u32` on completion |
| `StorageKey::PendingDeveloperMigration(addr)` | - | New: timelock'd address migration |

---

## Pre-migration checklist

Before running the migration:

1. **Upgrade the WASM** to the V2 binary via `upgrade(admin, new_wasm_hash)`.
2. **Configure USDC** if not already set: `set_usdc_token(admin, usdc_address)`.
3. **Freeze incoming payments** (recommended): pause the vault or route new
   payments to a staging account during the migration window.
4. **Determine developer count**: query `get_developer_balances_page` to count
   registered developers. If the count is > 50, use the paginated migration.

---

## Migration procedure

### Option A - one-shot (<=50 developers)

Call once; the entire migration runs in a single transaction:

```bash
stellar contract invoke \
  --id  <SETTLEMENT_CONTRACT_ID> \
  --source-account <ADMIN_SECRET_KEY> \
  --network mainnet \
  -- migrate_v1_to_v2 \
     --caller <ADMIN_ADDRESS>
```

Verify completion:

```bash
stellar contract invoke \
  --id  <SETTLEMENT_CONTRACT_ID> \
  --source-account <ADMIN_SECRET_KEY> \
  --network mainnet \
  -- migration_storage_version
# Expected output: 2
```

### Option B - paginated (>50 developers)

Call in a loop until `is_complete` is `true`:

```bash
OFFSET=0
while true; do
  RESULT=$(stellar contract invoke \
    --id  <SETTLEMENT_CONTRACT_ID> \
    --source-account <ADMIN_SECRET_KEY> \
    --network mainnet \
    -- migrate_v1_to_v2_page \
       --caller     <ADMIN_ADDRESS> \
       --offset     $OFFSET \
       --batch_size 50)

  # RESULT is a JSON tuple: [next_offset, is_complete]
  NEXT_OFFSET=$(echo $RESULT | jq '.[0]')
  IS_COMPLETE=$(echo $RESULT | jq '.[1]')

  echo "Migrated up to offset $NEXT_OFFSET; complete=$IS_COMPLETE"

  if [ "$IS_COMPLETE" = "true" ]; then
    break
  fi
  OFFSET=$NEXT_OFFSET
done
```

---

## Merge semantics

If the V2 WASM began accepting payments for a developer **before** the
migration ran (e.g. a payment arrived immediately after WASM upgrade), that
developer will have both a V1 and a V2 balance. The migration merges them:

```
new_v2_balance = v1_balance + existing_v2_balance
```

Overflow of `i128` causes the transaction to abort with error code `8`
(`DeveloperOverflow`). This is an extreme edge case; balances would need to
approach 2^127 micro-units simultaneously.

---

## Idempotency

Every migration entry point is idempotent. Calling `migrate_v1_to_v2` or
`migrate_v1_to_v2_page` after a completed migration (`StorageVersion == 2`)
returns immediately without modifying state. It is safe to call the migration
more than once.

---

## Rollback plan

The migration is **irreversible** via the public API: V1 slots are removed
after conversion. However, Soroban ledger history is permanent; a rollback
would require redeploying the V1 WASM and replaying credits from historical
events, which is costly and not recommended.

**Risk mitigation**: pause the vault before migrating to ensure no new V1
credits can arrive after the migration window opens.

---

## Verification

After migration, confirm:

1. `migration_storage_version()` returns `2`.
2. Each known developer has a non-zero V2 balance:
   ```bash
   stellar contract invoke -- get_developer_balance \
     --developer <DEV_ADDRESS> --token <USDC_ADDRESS>
   ```
3. No V1 slots remain (query `DeveloperBalanceV1` for known developers
   returns `None`/`0`).
4. The `mig_done` event was emitted (check ledger event stream for topic
   `"mig_done"` with data `2u32`).

---

## API reference

| Function | Arguments | Returns | Auth |
|----------|-----------|---------|------|
| `migrate_v1_to_v2(caller)` | admin address | `()` | Admin |
| `migrate_v1_to_v2_page(caller, offset, batch_size)` | admin, start index, page size | `(u32, bool)` | Admin |
| `migration_storage_version()` | - | `u32` | None |

### Error codes

| Code | Variant | Trigger |
|------|---------|---------|
| 1 | `NotInitialized` | Contract not initialised |
| 3 | `Unauthorized` | Caller is not the admin |
| 8 | `DeveloperOverflow` | V1 + V2 balance overflows `i128` |
| 9 | `UsdcTokenNotConfigured` | USDC token not configured |

---

## Settlement error outcome additions (ABI-additive)

`SettleOutcome` (used by `batch_settle`) gained four new variants so batch
callers can react to common per-item failures instead of receiving a generic
`OtherError`:

| Variant | Trigger |
|---------|---------|
| `DeveloperFrozen` | The developer's balance is frozen |
| `MinBalanceViolation` | The withdrawal would drop the balance below the minimum |
| `InsufficientContractBalance` | The contract's USDC balance is less than the claim amount |
| `InvalidRecipient` | The recipient is the settlement contract itself |

These variants are **appended** to the enum, which is ABI-additive for
`contracttype` enums: existing variant discriminants are unchanged, so
off-chain consumers that already decode `SettleOutcome` keep working. Clients
that exhaustively match on `SettleOutcome` should add arms for the new
variants.

Relatedly, `validate_claim` now returns `Err(InvalidRecipient)` instead of
trapping, so `simulate_claim` no longer panics on an invalid recipient and a
single bad item no longer aborts an entire `batch_settle`. The validation
order is also pinned: `InsufficientDeveloperBalance` is reported before
`MinBalanceViolation`.

---

## Security considerations

- All migration entry points call `caller.require_auth()` and verify the
  stored admin address before mutating any state.
- The migration is protected by the standard two-step admin transfer; only
  an address that has gone through `set_admin` -> `accept_admin` can act
  as admin.
- Arithmetic uses `checked_add` throughout; no silent integer overflow is
  possible.
- The `StorageVersion` marker ensures migration is not re-run accidentally.

---

## Storage-migration validator key namespacing (Issue #1415)

The shared `callora-storage-migration` crate records its bookkeeping in the
host contract's **instance** storage. Through version 1 those keys were not
namespaced (`Version`, `SchemaLayoutHash`, `AuthorizedUpgrade`,
`BackupPresent`). Soroban encodes every `#[contracttype]` unit variant as
`Vec[Symbol("VariantName")]` regardless of the enum it belongs to, so any host
that declared a key with the same name shared the slot.

That was exploitable in `EmergencyMigration`: an unauthenticated call to
`validate_before_upgrade` wrote the validator's `AuthorizedUpgrade` slot, which
`is_upgrade_authorized` read as if it were the admin's own authorization.

### Key map

| Purpose | Legacy key (v1) | Namespaced key (v2) |
|---------|-----------------|---------------------|
| Storage-layout version | `Version` | `SmVersion` |
| Schema/layout hash | `SchemaLayoutHash` | `SmSchemaLayoutHash` |
| Upgrade authorization | `AuthorizedUpgrade` | `SmAuthorizedUpgrade` |
| Rollback backup marker | `BackupPresent` | `SmBackupPresent` |

`STORAGE_MIGRATION_VERSION` in the validator crate is now `2`. Host
`STORAGE_MIGRATION_VERSION` constants are unchanged because the *business*
storage layout did not change; only the validator's own bookkeeping moved.

### Read-through migration

Deployed hosts keep working without an explicit migration transaction:

1. On the first validator read, if a namespaced slot is absent, the legacy
   slot is copied into it.
2. Once the namespaced slot exists it is authoritative and the legacy slot is
   never consulted again.
3. Legacy slots are **not deleted**. In `EmergencyMigration` those same
   encodings belong to the host's own business keys, so deleting them would
   destroy host state.

`record_deployed_schema`, `current_version`, `is_upgrade_authorized`,
`validate_before_upgrade`, `finalize_migration`, and `authorized_upgrade` all
perform this read-through.

### Upgrade authorization is now admin-only

`EmergencyMigration::validate_before_upgrade(caller, target_version, wasm_hash,
allow_rollback)` calls `caller.require_auth()` and requires `caller == Admin`
before it touches storage. Error precedence is **auth → admin → hash →
version**; a non-admin receives `StorageMigrationError::Unauthorized` and can
neither forge nor overwrite an authorization. The entrypoint remains
state-changing (it records the validator authorization guard), so it is not a
read-only dry run.

Hosts that embed the validator (`revenue_pool`, `checkpoint`, `yield`, and
`migrate` itself) are covered by a cross-host collision regression test that
compares the canonical `ScVal` XDR encoding of every key.

# callora-rescue: Per-token `TotalRescued` counter (v0.2.0)

## Background

The original rescue contract kept a single global `TotalRescued` counter in
instance storage.  Every call to `rescue` or `rescue_capped` added `amount` to
this counter regardless of which token was being rescued.  This produced a
meaningless aggregate (e.g. "100 USDC + 200 XLM = 300") that could not be used
for auditing because it mixed amounts of unrelated tokens.

Additionally, an admin could call `rescue` with `to == current_contract_address()`,
which transferred nothing off-chain but still incremented `TotalRescued` and
emitted a rescue event — fabricating an audit trail without moving any funds.

## What changed

| Area | Before (v0.1.x) | After (v0.2.0) |
|------|-----------------|----------------|
| Storage key | `DataKey::TotalRescued` (single `i128`) | `DataKey::TotalRescued(Address)` (per-token `i128`) |
| View function | `total_rescued() -> i128` | `total_rescued_for(token: Address) -> i128` |
| Self-transfer | Allowed; incremented counter | Rejected with `RescueError::InvalidRecipient` (code 8) |
| `cap <= 0` in `rescue_capped` | `ExceedsCap` | `InvalidCap` (code 9) |
| CEI ordering | Counter updated **after** transfer | Counter updated **before** transfer |
| Instance TTL | Never bumped | Bumped on every entrypoint |

## New error codes

These are append-only; all existing codes are unchanged.

| Code | Variant | Trigger |
|------|---------|---------|
| 8 | `InvalidRecipient` | `to == current_contract_address()` |
| 9 | `InvalidCap` | `cap <= 0` in `rescue_capped` |

## Deprecated API

`total_rescued()` is kept for backwards compatibility but **always returns 0**.
It will be removed in a future major version.  Migrate callers to
`total_rescued_for(token)`.

```rust
// Before
let total = client.total_rescued();

// After
let total = client.total_rescued_for(&token_address);
```

## On-chain state migration

There is **no automatic on-chain state migration**.  The old single-key
`DataKey::TotalRescued` entry (if any exists from a v0.1.x deployment) is
simply ignored after the upgrade; it will eventually expire with the instance
TTL.  Per-token entries start from zero.

If you need to preserve historical totals, read the old value via any v0.1.x
client before upgrading and record it off-chain.

## Invariant changes

The balance-sync invariant used in property tests now reads:

```
total_rescued_for(token) + on_ledger_balance(contract, token) == initial_mint(token)
```

(Previously: `total_rescued() + on_ledger_balance == initial_mint`, which was
incorrect when multiple tokens were involved.)
