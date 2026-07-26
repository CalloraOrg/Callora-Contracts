#![no_std]

pub mod emergency;
pub mod events;

use emergency::{PendingEmergencyDrain, EMERGENCY_DRAIN_KEY, EMERGENCY_DRAIN_TIMELOCK_SECONDS};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, BytesN, Env, Map, String,
    Symbol, Vec,
};

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// String-keyed storage identifiers.  Using a single `DataKey` enum keeps the
/// key space tidy and avoids accidental key collisions with raw strings.
#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Admin,
    UsdcToken,
    Paused,
}

// String keys for fields that must remain stable across upgrades.
const ADMIN_KEY: &str = "admin";
const USDC_KEY: &str = "usdc";
const PAUSED_KEY: &str = "paused";
const PENDING_ADMIN_KEY: &str = "pending_admin";
const PAUSE_GUARDIAN_KEY: &str = "pause_guardian";
const MAX_DISTRIBUTE_KEY: &str = "max_distribute";
const CUMULATIVE_YIELD_DEPOSITED_KEY: &str = "cumulative_yield";
const VERSION_KEY: &str = "version";

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default per-leg distribution cap — effectively unlimited until explicitly set.
pub const DEFAULT_MAX_DISTRIBUTE: i128 = i128::MAX;

/// Maximum number of payments in a single `batch_distribute` call.
pub const MAX_BATCH_SIZE: u32 = 50;

/// Maximum admin broadcast message length in characters.
pub const MAX_MESSAGE_LEN: u32 = 256;

/// TTL bump constants for instance storage archival risk mitigation.
/// Soroban archives ledger entries after ~7 days (631 ledgers) of inactivity.
///
/// - `BUMP_AMOUNT`: extend TTL by 10 000 ledgers (≈16 days)
/// - `LIFETIME_THRESHOLD`: minimum TTL before triggering a bump (≈1.5 days)
pub const BUMP_AMOUNT: u32 = 10_000;
pub const LIFETIME_THRESHOLD: u32 = 1_000;

// ---------------------------------------------------------------------------
// Error strings
// ---------------------------------------------------------------------------

const ERR_UNAUTHORIZED: &str = "unauthorized: caller is not admin";
const ERR_UNAUTHORIZED_PAUSE: &str = "unauthorized: caller is not admin or pause guardian";
const ERR_PAUSED: &str = "revenue pool is paused";
const ERR_NOT_INITIALIZED: &str = "revenue pool not initialized";
const ERR_AMOUNT_NOT_POSITIVE: &str = "amount must be positive";
const ERR_AMOUNT_EXCEEDS_MAX_DISTRIBUTE: &str = "amount exceeds max_distribute";
const ERR_INSUFFICIENT_BALANCE: &str = "insufficient USDC balance";
const ERR_DUPLICATE_RECIPIENT: &str = "duplicate recipient in batch";

// ---------------------------------------------------------------------------
// Typed error enum (stable numeric codes for SDK integrators)
// ---------------------------------------------------------------------------

/// Typed errors returned by `batch_distribute` so off-chain callers can branch
/// on a stable numeric code without parsing panic strings.
#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevenuePoolError {
    /// `batch_distribute` received an empty `payments` vector.
    BatchEmpty = 1,
    /// `batch_distribute` exceeded `MAX_BATCH_SIZE` entries.
    BatchTooLarge = 2,
}

// ---------------------------------------------------------------------------
// Auxiliary contract-types
// ---------------------------------------------------------------------------

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

/// Remaining storage TTL information for a storage category.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct StorageEntryTtl {
    pub category: String,
    pub key_desc: String,
    pub storage_type: String,
    pub ttl: u32,
    pub threshold: u32,
    pub bump_amount: u32,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct RevenuePool;

#[contractimpl]
impl RevenuePool {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialize the revenue pool with an admin and the USDC token address.
    ///
    /// Can only be called once. Rejects `usdc_token == contract address`.
    ///
    /// # Panics
    /// * `"revenue pool already initialized"` — called more than once.
    /// * `"invalid config: usdc_token cannot be the contract itself"` — bad token address.
    /// * `"invalid config: usdc_token cannot be the admin address"` - token/admin aliasing.
    ///
    /// # Events
    /// Emits `init` with `admin` as topic and `usdc_token` as data.
    pub fn init(env: Env, admin: Address, usdc_token: Address) {
        if env.storage().instance().has(&Symbol::new(&env, ADMIN_KEY)) {
            panic!("revenue pool already initialized");
        }
        let contract_addr = env.current_contract_address();
        if usdc_token == contract_addr {
            panic!("invalid config: usdc_token cannot be the contract itself");
        }
        if usdc_token == admin {
            panic!("invalid config: usdc_token cannot be the admin address");
        }
        let inst = env.storage().instance();
        inst.set(&Symbol::new(&env, ADMIN_KEY), &admin);
        inst.set(&Symbol::new(&env, USDC_KEY), &usdc_token);
        inst.set(&Symbol::new(&env, PAUSED_KEY), &false);
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);
        env.events()
            .publish((events::event_init(&env), admin), usdc_token);
    }

    // -----------------------------------------------------------------------
    // Admin helpers (internal)
    // -----------------------------------------------------------------------

    fn admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(env, ADMIN_KEY))
            .expect(ERR_NOT_INITIALIZED)
    }

    fn require_admin(env: &Env, caller: &Address) {
        if *caller != Self::admin(env) {
            panic!("{}", ERR_UNAUTHORIZED);
        }
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

    fn validate_recipient(recipient: &Address, contract_self: &Address) {
        // Rule 1 — no self-distributions.
        if recipient == contract_self {
            panic!("invalid recipient: cannot distribute to the contract itself");
        }
    }

    // -----------------------------------------------------------------------
    // Admin view
    // -----------------------------------------------------------------------

    /// Return the current admin address.
    ///
    /// # Panics
    /// * `"revenue pool not initialized"` — called before `init`.
    pub fn get_admin(env: Env) -> Address {
        Self::admin(&env)
    }

    /// Return the USDC token address configured for this pool.
    ///
    /// # Panics
    /// * `"revenue pool not initialized"` — called before `init`.
    pub fn get_usdc_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect(ERR_NOT_INITIALIZED)
    }

    // -----------------------------------------------------------------------
    // Two-step admin rotation
    // -----------------------------------------------------------------------

    /// Nominate a new admin. Only the current admin may call.
    /// The nominee must call `claim_admin` (alias `accept_admin`) to complete.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — caller is not the current admin.
    ///
    /// # Events
    /// Emits `admin_changed` with `(current, new_admin)` and
    /// `admin_transfer_started` with `new_admin`.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let current = Self::admin(&env);
        if caller != current {
            panic!("{}", ERR_UNAUTHORIZED);
        }
        let inst = env.storage().instance();
        inst.set(&Symbol::new(&env, PENDING_ADMIN_KEY), &new_admin);
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);
        env.events().publish(
            (events::event_admin_changed(&env), current.clone()),
            (current.clone(), new_admin.clone()),
        );
        env.events().publish(
            (events::event_admin_transfer_started(&env), current),
            new_admin,
        );
    }

    /// Complete the admin transfer. Only the pending admin may call.
    ///
    /// # Panics
    /// * `"no pending admin"` — no transfer is in progress.
    /// * `"unauthorized: caller is not pending admin"` — wrong caller.
    ///
    /// # Events
    /// Emits `admin_transfer_completed` with the new admin as topic.
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

    /// Alias for `accept_admin` — legacy name kept for backward compatibility.
    ///
    /// # Panics
    /// * `"no pending admin"` - no transfer is in progress.
    /// * `"unauthorized: caller is not pending admin"` - wrong caller.
    ///
    /// # Events
    /// Emits `admin_transfer_completed` with the new admin as topic.
    pub fn claim_admin(env: Env, caller: Address) {
        Self::accept_admin(env, caller);
    }

    /// Cancel a pending admin transfer. Only the current admin may call.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — caller is not the current admin.
    /// * `"no admin transfer pending"` — no transfer in progress.
    ///
    /// # Events
    /// Emits `admin_cancelled` with `(current_admin, pending_admin)`.
    pub fn cancel_admin_transfer(env: Env, caller: Address) {
        caller.require_auth();
        let current = Self::admin(&env);
        if caller != current {
            panic!("{}", ERR_UNAUTHORIZED);
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

    /// Return the pending admin address, or `None` if no transfer is in progress.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, PENDING_ADMIN_KEY))
    }

    // -----------------------------------------------------------------------
    // Pause guardian
    // -----------------------------------------------------------------------

    /// Set or replace the emergency pause guardian.
    ///
    /// The guardian may call `pause` but has no authority to unpause, distribute,
    /// rotate admin, change caps, or upgrade the contract.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — caller is not the current admin.
    ///
    /// # Events
    /// Emits `pause_guardian_set` with `caller` as topic and `guardian` as data.
    pub fn set_pause_guardian(env: Env, caller: Address, guardian: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PAUSE_GUARDIAN_KEY), &guardian);
        env.storage()
            .instance()
            .extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);
        env.events()
            .publish((events::event_pause_guardian_set(&env), caller), guardian);
    }

    /// Clear the emergency pause guardian role. Only the admin may call.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — caller is not the current admin.
    /// * `"no pause guardian set"` — no guardian is configured.
    ///
    /// # Events
    /// Emits `pause_guardian_cleared` with `caller` as topic and the previous guardian as data.
    pub fn clear_pause_guardian(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        let inst = env.storage().instance();
        let guardian: Address = inst
            .get(&Symbol::new(&env, PAUSE_GUARDIAN_KEY))
            .expect("no pause guardian set");
        inst.remove(&Symbol::new(&env, PAUSE_GUARDIAN_KEY));
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);
        env.events().publish(
            (events::event_pause_guardian_cleared(&env), caller),
            guardian,
        );
    }

    /// Return the configured pause guardian, or `None` if unset.
    pub fn get_pause_guardian(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, PAUSE_GUARDIAN_KEY))
    }

    // -----------------------------------------------------------------------
    // Pause / unpause
    // -----------------------------------------------------------------------

    /// Activate the circuit-breaker. Blocks `distribute` and `batch_distribute`.
    ///
    /// The admin or configured pause guardian may call.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED_PAUSE` — caller is neither admin nor guardian.
    /// * `"revenue pool already paused"` — pool is already paused.
    ///
    /// # Events
    /// Emits `pause_set` with `caller` as topic and `true` as data.
    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        let admin = Self::admin(&env);
        let guardian = Self::get_pause_guardian(env.clone());
        if caller != admin && guardian.as_ref() != Some(&caller) {
            panic!("{}", ERR_UNAUTHORIZED_PAUSE);
        }
        assert!(!Self::is_paused(env.clone()), "revenue pool already paused");
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PAUSED_KEY), &true);
        env.storage()
            .instance()
            .extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);
        env.events()
            .publish((events::event_pause_set(&env), caller), true);
    }

    /// Deactivate the circuit-breaker. Only the admin may call.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — caller is not the current admin.
    /// * `"revenue pool not paused"` — pool is not currently paused.
    ///
    /// # Events
    /// Emits `pause_set` with `caller` as topic and `false` as data.
    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        assert!(Self::is_paused(env.clone()), "revenue pool not paused");
        env.storage()
            .instance()
            .set(&Symbol::new(&env, PAUSED_KEY), &false);
        env.storage()
            .instance()
            .extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);
        env.events()
            .publish((events::event_pause_set(&env), caller), false);
    }

    /// Return `true` if the revenue pool is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&Symbol::new(&env, PAUSED_KEY))
            .unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // Yield deposit
    // -----------------------------------------------------------------------

    /// Event-only helper to log an incoming vault payment for indexers.
    ///
    /// Does **not** move tokens. Only the admin may call.
    ///
    /// # Panics
    /// * `"unauthorized: caller is not admin"` — wrong caller.
    ///
    /// # Events
    /// Emits `receive_payment` with `caller` as topic and `(amount, from_vault)` as data.
    pub fn receive_payment(env: Env, caller: Address, amount: i128, from_vault: bool) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.events().publish(
            (events::event_receive_payment(&env), caller),
            (amount, from_vault),
        );
    }

    /// Deposit accumulated protocol yield into the revenue pool.
    ///
    /// Transfers USDC from `treasury` to this contract, then updates the
    /// cumulative yield metric. The external token call happens before storage
    /// writes so a callee panic cannot leave the metric ahead of a failed
    /// transfer (Soroban still rolls the whole invocation back on panic).
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — treasury is not the current admin.
    /// * `ERR_AMOUNT_NOT_POSITIVE` — amount ≤ 0.
    /// * `"cumulative yield overflow"` — cumulative metric would overflow `i128`.
    ///
    /// # Events
    /// Emits `yield_deposited` with `treasury` as topic and
    /// `(amount, source, cumulative_yield_deposited)` as data.
    pub fn deposit_yield(env: Env, treasury: Address, amount: i128, source: Symbol) {
        treasury.require_auth();
        if treasury != Self::admin(&env) {
            panic!("unauthorized: caller is not treasury");
        }
        if amount <= 0 {
            panic!("{}", ERR_AMOUNT_NOT_POSITIVE);
        }
        let previous_total = Self::get_cumulative_yield_deposited(env.clone());
        let new_total = previous_total
            .checked_add(amount)
            .unwrap_or_else(|| panic!("cumulative yield overflow"));
        let usdc_address: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect(ERR_NOT_INITIALIZED);
        let usdc = token::Client::new(&env, &usdc_address);
        let contract_address = env.current_contract_address();
        // Transfer before persisting the cumulative metric so a callee panic
        // (token revert) cannot leave storage ahead of a failed transfer.
        // Soroban still rolls the whole invocation back on panic; this order
        // keeps the success path effects-after-external-call.
        usdc.transfer(&treasury, &contract_address, &amount);
        let inst = env.storage().instance();
        inst.set(
            &Symbol::new(&env, CUMULATIVE_YIELD_DEPOSITED_KEY),
            &new_total,
        );
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);
        env.events().publish(
            (events::event_yield_deposited(&env), treasury),
            (amount, source, new_total),
        );
    }

    /// Return the cumulative USDC yield deposited via `deposit_yield`. Defaults to 0.
    pub fn get_cumulative_yield_deposited(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, CUMULATIVE_YIELD_DEPOSITED_KEY))
            .unwrap_or(0)
    }

    // -----------------------------------------------------------------------
    // Distribution cap
    // -----------------------------------------------------------------------

    /// Return the per-leg distribution cap. Defaults to `i128::MAX` when unset.
    pub fn get_max_distribute(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, MAX_DISTRIBUTE_KEY))
            .unwrap_or(DEFAULT_MAX_DISTRIBUTE)
    }

    /// Set the maximum amount distributable per leg. Must be positive. Admin only.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — caller is not the current admin.
    /// * `"max_distribute must be positive"` — value ≤ 0.
    ///
    /// # Events
    /// Emits `set_max_distribute` with `(old_max, new_max)`.
    pub fn set_max_distribute(env: Env, caller: Address, max_distribute: i128) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        assert!(max_distribute > 0, "max_distribute must be positive");
        let old_max = Self::get_max_distribute(env.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, MAX_DISTRIBUTE_KEY), &max_distribute);
        env.storage()
            .instance()
            .extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);
        env.events().publish(
            (events::event_set_max_distribute(&env), Self::admin(&env)),
            (old_max, max_distribute),
        );
    }

    // -----------------------------------------------------------------------
    // Distribution
    // -----------------------------------------------------------------------

    /// Distribute USDC from this contract to a single developer wallet.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — caller is not the current admin.
    /// * `ERR_PAUSED` — pool is paused.
    /// * `ERR_AMOUNT_NOT_POSITIVE` — amount ≤ 0.
    /// * `ERR_AMOUNT_EXCEEDS_MAX_DISTRIBUTE` — amount exceeds the cap.
    /// * `"invalid recipient: cannot distribute to the contract itself"`.
    /// * `ERR_INSUFFICIENT_BALANCE` — pool holds less than `amount`.
    ///
    /// # Events
    /// Emits `distribute` with `to` as topic and `amount` as data.
    pub fn distribute(env: Env, caller: Address, to: Address, amount: i128) {
        caller.require_auth();
        Self::require_not_paused(&env);
        Self::require_admin(&env, &caller);
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

    /// Distribute USDC to multiple developer wallets in one atomic transaction.
    ///
    /// All validation runs before any transfer. Returns a typed error for size
    /// violations; panics for all other invalid conditions.
    ///
    /// # Errors
    /// * `RevenuePoolError::BatchEmpty` — `payments` is empty.
    /// * `RevenuePoolError::BatchTooLarge` — `payments` exceeds `MAX_BATCH_SIZE`.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED`, `ERR_PAUSED`, `ERR_AMOUNT_NOT_POSITIVE`,
    ///   `ERR_AMOUNT_EXCEEDS_MAX_DISTRIBUTE`, `ERR_DUPLICATE_RECIPIENT`,
    ///   `"total overflow"`, `ERR_INSUFFICIENT_BALANCE`,
    ///   `"invalid recipient: cannot distribute to the contract itself"`.
    ///
    /// # Events
    /// Emits one `batch_distribute` event per payment leg, after all validation.
    pub fn batch_distribute(
        env: Env,
        caller: Address,
        payments: Vec<(Address, i128)>,
    ) -> Result<(), RevenuePoolError> {
        caller.require_auth();
        Self::require_not_paused(&env);
        Self::require_admin(&env, &caller);

        let n = payments.len();
        if n == 0 {
            return Err(RevenuePoolError::BatchEmpty);
        }
        if n > MAX_BATCH_SIZE {
            return Err(RevenuePoolError::BatchTooLarge);
        }

        let max_distribute = Self::get_max_distribute(env.clone());
        let mut seen: Map<Address, bool> = Map::new(&env);
        let mut total_amount: i128 = 0;

        for payment in payments.iter() {
            let (to, amount) = payment;
            if seen.contains_key(to.clone()) {
                panic!("{}", ERR_DUPLICATE_RECIPIENT);
            }
            seen.set(to.clone(), true);
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

        env.storage()
            .instance()
            .extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        for payment in payments.iter() {
            let (to, amount) = payment;
            Self::validate_recipient(&to, &contract_address);
            usdc.transfer(&contract_address, &to, &amount);
            env.events()
                .publish((events::event_batch_distribute(&env), to), amount);
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Balance view
    // -----------------------------------------------------------------------

    /// Return this contract's on-ledger USDC balance.
    ///
    /// # Panics
    /// * `ERR_NOT_INITIALIZED` — called before `init`.
    pub fn balance(env: Env) -> i128 {
        let usdc_addr: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, USDC_KEY))
            .expect(ERR_NOT_INITIALIZED);
        let usdc = token::Client::new(&env, &usdc_addr);
        usdc.balance(&env.current_contract_address())
    }

    // -----------------------------------------------------------------------
    // Upgrade
    // -----------------------------------------------------------------------

    /// Admin-gated contract upgrade. Replaces the WASM and persists the version.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — caller is not the current admin.
    ///
    /// # Events
    /// Emits `upgraded` with `admin` as topic and `new_wasm_hash` as data.
    pub fn upgrade(env: Env, caller: Address, new_wasm_hash: BytesN<32>) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());
        env.storage()
            .instance()
            .set(&Symbol::new(&env, VERSION_KEY), &new_wasm_hash.clone());
        env.storage()
            .instance()
            .extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);
        env.events().publish(
            (events::event_upgraded(&env), Self::admin(&env)),
            new_wasm_hash,
        );
    }

    /// Return the stored WASM version hash, or `None` if never upgraded.
    pub fn get_version(env: Env) -> Option<BytesN<32>> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, VERSION_KEY))
    }

    /// Return the crate version string baked in at compile time.
    pub fn version(env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&env, env!("CARGO_PKG_VERSION"))
    }

    // -----------------------------------------------------------------------
    // Admin broadcast
    // -----------------------------------------------------------------------

    /// Broadcast an emergency message from the admin.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — caller is not the current admin.
    /// * `"message cannot be empty"` — empty message.
    /// * `"message length exceeds maximum of 256 characters"`.
    ///
    /// # Events
    /// Emits `admin_broadcast` with `caller` as topic and `AdminBroadcast` as data.
    pub fn broadcast(env: Env, caller: Address, severity: Severity, message: String) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
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

    // -----------------------------------------------------------------------
    // Storage TTL introspection
    // -----------------------------------------------------------------------

    /// Return remaining TTL information for each storage category.
    pub fn get_storage_ttl(env: Env) -> Vec<StorageEntryTtl> {
        let mut result = Vec::new(&env);
        let instance_ttl = {
            #[cfg(any(test, feature = "testutils"))]
            {
                use soroban_sdk::testutils::storage::Instance as _;
                env.storage().instance().get_ttl()
            }
            #[cfg(not(any(test, feature = "testutils")))]
            {
                BUMP_AMOUNT
            }
        };
        result.push_back(StorageEntryTtl {
            category: String::from_str(&env, "Instance"),
            key_desc: String::from_str(&env, "Instance"),
            storage_type: String::from_str(&env, "Instance"),
            ttl: instance_ttl,
            threshold: LIFETIME_THRESHOLD,
            bump_amount: BUMP_AMOUNT,
        });
        result
    }

    // -----------------------------------------------------------------------
    // Emergency drain — Multisig + timelock
    // -----------------------------------------------------------------------

    /// Propose a timelocked USDC emergency drain to `treasury`.
    ///
    /// Stores a [`PendingEmergencyDrain`] snapshot. The drain may only be
    /// executed after `EMERGENCY_DRAIN_TIMELOCK_SECONDS` (24 h) have elapsed,
    /// giving operators a window to cancel a fraudulent proposal.
    ///
    /// A new call **replaces** any existing pending proposal, resetting the
    /// timelock clock — this is intentional to allow the admin to correct a
    /// mistaken amount or destination without first cancelling.
    ///
    /// When the admin is a Stellar multisig account, `require_auth` enforces
    /// the native multi-signature threshold automatically.
    ///
    /// # Arguments
    /// * `caller` — Must be the current admin; must authorize.
    /// * `treasury` — Destination address for the drained USDC. Cannot be the
    ///   contract itself.
    /// * `amount` — USDC amount in base units. Must be positive.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — caller is not the current admin.
    /// * `ERR_AMOUNT_NOT_POSITIVE` — amount ≤ 0.
    /// * `"invalid recipient: cannot drain to the contract itself"` — self-drain.
    /// * `"timelock overflow"` — proposed_at + 86 400 would overflow `u64`.
    ///
    /// # Events
    /// Emits `emergency_drain_proposed` with `caller` as topic and the full
    /// [`PendingEmergencyDrain`] snapshot as data.
    pub fn propose_emergency_drain(env: Env, caller: Address, treasury: Address, amount: i128) {
        caller.require_auth();
        Self::require_admin(&env, &caller);
        if amount <= 0 {
            panic!("{}", ERR_AMOUNT_NOT_POSITIVE);
        }
        let contract_address = env.current_contract_address();
        if treasury == contract_address {
            panic!("invalid recipient: cannot drain to the contract itself");
        }
        let proposed_at: u64 = env.ledger().timestamp();
        let execute_after: u64 = proposed_at
            .checked_add(EMERGENCY_DRAIN_TIMELOCK_SECONDS)
            .unwrap_or_else(|| panic!("timelock overflow"));

        let drain = PendingEmergencyDrain {
            to: treasury,
            amount,
            proposed_at,
            execute_after,
        };

        let inst = env.storage().instance();
        inst.set(&Symbol::new(&env, EMERGENCY_DRAIN_KEY), &drain);
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        env.events().publish(
            (events::event_emergency_drain_proposed(&env), caller),
            drain,
        );
    }

    /// Execute a pending emergency drain after the timelock has expired.
    ///
    /// Transfers `pending.amount` USDC from this contract to `pending.to`,
    /// then removes the pending snapshot to prevent replay.
    ///
    /// # Arguments
    /// * `caller` — Must be the current admin; must authorize.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — caller is not the current admin.
    /// * `"no pending emergency drain"` — no proposal exists.
    /// * `"emergency drain timelock has not expired"` — too early to execute.
    /// * `ERR_INSUFFICIENT_BALANCE` — pool holds less than the proposed amount.
    ///
    /// # Events
    /// Emits `emergency_drain_executed` with `caller` as topic and
    /// `(to, amount, proposed_at, executed_at)` as data.
    pub fn execute_emergency_drain(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let inst = env.storage().instance();
        let drain: PendingEmergencyDrain = inst
            .get(&Symbol::new(&env, EMERGENCY_DRAIN_KEY))
            .expect("no pending emergency drain");

        let now: u64 = env.ledger().timestamp();
        if now < drain.execute_after {
            panic!("emergency drain timelock has not expired");
        }

        let usdc_address: Address = inst
            .get(&Symbol::new(&env, USDC_KEY))
            .expect(ERR_NOT_INITIALIZED);
        let usdc = token::Client::new(&env, &usdc_address);
        let contract_address = env.current_contract_address();

        if usdc.balance(&contract_address) < drain.amount {
            panic!("{}", ERR_INSUFFICIENT_BALANCE);
        }

        // Remove before transfer to prevent re-entrancy replay.
        inst.remove(&Symbol::new(&env, EMERGENCY_DRAIN_KEY));
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        usdc.transfer(&contract_address, &drain.to, &drain.amount);

        env.events().publish(
            (events::event_emergency_drain_executed(&env), caller),
            (drain.to, drain.amount, drain.proposed_at, now),
        );
    }

    /// Cancel a pending emergency drain. Only the admin may call.
    ///
    /// Removes the pending snapshot and emits an audit event. May be called at
    /// any time before execution, including before the timelock expires.
    ///
    /// # Arguments
    /// * `caller` — Must be the current admin; must authorize.
    ///
    /// # Panics
    /// * `ERR_UNAUTHORIZED` — caller is not the current admin.
    /// * `"no pending emergency drain"` — no proposal exists to cancel.
    ///
    /// # Events
    /// Emits `emergency_drain_cancelled` with `caller` as topic and the full
    /// [`PendingEmergencyDrain`] snapshot as data.
    pub fn cancel_emergency_drain(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin(&env, &caller);

        let inst = env.storage().instance();
        let drain: PendingEmergencyDrain = inst
            .get(&Symbol::new(&env, EMERGENCY_DRAIN_KEY))
            .expect("no pending emergency drain");

        inst.remove(&Symbol::new(&env, EMERGENCY_DRAIN_KEY));
        inst.extend_ttl(LIFETIME_THRESHOLD, BUMP_AMOUNT);

        env.events().publish(
            (events::event_emergency_drain_cancelled(&env), caller),
            drain,
        );
    }

    /// Return the pending emergency drain proposal, or `None` if none exists.
    ///
    /// Off-chain monitors and the admin can poll this to verify or cancel a
    /// pending drain before the timelock expires.
    pub fn get_pending_emergency_drain(env: Env) -> Option<PendingEmergencyDrain> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, EMERGENCY_DRAIN_KEY))
    }
}

// ---------------------------------------------------------------------------
// chunk_iter — pure batch-splitting helper
// ---------------------------------------------------------------------------

/// Split `payments` into consecutive chunks of at most `chunk_size` legs each,
/// preserving order.
///
/// Intended for backend integrators who need to distribute to more than
/// [`MAX_BATCH_SIZE`] developers: pre-chunk the full payout list and submit one
/// [`RevenuePool::batch_distribute`] call per chunk.
///
/// A zero `chunk_size` or empty `payments` yields no chunks.
/// This is a pure helper: no storage access, no auth, no token transfers.
///
/// # Arguments
/// * `env` - Soroban environment used to allocate the returned vectors.
/// * `payments` - Ordered `(recipient, amount)` pairs to split.
/// * `chunk_size` - Maximum number of payment legs per returned chunk.
///
/// # Returns
/// A vector of consecutive payment chunks, each no larger than `chunk_size`.
pub fn chunk_iter(
    env: &Env,
    payments: Vec<(Address, i128)>,
    chunk_size: u32,
) -> Vec<Vec<(Address, i128)>> {
    let mut chunks: Vec<Vec<(Address, i128)>> = Vec::new(env);
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
    if !current.is_empty() {
        chunks.push_back(current);
    }
    chunks
}

// ---------------------------------------------------------------------------
// Test modules
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_balance;

#[cfg(test)]
mod test_error_codes;

#[cfg(test)]
mod test_emergency;

#[cfg(test)]
mod test_invariant;

#[cfg(test)]
mod test_proptest;

#[cfg(test)]
extern crate std;

#[cfg(test)]
mod rustdoc_tests {
    #[test]
    fn every_public_fn_in_lib_has_rustdoc() {
        let source = include_str!("lib.rs")
            .split("// ---------------------------------------------------------------------------\n// Test modules")
            .next()
            .expect("lib.rs contains test module marker");
        let lines: std::vec::Vec<&str> = source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if !(trimmed.starts_with("pub fn ")
                || trimmed.starts_with("pub(crate) fn ")
                || trimmed.starts_with("pub(super) fn "))
            {
                continue;
            }

            let has_rustdoc = lines[..idx]
                .iter()
                .rev()
                .map(|candidate| candidate.trim_start())
                .find(|candidate| !candidate.is_empty())
                .map(|candidate| candidate.starts_with("///"))
                .unwrap_or(false);

            assert!(
                has_rustdoc,
                "public function on line {} is missing /// rustdoc: {}",
                idx + 1,
                trimmed
            );
        }
    }
}
