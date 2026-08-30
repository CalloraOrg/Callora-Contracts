//! Event topic Symbol constructors for the Callora Escrow contract.
//!
//! This module centralises all event topic strings into dedicated functions,
//! ensuring byte-identity is preserved and preventing accidental topic name
//! drift across call sites.

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"init"` event topic.
///
/// Emitted when the escrow contract is first initialized with an admin address
/// and a cooldown window.
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

/// Returns the Symbol for the `"cooldown_set"` event topic.
///
/// Emitted when the admin updates the global cool-off window via
/// [`crate::CalloraEscrow::set_cooldown`].
pub fn event_cooldown_set(env: &Env) -> Symbol {
    Symbol::new(env, "cooldown_set")
}

/// Returns the Symbol for the `"action"` event topic.
///
/// Emitted every time a guarded critical action is successfully executed,
/// recording the action tag so indexers can reconstruct the cool-off timeline.
pub fn event_action(env: &Env) -> Symbol {
    Symbol::new(env, "action")
}

/// Returns the Symbol for the `"admin_nominated"` event topic.
///
/// Emitted when the current admin nominates a new admin via
/// [`crate::CalloraEscrow::set_admin`]. The nominated admin must call
/// [`crate::CalloraEscrow::accept_admin`] to complete the transfer.
pub fn event_admin_nominated(env: &Env) -> Symbol {
    Symbol::new(env, "admin_nominated")
}

/// Returns the Symbol for the `"admin_accepted"` event topic.
///
/// Emitted when the pending admin accepts the role via
/// [`crate::CalloraEscrow::accept_admin`], completing the two-step handover.
pub fn event_admin_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "admin_accepted")
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

    /// Snapshot: proves event_cooldown_set still maps to exactly the bytes for "cooldown_set".
    #[test]
    fn test_event_cooldown_set_bytes() {
        let env = Env::default();
        assert_eq!(event_cooldown_set(&env), Symbol::new(&env, "cooldown_set"));
    }

    /// Snapshot: proves event_action still maps to exactly the bytes for "action".
    #[test]
    fn test_event_action_bytes() {
        let env = Env::default();
        assert_eq!(event_action(&env), Symbol::new(&env, "action"));
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
}
