//! Focused integration tests for the emergency `capabilities()` view (#895).
//!
//! These tests exercise the on-chain entrypoint via a generated Soroban client
//! to verify that the bitmap is consistent, stable, and correctly exposes
//! every documented emergency feature.

extern crate std;

use callora_emergency::{
    CalloraEmergency, CalloraEmergencyClient, ALL_CAPABILITIES, CAP_EMERGENCY_DRAIN_CANCEL,
    CAP_EMERGENCY_DRAIN_EXECUTE, CAP_EMERGENCY_DRAIN_PROPOSE, CAP_EMERGENCY_PAUSE,
    CAP_EMERGENCY_UNPAUSE, CAP_PENDING_DRAIN_VIEW,
};
use soroban_sdk::Env;

fn client(env: &Env) -> CalloraEmergencyClient<'_> {
    let addr = env.register(CalloraEmergency, ());
    CalloraEmergencyClient::new(env, &addr)
}

// ---------------------------------------------------------------------------
// Basic contract invocation tests
// ---------------------------------------------------------------------------

#[test]
fn capabilities_returns_nonzero() {
    let env = Env::default();
    assert_ne!(client(&env).capabilities(), 0);
}

#[test]
fn capabilities_equals_all_capabilities_constant() {
    let env = Env::default();
    assert_eq!(client(&env).capabilities(), ALL_CAPABILITIES);
}

#[test]
fn capabilities_is_stable_across_calls() {
    let env = Env::default();
    let c = client(&env);
    assert_eq!(c.capabilities(), c.capabilities());
}

// ---------------------------------------------------------------------------
// Individual bit coverage
// ---------------------------------------------------------------------------

#[test]
fn each_documented_emergency_bit_is_set() {
    let env = Env::default();
    let caps = client(&env).capabilities();
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

// ---------------------------------------------------------------------------
// Reserved-bit safety
// ---------------------------------------------------------------------------

#[test]
fn reserved_high_bits_remain_zero() {
    let env = Env::default();
    let caps = client(&env).capabilities();
    assert_eq!(caps >> 6, 0, "reserved bits 6–63 must remain clear");
}

// ---------------------------------------------------------------------------
// Capability-delta detection (contract-level)
// ---------------------------------------------------------------------------

#[test]
fn capability_delta_detects_added_and_removed_bits_via_contract() {
    let env = Env::default();
    let caps = client(&env).capabilities();

    // Simulate an older deployment that lacked pending-drain view.
    let old = caps & !CAP_PENDING_DRAIN_VIEW;
    let added = caps & !old;
    assert_eq!(added, CAP_PENDING_DRAIN_VIEW);

    // Simulate a future deployment that dropped emergency-pause.
    let future = caps & !CAP_EMERGENCY_PAUSE;
    let removed = caps & !future;
    assert_eq!(removed, CAP_EMERGENCY_PAUSE);
}

// ---------------------------------------------------------------------------
// Bit-position uniqueness
// ---------------------------------------------------------------------------

#[test]
fn all_defined_bits_are_distinct_powers_of_two() {
    let bits = [
        CAP_EMERGENCY_PAUSE,
        CAP_EMERGENCY_UNPAUSE,
        CAP_EMERGENCY_DRAIN_PROPOSE,
        CAP_EMERGENCY_DRAIN_EXECUTE,
        CAP_EMERGENCY_DRAIN_CANCEL,
        CAP_PENDING_DRAIN_VIEW,
    ];
    for &b in &bits {
        // Must be a power of two (exactly one bit set).
        assert!(
            b != 0 && (b & (b - 1)) == 0,
            "bit {b:#x} is not a power of two"
        );
    }
    // All bits must be unique — their OR must equal their sum.
    let or_sum: u64 = bits.iter().copied().fold(0, |a, b| a | b);
    let arith_sum: u64 = bits.iter().copied().sum();
    assert_eq!(or_sum, arith_sum, "duplicate bit positions detected");
}
