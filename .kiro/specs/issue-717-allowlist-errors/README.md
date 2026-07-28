# Issue #717: Allowlist Error Enum Expansion

**Status:** ✅ Ready for Implementation  
**Created:** 2026-07-26  
**Contract:** Callora Vault (`contracts/vault`)

---

## Quick Summary

This specification implements semantic error handling for the Callora Vault's allowlist functionality by:

1. **Adding 1 new error variant** — `CallerNotInAllowlist = 44`
2. **Implementing 3 allowlist management functions** — `add_address`, `clear_all`, `get_allowlist`
3. **Replacing all 23 generic panics** with typed `Result<T, VaultError>` returns
4. **Adding 17 comprehensive tests** — covering functionality, access control, events, and integration

---

## Specification Files

- **[requirements.md](./requirements.md)** — Complete requirements with 9 question categories and confirmed decisions
- **[design.md](./design.md)** — Technical design with architecture, API, storage, events, and security considerations
- **[tasks.md](./tasks.md)** — 17 implementation tasks with dependencies, estimates, and acceptance criteria

---

## Key Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| **Duplicate adds** | Idempotent (silent success) | Matches ALLOWLIST_IMPLEMENTATION.md "prevents duplicates automatically" |
| **Max allowlist size** | Unbounded Vec | Typical use case 1-10 addresses, O(n) acceptable |
| **Storage strategy** | Single Vec (`StorageKey::AllowedDepositors`) | Matches doc, easy enumeration, efficient for small lists |
| **Panic replacement** | All 23 panics → typed errors | Implementation prompt requirement |
| **Authorization** | Owner-only for allowlist management | Matches doc security model |
| **Legacy function** | Skip `set_allowed_depositor` | Doesn't exist in codebase, not needed |

---

## Implementation Phases

### Phase 1: Foundation (Tasks 1-3)
- Add error variant `CallerNotInAllowlist = 44`
- Add storage key `AllowedDepositors`
- Add event functions

### Phase 2: Helpers (Task 4)
- Refactor validation helpers to return Result

### Phase 3: Allowlist Functions (Tasks 5-7)
- Implement `add_address` (owner-only, idempotent, emits event)
- Implement `clear_all` (owner-only, idempotent, emits event)
- Implement `get_allowlist` (public read, no auth)

### Phase 4: Core Functions (Tasks 8-12)
- Update `init`, `deposit`, `deduct`, `batch_deduct` to return Result
- Replace all panics with typed errors
- Update allowlist check in `deposit` to use Vec storage

### Phase 5: Testing (Tasks 13-16)
- Add 17 new allowlist tests
- Update existing tests for Result returns

### Phase 6: Verification (Task 17)
- Run fmt, clippy, tests, WASM build, coverage

---

## API Changes Summary

### New Functions
```rust
pub fn add_address(env: Env, caller: Address, depositor: Address) -> Result<(), VaultError>
pub fn clear_all(env: Env, caller: Address) -> Result<(), VaultError>
pub fn get_allowlist(env: Env) -> Vec<Address>
```

### Updated Signatures (Breaking Changes)
```rust
// OLD: pub fn init(...) 
// NEW: 
pub fn init(...) -> Result<(), VaultError>

// OLD: pub fn deposit(...)
// NEW:
pub fn deposit(...) -> Result<(), VaultError>

// OLD: pub fn deduct(...)
// NEW:
pub fn deduct(...) -> Result<(), VaultError>

// OLD: pub fn batch_deduct(...)
// NEW:
pub fn batch_deduct(...) -> Result<(), VaultError>

// ... and 5 more owner-gated functions
```

### New Error Variant
```rust
CallerNotInAllowlist = 44  // Caller not in allowlist and not owner
```

---

## Success Metrics

- ✅ 1 new error variant added
- ✅ 3 new allowlist functions implemented
- ✅ 23 panics replaced with typed errors
- ✅ 9 function signatures updated to return Result
- ✅ 17 new tests added
- ✅ All existing tests passing
- ✅ 95%+ code coverage
- ✅ Zero clippy warnings
- ✅ WASM build successful

---

## Estimated Effort

**Total:** ~10-12 hours of implementation time

**Breakdown:**
- Foundation & helpers: 2 hours
- Allowlist functions: 2 hours
- Core function updates: 3 hours
- Testing: 3-4 hours
- Verification: 1 hour

---

## Next Steps

**To execute this specification:**

```bash
# From project root, run all tasks
kiro run-all-tasks --spec issue-717-allowlist-errors
```

**Or execute tasks individually:**

```bash
kiro run-task 1 --spec issue-717-allowlist-errors
kiro run-task 2 --spec issue-717-allowlist-errors
# ... etc
```

---

## References

- **Original documentation:** `ALLOWLIST_IMPLEMENTATION.md` (root directory)
- **Issue:** GitHub Issue #717 (Allowlist Error Enum Expansion)
- **Soroban SDK:** Version 22 (workspace dependency)
- **Contract:** `contracts/vault` (Callora Vault)

---

**Specification Status:** ✅ Complete and ready for implementation
