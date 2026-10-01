# Gas Snapshot Testing

## Overview

Per-entrypoint CPU/memory profile snapshots are captured for key contracts
and compared against a baseline on every CI run. The CI gate fails when any
entrypoint regresses by more than the configured threshold (default **5%**).

## Contracts with Gas Snapshots

| Contract | Test File | Entrypoints |
|----------|-----------|-------------|
| `callora-vault` | `contracts/vault/src/test.rs` | `gas_budget` tests (single/batch deduct) |
| `callora-allowlist` | `contracts/allowlist/tests/gas_snap.rs` | `add_address`, `clear_all`, `get_allowlist`, `set_allowed_depositor`, `clear_allowed_depositors`, `is_authorized_depositor` |
| `callora-limits` | `contracts/limits/tests/gas_snap.rs` | `set_developer_min_balance`, `set_minimum_balance`, `set_daily_withdraw_cap`, `get_developer_min_balance`, `get_minimum_balance`, `get_daily_withdraw_cap`, `get_withdrawal_today`, `set_max_distribute`, `get_max_distribute` |
| `callora-cold` | `contracts/cold/tests/gas_snap.rs` | `capabilities` |

## How It Works

1. Each test exercises one entrypoint, then reads host resource counters via
   `env.cost_estimate().resources()`.
2. The test prints a machine-readable JSON line to stdout:
   ```json
   {"contract":"callora-limits","entrypoint":"set_max_distribute","cpu":56997,"mem":648,"budget_cpu":200000,"budget_mem":4000}
   ```
3. `scripts/gas-regression.sh` harvests those lines, compares them against
   `contracts/.gas-baseline.json`, and fails CI if CPU or memory grows by > 5%.

## Running Locally

```bash
# Run only gas snapshot tests with JSON output
cargo test -p callora-limits -- gas_snap --nocapture 2>/dev/null | grep '^{' | jq .

# Full regression check (builds, measures, compares against baseline)
./scripts/gas-regression.sh

# Update the baseline after an intentional change
./scripts/gas-regression.sh --update-baseline
```

## Adding New Entrypoints

1. Add a `gas_snap_<entrypoint>()` test in the contract's `tests/gas_snap.rs`.
2. Follow the existing pattern: `measure_snap!` → `assert_within_budget`.
3. Bump the inventory test constant (`EXPECTED_*_ENTRYPOINTS`).
4. Run `./scripts/gas-regression.sh --update-baseline` to record new baselines.
