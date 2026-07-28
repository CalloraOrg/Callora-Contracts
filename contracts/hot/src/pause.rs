//! Pause / unpause module for the Callora Hot contract.
//!
//! # Purpose
//!
//! Provides a focused, admin-gated **circuit-breaker** that halts all
//! state-changing operations on the `hot` contract while allowing read-only
//! views to continue serving. Three functions are exposed:
//!
//! * [`is_paused`] — read the current flag (no auth, no side-effects).
//! * [`do_pause`] — set the paused flag to `true` (enforces cool-off).
//! * [`do_unpause`] — clear the paused flag to `false` (enforces cool-off).
//!
//! Both state-changing functions are cool-off-guarded (via
//! [`crate::admin::guard`]) and require the caller to already have passed the
//! admin check performed by the entrypoints in `lib.rs`.
//!
//! # State
//!
//! The paused state lives in instance storage under [`crate::StorageKey::Paused`]
//! as a `bool`. It defaults to `false` when the key is absent (pre-init reads).
//!
//! # Design rationale
//!
//! Extracting the pause logic into its own module:
//! - keeps `lib.rs` focused on wiring and dispatch;
//! - makes the guard semantics easy to read and unit-test in isolation;
//! - provides a clean extension point for future circuit-breaker features
//!   (e.g. partial pausing per subsystem).

use crate::admin;
use crate::errors::HotError;
use crate::events;
use crate::StorageKey;
use soroban_sdk::{Address, Env, Symbol};

// ---------------------------------------------------------------------------
// Public interface
// ---------------------------------------------------------------------------

/// Return `true` when the contract is currently paused.
///
/// This is a pure read and does not require initialization or authentication.
/// Absent storage (pre-`init`) is treated as `false` (not paused).
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::Paused)
        .unwrap_or(false)
}

/// Activate the circuit-breaker.
///
/// Sets the [`StorageKey::Paused`] flag to `true` and records the action
/// timestamp for the cool-off guard. Emits a dedicated `paused` event.
///
/// # Preconditions (must be satisfied by the caller in `lib.rs`)
/// * `caller` has already passed `require_admin`.
///
/// # Errors
/// * [`HotError::AlreadyPaused`] — the contract is already paused.
/// * [`HotError::CooldownActive`] — a `"pause"` action fired within the
///   current cool-off window.
///
/// # Events
/// Emits `paused` with `caller` as topic and no extra data.
pub fn do_pause(env: &Env, caller: &Address, action: &Symbol) -> Result<(), HotError> {
    if is_paused(env) {
        return Err(HotError::AlreadyPaused);
    }

    admin::guard(env, action)?;

    env.storage().instance().set(&StorageKey::Paused, &true);

    env.events()
        .publish((events::event_paused(env), caller.clone()), ());

    Ok(())
}

/// Deactivate the circuit-breaker.
///
/// Clears the [`StorageKey::Paused`] flag to `false` and records the action
/// timestamp for the cool-off guard. Emits a dedicated `unpaused` event.
///
/// # Preconditions (must be satisfied by the caller in `lib.rs`)
/// * `caller` has already passed `require_admin`.
///
/// # Errors
/// * [`HotError::NotPaused`] — the contract is not currently paused.
/// * [`HotError::CooldownActive`] — an `"unpause"` action fired within the
///   current cool-off window.
///
/// # Events
/// Emits `unpaused` with `caller` as topic and no extra data.
pub fn do_unpause(env: &Env, caller: &Address, action: &Symbol) -> Result<(), HotError> {
    if !is_paused(env) {
        return Err(HotError::NotPaused);
    }

    admin::guard(env, action)?;

    env.storage().instance().set(&StorageKey::Paused, &false);

    env.events()
        .publish((events::event_unpaused(env), caller.clone()), ());

    Ok(())
}

// ---------------------------------------------------------------------------
// Unit tests (pause module internals)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CalloraHot, CalloraHotClient, HotError};
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{Address, Env};

    /// Helper: register + init a fresh hot contract and return the client + admin.
    fn setup_with(cooldown: u64) -> (Env, Address, CalloraHotClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let signer = Address::generate(&env);
        let id = env.register(CalloraHot, ());
        let client = CalloraHotClient::new(&env, &id);
        client.init(&admin, &signer, &Some(cooldown));
        (env, admin, client)
    }

    /// Advance ledger timestamp by `secs` seconds.
    fn advance(env: &Env, secs: u64) {
        let now = env.ledger().timestamp();
        env.ledger().set_timestamp(now + secs);
    }

    // -----------------------------------------------------------------------
    // is_paused
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_paused_false_after_init() {
        let (_env, _admin, client) = setup_with(60);
        assert!(!client.is_paused());
    }

    // -----------------------------------------------------------------------
    // do_pause
    // -----------------------------------------------------------------------

    #[test]
    fn test_pause_sets_flag() {
        let (_env, admin, client) = setup_with(60);
        client.pause(&admin);
        assert!(client.is_paused());
    }

    #[test]
    fn test_pause_requires_admin() {
        let (env, _admin, client) = setup_with(60);
        let intruder = Address::generate(&env);
        let res = client.try_pause(&intruder);
        assert_eq!(res, Err(Ok(HotError::Unauthorized)));
    }

    #[test]
    fn test_pause_already_paused_returns_error() {
        let (_env, admin, client) = setup_with(60);
        client.pause(&admin);
        // AlreadyPaused is checked before the cooldown guard; even without
        // advancing time we get AlreadyPaused, not CooldownActive.
        let res = client.try_pause(&admin);
        assert_eq!(res, Err(Ok(HotError::AlreadyPaused)));
    }

    #[test]
    fn test_pause_blocked_by_cooldown() {
        let (env, admin, client) = setup_with(300);
        // Arm the pause cooldown, then clear the paused flag via unpause.
        client.pause(&admin);
        client.unpause(&admin);
        // Advance past both cooldowns (both fired at t=0), then re-arm them.
        advance(&env, 300);
        client.pause(&admin);
        client.unpause(&admin);
        // Both cooldowns fired at t=300; advance only 299 s — pause still cooling.
        advance(&env, 299);
        let res = client.try_pause(&admin);
        assert_eq!(res, Err(Ok(HotError::CooldownActive)));
    }

    // -----------------------------------------------------------------------
    // do_unpause
    // -----------------------------------------------------------------------

    #[test]
    fn test_unpause_clears_flag() {
        let (_env, admin, client) = setup_with(60);
        client.pause(&admin);
        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    #[test]
    fn test_unpause_requires_admin() {
        let (env, admin, client) = setup_with(60);
        client.pause(&admin);
        let intruder = Address::generate(&env);
        let res = client.try_unpause(&intruder);
        assert_eq!(res, Err(Ok(HotError::Unauthorized)));
    }

    #[test]
    fn test_unpause_not_paused_returns_error() {
        let (_env, admin, client) = setup_with(60);
        // Contract is not paused; unpause should return NotPaused.
        let res = client.try_unpause(&admin);
        assert_eq!(res, Err(Ok(HotError::NotPaused)));
    }

    #[test]
    fn test_unpause_blocked_by_cooldown() {
        let (env, admin, client) = setup_with(300);
        // Arm both cooldowns at t=0, then advance past both.
        client.pause(&admin);
        client.unpause(&admin);
        advance(&env, 300);
        // Re-pause at t=300 (pause cooldown clear). Now advance 300 more so
        // the pause cooldown clears again, then unpause — arming the unpause
        // cooldown at t=600.
        client.pause(&admin);
        advance(&env, 300); // t=600; pause cooldown from t=300 is now clear
        client.unpause(&admin); // unpause fires at t=600
        // The contract is now unpaused. Re-pause immediately (pause cooldown
        // cleared at t=600, so this succeeds).
        client.pause(&admin); // pause fires at t=600
        // Now: contract is paused, unpause cooldown armed at t=600 (window=300).
        // Advance only 299 s — unpause still cooling.
        advance(&env, 299); // t=899
        let res = client.try_unpause(&admin);
        assert_eq!(res, Err(Ok(HotError::CooldownActive)));
    }

    // -----------------------------------------------------------------------
    // Round-trip
    // -----------------------------------------------------------------------

    #[test]
    fn test_pause_unpause_round_trip() {
        let (env, admin, client) = setup_with(1);
        assert!(!client.is_paused());
        client.pause(&admin);
        assert!(client.is_paused());
        // Advance 1 second so the unpause cooldown clears.
        advance(&env, 1);
        client.unpause(&admin);
        assert!(!client.is_paused());
    }

    // -----------------------------------------------------------------------
    // Pre-init reads
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_paused_before_init_returns_false() {
        let env = Env::default();
        let id = env.register(CalloraHot, ());
        let client = CalloraHotClient::new(&env, &id);
        // is_paused is a pure read — defaults to false pre-init.
        assert!(!client.is_paused());
    }
}
