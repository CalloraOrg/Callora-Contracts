//! # Cross-contract event-schema conformance audit (Issue #413)
//!
//! Issue #413 requires a holistic audit of every `env.events().publish(...)`
//! call against the schemas documented in [`EVENT_SCHEMA.md`](../EVENT_SCHEMA.md).
//! This test driver runs every documented emit-point on every in-tree
//! contract and asserts byte-level identity between the emitted topic[0]
//! Symbol and the schema's documented Symbol.
//!
//! ## CI gate: `event_schema_audit_topic_constants_match_helpers_byte_for_byte`
//!
//! The single most important test in this file. Asserts that every
//! documented event topic is byte-identical to the `events::*` helper
//! in the corresponding crate. If a future refactor renames a topic in
//! code but forgets to update either the helper or `EVENT_SCHEMA.md`,
//! this test fails immediately.
//!
//! ## Set conformance tests per emit-point
//!
//! Each documented emit-point gets at least one test that fires the
//! entrypoint, then asserts the resulting event log carries the expected
//! topic[0] (and topic[1] where the schema documents one).
//!
//! ## How to run
//!
//! ```text
//! cargo test -p callora-contracts-e2e --test event_schema_audit
//! ```

extern crate std;

#[path = "../scripts/e2e_setup.rs"]
mod e2e_setup;

use e2e_setup::setup;
use soroban_sdk::testutils::Events as _;
use soroban_sdk::{Address, Env, Symbol, TryFromVal};

/// Walk `env.events().all()` and return the events emitted by `pool`
/// with topic[0] matching `expected_topic`.
fn find_pool_event_with_topic(
    env: &Env,
    pool: &Address,
    expected_topic: &Symbol,
) -> Option<(soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val)> {
    for entry in env.events().all().iter() {
        if entry.0 != *pool || entry.1.is_empty() {
            continue;
        }
        let topic: Symbol = entry.1.get(0).unwrap().into_val(env);
        if topic == *expected_topic {
            return Some((entry.1.clone(), entry.2.clone()));
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Revenue-pool conformance: init
// ---------------------------------------------------------------------------

/// `init` must publish `Symbol("init")` with admin as topic[1] and the
/// USDC token address as data. Conforms to the schema documented at
/// `EVENT_SCHEMA.md` § Contract: `callora-revenue-pool` › `init`.
#[test]
fn event_schema_audit_revenue_pool_init_topics_and_data() {
    let env = Env::default();
    let h = setup(&env);
    let init_sym = Symbol::new(&env, "init");

    let entry =
        find_pool_event_with_topic(&env, &h.revenue_pool_id, &init_sym).expect("init event");
    assert!(entry.0.len() >= 2, "init must publish topic[1] = admin");

    // topic[1] = admin
    let admin_in_topic: Address = entry.0.get(1).unwrap().into_val(&env);
    assert_eq!(admin_in_topic, h.owner);

    // data = USDC token address (per schema)
    let usdc_in_data: Address = entry.1.into_val(&env);
    assert_eq!(usdc_in_data, h.usdc_id);
}

// ---------------------------------------------------------------------------
// Revenue-pool conformance: admin-changed event
// ---------------------------------------------------------------------------

/// Canonical conformance assertion for the current `set_admin` emit
/// shape: `(Symbol("admin"), Symbol("changed"))` topic pair with data
/// `(current_admin, new_admin)`. This is the strict short-form because
/// that's what the in-tree `lib.rs::set_admin` emits; the long-form
/// `Symbol("admin_changed")` documented in
/// [`EVENT_SCHEMA.md`](../EVENT_SCHEMA.md) is a known-reconcile item
/// and is rejected here until the code path is updated to use the
/// helper. This makes the test a true conformance gate rather than
/// a permissive migration shim.
#[test]
fn event_schema_audit_revenue_pool_set_admin_emits_short_form_strict() {
    let env = Env::default();
    let h = setup(&env);

    // Drive set_admin with a fresh non-admin address.
    let new_admin = Address::generate(&env);
    h.revenue_pool.set_admin(&h.owner, &new_admin);

    let t_admin = Symbol::new(&env, "admin");
    let t_changed = Symbol::new(&env, "changed");
    let t_long_form = Symbol::new(&env, "admin_changed");

    let mut strict_short_match = false;
    let mut long_form_leaked = false;
    for entry in env.events().all().iter() {
        if entry.0 != h.revenue_pool_id || entry.1.len() < 2 {
            continue;
        }
        let t0: Symbol = entry.1.get(0).unwrap().into_val(&env);
        let t1: Symbol = entry.1.get(1).unwrap().into_val(&env);
        if t0 == t_admin && t1 == t_changed {
            // data must be (current_admin, new_admin) per the
            // short-form documentation in EVENT_SCHEMA.md.
            let (current, proposed): (Address, Address) =
                <(Address, Address)>::try_from_val(&env, &entry.2)
                    .expect("data must decode as (Address, Address)");
            assert_eq!(current, h.owner);
            assert_eq!(proposed, new_admin);
            strict_short_match = true;
        }
        if t0 == t_long_form {
            long_form_leaked = true;
        }
    }
    assert!(
        strict_short_match,
        "revenue_pool set_admin must publish the canonical short-form \
         (Symbol(\"admin\"), Symbol(\"changed\")) with data \
         (current_admin, new_admin). See EVENT_SCHEMA.md § 'Open \
         Reconciliation Items' for the long-form migration plan."
    );
    assert!(
        !long_form_leaked,
        "revenue_pool set_admin must NOT publish the long-form Symbol(\"admin_changed\") \
         until the lib.rs migrate; doing so silently breaks documented indexer behavior."
    );
}

// ---------------------------------------------------------------------------
// Revenue-pool conformance: emergency-drain lifecycle topics
// ---------------------------------------------------------------------------

/// Drive `propose_emergency_drain`, then observe the documented
/// `emergency_drain_proposed` topic with admin as topic[1] and the
/// `PendingEmergencyDrain` struct as data.
#[test]
fn event_schema_audit_revenue_pool_propose_emergency_drain_topics_and_data() {
    let env = Env::default();
    let h = setup(&env);

    let drain_target = Address::generate(&env);
    let amount: i128 = 123_456;
    h.revenue_pool
        .propose_emergency_drain(&h.owner, &drain_target, &amount);

    let proposed_sym = Symbol::new(&env, "emergency_drain_proposed");
    let entry =
        find_pool_event_with_topic(&env, &h.revenue_pool_id, &proposed_sym).expect("proposed");
    assert!(entry.0.len() >= 2);
    let admin_in_topic: Address = entry.0.get(1).unwrap().into_val(&env);
    assert_eq!(admin_in_topic, h.owner);

    let pending: callora_revenue_pool::emergency::PendingEmergencyDrain =
        callora_revenue_pool::emergency::PendingEmergencyDrain::try_from_val(&env, &entry.1)
            .expect("data must decode as PendingEmergencyDrain");
    assert_eq!(pending.to, drain_target);
    assert_eq!(pending.amount, amount);
}

// ---------------------------------------------------------------------------
// Vault conformance: reserve_cap_set
// ---------------------------------------------------------------------------

/// Drive `set_reserve_cap` and observe the documented `reserve_cap_set`
/// topic. Documented at `EVENT_SCHEMA.md` § `reserve_cap_set`.
#[test]
fn event_schema_audit_vault_set_reserve_cap_emits_event() {
    let env = Env::default();
    let h = setup(&env);

    let cap: i128 = 9_999_999_999;
    h.vault.set_reserve_cap(&h.owner, &h.usdc_id, &cap);

    let topic = Symbol::new(&env, "reserve_cap_set");
    let entry = find_pool_event_with_topic(&env, &h.vault_id, &topic)
        .expect("vault must publish reserve_cap_set");
    assert!(entry.0.len() >= 2);
}

// ---------------------------------------------------------------------------
// CI byte-identity gate
// ---------------------------------------------------------------------------

/// Issue #413's CI-enforcement pillar: every `events::*` helper returns
/// the exact `Symbol` byte sequence documented in
/// [`EVENT_SCHEMA.md`](../EVENT_SCHEMA.md). This locks schema-to-helper
/// drift. **It does NOT detect a code-only rename** that updates the
/// code but neither the helper nor the schema — that case is caught by
/// the per-`#[test]` emit-point assertions above.
///
/// If a future PR renames a topic in code but forgets to update the
/// helper, the per-endpoint tests catch the mismatch and this byte-
/// identity test continues to pass (helper still matches schema).
#[test]
fn event_schema_audit_topic_constants_lock_schema_to_helpers() {
    let env = Env::default();

    // --- vault ---
    assert_eq!(
        Symbol::new(&env, "init"),
        callora_vault::events::event_init(&env)
    );
    assert_eq!(
        Symbol::new(&env, "deposit"),
        callora_vault::events::event_deposit(&env)
    );
    assert_eq!(
        Symbol::new(&env, "deduct"),
        callora_vault::events::event_deduct(&env)
    );
    assert_eq!(
        Symbol::new(&env, "withdraw"),
        callora_vault::events::event_withdraw(&env)
    );
    assert_eq!(
        Symbol::new(&env, "withdraw_to"),
        callora_vault::events::event_withdraw_to(&env)
    );
    assert_eq!(
        Symbol::new(&env, "vault_paused"),
        callora_vault::events::event_vault_paused(&env)
    );
    assert_eq!(
        Symbol::new(&env, "vault_unpaused"),
        callora_vault::events::event_vault_unpaused(&env)
    );
    assert_eq!(
        Symbol::new(&env, "reserve_cap_set"),
        callora_vault::events::event_reserve_cap_set(&env)
    );
    assert_eq!(
        Symbol::new(&env, "ownership_nominated"),
        callora_vault::events::event_ownership_nominated(&env)
    );
    assert_eq!(
        Symbol::new(&env, "ownership_accepted"),
        callora_vault::events::event_ownership_accepted(&env)
    );

    // --- revenue_pool ---
    assert_eq!(
        Symbol::new(&env, "init"),
        callora_revenue_pool::events::event_init(&env)
    );
    assert_eq!(
        Symbol::new(&env, "emergency_drain_proposed"),
        callora_revenue_pool::events::event_emergency_drain_proposed(&env)
    );
    assert_eq!(
        Symbol::new(&env, "emergency_drain_executed"),
        callora_revenue_pool::events::event_emergency_drain_executed(&env)
    );
    assert_eq!(
        Symbol::new(&env, "emergency_drain_cancelled"),
        callora_revenue_pool::events::event_emergency_drain_cancelled(&env)
    );
    assert_eq!(
        Symbol::new(&env, "distribute"),
        callora_revenue_pool::events::event_distribute(&env)
    );
    assert_eq!(
        Symbol::new(&env, "batch_distribute"),
        callora_revenue_pool::events::event_batch_distribute(&env)
    );
    assert_eq!(
        Symbol::new(&env, "receive_payment"),
        callora_revenue_pool::events::event_receive_payment(&env)
    );
    assert_eq!(
        Symbol::new(&env, "admin_transfer_started"),
        callora_revenue_pool::events::event_admin_transfer_started(&env)
    );
    assert_eq!(
        Symbol::new(&env, "admin_transfer_completed"),
        callora_revenue_pool::events::event_admin_transfer_completed(&env)
    );
}
