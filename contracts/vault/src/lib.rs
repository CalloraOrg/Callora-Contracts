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

/// Vault configuration stored on-chain.
#[contracttype]
#[derive(Clone, Debug)]
pub struct VaultConfig {
    pub owner: Address,
    pub admin: Address,
    pub usdc_token: Address,
    pub revenue_pool: Option<Address>,
    pub settlement: Option<Address>,
    pub min_deposit: i128,
    pub max_deduct: i128,
    pub authorized_caller: Option<Address>,
}

/// Vault state (mutable) stored on-chain.
#[contracttype]
#[derive(Clone, Debug)]
pub struct VaultState {
    pub balance: i128,
}

#[contracttype]
pub enum StorageKey {
    Config,
    State,
    AllowedDepositors,
    Metadata(String),
}

// Replaced by StorageKey enum variants

/// Default maximum single deduct amount when not set at init (no cap).
pub const DEFAULT_MAX_DEDUCT: i128 = i128::MAX;

#[contract]
pub struct CalloraVault;

#[contractimpl]
impl CalloraVault {
    /// Initialize vault for an owner with optional initial balance.
    /// Emits an "init" event with the owner address and initial balance.
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
        authorized_caller: Option<Address>,
        min_deposit: Option<i128>,
        revenue_pool: Option<Address>,
        max_deduct: Option<i128>,
    ) -> VaultConfig {
        owner.require_auth();
        let inst = env.storage().instance();
        if inst.has(&StorageKey::Config) {
            panic!("vault already initialized");
        }
        let balance = initial_balance.unwrap_or(0);
        assert!(balance >= 0, "initial balance must be non-negative");
        let min_deposit_val = min_deposit.unwrap_or(0);
        let max_deduct_val = max_deduct.unwrap_or(DEFAULT_MAX_DEDUCT);

        let config = VaultConfig {
            owner: owner.clone(),
            admin: owner.clone(),
            usdc_token,
            revenue_pool,
            settlement: None,
            min_deposit: min_deposit_val,
            max_deduct: max_deduct_val,
            authorized_caller,
        };

        inst.set(&StorageKey::Config, &config);
        inst.set(&StorageKey::State, &VaultState { balance });

        env.events()
            .publish((Symbol::new(&env, "init"), owner), balance);
        config
    }

    /// Check if the caller is authorized to deposit (owner or allowed depositor).
    pub fn is_authorized_depositor(env: Env, caller: Address) -> bool {
        let config = Self::get_config(env.clone());
        if caller == config.owner {
            return true;
        }

        let allowed: Vec<Address> = env
            .storage()
            .instance()
            .get(&StorageKey::AllowedDepositors)
            .unwrap_or(Vec::new(&env));
        allowed.contains(&caller)
    }

    /// Return the current admin address.
    ///
    /// # Panics
    /// * `"vault not initialized"` – if called before `init`.
    pub fn get_admin(env: Env) -> Address {
        Self::get_config(env).admin
    }

    /// Transfers the administrative role to a new address.
    /// Can only be called by the current Admin.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let mut config = Self::get_config(env.clone());
        if caller != config.admin {
            panic!("unauthorized: caller is not admin");
        }
        config.admin = new_admin;
        env.storage().instance().set(&StorageKey::Config, &config);
    }

    /// Require that the caller is the owner, panic otherwise.
    pub fn require_owner(env: Env, caller: Address) {
        let config = Self::get_config(env.clone());
        assert!(caller == config.owner, "unauthorized: owner only");
    }

    /// Distribute accumulated USDC to a single developer address.
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
        let config = Self::get_config(env.clone());
        if caller != config.admin {
            panic!("unauthorized: caller is not admin");
        }
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let usdc = token::Client::new(&env, &config.usdc_token);
        let vault_balance = usdc.balance(&env.current_contract_address());
        if vault_balance < amount {
            panic!("insufficient USDC balance");
        }
        usdc.transfer(&env.current_contract_address(), &to, &amount);
        env.events()
            .publish((Symbol::new(&env, "distribute"), to), amount);
    }

    /// Get vault configuration.
    ///
    /// # Panics
    /// * `"vault not initialized"` – if called before `init`.
    pub fn get_config(env: Env) -> VaultConfig {
        let inst = env.storage().instance();
        if let Some(config) = inst.get(&StorageKey::Config) {
            return config;
        }

        // LEGACY MIGRATION: Try to read from multiple old keys
        // (This would ideally use a predefined VaultMeta struct, but here we construct it from pieces if needed)
        #[contracttype]
        #[derive(Clone)]
        pub struct LegacyVaultMeta {
            pub owner: Address,
            pub balance: i128,
            pub authorized_caller: Option<Address>,
            pub min_deposit: i128,
        }

        // Search for legacy "Meta" key (enum variant or Symbol)
        let old_meta: Option<LegacyVaultMeta> =
            inst.get(&Symbol::new(&env, "Meta")).or_else(|| {
                // Some versions might have used Symbol("meta")
                inst.get(&Symbol::new(&env, "meta"))
            });

        if let Some(meta) = old_meta {
            let config = VaultConfig {
                owner: meta.owner.clone(),
                admin: inst
                    .get(&Symbol::new(&env, "Admin"))
                    .unwrap_or(meta.owner.clone()),
                usdc_token: inst
                    .get(&Symbol::new(&env, "UsdcToken"))
                    .expect("missing USDC"),
                revenue_pool: inst.get(&Symbol::new(&env, "RevenuePool")),
                settlement: inst.get(&Symbol::new(&env, "Settlement")),
                min_deposit: meta.min_deposit,
                max_deduct: inst
                    .get(&Symbol::new(&env, "MaxDeduct"))
                    .unwrap_or(DEFAULT_MAX_DEDUCT),
                authorized_caller: meta.authorized_caller,
            };
            // Do NOT remove old keys here to be safe, but return new format
            return config;
        }

        panic!("vault not initialized")
    }

    pub fn get_state(env: Env) -> VaultState {
        let inst = env.storage().instance();
        if let Some(state) = inst.get(&StorageKey::State) {
            return state;
        }

        // LEGACY MIGRATION: Try to read balance from old Meta key
        #[contracttype]
        #[derive(Clone)]
        pub struct LegacyVaultMeta {
            pub owner: Address,
            pub balance: i128,
            pub authorized_caller: Option<Address>,
            pub min_deposit: i128,
        }
        if let Some(meta) = inst.get::<Symbol, LegacyVaultMeta>(&Symbol::new(&env, "Meta")) {
            return VaultState {
                balance: meta.balance,
            };
        }
        if let Some(meta) = inst.get::<Symbol, LegacyVaultMeta>(&Symbol::new(&env, "meta")) {
            return VaultState {
                balance: meta.balance,
            };
        }

        panic!("vault not initialized")
    }

    /// Sets whether an address is allowed to deposit into the vault.
    /// Can only be called by the Owner.
    pub fn set_allowed_depositor(env: Env, caller: Address, depositor: Option<Address>) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller);
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

    /// Sets the authorized caller permitted to trigger deductions.
    /// Can only be called by the Owner.
    pub fn set_authorized_caller(env: Env, caller: Address) {
        let mut config = Self::get_config(env.clone());
        config.owner.require_auth();

        config.authorized_caller = Some(caller.clone());
        env.storage().instance().set(&StorageKey::Config, &config);

        env.events().publish(
            (Symbol::new(&env, "set_auth_caller"), config.owner.clone()),
            caller,
        );
    }

    /// Deposits USDC into the vault.
    /// Can be called by the Owner or any Allowed Depositor.
    pub fn deposit(env: Env, caller: Address, amount: i128) -> i128 {
        caller.require_auth();
        assert!(amount > 0, "amount must be positive");
        assert!(
            Self::is_authorized_depositor(env.clone(), caller.clone()),
            "unauthorized: only owner or allowed depositor can deposit"
        );

        let config = Self::get_config(env.clone());
        assert!(
            amount >= config.min_deposit,
            "deposit below minimum: {} < {}",
            amount,
            config.min_deposit
        );
        let usdc = token::Client::new(&env, &config.usdc_token);
        usdc.transfer(&caller, &env.current_contract_address(), &amount);

        let mut state = Self::get_state(env.clone());
        state.balance += amount;
        env.storage().instance().set(&StorageKey::State, &state);

        env.events()
            .publish((Symbol::new(&env, "deposit"), caller), amount);
        state.balance
    }

    pub fn get_max_deduct(env: Env) -> i128 {
        Self::get_config(env).max_deduct
    }

    pub fn deduct(env: Env, caller: Address, amount: i128, request_id: Option<Symbol>) -> i128 {
        caller.require_auth();
        assert!(amount > 0, "amount must be positive");
        let config = Self::get_config(env.clone());
        assert!(
            amount <= config.max_deduct,
            "deduct amount exceeds max_deduct"
        );
        let mut state = Self::get_state(env.clone());

        // Check authorization: must be either the authorized_caller if set, or the owner.
        let authorized = match &config.authorized_caller {
            Some(auth_caller) => caller == *auth_caller || caller == config.owner,
            None => caller == config.owner,
        };
        assert!(authorized, "unauthorized caller");

        assert!(state.balance >= amount, "insufficient balance");
        state.balance -= amount;
        env.storage().instance().set(&StorageKey::State, &state);

        // Transfer USDC to settlement contract or revenue pool if configured
        if let Some(settlement) = &config.settlement {
            Self::transfer_funds(&env, &config.usdc_token, settlement, amount);
        } else if let Some(revenue_pool) = &config.revenue_pool {
            Self::transfer_funds(&env, &config.usdc_token, revenue_pool, amount);
        }

        let topics = match &request_id {
            Some(rid) => (Symbol::new(&env, "deduct"), caller.clone(), rid.clone()),
            None => (Symbol::new(&env, "deduct"), caller, Symbol::new(&env, "")),
        };
        env.events().publish(topics, (amount, state.balance));
        state.balance
    }

    pub fn batch_deduct(env: Env, caller: Address, items: Vec<DeductItem>) -> i128 {
        caller.require_auth();
        let config = Self::get_config(env.clone());
        let mut state = Self::get_state(env.clone());

        let authorized = match &config.authorized_caller {
            Some(auth_caller) => caller == *auth_caller || caller == config.owner,
            None => caller == config.owner,
        };
        assert!(authorized, "unauthorized caller");

        let n = items.len();
        assert!(n > 0, "batch_deduct requires at least one item");

        let mut running = state.balance;
        let mut total_amount = 0i128;
        for item in items.iter() {
            assert!(item.amount > 0, "amount must be positive");
            assert!(
                item.amount <= config.max_deduct,
                "deduct amount exceeds max_deduct"
            );
            assert!(running >= item.amount, "insufficient balance");
            running -= item.amount;
            total_amount += item.amount;
        }
        // Apply deductions and emit per-item events.
        let mut balance = state.balance;
        for item in items.iter() {
            balance -= item.amount;
            let rid = item.request_id.clone().unwrap_or(Symbol::new(&env, ""));
            env.events().publish(
                (Symbol::new(&env, "deduct"), caller.clone(), rid),
                (item.amount, balance),
            );
        }
        state.balance = balance;
        env.storage().instance().set(&StorageKey::State, &state);

        if let Some(settlement) = &config.settlement {
            Self::transfer_funds(&env, &config.usdc_token, settlement, total_amount);
        } else if let Some(revenue_pool) = &config.revenue_pool {
            Self::transfer_funds(&env, &config.usdc_token, revenue_pool, total_amount);
        }

        state.balance
    }

    /// Return current balance.
    pub fn balance(env: Env) -> i128 {
        Self::get_state(env).balance
    }

    pub fn transfer_ownership(env: Env, new_owner: Address) {
        let mut config = Self::get_config(env.clone());
        config.owner.require_auth();
        assert!(
            new_owner != config.owner,
            "new_owner must be different from current owner"
        );

        env.events().publish(
            (
                Symbol::new(&env, "transfer_ownership"),
                config.owner.clone(),
                new_owner.clone(),
            ),
            (),
        );

        config.owner = new_owner;
        env.storage().instance().set(&StorageKey::Config, &config);
    }

    /// Withdraws USDC from the vault to the owner.
    /// Can only be called by the Owner.
    pub fn withdraw(env: Env, amount: i128) -> i128 {
        let config = Self::get_config(env.clone());
        config.owner.require_auth();
        assert!(amount > 0, "amount must be positive");
        let mut state = Self::get_state(env.clone());
        assert!(state.balance >= amount, "insufficient balance");
        let usdc = token::Client::new(&env, &config.usdc_token);
        usdc.transfer(&env.current_contract_address(), &config.owner, &amount);
        state.balance -= amount;
        env.storage().instance().set(&StorageKey::State, &state);
        state.balance
    }

    /// Withdraws USDC from the vault to a specific recipient.
    /// Can only be called by the Owner.
    pub fn withdraw_to(env: Env, to: Address, amount: i128) -> i128 {
        let config = Self::get_config(env.clone());
        config.owner.require_auth();
        assert!(amount > 0, "amount must be positive");
        let mut state = Self::get_state(env.clone());
        assert!(state.balance >= amount, "insufficient balance");
        let usdc = token::Client::new(&env, &config.usdc_token);
        usdc.transfer(&env.current_contract_address(), &to, &amount);
        state.balance -= amount;
        env.storage().instance().set(&StorageKey::State, &state);
        state.balance
    }

    /// Sets the settlement contract address.
    /// Can only be called by the Admin.
    pub fn set_settlement(env: Env, caller: Address, settlement_address: Address) {
        caller.require_auth();
        let mut config = Self::get_config(env.clone());
        if caller != config.admin {
            panic!("unauthorized: caller is not admin");
        }
        config.settlement = Some(settlement_address);
        env.storage().instance().set(&StorageKey::Config, &config);
    }

    /// Get the settlement contract address.
    ///
    /// # Panics
    /// * `"settlement address not set"` – if no settlement address has been configured.
    pub fn get_settlement(env: Env) -> Address {
        Self::get_config(env)
            .settlement
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
        env.storage()
            .instance()
            .set(&StorageKey::Metadata(offering_id.clone()), &metadata);
        env.events().publish(
            (Symbol::new(&env, "metadata_set"), offering_id, caller),
            metadata.clone(),
        );
        metadata
    }

    /// Retrieve stored offering metadata. Returns `None` if not set.
    pub fn get_metadata(env: Env, offering_id: String) -> Option<String> {
        env.storage()
            .instance()
            .get(&StorageKey::Metadata(offering_id))
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
            .get(&StorageKey::Metadata(offering_id.clone()))
            .unwrap_or(String::from_str(&env, ""));
        env.storage()
            .instance()
            .set(&StorageKey::Metadata(offering_id.clone()), &metadata);
        env.events().publish(
            (Symbol::new(&env, "metadata_updated"), offering_id, caller),
            (old, metadata.clone()),
        );
        metadata
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Helper to transfer amount of USDC to a destination.
    fn transfer_funds(env: &Env, usdc_token: &Address, to: &Address, amount: i128) {
        let usdc = token::Client::new(env, usdc_token);
        usdc.transfer(&env.current_contract_address(), to, &amount);
    }
}

#[cfg(test)]
mod test;
