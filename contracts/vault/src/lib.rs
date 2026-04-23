//! # Callora Vault Contract  deposit/withdraw/deduct/distribute with pause circuit-breaker.
#![no_std]
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

#[contracttype]
pub enum StorageKey {
    Meta,
    AllowedDepositors,
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
        if let Some(p) = &revenue_pool {
            assert!(
                p != &env.current_contract_address(),
                "revenue_pool cannot be vault address"
            );
        }
        let balance = initial_balance.unwrap_or(0);
        assert!(balance >= 0, "initial balance must be non-negative");
        let min_d = min_deposit.unwrap_or(0);
        assert!(min_d >= 0, "min_deposit must be non-negative");
        let max_d = max_deduct.unwrap_or(DEFAULT_MAX_DEDUCT);
        assert!(max_d > 0, "max_deduct must be positive");
        assert!(min_d <= max_d, "min_deposit cannot exceed max_deduct");
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
            min_deposit: min_d,
        };
        inst.set(&StorageKey::Meta, &meta);
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
        let vb = usdc.balance(&env.current_contract_address());
        if vb < amount {
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
        Self::require_owner(env.clone(), caller);
        match depositor {
            Some(d) => {
                let mut list: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&StorageKey::DepositorList)
                    .unwrap_or(Vec::new(&env));
                if !list.contains(&d) {
                    env.storage()
                        .instance()
                        .set(&StorageKey::AllowedDepositors, &d);
                    list.push_back(d);
                }
                env.storage()
                    .instance()
                    .set(&StorageKey::DepositorList, &list);
            }
            None => {
                env.storage()
                    .instance()
                    .remove(&StorageKey::AllowedDepositors);
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
            .remove(&StorageKey::AllowedDepositors);
        env.storage()
            .instance()
            .set(&StorageKey::DepositorList, &Vec::<Address>::new(&env));
    }

    pub fn get_allowed_depositors(env: Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get(&StorageKey::DepositorList)
            .unwrap_or(Vec::new(&env))
    }

    pub fn set_authorized_caller(env: Env, caller: Address) {
        let mut meta = Self::get_meta(env.clone());
        meta.owner.require_auth();
        meta.authorized_caller = Some(caller.clone());
        env.storage().instance().set(&StorageKey::Meta, &meta);
        env.events().publish(
            (Symbol::new(&env, "set_auth_caller"), meta.owner.clone()),
            caller,
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
        caller.require_auth();
        Self::require_not_paused(env.clone());
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
            (Symbol::new(&env, "deposit"), caller.clone()),
            (amount, meta.balance),
        );
        meta.balance
    }

    pub fn deduct(env: Env, caller: Address, amount: i128, request_id: Option<Symbol>) -> i128 {
        caller.require_auth();
        Self::require_not_paused(env.clone());
        assert!(amount > 0, "amount must be positive");
        let max_d = Self::get_max_deduct(env.clone());
        assert!(amount <= max_d, "deduct amount exceeds max_deduct");
        let meta = Self::get_meta(env.clone());
        let auth = match &meta.authorized_caller {
            Some(ac) => caller == *ac || caller == meta.owner,
            None => true,
        };
        assert!(auth, "unauthorized caller");
        assert!(meta.balance >= amount, "insufficient balance");
        let mut meta = Self::get_meta(env.clone());
        meta.balance = meta.balance.checked_sub(amount).unwrap();
        env.storage().instance().set(&StorageKey::Meta, &meta);
        let inst = env.storage().instance();
        if let Some(s) = inst.get::<StorageKey, Address>(&StorageKey::Settlement) {
            let ut: Address = inst.get(&StorageKey::UsdcToken).unwrap();
            Self::transfer_funds(&env, &ut, &s, amount);
        } else if inst
            .get::<StorageKey, Address>(&StorageKey::RevenuePool)
            .is_some()
        {
            Self::transfer_to_revenue_pool(env.clone(), amount);
        }
        let rid = request_id.unwrap_or(Symbol::new(&env, ""));
        env.events().publish(
            (Symbol::new(&env, "deduct"), caller, rid),
            (amount, meta.balance),
        );
        meta.balance
    }

    pub fn batch_deduct(env: Env, caller: Address, items: Vec<DeductItem>) -> i128 {
        caller.require_auth();
        let n = items.len();
        assert!(n > 0, "batch_deduct requires at least one item");
        assert!(n <= MAX_BATCH_SIZE, "batch too large");
        let max_d = Self::get_max_deduct(env.clone());
        let mut meta = Self::get_meta(env.clone());
        let auth = match &meta.authorized_caller {
            Some(ac) => caller == *ac || caller == meta.owner,
            None => true,
        };
        assert!(auth, "unauthorized caller");
        let mut running = meta.balance;
        let mut total: i128 = 0;
        for item in items.iter() {
            assert!(item.amount > 0, "amount must be positive");
            assert!(item.amount <= max_d, "deduct amount exceeds max_deduct");
            assert!(running >= item.amount, "insufficient balance");
            running = running.checked_sub(item.amount).unwrap();
            total = total.checked_add(item.amount).unwrap();
        }
        meta.balance = running;
        env.storage().instance().set(&StorageKey::Meta, &meta);
        let mut eb = meta.balance.checked_add(total).unwrap();
        for item in items.iter() {
            eb = eb.checked_sub(item.amount).unwrap();
            let rid = item.request_id.clone().unwrap_or(Symbol::new(&env, ""));
            env.events().publish(
                (Symbol::new(&env, "deduct"), caller.clone(), rid),
                (item.amount, eb),
            );
        }
        let inst = env.storage().instance();
        if let Some(s) = inst.get::<StorageKey, Address>(&StorageKey::Settlement) {
            let ut: Address = inst.get(&StorageKey::UsdcToken).unwrap();
            Self::transfer_funds(&env, &ut, &s, total);
        } else if inst
            .get::<StorageKey, Address>(&StorageKey::RevenuePool)
            .is_some()
        {
            Self::transfer_to_revenue_pool(env.clone(), total);
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
        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingOwner)
            .expect("no ownership transfer pending");
        pending.require_auth();
        let mut meta = Self::get_meta(env.clone());
        let old = meta.owner.clone();
        meta.owner = pending;
        env.storage().instance().set(&StorageKey::Meta, &meta);
        env.storage().instance().remove(&StorageKey::PendingOwner);
        env.events().publish(
            (Symbol::new(&env, "ownership_accepted"), old, meta.owner),
            (),
        );
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
        let usdc = token::Client::new(&env, &ua);
        usdc.transfer(&env.current_contract_address(), &meta.owner, &amount);
        meta.balance = meta.balance.checked_sub(amount).unwrap();
        env.storage().instance().set(&StorageKey::Meta, &meta);
        env.events().publish(
            (Symbol::new(&env, "withdraw"), meta.owner.clone()),
            (amount, meta.balance),
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
        let usdc = token::Client::new(&env, &ua);
        usdc.transfer(&env.current_contract_address(), &to, &amount);
        meta.balance = meta.balance.checked_sub(amount).unwrap();
        env.storage().instance().set(&StorageKey::Meta, &meta);
        env.events().publish(
            (Symbol::new(&env, "withdraw_to"), meta.owner.clone(), to),
            (amount, meta.balance),
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

    pub fn get_revenue_pool(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::RevenuePool)
    }

    pub fn set_settlement(env: Env, caller: Address, settlement_address: Address) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone());
        if caller != admin {
            panic!("unauthorized: caller is not admin");
        }
        env.storage()
            .instance()
            .set(&StorageKey::Settlement, &settlement_address);
    }

    pub fn get_settlement(env: Env) -> Address {
        env.storage()
            .instance()
            .get(&StorageKey::Settlement)
            .unwrap_or_else(|| panic!("settlement address not set"))
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

    fn transfer_funds(env: &Env, usdc_token: &Address, to: &Address, amount: i128) {
        token::Client::new(env, usdc_token).transfer(&env.current_contract_address(), to, &amount);
    }

    fn transfer_to_revenue_pool(env: Env, amount: i128) {
        let inst = env.storage().instance();
        let rp: Address = inst
            .get(&StorageKey::RevenuePool)
            .expect("revenue pool address not set");
        let ua: Address = inst
            .get(&StorageKey::UsdcToken)
            .expect("vault not initialized");
        token::Client::new(&env, &ua).transfer(&env.current_contract_address(), &rp, &amount);
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