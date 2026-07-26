# Task 16: Update Existing Tests for Result Returns

**File:** `contracts/vault/src/test.rs` and other test files

---

## Overview

After updating functions to return `Result<(), VaultError>`, existing tests that call these functions need to handle the Result properly.

---

## Strategy

### 1. Find All Test Failures

Run the test suite to see which tests are failing:

```bash
cargo test -p callora-vault 2>&1 | tee test_failures.txt
```

### 2. Common Patterns to Fix

#### Pattern A: Add `.unwrap()` to function calls

**Before:**
```rust
client.init(&owner, &usdc, &0, &auth_caller, &100, &None, &1000, &settlement);
```

**After:**
```rust
client.init(&owner, &usdc, &0, &auth_caller, &100, &None, &1000, &settlement).unwrap();
```

#### Pattern B: Update panic expectations

**Before:**
```rust
#[test]
#[should_panic(expected = "Contract paused")]
fn test_deposit_when_paused_panics() {
    // ...
    client.deposit(&depositor, &100);
}
```

**After:**
```rust
#[test]
fn test_deposit_when_paused_returns_error() {
    // ...
    let result = client.try_deposit(&depositor, &100);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), VaultError::Paused as u32);
}
```

#### Pattern C: Use `?` in test helper functions

**Before:**
```rust
fn setup_vault(env: &Env) -> CalloraVaultClient {
    let client = CalloraVaultClient::new(env, &vault_addr);
    client.init(&owner, &usdc, &0, &auth_caller, &100, &None, &1000, &settlement);
    client
}
```

**After:**
```rust
fn setup_vault(env: &Env) -> CalloraVaultClient {
    let client = CalloraVaultClient::new(env, &vault_addr);
    client.init(&owner, &usdc, &0, &auth_caller, &100, &None, &1000, &settlement).unwrap();
    client
}
```

---

## Functions That Now Return Result

Add `.unwrap()` or handle errors for calls to:

1. `init()`
2. `deposit()`
3. `deduct()`
4. `batch_deduct()`
5. `set_authorized_caller()`
6. `pause()`
7. `unpause()`
8. `set_max_deduct()`
9. `set_settlement()`
10. `add_address()` (new)
11. `clear_all()` (new)

---

## Search & Replace Patterns

You can use these regex patterns to help:

### Pattern 1: init calls
**Search:** `\.init\((.*?)\);`  
**Replace:** `.init($1).unwrap();`

### Pattern 2: deposit calls
**Search:** `\.deposit\((.*?)\);`  
**Replace:** `.deposit($1).unwrap();`

### Pattern 3: deduct calls
**Search:** `\.deduct\((.*?)\);`  
**Replace:** `.deduct($1).unwrap();`

**⚠️ Warning:** Don't blindly search/replace. Some tests intentionally expect errors and use `try_` variants.

---

## Tests That Expect Errors

For tests that check error conditions, use the `try_` variants:

```rust
// Good - for error testing
let result = client.try_deposit(&depositor, &100);
assert!(result.is_err());

// Good - for success path
client.deposit(&depositor, &100).unwrap();
```

---

## Verification After Updates

After fixing test code, run:

```bash
cargo test -p callora-vault
```

All tests should pass, including:
- All existing tests (updated to handle Result)
- 17 new allowlist tests (from Tasks 13-15)

---

## Expected Test Count

After completion, you should have approximately:
- **Existing tests:** ~50-100 tests (updated)
- **New allowlist tests:** 17 tests
- **Total passing:** All tests

---

## Debugging Failed Tests

If tests still fail after adding `.unwrap()`:

1. **Check the error message:**
   ```bash
   cargo test test_name -- --nocapture
   ```

2. **Common issues:**
   - Missing `.unwrap()` on a new Result-returning function
   - Test expects a panic but function now returns Error
   - Test setup incomplete (missing token setup, etc.)

3. **Fix strategy:**
   - For panic tests: Convert to error assertion tests
   - For setup issues: Ensure all Result returns are handled
   - For logic issues: Review test expectations

---

## Example: Converting a Panic Test

**Before:**
```rust
#[test]
#[should_panic(expected = "Already initialized")]
fn test_double_init_panics() {
    let env = Env::default();
    let client = create_vault(&env);
    
    client.init(&owner, &usdc, &0, &auth_caller, &100, &None, &1000, &settlement);
    client.init(&owner, &usdc, &0, &auth_caller, &100, &None, &1000, &settlement); // Should panic
}
```

**After:**
```rust
#[test]
fn test_double_init_returns_error() {
    let env = Env::default();
    let client = create_vault(&env);
    
    client.init(&owner, &usdc, &0, &auth_caller, &100, &None, &1000, &settlement).unwrap();
    
    let result = client.try_init(&owner, &usdc, &0, &auth_caller, &100, &None, &1000, &settlement);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err().unwrap(), VaultError::AlreadyInitialized as u32);
}
```

---

## Status Tracking

- [ ] Run initial test suite to identify failures
- [ ] Update all test helper functions
- [ ] Update all test assertions
- [ ] Convert panic tests to error tests
- [ ] Verify all tests pass
- [ ] Document any tests that were intentionally skipped or removed
