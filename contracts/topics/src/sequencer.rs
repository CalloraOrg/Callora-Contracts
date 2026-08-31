//! Event sequencing and ordering helpers for Callora lifecycle events.
//!
//! This module provides utilities to guarantee deterministic ordering of lifecycle
//! events across all Callora contracts. Event sequence numbers act as immutable,
//! monotonically increasing identifiers that allow:
//!
//! 1. **Deterministic ordering** — even if events are delivered out-of-order,
//!    indexers can sort by sequence number to reconstruct the true order.
//! 2. **Idempotency** — retried calls that emit events with the same sequence
//!    number are recognized as duplicates and handled correctly.
//! 3. **Compatibility** — versioned payloads allow gradual schema evolution
//!    without breaking existing consumers.

use soroban_sdk::{Env, Symbol};

// ═══════════════════════════════════════════════════════════════════════════
// Event Sequencing
// ═══════════════════════════════════════════════════════════════════════════

/// Storage key for the monotonically increasing event sequence counter.
///
/// Each contract maintains its own sequence counter in instance storage.
/// The counter is thread-safe within a single Soroban environment context.
const EVENT_SEQUENCE_KEY: &str = "event_seq";

/// Retrieve the current event sequence counter value.
///
/// Returns 0 if the counter has not yet been initialized.
///
/// # Arguments
/// * `env` — Soroban environment
///
/// # Returns
/// The current sequence number, or 0 if uninitialized.
pub fn current_event_sequence(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get::<Symbol, u64>(&Symbol::new(env, EVENT_SEQUENCE_KEY))
        .unwrap_or(0)
}

/// Increment and return the next event sequence number.
///
/// This function atomically reads the current sequence counter from instance storage,
/// increments it, writes it back, and returns the new value. The returned number
/// uniquely identifies the next lifecycle event to be emitted.
///
/// # Arguments
/// * `env` — Soroban environment
///
/// # Returns
/// The next sequence number (starts at 1 after first call)
///
/// # Panics
/// Panics if the sequence counter overflows (i.e., reaches u64::MAX).
/// This is exceedingly unlikely in practice but fails closed.
///
/// # Example
/// ```rust
/// let env = Env::default();
/// let seq1 = next_event_sequence(&env);  // Returns 1
/// let seq2 = next_event_sequence(&env);  // Returns 2
/// assert!(seq1 < seq2);  // Guaranteed ordering
/// ```
pub fn next_event_sequence(env: &Env) -> u64 {
    let inst = env.storage().instance();
    let current: u64 = inst
        .get::<Symbol, u64>(&Symbol::new(env, EVENT_SEQUENCE_KEY))
        .unwrap_or(0);

    let next = current
        .checked_add(1)
        .expect("event sequence number overflow (reached u64::MAX)");

    inst.set(&Symbol::new(env, EVENT_SEQUENCE_KEY), &next);
    next
}

// ═══════════════════════════════════════════════════════════════════════════
// Schema Versioning
// ═══════════════════════════════════════════════════════════════════════════

/// Current schema version for Callora lifecycle events.
///
/// This version is used as the baseline for all new event definitions.
/// When a breaking change occurs (new required fields, type changes),
/// the version is incremented to signal to consumers that they may need
/// to upgrade their handlers.
///
/// Versioning strategy:
/// - v1: Initial release — basic lifecycle events with sequence numbers
/// - v2+: Reserved for future breaking changes (new required fields, renames)
///
/// Consumers should:
/// - Accept events with version >= their minimum supported version
/// - Implement version-aware payload parsing
/// - Fail safely on unknown versions rather than panicking
pub const EVENT_VERSION_V1: u32 = 1;

// ═══════════════════════════════════════════════════════════════════════════
// Version Marker Events
// ═══════════════════════════════════════════════════════════════════════════

/// Returns the Symbol for the event version marker (for tracking schema compatibility).
///
/// Used by consumers to determine which event schemas they should expect
/// and to detect when a contract has been upgraded with a new event version.
///
/// The version marker itself is NOT an event topic; rather, it's a field
/// that may appear in event payloads or metadata.
pub fn event_version_marker(env: &Env) -> Symbol {
    Symbol::new(env, "callora.lifecycle.v1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn sequence_starts_at_one() {
        let env = Env::default();
        let seq = next_event_sequence(&env);
        assert_eq!(seq, 1, "first sequence number must be 1");
    }

    #[test]
    fn sequence_increments_monotonically() {
        let env = Env::default();
        let seq1 = next_event_sequence(&env);
        let seq2 = next_event_sequence(&env);
        let seq3 = next_event_sequence(&env);

        assert_eq!(seq1, 1);
        assert_eq!(seq2, 2);
        assert_eq!(seq3, 3);

        assert!(seq1 < seq2 && seq2 < seq3);
    }

    #[test]
    fn current_sequence_returns_last_allocated() {
        let env = Env::default();
        assert_eq!(current_event_sequence(&env), 0, "uninitialized sequence is 0");

        let _ = next_event_sequence(&env);
        assert_eq!(current_event_sequence(&env), 1);

        let _ = next_event_sequence(&env);
        assert_eq!(current_event_sequence(&env), 2);
    }

    #[test]
    fn version_constant_is_v1() {
        assert_eq!(
            EVENT_VERSION_V1, 1,
            "initial event version must be v1 (1)"
        );
    }
}
