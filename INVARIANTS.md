## Vault Balance Invariant

**Invariant**: For every reachable state of the `CalloraVault` contract, the stored balance in `VaultMeta.balance` is always **greater than or equal to 0**.

- **Storage field**: `VaultMeta.balance : i128`
- **Accessors**:
  - `get_meta(env: Env) -> VaultMeta`
  - `balance(env: Env) -> i128`
- **Guarantee**: Any value returned by `get_meta(env).balance` or `balance(env)` is **never negative**.

This document lists all functions that can change the stored balance and the pre-/post-conditions that preserve this invariant.

---

## Functions That Modify Balance

Only the following functions mutate `VaultMeta.balance`:

- `init(env, owner, usdc_token, initial_balance, min_deposit, revenue_pool, max_deduct)`
- `deposit(env, from, amount)`
- `deduct(env, caller, amount, request_id)`
- `batch_deduct(env, caller, items: Vec<DeductItem>)`
- `withdraw(env, amount)`
- `withdraw_to(env, to, amount)`

Helper and view functions such as `get_meta`, `get_max_deduct`, `get_revenue_pool`, `get_admin`, and `balance` **do not** modify balance.

---

### `init`

**Effect on balance**  
- Sets `VaultMeta.balance` to `initial_balance.unwrap_or(0)`.

**Pre-conditions**
- Vault is not already initialized:
  - `!env.storage().instance().has(META_KEY)`
- `max_deduct.unwrap_or(DEFAULT_MAX_DEDUCT) > 0`
- If `initial_balance > 0`, the contract already holds at least that much USDC:
  - `usdc.balance(current_contract_address) >= initial_balance`

**Post-conditions**
- `VaultMeta.balance == initial_balance.unwrap_or(0)`
- `VaultMeta.balance >= 0` (since `initial_balance` is an `i128` and enforced via the token-balance check).

---

### `deposit`

**Effect on balance**  
- Increases `VaultMeta.balance` by `amount`:
  - `balance' = balance + amount`

**Pre-conditions**
- Caller is authorized:
  - `from.require_auth()`
- Vault is initialized (via `get_meta` and USDC address lookup).
- Vault is **not paused**:
  - `is_paused(env) == false` (deposit aborts with `"vault is paused"` if paused).
- Amount satisfies the minimum deposit:
  - `amount >= meta.min_deposit`
- USDC transfer-from must succeed:
  - Token contract must allow `current_contract_address` to transfer `amount` from `from` to `current_contract_address`.

**Post-conditions**
- `VaultMeta.balance' = balance + amount`
- Because `amount >= 0` in practice (negative amounts are not useful and would fail at the token layer) and `balance` is already non-negative, we maintain:
  - `VaultMeta.balance' >= 0`

---

### `deduct`

**Effect on balance**  
- Decreases `VaultMeta.balance` by `amount`:
  - `balance' = balance - amount`

**Pre-conditions**
- Caller is authorized:
  - `caller.require_auth()`
- Vault is initialized.
- Amount constraints:
  - `amount > 0`
  - `amount <= get_max_deduct(env)`
- Sufficient balance:
  - `meta.balance >= amount`

**Post-conditions**
- `VaultMeta.balance' = balance - amount`
- Because of the `meta.balance >= amount` assertion and `amount > 0`, we have:
  - `VaultMeta.balance' >= 0`

---

### `batch_deduct`

**Effect on balance**
- Total change: `balance' = balance - sum_i(amount_i)`.

**Pre-conditions**
- Caller is authorized: `caller.require_auth()`
- Vault is initialized.
- `1 <= items.len() <= MAX_BATCH_SIZE` (50)
- For every item: `item.amount > 0` and `item.amount <= get_max_deduct(env)`
- Cumulative deductions do not exceed balance:
  - Validated in a single pass before any state is written.

**Post-conditions**
- `VaultMeta.balance' = balance - sum_i(amount_i) >= 0`
- If **any** pre-condition fails, the call panics before storage is written —
  no partial balance update is possible.
- One `deduct` event is emitted per item, **only on success**, after state is written.

---

### `withdraw`

**Effect on balance**  
- Decreases `VaultMeta.balance` by `amount`:
  - `balance' = balance - amount`

**Pre-conditions**
- Vault is initialized.
- Only the owner may withdraw:
  - `meta.owner.require_auth()`
- Amount constraints:
  - `amount > 0`
  - `meta.balance >= amount`

**Post-conditions**
- `VaultMeta.balance' = balance - amount`
- From `meta.balance >= amount` and `amount > 0`:
  - `VaultMeta.balance' >= 0`

---

### `withdraw_to`

**Effect on balance**  
- Decreases `VaultMeta.balance` by `amount`:
  - `balance' = balance - amount`

**Pre-conditions**
- Vault is initialized.
- Only the owner may withdraw:
  - `meta.owner.require_auth()`
- Amount constraints:
  - `amount > 0`
  - `meta.balance >= amount`

**Post-conditions**
- `VaultMeta.balance' = balance - amount`
- From `meta.balance >= amount` and `amount > 0`:
  - `VaultMeta.balance' >= 0`

---

## How Tests Support the Invariant

The test suite in `contracts/vault/src/test.rs` provides practical evidence for the non-negative balance invariant:

- **Deterministic fuzz test** (`fuzz_deposit_and_deduct`):
  - Randomly mixes deposits and deducts, asserting after each step that:
    - `balance() >= 0`
    - `balance()` matches a locally tracked expected value.
- **Batch deduct tests**:
  - `batch_deduct_success`, `batch_deduct_all_succeed`, `batch_deduct_all_revert`, and `batch_deduct_revert_preserves_balance` all verify that:
    - Successful batches leave balance consistent with expectations.
    - Failing batches revert without corrupting balance.
- **Withdraw tests**:
  - `withdraw_owner_success`, `withdraw_exact_balance`, and `withdraw_exceeds_balance_fails` ensure that:
    - Withdrawals are only allowed up to the current balance.
    - Over-withdraw attempts panic before balance can become negative.

Together with the explicit pre-/post-conditions above, these tests help auditors and maintainers validate that **`VaultMeta.balance` is always non-negative** in all reachable states.

---

## Settlement Developer Credit Invariant

**Invariant**: For every reachable state of [`CalloraSettlement`](contracts/settlement/src/lib.rs#L45), every credited developer balance stored under [`DEVELOPER_BALANCES_KEY`](contracts/settlement/src/lib.rs#L42) is always **greater than or equal to 0**.

- **Storage field**: `Map<Address, i128>` stored at `DEVELOPER_BALANCES_KEY`
- **Accessors**:
  - [`get_developer_balance(env: Env, developer: Address) -> i128`](contracts/settlement/src/lib.rs#L163)
  - [`get_all_developer_balances(env: Env) -> Vec<DeveloperBalance>`](contracts/settlement/src/lib.rs#L172)
- **Guarantee**: Any developer balance returned by these accessors is **never negative**.

This document lists all functions that can change credited developer balances and the pre-/post-conditions that preserve this invariant.

---

## Functions That Modify Credited Developer Balances

Only the following functions mutate the developer-balance map in the settlement contract:

- [`init(env, admin, vault_address)`](contracts/settlement/src/lib.rs#L51)
- [`receive_payment(env, caller, amount, to_pool, developer)`](contracts/settlement/src/lib.rs#L80) when `to_pool == false`

Helper and admin functions such as [`get_developer_balance`](contracts/settlement/src/lib.rs#L163), [`get_all_developer_balances`](contracts/settlement/src/lib.rs#L172), [`get_admin`](contracts/settlement/src/lib.rs#L140), [`get_vault`](contracts/settlement/src/lib.rs#L148), [`get_global_pool`](contracts/settlement/src/lib.rs#L156), [`set_admin`](contracts/settlement/src/lib.rs#L186), and [`set_vault`](contracts/settlement/src/lib.rs#L198) **do not** modify credited developer balances.

---

### `init`

**Effect on credited balances**  
- Stores an empty `Map<Address, i128>` at `DEVELOPER_BALANCES_KEY`.

**Pre-conditions**
- Settlement contract is not already initialized:
  - `!env.storage().instance().has(ADMIN_KEY)`

**Post-conditions**
- The credited-balance map is empty.
- Therefore every stored developer balance is vacuously non-negative.

---

### `receive_payment`

**Effect on credited balances**  
- If `to_pool == false`, increases the selected developer balance by `amount`:
  - `developer_balance' = developer_balance + amount`
- If `to_pool == true`, the developer-balance map is unchanged.

**Pre-conditions**
- Caller passes the settlement authorization gate:
  - [`require_authorized_caller(env, caller)`](contracts/settlement/src/lib.rs#L210)
  - This requires `caller == get_vault(env)` or `caller == get_admin(env)`.
- Positive credit amount:
  - `amount > 0`
- If `to_pool == false`, a developer address must be supplied:
  - `developer.is_some()`

**Post-conditions**
- For the `to_pool == false` branch:
  - `developer_balance' = developer_balance + amount`
  - Because `developer_balance >= 0` by the inductive hypothesis and `amount > 0`, we maintain:
    - `developer_balance' > developer_balance >= 0`
- All other developers' balances are unchanged.
- For the `to_pool == true` branch, the developer-balance map is unchanged, so the invariant is preserved.
- If any pre-condition fails, the call reverts and the original credited balances are preserved.

---

## How Tests Support the Invariant

The test suite in `contracts/settlement/src/test.rs` provides practical evidence for the non-negative credited-balance invariant:

- **Developer credit test** (`test_receive_payment_to_developer`):
  - Verifies that a positive settlement credit creates a positive developer balance while leaving the global pool unchanged.
- **Accumulation test** (`test_receive_multiple_payments_accumulate`):
  - Verifies repeated credits to the same developer are additive and remain non-negative.
- **Missing developer guard** (`test_receive_payment_pool_false_no_developer`):
  - Verifies the contract rejects the only branch that could otherwise write an ill-formed developer credit.
- **Authorization and amount guards** (`test_receive_payment_unauthorized`, `test_receive_payment_zero_amount`):
  - Verify unauthorized or zero-amount calls revert before credited balances can be corrupted.

Together with the explicit pre-/post-conditions above, these tests help auditors and maintainers validate that **settlement developer credits are always non-negative** in all reachable states.

---

## Settlement Global Pool Accounting Invariant

**Invariant**: For every reachable state of [`CalloraSettlement`](contracts/settlement/src/lib.rs#L45), [`GlobalPool.total_balance`](contracts/settlement/src/lib.rs#L16) is always **greater than or equal to 0**, and equals the initial `0` plus the sum of all successful [`receive_payment(..., to_pool = true, ...)`](contracts/settlement/src/lib.rs#L80) credits since initialization.

- **Storage field**: [`GlobalPool`](contracts/settlement/src/lib.rs#L16) stored at `GLOBAL_POOL_KEY`
- **Accessor**:
  - [`get_global_pool(env: Env) -> GlobalPool`](contracts/settlement/src/lib.rs#L156)
- **Guarantee**:
  - `get_global_pool(env).total_balance >= 0`
  - `receive_payment(..., to_pool = false, ...)` leaves `GlobalPool.total_balance` unchanged

This invariant is intentionally about **internal accounting state**. The current settlement contract only records credits; it does not implement a debit path from `GlobalPool.total_balance`, so this field is a monotonic accounting counter rather than a proof of withdrawable USDC.

---

## Functions That Modify Global Pool Accounting

Only the following functions mutate `GlobalPool`:

- [`init(env, admin, vault_address)`](contracts/settlement/src/lib.rs#L51)
- [`receive_payment(env, caller, amount, to_pool, developer)`](contracts/settlement/src/lib.rs#L80) when `to_pool == true`

Helper and admin functions such as [`get_global_pool`](contracts/settlement/src/lib.rs#L156), [`get_developer_balance`](contracts/settlement/src/lib.rs#L163), [`get_all_developer_balances`](contracts/settlement/src/lib.rs#L172), [`set_admin`](contracts/settlement/src/lib.rs#L186), and [`set_vault`](contracts/settlement/src/lib.rs#L198) **do not** modify global-pool accounting.

---

### `init`

**Effect on global pool accounting**  
- Stores:
  - `GlobalPool { total_balance: 0, last_updated: env.ledger().timestamp() }`

**Pre-conditions**
- Settlement contract is not already initialized:
  - `!env.storage().instance().has(ADMIN_KEY)`

**Post-conditions**
- `GlobalPool.total_balance == 0`
- `GlobalPool.last_updated` equals the current ledger timestamp at initialization.
- Because the initialized pool balance is `0`, the non-negativity and additive-accounting invariants both hold.

---

### `receive_payment`

**Effect on global pool accounting**  
- If `to_pool == true`, increases `GlobalPool.total_balance` by `amount`:
  - `total_balance' = total_balance + amount`
- If `to_pool == false`, `GlobalPool` is unchanged.

**Pre-conditions**
- Caller passes [`require_authorized_caller(env, caller)`](contracts/settlement/src/lib.rs#L210).
- Positive credit amount:
  - `amount > 0`

**Post-conditions**
- For the `to_pool == true` branch:
  - `total_balance' = total_balance + amount`
  - `last_updated' = env.ledger().timestamp()`
  - Because `total_balance >= 0` by the inductive hypothesis and `amount > 0`, we maintain:
    - `total_balance' > total_balance >= 0`
- For the `to_pool == false` branch:
  - `GlobalPool.total_balance' = GlobalPool.total_balance`
  - `GlobalPool.last_updated' = GlobalPool.last_updated`
- If any pre-condition fails, the call reverts and the original global-pool accounting is preserved.

---

## How Tests Support the Invariant

The test suite in `contracts/settlement/src/test.rs` provides practical evidence for the global-pool accounting invariant:

- **Initialization test** (`test_settlement_initialization`):
  - Verifies that `get_global_pool().total_balance` starts at `0`.
- **Pool credit test** (`test_receive_payment_to_pool`):
  - Verifies a successful pool credit increments `total_balance` by the credited amount.
- **Developer credit isolation test** (`test_receive_payment_to_developer`):
  - Verifies developer-directed credits do not mutate `GlobalPool.total_balance`.
- **Admin caller path** (`test_admin_can_receive_payment`):
  - Verifies the admin can use the same guarded credit path and the accounting update remains additive.
- **Authorization and amount guards** (`test_receive_payment_unauthorized`, `test_receive_payment_zero_amount`):
  - Verify invalid calls revert before `GlobalPool` can be modified.

Together with the explicit pre-/post-conditions above, these tests help auditors and maintainers validate that **settlement global-pool accounting remains non-negative and additive** in all reachable states.

---

## Cross-Contract Authorization Invariant

**Invariant**: Only explicitly authorized principals may route funds out of the vault, credit settlement balances, reconfigure downstream contract addresses, or distribute USDC from the revenue pool.

- **Settlement guarantee**:
  - Only the registered vault or current settlement admin can invoke [`receive_payment`](contracts/settlement/src/lib.rs#L80).
  - Only the current settlement admin can invoke [`set_admin`](contracts/settlement/src/lib.rs#L186) and [`set_vault`](contracts/settlement/src/lib.rs#L198).
- **Revenue pool guarantee**:
  - Only the current revenue-pool admin can invoke [`set_admin`](contracts/revenue_pool/src/lib.rs#L67), [`receive_payment`](contracts/revenue_pool/src/lib.rs#L95), [`distribute`](contracts/revenue_pool/src/lib.rs#L125), and [`batch_distribute`](contracts/revenue_pool/src/lib.rs#L171).
- **Vault routing guarantee**:
  - Only an authenticated owner or stored authorized caller can invoke [`deduct`](contracts/vault/src/lib.rs#L304) and [`batch_deduct`](contracts/vault/src/lib.rs#L347).
  - Only the vault admin can invoke [`set_settlement`](contracts/vault/src/lib.rs#L467), which controls the settlement destination used by vault deductions.

This invariant is the authorization counterpart to the accounting invariants above: balances remain meaningful only if state-changing entry points are reachable by the intended principals.

---

## Functions That Enforce Authorization Constraints

The following functions are the relevant state-changing gates across the vault, settlement, and revenue-pool flow:

- Vault:
  - [`deduct(env, caller, amount, request_id)`](contracts/vault/src/lib.rs#L304)
  - [`batch_deduct(env, caller, items)`](contracts/vault/src/lib.rs#L347)
  - [`set_settlement(env, caller, settlement_address)`](contracts/vault/src/lib.rs#L467)
- Settlement:
  - [`receive_payment(env, caller, amount, to_pool, developer)`](contracts/settlement/src/lib.rs#L80)
  - [`set_admin(env, caller, new_admin)`](contracts/settlement/src/lib.rs#L186)
  - [`set_vault(env, caller, new_vault)`](contracts/settlement/src/lib.rs#L198)
  - [`require_authorized_caller(env, caller)`](contracts/settlement/src/lib.rs#L210)
- Revenue pool:
  - [`init(env, admin, usdc_token)`](contracts/revenue_pool/src/lib.rs#L28)
  - [`set_admin(env, caller, new_admin)`](contracts/revenue_pool/src/lib.rs#L67)
  - [`receive_payment(env, caller, amount, from_vault)`](contracts/revenue_pool/src/lib.rs#L95)
  - [`distribute(env, caller, to, amount)`](contracts/revenue_pool/src/lib.rs#L125)
  - [`batch_distribute(env, caller, payments)`](contracts/revenue_pool/src/lib.rs#L171)

Pure accessors such as [`get_admin`](contracts/settlement/src/lib.rs#L140), [`get_vault`](contracts/settlement/src/lib.rs#L148), [`get_global_pool`](contracts/settlement/src/lib.rs#L156), [`get_admin`](contracts/revenue_pool/src/lib.rs#L51), and [`balance`](contracts/revenue_pool/src/lib.rs#L217) **do not** weaken the authorization invariant because they are read-only.

---

### Vault routing entry points

**Effect on authorization-sensitive state**  
- [`deduct`](contracts/vault/src/lib.rs#L304) and [`batch_deduct`](contracts/vault/src/lib.rs#L347) are the only paths that route funds from the vault to a configured settlement or revenue-pool contract.
- [`set_settlement`](contracts/vault/src/lib.rs#L467) is the configuration entry point that changes where settlement-directed deductions are sent.

**Pre-conditions**
- `deduct` / `batch_deduct`:
  - `caller.require_auth()`
  - Caller is the vault owner or the stored `authorized_caller`.
- `set_settlement`:
  - `caller.require_auth()`
  - `caller == get_admin(env)`

**Post-conditions**
- Unauthorized callers cannot trigger downstream fund routing from the vault.
- Unauthorized callers cannot repoint the settlement destination used by the vault.

---

### Settlement entry points

**Effect on authorization-sensitive state**  
- [`receive_payment`](contracts/settlement/src/lib.rs#L80) is the only settlement entry point that mutates developer credits or global-pool accounting.
- [`set_admin`](contracts/settlement/src/lib.rs#L186) and [`set_vault`](contracts/settlement/src/lib.rs#L198) mutate the principals allowed to administer or feed settlement accounting.

**Pre-conditions**
- `receive_payment`:
  - Caller must satisfy [`require_authorized_caller`](contracts/settlement/src/lib.rs#L210):
    - `caller == get_vault(env)` or `caller == get_admin(env)`
- `set_admin` / `set_vault`:
  - `caller.require_auth()`
  - `caller == get_admin(env)`

**Post-conditions**
- No address other than the configured vault or current settlement admin can create accounting entries.
- No address other than the current settlement admin can rotate settlement admin or vault authority.

---

### Revenue-pool entry points

**Effect on authorization-sensitive state**  
- [`init`](contracts/revenue_pool/src/lib.rs#L28) establishes the initial admin.
- [`set_admin`](contracts/revenue_pool/src/lib.rs#L67) rotates the admin.
- [`receive_payment`](contracts/revenue_pool/src/lib.rs#L95) emits revenue-credit events.
- [`distribute`](contracts/revenue_pool/src/lib.rs#L125) and [`batch_distribute`](contracts/revenue_pool/src/lib.rs#L171) move USDC out of the contract.

**Pre-conditions**
- `init`:
  - `admin.require_auth()`
  - The contract is not already initialized.
- `set_admin`, `receive_payment`, `distribute`, `batch_distribute`:
  - Caller authenticates with `caller.require_auth()`
  - `caller == get_admin(env)`
- `distribute` / `batch_distribute` also require:
  - Positive amount(s)
  - Sufficient on-contract USDC balance before transfer

**Post-conditions**
- No address other than the current revenue-pool admin can emit administrative payment events or move USDC out of the revenue pool.
- Failed authorization checks revert before any payout or admin rotation occurs.

---

## How Tests Support the Invariant

The settlement, vault, and revenue-pool test suites provide practical evidence for the authorization invariant:

- **Settlement authorization tests** (`test_receive_payment_unauthorized`, `test_set_admin_unauthorized`, `test_set_vault_unauthorized` in `contracts/settlement/src/test.rs`):
  - Verify unauthorized callers cannot mutate settlement accounting or configuration.
- **Revenue-pool authorization tests** (`distribute_unauthorized_panics`, `set_admin_unauthorized_panics` in `contracts/revenue_pool/src/test.rs`):
  - Verify unauthorized callers cannot distribute funds or rotate revenue-pool control.
- **Vault routing authorization test** (`set_settlement_unauthorized_panics` in `contracts/vault/src/test.rs`):
  - Verifies unauthorized callers cannot change the vault's settlement destination.

Together with the explicit pre-/post-conditions above, these tests help auditors and maintainers validate that **cross-contract routing, accounting, and payout actions remain reachable only by the intended principals**.
