//! Limits module for per-developer minimum balance.

use soroban_sdk::{Env, Address, Symbol, contracterror, contracttype};
use crate::errors::SettlementError;
use crate::types::StorageKey;

/// Set the minimum balance for a developer.
///
/// # Arguments
/// * `env` - Execution environment.
/// * `caller` - Must be admin.
/// * `developer` - Target developer address.
/// * `min_balance` - Minimum balance in token micro‑units (>= 0).
pub fn set_developer_min_balance(env: Env, caller: Address, developer: Address, min_balance: i128) {
    // Auth check – admin only.
    caller.require_auth();
    let admin = crate::lib::CalloraSettlement::get_admin(env.clone());
    if caller != admin {
        env.panic_with_error(SettlementError::Unauthorized);
    }
    if min_balance < 0 {
        panic!("minimum balance must be non‑negative");
    }
    // Store the value.
    env.storage().persistent().set(&StorageKey::DeveloperMinBalance(developer.clone()), &min_balance);
    // Optional TTL similar to other persistent entries.
    env.storage().persistent().extend_ttl(&StorageKey::DeveloperMinBalance(developer), 50000, 50000);
}

/// Retrieve the minimum balance for a developer. Returns `0` if not set.
pub fn get_developer_min_balance(env: Env, developer: Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::DeveloperMinBalance(developer))
        .unwrap_or(0)
}
