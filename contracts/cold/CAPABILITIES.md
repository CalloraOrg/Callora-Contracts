# Callora Cold — Capability Bitmap

The cold surface exposes a `capabilities()` view that returns a `u64` bitmask.
Each set bit indicates a cold-storage feature supported by this deployment so
clients can detect capability deltas across upgrades without parsing versions.

Cold accounting itself lives in the vault (`contracts/vault/src/cold_storage.rs`);
this bitmap is the discovery API for that feature set.

## Querying capabilities

```typescript
const caps = await coldClient.capabilities();

if (caps & CAP_COLD_MULTISIG_SWEEP) {
  // safe to drive propose/approve cold-sweep flow
}
```

```rust
let caps: u64 = CalloraColdClient::new(&env, &cold).capabilities();
let has_rebalance = caps & CAP_AUTO_REBALANCE != 0;
```

## Detecting deltas

```typescript
const before = await oldClient.capabilities();
const after = await newClient.capabilities();
const added = after & ~before;
const removed = before & ~after;
```

## Bit registry

| Bit | Hex | Constant | Feature | Introduced |
|-----|-----|----------|---------|------------|
| 0 | `0x01` | `CAP_HOT_COLD_SPLIT` | Hot/cold accounting partition (`hot + cold == total`) | v1.0.0 |
| 1 | `0x02` | `CAP_AUTO_REBALANCE` | Deposit-time hot→cold rebalance on drift | v1.0.0 |
| 2 | `0x04` | `CAP_COLD_MULTISIG_SWEEP` | N-of-M propose/approve cold sweep | v1.0.0 |
| 3 | `0x08` | `CAP_SET_HOT_COLD_RATIO` | Update `hot_bps` / rebalance threshold | v1.0.0 |
| 4 | `0x10` | `CAP_SET_COLD_SIGNERS` | Rotate cold signer set / threshold | v1.0.0 |
| 5 | `0x20` | `CAP_COLD_BALANCE_VIEW` | Read `{hot, cold}` split | v1.0.0 |
| 6 | `0x40` | `CAP_PENDING_COLD_SWEEP_VIEW` | Read pending multisig sweep | v1.0.0 |
| 7–63 | — | *(reserved)* | Always zero | — |

## Stability guarantee

- A bit position is assigned once and never reused for a different feature.
- Removed features keep their bit **cleared**; the position stays reserved.
- New features occupy the lowest available bit index.
- Reserved bits (7–63) are always `0` in the current version.
