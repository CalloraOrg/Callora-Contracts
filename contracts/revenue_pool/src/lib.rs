#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, BytesN, Env, Map, String, Symbol, Vec,
};

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
const CUMULATIVE_YIELD_DEPOSITED_KEY: &str = "cumulative_yield_deposited";
const ERR_AMOUNT_NOT_POSITIVE: &str = "amount must be positive";
const ERR_AMOUNT_EXCEEDS_MAX_DISTRIBUTE: &str = "amount exceeds max_distribute";
const ERR_UNAUTHORIZED: &str = "unauthorized: caller is not admin";
const ERR_INSUFFICIENT_BALANCE: &str = "insufficient USDC balance";
const ERR_NOT_INITIALIZED: &str = "revenue pool not initialized";
const ERR_DUPLICATE_RECIPIENT: &str = "duplicate recipient in batch";
const PAUSED_KEY: &str = "paused";
const ERR_PAUSED: &str = "revenue pool paused";
const VERSION_KEY: &str = "version";

/// Typed contract errors for the revenue pool.
///
/// Returned (instead of string panics) for batch-size violations so backend
/// integrators can branch on a stable numeric code rather than parsing panic
/// strings. See [`chunk_iter`] for pre-chunking large payout lists to avoid
/// [`RevenuePoolError::BatchTooLarge`] entirely.
#[contracterror]
#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum RevenuePoolError {
    /// `batch_distribute` was called with an empty `payments` vector (code 1).
    BatchEmpty = 1,
    /// `batch_distribute` received more than [`MAX_BATCH_SIZE`] payment legs (code 2).
    BatchTooLarge = 2,
}

pub const DEFAULT_MAX_DISTRIBUTE: i128 = i128::MAX;

/// Maximum number of payments allowed in a single `batch_distribute` call.
/// Caps CPU/memory usage well within Soroban resource limits and aligns with
/// the vault's `MAX_BATCH_SIZE` for `batch_deduct`.
pub const MAX_BATCH_SIZE: u32 = 50;
pub const MAX_MESSAGE_LEN: u32 = 256;

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
    pub message: String,
}

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
    pub fn accept_admin(env: Env, caller: Address) {
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

    /// Complete the admin transfer. Legacy name for `accept_admin`.
    pub fn claim_admin(env: Env, caller: Address) {
        Self::accept_admin(env, caller);
    }

    /// Cancel a pending admin transfer. Only the current admin may call this.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be the current admin; must authorize.
    ///
    /// # Panics
    /// * If the caller is not the current admin.
    /// * If no admin transfer is pending.
    ///
    /// # Events
    /// Emits `admin_cancelled` event with `(current_admin, pending_admin)`.
    pub fn cancel_admin_transfer(env: Env, caller: Address) {
        caller.require_auth();
        let current = Self::get_admin(env.clone());
        if caller != current {
            panic!("unauthorized: caller is not admin");
        }
        let inst = env.storage().instance();
        let pending: Address = inst
            .get(&Symbol::new(&env, PENDING_ADMIN_KEY))
            .expect("no admin transfer pending");

        inst.remove(&Symbol::new(&env, PENDING_ADMIN_KEY));
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        env.events()
            .publish((events::event_admin_cancelled(&env), current, pending), ());
    }

    /// Return the pending admin address, or `None` if no two-step admin transfer is in progress.
    ///
    /// Integrators can poll this to detect an in-flight admin handover
    /// before `accept_admin` or `claim_admin` is called.
    ///
    /// # Returns
    /// `Some(Address)` of the nominated admin, or `None` when no transfer is pending.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, PENDING_ADMIN_KEY))
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

    /// Deposit accumulated protocol yield into the revenue pool.
    ///
    /// The current admin acts as the treasury authority. The treasury must
    /// authorize the call, and USDC is transferred from that treasury address to
    /// this revenue-pool contract. The cumulative deposited-yield metric is
    /// updated atomically with the transfer and event emission.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `treasury` - Must be the current admin and must authorize the call.
    /// * `amount` - USDC amount in base units. Must be positive.
    /// * `source` - Short source label for indexers, e.g. `fees` or `yield`.
    ///
    /// # Panics
    /// * If `treasury` is not the current admin (`"unauthorized: caller is not admin"`).
    /// * If `amount` is zero or negative (`"amount must be positive"`).
    /// * If the cumulative metric would overflow (`"cumulative yield overflow"`).
    /// * If the revenue pool has not been initialized.
    ///
    /// # Events
    /// Emits `yield_deposited` with `treasury` as topic and
    /// `(amount, source, cumulative_yield_deposited)` as data.
    pub fn deposit_yield(env: Env, treasury: Address, amount: i128, source: Symbol) {
        treasury.require_auth();
        let admin = Self::get_admin(env.clone());
        if treasury != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }
        if amount <= 0 {
            panic!("{}", ERR_AMOUNT_NOT_POSITIVE);
        }

        let previous_total = Self::get_cumulative_yield_deposited(env.clone());
        let new_total = match previous_total.checked_add(amount) {
            Some(total) => total,
            None => panic!("cumulative yield overflow"),
        };

        let usdc_address: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect(ERR_NOT_INITIALIZED);
        let usdc = token::Client::new(&env, &usdc_address);
        let contract_address = env.current_contract_address();

        let inst = env.storage().instance();
        inst.set(
            &Symbol::new(&env, CUMULATIVE_YIELD_DEPOSITED_KEY),
            &new_total,
        );
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        usdc.transfer(&treasury, &contract_address, &amount);
        env.events().publish(
            (events::event_yield_deposited(&env), treasury),
            (amount, source, new_total),
        );
    }

    /// Return the cumulative USDC yield deposited through [`Self::deposit_yield`].
    ///
    /// Defaults to zero before the first yield deposit.
    pub fn get_cumulative_yield_deposited(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, CUMULATIVE_YIELD_DEPOSITED_KEY))
            .unwrap_or(0)
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
    /// # Errors
    /// Returns a typed [`RevenuePoolError`] for batch-size violations so callers can
    /// branch on a stable numeric code without parsing panic strings:
    /// * [`RevenuePoolError::BatchEmpty`] if `payments` is empty.
    /// * [`RevenuePoolError::BatchTooLarge`] if `payments` exceeds [`MAX_BATCH_SIZE`] entries.
    ///   Use [`chunk_iter`] to pre-split large payout lists and avoid this error.
    ///
    /// # Panics
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
    pub fn batch_distribute(
        env: Env,
        caller: Address,
        payments: Vec<(Address, i128)>,
    ) -> Result<(), RevenuePoolError> {
        // Phase 0: Authorization
        caller.require_auth();
        Self::require_not_paused(&env);
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("{}", ERR_UNAUTHORIZED);
        }

        // Size guards return typed errors so backend integrators can branch on a
        // stable code instead of parsing panic strings. See `chunk_iter`.
        let n = payments.len();
        if n == 0 {
            return Err(RevenuePoolError::BatchEmpty);
        }
        if n > MAX_BATCH_SIZE {
            return Err(RevenuePoolError::BatchTooLarge);
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
        env.storage()
            .instance()
            .extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

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

        Ok(())
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
    /// Returns `None` if no version has been stored yet.
    pub fn get_version(env: Env) -> Option<BytesN<32>> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, VERSION_KEY))
    }

    /// Broadcast an emergency message from the admin.
    ///
    /// Only the current admin may call this function.
    /// The message length is capped at 256 characters.
    ///
    /// # Arguments
    /// * `env` - The environment running the contract.
    /// * `caller` - Must be the current admin; must authorize.
    /// * `severity` - Severity level of the broadcast (Info/Warn/Crit).
    /// * `message` - The broadcast message, capped at 256 characters.
    ///
    /// # Panics
    /// * If the caller is not the current admin.
    /// * If the message length exceeds 256 characters.
    /// * If the message is empty.
    pub fn broadcast(env: Env, caller: Address, severity: Severity, message: String) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
        }
        let len = message.len();
        if len == 0 {
            panic!("message cannot be empty");
        }
        if len > MAX_MESSAGE_LEN {
            panic!("message length exceeds maximum of 256 characters");
        }
        env.events().publish(
            (events::event_admin_broadcast(&env), caller),
            AdminBroadcast { severity, message },
        );
    }
}

mod events;
/// Split `payments` into consecutive chunks of at most `chunk_size` legs each,
/// preserving order.
///
/// Intended for backend integrators who need to distribute to more than
/// [`MAX_BATCH_SIZE`] developers: pre-chunk the full payout list and submit one
/// [`RevenuePool::batch_distribute`] call per chunk. Every chunk is guaranteed to
/// satisfy the size cap, so no call ever returns [`RevenuePoolError::BatchTooLarge`],
/// and there is no panic string to parse.
///
/// The last chunk may contain fewer than `chunk_size` entries. An empty `payments`
/// vector — or a `chunk_size` of `0` — yields an empty result (no chunks). A single
/// remaining leg produces a one-element chunk.
///
/// This is a pure, read-only helper: it performs no storage access, no
/// authorization, and moves no tokens.
///
/// # Examples
/// ```ignore
/// // Distribute to an arbitrarily large list, MAX_BATCH_SIZE legs at a time.
/// for chunk in chunk_iter(&env, payments, MAX_BATCH_SIZE).iter() {
///     pool.batch_distribute(&admin, &chunk);
/// }
/// ```
pub fn chunk_iter(
    env: &Env,
    payments: Vec<(Address, i128)>,
    chunk_size: u32,
) -> Vec<Vec<(Address, i128)>> {
    let mut chunks: Vec<Vec<(Address, i128)>> = Vec::new(env);
    // A zero chunk size has no well-defined chunking; return no chunks rather
    // than looping forever.
    if chunk_size == 0 {
        return chunks;
    }

    let mut current: Vec<(Address, i128)> = Vec::new(env);
    for payment in payments.iter() {
        current.push_back(payment);
        if current.len() == chunk_size {
            chunks.push_back(current);
            current = Vec::new(env);
        }
    }
    // Flush the trailing partial chunk, if any.
    if !current.is_empty() {
        chunks.push_back(current);
    }

    chunks
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_balance;

#[cfg(test)]
mod test_invariant;

#[cfg(test)]
mod test_proptest;

#[cfg(test)]
mod test_error_codes;
