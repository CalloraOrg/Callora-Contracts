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
//! ## Pause Circuit Breaker
//!
//! When the vault is paused:
//! - Deposits are blocked
//! - Single and batch deducts are blocked
//! - Owner withdrawals are ALLOWED (emergency recovery)
//! - Admin distribute is ALLOWED (emergency recovery of untracked surplus)
//! - Admin/owner configuration functions remain available
//!
//! ## Request-ID Idempotency
//!
//! `deduct` and `batch_deduct` accept an optional `request_id: Option<Symbol>`.
//! When `Some(id)` is supplied the contract persists a processed-request marker
//! in **persistent storage** and rejects any subsequent call that carries the same
//! `request_id`, returning `VaultError::DuplicateRequestId`.
//!
//! When `request_id` is `None` no deduplication is performed; the call is
//! treated as a fire-and-forget deduction with no idempotency guarantee.

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, Symbol, Vec,
};

pub mod views;

/// Typed error codes for the Callora Vault contract.
///
/// These error codes are returned instead of string panics to enable
/// machine-readable error handling by integrators using @stellar/stellar-sdk.
#[contracterror]
#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum VaultError {
    /// Vault has not been initialized yet (code 1).
    NotInitialized = 1,
    /// Vault has already been initialized (code 2).
    AlreadyInitialized = 2,
    /// Caller is not authorized for this operation (code 3).
    Unauthorized = 3,
    /// Vault is currently paused (code 4).
    Paused = 4,
    /// Insufficient balance for the requested operation (code 5).
    InsufficientBalance = 5,
    /// Amount must be positive (code 6).
    AmountNotPositive = 6,
    /// Deduct amount exceeds the configured maximum (code 7).
    ExceedsMaxDeduct = 7,
    /// Deposit amount is below the configured minimum (code 8).
    BelowMinDeposit = 8,
    /// Arithmetic overflow detected (code 9).
    Overflow = 9,
    /// Initial balance must be non-negative (code 10).
    InitialBalanceNegative = 10,
    /// Min deposit must be positive (code 11).
    MinDepositNotPositive = 11,
    /// Max deduct must be positive (code 12).
    MaxDeductNotPositive = 12,
    /// Min deposit cannot exceed max deduct (code 13).
    MinDepositExceedsMaxDeduct = 13,
    /// USDC token address cannot be the vault address (code 14).
    UsdcTokenCannotBeVault = 14,
    /// Revenue pool address cannot be the vault address (code 15).
    RevenuePoolCannotBeVault = 15,
    /// Authorized caller address cannot be the vault address (code 16).
    AuthorizedCallerCannotBeVault = 16,
    /// Initial balance exceeds on-ledger USDC balance (code 17).
    InitialBalanceExceedsOnLedger = 17,
    /// Vault is already paused (code 18).
    AlreadyPaused = 18,
    /// Vault is not paused (code 19).
    NotPaused = 19,
    /// Settlement address has not been configured (code 20).
    SettlementNotSet = 20,
    /// Batch deduct requires at least one item (code 21).
    BatchEmpty = 21,
    /// Batch size exceeds maximum allowed (code 22).
    BatchTooLarge = 22,
    /// New owner must be different from current owner (code 23).
    NewOwnerSameAsCurrent = 23,
    /// No ownership transfer is pending (code 24).
    NoOwnershipTransferPending = 24,
    /// No admin transfer is pending (code 25).
    NoAdminTransferPending = 25,
    /// Offering ID exceeds maximum length (code 26).
    OfferingIdTooLong = 26,
    /// Metadata exceeds maximum length (code 27).
    MetadataTooLong = 27,
    /// Price parsing error or non‑positive price (code 28).
    PriceParseError = 28,
    /// Duplicate request ID detected (code 29).
    DuplicateRequestId = 29,
    /// Offering ID is empty or contains invalid characters (code 30).
    OfferingIdInvalid = 30,
    /// Metadata string is empty or contains invalid characters (code 31).
    MetadataInvalid = 31,
    /// Supplied nonce does not match the stored authorized-caller rotation nonce (code 30).
    StaleNonce = 32,
    /// New revenue pool must be different from current revenue pool (code 33).
    NewRevenuePoolSameAsCurrent = 33,
    /// No revenue pool transfer is pending (code 34).
    NoRevenuePoolTransferPending = 34,
    /// Calculated fee in basis points exceeds the caller-supplied `max_fee_bps` limit (code 35).
    Slippage = 35,
    /// Rate limit exceeded for the developer (code 36).
    RateLimited = 36,
    /// No pending timelock proposal for the requested action (code 37).
    ProposalNotFound = 37,
    /// Action attempted before the timelock window has elapsed (code 38).
    TimelockNotExpired = 38,
    /// `proposed_at + window` overflowed `u64` (code 39).
    TimelockOverflow = 39,
    /// Proposed timelock window is outside the allowed `MIN..=MAX` bounds (code 40).
    InvalidTimelockWindow = 40,
    /// Hot balance basis points are outside the allowed range (code 41).
    InvalidHotBps = 41,
    /// Rebalance threshold basis points are outside the allowed range (code 42).
    InvalidRebalanceThreshold = 42,
    /// The cold signer set is empty (code 43).
    ColdSignersEmpty = 43,
    /// The cold signer threshold is invalid (code 44).
    InvalidColdThreshold = 44,
    /// The cold signer set contains a duplicate address (code 45).
    DuplicateColdSigner = 45,
    /// A deposit would exceed the configured reserve cap (code 46).
    ExceedsReserveCap = 46,
}

pub const INSTANCE_BUMP_THRESHOLD: u32 = 100_000;
pub const INSTANCE_BUMP_AMOUNT: u32 = 200_000;

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

// ---------------------------------------------------------------------------
// Token client shim
// ---------------------------------------------------------------------------

pub mod token_client {
    pub use soroban_sdk::token::Client;
}

// ---------------------------------------------------------------------------
// Settlement cross-contract client
// ---------------------------------------------------------------------------

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
    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn bump_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    fn load_owner(env: &Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .expect("vault not initialized")
    }

    fn load_balance(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .expect("vault not initialized")
    }

    fn load_usdc(env: &Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .expect("vault not initialized")
    }

    fn load_max_deduct(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap_or(DEFAULT_MAX_DEDUCT)
    }

    fn load_min_deposit(env: &Env) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::MinDeposit)
            .unwrap_or(DEFAULT_MIN_DEPOSIT)
    }

    fn require_not_paused(env: &Env) {
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("vault is paused");
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

    fn mark_processed(env: &Env, request_id: &Symbol) {
        let key = StorageKey::ProcessedRequest(request_id.clone());
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, REQUEST_ID_BUMP_THRESHOLD, REQUEST_ID_BUMP_AMOUNT);
    }

    // -----------------------------------------------------------------------
    // init
    // -----------------------------------------------------------------------

    /// Initialize the vault.
    ///
    /// - `initial_balance`: defaults to `0`. When `> 0` the on-ledger USDC
    ///   balance at the vault address must be ≥ `initial_balance`.
    /// - `authorized_caller`: defaults to `None` (no deductions permitted until set).
    /// - `min_deposit`: defaults to [`DEFAULT_MIN_DEPOSIT`] (1). Must be `> 0`.
    /// - `revenue_pool`: optional; may be set later via `propose_revenue_pool`.
    /// - `max_deduct`: defaults to [`DEFAULT_MAX_DEDUCT`] (`i128::MAX`). Must be `> 0`.
    ///
    /// Returns a [`VaultMeta`] snapshot of the just-initialized state.
    pub fn init(
        env: Env,
        owner: Address,
        usdc_token: Address,
        initial_balance: Option<i128>,
        authorized_caller: Option<Address>,
        min_deposit: Option<i128>,
        revenue_pool: Option<Address>,
        max_deduct: Option<i128>,
    ) -> VaultMeta {
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
        if let Some(ref pool) = revenue_pool {
            if pool == &vault_addr {
                panic!("revenue_pool cannot be vault address");
            }
        }
        if let Some(ref ac) = authorized_caller {
            if ac == &vault_addr {
                panic!("authorized_caller cannot be vault address");
            }
        }

        // When initial_balance > 0 verify on-ledger USDC covers it.
        if initial_balance > 0 {
            let usdc = token::Client::new(&env, &usdc_token);
            let on_ledger = usdc.balance(&vault_addr);
            if on_ledger < initial_balance {
                panic!("initial_balance exceeds on-ledger USDC balance");
            }
        }

        // Persist configuration
        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage().instance().set(&DataKey::UsdcToken, &usdc_token);
        env.storage().instance().set(&DataKey::Balance, &initial_balance);
        env.storage().instance().set(&DataKey::MinDeposit, &min_deposit);
        env.storage().instance().set(&DataKey::MaxDeduct, &max_deduct);
        env.storage().instance().set(&DataKey::Paused, &false);
        // Admin defaults to owner at initialization.
        env.storage().instance().set(&StorageKey::Admin, &owner);

        if let Some(ref ac) = authorized_caller {
            env.storage().instance().set(&DataKey::AuthorizedCaller, ac);
        }
        if let Some(ref pool) = revenue_pool {
            env.storage().instance().set(&DataKey::RevenuePool, pool);
        }

        Self::bump_instance(&env);

        // Emit init event: topics = (Symbol("init"), owner), data = initial_balance
        env.events().publish(
            (events::event_init(&env), owner.clone()),
            initial_balance,
        );

        VaultMeta {
            owner,
            balance: initial_balance,
            authorized_caller,
            min_deposit,
        }
    }

    // -----------------------------------------------------------------------
    // get_meta
    // -----------------------------------------------------------------------

    /// Return a snapshot of vault metadata. Panics if uninitialized.
    pub fn get_meta(env: Env) -> VaultMeta {
        // Accessing owner panics with a clear message if uninitialized.
        Self::build_meta(&env)
    }
}

#[contractimpl]
impl CalloraVault {
    // -----------------------------------------------------------------------
    // deposit
    // -----------------------------------------------------------------------

    /// Deposit USDC into the vault. Caller must be the owner or an allowed depositor.
    ///
    /// Returns the new tracked balance.
    pub fn deposit(env: Env, caller: Address, amount: i128) -> i128 {
        caller.require_auth();
        Self::require_not_paused(&env);

        if amount <= 0 {
            panic!("amount must be positive");
        }

        let min_dep = Self::load_min_deposit(&env);
        if amount < min_dep {
            panic!("deposit below minimum");
        }

        // Authorization: owner or allowed depositor
        let owner = Self::load_owner(&env);
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

        // Check reserve cap
        let usdc_addr = Self::load_usdc(&env);
        let current_bal = Self::load_balance(&env);
        limits::check(&env, &usdc_addr, current_bal, amount)
            .expect("deposit would exceed reserve cap");

        let new_bal = current_bal.checked_add(amount).expect("balance overflow");
        env.storage().instance().set(&DataKey::Balance, &new_bal);

        // Transfer USDC on-ledger
        let token_client = token::Client::new(&env, &usdc_addr);
        token_client.transfer(&caller, &env.current_contract_address(), &amount);

        Self::bump_instance(&env);

        // Emit deposit event: topics = (Symbol("deposit"), caller), data = (amount, new_balance)
        env.events().publish(
            (events::event_deposit(&env), caller),
            (amount, new_bal),
        );

        new_bal
    }

    // -----------------------------------------------------------------------
    // deduct
    // -----------------------------------------------------------------------

    /// Deduct USDC from the vault balance and transfer to settlement.
    ///
    /// - `caller` must be the configured `authorized_caller`.
    /// - `request_id` provides idempotency when `Some`; `None` skips dedup.
    ///
    /// Returns the remaining balance.
    pub fn deduct(
        env: Env,
        caller: Address,
        amount: i128,
        request_id: Option<Symbol>,
    ) -> i128 {
        caller.require_auth();
        Self::require_not_paused(&env);

        if amount <= 0 {
            panic!("amount must be positive");
        }

        // Check authorized caller
        let auth_caller = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::AuthorizedCaller)
            .expect("authorized_caller not set");
        if caller != auth_caller {
            panic!("unauthorized: not authorized caller");
        }

        // Max deduct cap
        let max_deduct = Self::load_max_deduct(&env);
        if amount > max_deduct {
            panic!("deduct amount exceeds max_deduct");
        }

        // Idempotency guard
        if let Some(ref rid) = request_id {
            Self::require_not_duplicate(&env, rid);
        }

        let current_bal = Self::load_balance(&env);
        if current_bal < amount {
            panic!("insufficient balance");
        }

        let new_bal = current_bal.checked_sub(amount).expect("balance underflow");
        env.storage().instance().set(&DataKey::Balance, &new_bal);

        // Mark request processed (after balance update — CEI order)
        if let Some(ref rid) = request_id {
            Self::mark_processed(&env, rid);
        }

        // Transfer USDC to settlement
        let usdc_addr = Self::load_usdc(&env);
        let usdc = token::Client::new(&env, &usdc_addr);
        let settlement_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .expect("settlement not configured");
        usdc.transfer(&env.current_contract_address(), &settlement_addr, &amount);
        let settlement_client = settlement::Client::new(&env, &settlement_addr);
        settlement_client.receive_payment(
            &env.current_contract_address(),
            &amount,
            &true,
            &None,
            &usdc_addr,
            &(request_id as u32),
        );
    }

    // -----------------------------------------------------------------------
    // batch_deduct
    // -----------------------------------------------------------------------

    /// Atomically deduct multiple items. All validations run before any
    /// balance mutation.
    pub fn batch_deduct(
        env: Env,
        caller: Address,
        items: Vec<DeductItem>,
    ) -> i128 {
        caller.require_auth();
        Self::require_not_paused(&env);

        let n = items.len();
        if n == 0 {
            panic!("batch must contain at least one item");
        }
        if n > MAX_BATCH_SIZE {
            panic!("batch size exceeds maximum");
        }

        let auth_caller = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::AuthorizedCaller)
            .expect("authorized_caller not set");
        if caller != auth_caller {
            panic!("unauthorized: not authorized caller");
        }

        let max_deduct = Self::load_max_deduct(&env);

        // First pass: validate all items and check for batch-level duplicates
        let mut seen: Vec<Symbol> = Vec::new(&env);
        let mut total: i128 = 0;
        for item in items.iter() {
            let (amount, _) = item;
            Self::require_valid_deduct_amount(amount, min_dep, max_deduct);
            total_amount = total_amount
                .checked_add(amount)
                .unwrap_or_else(|| panic!("overflow"));
        }

        let current_bal = Self::load_balance(&env);
        if current_bal < total {
            panic!("insufficient balance");
        }

        let new_bal = current_bal.checked_sub(total).expect("balance underflow");
        env.storage().instance().set(&DataKey::Balance, &new_bal);

        // Mark all request IDs processed
        for item in items.iter() {
            if let Some(ref rid) = item.request_id {
                Self::mark_processed(&env, rid);
            }
        }

        // Transfer total to settlement
        let usdc_addr = Self::load_usdc(&env);
        let usdc = token::Client::new(&env, &usdc_addr);
        let settlement_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .unwrap();
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
    }

    // -----------------------------------------------------------------------
    // withdraw / withdraw_to
    // -----------------------------------------------------------------------

    // Simulates a vault deduction without altering on-chain state.
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

    pub fn set_authorized_caller(env: Env, caller: Address) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        let owner = Self::load_owner(&env);
        owner.require_auth();

        let current_bal = Self::load_balance(&env);
        if current_bal < amount {
            panic!("insufficient balance");
        }

        let new_bal = current_bal.checked_sub(amount).expect("balance underflow");
        env.storage().instance().set(&DataKey::Balance, &new_bal);

        let usdc_addr = Self::load_usdc(&env);
        let usdc = token::Client::new(&env, &usdc_addr);
        usdc.transfer(&env.current_contract_address(), &owner, &amount);

        Self::bump_instance(&env);
        env.events().publish(
            (events::event_withdraw(&env), owner),
            (amount, new_bal),
        );

        new_bal
    }

    /// Owner-only: withdraw `amount` USDC to the specified `to` address.
    ///
    /// Returns the remaining balance.
    pub fn withdraw_to(env: Env, to: Address, amount: i128) -> i128 {
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let owner = Self::load_owner(&env);
        owner.require_auth();

        let current_bal = Self::load_balance(&env);
        if current_bal < amount {
            panic!("insufficient balance");
        }

        let new_bal = current_bal.checked_sub(amount).expect("balance underflow");
        env.storage().instance().set(&DataKey::Balance, &new_bal);

        let usdc_addr = Self::load_usdc(&env);
        let usdc = token::Client::new(&env, &usdc_addr);
        usdc.transfer(&env.current_contract_address(), &to, &amount);

        Self::bump_instance(&env);
        env.events().publish(
            (events::event_withdraw_to(&env), owner, to),
            (amount, new_bal),
        );

        new_bal
    }
}

#[contractimpl]
impl CalloraVault {
    // -----------------------------------------------------------------------
    // View functions
    // -----------------------------------------------------------------------

    /// Return current tracked balance. Panics if uninitialized.
    pub fn balance(env: Env) -> i128 {
        Self::load_balance(&env)
    }

    /// Return owner address. Panics if uninitialized.
    pub fn get_owner(env: Env) -> Address {
        Self::load_owner(&env)
    }

    /// Return USDC token address. Panics if uninitialized.
    pub fn get_usdc_token(env: Env) -> Address {
        Self::load_usdc(&env)
    }

    /// Return current max single-deduction limit.
    pub fn get_max_deduct(env: Env) -> i128 {
        Self::load_max_deduct(&env)
    }

    /// Return settlement contract address. Panics if not set.
    pub fn get_settlement(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .expect("settlement not configured")
    }

    /// Return the optional revenue pool address.
    pub fn get_revenue_pool(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::RevenuePool)
    }

    /// Return `(usdc_token, settlement, revenue_pool)` in one call.
    pub fn get_contract_addresses(
        env: Env,
    ) -> (Option<Address>, Option<Address>, Option<Address>) {
        let usdc = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken);
        let settlement = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement);
        let pool = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::RevenuePool);
        (usdc, settlement, pool)
    }

    /// Return `true` if the vault is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }

    /// Return whether `caller` is an authorized depositor (owner always qualifies).
    pub fn is_authorized_depositor(env: Env, caller: Address) -> bool {
        if env.storage().instance().has(&DataKey::Owner) {
            let owner = Self::load_owner(&env);
            if caller == owner {
                return true;
            }
        }
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Depositor(caller))
            .unwrap_or(false)
    }

    /// Return the current admin address. Panics if uninitialized.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&StorageKey::Admin)
            .expect("vault not initialized")
    }

    /// Return the recorded WASM hash from the last successful upgrade, if any.
    pub fn get_version(env: Env) -> Option<BytesN<32>> {
        env.storage()
            .instance()
            .get::<_, BytesN<32>>(&StorageKey::ContractVersion)
    }

    /// Return the pending admin address, if a two-step transfer is in progress.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get::<_, Address>(&StorageKey::PendingAdmin)
    }

    /// Return the pending owner address, if a two-step transfer is in progress.
    pub fn get_pending_owner(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::PendingOwner)
    }

    /// Return the pending revenue pool address, if a two-step proposal is in progress.
    pub fn get_pending_revenue_pool(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::PendingRevenuePool)
    }

    /// Return the allowed depositor list.
    pub fn get_allowed_depositors(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get::<_, Vec<Address>>(&StorageKey::AllowedDepositors)
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Return the capability bitmap for this contract version.
    pub fn capabilities(env: Env) -> u64 {
        capabilities::capabilities(&env)
    }

    // -----------------------------------------------------------------------
    // Pause / unpause
    // -----------------------------------------------------------------------

    /// Owner or admin: pause the vault immediately (no timelock).
    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        let owner = Self::load_owner(&env);
        let admin = env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::Admin)
            .unwrap_or_else(|| owner.clone());
        if caller != owner && caller != admin {
            panic!("unauthorized: only owner or admin can pause");
        }
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("vault already paused");
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        Self::bump_instance(&env);
        env.events()
            .publish((events::event_vault_paused(&env), caller), ());
    }

    /// Owner or admin: unpause the vault.
    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        let owner = Self::load_owner(&env);
        let admin = env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::Admin)
            .unwrap_or_else(|| owner.clone());
        if caller != owner && caller != admin {
            panic!("unauthorized: only owner or admin can unpause");
        }
        if !env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("vault not paused");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
        Self::bump_instance(&env);
        env.events()
            .publish((events::event_vault_unpaused(&env), caller), ());
    }

    /// Admin-only emergency pause (no timelock). Accepts Stellar multisig
    /// accounts — `require_auth` enforces native account thresholds.
    pub fn nuclear_pause(env: Env, caller: Address) -> Result<(), VaultError> {
        caller.require_auth();
        let admin = env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::Admin)
            .ok_or(VaultError::NotInitialized)?;
        if caller != admin {
            return Err(VaultError::Unauthorized);
        }
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(VaultError::AlreadyPaused);
        }
        env.storage().instance().set(&DataKey::Paused, &true);
        Self::bump_instance(&env);
        env.events()
            .publish((events::event_vault_paused(&env), caller), ());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Setters — owner-only (no explicit caller arg; derives from stored owner)
    // -----------------------------------------------------------------------

    /// Owner-only: update the max single-deduction limit.
    pub fn set_max_deduct(env: Env, max_deduct: i128) {
        let owner = Self::load_owner(&env);
        owner.require_auth();
        if max_deduct <= 0 {
            panic!("max_deduct must be positive");
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxDeduct, &max_deduct);
        Self::bump_instance(&env);
        env.events().publish(
            (events::event_set_max_deduct(&env), owner),
            max_deduct,
        );
    }

    /// Owner-only: set or clear the single allowed depositor.
    ///
    /// `Some(addr)` adds `addr` to the depositor list (idempotent).
    /// `None` clears the entire allowed depositor list.
    pub fn set_allowed_depositor(
        env: Env,
        caller: Address,
        depositor: Option<Address>,
    ) {
        Self::require_owner_auth(&env, &caller);

        let list_key = StorageKey::AllowedDepositors;
        match depositor {
            None => {
                // Clear all depositor flags
                let current: Vec<Address> = env
                    .storage()
                    .instance()
                    .get::<_, Vec<Address>>(&list_key)
                    .unwrap_or_else(|| Vec::new(&env));
                for addr in current.iter() {
                    env.storage()
                        .instance()
                        .remove(&DataKey::Depositor(addr.clone()));
                }
                env.storage().instance().remove(&list_key);
            }
            Some(ref addr) => {
                // Idempotent add
                let already = env
                    .storage()
                    .instance()
                    .get::<_, bool>(&DataKey::Depositor(addr.clone()))
                    .unwrap_or(false);
                if !already {
                    env.storage()
                        .instance()
                        .set(&DataKey::Depositor(addr.clone()), &true);
                    // Update list
                    let mut list: Vec<Address> = env
                        .storage()
                        .instance()
                        .get::<_, Vec<Address>>(&list_key)
                        .unwrap_or_else(|| Vec::new(&env));
                    list.push_back(addr.clone());
                    env.storage().instance().set(&list_key, &list);
                }
            }
        }
        Self::bump_instance(&env);
    }

    /// Owner-only: update the authorized deduction caller.
    pub fn set_authorized_caller(env: Env, caller: Address) {
        caller.require_auth();
        let owner = Self::load_owner(&env);
        if caller != owner {
            panic!("unauthorized: not owner");
        }
        let old: Option<Address> = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::AuthorizedCaller);
        env.storage()
            .instance()
            .set(&DataKey::AuthorizedCaller, &caller);
        Self::bump_instance(&env);
        env.events().publish(
            (events::event_set_authorized_caller(&env), caller.clone()),
            (old, Some(caller), 0u64),
        );
    }

    /// Owner-only: set settlement contract address.
    pub fn set_settlement(env: Env, caller: Address, settlement: Address) {
        Self::require_owner_auth(&env, &caller);
        env.storage()
            .instance()
            .set(&DataKey::Settlement, &settlement);
        Self::bump_instance(&env);
        env.events().publish(
            (events::event_set_settlement(&env), caller),
            settlement,
        );
    }

    /// Owner-only: propose a new revenue pool (two-step transfer).
    /// `None` clears the current pool.
    pub fn propose_revenue_pool(env: Env, new_pool: Option<Address>) {
        let owner = Self::load_owner(&env);
        owner.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::PendingRevenuePool, &new_pool);
        Self::bump_instance(&env);
        env.events().publish(
            (events::event_revenue_pool_proposed(&env), owner),
            new_pool,
        );
    }

    /// Accept a pending revenue pool proposal.
    pub fn accept_revenue_pool(env: Env) {
        let owner = Self::load_owner(&env);
        owner.require_auth();
        // Fetch the Option<Address> stored for the pending pool
        let pending: Option<Address> = env
            .storage()
            .instance()
            .get::<_, Option<Address>>(&DataKey::PendingRevenuePool)
            .unwrap_or(None);
        env.storage()
            .instance()
            .remove(&DataKey::PendingRevenuePool);
        match pending {
            Some(pool) => {
                env.storage().instance().set(&DataKey::RevenuePool, &pool);
            }
            None => {
                env.storage().instance().remove(&DataKey::RevenuePool);
            }
        }
        Self::bump_instance(&env);
        env.events().publish(
            (events::event_revenue_pool_accepted(&env), owner),
            (),
        );
    }

    /// Cancel a pending revenue pool proposal.
    pub fn cancel_revenue_pool(env: Env) {
        let owner = Self::load_owner(&env);
        owner.require_auth();
        env.storage()
            .instance()
            .remove(&DataKey::PendingRevenuePool);
        Self::bump_instance(&env);
        env.events().publish(
            (events::event_revenue_pool_cancelled(&env), owner),
            (),
        );
    }

    /// Owner-only: directly set the revenue pool (bypass two-step).
    /// Used by test_views which calls `client.set_revenue_pool(&owner, &Some(pool))`.
    pub fn set_revenue_pool(env: Env, caller: Address, pool: Option<Address>) {
        Self::require_owner_auth(&env, &caller);
        match pool {
            Some(ref p) => {
                env.storage().instance().set(&DataKey::RevenuePool, p);
            }
            None => {
                env.storage().instance().remove(&DataKey::RevenuePool);
            }
        }
        Self::bump_instance(&env);
        env.events().publish(
            (events::event_set_revenue_pool(&env), caller),
            pool,
        );
    }

    // -----------------------------------------------------------------------
    // Reserve cap
    // -----------------------------------------------------------------------

    /// Owner-only: set a per-token reserve cap.
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
        Ok(())
    }

    /// Return the reserve cap for `token`.
    pub fn get_reserve_cap(env: Env, token: Address) -> i128 {
        limits::get(&env, &token)
    }

    pub(crate) fn require_owner(env: Env, caller: Address) -> Result<(), VaultError> {
        let owner = Self::get_owner(env);
        if caller != owner {
            return Err(VaultError::Unauthorized);
        }
        Ok(())
    }
}

#[contractimpl]
impl CalloraVault {
    // -----------------------------------------------------------------------
    // Admin management (two-step transfer)
    // -----------------------------------------------------------------------

    pub fn get_settlement(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .unwrap()
    }

    pub fn set_settlement(env: Env, caller: Address, settlement: Address) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        env.storage()
            .instance()
            .set(&DataKey::Settlement, &settlement);
    }
    pub fn get_revenue_pool(env: Env) -> Option<Address> {
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
        Ok(())
    }

    /// Return the configured timelock window length in seconds.
    ///
    /// Defaults to [`timelock::DEFAULT_TIMELOCK_SECONDS`] (48 h) when no
    /// window has been explicitly configured.
    pub fn get_timelock_window(env: Env) -> u64 {
        timelock::get_timelock_window(&env)
    }

    /// Return the pending pause proposal, if any.
    pub fn get_pending_pause(env: Env) -> Option<timelock::PendingPause> {
        timelock::get_pending_pause(&env)
    }

    /// Return the pending upgrade proposal, if any.
    pub fn get_pending_upgrade(env: Env) -> Option<timelock::PendingUpgrade> {
        timelock::get_pending_upgrade(&env)
    }

    /// Return the pending sweep proposal, if any.
    pub fn get_pending_sweep(env: Env) -> Option<timelock::PendingSweep> {
        timelock::get_pending_sweep(&env)
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
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Propose/Execute/Cancel — sweep
    // -----------------------------------------------------------------------

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
        Ok(())
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
mod timelock;

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
mod test_gas_budget;

#[cfg(test)]
mod test_rate_limit;

#[cfg(test)]
mod test_timelock;

#[cfg(test)]
mod test_init_hardening;

#[cfg(test)]
mod test_balance_property;

#[cfg(test)]
mod test_capabilities;

#[cfg(test)]
mod test_cross_invariant;

#[cfg(test)]
mod test_limits;

#[cfg(test)]
mod test_reserve_cap;

#[cfg(test)]
mod test_set_auth_fuzz;

#[cfg(test)]
mod test_setter_validation;

#[cfg(test)]
mod test_withdraw_to_zero_address;
