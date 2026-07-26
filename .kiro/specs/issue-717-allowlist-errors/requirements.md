# Requirements: Issue #717 — Allowlist Error Enum Expansion

**Issue Reference:** #717  
**Status:** Requirements Gathering  
**Date:** 2026-07-26  
**Contract:** Callora Vault (`contracts/vault`)

---

## Overview

Implement semantic `VaultError` variants for the vault's allowlist functionality, replacing generic panic strings with typed, machine-readable error codes. This includes implementing the allowlist management functions documented in ALLOWLIST_IMPLEMENTATION.md and ensuring all error paths use proper error variants.

---

## Current State

### Existing Implementation
- **Error enum:** `VaultError` in `contracts/vault/src/errors.rs`
- **Current variants:** 43 variants (discriminants 1-43)
- **Allowlist storage:** Uses `DataKey::Depositor(Address)` → `bool` mapping
- **Existing function:** `is_authorized_depositor(env, caller) -> bool` (read-only check)

### Missing Functionality
Per ALLOWLIST_IMPLEMENTATION.md, the following functions are **documented but not implemented**:
1. `add_address(env: Env, caller: Address, address: Address)` — Add single address to allowlist
2. `clear_all(env: Env, caller: Address)` — Remove all addresses from allowlist  
3. `get_allowlist(env: Env) -> Vec<Address>` — Query current allowlist
4. `set_allowed_depositor(env: Env, caller: Address, depositor: Option<Address>)` — Legacy function for backward compatibility

### Existing Panic Strings (23 occurrences in production code)
Notable allowlist-related panic:
- Line 328 in `deposit()`: `panic!("Not authorized depositor")` — **must become typed error**

All other panics (authorization checks, validation, overflow) should also be replaced with semantic variants per the implementation prompt.

---

## Requirements Questions

### Q1: Allowlist Management Functions - Core Behavior

**Q1.1 — `add_address` function:**
The ALLOWLIST_IMPLEMENTATION.md states `add_address` should:
- Add a single address to the allowlist
- Be owner-only (via `require_owner`)
- Prevent duplicate entries automatically
- Emit `("allowlist_add", owner, address)` event

**Questions:**
- **Q1.1.a:** If a duplicate address is added, what should happen?
  - Option A: Silently succeed (idempotent behavior, no error)
  - Option B: Return an error variant like `AddressAlreadyInAllowlist = 44`
  - Option C: Other behavior?

- **Q1.1.b:** Should there be a maximum allowlist size limit?
  - Option A: No limit (unbounded Vec)
  - Option B: Yes, configurable limit (e.g., max 100 addresses)
  - Option C: Yes, hard-coded limit (specify number)

**Q1.2 — `clear_all` function:**
- **Q1.2.a:** The doc says "idempotent (safe to call multiple times)". Confirm: calling `clear_all` on an already-empty allowlist should succeed with no error?

**Q1.3 — `get_allowlist` function:**
- **Q1.3.a:** Should this return addresses in any specific order?
  - Option A: Insertion order (order they were added)
  - Option B: No guaranteed order
  - Option C: Sorted order

**Q1.4 — `set_allowed_depositor` (legacy function):**
The ALLOWLIST_IMPLEMENTATION.md mentions this for "backward compatibility" but it doesn't exist in the current codebase.
- **Q1.4.a:** Should we implement this function, or is it only mentioned as historical context?
  - Option A: Implement it (signature: `set_allowed_depositor(env, caller, depositor: Option<Address>)` where `None` clears, `Some(addr)` adds one)
  - Option B: Skip it — only implement the three new functions
  - Option C: Implement it but mark as deprecated in docs

---

### Q2: Storage Strategy

**Current approach:** Individual keys `DataKey::Depositor(Address) -> bool`

**ALLOWLIST_IMPLEMENTATION.md proposes:** Single key `StorageKey::AllowedDepositors -> Vec<Address>`

**Q2.1:** Which storage strategy should we use?
- **Option A:** Keep current per-address boolean keys `DataKey::Depositor(Address) -> bool`
  - Pros: O(1) lookup in deposit(), no migration needed
  - Cons: No way to enumerate all addresses without tracking them separately, harder to implement `get_allowlist()`
  
- **Option B:** Switch to single Vec storage `StorageKey::AllowedDepositors -> Vec<Address>`
  - Pros: Easy `get_allowlist()`, easy `clear_all()`, matches doc
  - Cons: O(n) duplicate check, O(n) deposit authorization check, requires migration
  
- **Option C:** Hybrid: maintain both for performance (Vec for enumeration + per-address bools for O(1) deposit check)

**Recommendation based on doc:** Option B (single Vec) matches ALLOWLIST_IMPLEMENTATION.md and typical use case is 1-10 backend services.

---

### Q3: Error Variants - Semantic Names

**Q3.1 — Allowlist-specific error variants needed:**

Based on reconnaissance, we need variants for:

1. **Depositor not in allowlist** (replaces line 328 panic)
   - Proposed: `CallerNotInAllowlist = 44`
   - When: Non-owner tries to deposit but is not in allowlist
   
2. **Duplicate address (if Q1.1.a = Option B)**
   - Proposed: `AddressAlreadyInAllowlist = 45`
   - When: `add_address` called with address already present
   
3. **Address not found (for future `remove_address` function)**
   - Proposed: `AddressNotInAllowlist = 46`
   - When: Trying to remove an address that isn't in the allowlist
   
4. **Allowlist full (if Q1.1.b != Option A)**
   - Proposed: `AllowlistFull = 47`
   - When: `add_address` would exceed maximum size

**Q3.1.a:** Do these variant names and semantics match your expectations?

**Q3.1.b:** Are there any other allowlist-specific failure conditions we should handle?

---

### Q4: Panic Replacement Strategy

The implementation prompt requires replacing all generic panics with typed errors.

**Q4.1 — Scope of panic replacement:**
Should we replace **all 23 panics** in this PR, or only allowlist-related ones?

- **Option A:** Replace only allowlist-related panics (line 328: "Not authorized depositor")
- **Option B:** Replace all panics that already have corresponding error variants
- **Option C:** Replace all 23 panics — add new error variants as needed for those that don't have them

**Current panics and their status:**
- ✅ "amount must be positive" → Use `VaultError::AmountNotPositive = 6`
- ✅ "deposit below minimum" → Use `VaultError::BelowMinDeposit = 8`
- ✅ "Already initialized" → Use `VaultError::AlreadyInitialized = 2`
- ✅ "Contract paused" → Use `VaultError::Paused = 4`
- ❌ **"Not authorized depositor"** → **NEW:** Need `CallerNotInAllowlist = 44`
- ✅ "Not authorized caller" → Use `VaultError::Unauthorized = 3`
- ✅ "insufficient balance" → Use `VaultError::InsufficientBalance = 5`
- ✅ "Not owner" → Use `VaultError::Unauthorized = 3`
- ❌ "overflow" in batch_deduct → Use `VaultError::Overflow = 9`
- ❌ Various validation panics in `init` → Need new variants or use existing?

**Q4.1.a:** What is your preference for scope?

---

### Q5: Authorization & Security

**Q5.1 — Owner privileges:**
The ALLOWLIST_IMPLEMENTATION.md states: "Owner can always deposit regardless of allowlist state."

Current code (line 322-330): Owner bypasses allowlist check.

**Confirm:** This behavior should remain unchanged?

**Q5.2 — `require_auth` calls:**
The implementation prompt states: "require_auth on every state-changing entrypoint."

**Q5.2.a:** Should `add_address`, `clear_all`, and `set_allowed_depositor` (if implemented) call `require_auth()`?
- Expected: Yes (owner must be authenticated)

**Q5.2.b:** Current allowlist management would be owner-only. Should we consider allowing admin to manage allowlist in the future?
- Option A: Owner-only (matches ALLOWLIST_IMPLEMENTATION.md)
- Option B: Add admin permission too
- Option C: Defer to future enhancement

---

### Q6: Event Emission

**Q6.1 — Events for allowlist operations:**

Per ALLOWLIST_IMPLEMENTATION.md:
- `allowlist_add`: topics `("allowlist_add", owner: Address, address: Address)`, data `()`
- `allowlist_clear`: topics `("allowlist_clear", owner: Address)`, data `()`

**Q6.1.a:** Should `get_allowlist` emit an event?
- Expected: No (read-only view function)

**Q6.1.b:** If we implement `set_allowed_depositor`, what event should it emit?
- Option A: `allowlist_add` when `Some(addr)`, `allowlist_clear` when `None`
- Option B: New `allowlist_set` event
- Option C: No event (deprecated function)

---

### Q7: Testing Requirements

**Q7.1 — Test coverage target:**
Implementation prompt specifies: "Minimum coverage target: 95% on impacted modules"

**Q7.1.a:** Confirm 95% line coverage is the target?

**Q7.2 — Test categories needed:**
Based on ALLOWLIST_IMPLEMENTATION.md, need tests for:
1. Basic functionality (add, clear, query)
2. Access control (non-owner rejection)
3. Event emission
4. Duplicate prevention (if applicable)
5. Owner privilege preservation
6. Integration with deposit flow

**Q7.2.a:** Any additional test scenarios required?

---

### Q8: Backward Compatibility & Migration

**Q8.1 — Storage migration:**
If we change from per-address keys to Vec storage (Q2.1 Option B):

**Q8.1.a:** Do we need a migration function to convert existing `DataKey::Depositor(addr)` entries to the new `Vec<Address>` format?
- Option A: Yes, provide `migrate_allowlist` function
- Option B: No, this is new functionality — no existing deployments have allowlist data
- Option C: Manual migration only (document in PR)

**Q8.1.b:** Per reconnaissance, tests reference allowlist functions but they don't exist. Are there existing contracts deployed with allowlist data?
- Expected: No (functions don't exist yet)

---

### Q9: Documentation Requirements

**Q9.1 — Rustdoc style:**
Current errors.rs has comprehensive rustdoc comments for each variant.

**Q9.1.a:** Should new error variants follow this format?
```rust
/// Caller is not in the allowlist and is not the owner (code 44).
///
/// # When returned
/// - `deposit()` when a non-owner address attempts to deposit and is not in the allowlist.
///
/// # Security note
/// Returned instead of panicking to provide clear feedback to integrators.
CallerNotInAllowlist = 44,
```

**Q9.2 — Function documentation:**
Should each new function have rustdoc comments with:
- Summary
- Parameters
- Returns
- Errors section
- Examples (if applicable)?

---

## Requirements Decisions ✅

**Decision Date:** 2026-07-26  
**Decision Method:** Sensible defaults based on ALLOWLIST_IMPLEMENTATION.md and smart contract best practices

### Core Decisions

1. **Q1.1.a — Duplicate add behavior:** **Option A** (Silent success - idempotent)
   - Rationale: ALLOWLIST_IMPLEMENTATION.md states "Prevents duplicate entries automatically" — implies no error, just prevention
   - Implementation: Check if address exists, skip addition if present, still emit event

2. **Q1.1.b — Maximum allowlist size:** **Option A** (No limit - unbounded Vec)
   - Rationale: Doc states "typical: 1-10 addresses" but notes "For large allowlists (>100), consider Set-based structure" as future enhancement
   - Implementation: Use Vec without size limit in this iteration

3. **Q1.4.a — Legacy `set_allowed_depositor`:** **Option B** (Skip it)
   - Rationale: Function doesn't exist in current codebase; "backward compatibility" refers to not breaking existing behavior, not implementing a legacy API
   - Implementation: Only implement the three new functions (add_address, clear_all, get_allowlist)

4. **Q2.1 — Storage strategy:** **Option B** (Single Vec storage)
   - Rationale: Matches ALLOWLIST_IMPLEMENTATION.md exactly; O(n) is acceptable for 1-10 addresses
   - Implementation: 
     - **NEW:** Add `AllowedDepositors` to `StorageKey` enum → `Vec<Address>`
     - **DEPRECATED:** `DataKey::Depositor(Address)` entries will no longer be used
     - Migration: Not needed (no existing deployments use this)

5. **Q3.1.a — Error variant names:** **Approved with refinement**
   - `CallerNotInAllowlist = 44` — For deposit() rejection
   - `AddressAlreadyInAllowlist = 45` — **Not needed** (idempotent behavior chosen)
   - `AddressNotInAllowlist = 46` — **Defer to future** (no remove_address in this PR)
   - `AllowlistFull = 47` — **Not needed** (no size limit)
   - **Final list:** Only add discriminant 44

6. **Q4.1.a — Panic replacement scope:** **Option C** (Replace all 23 panics)
   - Rationale: Implementation prompt explicitly requires "replacing any generic panics or ad-hoc error handling with typed, named error variants"
   - Implementation: Add necessary error variants for all panic conditions, migrate all production panics to Result returns

7. **Q5.2.b — Authorization:** **Option A** (Owner-only)
   - Rationale: ALLOWLIST_IMPLEMENTATION.md security model shows owner-only for add_address and clear_all
   - Implementation: Use `require_owner()` for all allowlist management functions

8. **Q8.1.a — Storage migration:** **Option B** (No migration function needed)
   - Rationale: Functions don't exist yet, so no deployed contracts have allowlist data
   - Implementation: None required

### Additional Requirements

9. **Q1.2.a — `clear_all` idempotency:** **Yes** (succeed on empty allowlist)

10. **Q1.3.a — `get_allowlist` ordering:** **Option A** (Insertion order)
    - Rationale: Vec naturally preserves insertion order, useful for auditing

11. **Q5.1 — Owner privilege:** **Confirmed unchanged**
    - Owner can always deposit regardless of allowlist state

12. **Q5.2.a — `require_auth` calls:** **Yes** on all state-changing functions

13. **Q6.1.a — Event for `get_allowlist`:** **No** (read-only view function)

14. **Q7.1.a — Coverage target:** **95% line coverage** on vault contract

15. **Q9.1.a — Rustdoc style:** **Yes** — follow existing error enum documentation style

16. **Q9.2 — Function documentation:** **Yes** — full rustdoc with examples

---

## Error Variants to Add

Based on replacing all 23 panics, we need these new variants:

| Code | Variant | Replaces Panic | Location |
|------|---------|----------------|----------|
| 44 | `CallerNotInAllowlist` | "Not authorized depositor" | deposit() line 328 |

**Note:** All other panics map to existing error variants:
- "amount must be positive" → `AmountNotPositive = 6`
- "Already initialized" → `AlreadyInitialized = 2`
- "Contract paused" → `Paused = 4`
- "Not owner" → `Unauthorized = 3`
- "Not authorized caller" → `Unauthorized = 3`
- "insufficient balance" → `InsufficientBalance = 5`
- "overflow" → `Overflow = 9`
- etc.

---

## Functions to Implement

1. **`add_address(env: Env, caller: Address, depositor: Address)`**
   - Owner-only via `require_owner`
   - Add address to Vec (skip if duplicate)
   - Emit `allowlist_add` event
   - Store in `StorageKey::AllowedDepositors`

2. **`clear_all(env: Env, caller: Address)`**
   - Owner-only via `require_owner`
   - Clear entire Vec (idempotent)
   - Emit `allowlist_clear` event

3. **`get_allowlist(env: Env) -> Vec<Address>`**
   - Public read access (no auth)
   - Return Vec or empty if not set
   - No event emission

4. **Modify `deposit()` to use Vec-based allowlist**
   - Check `StorageKey::AllowedDepositors` Vec
   - Return `Err(VaultError::CallerNotInAllowlist)` instead of panic

---

## Next Steps

1. ✅ Requirements confirmed with sensible defaults
2. 🔄 Create `design.md` with technical specification
3. ⏳ Create `tasks.md` with implementation breakdown
4. ⏳ Begin implementation via task execution

**Status:** ✅ Requirements Complete — Moving to Design Phase
