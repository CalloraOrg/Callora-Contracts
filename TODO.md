# Plan: #891 — Add cargo-fuzz target for batch_distribute [b#066]

## Completed Steps ✓

### Phase 1: Add `batch_distribute` entrypoint
- [x] 1. Add batch events (`batch_distribute_started`, `batch_distribute_completed`) to `contracts/distribute/src/events.rs`
- [x] 2. Add `batch_distribute` function to `contracts/distribute/src/lib.rs`
- [x] 3. Export `pub mod limits` in `contracts/distribute/src/lib.rs`

### Phase 2: Create fuzz crate
- [x] 4. Create `contracts/batch_distribute/fuzz/Cargo.toml`
- [x] 5. Create `contracts/batch_distribute/fuzz/targets/main.rs` with comprehensive fuzz harness

### Phase 3: Workspace integration
- [x] 6. Register `contracts/batch_distribute/fuzz` in root `Cargo.toml` workspace members
- [x] 7. Build and validate compilation — `cargo check` and `cargo test` pass for both crates

## Summary of Changes

### Files Created
- `contracts/batch_distribute/fuzz/Cargo.toml` — fuzz crate configuration
- `contracts/batch_distribute/fuzz/targets/main.rs` — fuzz harness with 9 operation types

### Files Modified
- `contracts/distribute/src/lib.rs` — added `pub mod limits`, `batch_distribute()`, `get_max_batch_size()`
- `contracts/distribute/src/events.rs` — added `event_batch_distribute_started()`, `event_batch_distribute_completed()`
- `Cargo.toml` — added `contracts/batch_distribute/fuzz` to workspace members, removed stale `contracts/admin` from default-members

