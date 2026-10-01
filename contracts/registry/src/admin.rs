//! Admin cooldown enforcement for the Callora registry.
//!
//! Implements a cool-off window between critical admin actions to prevent
//! rapid abuse. Every admin-gated entrypoint in
//! [`crate::CalloraRegistry`] must call [`require_cooldown`] before
//! mutating state and [`update_cooldown`] after a successful mutation.

use soroban_sdk::{Env, Symbol};

use crate::RegistryError;

/// Cooldown window in seconds between admin actions.
///
/// Set to 3 600 (1 hour): the admin must wait at least this long after one
/// registration or other critical action before performing the next one.
pub const COOLDOWN_SECONDS: u64 = 3_600;

/// Instance storage key for the last admin action timestamp.
pub(crate) const LAST_ADMIN_ACTION_KEY: &str = "last_admin_action";

/// Return the ledger timestamp of the last admin action, or `None` if no
/// action has been performed yet (e.g. right after `init`).
pub fn last_admin_action(env: &Env) -> Option<u64> {
    env.storage()
        .instance()
        .get(&Symbol::new(env, LAST_ADMIN_ACTION_KEY))
}

/// Assert that the cooldown window has elapsed since the last admin action.
///
/// Returns `Err(RegistryError::AdminCooldownActive)` if the cooldown has not
/// expired. This is a no-op when no prior action has been recorded, so the
/// first admin action always succeeds.
pub fn require_cooldown(env: &Env) -> Result<(), RegistryError> {
    if let Some(last) = last_admin_action(env) {
        let now = env.ledger().timestamp();
        let elapsed = now.saturating_sub(last);
        if elapsed < COOLDOWN_SECONDS {
            return Err(RegistryError::AdminCooldownActive);
        }
    }
    Ok(())
}

/// Record the current ledger timestamp as the last admin action.
///
/// Must be called after every successful admin-gated mutation so that
/// subsequent actions are subject to the cooldown window.
pub fn update_cooldown(env: &Env) {
    let now = env.ledger().timestamp();
    env.storage()
        .instance()
        .set(&Symbol::new(env, LAST_ADMIN_ACTION_KEY), &now);
}
