#![no_std]

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol, Vec};

/// Revenue settlement contract: receives USDC from vault deducts and distributes to developers.
///
/// Flow: vault deduct → vault transfers USDC to this contract → admin calls distribute(to, amount).
const ADMIN_KEY: &str = "admin";
const USDC_KEY: &str = "usdc";

#[contract]
pub struct Settlement;

#[contractimpl]
impl Settlement {
    /// Initialize the revenue pool with an admin and the USDC token address.
    ///
    /// # Arguments
    /// * `admin` – Address that may call `distribute`. Typically backend or multisig.
    /// * `usdc_token` – Stellar USDC (or wrapped USDC) token contract address.
    pub fn init(env: Env, admin: Address, usdc_token: Address) {
        admin.require_auth();
        if env.storage().instance().has(&Symbol::new(&env, ADMIN_KEY)) {
            panic!("revenue pool already initialized");
        }
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &admin);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, USDC_KEY), &usdc_token);

        env.events()
            .publish((Symbol::new(&env, "init"), admin), usdc_token);
    }

    /// Return the current admin address.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .unwrap_or_else(|| panic!("revenue pool not initialized"))
    }

    /// Replace the current admin. Only the existing admin may call this.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let current = Self::get_admin(env.clone());
        if caller != current {
            panic!("unauthorized: caller is not admin");
        }
        env.storage()
            .instance()
            .set(&Symbol::new(&env, ADMIN_KEY), &new_admin);
    }

    /// Placeholder: record that payment was received (e.g. from vault).
    /// In practice, USDC is received when the vault (or any address) transfers tokens
    /// to this contract's address; no separate "receive" call is required.
    ///
    /// This function can be used to emit an event for indexers when the backend
    /// wants to log that a payment was credited from the vault.
    ///
    /// # Arguments
    /// * `caller` – Must be admin (or could be extended to allow vault to call).
    /// * `amount` – Amount received (for event logging).
    /// * `from_vault` – Optional; true if the source was the vault.
    pub fn receive_payment(env: Env, caller: Address, amount: i128, from_vault: bool) {
        caller.require_auth();
        let _admin = Self::get_admin(env.clone());
        env.events().publish(
            (Symbol::new(&env, "receive_payment"), caller),
            (amount, from_vault),
        );
    }

    /// Distribute USDC from this contract to a developer wallet.
    ///
    /// Only the admin may call. Transfers USDC from this contract to `to`.
    ///
    /// # Arguments
    /// * `caller` – Must be the current admin.
    /// * `to` – Developer address to receive USDC.
    /// * `amount` – Amount in token base units (e.g. USDC stroops).
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
            .unwrap_or_else(|| panic!("revenue pool not initialized"));
        let usdc = token::Client::new(&env, &usdc_address);

        let contract_address = env.current_contract_address();
        if usdc.balance(&contract_address) < amount {
            panic!("insufficient USDC balance");
        }

        usdc.transfer(&contract_address, &to, &amount);
        env.events()
            .publish((Symbol::new(&env, "distribute"), to), amount);
    }

    /// Distribute USDC from this contract to multiple developer wallets in one transaction.
    ///
    /// Only the admin may call. Iterates through the vector of payments and atomically
    /// transfers USDC to each developer. Fails if the total amount exceeds balance
    /// or if any individual amount is not positive.
    ///
    /// # Arguments
    /// * `caller` - Must be the current admin.
    /// * `payments` - A vector of (Address, amount) tuples.
    pub fn batch_distribute(env: Env, caller: Address, payments: Vec<(Address, i128)>) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
        }

        let mut total_amount: i128 = 0;
        for payment in payments.iter() {
            let (_, amount) = payment;
            if amount <= 0 {
                panic!("amount must be positive");
            }
            total_amount = total_amount.checked_add(amount).expect("total amount overflow");
        }

        let usdc_address: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .unwrap_or_else(|| panic!("revenue pool not initialized"));
        let usdc = token::Client::new(&env, &usdc_address);

        let contract_address = env.current_contract_address();
        if usdc.balance(&contract_address) < total_amount {
            panic!("insufficient USDC balance");
        }

        for payment in payments.iter() {
            let (to, amount) = payment;
            usdc.transfer(&contract_address, &to, &amount);
            env.events()
                .publish((Symbol::new(&env, "batch_distribute"), to), amount);
        }
    }

    /// Return this contract's USDC balance (for testing and dashboards).
    pub fn balance(env: Env) -> i128 {
        let usdc_address: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .unwrap_or_else(|| panic!("revenue pool not initialized"));
        let usdc = token::Client::new(&env, &usdc_address);
        usdc.balance(&env.current_contract_address())
    }
}

#[cfg(test)]
mod test;
