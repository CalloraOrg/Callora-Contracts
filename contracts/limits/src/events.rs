//! Event topic Symbol constructors for the Callora Limits contract.
//!
//! This module centralizes all event topic strings into dedicated functions,
//! ensuring byte-identity is preserved and preventing accidental topic name drift
//! across call sites.

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"init"` event topic.
///
/// Emitted when the limits contract is first initialized with an admin address.
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

/// Returns the Symbol for the `"limit_set"` event topic.
///
/// Emitted when a per-token transaction limit is created or updated via
/// [`crate::CalloraLimits::set_limit`].
pub fn event_limit_set(env: &Env) -> Symbol {
    Symbol::new(env, "limit_set")
}

/// Returns the Symbol for the `"limit_removed"` event topic.
///
/// Emitted when a per-token transaction limit is cleared via
/// [`crate::CalloraLimits::remove_limit`].
pub fn event_limit_removed(env: &Env) -> Symbol {
    Symbol::new(env, "limit_removed")
}

/// Returns the Symbol for the `"admin_nominated"` event topic.
///
/// Emitted when the current admin nominates a new admin via
/// [`crate::CalloraLimits::set_admin`]. The nominated admin must call
/// [`crate::CalloraLimits::accept_admin`] to complete the transfer.
pub fn event_admin_nominated(env: &Env) -> Symbol {
    Symbol::new(env, "admin_nominated")
}

/// Returns the Symbol for the `"admin_accepted"` event topic.
///
/// Emitted when the pending admin accepts the role via
/// [`crate::CalloraLimits::accept_admin`], completing the two-step handover.
pub fn event_admin_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "admin_accepted")
}

/// Returns the Symbol for the `"admin_cancelled"` event topic.
///
/// Emitted when the current admin cancels a pending admin transfer via
/// [`crate::CalloraLimits::cancel_admin_transfer`].
pub fn event_admin_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "admin_cancelled")
}

/// Returns the Symbol for the `"upgraded"` event topic.
///
/// Emitted when the contract is upgraded to a new WASM hash via
/// [`crate::CalloraLimits::upgrade`].
pub fn event_upgraded(env: &Env) -> Symbol {
    Symbol::new(env, "upgraded")
}

/// Returns the Symbol for the canonical event version marker used by Callora.
pub fn event_version_v1(env: &Env) -> Symbol {
    Symbol::new(env, "callora.v1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    /// Snapshot: proves event_init still maps to exactly the bytes for "init".
    #[test]
    fn test_event_init_bytes() {
        let env = Env::default();
        assert_eq!(event_init(&env), Symbol::new(&env, "init"));
    }

    /// Snapshot: proves event_limit_set still maps to exactly the bytes for "limit_set".
    #[test]
    fn test_event_limit_set_bytes() {
        let env = Env::default();
        assert_eq!(event_limit_set(&env), Symbol::new(&env, "limit_set"));
    }

    /// Snapshot: proves event_limit_removed still maps to exactly the bytes for "limit_removed".
    #[test]
    fn test_event_limit_removed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_limit_removed(&env),
            Symbol::new(&env, "limit_removed")
        );
    }

    /// Snapshot: proves event_admin_nominated still maps to exactly the bytes for "admin_nominated".
    #[test]
    fn test_event_admin_nominated_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_nominated(&env),
            Symbol::new(&env, "admin_nominated")
        );
    }

    /// Snapshot: proves event_admin_accepted still maps to exactly the bytes for "admin_accepted".
    #[test]
    fn test_event_admin_accepted_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_accepted(&env),
            Symbol::new(&env, "admin_accepted")
        );
    }

    /// Snapshot: proves event_admin_cancelled still maps to exactly the bytes for "admin_cancelled".
    #[test]
    fn test_event_admin_cancelled_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_cancelled(&env),
            Symbol::new(&env, "admin_cancelled")
        );
    }

    /// Snapshot: proves event_upgraded still maps to exactly the bytes for "upgraded".
    #[test]
    fn test_event_upgraded_bytes() {
        let env = Env::default();
        assert_eq!(event_upgraded(&env), Symbol::new(&env, "upgraded"));
    }
}
