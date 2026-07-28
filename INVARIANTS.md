## Vault Balance Invariant

**Invariant**: For every reachable state of the `CalloraVault` contract, the stored balance in `VaultMeta.balance` is always **greater than or equal to 0** and **less than or equal to i128::MAX**.

- **Storage field**: `VaultMeta.balance : i128`
- **Accessors**:
  - `get_meta(env: Env) -> VaultMeta`
  - `balance(env: Env) -> i128`
- **Guarantee**: Any value returned by `get_meta(env).balance` or `balance(env)` is **never negative** and **cannot overflow** the `i128` numeric boundary. Any operation that would cause an overflow (e.g., `deposit` past `i128::MAX`) will panic and revert the transaction.

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
  - `!env.storage().instance().has(MetaKey)`
- `initial_balance.unwrap_or(0) >= 0`
- `max_deduct.unwrap_or(DEFAULT_MAX_DEDUCT) > 0`
- The on-ledger USDC balance already covers the requested internal starting balance:
  - `usdc.balance(current_contract_address) >= initial_balance.unwrap_or(0)`

**Post-conditions**
- `VaultMeta.balance == initial_balance.unwrap_or(0)`
- `VaultMeta.balance >= 0` (because `initial_balance.unwrap_or(0)` is explicitly checked to be non-negative before storage is written).

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
- Vault is initialized and not paused.
- Amount constraints:
  - `amount > 0`
  - `amount <= get_max_deduct(env)`
- Sufficient balance:
  - `meta.balance >= amount`
- **Settlement configured (Issue #263)**:
  - `StorageKey::Settlement` is present — i.e. `set_settlement` has been called.
  - If absent, the call panics with `"settlement address not set"` before any
    balance mutation, guaranteeing no partial state update.

**Post-conditions**
- `VaultMeta.balance' = balance - amount`
- Because of the `meta.balance >= amount` assertion and `amount > 0`, we have:
  - `VaultMeta.balance' >= 0`
- The on-ledger USDC decrease at the vault equals the internal balance decrease
  (both equal `amount`), because the deducted USDC is always transferred to the
  settlement address.
- **Formal conservation proof**: `contracts/vault/proofs/deduct.rs` contains a
  Kani harness for the successful `deduct` transition. It models the tracked
  vault balance and settlement credit as one combined accounting total and
  proves that `balance + settlement_credit` is unchanged when `amount` is moved
  from the vault to settlement.

---

### `batch_deduct`

**Effect on balance**
- Total change: `balance' = balance - sum_i(amount_i)`.

**Pre-conditions**
- Caller is authorized: `caller.require_auth()`
- Vault is initialized and not paused.
- `1 <= items.len() <= MAX_BATCH_SIZE` (50)
- The explicit batch cap is a practical Soroban resource bound:
  it limits looped validation work, transfer/event overhead, and invocation
  footprint in one call. Tune this cap conservatively if production
  workloads approach network CPU or budget limits.
- For every item: `item.amount > 0` and `item.amount <= get_max_deduct(env)`
- Cumulative deductions do not exceed balance:
  - Validated in a single pass before any state is written.
- **Settlement configured (Issue #263)**: `StorageKey::Settlement` is present;
  missing settlement causes `"settlement address not set"` panic before any
  state write, so the batch is atomically reverted.

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
    - Duplicate `request_id` values inside a single batch are rejected atomically and do not change `meta.balance`.
- **Withdraw tests**:
  - `withdraw_owner_success`, `withdraw_exact_balance`, and `withdraw_exceeds_balance_fails` ensure that:
    - Withdrawals are only allowed up to the current balance.
    - Over-withdraw attempts panic before balance can become negative.

Together with the explicit pre-/post-conditions above, these tests help auditors and maintainers validate that **`VaultMeta.balance` is always non-negative** in all reachable states.

---

## Settlement Developer Credit Invariant

**Invariant**: For every reachable state of [`CalloraSettlement`](contracts/settlement/src/lib.rs#L45), every credited developer balance stored under [`DEVELOPER_BALANCES_KEY`](contracts/settlement/src/lib.rs#L42) is always **greater than or equal to 0**.

- **Storage field**: per-developer persistent storage entries keyed by `StorageKey::DeveloperBalance(Address)`
- **Accessors**:
  - [`get_developer_balance(env: Env, developer: Address) -> i128`](contracts/settlement/src/lib.rs#L163)
  - [`get_all_developer_balances(env: Env, caller: Address) -> Result<Vec<DeveloperBalance>, SettlementError>`](contracts/settlement/src/lib.rs#L493)
  - [`get_developer_balances_page(env: Env, caller: Address, start: u32, limit: u32) -> Result<Vec<DeveloperBalance>, SettlementError>`](contracts/settlement/src/lib.rs#L531)
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
- `batch_distribute` additionally requires:
  - `1 <= payments.len() <= MAX_BATCH_SIZE` (50)

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


---

## Cross-Contract Value Conservation

**Invariant**: For every vault deduction operation across the Callora protocol (vault, settlement, and revenue pool contracts), the absolute value of the change in vault balance must exactly equal the sum of changes in all destination accounting buckets. No token unit may duplicate or disappear into unallocated state.

### Mathematical Formulation

```text
abs(Δ vault_balance) = Δ settlement_pool + Δ settlement_developer_balances + Δ revenue_pool
```

Where:
- **Δ vault_balance**: Change in `VaultMeta.balance` (typically negative for deductions)
- **Δ settlement_pool**: Change in `GlobalPool.total_balance` in the settlement contract
- **Δ settlement_developer_balances**: Sum of changes across all individual developer balances in settlement
- **Δ revenue_pool**: Change in on-ledger USDC balance held by the revenue pool contract

### Accounting Buckets

1. **Vault Balance** (`VaultMeta.balance`)
   - Storage: `StorageKey::MetaKey` in vault contract
   - Accessor: [`balance(env: Env) -> i128`](contracts/vault/src/lib.rs)
   - Modified by: `init`, `deposit`, `deduct`, `batch_deduct`, `withdraw`, `withdraw_to`

2. **Settlement Global Pool** (`GlobalPool.total_balance`)
   - Storage: `StorageKey::GlobalPool` in settlement contract
   - Accessor: [`get_global_pool(env: Env) -> GlobalPool`](contracts/settlement/src/lib.rs)
   - Modified by: `receive_payment(..., to_pool=true, ...)`

3. **Settlement Developer Balances** (sum of all `DeveloperBalance` entries)
   - Storage: `StorageKey::DeveloperBalance(Address)` in settlement contract (persistent storage)
   - Accessor: [`get_developer_balance(env, developer)`](contracts/settlement/src/lib.rs), [`get_all_developer_balances(env, caller)`](contracts/settlement/src/lib.rs)
   - Modified by: `receive_payment(..., to_pool=false, developer=Some(...))`
   - Test Helper: `settlement_tests::get_total_developer_balances(env, settlement_addr, admin)` computes the sum

4. **Revenue Pool Balance** (on-ledger USDC held by revenue pool)
   - Storage: On-ledger token balance (not internal accounting)
   - Accessor: `token::Client.balance(&revenue_pool_address)`
   - Modified by: `distribute`, `batch_distribute` (outbound), and external USDC transfers (inbound)

### Value Flow Architecture

```text
┌─────────────────┐
│  CalloraVault   │
│   (Internal     │
│   Accounting)   │
└────────┬────────┘
         │ deduct / batch_deduct
         ▼
    ┌────────────────────────────────────┐
    │   USDC Token Transfer              │
    │   (vault → settlement or revenue)  │
    └────────────┬───────────────────────┘
                 │
         ┌───────┴────────┐
         ▼                ▼
┌─────────────────┐  ┌──────────────────┐
│ CalloraSettlement│  │  RevenuePool     │
│  - Global Pool   │  │  (On-ledger USDC)│
│  - Dev Balances  │  │                  │
└──────────────────┘  └──────────────────┘
```

### Routing Rules

1. **Vault-Originated Deducts** (Current Implementation)
   - `deduct(env, caller, amount, request_id)` → Always routes to settlement global pool (`to_pool=true`)
   - `batch_deduct(env, caller, items)` → Always routes total to settlement global pool (`to_pool=true`)
   - Post-transfer: Vault calls `settlement.receive_payment(..., to_pool=true, developer=None)`

2. **Admin-Initiated Developer Credits**
   - `settlement.receive_payment(..., to_pool=false, developer=Some(addr))` → Credits specific developer
   - `settlement.batch_receive_payment(caller, items)` → Credits multiple developers atomically
   - These do **not** deduct from vault; they are administrative reallocations within settlement

3. **Revenue Pool Distribution**
   - `revenue_pool.distribute(caller, to, amount)` → Moves USDC out of revenue pool to developer
   - `revenue_pool.batch_distribute(caller, payments)` → Batch payout to multiple developers
   - These reduce `revenue_pool_balance` (on-ledger) but do not affect vault or settlement accounting

### Operations Covered by the Invariant

#### Single Deduct Operations
- **Entry Point**: [`deduct(env, caller, amount, request_id)`](contracts/vault/src/lib.rs)
- **Test Coverage**: [`conservation_scenario_1_standard_pool_routing`](contracts/vault/src/test.rs#conservation_invariant)
- **Pre-conditions**:
  - Vault is not paused
  - Caller is authorized (owner or `authorized_caller`)
  - `amount > 0` and `amount <= max_deduct`
  - Vault balance >= amount
  - Settlement address is configured
- **Atomicity**: Full validation before any state write; Soroban transaction atomicity ensures all-or-nothing
- **Conservation Path**:
  1. Vault balance decreases by `amount`
  2. USDC transfers from vault to settlement
  3. Settlement global pool increases by `amount`
  4. Result: `abs(Δ vault) = Δ pool`

#### Batch Deduct Operations
- **Entry Point**: [`batch_deduct(env, caller, items: Vec<DeductItem>)`](contracts/vault/src/lib.rs)
- **Test Coverage**:
  - [`conservation_scenario_3_zero_developer_batch`](contracts/vault/src/test.rs#conservation_invariant)
  - [`conservation_scenario_4_fully_pool_batch_max_size`](contracts/vault/src/test.rs#conservation_invariant)
- **Pre-conditions**:
  - Vault is not paused
  - Caller is authorized
  - `1 <= items.len() <= MAX_BATCH_SIZE` (50)
  - All items: `amount > 0`, `amount <= max_deduct`
  - Total deduction <= vault balance
  - Settlement address is configured
  - No duplicate `request_id` in batch or with previously processed requests
- **Atomicity**: Full batch validation before any transfer or state write
- **Conservation Path**:
  1. Vault balance decreases by `sum(items.amount)`
  2. USDC transfers from vault to settlement (single transfer for total)
  3. Settlement global pool increases by `sum(items.amount)`
  4. Result: `abs(Δ vault) = Δ pool`

#### Mixed Routing Scenarios
- **Test Coverage**: [`conservation_scenario_5_mixed_batch_routing`](contracts/vault/src/test.rs#conservation_invariant)
- **Scenario**:
  - Multiple vault deductions (batch or single)
  - Administrative developer credits via `settlement.batch_receive_payment`
  - Complex routing patterns with multiple destinations
- **Conservation Path**:
  1. Vault deductions route to settlement pool
  2. Admin actions reallocate pool → developer balances (within settlement)
  3. Aggregate: `abs(Δ vault) = Δ pool + Δ developer_balances`

### Safety Guarantees

1. **No Partial Updates**
   - All entry points use full validation before state writes
   - Soroban transaction atomicity: any panic reverts the entire transaction
   - See [Vault Balance Invariant](#vault-balance-invariant) for single-contract guarantees

2. **No Double-Spending**
   - Vault balance decreases **before** external USDC transfer (CEI pattern variant)
   - If transfer fails, Soroban reverts the balance decrease
   - Settlement credits only occur **after** successful USDC receipt

3. **No Value Loss**
   - Every deducted token unit must land in a destination bucket
   - Conservation test suite verifies `abs(Δ vault) = sum(Δ destinations)` across all scenarios
   - Failed operations leave all accounting buckets unchanged

4. **Idempotency Protection**
   - Optional `request_id` on `deduct`/`batch_deduct` prevents duplicate execution
   - Processed request markers live in temporary storage with TTL
   - Duplicate `request_id` returns `VaultError::DuplicateRequestId` before any state change

### Test Suite Implementation

The conservation invariant test suite is located in [`contracts/vault/src/test.rs`](contracts/vault/src/test.rs) under the `conservation_invariant` module.

#### Test Helper: `ConservationSnapshot`

```rust
struct ConservationSnapshot {
    vault_balance: i128,
    settlement_pool: i128,
    settlement_developer_total: i128,
    revenue_pool_balance: i128,
}
```

**Methods**:
- `capture(...)` → Snapshot current state across all contracts
- `delta(before, after)` → Compute deltas for each bucket
- `assert_conservation_invariant()` → Verify `abs(Δ vault) = Δ pool + Δ devs + Δ revenue`

#### Test Scenarios

| Scenario | Description | File Reference |
|----------|-------------|----------------|
| **Scenario 1** | Standard pool routing (`to_pool=true`) | `conservation_scenario_1_standard_pool_routing` |
| **Scenario 2** | Standard developer routing (`to_pool=false`) | `conservation_scenario_2_standard_developer_routing` |
| **Scenario 3** | Zero-developer batch (all to pool) | `conservation_scenario_3_zero_developer_batch` |
| **Scenario 4** | Fully-pool batch (50 items, max size) | `conservation_scenario_4_fully_pool_batch_max_size` |
| **Scenario 5** | Mixed batch routing (pool + developer credits) | `conservation_scenario_5_mixed_batch_routing` |

#### Running the Tests

```bash
# Run all conservation invariant tests
cargo test -p callora-vault conservation_invariant

# Run a specific scenario
cargo test -p callora-vault conservation_scenario_1_standard_pool_routing

# Run with output
cargo test -p callora-vault conservation_invariant -- --nocapture
```

#### Expected Output

```text
running 5 tests
test conservation_invariant::conservation_scenario_1_standard_pool_routing ... ok
test conservation_invariant::conservation_scenario_2_standard_developer_routing ... ok
test conservation_invariant::conservation_scenario_3_zero_developer_batch ... ok
test conservation_invariant::conservation_scenario_4_fully_pool_batch_max_size ... ok
test conservation_invariant::conservation_scenario_5_mixed_batch_routing ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Integration with Existing Invariants

This cross-contract conservation invariant **extends and refines** the existing single-contract invariants:

1. **Vault Balance Invariant** (see [above](#vault-balance-invariant))
   - Guarantees vault balance remains non-negative
   - Conservation invariant ensures deductions are fully accounted for **downstream**

2. **Settlement Developer Credit Invariant** (see [above](#settlement-developer-credit-invariant))
   - Guarantees developer balances remain non-negative
   - Conservation invariant ensures these credits originate from legitimate vault deductions

3. **Settlement Global Pool Accounting Invariant** (see [above](#settlement-global-pool-accounting-invariant))
   - Guarantees pool balance remains non-negative
   - Conservation invariant ensures pool credits match vault deductions

4. **Cross-Contract Authorization Invariant** (see [above](#cross-contract-authorization-invariant))
   - Guarantees only authorized principals can trigger value flow
   - Conservation invariant assumes authorization checks pass (tests use `mock_all_auths`)

### Audit Recommendations

When reviewing value conservation:

1. **Trace Deduction Paths**
   - Start from `deduct`/`batch_deduct` calls
   - Follow USDC transfer to settlement or revenue pool
   - Verify settlement accounting update (pool or developer balance)
   - Confirm no intermediate state allows value to escape

2. **Verify Atomicity**
   - Check that all validation occurs before any state write
   - Confirm Soroban transaction boundaries encompass all steps
   - Test failure scenarios to ensure full rollback

3. **Review Test Coverage**
   - Run `cargo test -p callora-vault conservation_invariant`
   - Verify all 5 scenarios pass
   - Inspect `ConservationSnapshot` logic for correctness
   - Check that helper functions (e.g., `get_total_developer_balances`) accurately sum balances

4. **Check Edge Cases**
   - Zero amounts (should panic before conservation violation)
   - Overflow scenarios (checked arithmetic prevents conservation violations via panic)
   - Concurrent operations (Soroban serializes transactions; no race conditions at contract level)
   - Request ID deduplication (prevents accidental double-deduction)

### Known Limitations

1. **Revenue Pool as Pass-Through**
   - Current architecture: Revenue pool is an endpoint, not a conservation participant
   - Distributions from revenue pool **reduce** its balance but do not affect vault/settlement
   - Future: If revenue pool becomes a routing intermediary, add `Δ revenue_pool` to conservation formula

2. **Admin-Initiated Credits**
   - `settlement.receive_payment(..., caller=admin)` can credit balances without vault deduction
   - Conservation invariant applies **per vault operation**, not per settlement credit
   - Admin credits are valid reallocation within settlement (e.g., refunds, adjustments)

3. **External USDC Transfers**
   - Anyone can transfer USDC directly to settlement or revenue pool
   - These do **not** trigger vault accounting changes
   - Conservation invariant is **not violated** because vault balance is unchanged
   - Tests focus on **vault-originated** value flow only

### References

- **Vault Contract**: [`contracts/vault/src/lib.rs`](contracts/vault/src/lib.rs)
- **Settlement Contract**: [`contracts/settlement/src/lib.rs`](contracts/settlement/src/lib.rs)
- **Revenue Pool Contract**: [`contracts/revenue_pool/src/lib.rs`](contracts/revenue_pool/src/lib.rs)
- **Test Suite**: [`contracts/vault/src/test.rs`](contracts/vault/src/test.rs) → `conservation_invariant` module
- **Settlement Test Helpers**: [`contracts/settlement/src/test.rs`](contracts/settlement/src/test.rs) → `get_total_developer_balances`, `get_settlement_pool_balance`

---

This cross-contract value conservation guarantee is the cornerstone of the Callora protocol's financial integrity. It ensures that every token unit deducted from the vault is precisely accounted for across the protocol's downstream contracts, with no possibility of duplication, loss, or unallocated state.
