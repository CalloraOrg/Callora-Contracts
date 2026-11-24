#![no_std]

pub mod errors;
pub mod events;
pub mod migrate;
mod views;

pub use errors::StakeError;
pub use views::{
    capabilities, CAP_DELEGATION, CAP_REWARDS, CAP_SLASHING, CAP_STAKE_UNSTAKE, CAP_STAKE_VIEW,
    CAP_WITHDRAW_TIMELOCK, IMPLEMENTED_CAPABILITIES, SUPPORTED_CAPABILITIES,
};

pub use migrate::{CalloraStakeMigrate, CurrentStake, LegacyStake, StakeMigrateError, StorageKey};

use soroban_sdk::{contract, contractimpl, contracttype, token, Address, Env};

/// Number of ledgers in a single day, assuming ~5 s close time.
pub const LEDGERS_PER_DAY: u32 = 17_280;

/// Instance storage TTL threshold (30 days).
pub const INSTANCE_TTL_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;

/// Instance storage TTL extension target (60 days).
pub const INSTANCE_TTL_EXTEND_TO: u32 = LEDGERS_PER_DAY * 60;

/// Persistent storage TTL threshold (30 days).
pub const PERSISTENT_TTL_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;

/// Persistent storage TTL extension target (60 days).
pub const PERSISTENT_TTL_EXTEND_TO: u32 = LEDGERS_PER_DAY * 60;

/// Storage keys for [`CalloraStake`].
///
/// Kept completely distinct from [`migrate::StorageKey`] so staking state
/// never collides with migration state.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StakeStorageKey {
    /// Instance: configured administrator address.
    Admin,
    /// Instance: SEP-41 staking token address.
    Token,
    /// Instance: aggregate total tokens staked across all stakers.
    TotalStaked,
    /// Persistent: per-staker staked balance (`i128`).
    StakerBalance(Address),
}

#[contract]
pub struct CalloraStake;

#[contractimpl]
impl CalloraStake {
    /// Initialize the staking contract with an administrator and staking token.
    ///
    /// May only be called once. Subsequent calls return
    /// [`StakeError::AlreadyInitialized`].
    pub fn init(env: Env, admin: Address, token: Address) -> Result<(), StakeError> {
        admin.require_auth();
        if env.storage().instance().has(&StakeStorageKey::Admin) {
            return Err(StakeError::AlreadyInitialized);
        }

        env.storage()
            .instance()
            .set(&StakeStorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&StakeStorageKey::Token, &token);
        env.storage()
            .instance()
            .set(&StakeStorageKey::TotalStaked, &0i128);
        extend_instance_ttl(&env);

        events::publish_init(&env, &admin, &token);
        Ok(())
    }

    /// Stake `amount` of the configured token from `staker` into contract custody.
    ///
    /// Returns the staker's updated staked balance.
    pub fn stake(env: Env, staker: Address, amount: i128) -> Result<i128, StakeError> {
        staker.require_auth();

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StakeStorageKey::Token)
            .ok_or(StakeError::NotInitialized)?;

        if amount <= 0 {
            return Err(StakeError::InvalidAmount);
        }

        let balance_key = StakeStorageKey::StakerBalance(staker.clone());
        let current_balance: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);
        let new_balance = current_balance
            .checked_add(amount)
            .ok_or(StakeError::Overflow)?;

        let current_total: i128 = env
            .storage()
            .instance()
            .get(&StakeStorageKey::TotalStaked)
            .unwrap_or(0);
        let new_total = current_total
            .checked_add(amount)
            .ok_or(StakeError::Overflow)?;

        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&staker, &env.current_contract_address(), &amount);

        env.storage().persistent().set(&balance_key, &new_balance);
        extend_persistent_ttl(&env, &balance_key);

        env.storage()
            .instance()
            .set(&StakeStorageKey::TotalStaked, &new_total);
        extend_instance_ttl(&env);

        events::publish_staked(&env, &staker, amount, new_balance, new_total);
        Ok(new_balance)
    }

    /// Unstake `amount` of the configured token back to `staker`.
    ///
    /// Follows Checks-Effects-Interactions: state is updated (and zeroed
    /// entries are removed) before transferring tokens back to `staker`.
    /// Returns the staker's remaining staked balance.
    pub fn unstake(env: Env, staker: Address, amount: i128) -> Result<i128, StakeError> {
        staker.require_auth();

        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StakeStorageKey::Token)
            .ok_or(StakeError::NotInitialized)?;

        if amount <= 0 {
            return Err(StakeError::InvalidAmount);
        }

        let balance_key = StakeStorageKey::StakerBalance(staker.clone());
        let current_balance: i128 = env.storage().persistent().get(&balance_key).unwrap_or(0);
        if amount > current_balance {
            return Err(StakeError::InsufficientStake);
        }

        let new_balance = current_balance
            .checked_sub(amount)
            .ok_or(StakeError::Overflow)?;

        let current_total: i128 = env
            .storage()
            .instance()
            .get(&StakeStorageKey::TotalStaked)
            .unwrap_or(0);
        let new_total = current_total
            .checked_sub(amount)
            .ok_or(StakeError::Overflow)?;

        // Effects before interactions (CEI).
        if new_balance == 0 {
            env.storage().persistent().remove(&balance_key);
        } else {
            env.storage().persistent().set(&balance_key, &new_balance);
            extend_persistent_ttl(&env, &balance_key);
        }

        env.storage()
            .instance()
            .set(&StakeStorageKey::TotalStaked, &new_total);
        extend_instance_ttl(&env);

        // Interaction: transfer tokens back to staker.
        let token_client = token::Client::new(&env, &token_addr);
        token_client.transfer(&env.current_contract_address(), &staker, &amount);

        events::publish_unstaked(&env, &staker, amount, new_balance, new_total);
        Ok(new_balance)
    }

    /// Return the staked balance for `staker` (0 if none).
    pub fn get_stake(env: Env, staker: Address) -> i128 {
        if env.storage().instance().has(&StakeStorageKey::Admin) {
            extend_instance_ttl(&env);
        }
        let balance_key = StakeStorageKey::StakerBalance(staker);
        match env.storage().persistent().get(&balance_key) {
            Some(balance) => {
                extend_persistent_ttl(&env, &balance_key);
                balance
            }
            None => 0,
        }
    }

    /// Return the aggregate total tokens staked across all stakers.
    pub fn total_staked(env: Env) -> i128 {
        if env.storage().instance().has(&StakeStorageKey::Admin) {
            extend_instance_ttl(&env);
        }
        env.storage()
            .instance()
            .get(&StakeStorageKey::TotalStaked)
            .unwrap_or(0)
    }

    /// Return the configured staking token address.
    pub fn get_token(env: Env) -> Result<Address, StakeError> {
        let token_addr: Address = env
            .storage()
            .instance()
            .get(&StakeStorageKey::Token)
            .ok_or(StakeError::NotInitialized)?;
        extend_instance_ttl(&env);
        Ok(token_addr)
    }

    /// Return the bitmask of implemented staking capabilities.
    pub fn capabilities(env: Env) -> u64 {
        views::capabilities(&env)
    }
}

fn extend_instance_ttl(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
}

fn extend_persistent_ttl(env: &Env, key: &StakeStorageKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, PERSISTENT_TTL_EXTEND_TO);
}
