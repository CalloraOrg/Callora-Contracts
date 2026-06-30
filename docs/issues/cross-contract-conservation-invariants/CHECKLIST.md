# Implementation Checklist

Work through each item in order. Check it off before moving to the next.

## Settlement contract

- [ ] Add `TotalReceived` variant to `StorageKey` enum
- [ ] In `receive_payment`: after crediting developer or pool, do
      `total = get_total_received() + amount` (checked_add, panic on overflow),
      write back to `StorageKey::TotalReceived`
- [ ] Same increment in `batch_receive_payment` (once per item, same pattern)
- [ ] Add `pub fn get_total_received(env: Env) -> i128` (returns 0 if key absent)
- [ ] Add unit test: single receive → `get_total_received() == amount`
- [ ] Add unit test: multiple receives → running total matches sum

## Vault contract

- [ ] Add `TotalDeducted` variant to `StorageKey` enum
- [ ] In `deduct`: after `meta.balance` update, increment `TotalDeducted` (checked_add)
- [ ] In the batch loop in `batch_deduct`: accumulate per-item amounts, then
      increment `TotalDeducted` once after the loop commits
- [ ] Add `pub fn get_total_deducted(env: Env) -> i128` (returns 0 if key absent)
- [ ] Add unit test: deduct → `get_total_deducted() == amount`
- [ ] Add unit test: batch_deduct → `get_total_deducted() == sum of items`

## Cross-contract test

- [ ] Create `contracts/vault/src/test_cross_invariant.rs`
- [ ] Register vault + settlement (+ optional revenue pool stub) in one env
- [ ] Run 64-seed × 48-step random sequence
- [ ] After every step assert:
      `vault.get_total_deducted() == settlement.get_total_received()`
- [ ] After every step assert settlement internal conservation:
      `settlement.get_total_received()
         == Σ developer_balance + global_pool.total_balance + cumulative_withdrawn`
- [ ] Add `#[cfg(test)] mod test_cross_invariant;` to `contracts/vault/src/lib.rs`

## Docs

- [ ] `docs/interfaces/vault.json`: add `get_total_deducted` function entry
- [ ] `docs/interfaces/settlement.json`: add `get_total_received` function entry

## Final checks

- [ ] `cargo fmt --all -- --check` passes
- [ ] `cargo clippy --all-targets --all-features -- -D warnings` passes
- [ ] `cargo test --workspace` green (all existing + new tests)
- [ ] Coverage ≥ 95% (`./scripts/coverage.sh`)
