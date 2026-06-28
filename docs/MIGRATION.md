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

## Security considerations

- All migration entry points call `caller.require_auth()` and verify the
  stored admin address before mutating any state.
- The migration is protected by the standard two-step admin transfer; only
  an address that has gone through `set_admin` -> `accept_admin` can act
  as admin.
- Arithmetic uses `checked_add` throughout; no silent integer overflow is
  possible.
- The `StorageVersion` marker ensures migration is not re-run accidentally.
