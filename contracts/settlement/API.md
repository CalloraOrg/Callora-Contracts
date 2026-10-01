# Settlement API Notes

## `simulate_claim(developer, amount, to)`

`simulate_claim` is a read-only view that previews the result of a developer
claim through `withdraw_developer_balance` without side effects. It returns a
`ClaimSimulation` record containing the simulated recipient, configured USDC
token, current developer balance, remaining balance, contract token balance,
daily cap, current same-day withdrawal amount, and same-day withdrawal amount
after the simulated claim.

The view performs the same claim validations for positive amount, claim window,
developer balance, daily withdrawal cap, configured USDC token, and settlement
contract token liquidity. It does not require developer authorization, transfer
tokens, mutate storage, extend TTLs, or emit events.
