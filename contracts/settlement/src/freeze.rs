//! Per-developer withdrawal freeze.
//!
//! Provides the [`freeze_developer`], [`unfreeze_developer`], and
//! [`is_developer_frozen`] helper functions used by the settlement contract
//! to block withdrawals for a specific developer while still allowing
//! deposits and other non-withdrawal operations.
//!
//! # Auth model
//!
//! | Entrypoint            | Authorized by           |
//! |-----------------------|-------------------------|
//! | `freeze_developer`    | `admin.require_auth()`  |
//! | `unfreeze_developer`  | `admin.require_auth()`  |
//! | `is_developer_frozen` | Read-only, no auth      |

use soroban_sdk::{Address, Env, Symbol};

use crate::errors::SettlementError;
use crate::types::StorageKey;

/// Freeze a developer's withdrawals.
///
/// Only the admin may call. Sets `FrozenDeveloper(developer)` to `true`.
///
/// # Arguments
/// * `env` - Soroban environment.
/// * `caller` - Must be the admin; must authorize.
/// * `developer` - The developer address to freeze.
/// * `_reason` - Opaque label for off-chain indexing.
///
/// # Errors
/// * [`SettlementError::FreezeUnauthorized`] — caller is not the admin.
/// * [`SettlementError::DeveloperFrozen`] — developer is already frozen.
pub fn freeze_developer(
    env: Env,
    caller: Address,
    developer: Address,
    _reason: Symbol,
) -> Result<(), SettlementError> {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&StorageKey::Admin)
        .ok_or(SettlementError::NotInitialized)?;
    if caller != admin {
        return Err(SettlementError::FreezeUnauthorized);
    }
    let key = StorageKey::FrozenDeveloper(developer.clone());
    if env
        .storage()
        .persistent()
        .get::<_, bool>(&key)
        .unwrap_or(false)
    {
        return Err(SettlementError::DeveloperFrozen);
    }
    env.storage().persistent().set(&key, &true);
    env.storage().persistent().extend_ttl(
        &key,
        crate::types::PERSISTENT_BUMP_THRESHOLD,
        crate::types::PERSISTENT_BUMP_AMOUNT,
    );
    Ok(())
}

/// Unfreeze a developer's withdrawals.
///
/// Only the admin may call.
///
/// # Arguments
/// * `env` - Soroban environment.
/// * `caller` - Must be the admin; must authorize.
/// * `developer` - The developer address to unfreeze.
///
/// # Errors
/// * [`SettlementError::FreezeUnauthorized`] — caller is not the admin.
/// * [`SettlementError::DeveloperNotFrozen`] — developer is not frozen.
pub fn unfreeze_developer(
    env: Env,
    caller: Address,
    developer: Address,
) -> Result<(), SettlementError> {
    caller.require_auth();
    let admin: Address = env
        .storage()
        .instance()
        .get(&StorageKey::Admin)
        .ok_or(SettlementError::NotInitialized)?;
    if caller != admin {
        return Err(SettlementError::FreezeUnauthorized);
    }
    let key = StorageKey::FrozenDeveloper(developer.clone());
    let frozen: bool = env.storage().persistent().get(&key).unwrap_or(false);
    if !frozen {
        return Err(SettlementError::DeveloperNotFrozen);
    }
    env.storage().persistent().set(&key, &false);
    env.storage().persistent().extend_ttl(
        &key,
        crate::types::PERSISTENT_BUMP_THRESHOLD,
        crate::types::PERSISTENT_BUMP_AMOUNT,
    );
    Ok(())
}

/// Return `true` if the developer's withdrawals are currently frozen.
///
/// Read-only; no auth required.
pub fn is_developer_frozen(env: Env, developer: Address) -> bool {
    let key = StorageKey::FrozenDeveloper(developer);
    env.storage()
        .persistent()
        .get::<_, bool>(&key)
        .unwrap_or(false)
}
