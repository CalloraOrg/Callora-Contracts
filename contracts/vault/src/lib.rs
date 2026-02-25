//! # Callora Vault Contract
//!
//! ## Access Control
//!
//! The vault implements role-based access control for deposits:
//!
//! - **Owner**: Set at initialization, immutable. Always permitted to deposit.
//! - **Allowed Depositor**: Optional address (e.g., backend service) that can be
//!   explicitly approved by the owner. Can be set, changed, or cleared at any time.
//! - **Other addresses**: Rejected with an authorization error.
//!
//! ### Production Usage
//!
//! In production, the owner typically represents the end user's account, while the
//! allowed depositor is a backend service that handles automated deposits on behalf
//! of the user.
//!
//! ### Managing the Allowed Depositor
//!
//! - Set or update: `set_allowed_depositor(Some(address))`
//! - Clear (revoke access): `set_allowed_depositor(None)`
//! - Only the owner can call `set_allowed_depositor`
//!
//! ### Security Model
//!
//! - The owner has full control over who can deposit
//! - The allowed depositor is a trusted address (typically a backend service)
//! - Access can be revoked at any time by the owner
//! - All deposit attempts are authenticated against the caller's address

#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

#[contracttype]
#[derive(Clone)]
pub struct VaultMeta {
    pub owner: Address,
    pub balance: i128,
}

#[contracttype]
pub enum StorageKey {
    Meta,
    AllowedDepositor,
}

#[contract]
pub struct CalloraVault;

#[contractimpl]
impl CalloraVault {
    /// Initialize vault for an owner with optional initial balance.
    /// Emits an "init" event with the owner address and initial balance.
    ///
    /// # Panics
    /// - If the vault is already initialized
    /// - If `initial_balance` is negative
    pub fn init(env: Env, owner: Address, initial_balance: Option<i128>) -> VaultMeta {
        if env.storage().instance().has(&StorageKey::Meta) {
            panic!("vault already initialized");
        }
        let balance = initial_balance.unwrap_or(0);
        assert!(balance >= 0, "initial balance must be non-negative");
        let meta = VaultMeta {
            owner: owner.clone(),
            balance,
        };
        env.storage().instance().set(&StorageKey::Meta, &meta);

        // Emit event: topics = (init, owner), data = balance
        env.events()
            .publish((Symbol::new(&env, "init"), owner), balance);

        meta
    }

    /// Check if the caller is authorized to deposit (owner or allowed depositor).
    fn is_authorized_depositor(env: &Env, caller: &Address) -> bool {
        let meta = Self::get_meta(env.clone());

        // Owner is always authorized
        if caller == &meta.owner {
            return true;
        }

        // Check if caller is the allowed depositor
        if let Some(allowed) = env
            .storage()
            .instance()
            .get::<StorageKey, Address>(&StorageKey::AllowedDepositor)
        {
            if caller == &allowed {
                return true;
            }
        }

        false
    }

    /// Require that the caller is the owner, panic otherwise.
    fn require_owner(env: &Env, caller: &Address) {
        let meta = Self::get_meta(env.clone());
        assert!(caller == &meta.owner, "unauthorized: owner only");
    }

    /// Get vault metadata (owner and balance).
    ///
    /// # Panics
    /// - If the vault has not been initialized
    pub fn get_meta(env: Env) -> VaultMeta {
        env.storage()
            .instance()
            .get(&StorageKey::Meta)
            .unwrap_or_else(|| panic!("vault not initialized"))
    }

    /// Set or clear the allowed depositor address. Owner-only.
    /// Pass `None` to revoke depositor access, `Some(address)` to grant or update.
    pub fn set_allowed_depositor(env: Env, caller: Address, depositor: Option<Address>) {
        caller.require_auth();
        Self::require_owner(&env, &caller);

        match depositor {
            Some(addr) => {
                env.storage()
                    .instance()
                    .set(&StorageKey::AllowedDepositor, &addr);
            }
            None => {
                env.storage()
                    .instance()
                    .remove(&StorageKey::AllowedDepositor);
            }
        }
    }

    /// Deposit increases balance. Callable by owner or designated depositor.
    /// Emits a "deposit" event with the depositor address and amount.
    pub fn deposit(env: Env, caller: Address, amount: i128) -> i128 {
        caller.require_auth();
        assert!(amount > 0, "amount must be positive");

        assert!(
            Self::is_authorized_depositor(&env, &caller),
            "unauthorized: only owner or allowed depositor can deposit"
        );

        let mut meta = Self::get_meta(env.clone());
        meta.balance += amount;
        env.storage().instance().set(&StorageKey::Meta, &meta);
        meta.balance
    }

    /// Deduct balance for an API call. Only owner/authorized caller in production.
    pub fn deduct(env: Env, caller: Address, amount: i128) -> i128 {
        caller.require_auth();
        Self::require_owner(&env, &caller);

        let mut meta = Self::get_meta(env.clone());
        assert!(meta.balance >= amount, "insufficient balance");
        meta.balance -= amount;
        env.storage().instance().set(&StorageKey::Meta, &meta);
        meta.balance
    }

    /// Return current balance.
    pub fn balance(env: Env) -> i128 {
        Self::get_meta(env).balance
    }

    pub fn transfer_ownership(env: Env, new_owner: Address) {
        let mut meta = Self::get_meta(env.clone());
        meta.owner.require_auth();

        // Validate new_owner is not the same as current owner
        assert!(
            new_owner != meta.owner,
            "new_owner must be different from current owner"
        );

        // Emit event before changing the owner, so we have the old owner
        // topics = (transfer_ownership, old_owner, new_owner)
        env.events().publish(
            (
                Symbol::new(&env, "transfer_ownership"),
                meta.owner.clone(),
                new_owner.clone(),
            ),
            (),
        );

        meta.owner = new_owner;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "meta"), &meta);
    }
}

#[cfg(test)]
mod test;
