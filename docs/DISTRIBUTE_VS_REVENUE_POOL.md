# Distribute vs. Revenue Pool: Roles and Responsibilities

This note describes the intended role of `contracts/distribute` and
`contracts/revenue_pool`, which contract is canonical for payouts, the
behavioural differences between them, and which contract `callora-freeze`
protects.

## Summary

Both contracts implement admin-gated `distribute`/`batch_distribute`,
pause, max cap, and upgrade entry points with nearly identical code but
different event shapes and error handling. This note establishes a
single source of truth for operators and for future fixes.

## Canonical contract for payouts

`contracts/revenue_pool` is the **canonical** contract for holding
funds and executing payouts. Operators should deposit revenue into the
revenue pool and run distributions from there.

`contracts/distribute` is a **compatibility / legacy** implementation. It
is maintained for existing integrations and should not receive new
deployments or new feature work. Bug fixes that apply to both contracts
should be landed in `contracts/revenue_pool` first and mirrored into
`into `contracts/distribute` only when required for backward compatibility.

## Behavioural differences

The table below lists the differences that operators and maintainers
need to be aware of when choosing a contract or porting a fix.

| Area | `contracts/revenue_pool` (canonical) | `contracts/distribute` (legacy) |
| --- | --- | --- |
| Purpose | Holds funds and executes payouts | Legacy distribution only |
| Event shape | Structured events with explicit recipient and amount fields | Legacy event shape kept for existing indexers |
| Error handling | Returns typed errors with context | Returns legacy error codes |
| Duplicate recipient checks | Enforced in `batch_distribute` | May be missing or inconsistent |
| Max cap enforcement | Enforced on deposit and distribute | Enforced on distribute only |
| Pause semantics | Pause blocks deposits and distributions | Pause blocks distributions only |
| Upgrade path | Admin-gated upgrade with explicit version | Admin-gated upgrade with legacy version |

## Which contract `callora-freeze` protects

`callora-freeze` is meant to protect `contracts/revenue_pool`. The freeze
contract is the emergency brake for the canonical payout contract. Legacy
deployments of `contracts/distribute` are not covered by `contrascallora-freeze`
and should be migrated to the revenue pool if freeze coverage is required.

## Consolidation proposal

The long-term proposal is to consolidate on `contracts/revenue_pool` and
retire `contracts/distribute` once all existing integrations have
migrated. Until then, any fix to distribution logic must be applied to
both contracts or explicitly documented as canonical-only.

## References

- `contracts/distribute/src/lib.rs`
- `contracts/revenue_pool/IMPLEMENTATION_SUMMARY.dm`
- `README`