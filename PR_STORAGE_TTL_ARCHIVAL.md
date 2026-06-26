# Feature: Storage TTL & Archival Simulation Tests

## 🎯 Objective
Implementa extensive testing to simulate storage TTL behavior and archival boundary conditions for the `vault`, `revenue_pool`, and `settlement` smart contracts. This PR guarantees that instance storage lifecycles are correctly managed and documents the archival behaviors observed under the Soroban SDK v22.

## 🛠 What's Included

### 1. TTL Archival Simulation Test Suites
Added `test_ttl_archival.rs` to all three core contracts to simulate the passage of time using `env.ledger().with_mut(|li| li.sequence_number += N)`:
- **`vault`**: Confirmed `INSTANCE_BUMP_AMOUNT` (~60 days) logic. Reads do not extend TTL, but writes (like `deposit`) correctly reset the lifecycle window.
- **`revenue_pool`**: Confirmed `BUMP_AMOUNT` (~16 days) logic. Calling `distribute` safely resets the TTL window if called within the lifecycle limits.
- **`settlement`**: **Critical Discovery:** Uncovered that the Settlement contract **does not** call `extend_ttl` on its instance storage (only on persistent developer balances). Tests confirm that instance storage will archive after the network default minimum TTL (~4096 ledgers).

### 2. Soroban v22 Archival Panic Handling
Discovered that the Soroban v22 host enforces real archival in the test environment. Accessing an archived entry does not return a graceful `Result::Err` to the contract, but instead escalates immediately to a `HostError: Error(Storage, InternalError)` panic. 
- Implemented `std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ...))` wrappers in our tests to safely capture these panics and rigorously assert success/failure across TTL boundaries without failing the test suite.

### 3. Workspace Stabilization & Bug Fixes
Fixed several pre-existing test compilation failures across the workspace to ensure `cargo test --workspace` passes cleanly:
- **`revenue_pool`**: Wrapped an existing panicking test closure with `AssertUnwindSafe` to fix `UnsafeCell` compilation errors.
- **`vault`**: Resolved type mismatches (`&String` vs `String`), removed an invalid `.unwrap()` on a unit type, fixed `no_std` `format!` macro errors, and pruned unused variables/duplicate attributes.

### 4. Storage Documentation
- Created `STORAGE.md` at the workspace root to document all TTL constants, lifecycle windows, and archival extension rules for all contracts, explicitly noting the Settlement contract's production archival risk.

## ⚠️ Important Note for Reviewers
The **Settlement contract** does not extend its instance TTL. In a production network, if no admin operations bump the contract's instance, it will be archived after the network's `min_persistent_entry_ttl`. Please review `STORAGE.md` for details and consider if we need a fast-follow PR to add `extend_ttl` to `receive_payment`.

## ✅ Checklist
- [x] All workspace tests pass (`cargo test --workspace`)
- [x] Test coverage maintained/improved
- [x] `STORAGE.md` documentation added
- [x] No `clippy` warnings

---
*Testing performed locally on Soroban SDK v22 testutils sandbox.*
