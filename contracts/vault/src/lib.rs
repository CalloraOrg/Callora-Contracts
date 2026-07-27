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
use soroban_sdk::{
    contract, contractclient, contractimpl, contracttype, Address, BytesN, Env, String,
    Symbol, Vec,
};

pub mod timelock;
pub mod views;

/// Bounded visible-ASCII metadata validators (shared `callora-validators` crate).
pub use callora_validators as validators;

mod errors;
pub use errors::VaultError;

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
    /// Caller is not in the allowlist and is not the owner (code 44).
    CallerNotInAllowlist = 44,
}

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
    AllowedDepositorsList,
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
    /// Vector of addresses allowed to deposit (owner-managed allowlist).
    AllowedDepositors,
}

/// TTL extension trigger for instance storage keys (~30 days of ledgers at 5 s/ledger).
pub const INSTANCE_BUMP_THRESHOLD: u32 = 17_280 * 30;

/// TTL extension target for instance storage keys (~60 days of ledgers at 5 s/ledger).
pub const INSTANCE_BUMP_AMOUNT: u32 = 17_280 * 60;

pub mod token {
    pub use soroban_sdk::token::Client;
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
    fn require_positive_amount(amount: i128) -> Result<(), VaultError> {
        if amount <= 0 {
            return Err(VaultError::AmountNotPositive);
        }
        Ok(())
    }

    fn require_valid_deposit_amount(amount: i128, min_deposit: i128) -> Result<(), VaultError> {
        Self::require_positive_amount(amount)?;
        if amount < min_deposit {
            return Err(VaultError::BelowMinDeposit);
        }
        Ok(())
    }

    fn require_valid_deduct_amount(amount: i128, min_amount: i128, max_deduct: i128) -> Result<(), VaultError> {
        Self::require_positive_amount(amount)?;
        if amount < min_amount {
            return Err(VaultError::BelowMinDeposit);
        }
        if amount > max_deduct {
            return Err(VaultError::ExceedsMaxDeduct);
        }
        Ok(())
    }

    /// Initialize the Callora Vault contract (one-time setup).
    ///
    /// Stores configuration in instance storage and sets the paused flag to `false`.
    /// The admin role defaults to `owner` at initialization and may be transferred
    /// later via the two-step [`set_admin`] / [`accept_admin`] flow.
    ///
    /// # Parameters
    /// - `owner` — vault owner; only this address may call owner-gated functions.
    /// - `usdc_token` — USDC token contract address used for all transfers.
    /// - `initial_balance` — the tracked balance to record at startup (must be ≥ 0).
    /// - `authorized_caller` — address permitted to call [`deduct`] and [`batch_deduct`].
    /// - `min_deposit` — minimum accepted deposit amount in stroops; must be > 0.
    /// - `revenue_pool` — optional revenue pool address; may be `None`.
    /// - `max_deduct` — per-call deduction cap in stroops; must be > 0 and ≥ `min_deposit`.
    /// - `settlement` — settlement contract address that receives deducted funds.
    ///
    /// # Panics
    /// - If already initialized.
    /// - If `min_deposit <= 0`, `max_deduct <= 0`, or `min_deposit > max_deduct`.
    ///
    /// No auth is required; `init` is a constructor and may only succeed once.
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
    ) -> Result<(), VaultError> {
        if env.storage().instance().has(&DataKey::Owner) {
            return Err(VaultError::AlreadyInitialized);
        }
        if min_deposit <= 0 {
            return Err(VaultError::MinDepositNotPositive);
        }
        if max_deduct <= 0 {
            return Err(VaultError::MaxDeductNotPositive);
        }
        if min_deposit > max_deduct {
            return Err(VaultError::MinDepositExceedsMaxDeduct);
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
        Ok(())
    }

    pub fn deposit(env: Env, caller: Address, amount: i128) -> Result<(), VaultError> {
        caller.require_auth();
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(VaultError::Paused);
        }
        let min_dep = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MinDeposit)
            .unwrap();
        Self::require_valid_deposit_amount(amount, min_dep)?;
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            let allowlist = env
                .storage()
                .instance()
                .get::<_, Vec<Address>>(&StorageKey::AllowedDepositors)
                .unwrap_or_else(|| Vec::new(&env));

            if !allowlist.contains(&caller) {
                return Err(VaultError::CallerNotInAllowlist);
            }
        }
        let current_bal = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0);
        let new_bal = current_bal.checked_add(amount).ok_or(VaultError::Overflow)?;
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        
        let token_addr = env.storage().instance().get::<_, Address>(&DataKey::UsdcToken)
            .unwrap_or_else(|| panic!("USDC Token not set"));
            
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&caller, &env.current_contract_address(), &amount);
        Ok(())
    }

    pub fn deduct(env: Env, caller: Address, amount: i128, request_id: u64) -> Result<(), VaultError> {
        caller.require_auth();
        
        let auth_caller = env.storage().instance().get::<_, Address>(&DataKey::AuthorizedCaller)
            .unwrap_or_else(|| panic!("Authorized caller not set"));
            
        if caller != auth_caller {
            return Err(VaultError::Unauthorized);
        }
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(VaultError::Paused);
        }
        let min_dep = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MinDeposit)
            .unwrap();
        let max_deduct = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap();
        Self::require_valid_deduct_amount(amount, min_dep, max_deduct)?;
        let current_bal = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0);
        if current_bal < amount {
            return Err(VaultError::InsufficientBalance);
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
        Ok(())
    }

    pub fn batch_deduct(env: Env, caller: Address, items: Vec<(i128, u64)>) -> Result<(), VaultError> {
        caller.require_auth();
        
        let auth_caller = env.storage().instance().get::<_, Address>(&DataKey::AuthorizedCaller)
            .unwrap_or_else(|| panic!("Authorized caller not set"));
            
        if caller != auth_caller {
            return Err(VaultError::Unauthorized);
        }
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            return Err(VaultError::Paused);
        }
        
        let min_dep = env.storage().instance().get::<_, i128>(&DataKey::MinDeposit)
            .unwrap_or_else(|| panic!("Min deposit not set"));
            
        let max_deduct = env.storage().instance().get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap_or_else(|| panic!("Max deduct not set"));
            
        let mut total_amount: i128 = 0;
        for item in items.iter() {
            let (amount, _) = item;
            Self::require_valid_deduct_amount(amount, min_dep, max_deduct)?;
            total_amount = total_amount.checked_add(amount).ok_or(VaultError::Overflow)?;
        }
        
        let current_bal = env.storage().instance().get::<_, i128>(&DataKey::Balance).unwrap_or(0);
        
        if current_bal < total_amount {
            return Err(VaultError::InsufficientBalance);
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
        Ok(())
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

    pub fn set_authorized_caller(env: Env, caller: Address) -> Result<(), VaultError> {
        caller.require_auth();
        
        let owner = env.storage().instance().get::<_, Address>(&DataKey::Owner)
            .unwrap_or_else(|| panic!("Owner not set"));
            
        if caller != owner {
            return Err(VaultError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::AuthorizedCaller, &caller);
        Ok(())
    }

    pub fn pause(env: Env, caller: Address) -> Result<(), VaultError> {
        caller.require_auth();
        
        let owner = env.storage().instance().get::<_, Address>(&DataKey::Owner)
            .unwrap_or_else(|| panic!("Owner not set"));
            
        if caller != owner {
            return Err(VaultError::Unauthorized);
        }
        
        env.storage().instance().set(&DataKey::Paused, &true);
        Ok(())
    }

    pub fn unpause(env: Env, caller: Address) -> Result<(), VaultError> {
        caller.require_auth();
        
        let owner = env.storage().instance().get::<_, Address>(&DataKey::Owner)
            .unwrap_or_else(|| panic!("Owner not set"));
            
        if caller != owner {
            return Err(VaultError::Unauthorized);
        }
        
        env.storage().instance().set(&DataKey::Paused, &false);
        Ok(())
    }

    /// Return `true` if the vault is currently paused, `false` otherwise.
    ///
    /// Reads the `Paused` instance storage flag set by [`pause`] / [`execute_pause`]
    /// and cleared by [`unpause`]. Defaults to `false` when the flag is absent
    /// (i.e., before initialization).
    ///
    /// No auth required; this is a read-only view.
    pub fn is_paused(env: Env) -> bool {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }
    /// Return the vault's internally tracked USDC balance in stroops.
    ///
    /// This value is incremented by [`deposit`] and decremented by [`deduct`],
    /// [`batch_deduct`], and successful sweep executions. It may diverge from
    /// the on-ledger USDC balance when USDC is sent directly to the vault address
    /// (bypassing `deposit`). Use [`dry_run_sweep_idle_balance`] to inspect any
    /// surplus.
    ///
    /// No auth required; this is a read-only view.
    pub fn balance(env: Env) -> i128 {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0)
    }
    /// Return the current vault owner address.
    ///
    /// The owner is set at [`init`] and may be transferred via a two-step
    /// ownership-transfer flow. No auth required; this is a read-only view.
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
    /// Return the USDC token contract address configured at [`init`].
    ///
    /// All vault transfers use this token contract. No auth required; this is a
    /// read-only view.
    pub fn get_usdc_token(env: Env) -> Address {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .unwrap_or_else(|| panic!("USDC Token not set"))
    }
    /// Return the per-call deduction cap in USDC stroops.
    ///
    /// [`deduct`] and each item of [`batch_deduct`] are rejected when their
    /// amount exceeds this value. Returns `i128::MAX` when no cap is stored
    /// (effectively unlimited), though in practice the limit is always set
    /// during [`init`].
    ///
    /// No auth required; this is a read-only view.
    pub fn get_max_deduct(env: Env) -> i128 {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap_or(i128::MAX)
    }

    pub fn set_max_deduct(env: Env, caller: Address, max_deduct: i128) -> Result<(), VaultError> {
        caller.require_auth();
        
        let owner = env.storage().instance().get::<_, Address>(&DataKey::Owner)
            .unwrap_or_else(|| panic!("Owner not set"));
            
        if caller != owner {
            return Err(VaultError::Unauthorized);
        }
        if max_deduct <= 0 {
            return Err(VaultError::MaxDeductNotPositive);
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxDeduct, &max_deduct);
        Ok(())
    }

    /// Return the configured settlement contract address.
    ///
    /// This is the on-ledger destination that receives USDC transferred during
    /// [`deduct`] and [`batch_deduct`]. Panics if the address has never been
    /// set, which cannot occur after a successful [`init`].
    ///
    /// No auth required; this is a read-only view.
    pub fn get_settlement(env: Env) -> Address {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .unwrap_or_else(|| panic!("Settlement not set"))
    }

    pub fn set_settlement(env: Env, caller: Address, settlement: Address) -> Result<(), VaultError> {
        caller.require_auth();
        
        let owner = env.storage().instance().get::<_, Address>(&DataKey::Owner)
            .unwrap_or_else(|| panic!("Owner not set"));
            
        if caller != owner {
            return Err(VaultError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&DataKey::Settlement, &settlement);
        Ok(())
    }
    /// Return the configured revenue pool address, if any.
    ///
    /// Returns `None` when no revenue pool has been set (the default after
    /// [`init`] when `revenue_pool` is `None`). No auth required.
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

    /// Return the outstanding timelocked pause proposal, if any.
    ///
    /// Returns `None` when no pause proposal is currently staged.  A `Some`
    /// value means a pause was proposed and can be executed once the ledger
    /// clock reaches `proposal.execute_after` (via [`execute_pause`]).
    ///
    /// No auth required; this is a read-only view.
    pub fn get_pending_pause(env: Env) -> Option<timelock::PendingPause> {
        Self::bump_instance_ttl(&env);
        let result = timelock::get_pending_pause(&env);
        if result.is_some() {
            Self::bump_persistent_key(&env, &StorageKey::PendingPause);
        }
        result
    }

    /// Return the outstanding timelocked upgrade proposal, if any.
    ///
    /// Returns `None` when no upgrade proposal is staged.  A `Some` value
    /// includes the target `wasm_hash` and the `execute_after` deadline (see
    /// [`execute_upgrade`]).
    ///
    /// No auth required; this is a read-only view.
    pub fn get_pending_upgrade(env: Env) -> Option<timelock::PendingUpgrade> {
        Self::bump_instance_ttl(&env);
        let result = timelock::get_pending_upgrade(&env);
        if result.is_some() {
            Self::bump_persistent_key(&env, &StorageKey::PendingUpgrade);
        }
        result
    }

    /// Return the outstanding timelocked sweep proposal, if any.
    ///
    /// Returns `None` when no sweep proposal is staged.  A `Some` value
    /// includes the recipient address, the amount, and the `execute_after`
    /// deadline (see [`execute_sweep`]).
    ///
    /// No auth required; this is a read-only view.
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
        env.events()
            .publish((events::event_pause_executed(&env), caller.clone()), env.ledger().timestamp());
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
            (existing.is_some()),
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
            (existing.is_some()),
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
            (existing.is_some()),
        );
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    /// Remove processed request-ID markers from persistent storage (owner only).
    ///
    /// Idempotency markers written by [`deduct`] and [`batch_deduct`] live in
    /// persistent storage indefinitely. This function lets the owner reclaim
    /// that storage once markers are no longer needed for duplicate detection.
    /// IDs not present in storage are silently skipped.
    ///
    /// # Authorization
    /// `caller.require_auth()` is enforced. `caller` must be the current owner.
    ///
    /// # Parameters
    /// - `ids` — list of request IDs whose markers should be removed.
    ///
    /// # Events
    /// Emits a `request_id_pruned` event for each successfully removed ID.
    ///
    /// # Errors
    /// - [`VaultError::Unauthorized`] if `caller` is not the owner.
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

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    #[inline(never)]
    fn require_authorized_deduct_caller(env: Env, caller: &Address) -> Result<(), VaultError> {
        let meta = Self::get_meta(env.clone())?;
        let auth = match &meta.authorized_caller {
            Some(ac) => caller == ac || *caller == meta.owner,
            None => *caller == meta.owner,
        };
        if !auth {
            return Err(VaultError::Unauthorized);
        }
        Ok(())
    }

    /// Return `true` if `request_id` has already been processed (marker present
    /// in persistent storage, or temporary storage for legacy markers).
    pub fn is_request_processed(env: Env, request_id: Symbol) -> bool {
        let key = StorageKey::ProcessedRequest(request_id);
        env.storage().persistent().has(&key) || env.storage().temporary().has(&key)
    }

    /// Check that `request_id` has NOT been processed yet.
    /// Returns `VaultError::DuplicateRequestId` if the marker exists.
    pub(crate) fn require_not_duplicate(env: &Env, request_id: &Symbol) -> Result<(), VaultError> {
        let key = StorageKey::ProcessedRequest(request_id.clone());
        if env.storage().persistent().has(&key) || env.storage().temporary().has(&key) {
            return Err(VaultError::DuplicateRequestId);
        }
        Ok(())
    }

    /// Persist a processed-request marker in persistent storage and set its TTL.
    fn mark_request_processed(env: &Env, request_id: &Symbol) {
        let key = StorageKey::ProcessedRequest(request_id.clone());
        env.storage().persistent().set(&key, &true);
        env.storage()
            .persistent()
            .extend_ttl(&key, REQUEST_ID_BUMP_THRESHOLD, REQUEST_ID_BUMP_AMOUNT);
    }

    fn transfer_funds(env: &Env, usdc_token: &Address, to: &Address, amount: i128) {
        token::Client::new(env, usdc_token).transfer(&env.current_contract_address(), to, &amount);
    }

    fn require_settlement(env: &Env) -> Result<Address, VaultError> {
        env.storage()
            .instance()
            .get(&StorageKey::Settlement)
            .ok_or(VaultError::SettlementNotSet)
    }

    #[inline(never)]
    fn require_not_paused(env: Env) -> Result<(), VaultError> {
        if Self::is_paused(env) {
            return Err(VaultError::Paused);
        }
        Ok(())
    }

    #[inline(never)]
    fn require_admin_or_owner(env: Env, caller: &Address) -> Result<(), VaultError> {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(VaultError::NotInitialized)?;
        let meta = Self::get_meta(env)?;
        if *caller != admin && *caller != meta.owner {
            return Err(VaultError::Unauthorized);
        }
        Ok(())
    }

    /// Broadcast an emergency message from the admin.
    ///
    /// The vault owner may always deposit regardless of this flag.
    /// Non-owner callers must be explicitly added to the allowlist to call
    /// [`deposit`]. No auth required; this is a read-only view.
    pub fn is_authorized_depositor(env: Env, caller: Address) -> bool {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Depositor(caller))
            .unwrap_or(false)
    }

    /// Add a single address to the deposit allowlist (owner-only).
    ///
    /// If the address is already present, this function is idempotent and will
    /// succeed without error. The `allowlist_add` event is emitted even for
    /// duplicate adds to maintain audit trail clarity.
    ///
    /// # Parameters
    /// - `caller` — Must be the vault owner (verified via `require_owner`)
    /// - `depositor` — Address to add to the allowlist
    ///
    /// # Returns
    /// `Ok(())` on success, or `VaultError::Unauthorized` if caller is not owner.
    ///
    /// # Events
    /// Emits `("allowlist_add", caller, depositor)` on every successful call,
    /// including duplicates.
    ///
    /// # Examples
    /// ```ignore
    /// vault.add_address(&owner, &backend_service_1)?;
    /// vault.add_address(&owner, &backend_service_2)?;
    /// ```
    pub fn add_address(
        env: Env,
        caller: Address,
        depositor: Address,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;
        
        let mut allowlist = env
            .storage()
            .instance()
            .get::<_, Vec<Address>>(&StorageKey::AllowedDepositors)
            .unwrap_or_else(|| Vec::new(&env));
        
        // Idempotent: only add if not already present
        if !allowlist.contains(&depositor) {
            allowlist.push_back(depositor.clone());
            env.storage()
                .instance()
                .set(&StorageKey::AllowedDepositors, &allowlist);
        }
        
        env.events().publish(
            (events::event_allowlist_add(&env), caller, depositor),
            ()
        );
        
        Ok(())
    }

    /// Remove all addresses from the deposit allowlist (owner-only).
    ///
    /// This function is idempotent — calling it on an empty allowlist succeeds
    /// without error.
    ///
    /// # Parameters
    /// - `caller` — Must be the vault owner (verified via `require_owner`)
    ///
    /// # Returns
    /// `Ok(())` on success, or `VaultError::Unauthorized` if caller is not owner.
    ///
    /// # Events
    /// Emits `("allowlist_clear", caller)` on every successful call, even when
    /// the allowlist is already empty.
    ///
    /// # Examples
    /// ```ignore
    /// vault.clear_all(&owner)?;
    /// // Subsequent calls succeed (idempotent):
    /// vault.clear_all(&owner)?;
    /// ```
    pub fn clear_all(
        env: Env,
        caller: Address,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;
        
        env.storage()
            .instance()
            .remove(&StorageKey::AllowedDepositors);
        
        env.events().publish(
            (events::event_allowlist_clear(&env), caller),
            ()
        );
        
        Ok(())
    }

    /// Return the current deposit allowlist.
    ///
    /// No authentication required — this is a public read-only view function.
    /// Addresses are returned in insertion order.
    ///
    /// # Returns
    /// `Vec<Address>` containing all addresses currently in the allowlist.
    /// Returns an empty vector if no allowlist has been configured.
    ///
    /// # Examples
    /// ```ignore
    /// let allowed = vault.get_allowlist();
    /// assert_eq!(allowed.len(), 3);
    /// ```
    pub fn get_allowlist(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get::<_, Vec<Address>>(&StorageKey::AllowedDepositors)
            .unwrap_or_else(|| Vec::new(&env))
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
mod views;

// ---------------------------------------------------------------------------
// Test modules
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_views;

#[cfg(test)]
mod test_simulate_deduct;

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
mod test_rustdoc_coverage;

// #[cfg(test)]
// mod test_gas_budget;
// #[cfg(test)]
// mod test_rate_limit;