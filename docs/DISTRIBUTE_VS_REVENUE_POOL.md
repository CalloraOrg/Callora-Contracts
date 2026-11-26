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

## Multi-token distributions in `callora-distribute`

`callora-distribute` supports an admin-managed allowlist of approved Stellar
asset tokens alongside its default `usdc_token` configured at `init`:

- **Allowlist management:** `add_approved_token(caller, token)`,
  `remove_approved_token(caller, token)`, and `is_approved_token(token)`. The
  default `usdc_token` is approved at `init` and cannot be removed
  (`CannotRemoveDefaultToken`). Duplicate additions fail with
  `TokenAlreadyApproved`, and unapproved tokens fail with `UnsupportedToken`.
- **Per-token caps:** `set_max_distribute_for(caller, token, max_distribute)`
  and `get_max_distribute_for(token)`. When unset for a token, the cap falls
  back to the global `max_distribute` value.
- **Balances:** `balance_of(token)` returns the contract's on-ledger balance
  for any token; `balance()` and `get_usdc_token()` remain unchanged for USDC.
- **Distribution entrypoints:** `distribute_token(caller, token, to, amount)`
  and `batch_distribute_multi(caller, payments)` (where each leg is
  `(token, to, amount)`). Both enforce pause, admin auth, `MAX_BATCH_SIZE` (50),
  per-token caps, and per-token balances atomically before any transfer occurs,
  and include `token` in `DistributionLifecycleEvent` payloads without changing
  existing event topic names. Legacy `distribute` and `batch_distribute`
  entrypoints remain unchanged.
