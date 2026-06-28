//! Per-developer minimum accrued-balance limits for settlement claims.
//!
//! A configured minimum is checked immediately before a developer withdrawal is
//! settled. The requirement is intentionally per developer rather than global so
//! operations can tune eligibility without affecting existing developers.

use crate::{SettlementError, StorageKey};
use soroban_sdk::{Address, Env};

const MIN_BALANCE_TTL_THRESHOLD: u32 = 50_000;
const MIN_BALANCE_TTL_EXTEND_TO: u32 = 50_000;

/// Set the minimum accrued balance required before a developer may claim.
///
/// The settlement admin is the only caller allowed to configure this value. A
/// minimum of `0` is allowed and behaves the same as an unset minimum.
/// Negative minimums are rejected because they would make the gate meaningless.
pub fn set_developer_min_balance(env: Env, caller: Address, developer: Address, min_balance: i128) {
    caller.require_auth();
    let admin = crate::CalloraSettlement::get_admin(env.clone());
    if caller != admin {
        env.panic_with_error(SettlementError::Unauthorized);
    }
    if min_balance < 0 {
        env.panic_with_error(SettlementError::AmountNotPositive);
    }

    let key = StorageKey::DeveloperMinBalance(developer);
    env.storage().persistent().set(&key, &min_balance);
    env.storage().persistent().extend_ttl(
        &key,
        MIN_BALANCE_TTL_THRESHOLD,
        MIN_BALANCE_TTL_EXTEND_TO,
    );
}

/// Retrieve a developer's configured claim minimum, or `0` if unset.
pub fn get_developer_min_balance(env: Env, developer: Address) -> i128 {
    env.storage()
        .persistent()
        .get(&StorageKey::DeveloperMinBalance(developer))
        .unwrap_or(0)
}

/// Ensure the developer has accrued enough balance to be eligible to claim.
///
/// This checks the balance before settlement. The minimum is an eligibility
/// threshold; once met, the developer may withdraw any amount otherwise allowed
/// by balance, daily cap, claim window, and contract-token checks.
pub fn require_developer_min_balance(
    env: &Env,
    developer: &Address,
    current_balance: i128,
) -> Result<(), SettlementError> {
    let min_balance = get_developer_min_balance(env.clone(), developer.clone());
    if min_balance > 0 && current_balance < min_balance {
        return Err(SettlementError::MinimumBalanceRequired);
    }
    Ok(())
}
