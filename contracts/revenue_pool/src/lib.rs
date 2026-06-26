#![no_std]

use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, Map, Symbol, Vec};

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
const MAX_DISTRIBUTE_KEY: &str = "max_distribute";
const ERR_AMOUNT_NOT_POSITIVE: &str = "amount must be positive";
const ERR_AMOUNT_EXCEEDS_MAX_DISTRIBUTE: &str = "amount exceeds max_distribute";
const ERR_UNAUTHORIZED: &str = "unauthorized: caller is not admin";
const ERR_INSUFFICIENT_BALANCE: &str = "insufficient USDC balance";
const ERR_NOT_INITIALIZED: &str = "revenue pool not initialized";
const ERR_DUPLICATE_RECIPIENT: &str = "duplicate recipient in batch";
const PAUSED_KEY: &str = "paused";
const ERR_PAUSED: &str = "revenue pool paused";
const VERSION_KEY: &str = "version";

pub const DEFAULT_MAX_DISTRIBUTE: i128 = i128::MAX;

/// Maximum number of payments allowed in a single `batch_distribute` call.
/// Caps CPU/memory usage well within Soroban resource limits and aligns with
/// the vault's `MAX_BATCH_SIZE` for `batch_deduct`.
pub const MAX_BATCH_SIZE: u32 = 50;

/// TTL bump constants for instance storage archival risk mitigation.
/// Soroban archives ledger entries after ~7 days (631 ledgers) of inactivity.
/// Bumping TTL ensures state remains accessible for critical operations.
///
/// # Constants
/// - `BUMP_AMOUNT`: Number of ledgers to extend TTL by (10000 ledgers ≈ 16 days)
/// - `LIFETIME_THRESHOLD`: Minimum TTL before triggering a bump (1000 ledgers ≈ 1.5 days)
pub const BUMP_AMOUNT: u32 = 10000;
pub const LIFETIME_THRESHOLD: u32 = 1000;

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

        // Extend TTL on initialization to prevent archival
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        env.events()
            .publish((events::event_init(&env), admin), usdc_token);
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
    /// * `caller` - Must be the current admin; must authorize.
    /// * `new_admin` - Address of the proposed new admin.
    ///
    /// # Panics
    /// * If the caller is not the current admin (`"unauthorized: caller is not admin"`).
    ///
    /// # Events
    /// Emits `admin_changed` with `current_admin` as topic and `(current_admin, new_admin)` as data.
    /// Emits `admin_transfer_started` with `current_admin` as topic and `new_admin` as data.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let current = Self::get_admin(env.clone());
        if caller != current {
            panic!("unauthorized: caller is not admin");
        }
        let inst = env.storage().instance();
        inst.set(&Symbol::new(&env, PENDING_ADMIN_KEY), &new_admin);
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        // Emit explicit before/after admin intent for indexers and audit trails.
        env.events().publish(
            (events::event_admin_changed(&env), current.clone()),
            (current.clone(), new_admin.clone()),
        );

        env.events().publish(
            (events::event_admin_transfer_started(&env), current),
            new_admin,
        );
    }

    /// Return the USDC token address configured for this pool.
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
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        env.events()
            .publish((events::event_admin_transfer_completed(&env), pending), ());
    }

    fn require_not_paused(env: &Env) {
        if env
            .storage()
            .instance()
            .get::<_, bool>(&Symbol::new(env, PAUSED_KEY))
            .unwrap_or(false)
        {
            panic!("{}", ERR_PAUSED);
        }
    }

    /// Pause the revenue pool, blocking `distribute` and `batch_distribute`.
    ///
    /// Only the admin may call. Admin rotation remains available while paused.
    ///
    /// # Panics
    /// * If the caller is not the current admin.
    /// * If the pool is already paused.
    ///
    /// # Events
    /// Emits a `pause_set` event with `caller` as a topic and `true` as data.
    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }
        assert!(!Self::is_paused(env.clone()), "revenue pool already paused");
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PAUSED_KEY), &true);
        env.events()
            .publish((events::event_pause_set(&env), caller), true);
    }

    /// Unpause the revenue pool, restoring `distribute` and `batch_distribute`.
    ///
    /// Only the admin may call.
    ///
    /// # Panics
    /// * If the caller is not the current admin.
    /// * If the pool is not currently paused.
    ///
    /// # Events
    /// Emits a `pause_set` event with `caller` as a topic and `false` as data.
    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }
        assert!(Self::is_paused(env.clone()), "revenue pool not paused");
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PAUSED_KEY), &false);
        env.events()
            .publish((events::event_pause_set(&env), caller), false);
    }

    /// Return `true` if the revenue pool is currently paused, `false` otherwise.
    ///
    /// Defaults to `false` when the pause key is absent (i.e. never paused).
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&Symbol::new(&env, PAUSED_KEY))
            .unwrap_or(false)
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
            (events::event_receive_payment(&env), caller),
            (amount, from_vault),
        );
    }

    /// Get the current per-leg distribution cap.
    /// Defaults to `i128::MAX` when unset.
    pub fn get_max_distribute(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, MAX_DISTRIBUTE_KEY))
            .unwrap_or(DEFAULT_MAX_DISTRIBUTE)
    }

    /// Set the maximum amount that may be distributed in a single `distribute`
    /// call or as an individual payment leg in `batch_distribute`.
    ///
    /// Only the current admin may call this. `max_distribute` must be positive.
    /// Emits `set_max_distribute` with `(old_max, new_max)`.
    pub fn set_max_distribute(env: Env, caller: Address, max_distribute: i128) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
        }
        assert!(max_distribute > 0, "max_distribute must be positive");
        let old_max = Self::get_max_distribute(env.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, MAX_DISTRIBUTE_KEY), &max_distribute);
        env.events().publish(
            (events::event_set_max_distribute(&env), admin),
            (old_max, max_distribute),
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
        Self::require_not_paused(&env);
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }
        if amount <= 0 {
            panic!("{}", ERR_AMOUNT_NOT_POSITIVE);
        }
        let max_distribute = Self::get_max_distribute(env.clone());
        if amount > max_distribute {
            panic!("{}", ERR_AMOUNT_EXCEEDS_MAX_DISTRIBUTE);
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

        env.storage()
            .instance()
            .extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        usdc.transfer(&contract_address, &to, &amount);
        env.events()
            .publish((events::event_distribute(&env), to), amount);
    }

    /// Distribute USDC from this contract to multiple developer wallets in one atomic transaction.
    ///
    /// This function implements a four-phase atomic batch transfer:
    /// 1. **Authorization**: Verifies the caller is the current admin.
    /// 2. **Precomputation & Validation**: Validates all amounts are positive, detects duplicate
    ///    recipients, and calculates the total required balance.
    /// 3. **Balance Check**: Ensures the contract holds sufficient USDC before any transfers.
    /// 4. **Execution**: Performs all transfers and emits one event per leg.
    ///
    /// The implementation guarantees atomicity: either all transfers succeed or none do.
    /// No partial transfers occur if any validation step fails.
    ///
    /// # Duplicate Recipient Policy
    ///
    /// **Duplicates are rejected.** If the same `Address` appears more than once in `payments`,
    /// the call panics with `"duplicate recipient in batch"` before any transfer is attempted.
    ///
    /// **Rationale:** A duplicate entry in the payload is almost always an off-chain bug (e.g.,
    /// a developer listed twice in a settlement CSV). Silently double-paying would drain the pool
    /// and be irreversible on-chain. Rejecting the batch forces the caller to fix the payload and
    /// resubmit, which is the safe default for a financial contract.
    ///
    /// If you genuinely need to pay the same address for two distinct milestones in one call,
    /// aggregate the amounts off-chain before submitting.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be the current admin.
    /// * `payments` - A vector of `(Address, i128)` tuples representing destinations and amounts.
    ///   Must contain between 1 and [`MAX_BATCH_SIZE`] entries (inclusive).
    ///   Each `Address` must be unique within the vector.
    ///
    /// # Panics
    /// * If `payments` is empty (`"batch_distribute requires at least one payment"`).
    /// * If `payments` exceeds [`MAX_BATCH_SIZE`] entries (`"batch too large"`).
    /// * If the caller is not the current admin (`"unauthorized: caller is not admin"`).
    /// * If any individual amount is zero or negative (`"amount must be positive"`).
    /// * If any individual amount exceeds `max_distribute` (`"amount exceeds max_distribute"`).
    /// * If the same recipient address appears more than once (`"duplicate recipient in batch"`).
    /// * If the total amount overflows `i128` (`"total overflow"`).
    /// * If the revenue pool has not been initialized (`"revenue pool not initialized"`).
    /// * If the total amount exceeds the contract's available balance (`"insufficient USDC balance"`).
    /// * If any recipient is the contract itself (`"invalid recipient: cannot distribute to the contract itself"`).
    ///
    /// # Events
    /// Emits one `batch_distribute` event per payment leg with `to` as a topic and `amount` as data.
    /// Events are only emitted after all validation passes — never for a partially-executed batch.
    ///
    /// # Atomicity Guarantee
    /// All validation (including duplicate detection) is performed before any external calls to
    /// the USDC token contract. If any check fails, no state changes or transfers occur.
    ///
    /// # Examples
    /// ```ignore
    /// // Valid: three distinct recipients
    /// let payments = vec![
    ///     (developer1, 1000),
    ///     (developer2, 2000),
    ///     (developer3, 1500),
    /// ];
    /// pool.batch_distribute(&admin, &payments);
    ///
    /// // Invalid: developer1 appears twice — will panic with "duplicate recipient in batch"
    /// let bad_payments = vec![
    ///     (developer1, 1000),
    ///     (developer1, 500),
    /// ];
    /// pool.batch_distribute(&admin, &bad_payments); // panics
    /// ```
    pub fn batch_distribute(env: Env, caller: Address, payments: Vec<(Address, i128)>) {
        // Phase 0: Authorization
        caller.require_auth();
        Self::require_not_paused(&env);
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

        // Phase 1: Precomputation, validation, and duplicate detection.
        //
        // We use a Map<Address, bool> as a seen-set. Map is the only ordered,
        // address-keyed collection available in no_std Soroban. Insertion is
        // O(log n) per entry, giving O(n log n) total — well within budget for
        // MAX_BATCH_SIZE = 50 entries.
        //
        // All checks run here, before any external call, to preserve atomicity.
        let max_distribute = Self::get_max_distribute(env.clone());
        let mut seen: Map<Address, bool> = Map::new(&env);
        let mut total_amount: i128 = 0;

        for payment in payments.iter() {
            let (to, amount) = payment;

            // Reject duplicate recipients before any transfer is attempted.
            if seen.contains_key(to.clone()) {
                panic!("{}", ERR_DUPLICATE_RECIPIENT);
            }
            seen.set(to.clone(), true);

            // Validate each amount is strictly positive.
            if amount <= 0 {
                panic!("{}", ERR_AMOUNT_NOT_POSITIVE);
            }
            if amount > max_distribute {
                panic!("{}", ERR_AMOUNT_EXCEEDS_MAX_DISTRIBUTE);
            }

            total_amount = total_amount
                .checked_add(amount)
                .unwrap_or_else(|| panic!("total overflow"));
        }

        // Phase 2: Balance Check — single external read before any writes.
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

        // Extend TTL before executing transfers.
        env.storage().instance().extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        // Phase 3: Execution — all validation passed, perform transfers.
        // Soroban's transaction model guarantees that if any transfer fails,
        // the entire transaction reverts (no partial state).
        for payment in payments.iter() {
            let (to, amount) = payment;
            Self::validate_recipient(&to, &contract_address);
            usdc.transfer(&contract_address, &to, &amount);

            // Emit one event per leg reflecting the final transferred amount.
            env.events()
                .publish((events::event_batch_distribute(&env), to), amount);
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

    /// Admin-gated contract upgrade.
    ///
    /// Only the current admin may call. This will instruct the host to update
    /// the current contract WASM to `new_wasm_hash` and persist the version.
    /// Emits an `upgraded` event with the admin as topic and the new version as data.
    pub fn upgrade(env: Env, caller: Address, new_wasm_hash: BytesN<32>) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }

        // Perform the on-chain upgrade via the deployer interface.
        // This is a host operation and may only succeed in the live environment.
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());

        // Persist the version marker for on-chain queries.
        env.storage()
            .instance()
            .set(&Symbol::new(&env, VERSION_KEY), &new_wasm_hash.clone());

        // Emit an event for indexers / audit logs.
        env.events()
            .publish((events::event_upgraded(&env), admin), new_wasm_hash);
    }

    /// Read the stored contract version (WASM hash) as last set by `upgrade`.
    ///
    /// Panics if no version has been stored yet.
    pub fn version(env: Env) -> BytesN<32> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, VERSION_KEY))
            .expect("version not set")
    }
}

mod events;

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_balance;
