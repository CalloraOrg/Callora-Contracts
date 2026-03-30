//! # Callora Vault Contract
//!
//! ## Access Control
//!
//! The vault implements role-based access control for deposits:
//!
//! - **Owner**: Set at initialization, immutable. Always permitted to deposit.
//! - **Allowed Depositors**: Optional addresses (e.g., backend service) that can be
//!   explicitly approved by the owner. Can be set, changed, or cleared at any time.
//! - **Other addresses**: Rejected with an authorization error.
//!
//! ### Production Usage
//!
//! In production, the owner typically represents the end user's account, while the
//! allowed depositors are backend services that handle automated deposits on behalf
//! of the user.
//!
//! ### Managing the Allowed Depositors
//!
//! - Add or update: `set_allowed_depositor(Some(address))` adds the address if not present
//! - Clear (revoke all access): `set_allowed_depositor(None)`
//! - Only the owner can call `set_allowed_depositor`
//!
//! ### Security Model
//!
//! - The owner has full control over who can deposit
//! - The allowed depositors are trusted addresses (typically backend services)
//! - Access can be revoked at any time by the owner
//! - All deposit attempts are authenticated against the caller's address

#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

#[contracttype]
#[derive(Clone)]
pub struct VaultMeta {
    pub owner: Address,
    pub balance: i128,
}

#[contracttype]
pub enum StorageKey {
    Meta,
    AllowedDepositors,
    ApiPrice(Symbol),
    Paused,
}

#[contract]
pub struct CalloraVault;

#[contractimpl]
impl CalloraVault {
    /// Initialize vault for an owner with optional initial balance.
    /// Emits an "init" event with the owner address and initial balance.
    ///
    /// # Security Note
    /// The `owner` address is required to authorize the initialization transaction via `owner.require_auth()`.
    /// This prevents unauthorized parties from initializing the vault with a "zero" or unauthenticated owner.
    ///
    /// # Panics
    /// - If the vault is already initialized
    /// - If `initial_balance` is negative
    pub fn init(env: Env, owner: Address, initial_balance: Option<i128>) -> VaultMeta {
        owner.require_auth();
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
            .publish((Symbol::new(&env, "init"), owner.clone()), balance);
        meta
    }

    /// Check if the caller is authorized to deposit (owner or allowed depositor).
    fn is_authorized_depositor(env: Env, caller: Address) -> bool {
        let meta = Self::get_meta(env.clone());
        // Owner is always authorized
        if caller == meta.owner {
            return true;
        }

        // Check if caller is in the allowed depositors
        let allowed: Vec<Address> = env
            .storage()
            .instance()
            .get(&StorageKey::AllowedDepositors)
            .unwrap_or(Vec::new(&env));
        allowed.contains(&caller)
    }

    /// Require that the caller is the owner, panic otherwise.
    pub fn require_owner(env: Env, caller: Address) {
        let meta = Self::get_meta(env.clone());
        assert!(caller == meta.owner, "unauthorized: owner only");
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

    /// Add or clear allowed depositors. Owner-only.
    /// Pass `None` to clear all allowed depositors, `Some(address)` to add the address if not already present.
    pub fn set_allowed_depositor(env: Env, caller: Address, depositor: Option<Address>) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone());

        match depositor {
            Some(addr) => {
                let mut allowed: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&StorageKey::AllowedDepositors)
                    .unwrap_or(Vec::new(&env));
                if !allowed.contains(&addr) {
                    allowed.push_back(addr);
                }
                env.storage()
                    .instance()
                    .set(&StorageKey::AllowedDepositors, &allowed);
            }
            None => {
                env.storage()
                    .instance()
                    .remove(&StorageKey::AllowedDepositors);
            }
        }
    }

    /// Deposit increases balance. Callable by owner or designated depositor.
    /// Emits a "deposit" event with the depositor address and amount.
    pub fn deposit(env: Env, caller: Address, amount: i128) -> i128 {
        caller.require_auth();
        assert!(amount > 0, "amount must be positive");

        assert!(
            Self::is_authorized_depositor(env.clone(), caller.clone()),
            "unauthorized: only owner or allowed depositor can deposit"
        );

        let mut meta = Self::get_meta(env.clone());
        meta.balance = meta.balance.checked_add(amount).expect("balance overflow");
        env.storage().instance().set(&StorageKey::Meta, &meta);

        env.events()
            .publish((Symbol::new(&env, "deposit"), caller), amount);
        meta.balance
    }

    /// Pause the vault. Only the owner may call this.
    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller);
        env.storage().instance().set(&StorageKey::Paused, &true);
    }

    /// Unpause the vault. Only the owner may call this.
    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller);
        env.storage().instance().set(&StorageKey::Paused, &false);
    }

    /// Return whether the vault is currently paused.
    pub fn paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&StorageKey::Paused)
            .unwrap_or(false)
    }

    /// Deduct balance for an API call. Only owner/authorized caller in production.
    /// Panics if the vault is paused.
    pub fn deduct(env: Env, caller: Address, amount: i128) -> i128 {
        caller.require_auth();
        Self::require_owner(env.clone(), caller);
        assert!(!Self::paused(env.clone()), "vault is paused");

        let mut meta = Self::get_meta(env.clone());
        assert!(amount > 0, "amount must be positive");
        assert!(meta.balance >= amount, "insufficient balance");
        meta.balance = meta.balance.checked_sub(amount).expect("balance underflow");
        env.storage().instance().set(&StorageKey::Meta, &meta);
        meta.balance
    }

    /// Set the price per API call (in smallest USDC units) for a given API ID.
    /// Callable by the owner or allowed depositor (backend/admin).
    pub fn set_price(env: Env, caller: Address, api_id: Symbol, price: i128) {
        caller.require_auth();

        assert!(
            Self::is_authorized_depositor(env.clone(), caller.clone()),
            "unauthorized: only owner or allowed depositor can set price"
        );

        env.storage()
            .instance()
            .set(&StorageKey::ApiPrice(api_id), &price);
    }

    /// Get the configured price per API call (in smallest USDC units) for a given API ID.
    /// Returns `None` if no price has been set for this API.
    pub fn get_price(env: Env, api_id: Symbol) -> Option<i128> {
        env.storage()
            .instance()
            .get::<StorageKey, i128>(&StorageKey::ApiPrice(api_id))
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
        env.storage().instance().set(&StorageKey::Meta, &meta);
    }
}

#[cfg(test)]
mod test;
