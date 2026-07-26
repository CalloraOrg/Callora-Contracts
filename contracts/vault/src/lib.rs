#![allow(clippy::too_many_arguments)]
#![no_std]
//!
//! # Callora Vault Contract — deposit/withdraw/deduct/distribute with pause circuit-breaker.
//!
//! ## Escape-Hatch Admin Actions (#482)
//!
//! The following critical admin actions are guarded by a mandatory timelock:
//!
//! | Action  | Propose              | Execute              | Cancel              |
//! |---------|----------------------|----------------------|---------------------|
//! | pause   | `propose_pause`      | `execute_pause`      | `cancel_pause`      |
//! | upgrade | `propose_upgrade`    | `execute_upgrade`    | `cancel_upgrade`    |
//! | sweep   | `propose_sweep`      | `execute_sweep`      | `cancel_sweep`      |
//!
//! Window length is configured by `set_timelock_window(admin, seconds)` and
//! defaults to 48 h (`172_800`). Valid bounds are 1 h – 30 d. All three slots
//! are independent so multiple proposals can coexist concurrently.
//!
///
/// ## Pause Circuit Breaker
///
/// When the vault is paused:
/// - Deposits are blocked
/// - Single and batch deducts are blocked
/// - Owner withdrawals are ALLOWED (emergency recovery)
/// - Admin distribute is ALLOWED (emergency recovery of untracked surplus)
/// - Admin/owner configuration functions remain available
///
/// ## Request-ID Idempotency
///
/// `deduct` and `batch_deduct` accept an optional `request_id: Option<Symbol>`.
/// When `Some(id)` is supplied the contract persists a processed-request marker
/// in **temporary storage** and rejects any subsequent call that carries the same
/// `request_id`, returning `VaultError::DuplicateRequestId`.
///
/// This gives safe **at-least-once retry** semantics: a backend can replay a
/// failed transaction with the same `request_id` and the contract will either
/// succeed (first time) or return a deterministic error (duplicate).
///
/// When `request_id` is `None` no deduplication is performed; the call is
/// treated as a fire-and-forget deduction with no idempotency guarantee.
///
/// ### Retention / TTL
/// Processed-request markers live in persistent storage and are bumped to
/// `REQUEST_ID_BUMP_AMOUNT` ledgers on every successful deduct. The threshold
/// for triggering a bump is `REQUEST_ID_BUMP_THRESHOLD`. Because they are now
/// persistent, they do not silently archive. To prevent state bloat, an owner
/// can explicitly prune old markers using `prune_processed_requests`.
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, BytesN, Env, Symbol, Vec};

pub mod timelock;
pub mod views;

mod errors;
pub use errors::VaultError;

/// The default amount to extend the TTL of instance storage entries.
pub const INSTANCE_BUMP_AMOUNT: u32 = 17280 * 30; // 30 days
pub const INSTANCE_BUMP_THRESHOLD: u32 = 17280 * 14; // 14 days

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Owner,
    UsdcToken,
    Balance,
    AuthorizedCaller,
    MinDeposit,
    RevenuePool,
    MaxDeduct,
    Settlement,
    Paused,
    Depositor(Address),
    /// Pending revenue pool address (two-step transfer).
    PendingRevenuePool,
    /// Pending owner address (two-step transfer).
    PendingOwner,
}

/// Instance / persistent storage keys for the vault.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    UsdcToken,
    ProcessedRequest(Symbol),
    ReserveCap(Address),
    DeveloperConfig(Address),
    DeveloperState(Address),
    /// Configured timelock window length in seconds (admin-settable).
    TimelockWindow,
    /// Active pause proposal pending timelock expiry.
    PendingPause,
    /// Active upgrade proposal (wasm hash) pending timelock expiry.
    PendingUpgrade,
    /// Active sweep proposal (recipient + amount) pending timelock expiry.
    PendingSweep,
    /// Current admin address (defaults to owner at init).
    Admin,
    /// Pending admin address awaiting acceptance (two-step transfer).
    PendingAdmin,
    /// Recorded wasm hash after a successful upgrade.
    ContractVersion,
    /// Depositor allowlist (stored as Vec<Address>).
    AllowedDepositors,
}

#[cfg(target_arch = "wasm32")]
pub mod settlement {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32-unknown-unknown/release/callora_settlement.wasm"
    );
}

/// In native/test mode the settlement contract is registered directly in the
/// test `Env`. The vault emits USDC transfers; settlement accounting is
/// verified through its own public interface.
#[cfg(not(target_arch = "wasm32"))]
pub mod settlement {
    use soroban_sdk::{Address, Env};
    pub struct Client<'a> {
        _env: &'a Env,
        _addr: &'a Address,
    }
    impl<'a> Client<'a> {
        pub fn new(env: &'a Env, addr: &'a Address) -> Self {
            Client { _env: env, _addr: addr }
        }
        pub fn receive_payment(
            &self,
            _caller: &Address,
            _amount: &i128,
            _to_global_pool: &bool,
            _developer: &Option<Address>,
            _token: &Address,
            _nonce: &u32,
        ) {
        }
    }
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct CalloraVault;

#[contractimpl]
impl CalloraVault {
    fn bump_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn bump_persistent_key(env: &Env, key: &StorageKey) {
        env.storage()
            .persistent()
            .extend_ttl(key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    }

    fn require_positive_amount(amount: i128) {
        if amount <= 0 {
            panic!("amount must be positive");
        }
    }

    fn require_owner_auth(env: &Env, caller: &Address) {
        caller.require_auth();
        let owner = Self::load_owner(env);
        if caller != &owner {
            panic!("unauthorized: caller is not owner");
        }
    }

    fn build_meta(env: &Env) -> VaultMeta {
        let owner = Self::load_owner(env);
        let balance = Self::load_balance(env);
        let authorized_caller = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::AuthorizedCaller);
        let min_deposit = Self::load_min_deposit(env);
        VaultMeta { owner, balance, authorized_caller, min_deposit }
    }

    fn require_not_duplicate(env: &Env, request_id: &Symbol) {
        let key = StorageKey::ProcessedRequest(request_id.clone());
        if env.storage().persistent().has(&key) {
            panic!("duplicate request_id");
        }
    }

    /// Initialize the vault with its core configuration.
    ///
    /// Must be called exactly once before any other entrypoint. Stores the
    /// owner, USDC token, authorized caller, deposit/deduct limits,
    /// settlement address, and optional revenue pool.
    ///
    /// # Arguments
    /// * `owner` — Address that controls administrative actions (pause, config).
    /// * `usdc_token` — Stellar USDC token contract address.
    /// * `initial_balance` — Starting tracked balance (must match on-ledger USDC).
    /// * `authorized_caller` — Address allowed to invoke `deduct` / `batch_deduct`.
    /// * `min_deposit` — Minimum deposit amount in USDC micro-units; must be > 0.
    /// * `revenue_pool` — Optional revenue pool address for yield routing.
    /// * `max_deduct` — Maximum single deduction; must be >= `min_deposit`.
    /// * `settlement` — Settlement contract address for recording deductions.
    ///
    /// # Panics
    /// * `"Already initialized"` — called more than once.
    /// * `"min_deposit must be positive"` — `min_deposit <= 0`.
    /// * `"max_deduct must be positive"` — `max_deduct <= 0`.
    /// * `"min_deposit cannot exceed max_deduct"`.
    ///
    /// # Events
    /// None (initialization is a pure state write).
    pub fn init(
        env: Env,
        owner: Address,
        usdc_token: Address,
        initial_balance: Option<i128>,
        authorized_caller: Option<Address>,
        min_deposit: Option<i128>,
        revenue_pool: Option<Address>,
        max_deduct: i128,
        settlement: Address,
    ) {
        owner.require_auth();

        if env.storage().instance().has(&DataKey::Owner) {
            panic!("vault already initialized");
        }

        let vault_addr = env.current_contract_address();

        // Validate usdc_token
        if usdc_token == vault_addr {
            panic!("usdc_token cannot be vault address");
        }

        // Resolve defaults
        let initial_balance = initial_balance.unwrap_or(0);
        let min_deposit = min_deposit.unwrap_or(DEFAULT_MIN_DEPOSIT);
        let max_deduct = max_deduct.unwrap_or(DEFAULT_MAX_DEDUCT);

        // Validate
        if initial_balance < 0 {
            panic!("initial_balance must be non-negative");
        }
        if min_deposit <= 0 {
            panic!("min_deposit must be positive");
        }
        if max_deduct <= 0 {
            panic!("max_deduct must be positive");
        }
        if min_deposit > max_deduct {
            panic!("min_deposit cannot exceed max_deduct");
        }

        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage().instance().set(&DataKey::UsdcToken, &usdc_token);
        env.storage().instance().set(&DataKey::Balance, &initial_balance);
        env.storage().instance().set(&DataKey::AuthorizedCaller, &authorized_caller);
        env.storage().instance().set(&DataKey::MinDeposit, &min_deposit);
        
        if let Some(pool) = revenue_pool {
            env.storage().instance().set(&DataKey::RevenuePool, &pool);
        }
        
        env.storage().instance().set(&DataKey::MaxDeduct, &max_deduct);
        env.storage().instance().set(&DataKey::Settlement, &settlement);
        env.storage().instance().set(&DataKey::Paused, &false);
        // Admin defaults to owner at initialization.
        env.storage().instance().set(&StorageKey::Admin, &owner);
        Self::bump_instance_ttl(&env);
    }

    /// Deposit USDC into the vault, increasing the tracked balance.
    ///
    /// The caller must be the vault owner or a pre-approved depositor.
    /// Transfers `amount` USDC from `caller` to this contract via the
    /// Stellar token transfer hook, then increments the internal balance.
    ///
    /// # Arguments
    /// * `caller` — Address performing the deposit; must authorize.
    /// * `amount` — USDC amount in micro-units; must be >= `min_deposit`.
    ///
    /// # Panics
    /// * `"Contract paused"` — vault is currently paused.
    /// * `"deposit below minimum"` — `amount < min_deposit`.
    /// * `"Not authorized depositor"` — caller is neither owner nor approved.
    ///
    /// # Events
    /// None emitted directly (on-ledger token transfer serves as the record).
    pub fn deposit(env: Env, caller: Address, amount: i128) {
        caller.require_auth();
        
        if env.storage().instance().get::<_, bool>(&DataKey::Paused).unwrap_or(false) {
            panic!("Contract paused");
        }
        
        let min_dep = env.storage().instance().get::<_, i128>(&DataKey::MinDeposit)
            .unwrap_or_else(|| panic!("Min deposit not set"));
            
        Self::require_valid_deposit_amount(amount, min_dep);
        
        let owner = env.storage().instance().get::<_, Address>(&DataKey::Owner)
            .unwrap_or_else(|| panic!("Owner not set"));
            
        if caller != owner {
            let is_allowed = env
                .storage()
                .instance()
                .get::<_, bool>(&DataKey::Depositor(caller.clone()))
                .unwrap_or(false);
            if !is_allowed {
                panic!("unauthorized: only owner or allowed depositor can deposit");
            }
        }
        
        let current_bal = env.storage().instance().get::<_, i128>(&DataKey::Balance).unwrap_or(0);
        
        let new_bal = current_bal.checked_add(amount).unwrap_or_else(|| panic!("Math overflow"));
        
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        
        let token_addr = env.storage().instance().get::<_, Address>(&DataKey::UsdcToken)
            .unwrap_or_else(|| panic!("USDC Token not set"));
            
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&caller, &env.current_contract_address(), &amount);
        Self::bump_instance_ttl(&env);
    }

    /// Deduct USDC from the vault and transfer it to the settlement contract.
    ///
    /// Only the designated authorized caller may invoke this. Transfers USDC
    /// on-ledger to the settlement address and records the deduction via
    /// `settlement.record_deduction(amount, request_id)`.
    ///
    /// # Arguments
    /// * `caller` — Must be the authorized caller; must authorize.
    /// * `amount` — USDC amount in micro-units; must be in `[min_deposit, max_deduct]`.
    /// * `request_id` — Unique identifier passed to settlement for replay tracking.
    ///
    /// # Panics
    /// * `"Not authorized caller"` — caller is not the authorized address.
    /// * `"Contract paused"` — vault is currently paused.
    /// * `"deduct below minimum"` / `"deduct amount exceeds max_deduct"`.
    /// * `"insufficient balance"` — vault balance < `amount`.
    ///
    /// # Events
    /// None emitted directly; settlement contract emits its own record.
    pub fn deduct(env: Env, caller: Address, amount: i128, request_id: u64) {
        caller.require_auth();
        
        let auth_caller = env.storage().instance().get::<_, Address>(&DataKey::AuthorizedCaller)
            .unwrap_or_else(|| panic!("Authorized caller not set"));
            
        if caller != auth_caller {
            panic!("unauthorized: not authorized caller");
        }
        
        if env.storage().instance().get::<_, bool>(&DataKey::Paused).unwrap_or(false) {
            panic!("Contract paused");
        }
        
        let min_dep = env.storage().instance().get::<_, i128>(&DataKey::MinDeposit)
            .unwrap_or_else(|| panic!("Min deposit not set"));
            
        let max_deduct = env.storage().instance().get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap_or_else(|| panic!("Max deduct not set"));
            
        Self::require_valid_deduct_amount(amount, min_dep, max_deduct);
        
        let current_bal = env.storage().instance().get::<_, i128>(&DataKey::Balance).unwrap_or(0);
        
        if current_bal < amount {
            panic!("insufficient balance");
        }
        
        let new_bal = current_bal.checked_sub(amount).unwrap_or_else(|| panic!("Math underflow"));
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        
        // Transfer USDC from vault to settlement on-ledger.
        let usdc_addr = env.storage().instance().get::<_, Address>(&DataKey::UsdcToken)
            .unwrap_or_else(|| panic!("USDC Token not set"));
            
        let usdc = token::Client::new(&env, &usdc_addr);
        
        let settlement_addr = env.storage().instance().get::<_, Address>(&DataKey::Settlement)
            .unwrap_or_else(|| panic!("Settlement not set"));
            
        usdc.transfer(&env.current_contract_address(), &settlement_addr, &amount);
        
        let settlement_client = settlement::Client::new(&env, &settlement_addr);
        settlement_client.record_deduction(&amount, &request_id);
        Self::bump_instance_ttl(&env);
    }

    /// Atomically deduct multiple USDC amounts and transfer the total to settlement.
    ///
    /// Validates all items before any state change, then transfers the summed
    /// amount to the settlement contract and records each deduction individually.
    ///
    /// # Arguments
    /// * `caller` — Must be the authorized caller; must authorize.
    /// * `items` — Vec of `(amount, request_id)` pairs; must not be empty.
    ///
    /// # Panics
    /// * `"Not authorized caller"` — caller is not the authorized address.
    /// * `"Contract paused"` — vault is currently paused.
    /// * Any per-item validation failure (`deduct below minimum`, etc.).
    /// * `"insufficient balance"` — vault balance < total of all amounts.
    /// * `"overflow"` — total amount exceeds `i128`.
    ///
    /// # Events
    /// None emitted directly; settlement records each deduction individually.
    pub fn batch_deduct(env: Env, caller: Address, items: Vec<(i128, u64)>) {
        caller.require_auth();
        
        let auth_caller = env.storage().instance().get::<_, Address>(&DataKey::AuthorizedCaller)
            .unwrap_or_else(|| panic!("Authorized caller not set"));
            
        if caller != auth_caller {
            panic!("unauthorized: not authorized caller");
        }
        
        if env.storage().instance().get::<_, bool>(&DataKey::Paused).unwrap_or(false) {
            panic!("Contract paused");
        }
        
        let min_dep = env.storage().instance().get::<_, i128>(&DataKey::MinDeposit)
            .unwrap_or_else(|| panic!("Min deposit not set"));
            
        let max_deduct = env.storage().instance().get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap_or_else(|| panic!("Max deduct not set"));
            
        let mut total_amount: i128 = 0;
        for item in items.iter() {
            let (amount, _) = item;
            Self::require_valid_deduct_amount(amount, min_dep, max_deduct);
            total_amount = total_amount
                .checked_add(amount)
                .unwrap_or_else(|| panic!("overflow"));
        }
        
        let current_bal = env.storage().instance().get::<_, i128>(&DataKey::Balance).unwrap_or(0);
        
        if current_bal < total_amount {
            panic!("insufficient balance");
        }
        
        let new_bal = current_bal.checked_sub(total_amount).unwrap_or_else(|| panic!("Math underflow"));
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        
        // Transfer total USDC from vault to settlement on-ledger atomically.
        let usdc_addr = env.storage().instance().get::<_, Address>(&DataKey::UsdcToken)
            .unwrap_or_else(|| panic!("USDC Token not set"));
            
        let usdc = token::Client::new(&env, &usdc_addr);
        let settlement_addr = env.storage().instance().get::<_, Address>(&DataKey::Settlement)
            .unwrap_or_else(|| panic!("Settlement not set"));
            
        usdc.transfer(
            &env.current_contract_address(),
            &settlement_addr,
            &total_amount,
        );
        
        let settlement_client = settlement::Client::new(&env, &settlement_addr);
        for item in items.iter() {
            let (amount, request_id) = item;
            settlement_client.receive_payment(
                &env.current_contract_address(),
                &amount,
                &true,
                &None,
                &usdc_addr,
                &(request_id as u32),
            );
        }
        Self::bump_instance_ttl(&env);
    }

    // -----------------------------------------------------------------------
    // View functions — TTL bump on hot read paths (buffer #5 pattern)
    // Bumps instance storage TTL so frequently-read vaults do not archive
    // even when writes are infrequent.
    // -----------------------------------------------------------------------

    /// Modifies the authorized caller (Owner only).
    ///
    /// Performs validation checks identical to `deduct` and returns the predicted
    /// balance after the specified `amount` is deducted.
    ///
    /// # Errors
    /// Returns `VaultError` under the exact same conditions as `deduct`
    /// (e.g., paused state, amount exceeding balance, amount exceeding max deduction limit).
    // pub fn simulate_deduct(
    //     env: Env,
    //     caller: Address,
    //     amount: i128,
    //     request_id: Option<Symbol>,
    // ) -> Result<i128, VaultError> {
    //     Self::require_not_paused(env.clone())?;
    //     caller.require_auth();
    //     if amount <= 0 {
    //         return Err(VaultError::AmountNotPositive);
    //     }
    //     Self::require_authorized_deduct_caller(env.clone(), &caller)?;
    //     let max_d = Self::get_max_deduct(env.clone());
    //     if amount > max_d {
    //         return Err(VaultError::ExceedsMaxDeduct);
    //     }
    //     if let Some(ref rid) = request_id {
    //         Self::require_not_duplicate(&env, rid)?;
    //     }
    //     let meta = Self::get_meta(env.clone())?;
    //     if meta.balance < amount {
    //         return Err(VaultError::InsufficientBalance);
    //     }
    //     let _ = Self::require_settlement(&env)?;
    //     meta.balance
    //         .checked_sub(amount)
    //         .ok_or(VaultError::Overflow)
    // }

    // pub fn simulate_batch_deduct(
    //     env: Env,
    //     caller: Address,
    //     items: Vec<DeductItem>,
    // ) -> Result<i128, VaultError> {
    //     Self::require_not_paused(env.clone())?;
    //     caller.require_auth();
    //     Self::require_authorized_deduct_caller(env.clone(), &caller)?;
    //     let n = items.len();
    //     if n == 0 {
    //         return Err(VaultError::BatchEmpty);
    //     }
    //     if n > MAX_BATCH_SIZE {
    //         return Err(VaultError::BatchTooLarge);
    //     }
    //     let max_d = Self::get_max_deduct(env.clone());
    //     let meta = Self::get_meta(env.clone())?;
    //     let mut running = meta.balance;
    //     let mut seen_in_batch: Vec<Symbol> = Vec::new(&env);
    //     for item in items.iter() {
    //         if item.amount <= 0 {
    //             return Err(VaultError::AmountNotPositive);
    //         }
    //         if item.amount > max_d {
    //             return Err(VaultError::ExceedsMaxDeduct);
    //         }
    //         if running < item.amount {
    //             return Err(VaultError::InsufficientBalance);
    //         }
    //         if let Some(ref rid) = item.request_id {
    //             Self::require_not_duplicate(&env, rid)?;
    //             if seen_in_batch.contains(rid) {
    //                 return Err(VaultError::DuplicateRequestId);
    //             }
    //             seen_in_batch.push_back(rid.clone());
    //         }
    //         running = running.checked_sub(item.amount).ok_or(VaultError::Overflow)?;
    //     }
    //     let _ = Self::require_settlement(&env)?;
    //     Ok(running)
    // }

    // pub fn get_meta(env: Env) -> Result<VaultMeta, VaultError> {
    //     env.storage()
    //         .instance()
    //         .set(&DataKey::Depositor(depositor), &true);
    // }

    /// Set the authorized caller address for deduct operations (owner only).
    ///
    /// Overwrites the previous authorized caller. The new address must be
    /// distinct from the vault's own contract address.
    ///
    /// # Arguments
    /// * `caller` — Must be the vault owner; must authorize.
    ///
    /// # Panics
    /// * `"Not owner"` — caller is not the current owner.
    ///
    /// # Events
    /// None emitted.
    pub fn set_authorized_caller(env: Env, caller: Address) {
        caller.require_auth();
        
        let owner = env.storage().instance().get::<_, Address>(&DataKey::Owner)
            .unwrap_or_else(|| panic!("Owner not set"));
            
        if caller != owner {
            panic!("Not owner");
        }
        env.storage()
            .instance()
            .set(&DataKey::AuthorizedCaller, &new_caller);
    }

    /// Immediately pause the vault (owner only).
    ///
    /// Blocks deposits and deductions while allowing owner withdrawals and
    /// admin distributes for emergency recovery. See the module-level
    /// **Pause Circuit Breaker** documentation for the full allowed/denied matrix.
    ///
    /// # Arguments
    /// * `caller` — Must be the vault owner; must authorize.
    ///
    /// # Panics
    /// * `"Not owner"` — caller is not the current owner.
    ///
    /// # Events
    /// None emitted (use `propose_pause` / `execute_pause` for audited pause).
    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        
        let owner = env.storage().instance().get::<_, Address>(&DataKey::Owner)
            .unwrap_or_else(|| panic!("Owner not set"));
            
        if caller != owner {
            panic!("Not owner");
        }
        
        env.storage().instance().set(&DataKey::Paused, &true);
        Self::bump_instance_ttl(&env);
    }

    /// Immediately unpause the vault (owner only).
    ///
    /// Re-enables deposits and deductions. This is the direct unpause
    /// counterpart to `pause`; for audited unpause use the timelock flow.
    ///
    /// # Arguments
    /// * `caller` — Must be the vault owner; must authorize.
    ///
    /// # Panics
    /// * `"Not owner"` — caller is not the current owner.
    ///
    /// # Events
    /// None emitted.
    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        
        let owner = env.storage().instance().get::<_, Address>(&DataKey::Owner)
            .unwrap_or_else(|| panic!("Owner not set"));
            
        if caller != owner {
            panic!("Not owner");
        }
        
        env.storage().instance().set(&DataKey::Paused, &false);
        Self::bump_instance_ttl(&env);
    }

    /// Return whether the vault is currently paused.
    ///
    /// Defaults to `false` if no pause state has been set (e.g., before init).
    pub fn is_paused(env: Env) -> bool {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }
    /// Return the vault's current tracked USDC balance.
    ///
    /// This is the internal accounting balance, not the on-ledger token
    /// balance. They should match under normal operation.
    pub fn balance(env: Env) -> i128 {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0)
    }
    /// Return the vault owner address.
    ///
    /// The owner controls administrative actions: pause/unpause, configuration
    /// changes, and timelock proposals. Defaults to the address set during `init`.
    pub fn get_owner(env: Env) -> Address {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap_or_else(|| panic!("Owner not set"))
    }

    pub(crate) fn require_owner(env: Env, caller: Address) -> Result<(), VaultError> {
        let owner = Self::get_owner(env);
        if caller != owner {
            return Err(VaultError::Unauthorized);
        }
        Ok(())
    }
    /// Return the USDC token contract address configured for this vault.
    pub fn get_usdc_token(env: Env) -> Address {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .unwrap_or_else(|| panic!("USDC Token not set"))
    }
    /// Return the maximum deduction amount allowed per single `deduct` call.
    ///
    /// Defaults to `i128::MAX` if no cap has been explicitly configured.
    pub fn get_max_deduct(env: Env) -> i128 {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap_or(i128::MAX)
    }

    /// Set the maximum deduction amount allowed per single `deduct` call (owner only).
    ///
    /// # Arguments
    /// * `caller` — Must be the vault owner; must authorize.
    /// * `max_deduct` — New maximum; must be > 0.
    ///
    /// # Panics
    /// * `"Not owner"` — caller is not the current owner.
    /// * `"max_deduct must be positive"` — `max_deduct <= 0`.
    ///
    /// # Events
    /// None emitted.
    pub fn set_max_deduct(env: Env, caller: Address, max_deduct: i128) {
        caller.require_auth();
        
        let owner = env.storage().instance().get::<_, Address>(&DataKey::Owner)
            .unwrap_or_else(|| panic!("Owner not set"));
            
        if caller != owner {
            panic!("Not owner");
        }
        if max_deduct <= 0 {
            panic!("max_deduct must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxDeduct, &max_deduct);
        Self::bump_instance_ttl(&env);
    }

    /// Return the settlement contract address used for recording deductions.
    pub fn get_settlement(env: Env) -> Address {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .unwrap_or_else(|| panic!("Settlement not set"))
    }

    /// Set or replace the settlement contract address (owner only).
    ///
    /// # Arguments
    /// * `caller` — Must be the vault owner; must authorize.
    /// * `settlement` — New settlement contract address.
    ///
    /// # Panics
    /// * `"Not owner"` — caller is not the current owner.
    ///
    /// # Events
    /// None emitted.
    pub fn set_settlement(env: Env, caller: Address, settlement: Address) {
        caller.require_auth();
        
        let owner = env.storage().instance().get::<_, Address>(&DataKey::Owner)
            .unwrap_or_else(|| panic!("Owner not set"));
            
        if caller != owner {
            panic!("Not owner");
        }
        env.storage()
            .instance()
            .set(&DataKey::Settlement, &settlement);
        Self::bump_instance_ttl(&env);
    }
    /// Return the revenue pool address, or `None` if not configured.
    ///
    /// The revenue pool receives yield distributions when set.
    pub fn get_revenue_pool(env: Env) -> Option<Address> {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::RevenuePool)
    }

    /// Return the capability bitmap for this contract version.
    ///
    /// Each set bit represents a feature that this contract supports.  Bits are
    /// stable — a position once assigned is never reused for a different feature.
    /// Reserved bits (18–63) are always zero.
    ///
    /// No authentication required; this is a pure view function.
    ///
    /// # Example
    /// ```ignore
    /// let caps = client.capabilities();
    /// let has_batch = caps & capabilities::CAP_BATCH_DEDUCT != 0;
    /// ```
    pub fn capabilities(env: Env) -> u64 {
        capabilities::capabilities(&env)
    }

    // =====================================================================
    //  Escape-hatch admin timelock (Issue #482)
    //
    //  Critical admin actions — `pause`, `upgrade`, and `sweep` — must
    //  propose before they can be executed. The proposal records a
    //  `proposed_at` timestamp and an `execute_after` deadline derived
    //  from the configured `TimelockWindow` (default 48h). Any admin may
    //  cancel an outstanding proposal at any time.
    //
    //  All three action slots are independent so multiple escape-hatch
    //  proposals can be staged in parallel.
    // =====================================================================

    /// Configure the timelock window length (admin only).
    ///
    /// The window sets the minimum delay between proposing and executing a
    /// critical admin action. Default on deployment is **48 h** (`172_800` s).
    ///
    /// # Bounds
    /// - Minimum: [`timelock::MIN_TIMELOCK_SECONDS`] (1 h).
    /// - Maximum: [`timelock::MAX_TIMELOCK_SECONDS`] (30 d).
    /// - Default: [`timelock::DEFAULT_TIMELOCK_SECONDS`] (48 h).
    ///
    /// Changing the window does **not** retroactively shorten existing
    /// proposals — each proposal carries its own `execute_after` deadline.
    ///
    /// # Authorization
    /// The caller must be the current admin. `require_auth` runs first so
    /// misconfigured callers can never silently poison the configuration.
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not admin.
    /// - `VaultError::InvalidTimelockWindow` if `window < MIN` or `window > MAX`.
    pub fn set_timelock_window(env: Env, caller: Address, window: u64) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        if !(timelock::MIN_TIMELOCK_SECONDS..=timelock::MAX_TIMELOCK_SECONDS).contains(&window) {
            return Err(VaultError::InvalidTimelockWindow);
        }
        timelock::set_timelock_window(&env, window);
        env.events().publish(
            (events::event_timelock_window_changed(&env), caller.clone()),
            (timelock::get_timelock_window(&env), window),
        );
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Return the configured timelock window length in seconds.
    ///
    /// Defaults to [`timelock::DEFAULT_TIMELOCK_SECONDS`] (48 h) when no
    /// window has been explicitly configured.
    pub fn get_timelock_window(env: Env) -> u64 {
        Self::bump_instance_ttl(&env);
        timelock::get_timelock_window(&env)
    }

    /// Return the pending pause proposal, if any.
    pub fn get_pending_pause(env: Env) -> Option<timelock::PendingPause> {
        Self::bump_instance_ttl(&env);
        let result = timelock::get_pending_pause(&env);
        if result.is_some() {
            Self::bump_persistent_key(&env, &StorageKey::PendingPause);
        }
        result
    }

    /// Return the pending upgrade proposal, if any.
    pub fn get_pending_upgrade(env: Env) -> Option<timelock::PendingUpgrade> {
        Self::bump_instance_ttl(&env);
        let result = timelock::get_pending_upgrade(&env);
        if result.is_some() {
            Self::bump_persistent_key(&env, &StorageKey::PendingUpgrade);
        }
        result
    }

    /// Return the pending sweep proposal, if any.
    pub fn get_pending_sweep(env: Env) -> Option<timelock::PendingSweep> {
        Self::bump_instance_ttl(&env);
        let result = timelock::get_pending_sweep(&env);
        if result.is_some() {
            Self::bump_persistent_key(&env, &StorageKey::PendingSweep);
        }
        result
    }

    /// Require the caller to be the current admin.
    fn require_admin(env: &Env, caller: &Address) -> Result<(), VaultError> {
        caller.require_auth();
        let admin = env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::Admin)
            .ok_or(VaultError::NotInitialized)?;
        if caller != &admin {
            return Err(VaultError::Unauthorized);
        }
        Ok(())
    }

    /// Return the current admin address.
    ///
    /// Defaults to the owner when no admin has been explicitly set.
    ///
    /// # Errors
    /// - `VaultError::NotInitialized` if the contract has not been initialized.
    pub fn get_admin(env: Env) -> Result<Address, VaultError> {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, Address>(&StorageKey::Admin)
            .ok_or(VaultError::NotInitialized)
    }

    /// Initiate a two-step admin transfer (current admin only).
    pub fn set_admin(
        env: Env,
        caller: Address,
        new_admin: Address,
    ) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Accept a pending admin transfer (pending admin only).
    pub fn accept_admin(env: Env) -> Result<(), VaultError> {
        let new_admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .ok_or(VaultError::NoAdminTransferPending)?;
        new_admin.require_auth();
        env.storage().instance().set(&StorageKey::Admin, &new_admin);
        env.storage().instance().remove(&StorageKey::PendingAdmin);
        Ok(())
    }

    /// Transfer ownership (two-step) — initiate.
    pub fn transfer_ownership(env: Env, caller: Address, new_owner: Address) {
        Self::require_owner_auth(&env, &caller);
        env.storage()
            .instance()
            .set(&DataKey::PendingOwner, &new_owner);
        Self::bump_instance(&env);
        env.events().publish(
            (events::event_ownership_nominated(&env), caller),
            new_owner,
        );
    }

    /// Accept pending ownership (new owner only).
    pub fn accept_ownership(env: Env) {
        let new_owner: Address = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::PendingOwner)
            .expect("no pending ownership transfer");
        new_owner.require_auth();
        env.storage().instance().set(&DataKey::Owner, &new_owner);
        env.storage().instance().remove(&DataKey::PendingOwner);
        Self::bump_instance(&env);
        env.events().publish(
            (events::event_ownership_accepted(&env), new_owner),
            (),
        );
    }

    // -----------------------------------------------------------------------
    // Idempotency key pruning
    // -----------------------------------------------------------------------

    /// Owner-only: remove processed request markers to reclaim storage.
    pub fn prune_processed_requests(
        env: Env,
        caller: Address,
        ids: Vec<Symbol>,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;
        for id in ids.iter() {
            let key = StorageKey::ProcessedRequest(id.clone());
            if env.storage().persistent().has(&key) {
                env.storage().persistent().remove(&key);
                env.events().publish(
                    (events::event_request_id_pruned(&env), caller.clone()),
                    id.clone(),
                );
            }
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Timelock window
    // -----------------------------------------------------------------------

    pub fn set_timelock_window(
        env: Env,
        caller: Address,
        window: u64,
    ) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        if window < timelock::MIN_TIMELOCK_SECONDS
            || window > timelock::MAX_TIMELOCK_SECONDS
        {
            return Err(VaultError::InvalidTimelockWindow);
        }
        timelock::set_timelock_window(&env, window);
        env.events().publish(
            (events::event_timelock_window_changed(&env), caller),
            (timelock::get_timelock_window(&env), window),
        );
        Ok(())
    }

    pub fn get_timelock_window(env: Env) -> u64 {
        timelock::get_timelock_window(&env)
    }

    pub fn get_pending_pause(env: Env) -> Option<timelock::PendingPause> {
        timelock::get_pending_pause(&env)
    }

    pub fn get_pending_upgrade(env: Env) -> Option<timelock::PendingUpgrade> {
        timelock::get_pending_upgrade(&env)
    }

    pub fn get_pending_sweep(env: Env) -> Option<timelock::PendingSweep> {
        timelock::get_pending_sweep(&env)
    }

    // -----------------------------------------------------------------------
    // Propose/Execute/Cancel — pause
    // -----------------------------------------------------------------------

    pub fn propose_pause(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let proposed_at = env.ledger().timestamp();
        let window = timelock::get_timelock_window(&env);
        let execute_after = timelock::saturating_deadline(proposed_at, window)
            .ok_or(VaultError::TimelockOverflow)?;
        timelock::set_pending_pause(
            &env,
            &timelock::PendingPause { proposed_at, execute_after },
        );
        env.events().publish(
            (events::event_pause_proposed(&env), caller),
            (proposed_at, execute_after),
        );
        env.events().publish(
            (events::event_pause_proposed(&env), caller),
            (proposed_at, execute_after),
        );
        env.events().publish(
            (events::event_pause_proposed(&env), caller),
            (proposed_at, execute_after),
        );
        Ok(())
    }

    pub fn execute_pause(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let proposal = timelock::get_pending_pause(&env).ok_or(VaultError::ProposalNotFound)?;
        if env.ledger().timestamp() < proposal.execute_after {
            return Err(VaultError::TimelockNotExpired);
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        timelock::clear_pending_pause(&env);
        env.events().publish(
            (events::event_pause_executed(&env), caller.clone()),
            env.ledger().timestamp(),
        );
        env.events()
            .publish((events::event_vault_paused(&env), caller), ());
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    pub fn cancel_pause(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let existing = timelock::get_pending_pause(&env);
        timelock::clear_pending_pause(&env);
        env.events().publish(
            (events::event_pause_cancelled(&env), caller.clone()),
            existing.is_some(),
        );
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Propose/Execute/Cancel — upgrade
    // -----------------------------------------------------------------------

    pub fn propose_upgrade(
        env: Env,
        caller: Address,
        new_wasm_hash: BytesN<32>,
    ) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let proposed_at = env.ledger().timestamp();
        let window = timelock::get_timelock_window(&env);
        let execute_after = timelock::saturating_deadline(proposed_at, window)
            .ok_or(VaultError::TimelockOverflow)?;
        timelock::set_pending_upgrade(
            &env,
            &timelock::PendingUpgrade {
                wasm_hash: new_wasm_hash.clone(),
                proposed_at,
                execute_after,
            },
        );
        env.events().publish(
            (events::event_upgrade_proposed(&env), caller),
            (new_wasm_hash, proposed_at, execute_after),
        );
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    pub fn execute_upgrade(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let proposal = timelock::get_pending_upgrade(&env).ok_or(VaultError::ProposalNotFound)?;
        if env.ledger().timestamp() < proposal.execute_after {
            return Err(VaultError::TimelockNotExpired);
        }
        let wasm_hash = proposal.wasm_hash.clone();
        let _admin = Self::get_admin(env.clone())?;
        env.deployer()
            .update_current_contract_wasm(wasm_hash.clone());
        env.storage()
            .instance()
            .set(&StorageKey::ContractVersion, &wasm_hash);
        timelock::clear_pending_upgrade(&env);
        env.events().publish(
            (events::event_upgrade_executed(&env), caller.clone()),
            env.ledger().timestamp(),
        );
        env.events()
            .publish((events::event_upgraded(&env), caller), wasm_hash);
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    pub fn cancel_upgrade(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let existing = timelock::get_pending_upgrade(&env);
        timelock::clear_pending_upgrade(&env);
        env.events().publish(
            (events::event_upgrade_cancelled(&env), caller.clone()),
            existing.is_some(),
        );
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Propose sweeping (distributing) on-ledger USDC surplus to a recipient (admin only, timelocked).
    ///
    /// After the configured window elapses, `execute_sweep` transfers the
    /// funds and emits the standard `distribute` event. Re-proposing
    /// replaces the recipient+amount **and restarts the timer**.
    ///
    /// `amount` is checked against the vault's configured `min_deposit` floor,
    /// the same per-call minimum enforced on `deposit`/`deduct`/`batch_deduct`.
    /// This closes the one remaining value-moving path that previously had no
    /// floor, so sub-unit/dust sweeps are rejected consistently everywhere the
    /// vault moves USDC.
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` if caller is not admin.
    /// - `VaultError::AmountNotPositive` if `amount <= 0`.
    /// - `VaultError::BelowMinTransferAmount` if `amount` is below the vault's
    ///   configured minimum transfer unit (`min_deposit`).
    /// - `VaultError::TimelockOverflow` if `proposed_at + window` overflows.
    pub fn propose_sweep(
        env: Env,
        caller: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        if amount <= 0 {
            return Err(VaultError::AmountNotPositive);
        }
        let min_amount: i128 = env
            .storage()
            .instance()
            .get(&DataKey::MinDeposit)
            .ok_or(VaultError::NotInitialized)?;
        if amount < min_amount {
            return Err(VaultError::BelowMinTransferAmount);
        }
        let proposed_at = env.ledger().timestamp();
        let window = timelock::get_timelock_window(&env);
        let execute_after = timelock::saturating_deadline(proposed_at, window)
            .ok_or(VaultError::TimelockOverflow)?;
        timelock::set_pending_sweep(
            &env,
            &timelock::PendingSweep { to: to.clone(), amount, proposed_at, execute_after },
        );
        env.events().publish(
            (events::event_sweep_proposed(&env), caller),
            (to, amount, proposed_at, execute_after),
        );
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    pub fn execute_sweep(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let proposal = timelock::get_pending_sweep(&env).ok_or(VaultError::ProposalNotFound)?;
        if env.ledger().timestamp() < proposal.execute_after {
            return Err(VaultError::TimelockNotExpired);
        }
        let usdc_addr: Address = env
            .storage()
            .instance()
            .get(&DataKey::UsdcToken)
            .ok_or(VaultError::NotInitialized)?;
        let usdc = token::Client::new(&env, &usdc_addr);
        if usdc.balance(&env.current_contract_address()) < proposal.amount {
            return Err(VaultError::InsufficientBalance);
        }
        usdc.transfer(
            &env.current_contract_address(),
            &proposal.to,
            &proposal.amount,
        );
        let executed_at = env.ledger().timestamp();
        timelock::clear_pending_sweep(&env);
        env.events().publish(
            (events::event_sweep_executed(&env), caller),
            (proposal.to.clone(), proposal.amount, executed_at),
        );
        env.events().publish(
            (events::event_distribute(&env), proposal.to),
            proposal.amount,
        );
        timelock::clear_pending_sweep(&env);
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    pub fn cancel_sweep(env: Env, caller: Address) -> Result<(), VaultError> {
        Self::require_admin(&env, &caller)?;
        let existing = timelock::get_pending_sweep(&env);
        timelock::clear_pending_sweep(&env);
        env.events().publish(
            (events::event_sweep_cancelled(&env), caller.clone()),
            existing.is_some(),
        );
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Garbage-collect processed request markers from persistent storage.
    ///
    /// Over time, idempotency markers accumulate and consume storage. This
    /// allows the owner to explicitly prune a batch of stale `request_id`
    /// entries to reclaim storage budget. Only existing markers are removed;
    /// absent IDs are silently skipped.
    ///
    /// # Arguments
    /// * `caller` — Must be the vault owner; must authorize.
    /// * `ids` — Vec of `Symbol` request IDs to remove.
    ///
    /// # Errors
    /// * `VaultError::Unauthorized` — caller is not the owner.
    ///
    /// # Events
    /// Emits `request_id_pruned` for each successfully removed marker.
    pub fn prune_processed_requests(
        env: Env,
        caller: Address,
        ids: Vec<Symbol>,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;

        for id in ids.iter() {
            let key = StorageKey::ProcessedRequest(id.clone());
            if env.storage().persistent().has(&key) {
                env.storage().persistent().remove(&key);
                env.events().publish(
                    (events::event_request_id_pruned(&env), caller.clone()),
                    id.clone(),
                );
            }
        }

        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Return whether `caller` is an approved depositor.
    ///
    /// Depositors may call `deposit` even when they are not the vault owner.
    /// Defaults to `false` for addresses not explicitly approved.
    pub fn is_authorized_depositor(env: Env, caller: Address) -> bool {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Depositor(caller))
            .unwrap_or(false)
    }

    /// Set or update the reserve cap for a token (owner only).
    ///
    /// The reserve cap is the maximum total balance the vault may hold for
    /// `token`.  Any `deposit` call that would push the balance beyond `cap`
    /// is rejected with [`VaultError::ExceedsReserveCap`].
    ///
    /// Pass `i128::MAX` to remove the effective cap (restore unlimited deposits).
    ///
    /// # Parameters
    /// - `caller` — must be the vault owner.
    /// - `token` — token contract address the cap applies to.
    /// - `cap` — maximum balance in token stroops; must be > 0.
    ///
    /// # Errors
    /// - [`VaultError::Unauthorized`] — `caller` is not the owner.
    /// - [`VaultError::AmountNotPositive`] — `cap <= 0`.
    pub fn set_reserve_cap(
        env: Env,
        caller: Address,
        token: Address,
        cap: i128,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;
        if cap <= 0 {
            return Err(VaultError::AmountNotPositive);
        }
        let prev = limits::set(&env, &token, cap);
        env.events().publish(
            (events::event_reserve_cap_set(&env), caller, token),
            (prev, cap),
        );
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Return the reserve cap for `token`.
    ///
    /// Returns `i128::MAX` when no cap has been configured (effectively unlimited).
    pub fn get_reserve_cap(env: Env, token: Address) -> i128 {
        Self::bump_instance_ttl(&env);
        limits::get(&env, &token)
    }
}

// ---------------------------------------------------------------------------
// Module declarations
// ---------------------------------------------------------------------------

pub mod capabilities;
mod cold_storage;
pub mod events;
pub mod limits;
pub mod rate_limit;
pub mod timelock;

// #[cfg(test)]
// #[path = "../proofs/deduct.rs"]
// mod deduct_proofs;

// ---------------------------------------------------------------------------
// Test modules
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_views;

#[cfg(test)]
mod test_idempotency;

#[cfg(test)]
mod test_error_codes;

#[cfg(test)]
mod test_reentrancy;

#[cfg(test)]
mod test_sweep_idle_balance;

#[cfg(test)]
mod test_access_control_matrix;

#[cfg(test)]
mod rustdoc_tests {
    #[test]
    fn every_public_fn_has_rustdoc() {
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

// #[cfg(test)]
// mod test_gas_budget;
// #[cfg(test)]
// mod test_rate_limit;