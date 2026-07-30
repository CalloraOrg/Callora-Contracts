//! Event topic Symbol constructors for the Callora Topics contract.
//!
//! All event topic strings are defined here to keep the XDR byte layout
//! stable and prevent accidental renames from silently breaking off-chain
//! indexers.
//!
//! ## Topic shape
//!
//! All events follow the canonical single-topic shape:
//!
//! ```text
//! topics: (action: Symbol, ...)
//! data:   <event-specific payload>
//! ```

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"topics_init"` event topic.
///
/// Emitted once during [`crate::CalloraTopics::init`].
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "topics_init")
}

/// Returns the Symbol for the `"topic_registered"` event topic.
///
/// Emitted after a new topic is successfully persisted by
/// [`crate::CalloraTopics::register_topic`].
pub fn event_topic_registered(env: &Env) -> Symbol {
    Symbol::new(env, "topic_registered")
}

/// Returns the Symbol for the `"topic_deactivated"` event topic.
///
/// Emitted after an existing topic is deactivated by
/// [`crate::CalloraTopics::deactivate`].
pub fn event_topic_deactivated(env: &Env) -> Symbol {
    Symbol::new(env, "topic_deactivated")
}
