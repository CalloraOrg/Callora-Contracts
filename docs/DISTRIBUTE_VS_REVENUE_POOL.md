# Distribute vs Revenue Pool: Roles and Responsibilities

## Purpose

This note clarifies the intended role of `callora-distribute` and
`callora-revenue-pool`, which currently share nearly identical admin-gated
`distribute`/`batch_distribute`, pause, max-cap, and upgrade code but differ
in event shapes and error handling. Operators need a single source of truth
for which contract holds funds and which contract is canonical for payouts.

## Canonical contract for payouts

`callora-revenue-pool` is the canonical contract for holding funds and
distributing payouts to recipients. It is the contract that should be
funded by operators and the one that is expected to be used in production
for revenue distribution.

`callora-distribute` is a general-purpose distribution primitive kept for
compatibility and for non-revenue distribution use cases. It is not the
preferred holder of operational revenue funds.

## Role of `callora-distribute`

- General-purpose distribution mechanism for admin-gated payouts.
- Used for non-revenue distribution scenarios and legacy integrations.
- Maintained for backward compatibility with existing integrations.
- Should not be the primary destination for revenue funds.

## Role of `callora-revenue-pool`

- Canonical contract for revenue collection and distribution.
- Holds operational revenue funds until they are distributed to recipients.
- Expected to be the contract operators fund and monitor in production.
- Provides the audit trail for revenue payouts.

## Behavioural differences

The two contracts share the same broad shape (admin-gated `distribute`,
`batch_distribute`, pause, max cap, upgrade) but differ in the following
areas:

- **Event shapes.** The events emitted on distribution differ between the two
contracts, so indexers and off-chain monitoring must handle each contract's
event schema separately.
- **Error handling.** The contracts differ in the errors they return and in
the conditions they check before distributing.
- **Duplicate recipient checks.** Duplicate recipient checks have landed in
one copy but not the other, which is exactly the kind of drift this note is meant
to surface. Both contracts should enforce equivalent validation for the
operations they expose.

## Which contract does `callora-freeze` protect?

`callora-freeze` is meant to protect the canonical payout contract,
`callora-revenue-pool`. Freezing the revenue pool is the primary safety
mechanism for stopping ongoing revenue payouts. Operators should not rely on
freezing `callora-distribute` as a substitute for freezing the revenue pool.

## Consolidation proposal

Given the overlap, the long-term proposal is to consolidate distribution
logic into a common internal module or trait that both contracts use, so
event shapes and error handling stay in sync. Until that consolidation lands
(and because it is out of scope for this note), the rule of thumb is:

- Fund and distribute revenue through `callora-revenue-pool`.
- Treat `callora-distribute` as a general-purpose primitive, not the
revenue holder.
- When fixing a bug in one contract's distribution path, check whether the
other contract needs the same fix.

## See also

- `contracts/distribute/src/lib.rs`
- `contracts/revenue_pool/IMPLEMENTATION_SUMMARY.md`
- `README.md` (“What's included”)
