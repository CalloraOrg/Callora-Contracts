# Distribute vs. Revenue Pool: Roles and Responsibilities

This note describes the intended role of `contracts/distribute` and
`contracts/revenue_pool`, which contract is canonical for payouts, the behavioural
differences between them, and which contract `callora-freeze` is meant to
protect.

## Canonical contract for payouts

`contracts/revenue_pool` is the canonical contract for holding funds and
performing payouts. Operators should deposit revenue into the revenue pool and
run distribution from there. `contracts/distribute` is retained as a legacy /
compatibility entry point for integrations that already target it, but new deployments
and new integrations should use `contracts/revenue_pool`.

## Intended role of each contract

### `contracts/revenue_pool` (canonical)

- Holds the protocol revenue balance that will be distributed to recipients.
- Owns the authoritative distribution book-keeping: total distributed amounts,
  per-recipient totals, and cap enforcement.
- Exposes admin-gated `distribute` / `batch_distribute`, pause / unpause, max cap
  configuration, and upgrade entry points.
- Is the contract that operational tooling (operator dashboards, reporting,
  freeze controls) should treat as the source of truth for payouts and balances.

### `contracts/distribute` (legacy / compatibility)

- Provides the same admin-gated `distribute` / `batch_distribute`, pause, max cap
  and upgrade surface for existing integrations.
- Is NOT intended to hold new protocol revenue. New funds should be deposited
  into `contracts/revenue_pool`.
- Should be treated as a thin compatibility shim: fixes that matter for payout
  correctness must land in `contracts/revenue_pool` first, then be mirrored here
  only if a consumer still depends on it.

## Behavioural differences

| Aspect | `contracts/revenue_pool` (canonical) | `contracts/distribute` (legacy) |
| --- | --- | --- |
| Purpose | Holds and distributes protocol revenue. | Compatibility distribution entry point. |
| Event shape | Emits the revenue-pool distribution events used by operational tooling. | Emits the legacy distribute event shape. |
| Error handling | Returns the revenue-pool error enum and codes. | Returns the distribute error enum and codes. |
| Duplicate recipient checks | Enforced as part of the canonical distribution path. | May lack checks that were only added to the canonical contract. |
| Funding expectation | Expected to hold new protocol revenue. | Not expected to hold new protocol revenue. |
| Admin gating | `distribute` / `batch_distribute`, pause, max cap, upgrade are admin-gated. | `distribute` / `batch_distribute`, pause, max cap, upgrade are admin-gated. |
| Status | Canonical for payouts. | Legacy; maintained for existing integrations only. |

## Which contract `callora-freeze` protects

`callora-freeze` is meant to protect `contracts/revenue_pool`, the
canonical contract for payouts. Freezing the canonical contract halts new payouts
from the contract that holds protocol revenue. `contracts/distribute` is not the
primary target of `callora-freeze`; if it is freshen at all, it is only to stop
legacy distributions while the canonical contract is migrated to.

## Consolidation proposal

Maintain `contracts/revenue_pool` as the single canonical implementation. Treat
`contracts/distribute` as a deprecated compatibility shim:

1. Route all new funding and new integrations to `contracts/revenue_pool`.
2. Land correctness fixes (such as duplicate recipient checks) and new features
   in `contracts/revenue_pool` first.
3. Mirror only the minimum behaviour required to keep existing consumers of
   `contracts/distribute` working.
4. Eventually retire `contracts/distribute` once no consumer depends on it.

## References

- `contracts/distribute/src/lib.rs`
- `contracts/revenue_pool/IMPLEMENTATION_SUMMARY.md`
- `README.md`
