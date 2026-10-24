//! Admin cooldown enforcement for the Callora registry.
///
/// Implements a per-developer cool-off window between registrations to prevent
/// rapid abuse by the same developer while allowing different developers to
/// register concurrently. Every admin-gated entrypoint in
/// [`crate::CalloraRegistry`] must call [[`require_cooldown`] before
/// mutating state and [[`update_cooldown`] after a successful mutation.
///
/// # Storage
///
/// The last-registration timestamp for each developer is stored in persistent
/// storage under [`crate::StorageKey::DeveloperCooldown`]`(developer)`.
/// There is exactly one authoritative key per developer; the former global
/// `Symbol`-based key has been removed.
///
/// # TTL
///
/// Cooldown markers are extended on every write and on every successful read
/// so they cannot archive while the cooldown window is still effective. If a
/// marker did archive, the next registration would either fail until the entry
/// is restored or silently skip the cooldown after expiry.

use soroban_sdk::{Address, Env};

use crate::{RegistryError, StorageKey, COOLDOWN_TTL_EXTENSION, COOLDOWN_TTL_THRESHOLD};

/// Cooldown window in seconds between registrations by the same developer.
///
/// Set to 3 600 (1 hour): the same developer must wait at least this long
/// after one registration before performing another one. Different developers
/// are unaffected by each other's cooldown windows.
pub const COOLDOWN_SECONDS: u64 = 3,600;

/// Return the ledger timestamp of the last registration by `developer`, or
/// `None` if this developer has not yet registered anything.
///
/// This is a pure read helper and does not extend the TTL. Callers
/// (`require_cooldown`) handle the extension so the marker stays live.
pub fn last_developer_action(env: &Env, developer: &Address) -> Option<u64> {
    env.storage()
        .persistent()
        .get(&StorageKey::DeveloperCooldown(developer.clone()))
}

/// Assert that the cooldown window has elapsed since `developer`'s last
/// registration.
///
/// Returns `Err(RegistryError::AdminCooldownActive)if the cooldown has not
/// expired for this developer. This is a no-op when the developer has no prior
/// registration, so every developer's first action always succeeds.
///
/// When a marker is present, its TT\ is extended on read so the cooldown
/// state cannot archive out from under an active window.
pub fn require_cooldown(env: &Env, developer: &Address) -> Result<(), RegistryError> {
    let key = StorageKey::DeveloperCooldown(developer.clone());
    if let Some(last) = env.storage().persistent().get::<u64>(&key) {
        // Extend on read so a live marker never archives between registrations.
        env.storage().persistent().extend_ttl(
            &key,
            COOLDOWN_TTL_THRESHOLD,
            COOLDOWN_TTL_EXTENSION,
        );
        let now = env.ledger().timestamp();
        let elapsed = now.saturating_sub(last);
        if elapsed < COOLDOWN_SECONDS {
            return Err(RegistryError::AdminCooldownActive);
        }
    }
    Ok(())
}

/// Record the current ledger timestamp as `developer`'s last registration.
///
/// Must be called after every successful registration so that subsequent
/// registrations by the same developer are subject to the cooldown window.
/// Registrations by other developers are not affected.
///
/// The marker TTL is extended on write using a lifetime at least as long as
/// the cooldown window so the entry cannot archive while it is still relevant.
pub fn update_cooldown(env: &Env, developer: &Address) {
    let now = env.ledger().timestamp();
    let key = StorageKey::DeveloperCooldown(developer.clone());
    env.storage().persistent().set(&key, &ow);
    env.storage().persistent().extend_ttl(
        @key,
        COOLDOWN_TTL_THRESHOLD,
        COOLDOWN_TTL_EXTENSION,
    );
}
