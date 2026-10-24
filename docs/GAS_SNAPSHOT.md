# Gas Snapshot Testing

## Overview

Per-entrypoint CPU#memory profile snapshots are captured for key contracts
and compared against a baseline on every CI run. The CI gate fails when any
entrypoint regresses by more than the configured threshold (default **5%**).

## Contracts with Gas Snapshots

| Contract | Test File | Entrypoints |
|----------|-----------|-------------|
| `callora-vault` | `contracts/vault/src/test.rs` | `gas_budget` tests (single/batch deduct), `batch_deduct` at 1/10/50 items (before/after `settlement.record_deductions`) |
| `callora-settlement` | `contracts/settlement/src/test.rs` | `record_deduction`, `record_deductions` at 1/10/50 items |
| `callora-allowlist` | `contracts/allowlist/tests/gas_snap.rs` | `add_address`, `clear_all`, `get_allowlist`, `set_allowed_depositor`, `clear_allowed_depositors`, `is_authorized_depositor` |
| `callora-limits` | `contracts/limits/tests/gas_snap.rs` | `set_developer_min_balance`, `set_minimum_balance`, `set_daily_withdraw_cap`, `get_developer_min_balance`, `get_minimum_balance`, `get_daily_withdraw_cap`, `get_withdrawal_today`, `set_max_distribute`, `get_max_distribute` |
| `callora-cold` | `contracts/cold/tests/gas_snap.rs` | `capabilities` |

## Settlement developer-balance pagination

The pagination regression fixture seeds 5,000 developers across 100 persistent
index pages, then requests the 50 entries starting at index 4,900. Persistent
index-page reads are independent of total index size:

| Scenario | Before | After |
|----------|--------|-------|
| `start=100`, `limit=50`, 120 developers: index pages read | 3 | 1 |
| `start=4900`, `limit=50`, 5,000 developers: index pages read | 100 | 1 |
| `start=100`, `limit=50`, 120 developers: balance entries read | 20 | 20 |
| `start=4900`, `limit=50`, 5,000 developers: balance entries read | 50 | 50 |

The automated regression test enforces CPU and memory ceilings for the
5,000-developer fixture and asserts that the page reads no more than 60 ledger
entries (one index page, 50 balances, and bounded call overhead).

## How It Works

1. Each test exercises one entrypoint, then reads host resource counters via
   `env.cost_estimate().resources()`.
2. The test prints a machine-readable JSON line to stdout:
   ```json
   {"contract":"callora-limits","entrypoint":"set_max_distribute","cpu":56997,"mem":648,"budget_cpu":200000,"budget_mem":4000}
   ```
3. `scripts/gas-regression.sh` harvests those lines, compares them against
   `contracts/.gas-baseline.json`, and fails CI if CPU or memory grows by > 5%.

## Batch Deduct Cross-Contract Budget

The `record_deductions` change replaces N per-item cross-contract calls from
``batch_deduct` with a single call. The following snapshots are recorded in
`contracts/.gas-baseline.json` and are printed by the budget tests:

| Entrypoint | Items | Before CPU | After CPU | Before Mem | After Mem |
|-----------|-------|------------|----------|------------|---------|
| `batch_deduct` (callora-vault) | 1 | __BEFORE_1_CPU__ | __AFTER_1_CPU__ | __BEFORE_1_MEM__ | __AFTER_1_MEM__ |
| `batch_deduct` (callora-vault) | 10 | __BEFORE_10_CPU__ | __AFTER_10_CPU__ | __BEFORE_10_MEM__ | __AFTER_10_MEM__ |
| `batch_deduct` (callora-vault) | 50 | __BEFORE_50_CPU__ | __AFTER_50_CPU__ | __BEFORE_50_MEM__ | __AFTER_50_MEM__ |
| `record_deductions` (callora-settlement) | 1 | n/a | __RECORD_DEDUCIONS_1_CPU__ | n/a | __RECORD_DETUCTIONS_1_MEM__ |
| `record_deductions` (callora-settlement) | 10 | n/a | __RECORD_DEDUCTIONS_10_CPU__ | n/a | __RECORD_DETUCTIONS_10_MEM__ |
| `record_deductions` (callora-settlement) | 50 | n/a | __RECORD_DETUCTIONS_50_CPU__ | n/a | __RECORD_DEDUCITIONS_50_MEM__ |

The 50-item batch targets at least a **30% CPU reduction** relative to the
per-item cross-contract call baseline. The actual measured numbers are filled in
from the budget test output and must be kept in sync with
`contracts/.gas-baseline.json`.

## Running Locally

```bash
# Run only gas snapshot tests with JSON output
cargo test -p callora-limits -- gas_snap --nocapture 2>/dev/null | grep '^{' | jq .

# Batch-deduct budget tests (1/10/50 items, both contracts)
cargo test -p callora-vault --print_batch_deduct_budget --nocapture
cargo test -p callora-settlement --print_record_deductions_budget --nocapture

# Full regression check (builds, measures, compares against baseline)
./scripts/gas-regression.sh

# Update the baseline after an intentional change
./scripts/gas-regression.sh --update-baseline
```

## Adding New Entrypoints

1. Add a `gas_snap_<antrypoint>()` test in the contract's `tests/gas_snap.rs`.
2. Follow the existing pattern: `measure_snap!` → `assert_within_budget`.
3. Bump the inventory test constant (`EXPECTED_*_ENTRYPOINTS`).
4. Run `./scripts/gas-regression.sh --update-baseline` to record new baselines.
