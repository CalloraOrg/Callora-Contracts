# Vault Operation Gas / Cost Notes

Approximate resource usage for Callora Vault operations to guide integration and capacity planning. Soroban uses resource metering (CPU instructions, ledger reads/writes, events). Exact numbers depend on network fee configuration and should be validated on testnet or via `soroban contract invoke` simulation.

> **Disclaimer**: The numbers provided below are estimates and should be used contextually. Exact costs diverge between testnet and mainnet depending on real-time network conditions.

## Methodology

Cost estimations are derived by running transaction simulations through `soroban contract invoke --simulate` on recent testnet deployments. Simulated operations log CPU/instruction costs, ledger entry reads/writes, event size, and network fee parameters.

## Relative Cost (typical order)

| Operation | Relative cost | Notes | Estimated CPU Instructions (Testnet)* |
|---|---|---|---|
| `balance()` | Lowest | Single instance read, no writes, no event. | < 500k |
| `get_meta()` | Low | Same as balance (reads full meta). | < 500k |
| `deposit` | Medium | One read, one write, one event. Cross-contract call to USDC. | ~ 2.5M |
| `deduct` | Medium | One read, one write, one event. May cross-call Settlement pool. | ~ 2.8M |
| `withdraw` | Medium | One read, one write, one event. Cross-contract call to USDC. | ~ 2.5M |
| `withdraw_to` | Medium | One read, one write, one event. Cross-contract call to USDC. | ~ 2.5M |
| `distribute`| Medium | One read, one write, one event. Cross-contract call to USDC. | ~ 2.5M |
| `receive_payment`| Low | One event emission. Validates caller and emits an event. | ~ 1.0M |
| `batch_deduct` | Medium–High | One read, one write, N events (one per item). Bulk process. | ~ 3.5M + (100k per item) |
| `init` | Highest | First write (create instance), one event; requires auth. | ~ 4.5M |

*\*These are purely structural estimates. Actual costs fluctuate and must be simulated per deployment.*

## Obtaining Exact Numbers

- **Testnet**: Deploy the vault and invoke each operation; inspect transaction meta for instructions and fee.
- **CLI**: Use `soroban contract invoke` with `--simulate` (or equivalent) and check returned resource/fee info.
- **Test env**: Run the optional benchmark test: `cargo test --ignored vault_operation_costs -- --nocapture`. This logs CPU/instruction and fee estimates per operation when invocation cost metering is enabled in the test environment.

## Fee Configuration

Soroban fees are configured per network (e.g. Pubnet). They are applied to:

- CPU instructions (per increment)
- Ledger entry reads and writes
- Event size
- Transaction size
- Rent for persistent/temporary storage

See [Stellar documentation on Fees and Resource Limits](https://developers.stellar.org/docs/encyclopedia/fees-and-resource-limits) for current fee parameters and a detailed breakdown of metering operations.
