#![no_std]
/// # Callora Vault Contract — deposit/withdraw/deduct/distribute with pause circuit-breaker.
///
/// ## Pause Circuit Breaker
///
/// When the vault is paused:
/// - Deposits are blocked
/// - Single and batch deducts are blocked
/// - Owner withdrawals are ALLOWED (emergency recovery)
/// - Admin distribute is ALLOWED (emergency recovery of untracked surplus)
/// - Admin/owner configuration functions remain available
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, token, Address, Env, String, Symbol, Vec,
};

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
}

#[contracttype]
#[derive(Clone)]
pub struct DeductItem {
    pub amount: i128,
    pub request_id: Option<Symbol>,
}

#[contracttype]
#[derive(Clone)]
pub struct VaultMeta {
    pub owner: Address,
    pub balance: i128,
    pub authorized_caller: Option<Address>,
    pub min_deposit: i128,
}

/// Payload for `withdraw` and `withdraw_to` events.
#[contracttype]
#[derive(Clone)]
pub struct WithdrawEventData {
    pub amount: i128,
    pub new_balance: i128,
}

/// Canonical storage keys for the Vault contract.
#[contracttype]
pub enum StorageKey {
    MetaKey,
    Admin,
    UsdcToken,
    Settlement,
    RevenuePool,
    /// Storage slot for the maximum allowed amount per deduct call.
    MaxDeduct,
    Paused,
    Metadata(String),
    PendingOwner,
    PendingAdmin,
    DepositorList,
    /// Contract version marker (WASM hash) set by `upgrade`.
    ContractVersion,
}

pub const DEFAULT_MAX_DEDUCT: i128 = i128::MAX;
pub const DEFAULT_MIN_DEPOSIT: i128 = 1;
pub const MAX_BATCH_SIZE: u32 = 50;
pub const MAX_METADATA_LEN: u32 = 256;
pub const MAX_OFFERING_ID_LEN: u32 = 64;

// ~17 280 ledgers per day at 5-second close time.
// Bump when fewer than 30 days remain; extend to 60 days.
pub const INSTANCE_BUMP_THRESHOLD: u32 = 17_280 * 30; // ~30 days
pub const INSTANCE_BUMP_AMOUNT: u32 = 17_280 * 60; // ~60 days

#[contract]
pub struct CalloraVault;

#[contractimpl]
impl CalloraVault {
    /// Initialize the vault. Exactly-once; returns error if called again.
    ///
    /// # Parameters
    /// - `owner` — vault owner; must sign the transaction.
    /// - `usdc_token` — USDC token contract address; must not be the vault itself.
    /// - `initial_balance` — optional starting balance (defaults to 0). The vault
    ///   must already hold at least this many USDC stroops on-ledger.
    /// - `authorized_caller` — optional address permitted to call `deduct`/`batch_deduct`.
    ///   Must not be the vault address.
    /// - `min_deposit` — minimum deposit amount (defaults to 1, must be > 0).
    /// - `revenue_pool` — optional revenue pool address; informational only.
    ///   Must not be the vault address.
    /// - `max_deduct` — maximum single deduction (defaults to `i128::MAX`, must be > 0).
    ///   Must be >= `min_deposit`.
    ///
    /// # Errors
    /// - `VaultError::AlreadyInitialized` — called more than once.
    /// - `VaultError::UsdcTokenCannotBeVault` — self-referential token.
    /// - `VaultError::RevenuePoolCannotBeVault` — self-referential pool.
    /// - `VaultError::AuthorizedCallerCannotBeVault` — self-referential caller.
    /// - `VaultError::InitialBalanceNegative` — negative initial balance.
    /// - `VaultError::MinDepositNotPositive` — `min_deposit <= 0`.
    /// - `VaultError::MaxDeductNotPositive` — `max_deduct <= 0`.
    /// - `VaultError::MinDepositExceedsMaxDeduct` — constraint violation.
    /// - `VaultError::InitialBalanceExceedsOnLedger` — vault underfunded.
    #[allow(clippy::too_many_arguments)]
    pub fn init(
        env: Env,
        owner: Address,
        usdc_token: Address,
        initial_balance: Option<i128>,
        authorized_caller: Option<Address>,
        min_deposit: Option<i128>,
        revenue_pool: Option<Address>,
        max_deduct: Option<i128>,
    ) -> Result<VaultMeta, VaultError> {
        owner.require_auth();
        let inst = env.storage().instance();
        if inst.has(&StorageKey::MetaKey) {
            return Err(VaultError::AlreadyInitialized);
        }
        if usdc_token == env.current_contract_address() {
            return Err(VaultError::UsdcTokenCannotBeVault);
        }
        if let Some(p) = &revenue_pool {
            if p == &env.current_contract_address() {
                return Err(VaultError::RevenuePoolCannotBeVault);
            }
        }
        if let Some(ac) = &authorized_caller {
            if ac == &env.current_contract_address() {
                return Err(VaultError::AuthorizedCallerCannotBeVault);
            }
        }
        let balance = initial_balance.unwrap_or(0);
        if balance < 0 {
            return Err(VaultError::InitialBalanceNegative);
        }
        let min_d = min_deposit.unwrap_or(DEFAULT_MIN_DEPOSIT);
        if min_d <= 0 {
            return Err(VaultError::MinDepositNotPositive);
        }
        let max_d = max_deduct.unwrap_or(DEFAULT_MAX_DEDUCT);
        if max_d <= 0 {
            return Err(VaultError::MaxDeductNotPositive);
        }
        if min_d > max_d {
            return Err(VaultError::MinDepositExceedsMaxDeduct);
        }
        if balance > 0 {
            let on_chain =
                token::Client::new(&env, &usdc_token).balance(&env.current_contract_address());
            if on_chain < balance {
                return Err(VaultError::InitialBalanceExceedsOnLedger);
            }
        }
        let meta = VaultMeta {
            owner: owner.clone(),
            balance,
            authorized_caller,
            min_deposit: min_d,
        };
        inst.set(&StorageKey::MetaKey, &meta);
        inst.set(&StorageKey::UsdcToken, &usdc_token);
        inst.set(&StorageKey::Admin, &owner);
        if let Some(p) = revenue_pool {
            inst.set(&StorageKey::RevenuePool, &p);
        }
        inst.set(&StorageKey::MaxDeduct, &max_d);
        inst.extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.events()
            .publish((Symbol::new(&env, "init"), owner.clone()), balance);
        Ok(meta)
    }

    // -----------------------------------------------------------------------
    // View functions — no TTL bump (read-only, zero write cost)
    // -----------------------------------------------------------------------

    /// Return full vault state. Returns error if vault is not initialized.
    pub fn get_meta(env: Env) -> Result<VaultMeta, VaultError> {
        env.storage()
            .instance()
            .get(&StorageKey::MetaKey)
            .ok_or(VaultError::NotInitialized)
    }

    /// Return the current tracked USDC balance. Returns error if vault is not initialized.
    pub fn balance(env: Env) -> Result<i128, VaultError> {
        Ok(Self::get_meta(env)?.balance)
    }

    /// Return the current admin address. Returns error if vault is not initialized.
    pub fn get_admin(env: Env) -> Result<Address, VaultError> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(VaultError::NotInitialized)
    }

    /// Return the USDC token contract address. Returns error if vault is not initialized.
    pub fn get_usdc_token(env: Env) -> Result<Address, VaultError> {
        env.storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .ok_or(VaultError::NotInitialized)
    }

    /// Return the configured max deduct value. Returns `i128::MAX` if not explicitly set.
    pub fn get_max_deduct(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&StorageKey::MaxDeduct)
            .unwrap_or(DEFAULT_MAX_DEDUCT)
    }

    /// Return the configured settlement address.
    /// Returns error if `set_settlement` has not been called.
    pub fn get_settlement(env: Env) -> Result<Address, VaultError> {
        env.storage()
            .instance()
            .get(&StorageKey::Settlement)
            .ok_or(VaultError::SettlementNotSet)
    }

    /// Return the configured revenue pool address, or `None` if not set.
    pub fn get_revenue_pool(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::RevenuePool)
    }

    /// Return the pending owner address, or `None` if no ownership transfer is in progress.
    pub fn get_pending_owner(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::PendingOwner)
    }

    /// Return the pending admin address, or `None` if no admin transfer is in progress.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::PendingAdmin)
    }

    /// Return `(usdc_token, settlement, revenue_pool)` in one call.
    /// Useful for operators verifying deployment configuration.
    pub fn get_contract_addresses(env: Env) -> (Option<Address>, Option<Address>, Option<Address>) {
        let inst = env.storage().instance();
        (
            inst.get(&StorageKey::UsdcToken),
            inst.get(&StorageKey::Settlement),
            inst.get(&StorageKey::RevenuePool),
        )
    }

    /// Return `true` if the vault is currently paused, `false` otherwise.
    /// Returns `false` before the first `pause()` call (safe default).
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&StorageKey::Paused)
            .unwrap_or(false)
    }

    /// Return `true` if `caller` is the owner or an allowed depositor.
    /// Returns error if vault is not initialized.
    pub fn is_authorized_depositor(env: Env, caller: Address) -> Result<bool, VaultError> {
        let meta = Self::get_meta(env.clone())?;
        if caller == meta.owner {
            return Ok(true);
        }
        let list: Vec<Address> = env
            .storage()
            .instance()
            .get(&StorageKey::DepositorList)
            .unwrap_or(Vec::new(&env));
        Ok(list.contains(&caller))
    }

    /// Return stored offering metadata, or `None` if not set.
    pub fn get_metadata(env: Env, offering_id: String) -> Option<String> {
        env.storage()
            .instance()
            .get(&StorageKey::Metadata(offering_id))
    }

    /// Return the full allowed-depositor list.
    pub fn get_allowed_depositors(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&StorageKey::DepositorList)
            .unwrap_or(Vec::new(&env))
    }

    // -----------------------------------------------------------------------
    // Mutating functions
    // -----------------------------------------------------------------------

    pub fn set_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), VaultError> {
        caller.require_auth();
        let cur = Self::get_admin(env.clone())?;
        if caller != cur {
            return Err(VaultError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);
        env.events()
            .publish((Symbol::new(&env, "admin_nominated"), cur, new_admin), ());
        Ok(())
    }

    pub fn accept_admin(env: Env) -> Result<(), VaultError> {
        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .ok_or(VaultError::NoAdminTransferPending)?;
        pending.require_auth();
        let cur = Self::get_admin(env.clone())?;
        env.storage().instance().set(&StorageKey::Admin, &pending);
        env.storage().instance().remove(&StorageKey::PendingAdmin);
        env.events()
            .publish((Symbol::new(&env, "admin_accepted"), cur, pending), ());
        Ok(())
    }

    pub fn require_owner(env: Env, caller: Address) -> Result<(), VaultError> {
        let meta = Self::get_meta(env.clone())?;
        if caller != meta.owner {
            return Err(VaultError::Unauthorized);
        }
        Ok(())
    }

    /// Set or clear the authorized caller for `deduct`/`batch_deduct` (owner only).
    pub fn set_authorized_caller(
        env: Env,
        new_caller: Option<Address>,
    ) -> Result<(), VaultError> {
        let mut meta = Self::get_meta(env.clone())?;
        meta.owner.require_auth();
        let old = meta.authorized_caller.clone();
        meta.authorized_caller = new_caller.clone();
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        env.events().publish(
            (
                Symbol::new(&env, "set_authorized_caller"),
                meta.owner.clone(),
            ),
            (old, new_caller),
        );
        Ok(())
    }

    /// Set `max_deduct` (owner only).
    ///
    /// # Errors
    /// - `VaultError::MaxDeductNotPositive` when `max_deduct <= 0`.
    pub fn set_max_deduct(env: Env, max_deduct: i128) -> Result<(), VaultError> {
        let meta = Self::get_meta(env.clone())?;
        meta.owner.require_auth();
        if max_deduct <= 0 {
            return Err(VaultError::MaxDeductNotPositive);
        }
        let old = Self::get_max_deduct(env.clone());
        env.storage()
            .instance()
            .set(&StorageKey::MaxDeduct, &max_deduct);
        env.events().publish(
            (Symbol::new(&env, "set_max_deduct"), meta.owner),
            (old, max_deduct),
        );
        Ok(())
    }

    pub fn set_allowed_depositor(
        env: Env,
        caller: Address,
        depositor: Option<Address>,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;
        match depositor {
            Some(d) => {
                let mut list: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&StorageKey::DepositorList)
                    .unwrap_or(Vec::new(&env));
                if !list.contains(&d) {
                    list.push_back(d);
                }
                env.storage()
                    .instance()
                    .set(&StorageKey::DepositorList, &list);
            }
            None => {
                env.storage()
                    .instance()
                    .set(&StorageKey::DepositorList, &Vec::<Address>::new(&env));
            }
        }
        Ok(())
    }

    pub fn clear_allowed_depositors(env: Env, caller: Address) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller)?;
        env.storage()
            .instance()
            .set(&StorageKey::DepositorList, &Vec::<Address>::new(&env));
        Ok(())
    }

    pub fn pause(env: Env, caller: Address) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_admin_or_owner(env.clone(), &caller)?;
        if Self::is_paused(env.clone()) {
            return Err(VaultError::AlreadyPaused);
        }
        env.storage().instance().set(&StorageKey::Paused, &true);
        env.events()
            .publish((Symbol::new(&env, "vault_paused"), caller), ());
        Ok(())
    }

    pub fn unpause(env: Env, caller: Address) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_admin_or_owner(env.clone(), &caller)?;
        if !Self::is_paused(env.clone()) {
            return Err(VaultError::NotPaused);
        }
        env.storage().instance().set(&StorageKey::Paused, &false);
        env.events()
            .publish((Symbol::new(&env, "vault_unpaused"), caller), ());
        Ok(())
    }

    pub fn deposit(env: Env, caller: Address, amount: i128) -> Result<i128, VaultError> {
        Self::require_not_paused(env.clone())?;
        caller.require_auth();
        if amount <= 0 {
            return Err(VaultError::AmountNotPositive);
        }
        if !Self::is_authorized_depositor(env.clone(), caller.clone())? {
            return Err(VaultError::Unauthorized);
        }
        let mut meta = Self::get_meta(env.clone())?;
        if amount < meta.min_deposit {
            return Err(VaultError::BelowMinDeposit);
        }
        let usdc_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .ok_or(VaultError::NotInitialized)?;
        token::Client::new(&env, &usdc_addr)
            .transfer(&caller, &env.current_contract_address(), &amount);
        meta.balance = meta
            .balance
            .checked_add(amount)
            .ok_or(VaultError::Overflow)?;
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.events().publish(
            (Symbol::new(&env, "deposit"), caller),
            (amount, meta.balance),
        );
        Ok(meta.balance)
    }

    /// Deduct USDC from the vault and transfer it to the configured settlement address.
    ///
    /// # Preconditions
    /// - `set_settlement` must have been called; returns error otherwise.
    /// - `amount` must be positive and <= `max_deduct`.
    /// - `caller` must be the owner or `authorized_caller`.
    /// - Vault balance must cover `amount`.
    pub fn deduct(
        env: Env,
        caller: Address,
        amount: i128,
        request_id: Option<Symbol>,
    ) -> Result<i128, VaultError> {
        Self::require_not_paused(env.clone())?;
        caller.require_auth();
        if amount <= 0 {
            return Err(VaultError::AmountNotPositive);
        }
        Self::require_authorized_deduct_caller(env.clone(), &caller)?;
        let max_d = Self::get_max_deduct(env.clone());
        if amount > max_d {
            return Err(VaultError::ExceedsMaxDeduct);
        }
        let mut meta = Self::get_meta(env.clone())?;
        if meta.balance < amount {
            return Err(VaultError::InsufficientBalance);
        }
        let settlement = Self::require_settlement(&env)?;
        meta.balance = meta
            .balance
            .checked_sub(amount)
            .ok_or(VaultError::Overflow)?;
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let ut: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .ok_or(VaultError::NotInitialized)?;
        Self::transfer_funds(&env, &ut, &settlement, amount);
        let rid = request_id.unwrap_or(Symbol::new(&env, ""));
        env.events().publish(
            (Symbol::new(&env, "deduct"), caller, rid),
            (amount, meta.balance),
        );
        Ok(meta.balance)
    }

    /// Deduct multiple items atomically.
    ///
    /// Full-batch validation completes before any state write or transfer.
    /// If any item fails validation, the entire batch reverts with no partial effects.
    pub fn batch_deduct(
        env: Env,
        caller: Address,
        items: Vec<DeductItem>,
    ) -> Result<i128, VaultError> {
        Self::require_not_paused(env.clone())?;
        caller.require_auth();
        Self::require_authorized_deduct_caller(env.clone(), &caller)?;
        let n = items.len();
        if n == 0 {
            return Err(VaultError::BatchEmpty);
        }
        if n > MAX_BATCH_SIZE {
            return Err(VaultError::BatchTooLarge);
        }
        let max_d = Self::get_max_deduct(env.clone());
        let mut meta = Self::get_meta(env.clone())?;
        let mut running = meta.balance;
        let mut total: i128 = 0;
        for item in items.iter() {
            if item.amount <= 0 {
                return Err(VaultError::AmountNotPositive);
            }
            if item.amount > max_d {
                return Err(VaultError::ExceedsMaxDeduct);
            }
            if running < item.amount {
                return Err(VaultError::InsufficientBalance);
            }
            running = running.checked_sub(item.amount).ok_or(VaultError::Overflow)?;
            total = total.checked_add(item.amount).ok_or(VaultError::Overflow)?;
        }
        let settlement = Self::require_settlement(&env)?;
        meta.balance = running;
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let ut: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .ok_or(VaultError::NotInitialized)?;
        Self::transfer_funds(&env, &ut, &settlement, total);
        for item in items.iter() {
            let rid = item.request_id.unwrap_or(Symbol::new(&env, ""));
            env.events().publish(
                (Symbol::new(&env, "deduct"), caller.clone(), rid),
                (item.amount, meta.balance),
            );
        }
        Ok(meta.balance)
    }

    pub fn transfer_ownership(env: Env, new_owner: Address) -> Result<(), VaultError> {
        let meta = Self::get_meta(env.clone())?;
        meta.owner.require_auth();
        if new_owner == meta.owner {
            return Err(VaultError::NewOwnerSameAsCurrent);
        }
        env.storage()
            .instance()
            .set(&StorageKey::PendingOwner, &new_owner);
        env.events().publish(
            (
                Symbol::new(&env, "ownership_nominated"),
                meta.owner,
                new_owner,
            ),
            (),
        );
        Ok(())
    }

    pub fn accept_ownership(env: Env) -> Result<(), VaultError> {
        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingOwner)
            .ok_or(VaultError::NoOwnershipTransferPending)?;
        pending.require_auth();
        let mut meta = Self::get_meta(env.clone())?;
        let old = meta.owner.clone();
        meta.owner = pending;
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        env.storage().instance().remove(&StorageKey::PendingOwner);
        env.events().publish(
            (Symbol::new(&env, "ownership_accepted"), old, meta.owner),
            (),
        );
        Ok(())
    }

    pub fn withdraw(env: Env, amount: i128) -> Result<i128, VaultError> {
        let mut meta = Self::get_meta(env.clone())?;
        meta.owner.require_auth();
        if amount <= 0 {
            return Err(VaultError::AmountNotPositive);
        }
        if meta.balance < amount {
            return Err(VaultError::InsufficientBalance);
        }
        let ua: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .ok_or(VaultError::NotInitialized)?;
        token::Client::new(&env, &ua).transfer(
            &env.current_contract_address(),
            &meta.owner,
            &amount,
        );
        meta.balance = meta.balance.checked_sub(amount).ok_or(VaultError::Overflow)?;
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.events().publish(
            (Symbol::new(&env, "withdraw"), meta.owner.clone()),
            (amount, meta.balance),
        );
        Ok(meta.balance)
    }

    pub fn withdraw_to(env: Env, to: Address, amount: i128) -> Result<i128, VaultError> {
        let mut meta = Self::get_meta(env.clone())?;
        meta.owner.require_auth();
        if amount <= 0 {
            return Err(VaultError::AmountNotPositive);
        }
        if meta.balance < amount {
            return Err(VaultError::InsufficientBalance);
        }
        let ua: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .ok_or(VaultError::NotInitialized)?;
        token::Client::new(&env, &ua).transfer(&env.current_contract_address(), &to, &amount);
        meta.balance = meta.balance.checked_sub(amount).ok_or(VaultError::Overflow)?;
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.events().publish(
            (Symbol::new(&env, "withdraw_to"), meta.owner.clone(), to.clone()),
            (amount, meta.balance),
        );
        Ok(meta.balance)
    }

    /// Distribute USDC from the vault to an arbitrary recipient (admin only).
    ///
    /// This function moves **untracked on-ledger surplus** — it checks the actual
    /// token balance, NOT `meta.balance`. Use this to recover funds that exist
    /// on-ledger but are not reflected in the vault's internal accounting.
    ///
    /// ## Pause Policy
    /// This function is **ALLOWED when paused**, matching the `withdraw` policy.
    /// Rationale: `distribute` is an emergency recovery tool for admins to move
    /// untracked surplus funds even during a circuit-breaker event.
    ///
    /// # Errors
    /// - `VaultError::Unauthorized` — caller is not the admin.
    /// - `VaultError::AmountNotPositive` — `amount <= 0`.
    /// - `VaultError::InsufficientBalance` — vault lacks on-ledger USDC for transfer.
    pub fn distribute(
        env: Env,
        caller: Address,
        to: Address,
        amount: i128,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone())?;
        if caller != admin {
            return Err(VaultError::Unauthorized);
        }
        if amount <= 0 {
            return Err(VaultError::AmountNotPositive);
        }
        let usdc_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .ok_or(VaultError::NotInitialized)?;
        let usdc = token::Client::new(&env, &usdc_addr);
        if usdc.balance(&env.current_contract_address()) < amount {
            return Err(VaultError::InsufficientBalance);
        }
        // CEI: emit event before external transfer
        env.events()
            .publish((Symbol::new(&env, "distribute"), to.clone()), amount);
        usdc.transfer(&env.current_contract_address(), &to, &amount);
        Ok(())
    }

    pub fn set_revenue_pool(
        env: Env,
        caller: Address,
        revenue_pool: Option<Address>,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone())?;
        if caller != admin {
            return Err(VaultError::Unauthorized);
        }
        match revenue_pool {
            Some(addr) => {
                env.storage()
                    .instance()
                    .set(&StorageKey::RevenuePool, &addr);
                env.events()
                    .publish((Symbol::new(&env, "set_revenue_pool"), caller), addr);
            }
            None => {
                env.storage().instance().remove(&StorageKey::RevenuePool);
                env.events()
                    .publish((Symbol::new(&env, "clear_revenue_pool"), caller), ());
            }
        }
        Ok(())
    }

    /// Store the settlement contract address (admin only).
    ///
    /// `deduct` and `batch_deduct` return error until this is called.
    pub fn set_settlement(
        env: Env,
        caller: Address,
        settlement_address: Address,
    ) -> Result<(), VaultError> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone())?;
        if caller != admin {
            return Err(VaultError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&StorageKey::Settlement, &settlement_address);
        env.events().publish(
            (Symbol::new(&env, "set_settlement"), caller),
            settlement_address,
        );
        Ok(())
    }

    pub fn set_metadata(
        env: Env,
        caller: Address,
        offering_id: String,
        metadata: String,
    ) -> Result<String, VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;
        if offering_id.len() > MAX_OFFERING_ID_LEN {
            return Err(VaultError::OfferingIdTooLong);
        }
        if metadata.len() > MAX_METADATA_LEN {
            return Err(VaultError::MetadataTooLong);
        }
        env.storage()
            .instance()
            .set(&StorageKey::Metadata(offering_id.clone()), &metadata);
        env.events().publish(
            (Symbol::new(&env, "metadata_set"), offering_id, caller),
            metadata.clone(),
        );
        Ok(metadata)
    }

    pub fn update_metadata(
        env: Env,
        caller: Address,
        offering_id: String,
        metadata: String,
    ) -> Result<String, VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;
        if offering_id.len() > MAX_OFFERING_ID_LEN {
            return Err(VaultError::OfferingIdTooLong);
        }
        if metadata.len() > MAX_METADATA_LEN {
            return Err(VaultError::MetadataTooLong);
        }
        let old: String = env
            .storage()
            .instance()
            .get(&StorageKey::Metadata(offering_id.clone()))
            .unwrap_or(String::from_str(&env, ""));
        env.storage()
            .instance()
            .set(&StorageKey::Metadata(offering_id.clone()), &metadata);
        env.events().publish(
            (Symbol::new(&env, "metadata_updated"), offering_id, caller),
            (old, metadata.clone()),
        );
        Ok(metadata)
    }

    /// Admin-gated contract upgrade.
    ///
    /// Only the current admin may call. This will instruct the host to update
    /// the current contract WASM to `new_wasm_hash` and persist the version marker.
    ///
    /// # Parameters
    /// - `caller` — must be the vault admin; signature required.
    /// - `new_wasm_hash` — 32-byte hash of the new WASM code to deploy.
    ///
    /// # Panics
    /// - `"unauthorized: caller is not admin"` — `caller` is not the admin.
    ///
    /// # Events
    /// Emits an `upgraded` event with the admin as topic and the new WASM hash as data.
    ///
    /// # Post-Upgrade Migration
    /// After calling `upgrade`, you may need to invoke a separate `migrate` function
    /// (if implemented in the new WASM) to update storage schema or perform data migrations.
    /// See UPGRADE.md for the complete operational flow.
    pub fn upgrade(env: Env, caller: Address, new_wasm_hash: BytesN<32>) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        assert!(
            caller == admin,
            "unauthorized: caller is not admin"
        );

        // Perform the on-chain upgrade via the deployer interface.
        // This is a host operation and may only succeed in the live environment.
        env.deployer().update_current_contract_wasm(new_wasm_hash.clone());

        // Persist the version marker for on-chain queries.
        env.storage()
            .instance()
            .set(&StorageKey::ContractVersion, &new_wasm_hash);

        // Emit an event for indexers / audit logs.
        env.events()
            .publish((Symbol::new(&env, "upgraded"), admin), new_wasm_hash);
    }

    /// Read the stored contract version (WASM hash) as last set by `upgrade`.
    ///
    /// Returns `None` if no upgrade has been performed yet (initial deployment).
    pub fn version(env: Env) -> Option<BytesN<32>> {
        env.storage()
            .instance()
            .get(&StorageKey::ContractVersion)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

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

    pub fn get_allowed_depositors(env: Env) -> Vec<Address> {
    fn transfer_funds(env: &Env, usdc_token: &Address, to: &Address, amount: i128) {
        token::Client::new(env, usdc_token).transfer(&env.current_contract_address(), to, &amount);
    }

    fn require_settlement(env: &Env) -> Result<Address, VaultError> {
        env.storage()
            .instance()
            .get(&StorageKey::Settlement)
            .ok_or(VaultError::SettlementNotSet)
    }

    fn require_not_paused(env: Env) -> Result<(), VaultError> {
        if Self::is_paused(env) {
            return Err(VaultError::Paused);
        }
        Ok(())
    }

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
}

// Allowlist aliases — convenience wrappers used by tests and external callers.
#[contractimpl]
impl CalloraVault {
    pub fn add_address(env: Env, caller: Address, depositor: Address) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;
        let mut list: Vec<Address> = env
            .storage()
            .instance()
            .get(&StorageKey::DepositorList)
            .unwrap_or(Vec::new(&env));
        if !list.contains(&depositor) {
            list.push_back(depositor.clone());
        }
        env.storage()
            .instance()
            .set(&StorageKey::DepositorList, &list);
        env.events()
            .publish((Symbol::new(&env, "allowlist_add"), caller, depositor), ());
        Ok(())
    }

    pub fn clear_all(env: Env, caller: Address) -> Result<(), VaultError> {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone())?;
        env.storage()
            .instance()
            .set(&StorageKey::DepositorList, &Vec::<Address>::new(&env));
        env.events()
            .publish((Symbol::new(&env, "allowlist_clear"), caller), ());
        Ok(())
    }

    pub fn get_allowlist(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&StorageKey::DepositorList)
            .unwrap_or(Vec::new(&env))
    }
}
