import re

with open('contracts/settlement/src/lib.rs', 'r') as f:
    content = f.read()

# Fix imports
content = content.replace(
    'use soroban_sdk::{contract, contractimpl, contracttype, Address, Env};',
    'use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, token};\npub mod batch;'
)

# Fix duplicate migrate functions
migrate_dup = '''    /// Migrate a single developer's V1 balance to V2 (admin only).
    pub fn migrate_developer_balance(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        migrate::migrate_single_developer(&env, &caller, &developer)
    }

    /// Migrate a single developer's V1 balance to V2 (admin only).
    pub fn migrate_single_dev_v2(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        migrate::migrate_single_developer(&env, &caller, &developer)
    }

    /// Migrate a single developer's V1 balance to V2 (admin only).
    pub fn migrate_developer_balance(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        migrate::migrate_single_developer(&env, &caller, &developer)
    }

    /// Migrate a single developer's V1 balance to V2 (admin only).
    pub fn migrate_single_dev_v2(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        migrate::migrate_single_developer(&env, &caller, &developer)
    }'''

migrate_single = '''    /// Migrate a single developer's V1 balance to V2 (admin only).
    pub fn migrate_developer_balance(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        migrate::migrate_single_developer(&env, &caller, &developer)
    }

    /// Migrate a single developer's V1 balance to V2 (admin only).
    pub fn migrate_single_dev_v2(
        env: Env,
        caller: Address,
        developer: Address,
    ) -> Result<(), SettlementError> {
        migrate::migrate_single_developer(&env, &caller, &developer)
    }'''

content = content.replace(migrate_dup, migrate_single)

# Fix get_global_pool
content = content.replace(
    '.get(&StorageKey::GlobalPool)',
    '.get::<_, GlobalPool>(&StorageKey::GlobalPool)'
)

# Fix withdraw_developer_balance and add helpers
bad_withdraw = '''        Self::require_claim_window_open(&env, &developer)?;

        let usdc_address = Self::get_usdc_token(env.clone())?;
        let current_balance: i128 = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Vault)
            .unwrap();
        vault.require_auth();
        let total = env
            .storage()
            .instance()
            .get::<_, i128>(&DataKey::TotalSettled)
            .unwrap_or(0);
        let new_total = total.checked_add(amount).unwrap();
        env.storage()
            .instance()
            .set(&DataKey::TotalSettled, &new_total);
    }'''

good_withdraw = '''        Self::require_claim_window_open(&env, &developer)?;

        let balance_key = StorageKey::DeveloperBalance(developer.clone(), usdc_address.clone());
        let current_balance: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);
        
        if current_balance < amount {
            return Err(SettlementError::InsufficientDeveloperBalance);
        }

        Self::check_daily_withdraw_limit(&env, &developer, amount)?;

        let new_balance = current_balance.checked_sub(amount).ok_or(SettlementError::DeveloperBalanceUnderflow)?;
        env.storage().persistent().set(&balance_key, &new_balance);

        usdc.transfer(&contract_address, &recipient, &amount);

        env.events().publish(
            (events::event_developer_withdraw(&env), developer.clone()),
            DeveloperWithdrawEvent {
                developer: developer.clone(),
                amount,
                remaining_balance: new_balance,
                to: recipient,
                token: usdc_address.clone(),
            },
        );

        Ok(())
    }

    pub fn batch_settle(
        env: Env,
        settlements: soroban_sdk::Vec<batch::SettleInput>,
    ) -> soroban_sdk::Vec<batch::SettleOutcome> {
        batch::batch_settle(&env, settlements)
    }

    fn require_authorized_caller(env: Env, caller: Address) {
        let vault = Self::get_vault(env.clone());
        let admin = Self::get_admin(env.clone());
        if caller != vault && caller != admin {
            env.panic_with_error(SettlementError::Unauthorized);
        }
    }

    fn sorted_insert(env: &Env, index: &mut soroban_sdk::Vec<Address>, address: Address) {
        if !index.contains(&address) {
            index.push_back(address);
        }
    }

    fn require_claim_window_open(env: &Env, developer: &Address) -> Result<(), SettlementError> {
        let window: Option<DeveloperClaimWindow> = env.storage().persistent().get(&StorageKey::DeveloperClaimWindow(developer.clone()));
        if let Some(w) = window {
            let now = env.ledger().timestamp();
            if now < w.start_ts || now > w.end_ts {
                return Err(SettlementError::ClaimWindowClosed);
            }
        }
        Ok(())
    }

    fn check_daily_withdraw_limit(env: &Env, developer: &Address, amount: i128) -> Result<(), SettlementError> {
        let cap_key = StorageKey::DailyWithdrawCap(developer.clone());
        if let Some(cap) = env.storage().persistent().get::<_, i128>(&cap_key) {
            let today = env.ledger().timestamp() / 86400;
            let state_key = StorageKey::WithdrawalToday(developer.clone());
            let mut state: DailyWithdrawState = env.storage().persistent().get(&state_key).unwrap_or(DailyWithdrawState { day: today, amount: 0 });
            
            if state.day != today {
                state.day = today;
                state.amount = 0;
            }
            
            let new_amount = state.amount.checked_add(amount).ok_or(SettlementError::DeveloperOverflow)?;
            if new_amount > cap {
                return Err(SettlementError::DailyWithdrawCapExceeded);
            }
            
            state.amount = new_amount;
            env.storage().persistent().set(&state_key, &state);
        }
        Ok(())
    }'''

content = content.replace(bad_withdraw, good_withdraw)

with open('contracts/settlement/src/lib.rs', 'w') as f:
    f.write(content)
