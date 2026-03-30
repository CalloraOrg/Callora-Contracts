# Security Policy

## Checked Arithmetic

All smart contracts in this repository are developed with a "no silent wrap" policy for balance updates. 

### Implementation

- We use `checked_add` and `checked_sub` for all `i128` balance mutations.
- In the event of an overflow or underflow, the contract will immediately panic with a descriptive message (e.g., `"balance overflow"`, `"total amount overflow"`), causing the transaction to revert.
- The `overflow-checks = true` setting is enabled in `Cargo.toml` for both `dev` and `release` profiles as an additional safety layer, though explicit checked arithmetic is preferred for clarity and deterministic error messages.

### Affected Contracts

- **`callora-vault`**: `balance` increases during `deposit` and decreases during `deduct`.
- **`callora-settlement`**: `total_amount` is calculated during `batch_distribute` to ensure the contract holds sufficient funds.

## Reporting a Vulnerability

If you find a security issue, please do not open a public issue. Instead, contact the maintainers at security@callora.org.
