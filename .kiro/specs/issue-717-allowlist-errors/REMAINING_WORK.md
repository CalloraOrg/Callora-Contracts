# Remaining Work: Tasks 12-17

**Status:** 11 of 17 tasks complete (65%)  
**Remaining:** Tasks 12-17

---

## Task 12: Update Remaining Owner-Gated Functions

**File:** `contracts/vault/src/lib.rs`

### Function 1: `set_authorized_caller` (around line 560-574)

**Current code:**
```rust
pub fn set_authorized_caller(env: Env, caller: Address) {
    caller.require_auth();
    let owner = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Owner)
        .unwrap();
    if caller != owner {
        panic!("Not owner");
    }
    env.storage()
        .instance()
        .set(&DataKey::AuthorizedCaller, &caller);
}
```

**Updated code:**
```rust
pub fn set_authorized_caller(env: Env, caller: Address) -> Result<(), VaultError> {
    caller.require_auth();
    let owner = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Owner)
        .unwrap();
    if caller != owner {
        return Err(VaultError::Unauthorized);
    }
    env.storage()
        .instance()
        .set(&DataKey::AuthorizedCaller, &caller);
    Ok(())
}
```

---

### Function 2: `pause` (around line 575-589)

**Current code:**
```rust
pub fn pause(env: Env, caller: Address) {
    caller.require_auth();
    let owner = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Owner)
        .unwrap();
    if caller != owner {
        panic!("Not owner");
    }
    env.storage().instance().set(&DataKey::Paused, &true);
}
```

**Updated code:**
```rust
pub fn pause(env: Env, caller: Address) -> Result<(), VaultError> {
    caller.require_auth();
    let owner = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Owner)
        .unwrap();
    if caller != owner {
        return Err(VaultError::Unauthorized);
    }
    env.storage().instance().set(&DataKey::Paused, &true);
    Ok(())
}
```

---

### Function 3: `unpause` (around line 590-604)

**Current code:**
```rust
pub fn unpause(env: Env, caller: Address) {
    caller.require_auth();
    let owner = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Owner)
        .unwrap();
    if caller != owner {
        panic!("Not owner");
    }
    env.storage().instance().set(&DataKey::Paused, &false);
}
```

**Updated code:**
```rust
pub fn unpause(env: Env, caller: Address) -> Result<(), VaultError> {
    caller.require_auth();
    let owner = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Owner)
        .unwrap();
    if caller != owner {
        return Err(VaultError::Unauthorized);
    }
    env.storage().instance().set(&DataKey::Paused, &false);
    Ok(())
}
```

---

### Function 4: `set_max_deduct` (around line 640-660)

**Current code:**
```rust
pub fn set_max_deduct(env: Env, caller: Address, max_deduct: i128) {
    caller.require_auth();
    let owner = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Owner)
        .unwrap();
    if caller != owner {
        panic!("Not owner");
    }
    if max_deduct <= 0 {
        panic!("max_deduct must be positive");
    }
    env.storage()
        .instance()
        .set(&DataKey::MaxDeduct, &max_deduct);
}
```

**Updated code:**
```rust
pub fn set_max_deduct(env: Env, caller: Address, max_deduct: i128) -> Result<(), VaultError> {
    caller.require_auth();
    let owner = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Owner)
        .unwrap();
    if caller != owner {
        return Err(VaultError::Unauthorized);
    }
    if max_deduct <= 0 {
        return Err(VaultError::MaxDeductNotPositive);
    }
    env.storage()
        .instance()
        .set(&DataKey::MaxDeduct, &max_deduct);
    Ok(())
}
```

---

### Function 5: `set_settlement` (around line 665-680)

**Current code:**
```rust
pub fn set_settlement(env: Env, caller: Address, settlement: Address) {
    caller.require_auth();
    let owner = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Owner)
        .unwrap();
    if caller != owner {
        panic!("Not owner");
    }
    env.storage()
        .instance()
        .set(&DataKey::Settlement, &settlement);
}
```

**Updated code:**
```rust
pub fn set_settlement(env: Env, caller: Address, settlement: Address) -> Result<(), VaultError> {
    caller.require_auth();
    let owner = env
        .storage()
        .instance()
        .get::<_, Address>(&DataKey::Owner)
        .unwrap();
    if caller != owner {
        return Err(VaultError::Unauthorized);
    }
    env.storage()
        .instance()
        .set(&DataKey::Settlement, &settlement);
    Ok(())
}
```

---

## Quick Implementation Steps

1. Open `contracts/vault/src/lib.rs`
2. Search for each function name
3. Add `-> Result<(), VaultError>` to signature
4. Replace `panic!("Not owner")` with `return Err(VaultError::Unauthorized)`
5. Replace `panic!("max_deduct must be positive")` with `return Err(VaultError::MaxDeductNotPositive)` (in set_max_deduct only)
6. Add `Ok(())` before the closing brace
7. Save the file

**After completing Task 12, run:**
```bash
cargo build -p callora-vault
```

This will show you any remaining compilation errors from test code that needs updating.

---

## Next: Tasks 13-16 (Testing)

See `TEST_IMPLEMENTATION.md` for detailed test implementation instructions.

---

## Status Tracking

- [ ] Task 12.1: `set_authorized_caller` updated
- [ ] Task 12.2: `pause` updated
- [ ] Task 12.3: `unpause` updated
- [ ] Task 12.4: `set_max_deduct` updated
- [ ] Task 12.5: `set_settlement` updated
- [ ] Verify: `cargo build -p callora-vault` succeeds (production code)
