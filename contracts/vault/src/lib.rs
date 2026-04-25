#![no_std]
/// # Callora Vault Contract — deposit/withdraw/deduct/distribute with pause circuit-breaker.
///
/// ## Pause Circuit Breaker
///
/// When the vault is paused:
/// - Deposits are blocked
/// - Single and batch deducts are blocked
/// - Owner withdrawals are ALLOWED (emergency recovery)
/// - Admin/owner configuration functions remain available
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, String, Symbol, Vec};

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
    MaxDeduct,
    Paused,
    Metadata(String),
    PendingOwner,
    PendingAdmin,
    DepositorList,
}

pub const DEFAULT_MAX_DEDUCT: i128 = i128::MAX;
pub const DEFAULT_MIN_DEPOSIT: i128 = 1;
pub const MAX_BATCH_SIZE: u32 = 50;
pub const MAX_METADATA_LEN: u32 = 256;
pub const MAX_OFFERING_ID_LEN: u32 = 64;

#[contract]
pub struct CalloraVault;

#[contractimpl]
impl CalloraVault {
    /// Initialize the vault. Exactly-once; panics if called again.
    ///
    /// # Panics
    /// - `"vault already initialized"` — called more than once.
    /// - `"usdc_token cannot be vault address"` — self-referential token.
    /// - `"revenue_pool cannot be vault address"` — self-referential pool.
    /// - `"authorized_caller cannot be vault address"` — self-referential caller.
    /// - `"initial balance must be non-negative"` — negative initial balance.
    /// - `"min_deposit must be positive"` — `min_deposit <= 0`.
    /// - `"max_deduct must be positive"` — `max_deduct <= 0`.
    /// - `"min_deposit cannot exceed max_deduct"` — constraint violation.
    /// - `"initial_balance exceeds on-ledger USDC balance"` — vault underfunded.
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
    ) -> VaultMeta {
        owner.require_auth();
        let inst = env.storage().instance();
        if inst.has(&StorageKey::MetaKey) {
            panic!("vault already initialized");
        }
        assert!(
            usdc_token != env.current_contract_address(),
            "usdc_token cannot be vault address"
        );
        if let Some(p) = &revenue_pool {
            assert!(
                p != &env.current_contract_address(),
                "revenue_pool cannot be vault address"
            );
        }
        if let Some(ac) = &authorized_caller {
            assert!(
                ac != &env.current_contract_address(),
                "authorized_caller cannot be vault address"
            );
        }
        let balance = initial_balance.unwrap_or(0);
        assert!(balance >= 0, "initial balance must be non-negative");
        let min_d = min_deposit.unwrap_or(DEFAULT_MIN_DEPOSIT);
        assert!(min_d > 0, "min_deposit must be positive");
        let max_d = max_deduct.unwrap_or(DEFAULT_MAX_DEDUCT);
        assert!(max_d > 0, "max_deduct must be positive");
        assert!(min_d <= max_d, "min_deposit cannot exceed max_deduct");
        if balance > 0 {
            let on_chain =
                token::Client::new(&env, &usdc_token).balance(&env.current_contract_address());
            assert!(
                on_chain >= balance,
                "initial_balance exceeds on-ledger USDC balance"
            );
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
        env.events()
            .publish((Symbol::new(&env, "init"), owner.clone()), balance);
        meta
    }

    // -----------------------------------------------------------------------
    // View functions
    // -----------------------------------------------------------------------

    /// Return full vault state. Panics if vault is not initialized.
    pub fn get_meta(env: Env) -> VaultMeta {
        env.storage()
            .instance()
            .get(&StorageKey::MetaKey)
            .unwrap_or_else(|| panic!("vault not initialized"))
    }

    /// Return the current tracked USDC balance. Panics if vault is not initialized.
    pub fn balance(env: Env) -> i128 {
        Self::get_meta(env).balance
    }

    /// Return the current admin address. Panics if vault is not initialized.
    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .expect("vault not initialized")
    }

    /// Return the USDC token contract address. Panics if vault is not initialized.
    pub fn get_usdc_token(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .expect("vault not initialized")
    }

    /// Return the configured maximum single-deduction amount.
    /// Returns `i128::MAX` (no cap) if not explicitly set.
    pub fn get_max_deduct(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&StorageKey::MaxDeduct)
            .unwrap_or(DEFAULT_MAX_DEDUCT)
    }

    /// Return the configured settlement address.
    /// Panics with `"settlement address not set"` if `set_settlement` has not been called.
    pub fn get_settlement(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Settlement)
            .unwrap_or_else(|| panic!("settlement address not set"))
    }

    /// Return the configured revenue pool address, or `None` if not set.
    pub fn get_revenue_pool(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::RevenuePool)
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
    /// Panics if vault is not initialized.
    pub fn is_authorized_depositor(env: Env, caller: Address) -> bool {
        let meta = Self::get_meta(env.clone());
        if caller == meta.owner {
            return true;
        }
        let list: Vec<Address> = env
            .storage()
            .instance()
            .get(&StorageKey::DepositorList)
            .unwrap_or(Vec::new(&env));
        list.contains(&caller)
    }

    /// Return the current allowed-depositor list.
    pub fn get_allowed_depositors(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&StorageKey::DepositorList)
            .unwrap_or(Vec::new(&env))
    }

    /// Return stored offering metadata, or `None` if not set.
    pub fn get_metadata(env: Env, offering_id: String) -> Option<String> {
        env.storage()
            .instance()
            .get(&StorageKey::Metadata(offering_id))
    }

    // -----------------------------------------------------------------------
    // Mutating functions
    // -----------------------------------------------------------------------

    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let cur = Self::get_admin(env.clone());
        if caller != cur {
            panic!("unauthorized: caller is not admin");
        }
        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);
        env.events()
            .publish((Symbol::new(&env, "admin_nominated"), cur, new_admin), ());
    }

    pub fn accept_admin(env: Env) {
        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .expect("no admin transfer pending");
        pending.require_auth();
        let cur = Self::get_admin(env.clone());
        env.storage().instance().set(&StorageKey::Admin, &pending);
        env.storage().instance().remove(&StorageKey::PendingAdmin);
        env.events()
            .publish((Symbol::new(&env, "admin_accepted"), cur, pending), ());
    }

    pub fn require_owner(env: Env, caller: Address) {
        let meta = Self::get_meta(env.clone());
        assert!(caller == meta.owner, "unauthorized: owner only");
    }

    pub fn set_authorized_caller(env: Env, new_caller: Option<Address>) {
        let mut meta = Self::get_meta(env.clone());
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
    }

    pub fn set_allowed_depositor(env: Env, caller: Address, depositor: Option<Address>) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller);
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
    }

    pub fn clear_allowed_depositors(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller);
        env.storage()
            .instance()
            .set(&StorageKey::DepositorList, &Vec::<Address>::new(&env));
    }

    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin_or_owner(env.clone(), &caller);
        assert!(!Self::is_paused(env.clone()), "vault already paused");
        env.storage().instance().set(&StorageKey::Paused, &true);
        env.events()
            .publish((Symbol::new(&env, "vault_paused"), caller), ());
    }

    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_admin_or_owner(env.clone(), &caller);
        assert!(Self::is_paused(env.clone()), "vault not paused");
        env.storage().instance().set(&StorageKey::Paused, &false);
        env.events()
            .publish((Symbol::new(&env, "vault_unpaused"), caller), ());
    }

    pub fn deposit(env: Env, caller: Address, amount: i128) -> i128 {
        Self::require_not_paused(env.clone());
        caller.require_auth();
        assert!(amount > 0, "amount must be positive");
        assert!(
            Self::is_authorized_depositor(env.clone(), caller.clone()),
            "unauthorized: only owner or allowed depositor can deposit"
        );
        let mut meta = Self::get_meta(env.clone());
        assert!(
            amount >= meta.min_deposit,
            "deposit below minimum: {} < {}",
            amount,
            meta.min_deposit
        );
        let usdc_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .expect("vault not initialized");
        token::Client::new(&env, &usdc_addr).transfer(
            &caller,
            &env.current_contract_address(),
            &amount,
        );
        meta.balance = meta
            .balance
            .checked_add(amount)
            .unwrap_or_else(|| panic!("balance overflow"));
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        env.events().publish(
            (Symbol::new(&env, "deposit"), caller),
            (amount, meta.balance),
        );
        meta.balance
    }

    /// Deduct USDC for a single API call and forward to the settlement address.
    ///
    /// # Panics
    /// - `"settlement address not set"` — `set_settlement` has not been called.
    /// - `"amount must be positive"` — `amount <= 0`.
    /// - `"deduct amount exceeds max_deduct"` — `amount > max_deduct`.
    /// - `"unauthorized caller"` — caller is not owner or authorized_caller.
    /// - `"insufficient balance"` — vault balance below `amount`.
    pub fn deduct(env: Env, caller: Address, amount: i128, request_id: Option<Symbol>) -> i128 {
        Self::require_not_paused(env.clone());
        caller.require_auth();
        assert!(amount > 0, "amount must be positive");
        let max_d = Self::get_max_deduct(env.clone());
        assert!(amount <= max_d, "deduct amount exceeds max_deduct");
        Self::require_authorized_deduct_caller(env.clone(), &caller);
        let mut meta = Self::get_meta(env.clone());
        assert!(meta.balance >= amount, "insufficient balance");
        let settlement = Self::require_settlement(&env);
        meta.balance = meta
            .balance
            .checked_sub(amount)
            .unwrap_or_else(|| panic!("balance underflow"));
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        let ut: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .unwrap();
        Self::transfer_funds(&env, &ut, &settlement, amount);
        let rid = request_id.unwrap_or(Symbol::new(&env, ""));
        env.events().publish(
            (Symbol::new(&env, "deduct"), caller, rid),
            (amount, meta.balance),
        );
        meta.balance
    }

    /// Atomically deduct multiple items. Full validation before any state write.
    pub fn batch_deduct(env: Env, caller: Address, items: Vec<DeductItem>) -> i128 {
        Self::require_not_paused(env.clone());
        caller.require_auth();
        let n = items.len();
        assert!(n > 0, "batch_deduct requires at least one item");
        assert!(n <= MAX_BATCH_SIZE, "batch too large");
        let max_d = Self::get_max_deduct(env.clone());
        Self::require_authorized_deduct_caller(env.clone(), &caller);
        let mut meta = Self::get_meta(env.clone());
        let mut running = meta.balance;
        let mut total: i128 = 0;
        for item in items.iter() {
            assert!(item.amount > 0, "amount must be positive");
            assert!(item.amount <= max_d, "deduct amount exceeds max_deduct");
            assert!(running >= item.amount, "insufficient balance");
            running = running
                .checked_sub(item.amount)
                .unwrap_or_else(|| panic!("balance underflow"));
            total = total
                .checked_add(item.amount)
                .unwrap_or_else(|| panic!("total overflow"));
        }
        let settlement = Self::require_settlement(&env);
        meta.balance = running;
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        let ut: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .unwrap();
        Self::transfer_funds(&env, &ut, &settlement, total);
        // Emit one event per item after state is committed.
        let mut running_balance = meta.balance + total;
        for item in items.iter() {
            running_balance -= item.amount;
            let rid = item.request_id.clone().unwrap_or(Symbol::new(&env, ""));
            env.events().publish(
                (Symbol::new(&env, "deduct"), caller.clone(), rid),
                (item.amount, running_balance),
            );
        }
        meta.balance
    }

    pub fn withdraw(env: Env, amount: i128) -> i128 {
        let mut meta = Self::get_meta(env.clone());
        meta.owner.require_auth();
        assert!(amount > 0, "amount must be positive");
        assert!(meta.balance >= amount, "insufficient balance");
        let ua: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .expect("vault not initialized");
        token::Client::new(&env, &ua).transfer(
            &env.current_contract_address(),
            &meta.owner,
            &amount,
        );
        meta.balance = meta
            .balance
            .checked_sub(amount)
            .unwrap_or_else(|| panic!("balance underflow"));
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        env.events().publish(
            (Symbol::new(&env, "withdraw"), meta.owner.clone()),
            WithdrawEventData {
                amount,
                new_balance: meta.balance,
            },
        );
        meta.balance
    }

    pub fn withdraw_to(env: Env, to: Address, amount: i128) -> i128 {
        let mut meta = Self::get_meta(env.clone());
        meta.owner.require_auth();
        assert!(amount > 0, "amount must be positive");
        assert!(meta.balance >= amount, "insufficient balance");
        let ua: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .expect("vault not initialized");
        token::Client::new(&env, &ua).transfer(&env.current_contract_address(), &to, &amount);
        meta.balance = meta
            .balance
            .checked_sub(amount)
            .unwrap_or_else(|| panic!("balance underflow"));
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        env.events().publish(
            (Symbol::new(&env, "withdraw_to"), meta.owner.clone(), to),
            WithdrawEventData {
                amount,
                new_balance: meta.balance,
            },
        );
        meta.balance
    }

    pub fn distribute(env: Env, caller: Address, to: Address, amount: i128) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
        }
        if amount <= 0 {
            panic!("amount must be positive");
        }
        let usdc_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .expect("vault not initialized");
        let usdc = token::Client::new(&env, &usdc_addr);
        if usdc.balance(&env.current_contract_address()) < amount {
            panic!("insufficient USDC balance");
        }
        usdc.transfer(&env.current_contract_address(), &to, &amount);
        env.events()
            .publish((Symbol::new(&env, "distribute"), to), amount);
    }

    pub fn transfer_ownership(env: Env, new_owner: Address) {
        let meta = Self::get_meta(env.clone());
        meta.owner.require_auth();
        assert!(
            new_owner != meta.owner,
            "new_owner must be different from current owner"
        );
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
    }

    pub fn accept_ownership(env: Env) {
        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingOwner)
            .expect("no ownership transfer pending");
        pending.require_auth();
        let mut meta = Self::get_meta(env.clone());
        let old = meta.owner.clone();
        meta.owner = pending;
        env.storage().instance().set(&StorageKey::MetaKey, &meta);
        env.storage().instance().remove(&StorageKey::PendingOwner);
        env.events().publish(
            (Symbol::new(&env, "ownership_accepted"), old, meta.owner),
            (),
        );
    }

    pub fn set_revenue_pool(env: Env, caller: Address, revenue_pool: Option<Address>) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
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
    }

    /// Store the settlement contract address (admin only).
    /// `deduct` and `batch_deduct` panic with `"settlement address not set"` until called.
    pub fn set_settlement(env: Env, caller: Address, settlement_address: Address) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
        }
        env.storage()
            .instance()
            .set(&StorageKey::Settlement, &settlement_address);
        env.events().publish(
            (Symbol::new(&env, "set_settlement"), caller),
            settlement_address,
        );
    }

    pub fn set_metadata(
        env: Env,
        caller: Address,
        offering_id: String,
        metadata: String,
    ) -> String {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone());
        assert!(
            offering_id.len() <= MAX_OFFERING_ID_LEN,
            "offering_id exceeds max length"
        );
        assert!(
            metadata.len() <= MAX_METADATA_LEN,
            "metadata exceeds max length"
        );
        env.storage()
            .instance()
            .set(&StorageKey::Metadata(offering_id.clone()), &metadata);
        env.events().publish(
            (Symbol::new(&env, "metadata_set"), offering_id, caller),
            metadata.clone(),
        );
        metadata
    }

    pub fn update_metadata(
        env: Env,
        caller: Address,
        offering_id: String,
        metadata: String,
    ) -> String {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone());
        assert!(
            offering_id.len() <= MAX_OFFERING_ID_LEN,
            "offering_id exceeds max length"
        );
        assert!(
            metadata.len() <= MAX_METADATA_LEN,
            "metadata exceeds max length"
        );
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
        metadata
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    fn require_authorized_deduct_caller(env: Env, caller: &Address) {
        let meta = Self::get_meta(env.clone());
        let auth = match &meta.authorized_caller {
            Some(ac) => caller == ac || *caller == meta.owner,
            None => *caller == meta.owner,
        };
        assert!(auth, "unauthorized caller");
    }

    fn transfer_funds(env: &Env, usdc_token: &Address, to: &Address, amount: i128) {
        token::Client::new(env, usdc_token).transfer(&env.current_contract_address(), to, &amount);
    }

    fn require_settlement(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Settlement)
            .unwrap_or_else(|| panic!("settlement address not set"))
    }

    fn require_not_paused(env: Env) {
        assert!(!Self::is_paused(env), "vault is paused");
    }

    fn require_admin_or_owner(env: Env, caller: &Address) {
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .expect("vault not initialized");
        let meta = Self::get_meta(env);
        assert!(
            *caller == admin || *caller == meta.owner,
            "unauthorized: caller is not admin or owner"
        );
    }

    #[allow(dead_code)]
    fn migrate(env: &Env) {
        let inst = env.storage().instance();
        if !inst.has(&StorageKey::Admin) {
            if let Some(meta) = inst.get::<_, VaultMeta>(&StorageKey::MetaKey) {
                inst.set(&StorageKey::Admin, &meta.owner);
            }
        }
    }
}

// Allowlist aliases used by existing tests.
#[contractimpl]
impl CalloraVault {
    pub fn add_address(env: Env, caller: Address, depositor: Address) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone());
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
    }

    pub fn clear_all(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone());
        env.storage()
            .instance()
            .set(&StorageKey::DepositorList, &Vec::<Address>::new(&env));
        env.events()
            .publish((Symbol::new(&env, "allowlist_clear"), caller), ());
    }

    pub fn get_allowlist(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&StorageKey::DepositorList)
            .unwrap_or(Vec::new(&env))
    }
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_init_hardening;

#[cfg(test)]
mod test_views;
