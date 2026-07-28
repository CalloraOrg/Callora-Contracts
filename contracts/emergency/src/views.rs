//! Read-only capability views for Callora emergency operations.
//!
//! Each bit in the `u64` returned by [`capabilities`] represents a distinct,
//! stable emergency feature. Bits are assigned once and never reassigned,
//! so clients can detect capability deltas across upgrades with simple
//! bitwise comparison:
//!
//! ```ignore
//! let before = old_client.capabilities();
//! let after  = new_client.capabilities();
//! let added   = after & !before;
//! let removed = before & !after;
//! ```
//!
//! # Bit registry
//! Reserved bits (6–63) are always zero in this version.

use soroban_sdk::Env;

// ---------------------------------------------------------------------------
// Capability bits
// ---------------------------------------------------------------------------

/// Bit 0 — Emergency pause: admin can invoke `pause()` to halt all
/// state-changing entrypoints in an emergency.
/// Introduced: v1.0.0
pub const CAP_EMERGENCY_PAUSE: u64 = 1 << 0;

/// Bit 1 — Emergency unpause: admin can invoke `unpause()` to restore
/// normal operations after threat mitigation.
/// Introduced: v1.0.0
pub const CAP_EMERGENCY_UNPAUSE: u64 = 1 << 1;

/// Bit 2 — Emergency drain proposal: admin can open a time-locked drain
/// proposal via `propose_emergency_drain()`.
/// Introduced: v1.0.0
pub const CAP_EMERGENCY_DRAIN_PROPOSE: u64 = 1 << 2;

/// Bit 3 — Emergency drain execution: after the 24-hour timelock, the
/// proposal is executable via `execute_emergency_drain()`.
/// Introduced: v1.0.0
pub const CAP_EMERGENCY_DRAIN_EXECUTE: u64 = 1 << 3;

/// Bit 4 — Emergency drain cancellation: admin can cancel a pending
/// proposal via `cancel_emergency_drain()`.
/// Introduced: v1.0.0
pub const CAP_EMERGENCY_DRAIN_CANCEL: u64 = 1 << 4;

/// Bit 5 — Pending drain view: clients can inspect an in-flight proposal
/// via `get_pending_emergency_drain()` without auth.
/// Introduced: v1.0.0
pub const CAP_PENDING_DRAIN_VIEW: u64 = 1 << 5;

// Bits 6–63 are reserved for future emergency capabilities and are always zero.

/// Bitmask of all emergency capabilities exposed by this contract version.
///
/// Combine individual `CAP_*` constants with `&` to test a specific feature:
/// ```ignore
/// assert!(caps & CAP_EMERGENCY_PAUSE != 0);
/// ```
pub const ALL_CAPABILITIES: u64 = CAP_EMERGENCY_PAUSE
    | CAP_EMERGENCY_UNPAUSE
    | CAP_EMERGENCY_DRAIN_PROPOSE
    | CAP_EMERGENCY_DRAIN_EXECUTE
    | CAP_EMERGENCY_DRAIN_CANCEL
    | CAP_PENDING_DRAIN_VIEW;

/// Return the emergency capability bitmap for this deployment.
///
/// Each set bit signals a supported emergency feature. Bits are stable across
/// upgrades — once assigned a bit position is never reused for a different
/// feature. Reserved bits (6–63) are always zero.
///
/// Pure view: ignores `_env` (no storage reads). No authentication required.
pub fn capabilities(_env: &Env) -> u64 {
    ALL_CAPABILITIES
}

// ---------------------------------------------------------------------------
// Unit tests (run with `cargo test -p callora-emergency`)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn capabilities_equals_all_mask() {
        let env = Env::default();
        assert_eq!(capabilities(&env), ALL_CAPABILITIES);
    }

    #[test]
    fn capabilities_is_nonzero() {
        let env = Env::default();
        assert_ne!(capabilities(&env), 0);
    }

    #[test]
    fn reserved_bits_are_clear() {
        let env = Env::default();
        let caps = capabilities(&env);
        // Bits 6–63 must all be zero.
        assert_eq!(caps >> 6, 0, "reserved high bits must remain clear");
    }

    #[test]
    fn each_documented_bit_is_set() {
        let env = Env::default();
        let caps = capabilities(&env);
        for (name, bit) in [
            ("CAP_EMERGENCY_PAUSE", CAP_EMERGENCY_PAUSE),
            ("CAP_EMERGENCY_UNPAUSE", CAP_EMERGENCY_UNPAUSE),
            ("CAP_EMERGENCY_DRAIN_PROPOSE", CAP_EMERGENCY_DRAIN_PROPOSE),
            ("CAP_EMERGENCY_DRAIN_EXECUTE", CAP_EMERGENCY_DRAIN_EXECUTE),
            ("CAP_EMERGENCY_DRAIN_CANCEL", CAP_EMERGENCY_DRAIN_CANCEL),
            ("CAP_PENDING_DRAIN_VIEW", CAP_PENDING_DRAIN_VIEW),
        ] {
            assert_ne!(caps & bit, 0, "missing capability bit {name} ({bit:#x})");
        }
    }

    #[test]
    fn capability_delta_detects_added_and_removed_bits() {
        // Simulate an older deployment that lacked pending-drain view,
        // and a future one that drops unpause — clients XOR/mask to detect.
        let old = ALL_CAPABILITIES & !CAP_PENDING_DRAIN_VIEW;
        let new = ALL_CAPABILITIES & !CAP_EMERGENCY_UNPAUSE;

        let added = new & !old;
        let removed = old & !new;

        assert_eq!(added, CAP_PENDING_DRAIN_VIEW);
        assert_eq!(removed, CAP_EMERGENCY_UNPAUSE);
    }

    #[test]
    fn capabilities_is_stable_across_calls() {
        let env = Env::default();
        assert_eq!(capabilities(&env), capabilities(&env));
    }

    #[test]
    fn all_capabilities_is_union_of_individual_bits() {
        let expected = CAP_EMERGENCY_PAUSE
            | CAP_EMERGENCY_UNPAUSE
            | CAP_EMERGENCY_DRAIN_PROPOSE
            | CAP_EMERGENCY_DRAIN_EXECUTE
            | CAP_EMERGENCY_DRAIN_CANCEL
            | CAP_PENDING_DRAIN_VIEW;
        assert_eq!(ALL_CAPABILITIES, expected);
    }
}
