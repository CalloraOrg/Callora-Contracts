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
//! | Topic                | Trigger                                                       |
//! |----------------------|---------------------------------------------------------------|
//! | `"admin_init"`       | Contract initialized with the first admin (`init`)           |
//! | `"admin_nominated"`  | Current admin proposes a successor (`set_admin`)             |
//! | `"admin_changed"`    | Pending admin accepts the role and becomes the new admin     |
//! | `"admin_cancelled"`  | Current admin revokes a pending nomination                   |
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
}
