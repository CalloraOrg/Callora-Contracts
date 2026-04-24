#![no_std]

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol, Vec};

/// Revenue settlement contract: receives USDC from vault deducts and distributes to developers.
///
/// Flow: vault deduct → vault transfers USDC to this contract → admin calls distribute(to, amount).
const ADMIN_KEY: &str = "admin";
const USDC_KEY: &str = "usdc";

#[contract]
pub struct RevenuePool;

#[contractimpl]
impl RevenuePool {
    /// Initialize the revenue pool with an admin and the USDC token address.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `admin` - Address that may call `distribute`. Typically backend or multisig.
    /// * `usdc_token` - Stellar USDC (or wrapped USDC) token contract address.
    ///
    /// # Panics
    /// * If the revenue pool is already initialized.
    ///
    /// # Events
    /// Emits an `init` event with the `admin` address as a topic and `usdc_token` address as data.
    pub fn init(env: Env, admin: Address, usdc_token: Address) {
        admin.require_auth();
        let inst = env.storage().instance();
        if inst.has(&Symbol::new(&env, ADMIN_KEY)) {
            panic!("revenue pool already initialized");
        }
        inst.set(&Symbol::new(&env, ADMIN_KEY), &admin);
        inst.set(&Symbol::new(&env, USDC_KEY), &usdc_token);

        env.events()
            .publish((Symbol::new(&env, "init"), admin), usdc_token);
    }

    /// Return the current admin address.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    ///
    /// # Returns
    /// The `Address` of the current admin.
    ///
    /// # Panics
    /// * If the revenue pool has not been initialized.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, ADMIN_KEY))
            .expect("revenue pool not initialized")
    }

    /// Replace the current admin. Only the existing admin may call this.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be the current admin.
    /// * `new_admin` - Address of the new admin to be set.
    ///
    /// # Panics
    /// * If the caller is not the current admin (`"unauthorized: caller is not admin"`).
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let current = Self::get_admin(env.clone());
        if caller != current {
            panic!("unauthorized: caller is not admin");
        }
        let inst = env.storage().instance();
        inst.set(&Symbol::new(&env, ADMIN_KEY), &new_admin);
    }

    /// Placeholder: record that payment was received (e.g. from vault).
    /// In practice, USDC is received when the vault (or any address) transfers tokens
    /// to this contract's address; no separate "receive" call is required.
    ///
    /// This function can be used to emit an event for indexers when the backend
    /// wants to log that a payment was credited from the vault.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be admin (or could be extended to allow vault to call).
    /// * `amount` - Amount received (for event logging).
    /// * `from_vault` - Optional; true if the source was the vault.
    ///
    /// # Panics
    /// * If the caller does not have the correct authorization.
    ///
    /// # Events
    /// Emits a `receive_payment` event with `caller` as a topic, and a tuple of `(amount, from_vault)` as data.
    pub fn receive_payment(env: Env, caller: Address, amount: i128, from_vault: bool) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
        }
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
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be the current admin.
    /// * `to` - Developer address to receive USDC.
    /// * `amount` - Amount in token base units (e.g. USDC stroops).
    ///
    /// # Panics
    /// * If the caller is not the current admin (`"unauthorized: caller is not admin"`).
    /// * If the amount is zero or negative (`"amount must be positive"`).
    /// * If the revenue pool has not been initialized.
    /// * If the revenue pool holds less than the requested amount (`"insufficient USDC balance"`).
    ///
    /// # Events
    /// Emits a `distribute` event with `to` as a topic and `amount` as data.
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
            .expect("revenue pool not initialized");
        let usdc = token::Client::new(&env, &usdc_address);

        let contract_address = env.current_contract_address();
        if usdc.balance(&contract_address) < amount {
            panic!("insufficient USDC balance");
        }

        usdc.transfer(&contract_address, &to, &amount);
        env.events()
            .publish((Symbol::new(&env, "distribute"), to), amount);
    }

    /// Distribute USDC from this contract to multiple developer wallets in one atomic transaction.
    ///
    /// This function implements a three-phase atomic batch transfer:
    /// 1. **Precomputation & Validation**: Validates all amounts are positive and calculates total.
    /// 2. **Balance Check**: Ensures contract has sufficient USDC before any transfers.
    /// 3. **Execution**: Performs all transfers and emits events for each leg.
    ///
    /// The implementation guarantees atomicity: either all transfers succeed or none do.
    /// No partial transfers occur if a later leg would fail.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be the current admin.
    /// * `payments` - A vector of `(Address, i128)` tuples representing destinations and amounts.
    ///
    /// # Panics
    /// * If the caller is not the current admin (`"unauthorized: caller is not admin"`).
    /// * If any individual amount is zero or negative (`"amount must be positive"`).
    /// * If the revenue pool has not been initialized (`"revenue pool not initialized"`).
    /// * If the total amount exceeds the contract's available balance (`"insufficient USDC balance"`).
    /// * If the payments vector is empty (`"payments vector cannot be empty"`).
    ///
    /// # Events
    /// Emits a `batch_distribute` event for each payment with `to` as a topic and `amount` as data.
    ///
    /// # Atomicity Guarantee
    /// All validation is performed before any external calls to the USDC token contract.
    /// This ensures that if any validation fails, no state changes or transfers occur.
    ///
    /// # Vector Size Policy
    /// The maximum number of payments in a single batch is limited by Soroban's
    /// transaction budget and footprint limits. Recommended maximum: 100 payments per batch.
    /// For larger distributions, split into multiple transactions.
    ///
    /// # Examples
    /// ```ignore
    /// let payments = vec![
    ///     (developer1, 1000),
    ///     (developer2, 2000),
    ///     (developer3, 1500),
    /// ];
    /// pool.batch_distribute(&admin, &payments);
    /// ```
    pub fn batch_distribute(env: Env, caller: Address, payments: Vec<(Address, i128)>) {
        // Phase 0: Authorization
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
        }

        // Phase 1: Precomputation & Validation
        // Validate that the payments vector is not empty
        if payments.is_empty() {
            panic!("payments vector cannot be empty");
        }

        // Iterate through all payments and validate amounts
        // Calculate total required USDC before any external calls
        let mut total_required: i128 = 0;
        for payment in payments.iter() {
            let (_, amount) = payment;
            
            // Validate each amount is strictly positive
            if amount <= 0 {
                panic!("amount must be positive");
            }
            
            // Accumulate total with overflow check
            total_required = total_required
                .checked_add(amount)
                .expect("total amount overflow");
        }

        // Phase 2: Balance Check
        // Query the USDC token contract for current balance
        let usdc_address: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect("revenue pool not initialized");
        let usdc = token::Client::new(&env, &usdc_address);
        let contract_address = env.current_contract_address();
        let current_balance = usdc.balance(&contract_address);

        // Ensure sufficient balance before any transfers
        if current_balance < total_required {
            panic!("insufficient USDC balance");
        }

        // Phase 3: Execution
        // All validation passed - now perform the transfers
        // Each transfer is atomic; if any fails, the entire transaction reverts
        for payment in payments.iter() {
            let (to, amount) = payment;
            
            // Transfer USDC to recipient
            usdc.transfer(&contract_address, &to, &amount);
            
            // Emit event for this leg of the batch
            env.events()
                .publish((Symbol::new(&env, "batch_distribute"), to), amount);
        }
    }

    /// Return this contract's USDC balance (for testing and dashboards).
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    ///
    /// # Returns
    /// The balance of the contract in USDC base units.
    ///
    /// # Panics
    /// * If the revenue pool has not been initialized.
    pub fn balance(env: Env) -> i128 {
        let usdc_address: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect("revenue pool not initialized");
        let usdc = token::Client::new(&env, &usdc_address);
        usdc.balance(&env.current_contract_address())
    }
}

#[cfg(test)]
mod test;
