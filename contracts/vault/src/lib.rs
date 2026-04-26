#![no_std]
/// # Callora Vault Contract
///
/// Deposit, withdraw, deduct, and distribute with a pause circuit breaker.
///
/// ## Pause Circuit Breaker
///
/// When the vault is paused:
/// - Deposits are blocked.
/// - Single deducts are blocked.
/// - Batch deducts are blocked.
/// - Owner withdrawals remain available for recovery.
/// - Admin and owner configuration functions remain available.
///
/// This design allows the vault owner to recover funds while preventing new
/// deposits and deductions during emergency situations or upgrades.
use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env, String, Symbol, Vec};

const EMPTY_REQUEST_ID: &str = "";
const ERR_DUPLICATE_REQUEST_ID: &str = "duplicate request_id";

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
    /// Amount withdrawn in USDC stroops.
    pub amount: i128,
    /// Vault balance after the withdrawal.
    pub new_balance: i128,
}

/// Canonical storage keys for the Vault contract.
#[contracttype]
pub enum StorageKey {
    Meta,
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
    ProcessedRequest(Symbol),
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
        if inst.has(&StorageKey::Meta) {
            panic!("vault already initialized");
        }

        assert!(
            usdc_token != env.current_contract_address(),
            "usdc_token cannot be vault address"
        );

        if let Some(pool) = &revenue_pool {
            assert!(
                pool != &env.current_contract_address(),
                "revenue_pool cannot be vault address"
            );
        }

        if let Some(caller) = &authorized_caller {
            assert!(
                caller != &env.current_contract_address(),
                "authorized_caller cannot be vault address"
            );
        }

        let balance = initial_balance.unwrap_or(0);
        assert!(balance >= 0, "initial balance must be non-negative");

        let min_deposit = min_deposit.unwrap_or(DEFAULT_MIN_DEPOSIT);
        assert!(min_deposit > 0, "min_deposit must be positive");

        let max_deduct = max_deduct.unwrap_or(DEFAULT_MAX_DEDUCT);
        assert!(max_deduct > 0, "max_deduct must be positive");
        assert!(
            min_deposit <= max_deduct,
            "min_deposit cannot exceed max_deduct"
        );

        if balance > 0 {
            let onchain_usdc_balance =
                token::Client::new(&env, &usdc_token).balance(&env.current_contract_address());
            assert!(
                onchain_usdc_balance >= balance,
                "initial_balance exceeds on-ledger USDC balance"
            );
        }

        let meta = VaultMeta {
            owner: owner.clone(),
            balance,
            authorized_caller,
            min_deposit,
        };

        inst.set(&StorageKey::Meta, &meta);
        inst.set(&StorageKey::UsdcToken, &usdc_token);
        inst.set(&StorageKey::Admin, &owner);
        inst.set(&StorageKey::MaxDeduct, &max_deduct);

        if let Some(pool) = revenue_pool {
            inst.set(&StorageKey::RevenuePool, &pool);
        }

        env.events()
            .publish((Symbol::new(&env, "init"), owner.clone()), balance);

        meta
    }

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

    pub fn get_admin(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .expect("vault not initialized")
    }

    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();

        let current_admin = Self::get_admin(env.clone());
        if caller != current_admin {
            panic!("unauthorized: caller is not admin");
        }

        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);

        env.events().publish(
            (
                Symbol::new(&env, "admin_nominated"),
                current_admin,
                new_admin,
            ),
            (),
        );
    }

    pub fn accept_admin(env: Env) {
        let pending_admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .expect("no admin transfer pending");
        pending_admin.require_auth();

        let previous_admin = Self::get_admin(env.clone());
        env.storage()
            .instance()
            .set(&StorageKey::Admin, &pending_admin);
        env.storage().instance().remove(&StorageKey::PendingAdmin);

        env.events().publish(
            (
                Symbol::new(&env, "admin_accepted"),
                previous_admin,
                pending_admin,
            ),
            (),
        );
    }

    pub fn require_owner(env: Env, caller: Address) {
        let meta = Self::get_meta(env);
        assert!(caller == meta.owner, "unauthorized: owner only");
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

    pub fn get_meta(env: Env) -> VaultMeta {
        env.storage()
            .instance()
            .get(&StorageKey::Meta)
            .unwrap_or_else(|| panic!("vault not initialized"))
    }

    pub fn set_allowed_depositor(env: Env, caller: Address, depositor: Option<Address>) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone());

        match depositor {
            Some(depositor) => {
                let mut list: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&StorageKey::DepositorList)
                    .unwrap_or(Vec::new(&env));

                if !list.contains(&depositor) {
                    list.push_back(depositor.clone());
                    env.storage()
                        .instance()
                        .set(&StorageKey::DepositorList, &list);
                    env.events().publish(
                        (
                            Symbol::new(&env, "allowlist_add"),
                            caller,
                            depositor.clone(),
                        ),
                        (),
                    );
                }
            }
            None => {
                env.storage()
                    .instance()
                    .set(&StorageKey::DepositorList, &Vec::<Address>::new(&env));
                env.events()
                    .publish((Symbol::new(&env, "allowlist_clear"), caller), ());
            }
        }
    }

    pub fn clear_allowed_depositors(env: Env, caller: Address) {
        caller.require_auth();
        Self::require_owner(env.clone(), caller.clone());

        env.storage()
            .instance()
            .set(&StorageKey::DepositorList, &Vec::<Address>::new(&env));
        env.events()
            .publish((Symbol::new(&env, "allowlist_clear"), caller), ());
    }

    /// Backward-compatible alias for legacy tests and callers.
    pub fn add_address(env: Env, caller: Address, depositor: Address) {
        Self::set_allowed_depositor(env, caller, Some(depositor));
    }

    /// Backward-compatible alias for legacy tests and callers.
    pub fn get_allowlist(env: Env) -> Vec<Address> {
        Self::get_allowed_depositors(env)
    }

    /// Backward-compatible alias for legacy tests and callers.
    pub fn clear_all(env: Env, caller: Address) {
        Self::clear_allowed_depositors(env, caller);
    }

    pub fn get_allowed_depositors(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&StorageKey::DepositorList)
            .unwrap_or(Vec::new(&env))
    }

    pub fn set_authorized_caller(env: Env, caller: Option<Address>) {
        let mut meta = Self::get_meta(env.clone());
        meta.owner.require_auth();

        if let Some(new_caller) = &caller {
            assert!(
                new_caller != &env.current_contract_address(),
                "authorized_caller cannot be vault address"
            );
        }

        let old_authorized_caller = meta.authorized_caller.clone();
        meta.authorized_caller = caller.clone();
        env.storage().instance().set(&StorageKey::Meta, &meta);

        env.events().publish(
            (
                Symbol::new(&env, "set_authorized_caller"),
                meta.owner.clone(),
            ),
            (old_authorized_caller, caller),
        );
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

    /// Returns the current pause state of the vault.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&StorageKey::Paused)
            .unwrap_or(false)
    }

    pub fn get_max_deduct(env: Env) -> i128 {
        env.storage()
            .instance()
            .get(&StorageKey::MaxDeduct)
            .unwrap_or(DEFAULT_MAX_DEDUCT)
    }

    pub fn deposit(env: Env, caller: Address, amount: i128) -> i128 {
        Self::require_not_paused(env.clone());
        caller.require_auth();

        assert!(amount > 0, "amount must be positive");
        assert!(
            Self::is_authorized_depositor(env.clone(), caller.clone()),
            "unauthorized: only owner or allowed depositor can deposit"
        );

        let meta = Self::get_meta(env.clone());
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
        let usdc = token::Client::new(&env, &usdc_addr);
        usdc.transfer(&caller, &env.current_contract_address(), &amount);

        let mut meta = Self::get_meta(env.clone());
        meta.balance = meta
            .balance
            .checked_add(amount)
            .unwrap_or_else(|| panic!("balance overflow"));
        env.storage().instance().set(&StorageKey::Meta, &meta);

        env.events().publish(
            (Symbol::new(&env, "deposit"), caller),
            (amount, meta.balance),
        );

        meta.balance
    }

    /// Deduct USDC from the vault and transfer it to the configured settlement
    /// address.
    ///
    /// If `request_id` is provided it is treated as an idempotency key and may be
    /// used exactly once across successful `deduct` and `batch_deduct` calls.
    pub fn deduct(env: Env, caller: Address, amount: i128, request_id: Option<Symbol>) -> i128 {
        Self::require_not_paused(env.clone());
        caller.require_auth();
        Self::require_authorized_deduct_caller(env.clone(), &caller);

        assert!(amount > 0, "amount must be positive");
        assert!(
            amount <= Self::get_max_deduct(env.clone()),
            "deduct amount exceeds max_deduct"
        );

        let mut meta = Self::get_meta(env.clone());
        assert!(meta.balance >= amount, "insufficient balance");

        if let Some(request_id) = &request_id {
            Self::assert_request_id_available(&env, request_id);
        }

        let settlement = Self::require_settlement(&env);
        let usdc_token: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .expect("vault not initialized");

        meta.balance = meta
            .balance
            .checked_sub(amount)
            .unwrap_or_else(|| panic!("balance underflow"));
        env.storage().instance().set(&StorageKey::Meta, &meta);

        if let Some(request_id) = &request_id {
            Self::mark_request_id_processed(&env, request_id);
        }

        Self::transfer_funds(&env, &usdc_token, &settlement, amount);

        let event_request_id = request_id.unwrap_or(Symbol::new(&env, EMPTY_REQUEST_ID));
        env.events().publish(
            (Symbol::new(&env, "deduct"), caller, event_request_id),
            (amount, meta.balance),
        );

        meta.balance
    }

    /// Deduct multiple items atomically.
    ///
    /// Full validation, including duplicate `request_id` rejection, is completed
    /// before any state mutation or external transfer.
    pub fn batch_deduct(env: Env, caller: Address, items: Vec<DeductItem>) -> i128 {
        Self::require_not_paused(env.clone());
        caller.require_auth();
        Self::require_authorized_deduct_caller(env.clone(), &caller);

        let item_count = items.len();
        assert!(item_count > 0, "batch_deduct requires at least one item");
        assert!(item_count <= MAX_BATCH_SIZE, "batch too large");

        let max_deduct = Self::get_max_deduct(env.clone());
        let mut meta = Self::get_meta(env.clone());
        let mut remaining_balance = meta.balance;
        let mut total: i128 = 0;
        let mut seen_request_ids = Vec::<Symbol>::new(&env);

        for item in items.iter() {
            assert!(item.amount > 0, "amount must be positive");
            assert!(
                item.amount <= max_deduct,
                "deduct amount exceeds max_deduct"
            );
            assert!(remaining_balance >= item.amount, "insufficient balance");

            remaining_balance = remaining_balance
                .checked_sub(item.amount)
                .unwrap_or_else(|| panic!("balance underflow"));
            total = total
                .checked_add(item.amount)
                .unwrap_or_else(|| panic!("total overflow"));

            if let Some(request_id) = &item.request_id {
                assert!(
                    !seen_request_ids.contains(request_id),
                    "{}",
                    ERR_DUPLICATE_REQUEST_ID
                );
                Self::assert_request_id_available(&env, request_id);
                seen_request_ids.push_back(request_id.clone());
            }
        }

        let settlement = Self::require_settlement(&env);
        let usdc_token: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .expect("vault not initialized");

        meta.balance = remaining_balance;
        env.storage().instance().set(&StorageKey::Meta, &meta);

        for item in items.iter() {
            if let Some(request_id) = &item.request_id {
                Self::mark_request_id_processed(&env, request_id);
            }
        }

        Self::transfer_funds(&env, &usdc_token, &settlement, total);

        let mut event_balance = Self::get_meta(env.clone())
            .balance
            .checked_add(total)
            .unwrap_or_else(|| panic!("balance overflow"));
        for item in items.iter() {
            event_balance = event_balance
                .checked_sub(item.amount)
                .unwrap_or_else(|| panic!("balance underflow"));
            let event_request_id = item
                .request_id
                .clone()
                .unwrap_or(Symbol::new(&env, EMPTY_REQUEST_ID));
            env.events().publish(
                (
                    Symbol::new(&env, "deduct"),
                    caller.clone(),
                    event_request_id,
                ),
                (item.amount, event_balance),
            );
        }

        meta.balance
    }

    pub fn balance(env: Env) -> i128 {
        Self::get_meta(env).balance
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
        let pending_owner: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingOwner)
            .expect("no ownership transfer pending");
        pending_owner.require_auth();

        let mut meta = Self::get_meta(env.clone());
        let previous_owner = meta.owner.clone();
        meta.owner = pending_owner;
        env.storage().instance().set(&StorageKey::Meta, &meta);
        env.storage().instance().remove(&StorageKey::PendingOwner);

        env.events().publish(
            (
                Symbol::new(&env, "ownership_accepted"),
                previous_owner,
                meta.owner,
            ),
            (),
        );
    }

    pub fn withdraw(env: Env, amount: i128) -> i128 {
        let mut meta = Self::get_meta(env.clone());
        meta.owner.require_auth();

        assert!(amount > 0, "amount must be positive");
        assert!(meta.balance >= amount, "insufficient balance");

        let usdc_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .expect("vault not initialized");
        let usdc = token::Client::new(&env, &usdc_addr);
        usdc.transfer(&env.current_contract_address(), &meta.owner, &amount);

        meta.balance = meta
            .balance
            .checked_sub(amount)
            .unwrap_or_else(|| panic!("balance underflow"));
        env.storage().instance().set(&StorageKey::Meta, &meta);

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

        let usdc_addr: Address = env
            .storage()
            .instance()
            .get(&StorageKey::UsdcToken)
            .expect("vault not initialized");
        let usdc = token::Client::new(&env, &usdc_addr);
        usdc.transfer(&env.current_contract_address(), &to, &amount);

        meta.balance = meta
            .balance
            .checked_sub(amount)
            .unwrap_or_else(|| panic!("balance underflow"));
        env.storage().instance().set(&StorageKey::Meta, &meta);

        env.events().publish(
            (Symbol::new(&env, "withdraw_to"), meta.owner.clone(), to),
            WithdrawEventData {
                amount,
                new_balance: meta.balance,
            },
        );

        meta.balance
    }

    pub fn set_revenue_pool(env: Env, caller: Address, revenue_pool: Option<Address>) {
        caller.require_auth();

        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
        }

        match revenue_pool {
            Some(revenue_pool) => {
                env.storage()
                    .instance()
                    .set(&StorageKey::RevenuePool, &revenue_pool);
                env.events().publish(
                    (Symbol::new(&env, "set_revenue_pool"), caller),
                    revenue_pool,
                );
            }
            None => {
                env.storage().instance().remove(&StorageKey::RevenuePool);
                env.events()
                    .publish((Symbol::new(&env, "clear_revenue_pool"), caller), ());
            }
        }
    }

    pub fn get_revenue_pool(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::RevenuePool)
    }

    /// Store the settlement contract address (admin only).
    ///
    /// Once set, every `deduct` and `batch_deduct` call transfers the deducted
    /// USDC to this address. Settlement is a hard precondition for deductions.
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

    pub fn get_settlement(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Settlement)
            .unwrap_or_else(|| panic!("settlement address not set"))
    }

    /// Return all three configurable contract addresses in one read-only call.
    ///
    /// Returns `(usdc_token, settlement, revenue_pool)`.
    pub fn get_contract_addresses(env: Env) -> (Option<Address>, Option<Address>, Option<Address>) {
        let inst = env.storage().instance();
        let usdc_token: Option<Address> = inst.get(&StorageKey::UsdcToken);
        let settlement: Option<Address> = inst.get(&StorageKey::Settlement);
        let revenue_pool: Option<Address> = inst.get(&StorageKey::RevenuePool);
        (usdc_token, settlement, revenue_pool)
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

    pub fn get_metadata(env: Env, offering_id: String) -> Option<String> {
        env.storage()
            .instance()
            .get(&StorageKey::Metadata(offering_id))
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

        let old_metadata: String = env
            .storage()
            .instance()
            .get(&StorageKey::Metadata(offering_id.clone()))
            .unwrap_or(String::from_str(&env, ""));
        env.storage()
            .instance()
            .set(&StorageKey::Metadata(offering_id.clone()), &metadata);

        env.events().publish(
            (Symbol::new(&env, "metadata_updated"), offering_id, caller),
            (old_metadata, metadata.clone()),
        );

        metadata
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

    fn require_authorized_deduct_caller(env: Env, caller: &Address) {
        let meta = Self::get_meta(env);
        let is_authorized = match meta.authorized_caller {
            Some(authorized_caller) => *caller == authorized_caller || *caller == meta.owner,
            None => *caller == meta.owner,
        };
        assert!(is_authorized, "unauthorized caller");
    }

    fn assert_request_id_available(env: &Env, request_id: &Symbol) {
        assert!(
            !Self::is_request_id_processed(env, request_id),
            "{}",
            ERR_DUPLICATE_REQUEST_ID
        );
    }

    fn is_request_id_processed(env: &Env, request_id: &Symbol) -> bool {
        env.storage()
            .instance()
            .has(&StorageKey::ProcessedRequest(request_id.clone()))
    }

    fn mark_request_id_processed(env: &Env, request_id: &Symbol) {
        env.storage()
            .instance()
            .set(&StorageKey::ProcessedRequest(request_id.clone()), &true);
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
}

#[cfg(test)]
mod test;

#[cfg(test)]
mod test_init_hardening;
