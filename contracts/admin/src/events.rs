//! Event topic Symbol constructors for the Callora Admin contract.
//!
//! This module centralizes all event topic strings into dedicated functions,
//! ensuring byte-identity is preserved and preventing accidental topic name
//! drift across call sites. Every admin state transition emits exactly one
//! of the topics below so off-chain indexers can reconstruct the admin
//! lifecycle without polling storage.
//!
//! ## Event Overview
//!
//! | Topic                       | Trigger                                                       |
//! |-----------------------------|---------------------------------------------------------------|
//! | `"admin_init"`              | Contract initialized with the first admin (`init`)           |
//! | `"admin_nominated"`         | Current admin proposes a successor (`set_admin`)             |
//! | `"admin_changed"`           | Pending admin accepts the role and becomes the new admin     |
//! | `"admin_cancelled"`         | Current admin revokes a pending nomination                   |
//! | `"account_limits_set"`      | Admin sets per-account caps for a specific account           |
//! | `"account_limits_cleared"`  | Admin clears per-account caps (fallback to global defaults)  |
//! | `"default_limits_set"`      | Admin sets the global default caps                           |
//! | `"bet_consumed"`            | Account consumed one bet slot                                |
//! | `"bet_released"`            | Account released one bet slot                                |
//! | `"position_consumed"`       | Account consumed one position slot                           |
//! | `"position_released"`       | Account released one position slot                           |
//! | `"subscription_consumed"`   | Account consumed one subscription slot                       |
//! | `"subscription_released"`   | Account released one subscription slot                       |
//!
//! ## Topic Shape
//!
//! All events follow the canonical 2-topic shape used across Callora contracts:
//!
//! ```text
//! topics: (action: Symbol, caller: Address)
//! data:   <event-specific payload>
//! ```
//!
//! Off-chain indexers should filter on `topic[0]` (action) and scope queries
//! to the emitting contract address to avoid cross-contract topic collisions.

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"admin_init"` event topic.
///
/// Emitted exactly once, from [`crate::admin::init`], when the contract is
/// first deployed and its initial admin address is persisted.
///
/// Topics: `(admin_init, initial_admin: Address)`
/// Data:   `()` (no payload — the topic[1] value carries the admin identity)
pub fn event_admin_init(env: &Env) -> Symbol {
    Symbol::new(env, "admin_init")
}

/// Returns the Symbol for the `"admin_nominated"` event topic.
///
/// Emitted from [`crate::admin::set_admin`] when the current admin nominates
/// a new pending admin. The nominee has not yet accepted the role at this
/// point — they must call [`crate::admin::accept_admin`] to complete the
/// transfer.
///
/// Topics: `(admin_nominated, caller: Address)` — `caller` is the **current**
/// admin who issued the nomination.
/// Data:   `pending_admin: Address`
pub fn event_admin_nominated(env: &Env) -> Symbol {
    Symbol::new(env, "admin_nominated")
}

/// Returns the Symbol for the `"admin_changed"` event topic.
///
/// Emitted from [`crate::admin::accept_admin`] once the pending admin accepts
/// the role and becomes the new active admin. Pairing `admin_nominated` →
/// `admin_changed` lets indexers verify that a proposed rotation actually
/// completed and that the previous admin no longer holds the role.
///
/// Topics: `(admin_changed, caller: Address)` — `caller` is the **incoming**
/// admin who accepted (and therefore is now the active admin after this
/// event).
/// Data:   `(previous_admin: Address, new_admin: Address)`
pub fn event_admin_changed(env: &Env) -> Symbol {
    Symbol::new(env, "admin_changed")
}

/// Returns the Symbol for the `"admin_cancelled"` event topic.
///
/// Emitted from [`crate::admin::cancel_admin_transfer`] when the current admin
/// revokes a pending nomination. After this event the pending slot is empty
/// and the active admin remains the previous admin unchanged.
///
/// Topics: `(admin_cancelled, caller: Address)` — `caller` is the **current**
/// admin who issued the cancellation.
/// Data:   `cancelled_pending_admin: Address`
pub fn event_admin_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "admin_cancelled")
}

// ---------------------------------------------------------------------------
// Per-account limits event topics
// ---------------------------------------------------------------------------

/// Returns the Symbol for the `"account_limits_set"` event topic.
///
/// Emitted when the admin sets (or overwrites) per-account caps via
/// [`crate::limits::set_account_limits`].
///
/// Topics: `(account_limits_set, admin: Address)`
/// Data:   `(account: Address, AccountLimits)`
pub fn event_account_limits_set(env: &Env) -> Symbol {
    Symbol::new(env, "account_limits_set")
}

/// Returns the Symbol for the `"account_limits_cleared"` event topic.
///
/// Emitted when the admin clears per-account caps via
/// [`crate::limits::clear_account_limits`].
///
/// Topics: `(account_limits_cleared, admin: Address)`
/// Data:   `account: Address`
pub fn event_account_limits_cleared(env: &Env) -> Symbol {
    Symbol::new(env, "account_limits_cleared")
}

/// Returns the Symbol for the `"default_limits_set"` event topic.
///
/// Emitted when the admin sets the global default caps via
/// [`crate::limits::set_default_limits`].
///
/// Topics: `(default_limits_set, admin: Address)`
/// Data:   `AccountLimits`
pub fn event_default_limits_set(env: &Env) -> Symbol {
    Symbol::new(env, "default_limits_set")
}

/// Returns the Symbol for the `"bet_consumed"` event topic.
///
/// Emitted when an account successfully consumes one bet slot via
/// [`crate::limits::consume_bet`].
///
/// Topics: `(bet_consumed, account: Address)`
/// Data:   `(new_count: u32, cap: u32)`
pub fn event_bet_consumed(env: &Env) -> Symbol {
    Symbol::new(env, "bet_consumed")
}

/// Returns the Symbol for the `"bet_released"` event topic.
///
/// Emitted when an account successfully releases one bet slot via
/// [`crate::limits::release_bet`].
///
/// Topics: `(bet_released, account: Address)`
/// Data:   `new_count: u32`
pub fn event_bet_released(env: &Env) -> Symbol {
    Symbol::new(env, "bet_released")
}

/// Returns the Symbol for the `"position_consumed"` event topic.
///
/// Emitted when an account successfully consumes one position slot via
/// [`crate::limits::consume_position`].
///
/// Topics: `(position_consumed, account: Address)`
/// Data:   `(new_count: u32, cap: u32)`
pub fn event_position_consumed(env: &Env) -> Symbol {
    Symbol::new(env, "position_consumed")
}

/// Returns the Symbol for the `"position_released"` event topic.
///
/// Emitted when an account successfully releases one position slot via
/// [`crate::limits::release_position`].
///
/// Topics: `(position_released, account: Address)`
/// Data:   `new_count: u32`
pub fn event_position_released(env: &Env) -> Symbol {
    Symbol::new(env, "position_released")
}

/// Returns the Symbol for the `"subscription_consumed"` event topic.
///
/// Emitted when an account successfully consumes one subscription slot via
/// [`crate::limits::consume_subscription`].
///
/// Topics: `(subscription_consumed, account: Address)`
/// Data:   `(new_count: u32, cap: u32)`
pub fn event_subscription_consumed(env: &Env) -> Symbol {
    Symbol::new(env, "subscription_consumed")
}

/// Returns the Symbol for the `"subscription_released"` event topic.
///
/// Emitted when an account successfully releases one subscription slot via
/// [`crate::limits::release_subscription`].
///
/// Topics: `(subscription_released, account: Address)`
/// Data:   `new_count: u32`
pub fn event_subscription_released(env: &Env) -> Symbol {
    Symbol::new(env, "subscription_released")
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    /// Snapshot: proves `event_admin_init` still maps to exactly the bytes
    /// for `"admin_init"`. If this test fails, the topic was accidentally
    /// renamed — all off-chain indexers subscribed to that topic would
    /// silently stop receiving events.
    #[test]
    fn test_event_admin_init_bytes() {
        let env = Env::default();
        assert_eq!(event_admin_init(&env), Symbol::new(&env, "admin_init"));
    }

    /// Snapshot: proves `event_admin_nominated` still maps to exactly the
    /// bytes for `"admin_nominated"`.
    #[test]
    fn test_event_admin_nominated_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_nominated(&env),
            Symbol::new(&env, "admin_nominated")
        );
    }

    /// Snapshot: proves `event_admin_changed` still maps to exactly the
    /// bytes for `"admin_changed"`.
    #[test]
    fn test_event_admin_changed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_changed(&env),
            Symbol::new(&env, "admin_changed")
        );
    }

    /// Snapshot: proves `event_admin_cancelled` still maps to exactly the
    /// bytes for `"admin_cancelled"`.
    #[test]
    fn test_event_admin_cancelled_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_cancelled(&env),
            Symbol::new(&env, "admin_cancelled")
        );
    }

    /// Snapshot: proves `event_account_limits_set` maps to exactly
    /// `"account_limits_set"`.
    #[test]
    fn test_event_account_limits_set_bytes() {
        let env = Env::default();
        assert_eq!(
            event_account_limits_set(&env),
            Symbol::new(&env, "account_limits_set")
        );
    }

    /// Snapshot: proves `event_account_limits_cleared` maps to exactly
    /// `"account_limits_cleared"`.
    #[test]
    fn test_event_account_limits_cleared_bytes() {
        let env = Env::default();
        assert_eq!(
            event_account_limits_cleared(&env),
            Symbol::new(&env, "account_limits_cleared")
        );
    }

    /// Snapshot: proves `event_default_limits_set` maps to exactly
    /// `"default_limits_set"`.
    #[test]
    fn test_event_default_limits_set_bytes() {
        let env = Env::default();
        assert_eq!(
            event_default_limits_set(&env),
            Symbol::new(&env, "default_limits_set")
        );
    }

    /// Snapshot: proves `event_bet_consumed` maps to exactly
    /// `"bet_consumed"`.
    #[test]
    fn test_event_bet_consumed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_bet_consumed(&env),
            Symbol::new(&env, "bet_consumed")
        );
    }

    /// Snapshot: proves `event_bet_released` maps to exactly
    /// `"bet_released"`.
    #[test]
    fn test_event_bet_released_bytes() {
        let env = Env::default();
        assert_eq!(
            event_bet_released(&env),
            Symbol::new(&env, "bet_released")
        );
    }

    /// Snapshot: proves `event_position_consumed` maps to exactly
    /// `"position_consumed"`.
    #[test]
    fn test_event_position_consumed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_position_consumed(&env),
            Symbol::new(&env, "position_consumed")
        );
    }

    /// Snapshot: proves `event_position_released` maps to exactly
    /// `"position_released"`.
    #[test]
    fn test_event_position_released_bytes() {
        let env = Env::default();
        assert_eq!(
            event_position_released(&env),
            Symbol::new(&env, "position_released")
        );
    }

    /// Snapshot: proves `event_subscription_consumed` maps to exactly
    /// `"subscription_consumed"`.
    #[test]
    fn test_event_subscription_consumed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_subscription_consumed(&env),
            Symbol::new(&env, "subscription_consumed")
        );
    }

    /// Snapshot: proves `event_subscription_released` maps to exactly
    /// `"subscription_released"`.
    #[test]
    fn test_event_subscription_released_bytes() {
        let env = Env::default();
        assert_eq!(
            event_subscription_released(&env),
            Symbol::new(&env, "subscription_released")
        );
    }
}
