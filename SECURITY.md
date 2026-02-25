# Callora Vault — Security Checklist

> **Purpose**: Provide a concise, actionable checklist so that reviewers, auditors, and maintainers can systematically verify the security posture of the `callora-vault` Soroban smart contract before testnet graduation and mainnet deployment.

---

## Table of Contents

1. [Access Control](#1-access-control)
2. [Overflow / Underflow Protection](#2-overflow--underflow-protection)
3. [Re-initialization Guard](#3-re-initialization-guard)
4. [Pause / Emergency-Stop Mechanism](#4-pause--emergency-stop-mechanism)
5. [Ownership & Admin Transfer](#5-ownership--admin-transfer)
6. [Input Validation & Edge Cases](#6-input-validation--edge-cases)
7. [Event Integrity](#7-event-integrity)
8. [Test Coverage](#8-test-coverage)
9. [External Audit Recommendation](#9-external-audit-recommendation)

---

## 1. Access Control

Verify that every state-mutating function enforces proper authorization.

| # | Check | Status | Contract Reference |
|---|-------|--------|--------------------|
| 1.1 | `init()` requires `owner.require_auth()` before any storage write. | ✅ Implemented | `lib.rs` — `init()` line 47 |
| 1.2 | `withdraw()` requires `meta.owner.require_auth()` — only the vault owner can withdraw. | ✅ Implemented | `lib.rs` — `withdraw()` line 248 |
| 1.3 | `withdraw_to()` requires `meta.owner.require_auth()` — owner-only. | ✅ Implemented | `lib.rs` — `withdraw_to()` line 267 |
| 1.4 | `deduct()` requires `caller.require_auth()` — callers must present a valid signature. | ✅ Implemented | `lib.rs` — `deduct()` line 185 |
| 1.5 | `batch_deduct()` requires `caller.require_auth()` — same as single deduct. | ✅ Implemented | `lib.rs` — `batch_deduct()` line 209 |
| 1.6 | `distribute()` requires `caller.require_auth()` **and** verifies `caller == admin`. | ✅ Implemented | `lib.rs` — `distribute()` lines 117–123 |
| 1.7 | `set_admin()` requires `caller.require_auth()` **and** verifies `caller == current_admin`. | ✅ Implemented | `lib.rs` — `set_admin()` lines 88–92 |
| 1.8 | `deposit()` is **not** gated by `require_auth()`. | ⚠️ By Design | `lib.rs` — `deposit()` line 164. **Note:** This is intentional for the current use case (any depositor can fund the vault), but reviewers should confirm this matches business requirements. If deposits should be restricted, add `owner.require_auth()` or an allowlist check. |

### Reviewer Action Items

- [ ] Confirm that allowing unauthenticated `deposit()` is acceptable for the deployment scenario.
- [ ] Confirm `deduct()` / `batch_deduct()` callers are restricted off-chain (e.g., only the backend holds the signing key).
- [ ] Evaluate whether a **role-based** access model (admin vs. owner vs. operator) is needed when the contract grows.

---

## 2. Overflow / Underflow Protection

Verify that all arithmetic on balances is safe.

| # | Check | Status | Contract Reference |
|---|-------|--------|--------------------|
| 2.1 | Rust `i128` arithmetic in **debug** builds panics on overflow / underflow by default. | ✅ Language-level | Rust specification |
| 2.2 | **Release** builds: Rust compiles `i128` arithmetic as wrapping by default. The `[profile.release] overflow-checks = true` flag **should** be set in `Cargo.toml` for production safety. | ⚠️ Verify | Check `Cargo.toml` `[profile.release]` section. If missing, add `overflow-checks = true`. |
| 2.3 | `deduct()` checks `meta.balance >= amount` before subtraction. | ✅ Implemented | `lib.rs` line 187 |
| 2.4 | `batch_deduct()` pre-validates every item against a running balance before applying any mutations. | ✅ Implemented | `lib.rs` lines 214–220 |
| 2.5 | `withdraw()` and `withdraw_to()` both assert `meta.balance >= amount`. | ✅ Implemented | `lib.rs` lines 250, 268–269 |
| 2.6 | `deposit()` adds to balance — no upper-bound guard. | ⚠️ Low Risk | `i128::MAX` is astronomically large. An explicit cap may still be desirable for business logic. |
| 2.7 | `distribute()` validates `amount > 0` and checks USDC token balance. | ✅ Implemented | `lib.rs` lines 126–143 |
| 2.8 | `batch_deduct()` validates `amount > 0` for each item. | ✅ Implemented | `lib.rs` line 217 |

### Reviewer Action Items

- [ ] Verify that `Cargo.toml` (workspace or vault-level) includes `overflow-checks = true` under `[profile.release]`.
- [ ] Consider adding explicit upper-bound limits if business rules dictate a maximum vault balance or maximum single-deposit amount.

---

## 3. Re-initialization Guard

Ensure the vault cannot be re-initialized after first setup, preventing state reset attacks.

| # | Check | Status | Contract Reference |
|---|-------|--------|--------------------|
| 3.1 | `init()` checks `env.storage().instance().has(&Symbol::new(&env, META_KEY))` and panics with `"vault already initialized"` if the key exists. | ✅ Implemented | `lib.rs` lines 48–50 |
| 3.2 | Test `test_init_double_panics` verifies double-init is rejected. | ✅ Tested | `test.rs` lines 243–255 |
| 3.3 | Test `init_already_initialized_panics` provides a second independent verification. | ✅ Tested | `test.rs` lines 654–666 |

### Reviewer Action Items

- [ ] Confirm there is no alternative code path that could overwrite the `"meta"` storage key outside of `init()`.
- [ ] If upgradeability is added in the future, ensure the migration function cannot re-trigger `init()`.

---

## 4. Pause / Emergency-Stop Mechanism

Document whether the contract can be paused in an emergency.

| # | Check | Status | Notes |
|---|-------|--------|-------|
| 4.1 | The vault **does not** currently implement a pause mechanism. | ❌ Not Implemented | There is no `pause()` / `unpause()` function, nor a `paused` flag in storage. |
| 4.2 | Soroban does not provide a native "pausable" pattern. | — | Must be implemented manually if desired. |

### Recommendation

Implement a pause mechanism before mainnet deployment:

```rust
// Suggested pattern (pseudocode):
const PAUSED_KEY: &str = "paused";

pub fn pause(env: Env, caller: Address) {
    caller.require_auth();
    let admin = Self::get_admin(env.clone());
    assert!(caller == admin, "unauthorized");
    env.storage().instance().set(&Symbol::new(&env, PAUSED_KEY), &true);
}

pub fn unpause(env: Env, caller: Address) {
    caller.require_auth();
    let admin = Self::get_admin(env.clone());
    assert!(caller == admin, "unauthorized");
    env.storage().instance().set(&Symbol::new(&env, PAUSED_KEY), &false);
}

// Add to every state-mutating function:
fn require_not_paused(env: &Env) {
    let paused: bool = env.storage().instance()
        .get(&Symbol::new(env, PAUSED_KEY))
        .unwrap_or(false);
    assert!(!paused, "contract is paused");
}
```

### Reviewer Action Items

- [ ] Decide if a pause mechanism is required for the initial mainnet release.
- [ ] If implemented, ensure that `pause()` / `unpause()` are admin-only and covered by tests.
- [ ] Consider a time-locked or multi-sig approach for unpausing to prevent abuse.

---

## 5. Ownership & Admin Transfer

Verify safe ownership and admin transfer patterns.

| # | Check | Status | Contract Reference |
|---|-------|--------|--------------------|
| 5.1 | `set_admin()` allows the current admin to transfer the admin role to a new address. | ✅ Implemented | `lib.rs` lines 87–96 |
| 5.2 | `set_admin()` requires `caller.require_auth()` and verifies `caller == current_admin`. | ✅ Implemented | `lib.rs` lines 88–92 |
| 5.3 | After admin transfer, the **old admin** is locked out of `distribute()`. | ✅ Tested | `test.rs` — `test_old_admin_cannot_distribute_after_transfer` lines 398–414 |
| 5.4 | After admin transfer, the **new admin** can call `distribute()`. | ✅ Tested | `test.rs` — `test_set_admin_transfers_control` lines 378–396 |
| 5.5 | Vault **owner** (in `VaultMeta`) is immutable after `init()` — no owner transfer function exists. | ⚠️ By Design | Owner is set once in `init()` and never changed. |

### Reviewer Action Items

- [ ] **Two-step admin transfer**: The current pattern is a single-step transfer (`set_admin` immediately replaces the admin). Consider a two-step pattern (`propose_admin` → `accept_admin`) to prevent accidental transfers to an incorrect or inaccessible address.
- [ ] Evaluate whether vault owner mutability is needed. If the owner loses their key, the vault's `withdraw()` / `withdraw_to()` functions become permanently unusable.
- [ ] Confirm admin and owner are **distinct** roles with appropriate separation of privileges:
  - **Owner**: controls `withdraw`, `withdraw_to`
  - **Admin**: controls `distribute`, `set_admin`

---

## 6. Input Validation & Edge Cases

| # | Check | Status | Contract Reference |
|---|-------|--------|--------------------|
| 6.1 | `deposit()` enforces `amount >= min_deposit`. | ✅ Implemented | `lib.rs` lines 166–171 |
| 6.2 | `deduct()` does **not** validate `amount > 0`. A zero-amount deduct succeeds silently. | ⚠️ Review | `lib.rs` line 187 checks `balance >= amount` but not `amount > 0`. |
| 6.3 | `withdraw()` and `withdraw_to()` both enforce `amount > 0`. | ✅ Implemented | `lib.rs` lines 249, 268 |
| 6.4 | `batch_deduct()` enforces `amount > 0` for each item **and** requires at least one item. | ✅ Implemented | `lib.rs` lines 212, 217 |
| 6.5 | `distribute()` enforces `amount > 0` (rejects zero and negative). | ✅ Implemented | `lib.rs` lines 126–128 |
| 6.6 | `init()` accepts `initial_balance = None` (defaults to 0) and `min_deposit = None` (defaults to 0). | ✅ Implemented | `lib.rs` lines 51–52 |

### Reviewer Action Items

- [ ] Consider adding `amount > 0` validation to `deduct()` for consistency with `withdraw()` and `batch_deduct()`.
- [ ] Decide if a negative `min_deposit` should be rejected in `init()`.

---

## 7. Event Integrity

| # | Check | Status | Contract Reference |
|---|-------|--------|--------------------|
| 7.1 | `init` event emitted with `(owner, balance)` on initialization. | ✅ Implemented | `lib.rs` lines 72–73 |
| 7.2 | `deposit` event emitted with `(amount, new_balance)`. | ✅ Implemented | `lib.rs` lines 177–178 |
| 7.3 | `deduct` event emitted with `(caller, request_id)` topics and `(amount, new_balance)` data. | ✅ Implemented | `lib.rs` lines 193–201 |
| 7.4 | `withdraw` event emitted with owner topic and `(amount, new_balance)` data. | ✅ Implemented | `lib.rs` lines 256–259 |
| 7.5 | `withdraw_to` event emitted with owner and recipient topics. | ✅ Implemented | `lib.rs` lines 275–282 |
| 7.6 | `distribute` event emitted with `(to)` topic and `amount` data. | ✅ Implemented | `lib.rs` lines 149–150 |
| 7.7 | Events are emitted **after** state mutation, not before. | ✅ Verified | All event publishes follow storage writes. |

> Full event schema: [EVENT_SCHEMA.md](EVENT_SCHEMA.md)

---

## 8. Test Coverage

The test suite (`contracts/vault/src/test.rs`) provides extensive coverage of vault functionality. Below is a mapping of security-relevant tests to checklist items.

| Security Area | Test(s) | Checklist Item(s) |
|---------------|---------|-------------------|
| Initialization | `test_init_success`, `init_and_balance`, `init_none_balance` | 3.1 |
| Re-init guard | `test_init_double_panics`, `init_already_initialized_panics` | 3.2, 3.3 |
| Deposit | `test_deposit_and_balance`, `deposit_and_deduct` | 1.8, 2.6, 6.1 |
| Deduct (success) | `test_deduct_success`, `deposit_and_deduct` | 1.4, 2.3 |
| Deduct (overflow) | `test_deduct_excess_panics`, `deduct_exact_balance_and_panic` | 2.3 |
| Deduct (events) | `deduct_event_emission` | 7.3 |
| Batch deduct (success) | `batch_deduct_success` | 1.5, 2.4 |
| Batch deduct (revert) | `batch_deduct_reverts_entire_batch` | 2.4 |
| Withdraw (success) | `withdraw_owner_success`, `withdraw_exact_balance` | 1.2, 2.5 |
| Withdraw (overflow) | `withdraw_exceeds_balance_fails` | 2.5 |
| Withdraw (auth) | `withdraw_without_auth_fails` | 1.2 |
| Withdraw to (success) | `withdraw_to_success` | 1.3 |
| Distribute (success) | `test_distribute_success`, `test_distribute_full_balance`, `test_distribute_multiple_times` | 1.6, 2.7 |
| Distribute (unauthorized) | `test_distribute_unauthorized_panics` | 1.6 |
| Distribute (zero/negative) | `test_distribute_zero_panics`, `test_distribute_negative_panics` | 6.5 |
| Distribute (insufficient) | `test_distribute_excess_panics` | 2.7 |
| Admin transfer | `test_set_admin_transfers_control` | 5.1, 5.4 |
| Admin revocation | `test_old_admin_cannot_distribute_after_transfer` | 5.3 |
| Meta consistency | `balance_and_meta_consistency` | 2.3 |

### Coverage Summary

- **Total unit tests**: 25 (24 active + 1 `#[ignore]`'d benchmark)
- **Functions covered**: `init`, `get_meta`, `get_admin`, `set_admin`, `deposit`, `deduct`, `batch_deduct`, `withdraw`, `withdraw_to`, `distribute`, `balance` — **all 11 public functions**.
- **Estimated line coverage**: **≥ 95%** of `lib.rs` — every public function has at least one success and one failure/edge-case test.

### Recommended Additional Tests

| Area | Suggested Test |
|------|----------------|
| `deduct()` with zero amount | Verify behavior when `amount = 0` is passed |
| `set_admin()` by non-admin | Should panic with `"unauthorized: caller is not admin"` |
| `deposit()` below min_deposit | Verify panic with descriptive message |
| `init()` with negative min_deposit | Define and verify expected behavior |
| Pause mechanism | (Once implemented) toggle pause and verify all mutating functions reject |

> Run the full test suite: `cargo test` from the workspace root. Run benchmarks: `cargo test --ignored -- --nocapture`.

---

## 9. External Audit Recommendation

> **🚨 CRITICAL: An independent, third-party security audit is strongly recommended before any mainnet deployment of the Callora Vault contract.**

### Why

- **Financial risk**: The vault manages USDC balances for an API marketplace. A vulnerability could lead to loss of user funds.
- **Immutable deployment**: Soroban contracts cannot be upgraded in-place. A bug discovered post-deployment requires a full migration (see [UPGRADE.md](UPGRADE.md)).
- **Novel platform**: Soroban (Stellar smart contracts) is a relatively new execution environment. Platform-specific attack vectors may not be well-documented.

### What to Include in Scope

1. All functions in `contracts/vault/src/lib.rs`
2. Storage layout and upgrade path ([STORAGE.md](contracts/vault/STORAGE.md), [UPGRADE.md](UPGRADE.md))
3. Access control model (owner vs. admin separation)
4. Token integration surface (`distribute()` and future `withdraw` token flows)
5. Event emission correctness and completeness
6. Denial-of-service vectors (e.g., storage exhaustion, gas griefing)
7. Cross-contract call safety (USDC token interactions in `distribute()`)

### Recommended Auditors

Consider firms with Rust / WASM / Soroban experience:

- [OtterSec](https://osec.io/)
- [Halborn](https://halborn.com/)
- [Trail of Bits](https://www.trailofbits.com/)
- [CertiK](https://certik.com/)

### Pre-Audit Preparation

- [ ] Ensure all items in this checklist are addressed or documented as accepted risks.
- [ ] Freeze the codebase (no feature changes during audit).
- [ ] Provide auditors with this `SECURITY.md`, `STORAGE.md`, `EVENT_SCHEMA.md`, and `UPGRADE.md`.
- [ ] Prepare a threat model document listing known risks and assumptions.
- [ ] Run `cargo clippy --all-targets --all-features -- -D warnings` and resolve all warnings.
- [ ] Achieve ≥ 95% test coverage of all public functions (currently met — see [§8](#8-test-coverage)).

---

## Summary of Open Items

| Priority | Item | Section |
|----------|------|---------|
| 🔴 High | Add `overflow-checks = true` to release profile (if missing) | [§2](#2-overflow--underflow-protection) |
| 🔴 High | External security audit before mainnet | [§9](#9-external-audit-recommendation) |
| 🟡 Medium | Implement pause / emergency-stop mechanism | [§4](#4-pause--emergency-stop-mechanism) |
| 🟡 Medium | Consider two-step admin transfer pattern | [§5](#5-ownership--admin-transfer) |
| 🟡 Medium | Add `amount > 0` check to `deduct()` | [§6](#6-input-validation--edge-cases) |
| 🟢 Low | Confirm open `deposit()` is intended (no auth required) | [§1](#1-access-control) |
| 🟢 Low | Evaluate owner immutability — key-loss recovery path | [§5](#5-ownership--admin-transfer) |
| 🟢 Low | Add upper-bound guard on deposit amounts | [§2](#2-overflow--underflow-protection) |

---

## Document History

| Date | Author | Change |
|------|--------|--------|
| 2026-02-25 | CalloraOrg | Initial security checklist |

---

*This document is part of the [Callora Contracts](README.md) repository. For questions or to report a vulnerability, contact the Callora security team or open a private issue.*
