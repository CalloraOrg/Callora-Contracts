# Revenue Pool Yield Deposits

`deposit_yield(treasury, amount, source)` lets the current revenue-pool admin
deposit accumulated protocol earnings into the pool through one audited
entrypoint.

## Behavior

- `treasury` must be the current admin and must authorize the call.
- `amount` must be positive and is transferred from `treasury` to the revenue
  pool contract using the configured USDC token contract.
- `source` is a short Soroban `Symbol` label for indexers, such as `fees` or
  `yield`.
- `get_cumulative_yield_deposited()` returns the total amount deposited through
  this entrypoint.

## Event

Each successful deposit emits:

```text
topics: ["yield_deposited", treasury]
data:   (amount, source, cumulative_yield_deposited)
```

The metric update, token transfer, and event emission are part of the same
Soroban transaction. If the transfer fails, the metric and event are reverted
with the rest of the transaction.
