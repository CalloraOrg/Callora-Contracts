//! Admin module with a cool-off window between upgrade actions.
//!
//! Enforces a cooldown period between critical upgrade actions to prevent
//! rapid abuse. This implements the requirements for the GrantFox FWC26 campaign.

use soroban_sdk::{contracterror, Address, Env, Symbol};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum UpgradeError {
    /// The cooldown period for upgrades has not yet elapsed.
    CooldownNotElapsed = 1,
    /// Arithmetic overflow.
    Overflow = 2,
}

const LAST_UPGRADE_TIME_KEY: &str = "last_upg_tm";
const UPGRADE_COOLDOWN_KEY: &str = "upg_cooldown";

/// Default cooldown is 24 hours (86400 seconds).
pub const DEFAULT_COOLDOWN_SECONDS: u64 = 86400;

/// Set the cooldown window (in seconds).
///
/// Requires auth from the caller.
pub fn set_cooldown(env: &Env, caller: &Address, cooldown: u64) {
    caller.require_auth();
    env.storage()
        .instance()
        .set(&Symbol::new(env, UPGRADE_COOLDOWN_KEY), &cooldown);
}

/// Retrieve the current cooldown window.
///
/// Returns the configured cooldown window, or `DEFAULT_COOLDOWN_SECONDS` if not set.
pub fn get_cooldown(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get::<_, u64>(&Symbol::new(env, UPGRADE_COOLDOWN_KEY))
        .unwrap_or(DEFAULT_COOLDOWN_SECONDS)
}

/// Verify that the cooldown period has elapsed since the last upgrade.
///
/// Requires auth from the caller. Returns `Ok(())` if the cooldown has elapsed
/// or if this is the first upgrade. Otherwise, returns `UpgradeError::CooldownNotElapsed`.
/// Also updates the last upgrade time to the current ledger timestamp upon success.
pub fn check_and_record_upgrade(env: &Env, caller: &Address) -> Result<(), UpgradeError> {
    caller.require_auth();

    let current_time = env.ledger().timestamp();
    let last_time = env
        .storage()
        .instance()
        .get::<_, u64>(&Symbol::new(env, LAST_UPGRADE_TIME_KEY))
        .unwrap_or(0);

    let cooldown = get_cooldown(env);

    if last_time != 0 {
        let elapsed = current_time
            .checked_sub(last_time)
            .ok_or(UpgradeError::Overflow)?;
        if elapsed < cooldown {
            return Err(UpgradeError::CooldownNotElapsed);
        }
    }

    env.storage()
        .instance()
        .set(&Symbol::new(env, LAST_UPGRADE_TIME_KEY), &current_time);

    Ok(())
}
