# Tasks: Issue #717 — Allowlist Error Enum Expansion

**Issue Reference:** #717  
**Status:** Ready for Execution  
**Date:** 2026-07-26

---

## Task Breakdown

### Task 1: Add New Error Variant to VaultError Enum
**File:** `contracts/vault/src/errors.rs`  
**Estimated effort:** 15 minutes  
**Dependencies:** None

**Sub-tasks:**
1.1. Add `CallerNotInAllowlist = 44` variant to the `VaultError` enum with rustdoc comment following existing style
1.2. Update the file-level discriminant table to include code 44
1.3. Add variant to the duplicate enum at top of lib.rs (lines 67-140)

**Acceptance criteria:**
- New variant has discriminant 44
- Rustdoc includes: summary, "When returned" section, "Security note" section
- Table in file-level docs updated
- Code compiles without errors

**Implementation notes:**
```rust
/// Caller is not in the allowlist and is not the owner (code 44).
///
/// # When returned
/// - `deposit()` when a non-owner address attempts to deposit and the caller
///   is not present in the configured allowlist.
///
/// # Security note
/// Returned instead of panicking to provide machine-readable feedback to
/// integrators and prevent information leakage.
CallerNotInAllowlist = 44,
```

---

### Task 2: Add AllowedDepositors Storage Key
**File:** `contracts/vault/src/lib.rs`  
**Estimated effort:** 10 minutes  
**Dependencies:** None

**Sub-tasks:**
2.1. Add `AllowedDepositors` variant to the `StorageKey` enum (around line 180)
2.2. Add rustdoc comment explaining the storage structure

**Acceptance criteria:**
- New storage key added to enum
- Rustdoc comment describes: "Vector of addresses allowed to deposit (owner-managed allowlist)"
- Code compiles without errors

**Implementation notes:**
```rust
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    // ... existing variants ...
    /// Vector of addresses allowed to deposit (owner-managed allowlist).
    AllowedDepositors,
}
```

---

### Task 3: Add Event Functions for Allowlist Operations
**File:** `contracts/vault/src/events.rs`  
**Estimated effort:** 15 minutes  
**Dependencies:** None

**Sub-tasks:**
3.1. Add `event_allowlist_add` function returning Symbol
3.2. Add `event_allowlist_clear` function returning Symbol
3.3. Add rustdoc comments for both functions with event schema

**Acceptance criteria:**
- Both functions return `Symbol::new(env, "event_name")`
- Rustdoc includes topics and data schema
- Code compiles without errors

**Implementation notes:**
```rust
/// Emitted when an address is added to the deposit allowlist.
///
/// Topics: ("allowlist_add", caller: Address, depositor: Address)
/// Data: ()
pub fn event_allowlist_add(env: &Env) -> Symbol {
    Symbol::new(env, "allowlist_add")
}

/// Emitted when the deposit allowlist is cleared.
///
/// Topics: ("allowlist_clear", caller: Address)
/// Data: ()
pub fn event_allowlist_clear(env: &Env) -> Symbol {
    Symbol::new(env, "allowlist_clear")
}
```

---

### Task 4: Refactor Validation Helper Functions
**File:** `contracts/vault/src/lib.rs`  
**Estimated effort:** 30 minutes  
**Dependencies:** Task 1

**Sub-tasks:**
4.1. Change `require_positive_amount` signature to return `Result<(), VaultError>`
4.2. Change `require_valid_deposit_amount` signature to return `Result<(), VaultError>`
4.3. Change `require_valid_deduct_amount` signature to return `Result<(), VaultError>`
4.4. Replace panic calls with `return Err(VaultError::Variant)`

**Acceptance criteria:**
- All three functions return `Result<(), VaultError>`
- Panics replaced with appropriate error variants
- Function bodies use `?` operator where needed
- Code compiles without errors

**Implementation notes:**
```rust
fn require_positive_amount(amount: i128) -> Result<(), VaultError> {
    if amount <= 0 {
        return Err(VaultError::AmountNotPositive);
    }
    Ok(())
}

fn require_valid_deposit_amount(amount: i128, min_deposit: i128) -> Result<(), VaultError> {
    Self::require_positive_amount(amount)?;
    if amount < min_deposit {
        return Err(VaultError::BelowMinDeposit);
    }
    Ok(())
}

fn require_valid_deduct_amount(amount: i128, min_amount: i128, max_deduct: i128) -> Result<(), VaultError> {
    Self::require_positive_amount(amount)?;
    if amount < min_amount {
        return Err(VaultError::BelowMinDeposit);
    }
    if amount > max_deduct {
        return Err(VaultError::ExceedsMaxDeduct);
    }
    Ok(())
}
```

---

### Task 5: Implement add_address Function
**File:** `contracts/vault/src/lib.rs`  
**Estimated effort:** 45 minutes  
**Dependencies:** Task 1, Task 2, Task 3

**Sub-tasks:**
5.1. Add `add_address` function with full signature and rustdoc
5.2. Implement owner authentication via `require_owner`
5.3. Implement Vec retrieval/creation logic
5.4. Implement duplicate check (idempotent behavior)
5.5. Implement Vec update and storage
5.6. Implement event emission

**Acceptance criteria:**
- Function signature matches design
- Rustdoc includes: summary, parameters, returns, events, examples
- Owner-only access enforced
- Idempotent (duplicate adds succeed silently)
- Event emitted on every call
- Code compiles without errors

**Implementation notes:**
```rust
/// Add a single address to the deposit allowlist (owner-only).
///
/// If the address is already present, this function is idempotent and will
/// succeed without error. The `allowlist_add` event is emitted even for
/// duplicate adds to maintain audit trail clarity.
///
/// # Parameters
/// - `caller` — Must be the vault owner (verified via `require_owner`)
/// - `depositor` — Address to add to the allowlist
///
/// # Returns
/// `Ok(())` on success, or `VaultError::Unauthorized` if caller is not owner.
///
/// # Events
/// Emits `("allowlist_add", caller, depositor)` on every successful call,
/// including duplicates.
///
/// # Examples
/// ```ignore
/// vault.add_address(&owner, &backend_service_1)?;
/// vault.add_address(&owner, &backend_service_2)?;
/// ```
pub fn add_address(
    env: Env,
    caller: Address,
    depositor: Address,
) -> Result<(), VaultError> {
    caller.require_auth();
    Self::require_owner(env.clone(), caller.clone())?;
    
    let mut allowlist = env
        .storage()
        .instance()
        .get::<_, Vec<Address>>(&StorageKey::AllowedDepositors)
        .unwrap_or_else(|| Vec::new(&env));
    
    // Idempotent: only add if not already present
    if !allowlist.contains(&depositor) {
        allowlist.push_back(depositor.clone());
        env.storage()
            .instance()
            .set(&StorageKey::AllowedDepositors, &allowlist);
    }
    
    env.events().publish(
        (events::event_allowlist_add(&env), caller, depositor),
        ()
    );
    
    Ok(())
}
```

---

### Task 6: Implement clear_all Function
**File:** `contracts/vault/src/lib.rs`  
**Estimated effort:** 30 minutes  
**Dependencies:** Task 1, Task 2, Task 3

**Sub-tasks:**
6.1. Add `clear_all` function with full signature and rustdoc
6.2. Implement owner authentication via `require_owner`
6.3. Implement storage removal (idempotent)
6.4. Implement event emission

**Acceptance criteria:**
- Function signature matches design
- Rustdoc includes: summary, parameters, returns, events, examples
- Owner-only access enforced
- Idempotent (clearing empty allowlist succeeds)
- Event emitted on every call
- Code compiles without errors

**Implementation notes:**
```rust
/// Remove all addresses from the deposit allowlist (owner-only).
///
/// This function is idempotent — calling it on an empty allowlist succeeds
/// without error.
///
/// # Parameters
/// - `caller` — Must be the vault owner (verified via `require_owner`)
///
/// # Returns
/// `Ok(())` on success, or `VaultError::Unauthorized` if caller is not owner.
///
/// # Events
/// Emits `("allowlist_clear", caller)` on every successful call.
///
/// # Examples
/// ```ignore
/// vault.clear_all(&owner)?;
/// ```
pub fn clear_all(
    env: Env,
    caller: Address,
) -> Result<(), VaultError> {
    caller.require_auth();
    Self::require_owner(env.clone(), caller.clone())?;
    
    env.storage()
        .instance()
        .remove(&StorageKey::AllowedDepositors);
    
    env.events().publish(
        (events::event_allowlist_clear(&env), caller),
        ()
    );
    
    Ok(())
}
```

---

### Task 7: Implement get_allowlist Function
**File:** `contracts/vault/src/lib.rs`  
**Estimated effort:** 20 minutes  
**Dependencies:** Task 2

**Sub-tasks:**
7.1. Add `get_allowlist` function with full signature and rustdoc
7.2. Implement Vec retrieval logic (return empty if not exists)

**Acceptance criteria:**
- Function signature matches design
- Rustdoc includes: summary, returns, examples
- No authentication required (public read)
- Returns empty Vec if allowlist not configured
- No event emission
- Code compiles without errors

**Implementation notes:**
```rust
/// Return the current deposit allowlist.
///
/// No authentication required — this is a public read-only view function.
/// Addresses are returned in insertion order.
///
/// # Returns
/// `Vec<Address>` containing all addresses currently in the allowlist.
/// Returns an empty vector if no allowlist has been configured.
///
/// # Examples
/// ```ignore
/// let allowed = vault.get_allowlist();
/// assert_eq!(allowed.len(), 3);
/// ```
pub fn get_allowlist(env: Env) -> Vec<Address> {
    env.storage()
        .instance()
        .get::<_, Vec<Address>>(&StorageKey::AllowedDepositors)
        .unwrap_or_else(|| Vec::new(&env))
}
```

---

### Task 8: Update init Function to Return Result
**File:** `contracts/vault/src/lib.rs`  
**Estimated effort:** 30 minutes  
**Dependencies:** Task 4

**Sub-tasks:**
8.1. Change `init` signature to return `Result<(), VaultError>`
8.2. Replace panic calls with `return Err(...)` using appropriate variants
8.3. Update helper function calls to use `?` operator

**Acceptance criteria:**
- Function returns `Result<(), VaultError>`
- All panics replaced with typed errors
- Helper validation uses `?` operator
- Code compiles without errors

**Panic replacements:**
- "Already initialized" → `VaultError::AlreadyInitialized`
- "min_deposit must be positive" → `VaultError::MinDepositNotPositive`
- "max_deduct must be positive" → `VaultError::MaxDeductNotPositive`
- "min_deposit cannot exceed max_deduct" → `VaultError::MinDepositExceedsMaxDeduct`

---

### Task 9: Update deposit Function with Vec-Based Allowlist
**File:** `contracts/vault/src/lib.rs`  
**Estimated effort:** 45 minutes  
**Dependencies:** Task 1, Task 2, Task 4

**Sub-tasks:**
9.1. Change `deposit` signature to return `Result<(), VaultError>`
9.2. Replace "Contract paused" panic with `Paused` error
9.3. Update validation helper calls to use `?` operator
9.4. Replace allowlist check logic to use Vec storage
9.5. Replace "Not authorized depositor" panic with `CallerNotInAllowlist` error
9.6. Replace overflow unwrap with checked_add and error return

**Acceptance criteria:**
- Function returns `Result<(), VaultError>`
- All panics replaced with typed errors
- Allowlist check uses `StorageKey::AllowedDepositors` Vec
- Owner bypass logic preserved
- Code compiles without errors

**Key changes:**
```rust
// OLD allowlist check (lines 323-330):
let is_allowed = env
    .storage()
    .instance()
    .get::<_, bool>(&DataKey::Depositor(caller.clone()))
    .unwrap_or(false);
if !is_allowed {
    panic!("Not authorized depositor");
}

// NEW allowlist check:
let allowlist = env
    .storage()
    .instance()
    .get::<_, Vec<Address>>(&StorageKey::AllowedDepositors)
    .unwrap_or_else(|| Vec::new(&env));

if !allowlist.contains(&caller) {
    return Err(VaultError::CallerNotInAllowlist);
}
```

---

### Task 10: Update deduct Function to Return Result
**File:** `contracts/vault/src/lib.rs`  
**Estimated effort:** 30 minutes  
**Dependencies:** Task 4

**Sub-tasks:**
10.1. Change `deduct` signature to return `Result<(), VaultError>`
10.2. Replace panic calls with typed errors
10.3. Update validation helper calls to use `?` operator

**Acceptance criteria:**
- Function returns `Result<(), VaultError>`
- All panics replaced (Unauthorized, Paused, InsufficientBalance)
- Code compiles without errors

---

### Task 11: Update batch_deduct Function to Return Result
**File:** `contracts/vault/src/lib.rs`  
**Estimated effort:** 30 minutes  
**Dependencies:** Task 4

**Sub-tasks:**
11.1. Change `batch_deduct` signature to return `Result<(), VaultError>`
11.2. Replace panic calls with typed errors
11.3. Replace `.unwrap_or_else(|| panic!("overflow"))` with `checked_add().ok_or(VaultError::Overflow)?`
11.4. Update validation helper calls to use `?` operator

**Acceptance criteria:**
- Function returns `Result<(), VaultError>`
- All panics replaced (Unauthorized, Paused, InsufficientBalance, Overflow)
- Overflow protection uses checked arithmetic
- Code compiles without errors

---

### Task 12: Update Remaining Owner-Gated Functions
**File:** `contracts/vault/src/lib.rs`  
**Estimated effort:** 45 minutes  
**Dependencies:** Task 1

**Sub-tasks:**
12.1. Update `set_authorized_caller` to return Result
12.2. Update `pause` to return Result
12.3. Update `unpause` to return Result
12.4. Update `set_max_deduct` to return Result and replace validation panic
12.5. Update `set_settlement` to return Result

**Acceptance criteria:**
- All functions return `Result<(), VaultError>`
- "Not owner" panics replaced with `Unauthorized` error
- Validation panics replaced with appropriate error variants
- Code compiles without errors

---

### Task 13: Implement Allowlist Tests - Basic Functionality
**File:** `contracts/vault/src/test.rs`  
**Estimated effort:** 1 hour  
**Dependencies:** Task 5, Task 6, Task 7

**Sub-tasks:**
13.1. Create `#[cfg(test)] mod allowlist_tests` module
13.2. Add test helper function `setup_vault_with_allowlist`
13.3. Implement `test_add_address_adds_single_depositor`
13.4. Implement `test_add_address_prevents_duplicates`
13.5. Implement `test_add_address_multiple_depositors`
13.6. Implement `test_clear_all_removes_all_depositors`
13.7. Implement `test_clear_all_idempotent`

**Acceptance criteria:**
- 5 tests implemented and passing
- Tests use `env.mock_all_auths()` for owner authentication
- Tests verify Vec contents via `get_allowlist()`
- Code compiles and tests pass

---

### Task 14: Implement Allowlist Tests - Access Control & Events
**File:** `contracts/vault/src/test.rs`  
**Estimated effort:** 45 minutes  
**Dependencies:** Task 5, Task 6, Task 3

**Sub-tasks:**
14.1. Implement `test_add_address_non_owner_fails`
14.2. Implement `test_clear_all_non_owner_fails`
14.3. Implement `test_add_address_emits_event`
14.4. Implement `test_clear_all_emits_event`

**Acceptance criteria:**
- 4 tests implemented and passing
- Access control tests verify `Err(VaultError::Unauthorized)`
- Event tests use `env.events().all()` to verify emission
- Code compiles and tests pass

---

### Task 15: Implement Allowlist Tests - Query & Integration
**File:** `contracts/vault/src/test.rs`  
**Estimated effort:** 1 hour  
**Dependencies:** Task 7, Task 9

**Sub-tasks:**
15.1. Implement `test_get_allowlist_returns_empty_when_not_set`
15.2. Implement `test_get_allowlist_returns_all_addresses`
15.3. Implement `test_owner_always_permitted_regardless_of_allowlist`
15.4. Implement `test_depositor_in_allowlist_can_deposit`
15.5. Implement `test_depositor_not_in_allowlist_fails_with_correct_error`
15.6. Implement `test_deposit_after_clear_all_fails`
15.7. Implement `test_add_address_after_clear_all`
15.8. Implement `test_error_code_stability` (verify CallerNotInAllowlist = 44)

**Acceptance criteria:**
- 8 tests implemented and passing
- Integration tests create full vault setup with USDC token
- Error tests verify exact error variant and discriminant
- Owner privilege tests confirm bypass behavior
- Code compiles and tests pass

---

### Task 16: Update Existing Tests for Result Returns
**File:** `contracts/vault/src/test.rs` and other test files  
**Estimated effort:** 1.5 hours  
**Dependencies:** Tasks 8-12

**Sub-tasks:**
16.1. Search for all calls to updated functions in test code
16.2. Add `.unwrap()` or `?` to handle Result returns
16.3. Update tests that expect panics to expect Err(VaultError::...)
16.4. Fix any test compilation errors

**Acceptance criteria:**
- All existing tests compile
- All existing tests pass
- Tests correctly handle Result returns
- No unwrap() on production code, only in tests

**Search patterns:**
- `.init(` → add `.unwrap()` or `?`
- `.deposit(` → add `.unwrap()` or `?`
- `.deduct(` → add `.unwrap()` or `?`
- etc.

---

### Task 17: Run Full Test Suite and Verification
**Estimated effort:** 30 minutes  
**Dependencies:** All previous tasks

**Sub-tasks:**
17.1. Run `cargo fmt --all`
17.2. Run `cargo clippy --all-targets -- -D warnings`
17.3. Run `cargo test -p callora-vault`
17.4. Run `cargo build --target wasm32-unknown-unknown --release -p callora-vault`
17.5. Generate coverage report with tarpaulin
17.6. Verify 95%+ coverage on vault contract

**Acceptance criteria:**
- `cargo fmt` makes no changes (code already formatted)
- `cargo clippy` reports zero warnings
- All tests pass (existing + 17 new tests)
- WASM build succeeds with no errors
- Coverage ≥ 95% on `contracts/vault/src/`

**Commands:**
```bash
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test -p callora-vault
cargo build --target wasm32-unknown-unknown --release -p callora-vault
cargo tarpaulin --out Html --output-dir coverage -- -p callora-vault
```

---

## Task Execution Order

**Phase 1: Foundation (Parallel)**
- Task 1: Error variant
- Task 2: Storage key
- Task 3: Event functions

**Phase 2: Helpers (Sequential)**
- Task 4: Validation helpers (depends on Task 1)

**Phase 3: Allowlist Functions (Parallel after Phase 2)**
- Task 5: add_address (depends on Tasks 1, 2, 3)
- Task 6: clear_all (depends on Tasks 1, 2, 3)
- Task 7: get_allowlist (depends on Task 2)

**Phase 4: Core Function Updates (Parallel after Phase 2)**
- Task 8: init (depends on Task 4)
- Task 9: deposit (depends on Tasks 1, 2, 4)
- Task 10: deduct (depends on Task 4)
- Task 11: batch_deduct (depends on Task 4)
- Task 12: Owner functions (depends on Task 1)

**Phase 5: Tests (Parallel after Phase 3 & 4)**
- Task 13: Basic tests (depends on Tasks 5, 6, 7)
- Task 14: Access control tests (depends on Tasks 5, 6, 3)
- Task 15: Integration tests (depends on Tasks 7, 9)
- Task 16: Update existing tests (depends on Tasks 8-12)

**Phase 6: Verification (Sequential, after all)**
- Task 17: Full verification

**Total estimated effort:** ~10-12 hours

---

## Success Criteria Summary

✅ All 17 tasks completed  
✅ All 23 panics replaced with typed errors  
✅ New error variant added (CallerNotInAllowlist = 44)  
✅ Three allowlist functions implemented  
✅ 17 new tests added and passing  
✅ All existing tests updated and passing  
✅ 95%+ line coverage achieved  
✅ Zero clippy warnings  
✅ WASM builds successfully  
✅ Events emitted correctly  
✅ Owner privilege preserved  

---

**Status:** ✅ Tasks Ready for Execution  
**Next:** Execute tasks via orchestrator delegation
