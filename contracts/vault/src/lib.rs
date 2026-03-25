//! # Callora Vault Contract
//!
//! ## Access Control
//!
//! The vault implements role-based access control for deposits:
//!
//! - **Owner**: Set at initialization, immutable via `transfer_ownership`. Always permitted to deposit.
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
//! - Add: `set_allowed_depositor(Some(address))` – adds the address if not already present.
//! - Clear: `set_allowed_depositor(None)` – revokes all depositor access.
//! - Only the owner may call `set_allowed_depositor`.
//!
//! ### Security Model
//!
//! - The owner has full control over who can deposit.
//! - Allowed depositors are trusted addresses (typically backend services).
//! - Access can be revoked at any time by the owner.
//! - All deposit attempts are authenticated against the caller's address.

#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, String, Symbol, Vec};

/// Single item for batch deduct: amount and optional request id for idempotency/tracking.
#[contracttype]
#[derive(Clone)]
pub struct DeductItem {
    pub amount: i128,
    pub request_id: Option<Symbol>,
}

/// Vault metadata stored on-chain.
#[contracttype]
#[derive(Clone)]
pub struct VaultMeta {
    pub owner: Address,
    pub balance: i128,
    /// Minimum amount required per deposit; deposits below this value are rejected.
    pub min_deposit: i128,
}

const META_KEY: &str = "meta";
const USDC_KEY: &str = "usdc";
const ADMIN_KEY: &str = "admin";
const SETTLEMENT_KEY: &str = "settlement";
const REVENUE_POOL_KEY: &str = "revenue_pool";
const MAX_DEDUCT_KEY: &str = "max_deduct";
/// Storage key for the allowed-depositors list.
const ALLOWED_KEY: &str = "depositors";

/// Default maximum single deduct amount when not set at init (no cap).
pub const DEFAULT_MAX_DEDUCT: i128 = i128::MAX;

#[contract]
pub struct CalloraVault;

#[contractimpl]
impl CalloraVault {
    /// Initialize the vault.
    ///
    /// # Arguments
    /// * `owner`           – Vault owner; must authorize this call. Always permitted to deposit.
    /// * `usdc_token`      – Address of the USDC token contract.
    /// * `initial_balance` – Optional initial tracked balance (USDC must already be in the contract).
    /// * `min_deposit`     – Optional minimum per-deposit amount (default `0`).
    /// * `revenue_pool`    – Optional address to receive USDC on each deduct. If `None`, USDC stays in vault.
    /// * `max_deduct`      – Optional cap per single deduct; if `None`, uses `DEFAULT_MAX_DEDUCT` (no cap).
    ///
    /// # Panics
    /// * `"vault already initialized"` – if called more than once.
    /// * `"initial balance must be non-negative"` – if `initial_balance` is negative.
    ///
    /// # Events
    /// Emits topic `("init", owner)` with data `balance` on success.
    pub fn init(
        env: Env,
        owner: Address,
        usdc_token: Address,
        initial_balance: Option<i128>,
        min_deposit: Option<i128>,
        revenue_pool: Option<Address>,
        max_deduct: Option<i128>,
    ) -> VaultMeta {
        owner.require_auth();
        let inst = env.storage().instance();
        if inst.has(&Symbol::new(&env, META_KEY)) {
            panic!("vault already initialized");
        }
        let balance = initial_balance.unwrap_or(0);
        assert!(balance >= 0, "initial balance must be non-negative");
        let min_deposit_val = min_deposit.unwrap_or(0);
        let max_deduct_val = max_deduct.unwrap_or(DEFAULT_MAX_DEDUCT);

        let meta = VaultMeta {
            owner: owner.clone(),
            balance,
            min_deposit: min_deposit_val,
        };

        inst.set(&Symbol::new(&env, META_KEY), &meta);
        inst.set(&Symbol::new(&env, USDC_KEY), &usdc_token);
        inst.set(&Symbol::new(&env, ADMIN_KEY), &owner);
        if let Some(pool) = revenue_pool {
            inst.set(&Symbol::new(&env, REVENUE_POOL_KEY), &pool);
        }
        inst.set(&Symbol::new(&env, MAX_DEDUCT_KEY), &max_deduct_val);

        env.events()
            .publish((Symbol::new(&env, "init"), owner.clone()), balance);
        meta
    }

    /// Return the current admin address.
    ///
    /// # Panics
    /// * `"vault not initialized"` – if called before `init`.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .expect("vault not initialized")
    }

    /// Replace the current admin. Only the existing admin may call this.
    ///
    /// # Panics
    /// * `"unauthorized: caller is not admin"` – if caller is not the current admin.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let current_admin = Self::get_admin(env.clone());
        if caller != current_admin {
            panic!("unauthorized: caller is not admin");
        }
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &new_admin);
    }

    /// Return `true` if `caller` is authorized to deposit (owner or allowed depositor).
    pub fn is_authorized_depositor(env: Env, caller: Address) -> bool {
        let meta = Self::get_meta(env.clone());
        if caller == meta.owner {
            return true;
        }
        let allowed: Vec<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, ALLOWED_KEY))
            .unwrap_or(Vec::new(&env));
        allowed.contains(&caller)
    }

    /// Distribute accumulated USDC to a recipient. Admin-only.
    ///
    /// # Panics
    /// * `"unauthorized: caller is not admin"` – caller is not the admin.
    /// * `"amount must be positive"`           – amount is zero or negative.
    /// * `"insufficient USDC balance"`         – vault holds less than amount.
    ///
    /// # Events
    /// Emits topic `("distribute", to)` with data `amount` on success.
    pub fn distribute(env: Env, caller: Address, to: Address, amount: i128) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
        }
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let usdc_address: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect("vault not initialized");
        let usdc = token::Client::new(&env, &usdc_address);
        let vault_balance = usdc.balance(&env.current_contract_address());
        if vault_balance < amount {
            panic!("insufficient USDC balance");
        }
        usdc.transfer(&env.current_contract_address(), &to, &amount);
        env.events()
            .publish((Symbol::new(&env, "distribute"), to), amount);
    }

    /// Get vault metadata (owner, balance, and min_deposit).
    ///
    /// # Panics
    /// * `"vault not initialized"` – if called before `init`.
    pub fn get_meta(env: Env) -> VaultMeta {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, META_KEY))
            .expect("vault not initialized")
    }

    /// Get the maximum allowed per-deduct amount.
    pub fn get_max_deduct(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, MAX_DEDUCT_KEY))
            .unwrap_or(DEFAULT_MAX_DEDUCT)
    }

    /// Add or clear allowed depositors. Owner-only.
    ///
    /// Pass `None` to revoke all depositor access; `Some(address)` to add an address
    /// if not already present.
    ///
    /// # Panics
    /// * `"unauthorized: owner only"` – if caller is not the vault owner.
    pub fn set_allowed_depositor(env: Env, caller: Address, depositor: Option<Address>) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone());
        match depositor {
            Some(addr) => {
                let mut allowed: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&Symbol::new(&env, ALLOWED_KEY))
                    .unwrap_or(Vec::new(&env));
                if !allowed.contains(&addr) {
                    allowed.push_back(addr);
                }
                env.storage()
                    .instance()
                    .set(&Symbol::new(&env, ALLOWED_KEY), &allowed);
            }
            None => {
                env.storage()
                    .instance()
                    .remove(&Symbol::new(&env, ALLOWED_KEY));
            }
        }
    }

    /// Deposit USDC into the vault. Callable by the owner or an allowed depositor.
    ///
    /// Caller must have pre-approved `amount` USDC to this contract via `token.approve`.
    ///
    /// # Panics
    /// * `"amount must be positive"` – amount is zero or negative.
    /// * `"unauthorized: only owner or allowed depositor can deposit"` – caller not authorized.
    /// * `"deposit below minimum"` – amount is below `min_deposit`.
    ///
    /// # Events
    /// Emits topic `("deposit", caller)` with data `amount` on success.
    pub fn deposit(env: Env, caller: Address, amount: i128) -> i128 {
        caller.require_auth();
        assert!(amount > 0, "amount must be positive");
        assert!(
            Self::is_authorized_depositor(env.clone(), caller.clone()),
            "unauthorized: only owner or allowed depositor can deposit"
        );
        let mut meta = Self::get_meta(env.clone());
        assert!(amount >= meta.min_deposit, "deposit below minimum");
        let usdc_address: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect("vault not initialized");
        let usdc = token::Client::new(&env, &usdc_address);
        usdc.transfer_from(
            &env.current_contract_address(),
            &caller,
            &env.current_contract_address(),
            &amount,
        );
        meta.balance += amount;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, META_KEY), &meta);
        env.events()
            .publish((Symbol::new(&env, "deposit"), caller), amount);
        meta.balance
    }

    /// Deduct from the vault balance. Any authenticated caller may call this.
    ///
    /// Amount must not exceed `max_deduct`. If a revenue pool is configured, the
    /// equivalent USDC is transferred to it on each deduct.
    ///
    /// # Panics
    /// * `"amount must be positive"` – amount is zero or negative.
    /// * `"deduct amount exceeds max_deduct"` – amount exceeds the per-deduct cap.
    /// * `"insufficient balance"` – vault balance is less than amount.
    ///
    /// # Events
    /// Emits topic `("deduct", caller, request_id)` with data `(amount, new_balance)`.
    /// `request_id` defaults to `Symbol("")` when not provided.
    pub fn deduct(env: Env, caller: Address, amount: i128, request_id: Option<Symbol>) -> i128 {
        caller.require_auth();
        assert!(amount > 0, "amount must be positive");
        let max_deduct = Self::get_max_deduct(env.clone());
        assert!(amount <= max_deduct, "deduct amount exceeds max_deduct");
        let mut meta = Self::get_meta(env.clone());
        assert!(meta.balance >= amount, "insufficient balance");
        meta.balance -= amount;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, META_KEY), &meta);
        // Transfer USDC to revenue pool if configured.
        let pool: Option<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, REVENUE_POOL_KEY));
        if let Some(pool_addr) = pool {
            let usdc_address: Address = env
                .storage()
                .instance()
                .get(&Symbol::new(&env, USDC_KEY))
                .expect("vault not initialized");
            let usdc = token::Client::new(&env, &usdc_address);
            usdc.transfer(&env.current_contract_address(), &pool_addr, &amount);
        }
        let rid = request_id.unwrap_or(Symbol::new(&env, ""));
        env.events().publish(
            (Symbol::new(&env, "deduct"), caller, rid),
            (amount, meta.balance),
        );
        meta.balance
    }

    /// Batch deduct: process multiple [`DeductItem`]s atomically.
    ///
    /// All checks are validated before any state is mutated. If any item fails
    /// validation the entire batch is rejected. If a revenue pool is configured,
    /// the total USDC is transferred in a single call after all deductions.
    ///
    /// # Panics
    /// * `"batch_deduct requires at least one item"` – items list is empty.
    /// * `"amount must be positive"` – any item amount is zero or negative.
    /// * `"deduct amount exceeds max_deduct"` – any item exceeds the per-deduct cap.
    /// * `"insufficient balance"` – cumulative deductions would exceed vault balance.
    pub fn batch_deduct(env: Env, caller: Address, items: Vec<DeductItem>) -> i128 {
        caller.require_auth();
        let max_deduct = Self::get_max_deduct(env.clone());
        let mut meta = Self::get_meta(env.clone());
        assert!(!items.is_empty(), "batch_deduct requires at least one item");
        // Validate all items atomically before mutating state.
        let mut running = meta.balance;
        let mut total_amount = 0i128;
        for item in items.iter() {
            assert!(item.amount > 0, "amount must be positive");
            assert!(
                item.amount <= max_deduct,
                "deduct amount exceeds max_deduct"
            );
            assert!(running >= item.amount, "insufficient balance");
            running -= item.amount;
            total_amount += item.amount;
        }
        // Apply deductions and emit per-item events.
        let mut balance = meta.balance;
        for item in items.iter() {
            balance -= item.amount;
            let rid = item.request_id.clone().unwrap_or(Symbol::new(&env, ""));
            env.events().publish(
                (Symbol::new(&env, "deduct"), caller.clone(), rid),
                (item.amount, balance),
            );
        }
        meta.balance = balance;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, META_KEY), &meta);
        // Transfer total USDC to revenue pool if configured.
        let pool: Option<Address> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, REVENUE_POOL_KEY));
        if let Some(pool_addr) = pool {
            let usdc_address: Address = env
                .storage()
                .instance()
                .get(&Symbol::new(&env, USDC_KEY))
                .expect("vault not initialized");
            let usdc = token::Client::new(&env, &usdc_address);
            usdc.transfer(&env.current_contract_address(), &pool_addr, &total_amount);
        }
        meta.balance
    }

    /// Return the current vault balance.
    pub fn balance(env: Env) -> i128 {
        Self::get_meta(env).balance
    }

    /// Withdraw USDC from the vault to the owner's address. Owner-only.
    ///
    /// # Panics
    /// * `"amount must be positive"` – amount is zero or negative.
    /// * `"insufficient balance"` – vault balance is less than amount.
    pub fn withdraw(env: Env, amount: i128) -> i128 {
        let mut meta = Self::get_meta(env.clone());
        meta.owner.require_auth();
        assert!(amount > 0, "amount must be positive");
        assert!(meta.balance >= amount, "insufficient balance");
        let usdc_address: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect("vault not initialized");
        let usdc = token::Client::new(&env, &usdc_address);
        usdc.transfer(&env.current_contract_address(), &meta.owner, &amount);
        meta.balance -= amount;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, META_KEY), &meta);
        meta.balance
    }

    /// Withdraw USDC from the vault to a designated address. Owner-only.
    ///
    /// # Panics
    /// * `"amount must be positive"` – amount is zero or negative.
    /// * `"insufficient balance"` – vault balance is less than amount.
    pub fn withdraw_to(env: Env, to: Address, amount: i128) -> i128 {
        let mut meta = Self::get_meta(env.clone());
        meta.owner.require_auth();
        assert!(amount > 0, "amount must be positive");
        assert!(meta.balance >= amount, "insufficient balance");
        let usdc_address: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect("vault not initialized");
        let usdc = token::Client::new(&env, &usdc_address);
        usdc.transfer(&env.current_contract_address(), &to, &amount);
        meta.balance -= amount;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, META_KEY), &meta);
        meta.balance
    }

    /// Transfer vault ownership to `new_owner`. Owner-only.
    ///
    /// # Panics
    /// * `"new_owner must be different from current owner"` – same address passed.
    ///
    /// # Events
    /// Emits topic `("transfer_ownership", old_owner, new_owner)` with data `()`.
    pub fn transfer_ownership(env: Env, new_owner: Address) {
        let mut meta = Self::get_meta(env.clone());
        meta.owner.require_auth();
        assert!(
            new_owner != meta.owner,
            "new_owner must be different from current owner"
        );
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
            .set(&Symbol::new(&env, META_KEY), &meta);
    }

    /// Set the settlement contract address. Admin-only.
    ///
    /// # Panics
    /// * `"unauthorized: caller is not admin"` – caller is not the current admin.
    pub fn set_settlement(env: Env, caller: Address, settlement_address: Address) {
        caller.require_auth();
        let current_admin = Self::get_admin(env.clone());
        if caller != current_admin {
            panic!("unauthorized: caller is not admin");
        }
        env.storage()
            .instance()
            .set(&Symbol::new(&env, SETTLEMENT_KEY), &settlement_address);
    }

    /// Get the settlement contract address.
    ///
    /// # Panics
    /// * `"settlement address not set"` – if no settlement address has been configured.
    pub fn get_settlement(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, SETTLEMENT_KEY))
            .unwrap_or_else(|| panic!("settlement address not set"))
    }

    /// Store offering metadata. Owner-only.
    ///
    /// # Panics
    /// * `"unauthorized: owner only"` – caller is not the vault owner.
    ///
    /// # Events
    /// Emits topic `("metadata_set", offering_id, caller)` with data `metadata`.
    pub fn set_metadata(
        env: Env,
        caller: Address,
        offering_id: String,
        metadata: String,
    ) -> String {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone());
        env.storage().instance().set(&offering_id, &metadata);
        env.events().publish(
            (Symbol::new(&env, "metadata_set"), offering_id, caller),
            metadata.clone(),
        );
        metadata
    }

    /// Retrieve stored offering metadata. Returns `None` if not set.
    pub fn get_metadata(env: Env, offering_id: String) -> Option<String> {
        env.storage().instance().get(&offering_id)
    }

    /// Update existing offering metadata. Owner-only.
    ///
    /// # Panics
    /// * `"unauthorized: owner only"` – caller is not the vault owner.
    ///
    /// # Events
    /// Emits topic `("metadata_updated", offering_id, caller)` with data `(old_metadata, new_metadata)`.
    pub fn update_metadata(
        env: Env,
        caller: Address,
        offering_id: String,
        metadata: String,
    ) -> String {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone());
        let old: String = env
            .storage()
            .instance()
            .get(&offering_id)
            .unwrap_or(String::from_str(&env, ""));
        env.storage().instance().set(&offering_id, &metadata);
        env.events().publish(
            (Symbol::new(&env, "metadata_updated"), offering_id, caller),
            (old, metadata.clone()),
        );
        metadata
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Panics with `"unauthorized: owner only"` if `caller` is not the vault owner.
    fn require_owner(env: Env, caller: Address) {
        let meta = Self::get_meta(env);
        assert!(caller == meta.owner, "unauthorized: owner only");
    }
}

#[cfg(test)]
mod test;
