//! Event topic Symbol constructors for the Callora Registry contract.
//!
//! This module centralizes all event topic strings into dedicated functions,
//! ensuring byte-identity is preserved and preventing accidental topic name drift
//! across call sites.

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"init"` event topic.
///
/// Emitted when the registry contract is first initialized with an admin
/// address and an initial catalog configuration.
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

/// Returns the Symbol for the `"offering_registered"` event topic.
///
/// Emitted when a new offering is registered in the catalog via
/// [`crate::CalloraRegistry::register_offering`]. The offering ID is
/// included as a topic so indexers can track additions without polling.
pub fn event_offering_registered(env: &Env) -> Symbol {
    Symbol::new(env, "offering_registered")
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

    /// Snapshot: proves event_offering_registered still maps to exactly the bytes for "offering_registered".
    #[test]
    fn test_event_offering_registered_bytes() {
        let env = Env::default();
        assert_eq!(
            event_offering_registered(&env),
            Symbol::new(&env, "offering_registered")
        );
    }
}
