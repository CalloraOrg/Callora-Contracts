//! Event topic Symbol constructors for the Recipient Registry contract.
//!
//! This module centralizes all event topic strings into dedicated functions,
//! ensuring byte-identity and preventing accidental topic name drift.

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"init"` event topic.
///
/// Emitted when the recipient registry is first initialized with an admin.
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

/// Returns the Symbol for the `"recipient_registered"` event topic.
///
/// Emitted when a new named recipient address is registered by the admin.
pub fn event_recipient_registered(env: &Env) -> Symbol {
    Symbol::new(env, "recipient_registered")
}

/// Returns the Symbol for the `"recipient_updated"` event topic.
///
/// Emitted when an existing recipient's address is updated by the admin.
pub fn event_recipient_updated(env: &Env) -> Symbol {
    Symbol::new(env, "recipient_updated")
}

/// Returns the Symbol for the `"recipient_removed"` event topic.
///
/// Emitted when an existing recipient is removed from the registry.
pub fn event_recipient_removed(env: &Env) -> Symbol {
    Symbol::new(env, "recipient_removed")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Snapshot: proves `event_init` maps to exactly the bytes for `"init"`.
    #[test]
    fn test_event_init_bytes() {
        let env = Env::default();
        assert_eq!(event_init(&env), Symbol::new(&env, "init"));
    }

    /// Snapshot: proves `event_recipient_registered` maps to the expected bytes.
    #[test]
    fn test_event_recipient_registered_bytes() {
        let env = Env::default();
        assert_eq!(
            event_recipient_registered(&env),
            Symbol::new(&env, "recipient_registered")
        );
    }

    /// Snapshot: proves `event_recipient_updated` maps to the expected bytes.
    #[test]
    fn test_event_recipient_updated_bytes() {
        let env = Env::default();
        assert_eq!(
            event_recipient_updated(&env),
            Symbol::new(&env, "recipient_updated")
        );
    }

    /// Snapshot: proves `event_recipient_removed` maps to the expected bytes.
    #[test]
    fn test_event_recipient_removed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_recipient_removed(&env),
            Symbol::new(&env, "recipient_removed")
        );
    }
}
