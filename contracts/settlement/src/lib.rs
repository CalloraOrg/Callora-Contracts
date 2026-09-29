#![no_std]
pub mod admin;
pub mod archive;
pub mod batch;
pub mod errors;
pub mod events;
pub mod freeze;
pub mod limits;
pub mod migrate;
pub mod pagination;
pub mod price_registry;
pub mod replay_guard;
pub mod timelock;
mod types;

use soroban_sdk::{contract, contractimpl, token, Address, BytesN, Env, Symbol, Vec};

pub const MAX_BATCH_SIZE: u32 = 50;
pub const MAX_DEVELOPER_BALANCES_PAGE_SIZE: u32 = 100;
pub const MAX_BROADCAST_MESSAGE_LEN: u32 = 1024;

pub use errors::SettlementError;
pub use migrate::{STORAGE_VERSION_V1, STORAGE_VERSION_V2};
pub use timelock::{PendingDeveloperMigration, DEVELOPER_MIGRATION_TIMELOCK_SECONDS};
pub use types::*;

#[contract]
pub struct CalloraSettlement;

#[contractimpl]
impl CalloraSettlement {
    pub fn init(env: Env, admin: Address, vault_address: Address) {
        admin.require_auth();
        if env.storage().instance().has(&StorageKey::Admin) {
            env.panic_with_error(SettlementError::AlreadyInitialized);
        }
        if admin == vault_address {
            env.panic_with_error(SettlementError::InvalidConfigDistinct);
        }
        let contract_address = env.current_contract_address();
        if admin == contract_address {
            env.panic_with_error(SettlementError::InvalidConfigAdminContract);
        }
        if vault_address == contract_address {
            env.panic_with_error(SettlementError::InvalidConfigVaultContract);
        }

        let inst = env.storage().instance();
        inst.set(&StorageKey::Admin, &admin);
        inst.set(&StorageKey::Vault, &vault_address);
        let pool = GlobalPool {
            total_balance: 0,
            last_updated: env.ledger().timestamp(),
        };
        inst.set(&StorageKey::GlobalPool, &pool);
        inst.set(&StorageKey::TotalReceived, &0i128);

        events::emit_initialized(&env, &admin, &vault_address, &pool);
    }

    pub fn record_deduction(env: Env, amount: i128, _request_id: u64) {
        let vault = Self::get_vault(env.clone()).unwrap();
        vault.require_auth();
        let total = env
            .storage()
            .instance()
            .get::<_, i128>(&StorageKey::TotalReceived)
            .unwrap_or(0);
        let new_total = total
            .checked_add(amount)
            .unwrap_or_else(|| env.panic_with_error(SettlementError::PoolOverflow));
        env.storage()
            .instance()
            .set(&StorageKey::TotalReceived, &new_total);
    }

    pub fn receive_payment(
        env: Env,
        caller: Address,
        amount: i128,
        to_pool: bool,
        developer: Option<Address>,
        token: Address,
        ledger_seq: u32,
    ) {
        caller.require_auth();
        Self::require_authorized_caller(env.clone(), caller.clone());
        if amount <= 0 {
            env.panic_with_error(SettlementError::AmountNotPositive);
        }

        if to_pool {
            replay_guard::check_pool(&env, ledger_seq).unwrap_or_else(|e| env.panic_with_error(e));
        } else {
            let dev = developer
                .clone()
                .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperRequired));
            replay_guard::check_developer(&env, &dev, ledger_seq)
                .unwrap_or_else(|e| env.panic_with_error(e));
        }

        let inst = env.storage().instance();
        if to_pool {
            if developer.is_some() {
                env.panic_with_error(SettlementError::DeveloperMustBeNone);
            }
            let mut global_pool = Self::get_global_pool(env.clone()).unwrap();
            global_pool.total_balance = global_pool
                .total_balance
                .checked_add(amount)
                .unwrap_or_else(|| env.panic_with_error(SettlementError::PoolOverflow));
            global_pool.last_updated = env.ledger().timestamp();
            inst.set(&StorageKey::GlobalPool, &global_pool);
            events::emit_payment_received(
                &env,
                &caller,
                PaymentReceivedEvent {
                    from_vault: caller.clone(),
                    amount,
                    to_pool: true,
                    developer: None,
                    token: token.clone(),
                },
            );
        } else {
            let dev_address = developer
                .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperRequired));

            let balance_key = StorageKey::DeveloperBalance(dev_address.clone(), token.clone());

            let current_balance: i128 = env
                .storage()
                .persistent()
                .get(&balance_key)
                .unwrap_or(0i128);
            let new_balance = current_balance
                .checked_add(amount)
                .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperOverflow));

            env.storage().persistent().set(&balance_key, &new_balance);

            env.storage()
                .persistent()
                .extend_ttl(&balance_key, 50000, 50000);

            let mut index: Vec<Address> = inst
                .get(&StorageKey::DeveloperIndex)
                .unwrap_or_else(|| Vec::new(&env));
            Self::sorted_insert(&env, &mut index, dev_address.clone());
            inst.set(&StorageKey::DeveloperIndex, &index);

            events::emit_payment_received(
                &env,
                &caller,
                PaymentReceivedEvent {
                    from_vault: caller.clone(),
                    amount,
                    to_pool: false,
                    developer: Some(dev_address.clone()),
                    token: token.clone(),
                },
            );
            events::emit_balance_credited(
                &env,
                &dev_address,
                BalanceCreditedEvent {
                    developer: dev_address.clone(),
                    amount,
                    new_balance,
                    token: token.clone(),
                },
            );

            events::emit_deposit(
                &env,
                &dev_address,
                DepositEvent {
                    developer: dev_address.clone(),
                    token: token.clone(),
                    amount,
                },
            );
        }
    }

    pub fn batch_receive_payment(
        env: Env,
        caller: Address,
        items: Vec<(Address, i128)>,
        token: Address,
        ledger_seq: u32,
    ) {
        caller.require_auth();
        Self::require_authorized_caller(env.clone(), caller.clone());

        let n = items.len();
        if n == 0 {
            env.panic_with_error(SettlementError::BatchEmpty);
        }
        if n > MAX_BATCH_SIZE {
            env.panic_with_error(SettlementError::BatchTooLarge);
        }

        for item in items.iter() {
            let (_, amount) = item;
            if amount <= 0 {
                env.panic_with_error(SettlementError::AmountNotPositive);
            }
        }

        for item in items.iter() {
            let (dev, _) = item;
            replay_guard::check_developer(&env, &dev, ledger_seq)
                .unwrap_or_else(|e| env.panic_with_error(e));
        }

        let inst = env.storage().instance();

        for item in items.iter() {
            let (dev, amount) = item;
            let balance_key = StorageKey::DeveloperBalance(dev.clone(), token.clone());
            let current: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);
            let new_balance = current
                .checked_add(amount)
                .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperOverflow));
            env.storage().persistent().set(&balance_key, &new_balance);
            env.storage().persistent().set(
                &StorageKey::DeveloperBalance(dev.clone(), token.clone()),
                &new_balance,
            );
            env.storage().persistent().extend_ttl(
                &StorageKey::DeveloperBalance(dev.clone(), token.clone()),
                50000,
                50000,
            );
            let mut index: Vec<Address> = inst
                .get(&StorageKey::DeveloperIndex)
                .unwrap_or_else(|| Vec::new(&env));
            Self::sorted_insert(&env, &mut index, dev.clone());
            inst.set(&StorageKey::DeveloperIndex, &index);
            events::emit_balance_credited(
                &env,
                &dev,
                BalanceCreditedEvent {
                    developer: dev.clone(),
                    amount,
                    new_balance,
                    token: token.clone(),
                },
            );
            events::emit_deposit(
                &env,
                &dev,
                DepositEvent {
                    developer: dev.clone(),
                    token: token.clone(),
                    amount,
                },
            );
        }
    }

    pub fn get_admin(env: Env) -> Result<Address, SettlementError> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(SettlementError::NotInitialized)
    }

    pub fn set_developer_min_balance(
        env: Env,
        caller: Address,
        developer: Address,
        min_balance: i128,
    ) {
        limits::set_developer_min_balance(&env, caller, developer, min_balance);
    }

    pub fn get_developer_min_balance(env: Env, developer: Address) -> i128 {
        limits::get_developer_min_balance(&env, developer)
    }
    
    pub fn version(_env: Env) -> soroban_sdk::String {
        soroban_sdk::String::from_str(&_env, env!("CARGO_PKG_VERSION"))
    }

    pub fn get_vault(env: Env) -> Result<Address, SettlementError> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.storage()
            .instance()
            .get(&StorageKey::Vault)
            .ok_or(SettlementError::NotInitialized)
    }

    pub fn get_global_pool(env: Env) -> Result<GlobalPool, SettlementError> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.storage()
            .instance()
            .get::<_, GlobalPool>(&StorageKey::GlobalPool)
            .ok_or(SettlementError::NotInitialized)
    }

    pub fn get_total_received(env: Env) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.storage()
            .instance()
            .get(&StorageKey::TotalReceived)
            .unwrap_or(0)
    }

    pub fn get_developer_balance(
        env: Env,
        developer: Address,
        token: Address,
    ) -> Result<i128, SettlementError> {
        if !env.storage().instance().has(&StorageKey::Admin) {
            return Err(SettlementError::NotInitialized);
        }
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let key = StorageKey::DeveloperBalance(developer, token);
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_BUMP_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }
        Ok(env.storage().persistent().get(&key).unwrap_or(0))
    }

    pub fn propose_balance_migration(env: Env, caller: Address, from: Address, to: Address) {
        admin::propose_balance_migration(&env, &caller, &from, &to);
    }

    pub fn execute_balance_migration(env: Env, caller: Address, from: Address) {
        admin::execute_balance_migration(&env, &caller, &from);
    }

    pub fn get_balance_migration(env: Env, from: Address) -> Option<PendingDeveloperMigration> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        timelock::get_pending_migration(&env, &from)
    }

    pub fn set_usdc_token(env: Env, caller: Address, usdc_address: Address) {
        caller.require_auth();
        let current_admin = Self::get_admin(env.clone()).unwrap();
        if caller != current_admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        if usdc_address == env.current_contract_address() {
            env.panic_with_error(SettlementError::InvalidUsdcToken);
        }
        env.storage()
            .instance()
            .set(&StorageKey::Usdc, &usdc_address);
    }

    fn get_usdc_token(env: Env) -> Result<Address, SettlementError> {
        env.storage()
            .instance()
            .get(&StorageKey::Usdc)
            .ok_or(SettlementError::UsdcTokenNotConfigured)
    }

    pub fn withdraw_developer_balance(
        env: Env,
        developer: Address,
        amount: i128,
        to: Option<Address>,
    ) -> Result<(), SettlementError> {
        developer.require_auth();
        if freeze::is_developer_frozen(env.clone(), developer.clone()) {
            return Err(SettlementError::DeveloperFrozen);
        }
        if amount <= 0 {
            return Err(SettlementError::AmountNotPositive);
        }

        let recipient = to.unwrap_or_else(|| developer.clone());
        let contract_address = env.current_contract_address();
        if recipient == contract_address {
            env.panic_with_error(SettlementError::InvalidRecipient);
        }

        Self::require_claim_window_open(&env, &developer)?;

        let usdc_address = Self::get_usdc_token(env.clone())?;

        let balance_key = StorageKey::DeveloperBalance(developer.clone(), usdc_address.clone());
        let current_balance: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);
        if amount > current_balance {
            return Err(SettlementError::InsufficientDeveloperBalance);
        }

        let new_balance = current_balance
            .checked_sub(amount)
            .ok_or(SettlementError::DeveloperBalanceUnderflow)?;
        limits::check_min_balance(&env, &developer, new_balance)?;

        let today = env.ledger().timestamp() / 86400;
        let today_key = StorageKey::WithdrawalToday(developer.clone());
        let mut daily = env
            .storage()
            .persistent()
            .get::<_, DailyWithdrawState>(&today_key)
            .unwrap_or(DailyWithdrawState {
                day: today,
                amount: 0,
            });
        if daily.day != today {
            daily.day = today;
            daily.amount = 0;
        }

        let cap: i128 = env
            .storage()
            .persistent()
            .get(&StorageKey::DailyWithdrawCap(developer.clone()))
            .unwrap_or(0);
        if cap > 0 {
            let projected = daily
                .amount
                .checked_add(amount)
                .ok_or(SettlementError::DailyWithdrawCapExceeded)?;
            if projected > cap {
                return Err(SettlementError::DailyWithdrawCapExceeded);
            }
        }

        let usdc = token::Client::new(&env, &usdc_address);
        if usdc.balance(&contract_address) < amount {
            return Err(SettlementError::InsufficientContractBalance);
        }

        env.storage().persistent().set(&balance_key, &new_balance);
        env.storage()
            .persistent()
            .extend_ttl(&balance_key, 50000, 50000);

        daily.amount = daily
            .amount
            .checked_add(amount)
            .ok_or(SettlementError::DailyWithdrawCapExceeded)?;
        env.storage().persistent().set(&today_key, &daily);
        env.storage()
            .persistent()
            .extend_ttl(&today_key, 50000, 50000);

        usdc.transfer(&contract_address, &recipient, &amount);

        events::emit_developer_withdraw(
            &env,
            &developer.clone(),
            DeveloperWithdrawEvent {
                developer,
                amount,
                remaining_balance: new_balance,
                to: recipient,
                token: usdc_address,
            },
        );

        Ok(())
    }

    pub fn simulate_claim(
        env: Env,
        developer: Address,
        amount: i128,
        to: Option<Address>,
    ) -> Result<ClaimSimulation, SettlementError> {
        if amount <= 0 {
            return Err(SettlementError::AmountNotPositive);
        }

        let recipient = to.unwrap_or_else(|| developer.clone());
        let contract_address = env.current_contract_address();
        if recipient == contract_address {
            env.panic_with_error(SettlementError::InvalidRecipient);
        }

        Self::require_claim_window_open(&env, &developer)?;

        let usdc_address = Self::get_usdc_token(env.clone())?;
        let current_balance: i128 = env
            .storage()
            .persistent()
            .get(&StorageKey::DeveloperBalance(
                developer.clone(),
                usdc_address.clone(),
            ))
            .unwrap_or(0);
        if amount > current_balance {
            return Err(SettlementError::InsufficientDeveloperBalance);
        }

        let today = env.ledger().timestamp() / 86400;
        let cap: i128 = env
            .storage()
            .persistent()
            .get(&StorageKey::DailyWithdrawCap(developer.clone()))
            .unwrap_or(0);
        let daily = env
            .storage()
            .persistent()
            .get::<_, DailyWithdrawState>(&StorageKey::WithdrawalToday(developer.clone()))
            .unwrap_or(DailyWithdrawState {
                day: today,
                amount: 0,
            });
        let withdrawn_today = if daily.day == today { daily.amount } else { 0 };
        let withdrawn_today_after = withdrawn_today
            .checked_add(amount)
            .ok_or(SettlementError::DailyWithdrawCapExceeded)?;
        if cap > 0 && withdrawn_today_after > cap {
            return Err(SettlementError::DailyWithdrawCapExceeded);
        }

        let remaining_balance = current_balance
            .checked_sub(amount)
            .ok_or(SettlementError::DeveloperBalanceUnderflow)?;
        let contract_balance = token::Client::new(&env, &usdc_address).balance(&contract_address);
        if contract_balance < amount {
            return Err(SettlementError::InsufficientContractBalance);
        }

        Ok(ClaimSimulation {
            developer,
            amount,
            recipient,
            token: usdc_address,
            current_balance,
            remaining_balance,
            contract_balance,
            daily_withdraw_cap: cap,
            withdrawn_today,
            withdrawn_today_after,
        })
    }

    pub fn set_developer_claim_window(
        env: Env,
        caller: Address,
        developer: Address,
        start_ts: u64,
        end_ts: u64,
    ) -> Result<(), SettlementError> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone()).unwrap();
        if caller != admin {
            return Err(SettlementError::Unauthorized);
        }
        if end_ts < start_ts {
            return Err(SettlementError::InvalidClaimWindow);
        }

        let window_key = StorageKey::DeveloperClaimWindow(developer.clone());
        env.storage()
            .persistent()
            .set(&window_key, &DeveloperClaimWindow { start_ts, end_ts });
        env.storage()
            .persistent()
            .extend_ttl(&window_key, 50000, 50000);

        events::emit_developer_claim_window_changed(
            &env,
            &developer.clone(),
            DeveloperClaimWindowChanged {
                developer,
                start_ts,
                end_ts,
                enabled: true,
            },
        );

        Ok(())
    }

    pub fn clear_developer_claim_window(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone()).unwrap();
        if caller != admin {
            return Err(SettlementError::Unauthorized);
        }

        env.storage()
            .persistent()
            .remove(&StorageKey::DeveloperClaimWindow(developer.clone()));

        events::emit_developer_claim_window_changed(
            &env,
            &developer.clone(),
            DeveloperClaimWindowChanged {
                developer,
                start_ts: 0,
                end_ts: 0,
                enabled: false,
            },
        );

        Ok(())
    }

    pub fn get_developer_claim_window(
        env: Env,
        developer: Address,
    ) -> Option<DeveloperClaimWindow> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let key = StorageKey::DeveloperClaimWindow(developer);
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_BUMP_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }
        env.storage().persistent().get(&key)
    }

    fn require_claim_window_open(env: &Env, developer: &Address) -> Result<(), SettlementError> {
        let key = StorageKey::DeveloperClaimWindow(developer.clone());
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_BUMP_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }
        let window: Option<DeveloperClaimWindow> = env.storage().persistent().get(&key);
        if let Some(window) = window {
            let now = env.ledger().timestamp();
            if now < window.start_ts || now > window.end_ts {
                return Err(SettlementError::ClaimWindowClosed);
            }
        }
        Ok(())
    }

    pub fn set_daily_withdraw_cap(env: Env, caller: Address, developer: Address, cap: i128) {
        caller.require_auth();
        let current_admin = Self::get_admin(env.clone()).unwrap();
        if caller != current_admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        let cap_key = StorageKey::DailyWithdrawCap(developer.clone());
        env.storage().persistent().set(&cap_key, &cap);
        env.storage()
            .persistent()
            .extend_ttl(&cap_key, 50000, 50000);

        events::emit_daily_withdraw_cap_changed(
            &env,
            &caller,
            DailyWithdrawCapChanged {
                developer,
                new_cap: cap,
            },
        );
    }

    pub fn get_daily_withdraw_cap(env: Env, developer: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let key = StorageKey::DailyWithdrawCap(developer);
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_BUMP_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }
        env.storage().persistent().get(&key).unwrap_or(0)
    }

    pub fn get_withdrawal_today(env: Env, developer: Address) -> i128 {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let key = StorageKey::WithdrawalToday(developer);
        if env.storage().persistent().has(&key) {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_BUMP_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }
        let state: Option<DailyWithdrawState> = env.storage().persistent().get(&key);
        match state {
            Some(s) if s.day == env.ledger().timestamp() / 86400 => s.amount,
            _ => 0,
        }
    }

    pub fn set_minimum_balance(env: Env, caller: Address, developer: Address, min_balance: i128) {
        limits::set_developer_min_balance(&env, caller, developer, min_balance);
    }

    pub fn get_minimum_balance(env: Env, developer: Address) -> i128 {
        limits::get_developer_min_balance(&env, developer)
    }

    pub fn force_credit_developer(
        env: Env,
        caller: Address,
        developer: Address,
        amount: i128,
        token: Address,
        reason: Symbol,
    ) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone()).unwrap();
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        if amount <= 0 {
            env.panic_with_error(SettlementError::AmountNotPositive);
        }

        let balance_key = StorageKey::DeveloperBalance(developer.clone(), token.clone());
        let current_balance: i128 = env
            .storage()
            .persistent()
            .get(&balance_key)
            .unwrap_or(0i128);
        let new_balance = current_balance
            .checked_add(amount)
            .unwrap_or_else(|| env.panic_with_error(SettlementError::DeveloperOverflow));

        env.storage().persistent().set(&balance_key, &new_balance);
        env.storage()
            .persistent()
            .extend_ttl(&balance_key, 50000, 50000);

        let inst = env.storage().instance();
        let mut index: Vec<Address> = inst
            .get(&StorageKey::DeveloperIndex)
            .unwrap_or_else(|| Vec::new(&env));
        Self::sorted_insert(&env, &mut index, developer.clone());
        inst.set(&StorageKey::DeveloperIndex, &index);

        events::emit_developer_force_credited(
            &env,
            &developer.clone(),
            DeveloperForceCreditedEvent {
                developer,
                amount,
                reason,
                new_balance,
                token,
            },
        );
    }

    pub fn get_all_developer_balances(
        env: Env,
        caller: Address,
        token: Address,
    ) -> Vec<DeveloperBalance> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone()).unwrap_or_else(|e| env.panic_with_error(e));
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        let index: Vec<Address> = env
            .storage()
            .instance()
            .get(&StorageKey::DeveloperIndex)
            .unwrap_or_else(|| Vec::new(&env));

        let mut result = Vec::new(&env);
        for address in index.iter() {
            let key = StorageKey::DeveloperBalance(address.clone(), token.clone());
            if env.storage().persistent().has(&key) {
                env.storage().persistent().extend_ttl(
                    &key,
                    PERSISTENT_BUMP_THRESHOLD,
                    PERSISTENT_BUMP_AMOUNT,
                );
            }
            let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0i128);
            result.push_back(DeveloperBalance {
                address: address.clone(),
                token: token.clone(),
                balance,
            });
        }
        result
    }

    pub fn get_developer_balances_page(
        env: Env,
        caller: Address,
        start: u32,
        limit: u32,
        token: Address,
    ) -> Vec<DeveloperBalance> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone()).unwrap();
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);

        let index: Vec<Address> = env
            .storage()
            .instance()
            .get(&StorageKey::DeveloperIndex)
            .unwrap_or_else(|| Vec::new(&env));

        if limit == 0 || start >= index.len() {
            return Vec::new(&env);
        }

        let end = start
            .saturating_add(limit.min(MAX_DEVELOPER_BALANCES_PAGE_SIZE))
            .min(index.len());
        let mut result = Vec::new(&env);
        for (cursor, address) in (0_u32..).zip(index.iter()) {
            if cursor >= end {
                break;
            }
            if cursor >= start {
                let key = StorageKey::DeveloperBalance(address.clone(), token.clone());
                if env.storage().persistent().has(&key) {
                    env.storage().persistent().extend_ttl(
                        &key,
                        PERSISTENT_BUMP_THRESHOLD,
                        PERSISTENT_BUMP_AMOUNT,
                    );
                }
                let balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);
                result.push_back(DeveloperBalance {
                    address: address.clone(),
                    token: token.clone(),
                    balance,
                });
            }
        }
        result
    }

    pub fn get_developer_balances_cursor(
        env: Env,
        caller: Address,
        cursor: Option<Address>,
        limit: u32,
        token: Address,
    ) -> (Vec<DeveloperBalance>, Option<Address>) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone()).unwrap_or_else(|e| env.panic_with_error(e));
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }

        let index: Vec<Address> = env
            .storage()
            .instance()
            .get(&StorageKey::DeveloperIndex)
            .unwrap_or_else(|| Vec::new(&env));

        pagination::get_page(&env, &index, cursor, limit, &token)
    }

    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.storage().instance().get(&StorageKey::PendingAdmin)
    }

    pub fn set_admin(env: Env, caller: Address, new_admin: Address) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone()).unwrap();
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);
        events::emit_admin_nominated(&env, &admin, &new_admin);
    }

    pub fn accept_admin(env: Env) {
        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .unwrap_or_else(|| panic!("no admin transfer pending"));
        pending.require_auth();
        let old_admin = Self::get_admin(env.clone()).unwrap();
        let inst = env.storage().instance();
        inst.set(&StorageKey::Admin, &pending);
        inst.remove(&StorageKey::PendingAdmin);
        events::emit_admin_accepted(&env, &old_admin, &pending);
    }

    pub fn cancel_admin_transfer(env: Env, caller: Address) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone()).unwrap();
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        if !env.storage().instance().has(&StorageKey::PendingAdmin) {
            env.panic_with_error(SettlementError::NoAdminTransferPending);
        }
        env.storage().instance().remove(&StorageKey::PendingAdmin);
        events::emit_admin_cancelled(&env, &admin);
    }

    pub fn propose_vault(env: Env, caller: Address, new_vault: Address) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone()).unwrap();
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        if new_vault == env.current_contract_address() {
            env.panic_with_error(SettlementError::InvalidVault);
        }
        let current_vault = Self::get_vault(env.clone()).unwrap();
        env.storage()
            .instance()
            .set(&StorageKey::PendingVault, &new_vault);
        events::emit_vault_proposed(
            &env,
            &admin,
            VaultProposedEvent {
                current_vault,
                proposed_vault: new_vault,
            },
        );
    }

    pub fn set_vault(env: Env, caller: Address, new_vault: Address) {
        Self::propose_vault(env, caller, new_vault);
    }

    pub fn accept_vault(env: Env, caller: Address) {
        caller.require_auth();
        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingVault)
            .unwrap_or_else(|| panic!("no vault rotation pending"));
        let admin = Self::get_admin(env.clone()).unwrap();
        if caller != pending && caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        let old_vault = Self::get_vault(env.clone()).unwrap();
        let inst = env.storage().instance();
        inst.set(&StorageKey::Vault, &pending);
        inst.remove(&StorageKey::PendingVault);
        events::emit_vault_accepted(
            &env,
            &pending,
            VaultAcceptedEvent {
                old_vault,
                new_vault: pending.clone(),
                accepted_by: caller,
            },
        );
    }

    pub fn broadcast(env: Env, caller: Address, severity: Severity, message: soroban_sdk::String) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone()).unwrap();
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        if message.len() > MAX_BROADCAST_MESSAGE_LEN {
            env.panic_with_error(SettlementError::BroadcastMessageTooLong);
        }
        events::emit_admin_broadcast(&env, &caller, AdminBroadcast { severity, message });
    }

    pub fn upgrade(env: Env, caller: Address, new_wasm_hash: BytesN<32>) {
        caller.require_auth();
        let admin = Self::get_admin(env.clone()).unwrap();
        if caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
        env.deployer()
            .update_current_contract_wasm(new_wasm_hash.clone());
        env.storage()
            .instance()
            .set(&StorageKey::ContractVersion, &new_wasm_hash);
        events::emit_upgraded(&env, &caller, &new_wasm_hash);
    }

    pub fn get_version(env: Env) -> Option<BytesN<32>> {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
        env.storage().instance().get(&StorageKey::ContractVersion)
    }

    pub fn migrate_developer_balance(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        migrate::migrate_single_developer(&env, &caller, &developer)
    }

    pub fn migrate_single_dev_v2(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        migrate::migrate_single_developer(&env, &caller, &developer)
    }

    pub fn migrate_v1_to_v2(env: Env, caller: Address) {
        migrate::migrate_v1_to_v2(&env, &caller);
    }

    pub fn migrate_v1_to_v2_page(
        env: Env,
        caller: Address,
        offset: u32,
        batch_size: u32,
    ) -> (u32, bool) {
        migrate::migrate_v1_to_v2_page(&env, &caller, offset, batch_size)
    }

    pub fn migration_storage_version(env: Env) -> u32 {
        migrate::storage_version(&env)
    }

    pub fn batch_withdraw_balance_cursor(
        _env: Env,
        developers: Vec<Address>,
        amounts: Vec<i128>,
        _cursor: u32,
        _limit: u32,
    ) -> Result<(u32, bool), SettlementError> {
        let count = developers.len();
        if count != amounts.len() {
            return Err(SettlementError::AmountNotPositive);
        }
        if count > MAX_BATCH_SIZE {
            return Err(SettlementError::BatchTooLarge);
        }
        Ok((0, true))
    }

    pub fn batch_settle(
        env: Env,
        settlements: soroban_sdk::Vec<batch::SettleInput>,
    ) -> soroban_sdk::Vec<batch::SettleOutcome> {
        batch::batch_settle(&env, settlements)
    }

    pub fn freeze_developer(
        env: Env,
        caller: Address,
        developer: Address,
        reason: Symbol,
    ) -> Result<(), SettlementError> {
        freeze::freeze_developer(env, caller, developer, reason)
    }

    pub fn unfreeze_developer(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        freeze::unfreeze_developer(env, caller, developer)
    }

    pub fn is_developer_frozen(env: Env, developer: Address) -> bool {
        freeze::is_developer_frozen(env, developer)
    }

    pub fn set_price(
        env: Env,
        caller: Address,
        offering_id: soroban_sdk::String,
        price: soroban_sdk::String,
    ) {
        price_registry::set_price(&env, caller, offering_id, price);
    }

    pub fn remove_price(env: Env, caller: Address, offering_id: soroban_sdk::String) {
        price_registry::remove_price(&env, caller, offering_id);
    }

    pub fn get_price(env: Env, offering_id: soroban_sdk::String) -> Option<soroban_sdk::String> {
        price_registry::get_price(&env, offering_id)
    }

    fn require_authorized_caller(env: Env, caller: Address) {
        let vault = Self::get_vault(env.clone()).unwrap();
        let admin = Self::get_admin(env.clone()).unwrap();
        if caller != vault && caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
    }

    fn sorted_insert(_env: &Env, index: &mut Vec<Address>, addr: Address) {
        if index.iter().any(|a| a == addr) {
            return;
        }
        let mut pos: u32 = index.len();
        for (i, existing) in index.iter().enumerate() {
            if addr < existing {
                pos = i as u32;
                break;
            }
        }
        index.insert(pos, addr);
    }
}

#[cfg(test)]
mod test_freeze;
#[cfg(test)]
mod test_reentrancy;
#[cfg(test)]
mod test_error_codes;
#[cfg(test)]
mod test_events;
#[cfg(test)]
mod test_invariant;
#[cfg(test)]
mod test_multi_asset;
#[cfg(test)]
mod test_overflow_safe_math;
#[cfg(test)]
mod test_ttl_bump;
#[cfg(test)]
mod test_views;
#[cfg(test)]
mod test_admin_migration;
#[cfg(test)]
mod test_overdraft;
#[cfg(test)]
mod test_core;
#[cfg(test)]
mod test_withdraw;
#[cfg(test)]
mod test_admin;
#[cfg(test)]
mod test_pagination;
#[cfg(test)]
mod test_conservation;