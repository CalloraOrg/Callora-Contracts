//! Focused tests for the cold `capabilities()` view (#714).

extern crate std;

use callora_cold::{
    CalloraCold, CalloraColdClient, ALL_CAPABILITIES, CAP_AUTO_REBALANCE, CAP_COLD_BALANCE_VIEW,
    CAP_COLD_MULTISIG_SWEEP, CAP_HOT_COLD_SPLIT, CAP_PENDING_COLD_SWEEP_VIEW, CAP_SET_COLD_SIGNERS,
    CAP_SET_HOT_COLD_RATIO,
};
use soroban_sdk::Env;

fn client(env: &Env) -> CalloraColdClient<'_> {
    let addr = env.register(CalloraCold, ());
    CalloraColdClient::new(env, &addr)
}

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
fn each_documented_cold_bit_is_set() {
    let env = Env::default();
    let caps = client(&env).capabilities();
    for bit in [
        CAP_HOT_COLD_SPLIT,
        CAP_AUTO_REBALANCE,
        CAP_COLD_MULTISIG_SWEEP,
        CAP_SET_HOT_COLD_RATIO,
        CAP_SET_COLD_SIGNERS,
        CAP_COLD_BALANCE_VIEW,
        CAP_PENDING_COLD_SWEEP_VIEW,
    ] {
        assert_ne!(caps & bit, 0, "missing capability bit {bit:#x}");
    }
}

#[test]
fn capability_delta_detects_added_and_removed_bits() {
    // Simulate an older deployment that lacked pending-sweep view, and a
    // future one that drops auto-rebalance — clients XOR/mask to find deltas.
    let old = ALL_CAPABILITIES & !CAP_PENDING_COLD_SWEEP_VIEW;
    let new = ALL_CAPABILITIES & !CAP_AUTO_REBALANCE;

    let added = new & !old;
    let removed = old & !new;

    assert_eq!(added, CAP_PENDING_COLD_SWEEP_VIEW);
    assert_eq!(removed, CAP_AUTO_REBALANCE);
}

#[test]
fn reserved_high_bits_remain_zero() {
    let env = Env::default();
    let caps = client(&env).capabilities();
    assert_eq!(caps >> 7, 0);
}

#[test]
fn capabilities_is_stable_across_calls() {
    let env = Env::default();
    let c = client(&env);
    assert_eq!(c.capabilities(), c.capabilities());
}
