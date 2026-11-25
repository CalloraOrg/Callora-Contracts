//! Event topic `Symbol` constructors for the Callora Allowlist contract.
//!
//! Every mutation publishes `(action, version, caller[, affected])` so
//! off-chain indexers can reconstruct who changed the allowlist and which
//! address was affected. Counts and previous admin nominations travel in the
//! event data payload.
//!
//! Snapshot tests at the bottom of this file pin every topic to its literal
//! spelling so an accidental rename fails loudly in `cargo test`.

#![allow(dead_code)]

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the canonical Callora event version marker.
pub fn event_version_v1(env: &Env) -> Symbol {
    Symbol::new(env, "callora_v1")
}

/// Returns the Symbol for the `"init"` event topic.
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

/// Returns the Symbol for the `"address_added"` event topic.
pub fn event_address_added(env: &Env) -> Symbol {
    Symbol::new(env, "address_added")
}

/// Returns the Symbol for the `"address_removed"` event topic.
pub fn event_address_removed(env: &Env) -> Symbol {
    Symbol::new(env, "address_removed")
}

/// Returns the Symbol for the `"allowlist_cleared"` event topic.
pub fn event_allowlist_cleared(env: &Env) -> Symbol {
    Symbol::new(env, "allowlist_cleared")
}

/// Returns the Symbol for the `"admin_nominated"` event topic.
pub fn event_admin_nominated(env: &Env) -> Symbol {
    Symbol::new(env, "admin_nominated")
}

/// Returns the Symbol for the `"admin_accepted"` event topic.
pub fn event_admin_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "admin_accepted")
}

/// Returns the Symbol for the `"admin_cancelled"` event topic.
pub fn event_admin_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "admin_cancelled")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_version_v1_bytes() {
        let env = Env::default();
        assert_eq!(event_version_v1(&env), Symbol::new(&env, "callora_v1"));
    }

    #[test]
    fn test_event_init_bytes() {
        let env = Env::default();
        assert_eq!(event_init(&env), Symbol::new(&env, "init"));
    }

    #[test]
    fn test_event_address_added_bytes() {
        let env = Env::default();
        assert_eq!(
            event_address_added(&env),
            Symbol::new(&env, "address_added")
        );
    }

    #[test]
    fn test_event_address_removed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_address_removed(&env),
            Symbol::new(&env, "address_removed")
        );
    }

    #[test]
    fn test_event_allowlist_cleared_bytes() {
        let env = Env::default();
        assert_eq!(
            event_allowlist_cleared(&env),
            Symbol::new(&env, "allowlist_cleared")
        );
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
}
