#![no_std]

use soroban_sdk::{contract, contractimpl, token, Address, Env, Symbol, Vec};

/// Revenue settlement contract: receives USDC from vault deducts and distributes to developers.
///
/// Flow: vault deduct → vault transfers USDC to this contract → admin calls distribute(to, amount).
///
/// # Security Assumptions
/// - **Admin Key**: The admin has full control over fund distribution. Must be a secure multisig.
/// - **USDC Asset**: The token address is permanently set on initialization. Must be carefully verified.
/// - **Balances / Griefing**: The contract does not rely on strict balance invariants. External transfers
///   increase balance without breaking logic.
///
/// For detailed threat models and mitigations, see [`SECURITY.md`](../../SECURITY.md).
const ADMIN_KEY: &str = "admin";
const PENDING_ADMIN_KEY: &str = "pending_admin";
const USDC_KEY: &str = "usdc";
const ERR_AMOUNT_NOT_POSITIVE: &str = "amount must be positive";
const ERR_UNAUTHORIZED: &str = "unauthorized: caller is not admin";
const ERR_INSUFFICIENT_BALANCE: &str = "insufficient USDC balance";
const ERR_NOT_INITIALIZED: &str = "revenue pool not initialized";

/// Maximum number of payments allowed in a single `batch_distribute` call.
/// Caps CPU/memory usage well within Soroban resource limits and aligns with
/// the vault's `MAX_BATCH_SIZE` for `batch_deduct`.
pub const MAX_BATCH_SIZE: u32 = 50;

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
        if usdc_token == env.current_contract_address() {
            panic!("invalid config: usdc_token cannot be the contract itself");
        }
        if usdc_token == admin {
            panic!("invalid config: usdc_token cannot be the admin address");
        }
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

    /// Initiate replacement of the current admin. Only the existing admin may call this.
    /// The new admin must call `claim_admin` to complete the transfer.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be the current admin.
    /// * `new_admin` - Address of the proposed new admin.
    ///
    /// # Panics
    /// * If the caller is not the current admin (`"unauthorized: caller is not admin"`).
    ///
    /// # Events
    /// Emits an `admin_changed` event with the `current_admin` as a topic and
    /// `(current_admin, new_admin)` as data, followed by `admin_transfer_started`
    /// with the `current_admin` as a topic and `new_admin` as data.
    /// Return the USDC token address configured for this pool.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    ///
    /// # Returns
    /// The `Address` of the USDC token contract.
    ///
    /// # Panics
    /// * If the revenue pool has not been initialized.
    pub fn get_usdc_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect("revenue pool not initialized")
    }

    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let current = Self::get_admin(env.clone());
        if caller != current {
            panic!("unauthorized: caller is not admin");
        }
        let inst = env.storage().instance();
        inst.set(&Symbol::new(&env, PENDING_ADMIN_KEY), &new_admin);

        // Emit explicit before/after admin intent for indexers and audit trails.
        env.events().publish(
            (Symbol::new(&env, "admin_changed"), current.clone()),
            (current.clone(), new_admin.clone()),
        );

        env.events().publish(
            (Symbol::new(&env, "admin_transfer_started"), current),
            new_admin,
        );
    }

    /// Complete the admin transfer. Only the pending admin may call this.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be the pending admin set via `set_admin`.
    ///
    /// # Panics
    /// * If no pending admin is set (`"no pending admin"`).
    /// * If the caller is not the pending admin (`"unauthorized: caller is not pending admin"`).
    ///
    /// # Events
    /// Emits an `admin_transfer_completed` event with the `new_admin` as a topic.
    pub fn claim_admin(env: Env, caller: Address) {
        caller.require_auth();
        let inst = env.storage().instance();
        let pending: Address = inst
            .get(&Symbol::new(&env, PENDING_ADMIN_KEY))
            .expect("no pending admin");

        if caller != pending {
            panic!("unauthorized: caller is not pending admin");
        }

        inst.set(&Symbol::new(&env, ADMIN_KEY), &pending);
        inst.remove(&Symbol::new(&env, PENDING_ADMIN_KEY));

        env.events()
            .publish((Symbol::new(&env, "admin_transfer_completed"), pending), ());
    }

    /// **Note**: This function is an **event-only helper**. It is **not** a substitute
    /// for real token settlement and does **not** move any tokens. It exists purely
    /// for event emission / indexer alignment when configured.
    /// In practice, USDC is received when the vault (or any address) transfers tokens
    /// to this contract's address; no separate "receive_payment" call is required
    /// for the transfer to succeed.
    ///
    /// This function can be used to emit an event for indexers when the backend
    /// wants to log that a payment was credited from the vault.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be the current admin.
    /// * `amount` - Amount received (for event logging).
    /// * `from_vault` - Optional; true if the source was the vault.
    ///
    /// # Panics
    /// * If the caller is not the current admin (`"unauthorized: caller is not admin"`).
    ///
    /// # Events
    /// Emits a `receive_payment` event with `caller` as a topic, and a tuple of
    /// `(amount, from_vault)` as data.
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

    fn validate_recipient(recipient: &Address, contract_self: &Address) {
        // Rule 1 — no self-distributions (the contract sending to itself is almost
        // certainly a logic bug; if you want to "reclaim" funds use a dedicated fn).
        if recipient == contract_self {
            panic!("invalid recipient: cannot distribute to the contract itself");
        }
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
            panic!("{}", ERR_UNAUTHORIZED);
        }
        if amount <= 0 {
            panic!("{}", ERR_AMOUNT_NOT_POSITIVE);
        }

        let usdc_address: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect(ERR_NOT_INITIALIZED);
        let usdc = token::Client::new(&env, &usdc_address);

        let contract_address = env.current_contract_address();
        Self::validate_recipient(&to, &contract_address);

        let _ = usdc.try_balance(&to).unwrap_or_else(|_| {
            panic!(
                "invalid recipient: account does not exist \
                                      or has no USDC trustline"
            )
        });

        if usdc.balance(&contract_address) < amount {
            panic!("{}", ERR_INSUFFICIENT_BALANCE);
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
    ///   Must contain between 1 and [`MAX_BATCH_SIZE`] entries (inclusive).
    ///
    /// # Panics
    /// * If `payments` is empty (`"batch_distribute requires at least one payment"`).
    /// * If `payments` exceeds [`MAX_BATCH_SIZE`] entries (`"batch too large"`).
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
            panic!("{}", ERR_UNAUTHORIZED);
        }

        let n = payments.len();
        if n == 0 {
            panic!("batch_distribute requires at least one payment");
        }
        if n > MAX_BATCH_SIZE {
            panic!("batch too large");
        }

        let mut total_amount: i128 = 0;
        for payment in payments.iter() {
            let (_, amount) = payment;

            // Validate each amount is strictly positive
            if amount <= 0 {
                panic!("{}", ERR_AMOUNT_NOT_POSITIVE);
            }
            total_amount = total_amount
                .checked_add(amount)
                .unwrap_or_else(|| panic!("total overflow"));
        }

        // Phase 2: Balance Check
        // Query the USDC token contract for current balance
        let usdc_address: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect(ERR_NOT_INITIALIZED);
        let usdc = token::Client::new(&env, &usdc_address);
        let contract_address = env.current_contract_address();

        if usdc.balance(&contract_address) < total_amount {
            panic!("{}", ERR_INSUFFICIENT_BALANCE);
        }

        // Phase 3: Execution
        // All validation passed - now perform the transfers
        // Each transfer is atomic; if any fails, the entire transaction reverts
        for payment in payments.iter() {
            let (to, amount) = payment;
            Self::validate_recipient(&to, &contract_address);
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

#[cfg(test)]
mod test_balance;
