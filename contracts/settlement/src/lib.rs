#![no_std]

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Map, Symbol, Vec};

/// Developer balance record in settlement contract
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct DeveloperBalance {
    pub address: Address,
    pub balance: i128,
}

/// Global pool balance tracking
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct GlobalPool {
    pub total_balance: i128,
    pub last_updated: u64,
}

/// Payment received event
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PaymentReceivedEvent {
    pub from_vault: Address,
    pub amount: i128,
    pub to_pool: bool, // true if credited to global pool, false if to specific developer
    pub developer: Option<Address>, // developer address if credited to specific developer
}

/// Balance credited event
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct BalanceCreditedEvent {
    pub developer: Address,
    pub amount: i128,
    pub new_balance: i128,
}

/// Storage key for the registered vault address.
const VAULT_KEY: &str = "vault";
/// Storage key for the admin address.
const ADMIN_KEY: &str = "admin";
const PENDING_ADMIN_KEY: &str = "pending_admin";
const DEVELOPER_BALANCES_KEY: &str = "developer_balances";
/// Storage key for the global pool state.
const GLOBAL_POOL_KEY: &str = "global_pool";

#[contract]
pub struct CalloraSettlement;

#[contractimpl]
impl CalloraSettlement {
    /// Initialize the settlement contract with admin and vault address.
    ///
    /// Persists admin + registered vault, initializes an empty developer balance map,
    /// and stores a timestamped global pool.
    ///
    /// # Panics
    /// Panics if the contract is already initialized.
    pub fn init(env: Env, admin: Address, vault_address: Address) {
        let inst = env.storage().instance();
        if inst.has(&Symbol::new(&env, ADMIN_KEY)) {
            panic!("settlement contract already initialized");
        }
        inst.set(&Symbol::new(&env, ADMIN_KEY), &admin);
        inst.set(&Symbol::new(&env, VAULT_KEY), &vault_address);
        let empty_balances: Map<Address, i128> = Map::new(&env);
        inst.set(&Symbol::new(&env, DEVELOPER_BALANCES_KEY), &empty_balances);
        let global_pool = GlobalPool {
            total_balance: 0,
            last_updated: env.ledger().timestamp(),
        };
        inst.set(&Symbol::new(&env, GLOBAL_POOL_KEY), &global_pool);
    }

    /// Receive payment from vault and credit to pool or developer balance.
    ///
    /// # Arguments
    /// * `caller` - Must be authorized vault address or admin
    /// * `amount` - Payment amount in USDC micro-units; must be > 0
    /// * `to_pool` - If true, credit global pool; if false, credit a specific developer
    /// * `developer` - Required when `to_pool=false`; ignored when `to_pool=true`
    ///
    /// # Access Control
    /// Only the registered vault address or admin can call this function.
    ///
    /// # Events
    /// Always emits `payment_received`. Also emits `balance_credited` when `to_pool=false`.
    pub fn receive_payment(
        env: Env,
        caller: Address,
        amount: i128,
        to_pool: bool,
        developer: Option<Address>,
    ) {
        caller.require_auth();
        Self::require_authorized_caller(env.clone(), caller.clone());
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let inst = env.storage().instance();
        if to_pool {
            let mut global_pool = Self::get_global_pool(env.clone());
            global_pool.total_balance = global_pool
                .total_balance
                .checked_add(amount)
                .unwrap_or_else(|| panic!("pool balance overflow"));
            global_pool.last_updated = env.ledger().timestamp();
            inst.set(&Symbol::new(&env, GLOBAL_POOL_KEY), &global_pool);
            env.events().publish(
                (Symbol::new(&env, "payment_received"), caller.clone()),
                PaymentReceivedEvent {
                    from_vault: caller.clone(),
                    amount,
                    to_pool: true,
                    developer: None,
                },
            );
        } else {
            let dev_address = developer
                .unwrap_or_else(|| panic!("developer address required when to_pool=false"));
            let mut balances: Map<Address, i128> = inst
                .get(&Symbol::new(&env, DEVELOPER_BALANCES_KEY))
                .unwrap_or_else(|| Map::new(&env));
            let current_balance = balances.get(dev_address.clone()).unwrap_or(0);
            let new_balance = current_balance
                .checked_add(amount)
                .unwrap_or_else(|| panic!("developer balance overflow"));
            balances.set(dev_address.clone(), new_balance);
            inst.set(&Symbol::new(&env, DEVELOPER_BALANCES_KEY), &balances);
            env.events().publish(
                (Symbol::new(&env, "payment_received"), caller.clone()),
                PaymentReceivedEvent {
                    from_vault: caller.clone(),
                    amount,
                    to_pool: false,
                    developer: Some(dev_address.clone()),
                },
            );
            env.events().publish(
                (Symbol::new(&env, "balance_credited"), dev_address.clone()),
                BalanceCreditedEvent {
                    developer: dev_address,
                    amount,
                    new_balance,
                },
            );
        }
    }

    /// Get current admin address
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .unwrap_or_else(|| panic!("settlement contract not initialized"))
    }

    /// Get registered vault address
    pub fn get_vault(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, VAULT_KEY))
            .unwrap_or_else(|| panic!("settlement contract not initialized"))
    }

    /// Get global pool information
    pub fn get_global_pool(env: Env) -> GlobalPool {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, GLOBAL_POOL_KEY))
            .unwrap_or_else(|| panic!("settlement contract not initialized"))
    }

    /// Get developer balance
    pub fn get_developer_balance(env: Env, developer: Address) -> i128 {
        if !env.storage().instance().has(&Symbol::new(&env, ADMIN_KEY)) {
            panic!("settlement contract not initialized");
        }
        let inst = env.storage().instance();
        let balances: Map<Address, i128> = inst
            .get(&Symbol::new(&env, DEVELOPER_BALANCES_KEY))
            .unwrap_or_else(|| Map::new(&env));
        balances.get(developer).unwrap_or(0)
    }

    /// Get all developer balances (for admin use)
    pub fn get_all_developer_balances(env: Env) -> Vec<DeveloperBalance> {
        if !env.storage().instance().has(&Symbol::new(&env, ADMIN_KEY)) {
            panic!("settlement contract not initialized");
        }
        let inst = env.storage().instance();
        let balances: Map<Address, i128> = inst
            .get(&Symbol::new(&env, DEVELOPER_BALANCES_KEY))
            .unwrap_or_else(|| Map::new(&env));
        let mut result = Vec::new(&env);
        for (address, balance) in balances.iter() {
            result.push_back(DeveloperBalance { address, balance });
        }
        result
    }

    /// Nominate a new admin (admin only)
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let current_admin = Self::get_admin(env.clone());
        if caller != current_admin {
            panic!("unauthorized: caller is not admin");
        }
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PENDING_ADMIN_KEY), &new_admin);

        env.events().publish(
            (
                Symbol::new(&env, "admin_nominated"),
                current_admin,
                new_admin,
            ),
            (),
        );
    }

    /// Accept the admin role (pending admin only)
    pub fn accept_admin(env: Env) {
        let inst = env.storage().instance();
        let pending: Address = inst
            .get(&Symbol::new(&env, PENDING_ADMIN_KEY))
            .expect("no admin transfer pending");
        pending.require_auth();

        let current = Self::get_admin(env.clone());
        inst.set(&Symbol::new(&env, ADMIN_KEY), &pending);
        inst.remove(&Symbol::new(&env, PENDING_ADMIN_KEY));

        env.events().publish(
            (Symbol::new(&env, "admin_accepted"), current, pending),
            (),
        );
    }

    /// Update vault address (admin only)
    pub fn set_vault(env: Env, caller: Address, new_vault: Address) {
        caller.require_auth();
        let current_admin = Self::get_admin(env.clone());
        if caller != current_admin {
            panic!("unauthorized: caller is not admin");
        }
        env.storage()
            .instance()
            .set(&Symbol::new(&env, VAULT_KEY), &new_vault);
    }

    /// Internal function to require authorized caller (vault or admin)
    fn require_authorized_caller(env: Env, caller: Address) {
        let vault = Self::get_vault(env.clone());
        let admin = Self::get_admin(env.clone());
        if caller != vault && caller != admin {
            panic!("unauthorized: caller must be vault or admin");
        }
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_views;
