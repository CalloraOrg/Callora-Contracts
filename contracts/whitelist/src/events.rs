//! Event topic Symbol constructors for the Callora Whitelist contract.

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"address_added_until"` event topic.
///
/// Emitted when the admin grants a temporary whitelist entry via
/// [`crate::CalloraWhitelist::add_address_until`]. The topic list carries the
/// caller and the address; the data payload is the `expires_at` timestamp.
pub fn event_address_added_until(env: &Env) -> Symbol {
    Symbol::new(env, "address_added_until")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Snapshot: proves the topic still maps to exactly `"address_added_until"`.
    #[test]
    fn test_event_address_added_until_bytes() {
        let env = Env::default();
        assert_eq!(
            event_address_added_until(&env),
            Symbol::new(&env, "address_added_until")
        );
    }
}
