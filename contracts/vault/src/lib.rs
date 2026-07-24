#![allow(clippy::too_many_arguments)]
#![no_std]
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

mod errors;
pub use errors::VaultError;

/// Instance storage bump constants.
pub const INSTANCE_BUMP_AMOUNT: u32 = 50000;
pub const INSTANCE_BUMP_THRESHOLD: u32 = 50000;

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
}

/// Persistent / instance storage keys for the vault.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    UsdcToken,
    ProcessedRequest(Symbol),
    ReserveCap(Address),
    DeveloperConfig(Address),
    DeveloperState(Address),
    AuthorizedCallerNonce,
}

pub mod token {
    pub use soroban_sdk::token::Client;
}

#[cfg(target_arch = "wasm32")]
pub mod settlement {
    soroban_sdk::contractimport!(
        file = "../../target/wasm32-unknown-unknown/release/callora_settlement.wasm"
    );
}

/// In native/test mode, vault calls to settlement are no-ops since the settlement
/// contract is registered directly in the test `Env` and credits are verified
/// through its public interface.
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
        pub fn record_deduction(&self, _amount: &i128, _request_id: &u64) {}
    }
}

#[contract]
pub struct CalloraVault;

#[contractimpl]
impl CalloraVault {
    pub fn init(
        env: Env,
        owner: Address,
        usdc_token: Address,
        initial_balance: i128,
        authorized_caller: Address,
        min_deposit: i128,
        revenue_pool: Option<Address>,
        max_deduct: i128,
        settlement: Address,
    ) {
        if env.storage().instance().has(&DataKey::Owner) {
            panic!("Already initialized");
        }
        if min_deposit <= 0 {
            panic!("Invalid min deposit");
        }
        if max_deduct <= 0 {
            panic!("Invalid max deduct");
        }
        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage()
            .instance()
            .set(&DataKey::UsdcToken, &usdc_token);
        env.storage()
            .instance()
            .set(&DataKey::Balance, &initial_balance);
        env.storage()
            .instance()
            .set(&DataKey::AuthorizedCaller, &authorized_caller);
        env.storage()
            .instance()
            .set(&DataKey::MinDeposit, &min_deposit);
        if let Some(pool) = revenue_pool {
            env.storage().instance().set(&DataKey::RevenuePool, &pool);
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxDeduct, &max_deduct);
        env.storage()
            .instance()
            .set(&DataKey::Settlement, &settlement);
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    pub fn deposit(env: Env, caller: Address, amount: i128) {
        caller.require_auth();
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("Contract paused");
        }
        let min_dep = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MinDeposit)
            .unwrap();
        if amount < min_dep {
            panic!("Deposit under minimum");
        }
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            let is_allowed = env
                .storage()
                .instance()
                .get::<_, bool>(&DataKey::Depositor(caller.clone()))
                .unwrap_or(false);
            if !is_allowed {
                panic!("Not authorized depositor");
            }
        }
        let current_bal = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0);
        let new_bal = current_bal.checked_add(amount).unwrap();
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        let token_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .unwrap();
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&caller, &env.current_contract_address(), &amount);
    }

    pub fn deduct(env: Env, caller: Address, amount: i128, request_id: u64) {
        caller.require_auth();
        let auth_caller = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::AuthorizedCaller)
            .unwrap();
        if caller != auth_caller {
            panic!("Not authorized caller");
        }
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("Contract paused");
        }
        let max_deduct = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap();
        if amount > max_deduct || amount <= 0 {
            panic!("Invalid deduct amount");
        }
        let current_bal = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0);
        let new_bal = current_bal.checked_sub(amount).expect("deduct: underflow");
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        // Transfer USDC from vault to settlement on-ledger.
        let usdc_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .unwrap();
        let usdc = token::Client::new(&env, &usdc_addr);
        let settlement_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .unwrap();
        usdc.transfer(&env.current_contract_address(), &settlement_addr, &amount);
        let settlement_client = settlement::Client::new(&env, &settlement_addr);
        settlement_client.record_deduction(&amount, &request_id);
    }

    pub fn batch_deduct(env: Env, caller: Address, items: Vec<(i128, u64)>) {
        caller.require_auth();
        let auth_caller = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::AuthorizedCaller)
            .unwrap();
        if caller != auth_caller {
            panic!("Not authorized caller");
        }
        if env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
        {
            panic!("Contract paused");
        }
        let max_deduct = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap();
        let mut total_amount: i128 = 0;
        for item in items.iter() {
            let (amount, _) = item;
            if amount > max_deduct || amount <= 0 {
                panic!("Invalid deduct amount");
            }
            total_amount = total_amount.checked_add(amount).unwrap();
        }
        let current_bal = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap_or(0);
        let new_bal = current_bal.checked_sub(total_amount).unwrap();
        env.storage().instance().set(&DataKey::Balance, &new_bal);
        // Transfer total USDC from vault to settlement on-ledger atomically.
        let usdc_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .unwrap();
        let usdc = token::Client::new(&env, &usdc_addr);
        let settlement_addr = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Settlement)
            .unwrap();
        usdc.transfer(&env.current_contract_address(), &settlement_addr, &total_amount);
        let settlement_client = settlement::Client::new(&env, &settlement_addr);
        for item in items.iter() {
            let (amount, request_id) = item;
            settlement_client.record_deduction(&amount, &request_id);
        }
    }

    // -----------------------------------------------------------------------
    // View functions — no TTL bump (read-only, zero write cost)
    // -----------------------------------------------------------------------

    /// Simulates a vault deduction without altering on-chain state.
    ///
    /// Performs validation checks identical to `deduct` and returns the predicted
    /// balance after the specified `amount` is deducted.
    ///
    /// # Errors
    /// Returns `VaultError` under the exact same conditions as `deduct`
    /// (e.g., paused state, amount exceeding balance, amount exceeding max deduction limit).
    pub fn simulate_deduct(
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
        if let Some(ref rid) = request_id {
            Self::require_not_duplicate(&env, rid)?;
        }
        let meta = Self::get_meta(env.clone())?;
        if meta.balance < amount {
            return Err(VaultError::InsufficientBalance);
        }
        let _ = Self::require_settlement(&env)?;
        meta.balance
            .checked_sub(amount)
            .ok_or(VaultError::Overflow)
    }

    /// Simulates a batch vault deduction without altering on-chain state.
    ///
    /// Performs validation checks identical to `batch_deduct` and returns the predicted
    /// balance after all specified deductions are applied.
    ///
    /// # Errors
    /// Returns `VaultError` under the exact same conditions as `batch_deduct`.
    pub fn simulate_batch_deduct(
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
        let meta = Self::get_meta(env.clone())?;
        let mut running = meta.balance;
        let mut seen_in_batch: Vec<Symbol> = Vec::new(&env);
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
            if let Some(ref rid) = item.request_id {
                Self::require_not_duplicate(&env, rid)?;
                if seen_in_batch.contains(rid) {
                    return Err(VaultError::DuplicateRequestId);
                }
                seen_in_batch.push_back(rid.clone());
            }
            running = running.checked_sub(item.amount).ok_or(VaultError::Overflow)?;
        }
        let _ = Self::require_settlement(&env)?;
        Ok(running)
    }

    /// Return full vault state. Returns error if vault is not initialized.
    pub fn get_meta(env: Env) -> Result<VaultMeta, VaultError> {
        env.storage()
            .instance()
            .set(&DataKey::Depositor(depositor), &true);
    }

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
        env.storage()
            .instance()
            .set(&DataKey::AuthorizedCaller, &caller);
    }

    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        env.storage().instance().set(&DataKey::Paused, &true);
    }

    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        env.storage().instance().set(&DataKey::Paused, &false);
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Paused)
            .unwrap_or(false)
    }
    pub fn balance(env: Env) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::Balance)
            .unwrap()
    }
    pub fn get_owner(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap()
    }

    pub(crate) fn require_owner(env: Env, caller: Address) -> Result<(), VaultError> {
        let owner = Self::get_owner(env);
        if caller != owner {
            return Err(VaultError::Unauthorized);
        }
        Ok(())
    }
    pub fn get_usdc_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get::<_, Address>(&DataKey::UsdcToken)
            .unwrap()
    }
    pub fn get_max_deduct(env: Env) -> i128 {
        env.storage()
            .instance()
            .get::<_, i128>(&DataKey::MaxDeduct)
            .unwrap_or(i128::MAX)
    }

    pub fn set_max_deduct(env: Env, caller: Address, max_deduct: i128) {
        caller.require_auth();
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .unwrap();
        if caller != owner {
            panic!("Not owner");
        }
        if max_deduct <= 0 {
            panic!("Invalid max deduct");
        }
        env.storage()
            .instance()
            .set(&DataKey::MaxDeduct, &max_deduct);
    }

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

    /// Garbage-collect processed request markers from persistent storage.
    /// Only the owner can call this.
    /// Emits a `request_id_pruned` event for each removed ID.
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

    pub fn is_authorized_depositor(env: Env, caller: Address) -> bool {
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
        Ok(())
    }

    /// Return the reserve cap for `token`.
    ///
    /// Returns `i128::MAX` when no cap has been configured (effectively unlimited).
    pub fn get_reserve_cap(env: Env, token: Address) -> i128 {
        limits::get(&env, &token)
    }

    /// Internal helper: require that `caller` is the vault owner.
    fn require_owner(env: Env, caller: Address) -> Result<(), VaultError> {
        let owner = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Owner)
            .ok_or(VaultError::NotInitialized)?;
        if caller != owner {
            return Err(VaultError::Unauthorized);
        }
        Ok(())
    }
}

pub mod capabilities;
mod cold_storage;
pub mod events;
pub mod capabilities;
pub mod rate_limit;
pub mod limits;
pub mod rate_limit;

#[cfg(test)]
#[path = "../proofs/deduct.rs"]
mod deduct_proofs;

// ---------------------------------------------------------------------------
// Test modules
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;

// NOTE: The following test modules expect a richer contract API (DeductItem,
// Option<Symbol> request IDs, get_meta, DEFAULT_MIN_DEPOSIT, etc.) that the
// current simplified vault does not expose. They are commented out until the
// vault API is migrated.
//
// #[cfg(test)]
// mod test_settler_validation;

#[cfg(test)]
mod test_views;

#[cfg(test)]
mod test_idempotency;

#[cfg(test)]
mod test_error_codes;

#[cfg(test)]
mod test_reentrancy;

#[cfg(test)]
mod test_balance_property;

#[cfg(test)]
mod test_gas_budget;
#[cfg(test)]
mod test_rate_limit;
