//! Event topic Symbol constructors for the Callora Distribute contract.
//!
//! This module centralizes all event topic strings into dedicated functions,
//! ensuring byte-identity is preserved and preventing accidental topic name drift
//! across call sites.

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"init"` event topic.
///
/// Emitted when the distribute contract is first initialized with an admin
/// and global per-account cap.
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

/// Returns the Symbol for the `"open"` event topic.
///
/// Emitted when a new state entry is opened for an account.
pub fn event_open(env: &Env) -> Symbol {
    Symbol::new(env, "open")
}

/// Returns the Symbol for the `"close"` event topic.
///
/// Emitted when an existing state entry is closed for an account.
pub fn event_close(env: &Env) -> Symbol {
    Symbol::new(env, "close")
}

/// Returns the Symbol for the `"batch_open"` event topic.
///
/// Emitted once per item during a `batch_open` call.
pub fn event_batch_open(env: &Env) -> Symbol {
    Symbol::new(env, "batch_open")
}

/// Returns the Symbol for the `"batch_close"` event topic.
///
/// Emitted once per item during a `batch_close` call.
pub fn event_batch_close(env: &Env) -> Symbol {
    Symbol::new(env, "batch_close")
}

/// Returns the Symbol for the `"set_global_cap"` event topic.
///
/// Emitted when the admin updates the global per-account cap.
pub fn event_set_global_cap(env: &Env) -> Symbol {
    Symbol::new(env, "set_global_cap")
}

/// Returns the Symbol for the `"paused"` event topic.
///
/// Emitted when the contract is paused.
pub fn event_paused(env: &Env) -> Symbol {
    Symbol::new(env, "paused")
}

/// Returns the Symbol for the `"unpaused"` event topic.
///
/// Emitted when the contract is unpaused.
pub fn event_unpaused(env: &Env) -> Symbol {
    Symbol::new(env, "unpaused")
}

/// Returns the Symbol for the `"admin_nominated"` event topic.
///
/// Emitted when the admin nominates a new admin.
pub fn event_admin_nominated(env: &Env) -> Symbol {
    Symbol::new(env, "admin_nominated")
}

/// Returns the Symbol for the `"admin_accepted"` event topic.
///
/// Emitted when a nominated admin accepts the admin role.
pub fn event_admin_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "admin_accepted")
}

/// Returns the Symbol for the `"admin_cancelled"` event topic.
///
/// Emitted when the admin cancels a pending admin transfer.
pub fn event_admin_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "admin_cancelled")
}

/// Returns the Symbol for the `"upgraded"` event topic.
///
/// Emitted when the contract is upgraded to a new WASM hash.
pub fn event_upgraded(env: &Env) -> Symbol {
    Symbol::new(env, "upgraded")
}

/// Returns the Symbol for the `"admin_broadcast"` event topic.
///
/// Emitted when the admin broadcasts an emergency message.
pub fn event_admin_broadcast(env: &Env) -> Symbol {
    Symbol::new(env, "admin_broadcast")
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_event_init_bytes() {
        let env = Env::default();
        assert_eq!(event_init(&env), Symbol::new(&env, "init"));
    }

    #[test]
    fn test_event_open_bytes() {
        let env = Env::default();
        assert_eq!(event_open(&env), Symbol::new(&env, "open"));
    }

    #[test]
    fn test_event_close_bytes() {
        let env = Env::default();
        assert_eq!(event_close(&env), Symbol::new(&env, "close"));
    }

    #[test]
    fn test_event_batch_open_bytes() {
        let env = Env::default();
        assert_eq!(event_batch_open(&env), Symbol::new(&env, "batch_open"));
    }

    #[test]
    fn test_event_batch_close_bytes() {
        let env = Env::default();
        assert_eq!(event_batch_close(&env), Symbol::new(&env, "batch_close"));
    }

    #[test]
    fn test_event_set_global_cap_bytes() {
        let env = Env::default();
        assert_eq!(
            event_set_global_cap(&env),
            Symbol::new(&env, "set_global_cap")
        );
    }

    #[test]
    fn test_event_paused_bytes() {
        let env = Env::default();
        assert_eq!(event_paused(&env), Symbol::new(&env, "paused"));
    }

    #[test]
    fn test_event_unpaused_bytes() {
        let env = Env::default();
        assert_eq!(event_unpaused(&env), Symbol::new(&env, "unpaused"));
    }

    #[test]
    fn test_event_admin_nominated_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_nominated(&env),
            Symbol::new(&env, "admin_nominated")
        );
    }

    #[test]
    fn test_event_admin_accepted_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_accepted(&env),
            Symbol::new(&env, "admin_accepted")
        );
    }

    #[test]
    fn test_event_admin_cancelled_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_cancelled(&env),
            Symbol::new(&env, "admin_cancelled")
        );
    }

    #[test]
    fn test_event_upgraded_bytes() {
        let env = Env::default();
        assert_eq!(event_upgraded(&env), Symbol::new(&env, "upgraded"));
    }

    #[test]
    fn test_event_admin_broadcast_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_broadcast(&env),
            Symbol::new(&env, "admin_broadcast")
        );
    }
}
