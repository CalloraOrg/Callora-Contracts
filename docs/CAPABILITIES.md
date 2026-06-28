# Callora Vault — Capability Bitmap

The vault exposes a `capabilities()` view function that returns a `u64` bitmask.
Each set bit indicates a feature that the current contract version supports.
Clients can use bit operations to detect available capabilities without complex
version-string parsing.

## Querying capabilities

```typescript
// Using @stellar/stellar-sdk
const caps = await client.capabilities();

if (caps & CAP_BATCH_DEDUCT) {
  // safe to call batch_deduct
}
```

```rust
// Inside another Soroban contract
let caps: u64 = VaultClient::new(&env, &vault).capabilities();
let has_rate_limit = caps & CAP_RATE_LIMIT != 0;
```

## Bit registry

| Bit | Hex value | Constant | Feature | Introduced |
|-----|-----------|----------|---------|-----------|
| 0 | `0x00001` | `CAP_DEPOSIT` | `deposit()` — accept USDC from allowlisted callers | v1.0.0 |
| 1 | `0x00002` | `CAP_WITHDRAW` | `withdraw()` / `withdraw_to()` — owner-initiated withdrawal | v1.0.0 |
| 2 | `0x00004` | `CAP_DEDUCT` | `deduct()` — authorized caller deducts to settlement | v1.0.0 |
| 3 | `0x00008` | `CAP_BATCH_DEDUCT` | `batch_deduct()` — atomic multi-item deduct | v1.0.0 |
| 4 | `0x00010` | `CAP_PAUSE` | `pause()` / `unpause()` — circuit-breaker (admin or owner) | v1.0.0 |
| 5 | `0x00020` | `CAP_AUTHORIZED_CALLER` | `set_authorized_caller()` — delegate deduct permission | v1.0.0 |
| 6 | `0x00040` | `CAP_OFFERING_METADATA` | `set_metadata()` / `update_metadata()` / `remove_metadata()` | v1.0.0 |
| 7 | `0x00080` | `CAP_PRICE_REGISTRY` | `set_price()` / `get_price()` / `list_prices()` / `remove_price()` | v1.0.0 |
| 8 | `0x00100` | `CAP_REQUEST_IDEMPOTENCY` | Optional `request_id` on `deduct` / `batch_deduct` for at-least-once retry | v1.0.0 |
| 9 | `0x00200` | `CAP_TWO_STEP_OWNERSHIP` | `transfer_ownership()` / `accept_ownership()` | v1.0.0 |
| 10 | `0x00400` | `CAP_TWO_STEP_ADMIN` | `set_admin()` / `accept_admin()` / `cancel_admin_transfer()` | v1.0.0 |
| 11 | `0x00800` | `CAP_SETTLEMENT` | Settlement contract integration via `set_settlement()` | v1.0.0 |
| 12 | `0x01000` | `CAP_REVENUE_POOL` | `propose_revenue_pool()` / `accept_revenue_pool()` / `cancel_revenue_pool()` | v1.0.0 |
| 13 | `0x02000` | `CAP_RATE_LIMIT` | Per-developer token-bucket rate limits via `set_developer_rate_limit()` | v1.0.0 |
| 14 | `0x04000` | `CAP_ADMIN_BROADCAST` | `broadcast()` — admin-signed on-chain messages with severity | v1.0.0 |
| 15 | `0x08000` | `CAP_DEPOSITOR_ALLOWLIST` | `add_address()` / `set_allowed_depositor()` / `clear_all()` | v1.0.0 |
| 16 | `0x10000` | `CAP_SLIPPAGE_GUARD` | `max_fee_bps` parameter on `deduct()` | v1.0.0 |
| 17 | `0x20000` | `CAP_UPGRADE` | `upgrade()` — admin-gated WASM replacement | v1.0.0 |
| 18–63 | — | *(reserved)* | Always zero; reserved for future capabilities | — |

## Stability guarantee

- A bit position is assigned once and never reused for a different feature.
- Removed features keep their bit **cleared** in future versions; the position stays reserved.
- New features always occupy the lowest available bit index.
- Reserved bits (18–63) are always `0` in the current version.

## Integration checklist

1. Call `capabilities()` once at startup or after detecting a contract upgrade.
2. Cache the result — it is immutable for a given WASM deployment.
3. Use `caps & CAP_<FEATURE> != 0` to gate feature-specific code paths.
4. Treat an unknown bit as a forward-compatible signal: ignore it, do not error.

## Constant values (for off-chain SDKs)

```typescript
export const CAP_DEPOSIT            = 0x00001n;
export const CAP_WITHDRAW           = 0x00002n;
export const CAP_DEDUCT             = 0x00004n;
export const CAP_BATCH_DEDUCT       = 0x00008n;
export const CAP_PAUSE              = 0x00010n;
export const CAP_AUTHORIZED_CALLER  = 0x00020n;
export const CAP_OFFERING_METADATA  = 0x00040n;
export const CAP_PRICE_REGISTRY     = 0x00080n;
export const CAP_REQUEST_IDEMPOTENCY= 0x00100n;
export const CAP_TWO_STEP_OWNERSHIP = 0x00200n;
export const CAP_TWO_STEP_ADMIN     = 0x00400n;
export const CAP_SETTLEMENT         = 0x00800n;
export const CAP_REVENUE_POOL       = 0x01000n;
export const CAP_RATE_LIMIT         = 0x02000n;
export const CAP_ADMIN_BROADCAST    = 0x04000n;
export const CAP_DEPOSITOR_ALLOWLIST= 0x08000n;
export const CAP_SLIPPAGE_GUARD     = 0x10000n;
export const CAP_UPGRADE            = 0x20000n;
```
