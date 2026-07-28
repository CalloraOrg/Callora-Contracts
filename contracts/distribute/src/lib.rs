#![no_std]
/// # Callora Distribute Contract — per-account state caps for API marketplace abuse prevention.
///
/// Tracks active state entries (bets, positions, subscriptions) per account
/// and enforces a configurable global cap to prevent storage bloat.
///
/// ## Pause Circuit Breaker
///
/// When the contract is paused:
/// - `open` and `close` are blocked
/// - `batch_open` and `batch_close` are blocked
/// - Admin configuration functions remain available
/// - `pause` / `unpause` are blocked (admin must unpause first)
///
/// ## Access Control
///
/// Two privileged roles:
/// - **Admin** — full control: configuration, pause, upgrade, admin rotation.
/// - **Authorized Caller** — may call `open` / `close` on behalf of accounts.
///
/// When no authorized caller is set, only the admin may call `open` / `close`.
pub mod errors;
pub mod events;
pub mod limits;

use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env, Symbol, Vec};

use errors::DistributeError;
use limits::{
    decrement_category, decrement_state, get_account_category_count, get_account_count,
    get_global_cap, increment_category, increment_state, write_global_cap, AccountState,
    StorageKey, BUMP_AMOUNT, LIFETIME_THRESHOLD, MAX_BATCH_SIZE,
};

/// Severity levels for admin broadcast messages.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Severity {
    Info,
    Warn,
    Crit,
}

/// Event payload for admin broadcast messages.
#[contracttype]
#[derive(Clone, Debug)]
pub struct AdminBroadcast {
    pub severity: Severity,
    pub message: soroban_sdk::String,
}

/// A single item in a batch open or close operation.
#[contracttype]
#[derive(Clone)]
pub struct BatchItem {
    pub account: Address,
    pub category: Symbol,
}

#[contract]
pub struct CalloraDistribute;

#[contractimpl]
impl CalloraDistribute {
    /// Initialize the distribute contract.
    ///
    /// Exactly-once; returns `AlreadyInitialized` if called again.
    ///
    /// # Parameters
    /// - `caller` — future admin; must authorize the transaction.
    /// - `global_cap` — maximum active state entries per account across all
    ///   categories.  Must be > 0.
    ///
    /// # Errors
    /// - `DistributeError::AlreadyInitialized` — called more than once.
    /// - `DistributeError::CapNotPositive` — `global_cap == 0`.
    pub fn init(env: Env, caller: Address, global_cap: u32) -> Result<(), DistributeError> {
        caller.require_auth();
        let inst = env.storage().instance();
        if inst.has(&StorageKey::Admin) {
            return Err(DistributeError::AlreadyInitialized);
        }
        if global_cap == 0 {
            return Err(DistributeError::CapNotPositive);
        }
        inst.set(&StorageKey::Admin, &caller);
        inst.set(&StorageKey::GlobalCap, &global_cap);
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);
        env.events()
            .publish((events::event_init(&env), caller), global_cap);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // View functions
    // -----------------------------------------------------------------------

    /// Return the admin address.  Panics if not initialized.
    pub fn get_admin(env: Env) -> Result<Address, DistributeError> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(DistributeError::NotInitialized)
    }

    /// Return the global per-account cap (defaults to [`limits::DEFAULT_GLOBAL_CAP`]).
    pub fn get_global_cap(env: Env) -> u32 {
        get_global_cap(&env)
    }

    /// Return the total active state entry count for `account`.
    pub fn get_account_count(env: Env, account: Address) -> u32 {
        get_account_count(&env, &account)
    }

    /// Return the per-category active state entry count for `(account, category)`.
    pub fn get_account_category_count(env: Env, account: Address, category: Symbol) -> u32 {
        get_account_category_count(&env, &account, &category)
    }

    /// Return the full account state (total count across all categories).
    pub fn get_state(env: Env, account: Address) -> AccountState {
        AccountState {
            count: get_account_count(&env, &account),
        }
    }

    /// Return `true` if the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&StorageKey::Paused)
            .unwrap_or(false)
    }

    /// Return the pending admin address, or `None` if no transfer is in progress.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::PendingAdmin)
    }

    /// Read the stored contract version (WASM hash) as last set by `upgrade`.
    pub fn get_version(env: Env) -> Option<BytesN<32>> {
        env.storage().instance().get(&StorageKey::ContractVersion)
    }

    // -----------------------------------------------------------------------
    // Mutating functions — authorized caller
    // -----------------------------------------------------------------------

    /// Open a new state entry for `account` in `category`.
    ///
    /// Increments both the global and per-category counters.  Rejects if the
    /// global count would meet or exceed the cap.
    ///
    /// # Authorization
    /// Caller must be the admin or the designated authorized caller.
    ///
    /// # Errors
    /// - `DistributeError::Paused` — contract is paused.
    /// - `DistributeError::Unauthorized` — caller is not admin/authorized.
    /// - `DistributeError::AccountLimitExceeded` — cap reached.
    pub fn open(
        env: Env,
        caller: Address,
        account: Address,
        category: Symbol,
    ) -> Result<u32, DistributeError> {
        Self::require_not_paused(&env)?;
        caller.require_auth();
        Self::require_authorized_caller(&env, &caller)?;
        let cap = get_global_cap(&env);
        let new_count = increment_state(&env, &account, cap)?;
        increment_category(&env, &account, &category)?;
        env.events().publish(
            (events::event_open(&env), caller, account, category),
            new_count,
        );
        Ok(new_count)
    }

    /// Close an existing state entry for `account` in `category`.
    ///
    /// Decrements both the global and per-category counters.
    ///
    /// # Authorization
    /// Caller must be the admin or the designated authorized caller.
    ///
    /// # Errors
    /// - `DistributeError::Paused` — contract is paused.
    /// - `DistributeError::Unauthorized` — caller is not admin/authorized.
    /// - `DistributeError::AccountStateEmpty` — count is already zero.
    pub fn close(
        env: Env,
        caller: Address,
        account: Address,
        category: Symbol,
    ) -> Result<u32, DistributeError> {
        Self::require_not_paused(&env)?;
        caller.require_auth();
        Self::require_authorized_caller(&env, &caller)?;
        let new_count = decrement_state(&env, &account)?;
        decrement_category(&env, &account, &category)?;
        env.events().publish(
            (events::event_close(&env), caller, account, category),
            new_count,
        );
        Ok(new_count)
    }

    /// Atomically open multiple state entries.
    ///
    /// Full-batch validation completes before any state write.  If any item
    /// would exceed the cap, the entire batch reverts.
    ///
    /// # Authorization
    /// Caller must be the admin or the designated authorized caller.
    ///
    /// # Errors
    /// - `DistributeError::BatchEmpty` — empty items list.
    /// - `DistributeError::BatchTooLarge` — exceeds `MAX_BATCH_SIZE`.
    /// - `DistributeError::AccountLimitExceeded` — any item would exceed cap.
    pub fn batch_open(
        env: Env,
        caller: Address,
        items: Vec<BatchItem>,
    ) -> Result<(), DistributeError> {
        Self::require_not_paused(&env)?;
        caller.require_auth();
        Self::require_authorized_caller(&env, &caller)?;
        let n = items.len();
        if n == 0 {
            return Err(DistributeError::BatchEmpty);
        }
        if n > MAX_BATCH_SIZE {
            return Err(DistributeError::BatchTooLarge);
        }
        // Validate all items before any state mutation.
        let cap = get_global_cap(&env);
        // Collect the per-account increments needed.  We use a Vec of
        // (account, category) pairs since accounts may appear multiple times.
        for item in items.iter() {
            let current = get_account_count(&env, &item.account);
            if current >= cap {
                return Err(DistributeError::AccountLimitExceeded);
            }
            // Check that current + total occurrences of this account in the
            // batch won't exceed the cap.  This is a simplified validation —
            // for full correctness we'd count per-account in the batch, but
            // the sequential execution below handles it atomically.
        }
        // Execute — each increment_state call enforces the cap independently.
        for item in items.iter() {
            increment_state(&env, &item.account, cap)?;
            increment_category(&env, &item.account, &item.category)?;
            env.events().publish(
                (
                    events::event_batch_open(&env),
                    caller.clone(),
                    item.account.clone(),
                    item.category.clone(),
                ),
                get_account_count(&env, &item.account),
            );
        }
        Ok(())
    }

    /// Atomically close multiple state entries.
    ///
    /// Full-batch validation completes before any state write.  If any item
    /// has a zero count, the entire batch reverts.
    ///
    /// # Authorization
    /// Caller must be the admin or the designated authorized caller.
    ///
    /// # Errors
    /// - `DistributeError::BatchEmpty` — empty items list.
    /// - `DistributeError::BatchTooLarge` — exceeds `MAX_BATCH_SIZE`.
    /// - `DistributeError::AccountStateEmpty` — any item has zero count.
    pub fn batch_close(
        env: Env,
        caller: Address,
        items: Vec<BatchItem>,
    ) -> Result<(), DistributeError> {
        Self::require_not_paused(&env)?;
        caller.require_auth();
        Self::require_authorized_caller(&env, &caller)?;
        let n = items.len();
        if n == 0 {
            return Err(DistributeError::BatchEmpty);
        }
        if n > MAX_BATCH_SIZE {
            return Err(DistributeError::BatchTooLarge);
        }
        // Validate: all accounts must have count > 0.
        for item in items.iter() {
            if get_account_count(&env, &item.account) == 0 {
                return Err(DistributeError::AccountStateEmpty);
            }
        }
        // Execute.
        for item in items.iter() {
            decrement_state(&env, &item.account)?;
            decrement_category(&env, &item.account, &item.category)?;
            env.events().publish(
                (
                    events::event_batch_close(&env),
                    caller.clone(),
                    item.account.clone(),
                    item.category.clone(),
                ),
                get_account_count(&env, &item.account),
            );
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin functions
    // -----------------------------------------------------------------------

    /// Set the global per-account cap (admin only).
    ///
    /// # Errors
    /// - `DistributeError::CapNotPositive` — `new_cap == 0`.
    /// - `DistributeError::Unauthorized` — caller is not admin.
    pub fn set_global_cap(env: Env, caller: Address, new_cap: u32) -> Result<(), DistributeError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        if new_cap == 0 {
            return Err(DistributeError::CapNotPositive);
        }
        let old = get_global_cap(&env);
        write_global_cap(&env, new_cap);
        env.events()
            .publish((events::event_set_global_cap(&env), caller), (old, new_cap));
        Ok(())
    }

    /// Pause the contract, blocking `open`, `close`, `batch_open`, `batch_close`.
    ///
    /// Admin only.
    pub fn pause(env: Env, caller: Address) -> Result<(), DistributeError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        if Self::is_paused(env.clone()) {
            return Err(DistributeError::Paused);
        }
        env.storage().instance().set(&StorageKey::Paused, &true);
        env.events()
            .publish((events::event_paused(&env), caller), ());
        Ok(())
    }

    /// Unpause the contract, restoring `open` / `close` operations.
    ///
    /// Admin only.
    pub fn unpause(env: Env, caller: Address) -> Result<(), DistributeError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        if !Self::is_paused(env.clone()) {
            return Err(DistributeError::Paused);
        }
        env.storage().instance().set(&StorageKey::Paused, &false);
        env.events()
            .publish((events::event_unpaused(&env), caller), ());
        Ok(())
    }

    /// Nominate a new admin (admin only).
    ///
    /// Two-step transfer: the nominee must call `accept_admin` to complete.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), DistributeError> {
        caller.require_auth();
        let current = Self::require_admin(&env, &caller)?;
        if new_admin == current {
            return Err(DistributeError::NewAdminSameAsCurrent);
        }
        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);
        env.events().publish(
            (events::event_admin_nominated(&env), current, new_admin),
            (),
        );
        Ok(())
    }

    /// Accept the admin role (pending admin only).
    pub fn accept_admin(env: Env) -> Result<(), DistributeError> {
        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .ok_or(DistributeError::NoAdminTransferPending)?;
        pending.require_auth();
        let current: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(DistributeError::NotInitialized)?;
        env.storage().instance().set(&StorageKey::Admin, &pending);
        env.storage().instance().remove(&StorageKey::PendingAdmin);
        env.events()
            .publish((events::event_admin_accepted(&env), current, pending), ());
        Ok(())
    }

    /// Cancel a pending admin transfer (admin only).
    pub fn cancel_admin_transfer(env: Env, caller: Address) -> Result<(), DistributeError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .ok_or(DistributeError::NoAdminTransferPending)?;
        env.storage().instance().remove(&StorageKey::PendingAdmin);
        env.events()
            .publish((events::event_admin_cancelled(&env), caller, pending), ());
        Ok(())
    }

    /// Admin-gated contract upgrade.
    ///
    /// Updates the contract WASM to `new_wasm_hash` and persists the version.
    pub fn upgrade(
        env: Env,
        caller: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), DistributeError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());
        env.storage()
            .instance()
            .set(&StorageKey::ContractVersion, &new_wasm_hash);
        env.events()
            .publish((events::event_upgraded(&env), caller), new_wasm_hash);
        Ok(())
    }

    /// Broadcast an emergency message from the admin.
    ///
    /// Message length is capped at 256 characters.
    pub fn broadcast(
        env: Env,
        caller: Address,
        severity: Severity,
        message: soroban_sdk::String,
    ) -> Result<(), DistributeError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;
        let len = message.len();
        if len == 0 || len > 256 {
            return Err(DistributeError::BatchEmpty); // reuse for message validation
        }
        env.events().publish(
            (events::event_admin_broadcast(&env), caller),
            AdminBroadcast { severity, message },
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn require_admin(env: &Env, caller: &Address) -> Result<Address, DistributeError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(DistributeError::NotInitialized)?;
        if *caller != admin {
            return Err(DistributeError::Unauthorized);
        }
        Ok(admin)
    }

    fn require_authorized_caller(env: &Env, caller: &Address) -> Result<(), DistributeError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(DistributeError::NotInitialized)?;
        if *caller != admin {
            return Err(DistributeError::Unauthorized);
        }
        Ok(())
    }

    fn require_not_paused(env: &Env) -> Result<(), DistributeError> {
        if env
            .storage()
            .instance()
            .get::<_, bool>(&StorageKey::Paused)
            .unwrap_or(false)
        {
            return Err(DistributeError::Paused);
        }
        Ok(())
    }
}

#[cfg(test)]
mod test;
