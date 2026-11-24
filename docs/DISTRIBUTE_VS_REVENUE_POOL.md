# Distribute vs Revenue Pool: Roles and Responsibilities

## Purpose

This note clarifies the intended role of `callora-distribute` and
`callora-revenue-pool`, which currently share nearly identical admin-gated
`distribute` / `batch_distribute`, pause, max-cap, and upgrade mechanisms.
It exists to prevent operator confusion about which contract should hold
funds and to avoid fixes landing in one copy but not the other.

## Canonical contract for payouts

`callora-distribute` is the **canonical contract for payouts**. All new
distribution flows, integrations, and operational runbooks should target
`contracts/distribute/src/lib.rs`.

`callora-revenue-pool` is retained as a legacy / compatibility surface. It is
expected to be deprecated in favor of `callora-distribute` once all existing
deployments and integrations have migrated. New features should not be added
to the revenue pool unless they are required to maintain backward
compatibility for existing deployments.

## Behavioural differences

The two contracts implement the same broad capabilities but differ in
observability and validation details:

| Aspect                   | `callora-distribute`                          | `callora-revenue-pool`                        |
| ------------------------- | ------------------------------------------ | ----------------------------------------- |
| Event shape               | Emits distinct events for single and batch distribution | Emits events with a different field layout / naming |
| Error handling           | Returns structured errors with explicit variants  | Returns errors with different variant names / messages |
| Duplicate recipient check | Enforced in the canonical implementation      | May be missing or inconsistent in the legacy copy |
| Admin gating             | Present                                         | Present                                         |
| Pause / max cap / upgrade | Present                                      | Present                                         |

When a fix applies to shared behaviour (e.g. duplicate recipient checks), it
MUST be applied to `callora-distribute` first. Applying it only to the
revenue pool is not sufficient and will be treated as an incomplete fix.

## Batch preflight (Distribute only)

`simulate_batch_distribute(payments: Vec<(Address, i128)>) -> BatchSimulation`
is a public, authorization-free view. It returns `ok`, `total`, `legs`,
`contract_balance`, `paused`, and `errors: Vec<u32>`. Unlike Distribute's other
views, it does not refresh Distribute's instance TTL. It does not write its
storage, transfer tokens, or emit events.

Errors are collected in execution order: pause (`4`), missing admin (`1`),
empty/oversized batch (`7`/`8`), missing token (`1`), then each payment's
nonpositive amount (`15`), cap violation (`16`), self-recipient (`14`), or
running-total overflow (`9`), followed by insufficient balance (`17`). The
batch limit is 50. Repeated codes are retained, and an amount error does not
hide a self-recipient error on the same leg. Missing configuration is reported
once; a missing token yields a reported balance of zero.

`total` sums **positive** amounts, including those on invalid legs. Nonpositive
amounts never offset the required balance. On overflow it saturates at
`i128::MAX`; the report includes overflow and insufficient balance even if the
contract holds `i128::MAX`. Fix all reported errors and simulate again.

For an authorized admin using the same contract state, the first error matches
the real batch's first local validation failure. `ok` does not validate caller
authorization or predict external transfer failures, such as a frozen token
recipient. The token's `balance` view is invoked and may refresh **its own**
instance/balance TTL (the Stellar Asset Contract does this internally).
Distribute cannot suppress external token behavior. Use RPC simulation rather
than a submitted transaction when no on-chain TTL changes are wanted.

Example: with a cap of 100 and balance of 100, payments of 60 and 40 report
`ok: true, total: 100, legs: 2, contract_balance: 100, paused: false, errors: []`.
Payments of 0 to Distribute itself and 101 to another address report
`errors: [15, 14, 16, 17]` and `total: 101`.

## Relationship to `callora-freeze`

`callora-freeze` is meant to protect the **canonical payout contract**,
i.e. `callora-distribute`. Freezing should halt new distributions through
the canonical contract and prevent funds from being moved while an incident is
under investigation. The legacy revenue pool is secondary and is only expected
to be frozen if it still holds funds or remains active in a given deployment.

## Consolidation proposal

The long-term plan is to consolidate distribution logic into
`callora-distribute` and retire `callora-revenue-pool` once all known
deployments have migrated. Until then, both contracts must remain buildable
and tested, and any shared behavioural fix must be applied to both copies with
`callora-distribute treated as the source of truth.

## Summary

- **Canonical payout contract:** `callora-distribute`
- **Legacy / compatibility contract:** `callora-revenue-pool`
- **Freeze target:** `callora-distribute` (with the revenue pool frozen only if still active)
- **New features:** go into `callora-distribute` only

## Revenue pool share distributions

`callora-revenue-pool` exposes `distribute_by_shares(total, shares)`, where
`shares` is an ordered list of `(recipient, basis_points)` pairs. All shares
must be positive and sum exactly to `10_000`. Recipient amounts are calculated
with floor division: `total * bps / 10_000`.

Any remainder from rounding is added to the first recipient. This rule makes
the distribution deterministic and ensures the final payment amounts sum exactly
to `total`. The entrypoint is limited to `MAX_BATCH_SIZE` (50) recipients,
preserves duplicate-recipient, pause, emergency-pause, admin, cap and balance
checks, and emits the existing batch distribution lifecycle events per recipient.
