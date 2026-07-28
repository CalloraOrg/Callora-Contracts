//! Per-account limits for bets, positions, and subscriptions.
//!
//! Limit configuration is admin-controlled. Usage increments are authenticated
//! by the account whose state is being consumed and use checked arithmetic so
//! exceeding a configured cap fails without modifying stored usage.

use soroban_sdk::{contracttype, Address, Env};

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AccountLimits {
    pub max_bets: u32,
    pub max_positions: u32,
    pub max_subscriptions: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct AccountUsage {
    pub bets: u32,
    pub positions: u32,
    pub subscriptions: u32,
}

#[derive(Clone)]
#[contracttype]
enum StorageKey {
    Limits(Address),
    Usage(Address),
}

fn require_admin(env: &Env, caller: &Address) {
    caller.require_auth();
    let admin = crate::admin::get_admin(env)
        .unwrap_or_else(|| panic!("admin contract not initialized"));
    if *caller != admin {
        panic!("unauthorized");
    }
}

fn limits_for(env: &Env, account: &Address) -> AccountLimits {
    env.storage()
        .persistent()
        .get(&StorageKey::Limits(account.clone()))
        .unwrap_or(AccountLimits {
            max_bets: u32::MAX,
            max_positions: u32::MAX,
            max_subscriptions: u32::MAX,
        })
}

fn usage_for(env: &Env, account: &Address) -> AccountUsage {
    env.storage()
        .persistent()
        .get(&StorageKey::Usage(account.clone()))
        .unwrap_or(AccountUsage {
            bets: 0,
            positions: 0,
            subscriptions: 0,
        })
}

fn save_usage(env: &Env, account: &Address, usage: &AccountUsage) {
    env.storage()
        .persistent()
        .set(&StorageKey::Usage(account.clone()), usage);
}

/// Set all per-account state caps.
///
/// Only the current admin may update limits. A zero value disables the
/// corresponding category for the account; the default for an account with no
/// configured limits is unlimited.
pub fn set_account_limits(
    env: &Env,
    caller: &Address,
    account: &Address,
    max_bets: u32,
    max_positions: u32,
    max_subscriptions: u32,
) {
    require_admin(env, caller);
    env.storage().persistent().set(
        &StorageKey::Limits(account.clone()),
        &AccountLimits {
            max_bets,
            max_positions,
            max_subscriptions,
        },
    );
}

/// Return the configured caps for an account.
pub fn get_account_limits(env: &Env, account: &Address) -> AccountLimits {
    limits_for(env, account)
}

/// Return the current tracked usage for an account.
pub fn get_account_usage(env: &Env, account: &Address) -> AccountUsage {
    usage_for(env, account)
}

/// Consume one bet slot for an account.
///
/// The account must authorize the state change. The usage counter is updated
/// only when the configured cap has not been reached.
pub fn consume_bet(env: &Env, account: &Address) {
    account.require_auth();
    let limits = limits_for(env, account);
    let mut usage = usage_for(env, account);
    let next = usage
        .bets
        .checked_add(1)
        .unwrap_or_else(|| panic!("bet usage overflow"));
    if next > limits.max_bets {
        panic!("bet limit exceeded");
    }
    usage.bets = next;
    save_usage(env, account, &usage);
}

/// Consume one position slot for an account.
pub fn consume_position(env: &Env, account: &Address) {
    account.require_auth();
    let limits = limits_for(env, account);
    let mut usage = usage_for(env, account);
    let next = usage
        .positions
        .checked_add(1)
        .unwrap_or_else(|| panic!("position usage overflow"));
    if next > limits.max_positions {
        panic!("position limit exceeded");
    }
    usage.positions = next;
    save_usage(env, account, &usage);
}

/// Consume one subscription slot for an account.
pub fn consume_subscription(env: &Env, account: &Address) {
    account.require_auth();
    let limits = limits_for(env, account);
    let mut usage = usage_for(env, account);
    let next = usage
        .subscriptions
        .checked_add(1)
        .unwrap_or_else(|| panic!("subscription usage overflow"));
    if next > limits.max_subscriptions {
        panic!("subscription limit exceeded");
    }
    usage.subscriptions = next;
    save_usage(env, account, &usage);
}
