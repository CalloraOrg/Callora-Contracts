# Design: Issue #717 — Allowlist Error Enum Expansion

**Issue Reference:** #717  
**Status:** Design Complete  
**Date:** 2026-07-26  
**Contract:** Callora Vault (`contracts/vault`)

---

## Design Overview

This design implements semantic error handling for the vault's allowlist functionality by:
1. Adding new `VaultError` variant for allowlist-specific failures
2. Implementing three allowlist management functions
3. Replacing all 23 generic panics with typed `Result<T, VaultError>` returns
4. Adding comprehensive test coverage

---

## Architecture Decisions

### 1. Storage Architecture

**Decision:** Single Vec storage model

**Implementation:**
```rust
// Add to StorageKey enum in lib.rs:
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    // ... existing variants ...
    /// Vector of addresses allowed to deposit (owner-managed allowlist).
    AllowedDepositors,
}
```

**Storage Layout:**
- **Key:** `StorageKey::AllowedDepositors`
- **Value:** `Vec<Address>` (Soroban SDK Vec)
- **Typical size:** 1-10 addresses (backend services)
- **Max size:** Unbounded (reasonable for expected use case)

**Deprecation:**
- `DataKey::Depositor(Address)` will no longer be used for new allowlist entries
- No migration needed (feature doesn't exist in deployed contracts)

---

### 2. Error Enum Expansion

**New Error Variant:**

```rust
// Add to VaultError enum in errors.rs:

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

**Updated discriminant table in errors.rs file-level doc:**
```rust
/// | 44   | CallerNotInAllowlist           | Caller not in allowlist and not owner            |
```

---

### 3. Function Signatures

#### 3.1 `add_address`

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
/// // Duplicate add succeeds silently:
/// vault.add_address(&owner, &backend_service_1)?;
/// ```
pub fn add_address(
    env: Env,
    caller: Address,
    depositor: Address,
) -> Result<(), VaultError>
```

**Implementation logic:**
1. Call `caller.require_auth()`
2. Call `Self::require_owner(env.clone(), caller.clone())?`
3. Get `StorageKey::AllowedDepositors` Vec (or empty Vec if not exists)
4. Check if `depositor` already in Vec (linear scan)
5. If not present, push to Vec and save back to storage
6. Emit event `("allowlist_add", caller, depositor)`
7. Return `Ok(())`

#### 3.2 `clear_all`

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
/// Emits `("allowlist_clear", caller)` on every successful call, even when
/// the allowlist is already empty.
///
/// # Examples
/// ```ignore
/// vault.clear_all(&owner)?;
/// // Subsequent calls succeed (idempotent):
/// vault.clear_all(&owner)?;
/// ```
pub fn clear_all(
    env: Env,
    caller: Address,
) -> Result<(), VaultError>
```

**Implementation logic:**
1. Call `caller.require_auth()`
2. Call `Self::require_owner(env.clone(), caller.clone())?`
3. Remove `StorageKey::AllowedDepositors` from storage (idempotent)
4. Emit event `("allowlist_clear", caller)`
5. Return `Ok(())`

#### 3.3 `get_allowlist`

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
pub fn get_allowlist(env: Env) -> Vec<Address>
```

**Implementation logic:**
1. Get `StorageKey::AllowedDepositors` Vec
2. Return Vec or empty Vec if key doesn't exist
3. No authentication, no event emission

#### 3.4 Modified `deposit` function

**Current signature** (unchanged):
```rust
pub fn deposit(env: Env, caller: Address, amount: i128)
```

**Changes to implementation** (lines 300-345):
1. Replace panic strings with `return Err(VaultError::Variant)`
2. Change allowlist check logic:

**OLD (line 323-330):**
```rust
if caller != owner {
    let is_allowed = env
        .storage()
        .instance()
        .get::<_, bool>(&DataKey::Depositor(caller.clone()))
        .unwrap_or(false);
    if !is_allowed {
        panic!("Not authorized depositor");
    }
}
```

**NEW:**
```rust
if caller != owner {
    let allowlist = env
        .storage()
        .instance()
        .get::<_, Vec<Address>>(&StorageKey::AllowedDepositors)
        .unwrap_or_else(|| Vec::new(&env));
    
    if !allowlist.contains(&caller) {
        return Err(VaultError::CallerNotInAllowlist);
    }
}
```

---

### 4. Panic Replacement Strategy

**Scope:** Replace all 23 production panics with typed errors.

**Strategy:**
1. Change function return types from `()` to `Result<(), VaultError>`
2. Replace `panic!(msg)` with `return Err(VaultError::Variant)`
3. Use existing error variants where semantically appropriate
4. Propagate errors with `?` operator in calling functions

**Panic → Error Mapping:**

| Panic String | Line(s) | Error Variant | Action |
|--------------|---------|---------------|--------|
| "amount must be positive" | 229 | `AmountNotPositive` | Change helper to return Result |
| "deposit below minimum" | 236 | `BelowMinDeposit` | Change helper to return Result |
| "deduct below minimum" | 243 | `BelowMinDeposit` | Change helper to return Result |
| "deduct amount exceeds max_deduct" | 246 | `ExceedsMaxDeduct` | Change helper to return Result |
| "Already initialized" | 262 | `AlreadyInitialized` | Return error |
| "min_deposit must be positive" | 265 | `MinDepositNotPositive` | Return error |
| "max_deduct must be positive" | 268, 651 | `MaxDeductNotPositive` | Return error |
| "min_deposit cannot exceed max_deduct" | 271 | `MinDepositExceedsMaxDeduct` | Return error |
| "Contract paused" | 308, 363, 419 | `Paused` | Return error |
| **"Not authorized depositor"** | 328 | **`CallerNotInAllowlist`** | **Return new error** |
| "Not authorized caller" | 355, 411 | `Unauthorized` | Return error |
| "insufficient balance" | 382, 443 | `InsufficientBalance` | Return error |
| "overflow" | 435 | `Overflow` | Return error from checked_add |
| "Not owner" | 568, 583, 596, 648, 673 | `Unauthorized` | Return error |

**Note:** Some functions already return `Result<(), VaultError>` (admin functions). Others need signature changes.

---

### 5. Event Schema

**Events to add to `events.rs`:**

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

**Event emission locations:**
1. `add_address` → emit `allowlist_add`
2. `clear_all` → emit `allowlist_clear`
3. `get_allowlist` → no event (read-only)

---

### 6. Test Architecture

**Test organization:**
- Add tests to `contracts/vault/src/test.rs`
- Group tests under module `#[cfg(test)] mod allowlist_tests`

**Test categories:**

#### 6.1 Basic Functionality (5 tests)
- `test_add_address_adds_single_depositor`
- `test_add_address_prevents_duplicates` (idempotent check)
- `test_add_address_multiple_depositors`
- `test_clear_all_removes_all_depositors`
- `test_clear_all_idempotent`

#### 6.2 Access Control (2 tests)
- `test_add_address_non_owner_fails`
- `test_clear_all_non_owner_fails`

#### 6.3 Event Emission (2 tests)
- `test_add_address_emits_event`
- `test_clear_all_emits_event`

#### 6.4 Query Functionality (2 tests)
- `test_get_allowlist_returns_empty_when_not_set`
- `test_get_allowlist_returns_all_addresses`

#### 6.5 Integration with Deposit (5 tests)
- `test_owner_always_permitted_regardless_of_allowlist`
- `test_depositor_in_allowlist_can_deposit`
- `test_depositor_not_in_allowlist_fails_with_correct_error`
- `test_deposit_after_clear_all_fails`
- `test_add_address_after_clear_all`

#### 6.6 Error Code Stability (1 test)
- `test_error_code_stability` (verify discriminant values)

**Total new tests:** 17

**Existing tests to update:**
- Any tests that expect panics from `deposit()` must now expect `Err(VaultError::...)`
- Search for `.expect()` calls in test code and update assertions

---

### 7. API Surface Changes

**Breaking changes:** ⚠️ Yes (return type changes)

| Function | Old Signature | New Signature |
|----------|---------------|---------------|
| `init` | `pub fn init(...) ` | `pub fn init(...) -> Result<(), VaultError>` |
| `deposit` | `pub fn deposit(...)` | `pub fn deposit(...) -> Result<(), VaultError>` |
| `deduct` | `pub fn deduct(...)` | `pub fn deduct(...) -> Result<(), VaultError>` |
| `batch_deduct` | `pub fn batch_deduct(...)` | `pub fn batch_deduct(...) -> Result<(), VaultError>` |
| `set_authorized_caller` | `pub fn set_authorized_caller(...)` | `pub fn set_authorized_caller(...) -> Result<(), VaultError>` |
| `pause` | `pub fn pause(...)` | `pub fn pause(...) -> Result<(), VaultError>` |
| `unpause` | `pub fn unpause(...)` | `pub fn unpause(...) -> Result<(), VaultError>` |
| `set_max_deduct` | `pub fn set_max_deduct(...)` | `pub fn set_max_deduct(...) -> Result<(), VaultError>` |
| `set_settlement` | `pub fn set_settlement(...)` | `pub fn set_settlement(...) -> Result<(), VaultError>` |

**New functions:**
- `pub fn add_address(...) -> Result<(), VaultError>`
- `pub fn clear_all(...) -> Result<(), VaultError>`
- `pub fn get_allowlist(...) -> Vec<Address>` (no Result — infallible)

**Migration impact:**
- All callers must handle `Result` instead of assuming success
- Tests must use `.unwrap()`, `?`, or match on `Result`

---

## Implementation Plan

### Phase 1: Error Variant Addition
1. Update `contracts/vault/src/errors.rs`
   - Add `CallerNotInAllowlist = 44`
   - Update file-level discriminant table
   - Add rustdoc comments

### Phase 2: Storage Key Addition
1. Update `contracts/vault/src/lib.rs`
   - Add `AllowedDepositors` to `StorageKey` enum

### Phase 3: Event Functions
1. Update `contracts/vault/src/events.rs`
   - Add `event_allowlist_add`
   - Add `event_allowlist_clear`

### Phase 4: Helper Function Refactoring
1. Change `require_positive_amount` to return `Result`
2. Change `require_valid_deposit_amount` to return `Result`
3. Change `require_valid_deduct_amount` to return `Result`

### Phase 5: Allowlist Management Functions
1. Implement `add_address` with full error handling
2. Implement `clear_all` with full error handling
3. Implement `get_allowlist` (infallible)

### Phase 6: Modify Existing Functions
1. Update `init` to return `Result` and propagate errors
2. Update `deposit` to:
   - Return `Result`
   - Use Vec-based allowlist check
   - Return `CallerNotInAllowlist` error
3. Update `deduct`, `batch_deduct`, etc. to return `Result`

### Phase 7: Test Implementation
1. Implement 17 new allowlist tests
2. Update existing tests to handle `Result` returns
3. Add error discriminant stability test

### Phase 8: Verification
1. Run `cargo fmt --all`
2. Run `cargo clippy --all-targets -- -D warnings`
3. Run `cargo test -p callora-vault`
4. Run `cargo build --target wasm32-unknown-unknown --release -p callora-vault`
5. Verify 95%+ coverage with tarpaulin

---

## Security Considerations

### 1. Authorization
- All allowlist management functions require owner authentication
- Owner privilege preserved: owner can always deposit
- No privilege escalation vector (only owner can manage allowlist)

### 2. Information Leakage
- Error variants reveal minimal information
- `CallerNotInAllowlist` doesn't reveal who IS in the allowlist
- Event emission provides audit trail without exposing internal state

### 3. Storage Limits
- Vec storage unbounded but reasonable for expected use (1-10 addresses)
- No DoS risk: only owner can add addresses
- Future enhancement: consider max size limit if needed

### 4. Overflow Safety
- Use `checked_add` in deposit balance calculation
- Return `VaultError::Overflow` instead of panic
- Maintains profile.dev overflow-checks = true

### 5. Reentrancy
- All state changes before external calls
- No reentrancy risk in allowlist management (no external calls)

---

## Performance Analysis

### Time Complexity

| Operation | Complexity | Notes |
|-----------|------------|-------|
| `add_address` | O(n) | Linear scan for duplicate check (n = allowlist size) |
| `clear_all` | O(1) | Single storage removal |
| `get_allowlist` | O(1) | Single storage read, returns Vec reference |
| `deposit` (allowlist check) | O(n) | `Vec::contains()` linear scan |

### Space Complexity

| Storage Key | Size | Notes |
|-------------|------|-------|
| `AllowedDepositors` | ~32 bytes per address | Plus Vec overhead |
| Typical case | ~320 bytes | 10 addresses × 32 bytes |

### Gas Estimates (relative)

- `add_address`: ~5,000 + 1,000n gas (n = existing addresses)
- `clear_all`: ~2,000 gas (constant)
- `get_allowlist`: ~1,000 gas (read-only)

---

## Documentation Requirements

### 1. Rustdoc Comments
- All new functions: summary, params, returns, errors, examples
- New error variant: when returned, security note
- Storage key: purpose and structure

### 2. File-Level Documentation
Update `lib.rs` module docs to mention allowlist management

### 3. Error Table
Update `errors.rs` file-level discriminant table

### 4. No External Docs
- No changes needed to ALLOWLIST_IMPLEMENTATION.md (already documented)
- PR description will reference this design

---

## Success Criteria

1. ✅ All 23 panics replaced with typed errors
2. ✅ New error variant `CallerNotInAllowlist = 44` added
3. ✅ Three allowlist functions implemented and working
4. ✅ 17 new tests added and passing
5. ✅ All existing tests updated and passing
6. ✅ 95%+ line coverage on vault contract
7. ✅ Zero clippy warnings
8. ✅ WASM builds successfully
9. ✅ Events emitted correctly for allowlist operations
10. ✅ Owner privilege preserved (can always deposit)

---

## Next Steps

1. ✅ Requirements complete
2. ✅ Design complete
3. 🔄 Create `tasks.md` with task breakdown
4. ⏳ Execute tasks via subagent delegation

**Status:** ✅ Design Complete — Ready for Task Breakdown
