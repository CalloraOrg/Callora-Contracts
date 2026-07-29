//! Event topic Symbol constructors for the Callora Yield per-account limits surface.
//!
//! This module centralizes every event topic string emitted by
//! [`crate::limits`] and the [`crate::CalloraYieldLimits`] contract into a
//! dedicated constructor function. Centralizing topics ensures byte-identity
//! is preserved across call sites and prevents accidental topic-name drift
//! between producers and off-chain indexers.
//!
//! Snapshot tests at the bottom of the file pin each topic to its raw byte
//! representation so accidental renames fail loudly in `cargo test` *before*
//! a pull request is opened.

#![allow(dead_code)]

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"init"` event topic.
///
/// Emitted when the Callora yield-limits contract is first initialized with
/// the admin address.
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

/// Returns the Symbol for the `"admin_nominated"` event topic.
///
/// Emitted when the current admin nominates a new admin via
/// [`crate::CalloraYieldLimits::set_admin`]. The nominated admin must call
/// [`crate::CalloraYieldLimits::accept_admin`] to complete the transfer.
pub fn event_admin_nominated(env: &Env) -> Symbol {
    Symbol::new(env, "admin_nominated")
}

/// Returns the Symbol for the `"admin_accepted"` event topic.
///
/// Emitted when the pending admin accepts the role via
/// [`crate::CalloraYieldLimits::accept_admin`], completing the two-step
/// handover.
pub fn event_admin_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "admin_accepted")
}

/// Returns the Symbol for the `"admin_cancelled"` event topic.
///
/// Emitted when the current admin cancels a pending admin transfer via
/// [`crate::CalloraYieldLimits::cancel_admin_transfer`].
pub fn event_admin_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "admin_cancelled")
}

/// Returns the Symbol for the `"default_limits_set"` event topic.
///
/// Emitted when the admin updates the global default caps via
/// [`crate::CalloraYieldLimits::set_default_limits`].
pub fn event_default_limits_set(env: &Env) -> Symbol {
    Symbol::new(env, "default_limits_set")
}

/// Returns the Symbol for the `"account_limits_set"` event topic.
///
/// Emitted when the admin sets (or overwrites) per-account caps via
/// [`crate::CalloraYieldLimits::set_account_limits`].
pub fn event_account_limits_set(env: &Env) -> Symbol {
    Symbol::new(env, "account_limits_set")
}

/// Returns the Symbol for the `"account_limits_cleared"` event topic.
///
/// Emitted when the admin removes a per-account cap override via
/// [`crate::CalloraYieldLimits::clear_account_limits`].
pub fn event_account_limits_cleared(env: &Env) -> Symbol {
    Symbol::new(env, "account_limits_cleared")
}

/// Returns the Symbol for the `"bet_placed"` event topic.
///
/// Emitted when the caller successfully increments their open-bet counter
/// via [`crate::CalloraYieldLimits::place_bet`].
pub fn event_bet_placed(env: &Env) -> Symbol {
    Symbol::new(env, "bet_placed")
}

/// Returns the Symbol for the `"bet_cleared"` event topic.
///
/// Emitted when the caller successfully decrements their open-bet counter
/// via [`crate::CalloraYieldLimits::clear_bet`].
pub fn event_bet_cleared(env: &Env) -> Symbol {
    Symbol::new(env, "bet_cleared")
}

/// Returns the Symbol for the `"position_opened"` event topic.
///
/// Emitted when the caller successfully increments their active-position
/// counter via [`crate::CalloraYieldLimits::open_position`].
pub fn event_position_opened(env: &Env) -> Symbol {
    Symbol::new(env, "position_opened")
}

/// Returns the Symbol for the `"position_closed"` event topic.
///
/// Emitted when the caller successfully decrements their active-position
/// counter via [`crate::CalloraYieldLimits::close_position`].
pub fn event_position_closed(env: &Env) -> Symbol {
    Symbol::new(env, "position_closed")
}

/// Returns the Symbol for the `"subscription_added"` event topic.
///
/// Emitted when the caller successfully increments their active-subscription
/// counter via [`crate::CalloraYieldLimits::subscribe`].
pub fn event_subscription_added(env: &Env) -> Symbol {
    Symbol::new(env, "subscription_added")
}

/// Returns the Symbol for the `"subscription_removed"` event topic.
///
/// Emitted when the caller successfully decrements their active-subscription
/// counter via [`crate::CalloraYieldLimits::unsubscribe`].
pub fn event_subscription_removed(env: &Env) -> Symbol {
    Symbol::new(env, "subscription_removed")
}

/// Returns the Symbol for the `"upgraded"` event topic.
///
/// Emitted when the admin upgrades the contract to a new WASM hash via
/// [`crate::CalloraYieldLimits::upgrade`].
pub fn event_upgraded(env: &Env) -> Symbol {
    Symbol::new(env, "upgraded")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Snapshot: proves `event_init` still maps to exactly the bytes for `"init"`.
    #[test]
    fn test_event_init_bytes() {
        let env = Env::default();
        assert_eq!(event_init(&env), Symbol::new(&env, "init"));
    }

    /// Snapshot: proves `event_admin_nominated` still maps to exactly the bytes for `"admin_nominated"`.
    #[test]
    fn test_event_admin_nominated_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_nominated(&env),
            Symbol::new(&env, "admin_nominated")
        );
    }

    /// Snapshot: proves `event_admin_accepted` still maps to exactly the bytes for `"admin_accepted"`.
    #[test]
    fn test_event_admin_accepted_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_accepted(&env),
            Symbol::new(&env, "admin_accepted")
        );
    }

    /// Snapshot: proves `event_admin_cancelled` still maps to exactly the bytes for `"admin_cancelled"`.
    #[test]
    fn test_event_admin_cancelled_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_cancelled(&env),
            Symbol::new(&env, "admin_cancelled")
        );
    }

    /// Snapshot: proves `event_default_limits_set` still maps to exactly the bytes for `"default_limits_set"`.
    #[test]
    fn test_event_default_limits_set_bytes() {
        let env = Env::default();
        assert_eq!(
            event_default_limits_set(&env),
            Symbol::new(&env, "default_limits_set")
        );
    }

    /// Snapshot: proves `event_account_limits_set` still maps to exactly the bytes for `"account_limits_set"`.
    #[test]
    fn test_event_account_limits_set_bytes() {
        let env = Env::default();
        assert_eq!(
            event_account_limits_set(&env),
            Symbol::new(&env, "account_limits_set")
        );
    }

    /// Snapshot: proves `event_account_limits_cleared` still maps to exactly the bytes for `"account_limits_cleared"`.
    #[test]
    fn test_event_account_limits_cleared_bytes() {
        let env = Env::default();
        assert_eq!(
            event_account_limits_cleared(&env),
            Symbol::new(&env, "account_limits_cleared")
        );
    }

    /// Snapshot: proves `event_bet_placed` still maps to exactly the bytes for `"bet_placed"`.
    #[test]
    fn test_event_bet_placed_bytes() {
        let env = Env::default();
        assert_eq!(event_bet_placed(&env), Symbol::new(&env, "bet_placed"));
    }

    /// Snapshot: proves `event_bet_cleared` still maps to exactly the bytes for `"bet_cleared"`.
    #[test]
    fn test_event_bet_cleared_bytes() {
        let env = Env::default();
        assert_eq!(event_bet_cleared(&env), Symbol::new(&env, "bet_cleared"));
    }

    /// Snapshot: proves `event_position_opened` still maps to exactly the bytes for `"position_opened"`.
    #[test]
    fn test_event_position_opened_bytes() {
        let env = Env::default();
        assert_eq!(
            event_position_opened(&env),
            Symbol::new(&env, "position_opened")
        );
    }

    /// Snapshot: proves `event_position_closed` still maps to exactly the bytes for `"position_closed"`.
    #[test]
    fn test_event_position_closed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_position_closed(&env),
            Symbol::new(&env, "position_closed")
        );
    }

    /// Snapshot: proves `event_subscription_added` still maps to exactly the bytes for `"subscription_added"`.
    #[test]
    fn test_event_subscription_added_bytes() {
        let env = Env::default();
        assert_eq!(
            event_subscription_added(&env),
            Symbol::new(&env, "subscription_added")
        );
    }

    /// Snapshot: proves `event_subscription_removed` still maps to exactly the bytes for `"subscription_removed"`.
    #[test]
    fn test_event_subscription_removed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_subscription_removed(&env),
            Symbol::new(&env, "subscription_removed")
        );
    }

    /// Snapshot: proves `event_upgraded` still maps to exactly the bytes for `"upgraded"`.
    #[test]
    fn test_event_upgraded_bytes() {
        let env = Env::default();
        assert_eq!(event_upgraded(&env), Symbol::new(&env, "upgraded"));
    }
}
