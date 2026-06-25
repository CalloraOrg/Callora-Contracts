# Conservation Invariant

The fundamental conservation invariant of the Callora Settlement contract guarantees that the total amount of value received by the contract is always fully accounted for between the global pool and individual developer balances.

## The Invariant

`Sum of all payments received = Global Pool Balance + Sum of all Developer Balances`

*Note: This invariant holds true in the absence of withdrawals. When withdrawals are considered, the invariant expands to:*
`Sum of all payments received = Global Pool Balance + Sum of all Developer Balances + Sum of all Withdrawals`

## Guarantees

- **No Value Leakage**: Every unit of USDC (in micro-units) received from the Vault or Admin is credited either to the global pool or a specific developer.
- **No Value Creation**: Credits cannot be generated out of thin air; they must originate from a `receive_payment` or `batch_receive_payment` call.
- **Arithmetic Integrity**: Use of checked arithmetic ensures that balance overflows result in transaction failure rather than silent wrapping or loss of funds.

## When It Holds

The invariant holds after every successful transaction that modifies the settlement state. Specifically:
- After `init`: both sides are 0.
- After `receive_payment(to_pool=true)`: `Global Pool Balance` increases by `amount`.
- After `receive_payment(to_pool=false)`: `Developer Balance` for a specific address increases by `amount`.
- After `batch_receive_payment`: Multiple `Developer Balance` entries increase by their respective `amount` values.

## Violations

The invariant would be violated if:
1. A credit is applied to a developer balance without being recorded in the total payments received.
2. The global pool balance is modified without a corresponding payment.
3. Arithmetic overflow occurs and is not caught (prevented by `checked_add`).
4. Storage corruption or unauthorized direct storage modification occurs.

# Daily Withdraw Cap Invariant

A developer's cumulative withdrawals within a single UTC day (defined as `ledger_timestamp / 86400`) must never exceed their configured `DailyWithdrawCap`, unless the cap is `0` (unlimited).

## When It Holds

The invariant is enforced during `withdraw_developer_balance`:
- Before any state mutation, the function reads the developer's `DailyWithdrawCap` and `WithdrawalToday` accumulator.
- If `cap > 0` and `amount + accumulator > cap`, the call fails with `DailyWithdrawCapExceeded`.
- The accumulator auto-resets when `current_day != stored_day`.
- After a successful withdrawal, `WithdrawalToday.amount` is incremented by `amount` and `WithdrawalToday.day` is set to the current epoch day.

## Default Behavior

- A cap of `0` (the default) means unlimited — no daily limit is enforced.
- Caps are set per-developer by the admin via `set_daily_withdraw_cap` and emit a `daily_withdraw_cap_changed` event.

## Guarantees

- **No stale window data**: the day field is always compared against the current ledger timestamp on every write.
- **Per-developer isolation**: cap and accumulator are scoped to individual developer addresses.
- **Admin-only configuration**: only the current admin can modify caps, enforced by `require_auth`.
