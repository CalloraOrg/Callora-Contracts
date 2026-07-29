//! Integration tests for the Callora Admin contract.
//!
//! These tests drive the admin module directly (no `#[contract]` macro
//! is involved), matching the testing style used in `contracts/upgrade`.
//!
//! Coverage:
//! - get_admin / get_pending_admin view behaviour
//! - init happy path + double-init panic
//! - set_admin auth + topic + data shape + repeated-nomination replacement
//! - accept_admin auth + topic + data shape + non-pending caller panic
//! - cancel_admin_transfer auth + topic + data shape + no-pending panic
//! - full three-event lifecycle ordering
//! - failed calls emit zero events
//! - event-log completeness (every path emits exactly the documented count)
//! - TTL bookkeeping fires on every write

#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, Env, IntoVal, Symbol,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Return all events published by any contract in the environment, filtered
/// to those whose topic[0] matches the given string.
///
/// Mirrors the helper used in `contracts/upgrade/src/test.rs` so that the
/// assertion style stays identical across the workspace.
fn events_with_topic<'a>(
    env: &'a Env,
    topic: &str,
) -> std::vec::Vec<(soroban_sdk::Vec<soroban_sdk::Val>, soroban_sdk::Val)> {
    let needle = Symbol::new(env, topic);
    env.events()
        .all()
        .iter()
        .filter(|e| {
            !e.1.is_empty() && {
                let t0: Symbol = e.1.get(0).unwrap().into_val(env);
                t0 == needle
            }
        })
        .map(|e| (e.1.clone(), e.2.clone()))
        .collect()
}

/// Initialize the admin contract in a fresh env with a single admin.
fn fresh_env_with_admin() -> (Env, Address) {
    let env = Env::default();
    let admin = Address::generate(&env);
    admin::init(&env, &admin);
    (env, admin)
}

// ===========================================================================
// View behaviour
// ===========================================================================

/// `init` makes the admin address visible through `get_admin`.
#[test]
fn get_admin_returns_initial_admin_after_init() {
    let env = Env::default();
    assert!(admin::get_admin(&env).is_none());

    let admin = Address::generate(&env);
    admin::init(&env, &admin);

    assert_eq!(admin::get_admin(&env), Some(admin.clone()));
}

/// `get_pending_admin` is `None` before any nomination is made.
#[test]
fn get_pending_admin_is_none_initially() {
    let (env, _admin) = fresh_env_with_admin();
    assert!(admin::get_pending_admin(&env).is_none());
}

// ===========================================================================
// init
// ===========================================================================

/// `init` emits exactly one `admin_init` event with the initial admin in
/// topic[1] and `()` as data.
#[test]
fn init_emits_admin_init_event() {
    let env = Env::default();
    let admin = Address::generate(&env);

    admin::init(&env, &admin);

    let matches = events_with_topic(&env, "admin_init");
    assert_eq!(
        matches.len(),
        1,
        "init must emit exactly one admin_init event"
    );

    let (topics, data) = &matches[0];
    assert_eq!(topics.len(), 2, "admin_init must carry exactly 2 topics");

    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_init"));

    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    assert_eq!(
        topic1, admin,
        "admin_init topic[1] must be the initial admin"
    );

    // data is `()` (empty tuple).
    let _: () = data.clone().into_val(&env);
}

/// Calling `init` twice must panic with the documented error string and
/// **not** emit a second `admin_init` event.
#[test]
#[should_panic(expected = "admin contract already initialized")]
fn init_twice_panics() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let imposter = Address::generate(&env);

    admin::init(&env, &admin);
    admin::init(&env, &imposter);
}

// ===========================================================================
// set_admin
// ===========================================================================

/// `set_admin` requires `require_auth` on the caller.
#[test]
fn set_admin_requires_auth() {
    let env = Env::default();
    let invader = Address::generate(&env);
    let new_admin = Address::generate(&env);

    // No `env.mock_all_auths()` here — auth must be enforced.
    let res = std::panic::catch_unwind(|| {
        admin::set_admin(&env, &invader, &new_admin);
    });
    assert!(
        res.is_err(),
        "set_admin must require auth and panic when caller does not authorize"
    );
}

/// `set_admin` rejects a caller that is not the current admin with the
/// documented error string. Auth must still succeed first.
#[test]
fn set_admin_unauthorized_caller_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let intruder = Address::generate(&env);
    let new_admin = Address::generate(&env);

    admin::init(&env, &admin);

    let res = std::panic::catch_unwind(|| {
        admin::set_admin(&env, &intruder, &new_admin);
    });
    assert!(
        res.is_err(),
        "intruder must not be allowed to nominate a new admin"
    );
}

/// `set_admin` emits exactly one `admin_nominated` event with caller in
/// topic[1] and the nominated address as data.
#[test]
fn set_admin_emits_admin_nominated_event_with_correct_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    admin::init(&env, &admin);

    admin::set_admin(&env, &admin, &new_admin);

    let matches = events_with_topic(&env, "admin_nominated");
    assert_eq!(
        matches.len(),
        1,
        "set_admin must emit exactly one admin_nominated event"
    );

    let (topics, data) = &matches[0];
    assert_eq!(topics.len(), 2, "admin_nominated must carry 2 topics");

    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_nominated"));

    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    assert_eq!(
        topic1, admin,
        "admin_nominated topic[1] must be the current admin (caller)"
    );

    let payload: Address = data.clone().into_val(&env);
    assert_eq!(
        payload, new_admin,
        "admin_nominated data must be the pending admin address"
    );
}

/// After `set_admin`, `get_pending_admin` returns the nominated address and
/// `get_admin` is unchanged (the rotation has not completed yet).
#[test]
fn set_admin_updates_only_pending_slot() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    admin::init(&env, &admin);

    admin::set_admin(&env, &admin, &new_admin);

    assert_eq!(
        admin::get_pending_admin(&env),
        Some(new_admin.clone()),
        "pending_admin must equal the nominated address after set_admin"
    );
    assert_eq!(
        admin::get_admin(&env),
        Some(admin.clone()),
        "active admin must NOT change on set_admin"
    );
}

/// Calling `set_admin` twice without an intervening accept replaces the
/// pending slot and emits a fresh `admin_nominated` event for the new
/// nominee — the prior nomination is implicitly cancelled.
#[test]
fn set_admin_replaces_prior_pending_nomination() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let nominee_a = Address::generate(&env);
    let nominee_b = Address::generate(&env);
    admin::init(&env, &admin);

    admin::set_admin(&env, &admin, &nominee_a);
    admin::set_admin(&env, &admin, &nominee_b);

    assert_eq!(admin::get_pending_admin(&env), Some(nominee_b.clone()));

    let matches = events_with_topic(&env, "admin_nominated");
    assert_eq!(
        matches.len(),
        2,
        "two set_admin calls must emit two admin_nominated events"
    );

    // Most recent event carries nominee_b as data.
    let payload_b: Address = matches[1].1.clone().into_val(&env);
    assert_eq!(payload_b, nominee_b);
}

// ===========================================================================
// accept_admin
// ===========================================================================

/// `accept_admin` requires `require_auth` on the caller.
#[test]
fn accept_admin_requires_auth() {
    let env = Env::default();
    let nominee = Address::generate(&env);

    // Establish a pending admin through a mocked-auth path.
    env.mock_all_auths();
    let admin = Address::generate(&env);
    admin::init(&env, &admin);
    admin::set_admin(&env, &admin, &nominee);

    // Strip auth — accept must panic.
    env.set_auths(&[]);
    let res = std::panic::catch_unwind(|| {
        admin::accept_admin(&env, &nominee);
    });
    assert!(
        res.is_err(),
        "accept_admin must require auth and panic when caller does not authorize"
    );
}

/// `accept_admin` panics with `no pending admin transfer` when there is no
/// pending nomination.
#[test]
#[should_panic(expected = "no pending admin transfer")]
fn accept_admin_with_no_pending_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let stranger = Address::generate(&env);
    admin::init(&env, &admin);

    admin::accept_admin(&env, &stranger);
}

/// `accept_admin` rejects a caller that is not equal to the pending admin.
#[test]
fn accept_admin_wrong_caller_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let nominee = Address::generate(&env);
    let attacker = Address::generate(&env);
    admin::init(&env, &admin);
    admin::set_admin(&env, &admin, &nominee);

    let res = std::panic::catch_unwind(|| {
        admin::accept_admin(&env, &attacker);
    });
    assert!(
        res.is_err(),
        "a non-pending address must not be able to accept the role"
    );
}

/// `accept_admin` emits exactly one `admin_changed` event whose data
/// carries `(previous_admin, new_admin)`, promoting the pending admin to
/// the active role.
#[test]
fn accept_admin_emits_admin_changed_event_with_correct_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    admin::init(&env, &admin);
    admin::set_admin(&env, &admin, &new_admin);

    admin::accept_admin(&env, &new_admin);

    let matches = events_with_topic(&env, "admin_changed");
    assert_eq!(
        matches.len(),
        1,
        "accept_admin must emit exactly one admin_changed event"
    );

    let (topics, data) = &matches[0];
    assert_eq!(topics.len(), 2, "admin_changed must carry 2 topics");

    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_changed"));

    // topic[1] is the incoming admin (the caller who just accepted).
    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    assert_eq!(
        topic1, new_admin,
        "admin_changed topic[1] must be the incoming admin"
    );

    let (previous_admin, new_admin_data): (Address, Address) = data.clone().into_val(&env);
    assert_eq!(
        previous_admin, admin,
        "admin_changed data[0] must be the previous admin"
    );
    assert_eq!(
        new_admin_data, new_admin,
        "admin_changed data[1] must be the new admin"
    );

    // Active admin must have flipped; pending slot must be empty.
    assert_eq!(
        admin::get_admin(&env),
        Some(new_admin.clone()),
        "active admin must equal the pending admin after accept"
    );
    assert!(
        admin::get_pending_admin(&env).is_none(),
        "pending slot must be cleared after accept"
    );
}

// ===========================================================================
// cancel_admin_transfer
// ===========================================================================

/// `cancel_admin_transfer` requires `require_auth` on the caller.
#[test]
fn cancel_admin_transfer_requires_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pending = Address::generate(&env);
    admin::init(&env, &admin);
    admin::set_admin(&env, &admin, &pending);

    // Strip auth — cancel must panic.
    env.set_auths(&[]);
    let res = std::panic::catch_unwind(|| {
        admin::cancel_admin_transfer(&env, &admin);
    });
    assert!(res.is_err(), "cancel_admin_transfer must require auth");
}

/// `cancel_admin_transfer` panics with `no pending admin transfer` when no
/// nomination is in progress.
#[test]
#[should_panic(expected = "no pending admin transfer")]
fn cancel_admin_transfer_with_no_pending_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    admin::init(&env, &admin);

    admin::cancel_admin_transfer(&env, &admin);
}

/// `cancel_admin_transfer` rejects an unauthorized caller.
#[test]
fn cancel_admin_transfer_unauthorized_caller_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pending = Address::generate(&env);
    let stranger = Address::generate(&env);
    admin::init(&env, &admin);
    admin::set_admin(&env, &admin, &pending);

    let res = std::panic::catch_unwind(|| {
        admin::cancel_admin_transfer(&env, &stranger);
    });
    assert!(
        res.is_err(),
        "a non-admin caller must not be able to cancel a transfer"
    );
}

/// `cancel_admin_transfer` emits exactly one `admin_cancelled` event whose
/// data carries the address of the pending admin that was dropped.
#[test]
fn cancel_admin_transfer_emits_admin_cancelled_event_with_correct_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pending = Address::generate(&env);
    admin::init(&env, &admin);
    admin::set_admin(&env, &admin, &pending);

    admin::cancel_admin_transfer(&env, &admin);

    let matches = events_with_topic(&env, "admin_cancelled");
    assert_eq!(
        matches.len(),
        1,
        "cancel_admin_transfer must emit exactly one admin_cancelled event"
    );

    let (topics, data) = &matches[0];
    assert_eq!(topics.len(), 2, "admin_cancelled must carry 2 topics");

    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_cancelled"));

    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    assert_eq!(
        topic1, admin,
        "admin_cancelled topic[1] must be the current admin"
    );

    let payload: Address = data.clone().into_val(&env);
    assert_eq!(
        payload, pending,
        "admin_cancelled data must be the pending admin that was cancelled"
    );

    // Pending slot is empty after cancellation; active admin is unchanged.
    assert!(
        admin::get_pending_admin(&env).is_none(),
        "pending slot must be empty after cancellation"
    );
    assert_eq!(
        admin::get_admin(&env),
        Some(admin.clone()),
        "active admin must NOT change on cancel"
    );
}

// ===========================================================================
// Full rotation lifecycle — happy path
// ===========================================================================

/// A full admin rotation:
/// `init(admin)` → `set_admin(admin, new)` → `accept_admin(new)`.
///
/// Produces exactly three events in this order:
/// 1. `admin_init` (admin, ...)
/// 2. `admin_nominated` (admin, new)
/// 3. `admin_changed` (new, (admin, new))
#[test]
fn full_rotation_lifecycle_emits_three_events_in_order() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);

    admin::init(&env, &admin);
    admin::set_admin(&env, &admin, &new_admin);
    admin::accept_admin(&env, &new_admin);

    // One event of each kind.
    assert_eq!(events_with_topic(&env, "admin_init").len(), 1);
    assert_eq!(events_with_topic(&env, "admin_nominated").len(), 1);
    assert_eq!(events_with_topic(&env, "admin_changed").len(), 1);
    assert_eq!(events_with_topic(&env, "admin_cancelled").len(), 0);

    // Position the events by matching topics in the order they appear.
    let init_sym = Symbol::new(&env, "admin_init");
    let nom_sym = Symbol::new(&env, "admin_nominated");
    let chg_sym = Symbol::new(&env, "admin_changed");

    let mut init_pos: Option<usize> = None;
    let mut nom_pos: Option<usize> = None;
    let mut chg_pos: Option<usize> = None;

    for (i, event) in env.events().all().iter().enumerate() {
        if event.1.is_empty() {
            continue;
        }
        let t0: Symbol = event.1.get(0).unwrap().into_val(&env);
        if t0 == init_sym {
            init_pos = Some(i);
        } else if t0 == nom_sym {
            nom_pos = Some(i);
        } else if t0 == chg_sym {
            chg_pos = Some(i);
        }
    }

    assert!(init_pos.is_some(), "admin_init event missing");
    assert!(nom_pos.is_some(), "admin_nominated event missing");
    assert!(chg_pos.is_some(), "admin_changed event missing");

    assert!(
        init_pos.unwrap() < nom_pos.unwrap(),
        "admin_init must precede admin_nominated"
    );
    assert!(
        nom_pos.unwrap() < chg_pos.unwrap(),
        "admin_nominated must precede admin_changed"
    );

    // Final state matches the new admin with no pending rotation.
    assert_eq!(admin::get_admin(&env), Some(new_admin));
    assert!(admin::get_pending_admin(&env).is_none());
}

/// A full lifecycle with an explicit cancellation between two successful
/// rotations emits:
/// `admin_init`, `admin_nominated`, `admin_cancelled`, `admin_nominated`,
/// `admin_changed` — in that positional order.
#[test]
fn cancel_between_rotations_keeps_event_order_consistent() {
    let env = Env::default();
    env.mock_all_auths();
    let admin_a = Address::generate(&env);
    let dropped = Address::generate(&env);
    let admin_b = Address::generate(&env);

    admin::init(&env, &admin_a);
    admin::set_admin(&env, &admin_a, &dropped);
    admin::cancel_admin_transfer(&env, &admin_a);
    admin::set_admin(&env, &admin_a, &admin_b);
    admin::accept_admin(&env, &admin_b);

    assert_eq!(events_with_topic(&env, "admin_init").len(), 1);
    assert_eq!(events_with_topic(&env, "admin_nominated").len(), 2);
    assert_eq!(events_with_topic(&env, "admin_cancelled").len(), 1);
    assert_eq!(events_with_topic(&env, "admin_changed").len(), 1);

    assert_eq!(admin::get_admin(&env), Some(admin_b));
    assert!(admin::get_pending_admin(&env).is_none());
}

/// Multiple rotations back-to-back show that each successful handover
/// emits exactly one `admin_changed` event with the correct before/after
/// pair.
#[test]
fn repeated_rotations_each_emit_one_admin_changed_with_correct_pair() {
    let env = Env::default();
    env.mock_all_auths();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);

    admin::init(&env, &a);
    admin::set_admin(&env, &a, &b);
    admin::accept_admin(&env, &b);
    admin::set_admin(&env, &b, &c);
    admin::accept_admin(&env, &c);

    let changed = events_with_topic(&env, "admin_changed");
    assert_eq!(
        changed.len(),
        2,
        "two successful handovers must emit exactly two admin_changed events"
    );

    let (prev1, new1): (Address, Address) = changed[0].1.clone().into_val(&env);
    assert_eq!(prev1, a);
    assert_eq!(new1, b);

    let (prev2, new2): (Address, Address) = changed[1].1.clone().into_val(&env);
    assert_eq!(prev2, b);
    assert_eq!(new2, c);

    assert_eq!(admin::get_admin(&env), Some(c));
}

// ===========================================================================
// Negative lifecycle — failed calls must NOT emit events
// ===========================================================================

/// An unauthorized `set_admin` call (caught panic) must not produce any
/// `admin_nominated` event in the event stream.
#[test]
fn unauthorized_set_admin_emits_no_events() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let intruder = Address::generate(&env);
    let nominee = Address::generate(&env);
    admin::init(&env, &admin);

    let res = std::panic::catch_unwind(|| {
        admin::set_admin(&env, &intruder, &nominee);
    });
    assert!(res.is_err());

    assert_eq!(
        events_with_topic(&env, "admin_nominated").len(),
        0,
        "unauthorized set_admin must not emit admin_nominated"
    );
    assert!(
        admin::get_pending_admin(&env).is_none(),
        "no pending admin should be created on failed set_admin"
    );
}

/// An unauthorized `accept_admin` call (caught panic) must not produce any
/// `admin_changed` event, and pending slot must remain populated.
#[test]
fn unauthorized_accept_admin_emits_no_events_and_leaves_state_unchanged() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let nominee = Address::generate(&env);
    let impostor = Address::generate(&env);
    admin::init(&env, &admin);
    admin::set_admin(&env, &admin, &nominee);

    let res = std::panic::catch_unwind(|| {
        admin::accept_admin(&env, &impostor);
    });
    assert!(res.is_err());

    assert_eq!(
        events_with_topic(&env, "admin_changed").len(),
        0,
        "unauthorized accept_admin must not emit admin_changed"
    );
    assert_eq!(
        admin::get_pending_admin(&env),
        Some(nominee),
        "failed accept must leave pending slot intact"
    );
    assert_eq!(
        admin::get_admin(&env),
        Some(admin),
        "failed accept must not change active admin"
    );
}

/// An unauthorized `cancel_admin_transfer` call (caught panic) must not
/// produce any `admin_cancelled` event, and pending slot must remain.
#[test]
fn unauthorized_cancel_admin_transfer_emits_no_events() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pending = Address::generate(&env);
    let impostor = Address::generate(&env);
    admin::init(&env, &admin);
    admin::set_admin(&env, &admin, &pending);

    let res = std::panic::catch_unwind(|| {
        admin::cancel_admin_transfer(&env, &impostor);
    });
    assert!(res.is_err());

    assert_eq!(
        events_with_topic(&env, "admin_cancelled").len(),
        0,
        "unauthorized cancel must not emit admin_cancelled"
    );
    assert_eq!(
        admin::get_pending_admin(&env),
        Some(pending),
        "failed cancel must leave pending slot intact"
    );
}

// ===========================================================================
// Event-log completeness — every entrypoint emits *exactly* its documented
// events, no extras, no missing.
// ===========================================================================

/// A bare `init` must add **exactly one** event to the event log — no
/// accidental topics from storage writes or TTL bumps.
#[test]
fn init_event_log_contains_exactly_one_event() {
    let env = Env::default();
    let admin = Address::generate(&env);

    admin::init(&env, &admin);

    let all = env.events().all();
    assert_eq!(
        all.len(),
        1,
        "init must add exactly one event (admin_init) to the log — extras indicate a leaky invariant"
    );
}

/// A bare `set_admin` (after init) must add **exactly one** event to the
/// event log — the single `admin_nominated`. Catches the silent regression
/// where `set_admin` would emit `admin_changed` *or* `admin_transfer_started`
/// alongside the documented topic.
#[test]
fn set_admin_event_log_contains_exactly_one_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    admin::init(&env, &admin);

    admin::set_admin(&env, &admin, &new_admin);

    let all = env.events().all();
    // 1 init + 1 nominated = 2 total.
    assert_eq!(
        all.len(),
        2,
        "set_admin must add exactly one event to the log (init + nominated)"
    );
}

/// After a full `init → set_admin → accept_admin` rotation, the event log
/// must contain **exactly three** entries. Anything more would mean the
/// contract is leaking ancillary events (e.g. on the underlying instance
/// bump).
#[test]
fn full_rotation_event_log_contains_exactly_three_events() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);

    admin::init(&env, &admin);
    admin::set_admin(&env, &admin, &new_admin);
    admin::accept_admin(&env, &new_admin);

    let all = env.events().all();
    assert_eq!(
        all.len(),
        3,
        "full rotation must emit exactly three events (init + nominated + changed)"
    );
}

// ===========================================================================
// Per-account limits — auth
// ===========================================================================

/// `set_account_limits` with `mock_all_auths` rejects a non-admin caller.
///
/// Host-level `require_auth()` panics without auth setup, matching the Soroban
/// convention used across the admin contract. This test uses `mock_all_auths()`
/// so the non-admin error surfaces as `Err(Unauthorized)`.
#[test]
fn limits_set_account_requires_admin_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);

    let res = limits::set_account_limits(&env, &alice, &alice, 5, 5, 5);
    assert_eq!(res, Err(errors::AdminLimitError::Unauthorized));
}

/// `set_account_limits` returns `NotInitialized` when no admin is set.
#[test]
fn limits_set_account_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let caller = Address::generate(&env);
    let target = Address::generate(&env);

    let res = limits::set_account_limits(&env, &caller, &target, 5, 5, 5);
    assert_eq!(res, Err(errors::AdminLimitError::NotInitialized));
}

/// `set_default_limits` returns `NotInitialized` when no admin is set.
#[test]
fn limits_set_default_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let caller = Address::generate(&env);

    let res = limits::set_default_limits(&env, &caller, 5, 5, 5);
    assert_eq!(res, Err(errors::AdminLimitError::NotInitialized));
}

/// `clear_account_limits` returns `NotInitialized` when no admin is set.
#[test]
fn limits_clear_account_requires_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let caller = Address::generate(&env);
    let target = Address::generate(&env);

    let res = limits::clear_account_limits(&env, &caller, &target);
    assert_eq!(res, Err(errors::AdminLimitError::NotInitialized));
}

/// `set_default_limits` rejects a non-admin caller even when authorized.
#[test]
fn limits_set_default_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let intruder = Address::generate(&env);
    admin::init(&env, &admin);

    let res = limits::set_default_limits(&env, &intruder, 5, 5, 5);
    assert_eq!(res, Err(errors::AdminLimitError::Unauthorized));
}

/// `clear_account_limits` rejects a non-admin caller.
#[test]
fn limits_clear_account_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let intruder = Address::generate(&env);
    let target = Address::generate(&env);
    admin::init(&env, &admin);

    let res = limits::clear_account_limits(&env, &intruder, &target);
    assert_eq!(res, Err(errors::AdminLimitError::Unauthorized));
}

// ===========================================================================
// Per-account limits — views (uninitialized)
// ===========================================================================

/// Views work without admin initialization (they have no auth requirement).
#[test]
fn limits_views_work_without_admin_init() {
    let env = Env::default();
    let alice = Address::generate(&env);

    // get_default_limits returns DEFAULT_LIMITS
    assert_eq!(limits::get_default_limits(&env), limits::DEFAULT_LIMITS);

    // get_account_limits returns DEFAULT_LIMITS
    let caps = limits::get_account_limits(&env, &alice);
    assert_eq!(caps, limits::DEFAULT_LIMITS);

    // get_account_usage returns zero
    let usage = limits::get_account_usage(&env, &alice);
    assert_eq!(usage.bets, 0);
    assert_eq!(usage.positions, 0);
    assert_eq!(usage.subscriptions, 0);

    // can_* checks return true (default caps are non-zero)
    assert!(limits::can_place_bet(&env, &alice));
    assert!(limits::can_open_position(&env, &alice));
    assert!(limits::can_subscribe(&env, &alice));
}

// ===========================================================================
// Per-account limits — set / get caps
// ===========================================================================

/// `set_account_limits` persists caps and they are readable via
/// `get_account_limits`.
#[test]
fn limits_set_account_persists_and_is_readable() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);

    let res = limits::set_account_limits(&env, &admin, &alice, 2, 3, 4);
    assert_eq!(res, Ok(()));

    let caps = limits::get_account_limits(&env, &alice);
    assert_eq!(caps.max_bets, 2);
    assert_eq!(caps.max_positions, 3);
    assert_eq!(caps.max_subscriptions, 4);
}

/// `set_account_limits` emits an `account_limits_set` event.
#[test]
fn limits_set_account_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);

    limits::set_account_limits(&env, &admin, &alice, 5, 6, 7).unwrap();

    let matches = events_with_topic(&env, "account_limits_set");
    assert_eq!(matches.len(), 1);
}

/// `set_default_limits` persists and `get_default_limits` reads it back.
#[test]
fn limits_set_default_persists() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    admin::init(&env, &admin);

    limits::set_default_limits(&env, &admin, 10, 20, 30).unwrap();

    let caps = limits::get_default_limits(&env);
    assert_eq!(caps.max_bets, 10);
    assert_eq!(caps.max_positions, 20);
    assert_eq!(caps.max_subscriptions, 30);
}

/// `set_default_limits` emits a `default_limits_set` event.
#[test]
fn limits_set_default_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    admin::init(&env, &admin);

    limits::set_default_limits(&env, &admin, 10, 20, 30).unwrap();

    let matches = events_with_topic(&env, "default_limits_set");
    assert_eq!(matches.len(), 1);
}

/// `set_account_limits` rejects caps exceeding `MAX_CAP`.
#[test]
fn limits_set_account_rejects_invalid_caps() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);

    let bad_bets = limits::set_account_limits(&env, &admin, &alice, limits::MAX_CAP + 1, 0, 0);
    assert_eq!(bad_bets, Err(errors::AdminLimitError::InvalidLimit));

    let bad_positions = limits::set_account_limits(&env, &admin, &alice, 0, limits::MAX_CAP + 1, 0);
    assert_eq!(bad_positions, Err(errors::AdminLimitError::InvalidLimit));

    let bad_subs = limits::set_account_limits(&env, &admin, &alice, 0, 0, limits::MAX_CAP + 1);
    assert_eq!(bad_subs, Err(errors::AdminLimitError::InvalidLimit));
}

/// `clear_account_limits` removes per-account override so `get_account_limits`
/// falls back to the default.
#[test]
fn limits_clear_account_falls_back_to_default() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);

    // Set a per-account override then set a new global default.
    limits::set_account_limits(&env, &admin, &alice, 1, 1, 1).unwrap();
    limits::set_default_limits(&env, &admin, 11, 12, 13).unwrap();

    // Per-account still shows the override.
    assert_eq!(limits::get_account_limits(&env, &alice).max_bets, 1);

    // Clear and verify fallback to global.
    limits::clear_account_limits(&env, &admin, &alice).unwrap();
    let caps = limits::get_account_limits(&env, &alice);
    assert_eq!(caps.max_bets, 11);
    assert_eq!(caps.max_positions, 12);
    assert_eq!(caps.max_subscriptions, 13);
}

/// `clear_account_limits` emits an `account_limits_cleared` event.
#[test]
fn limits_clear_account_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);

    limits::set_account_limits(&env, &admin, &alice, 1, 1, 1).unwrap();
    limits::clear_account_limits(&env, &admin, &alice).unwrap();

    let cleared = events_with_topic(&env, "account_limits_cleared");
    assert_eq!(cleared.len(), 1);
}

// ===========================================================================
// Per-account limits — consume operations
// ===========================================================================

/// `consume_bet` panics via `require_auth()` when the caller has not
/// authorized. Uses `catch_unwind` matching the existing admin test pattern.
#[test]
fn limits_consume_bet_requires_account_auth() {
    let env = Env::default();
    let alice = Address::generate(&env);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        limits::consume_bet(&env, &alice);
    }));
    assert!(
        res.is_err(),
        "consume_bet must require auth and panic when caller does not authorize"
    );
}

/// `consume_bet` succeeds when under cap.
#[test]
fn limits_consume_bet_succeeds_under_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 3, 3, 3).unwrap();

    // First two succeed.
    assert_eq!(limits::consume_bet(&env, &alice), Ok(()));
    assert_eq!(limits::consume_bet(&env, &alice), Ok(()));

    let usage = limits::get_account_usage(&env, &alice);
    assert_eq!(usage.bets, 2);
}

/// `consume_bet` emits `bet_consumed` event.
#[test]
fn limits_consume_bet_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 3, 3, 3).unwrap();

    limits::consume_bet(&env, &alice).unwrap();

    let matches = events_with_topic(&env, "bet_consumed");
    assert_eq!(matches.len(), 1);

    let (topics, _data) = &matches[0];
    assert_eq!(topics.len(), 2);
    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    assert_eq!(topic1, alice);
}

/// `consume_bet` fails when at cap.
#[test]
fn limits_consume_bet_fails_at_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 2, 3, 3).unwrap();

    limits::consume_bet(&env, &alice).unwrap();
    limits::consume_bet(&env, &alice).unwrap();
    assert_eq!(
        limits::consume_bet(&env, &alice),
        Err(errors::AdminLimitError::BetsAtCap)
    );

    // No state change on failure.
    let usage = limits::get_account_usage(&env, &alice);
    assert_eq!(usage.bets, 2);
}

/// `consume_bet` fails when the account is fully disabled (cap 0).
#[test]
fn limits_consume_bet_fails_when_fully_disabled() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 0, 0, 0).unwrap();

    assert_eq!(
        limits::consume_bet(&env, &alice),
        Err(errors::AdminLimitError::BetsAtCap)
    );
}

/// `consume_position` succeeds under cap.
#[test]
fn limits_consume_position_succeeds_under_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 3, 3, 3).unwrap();

    limits::consume_position(&env, &alice).unwrap();
    let usage = limits::get_account_usage(&env, &alice);
    assert_eq!(usage.positions, 1);
}

/// `consume_position` fails at cap.
#[test]
fn limits_consume_position_fails_at_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 3, 1, 3).unwrap();

    limits::consume_position(&env, &alice).unwrap();
    assert_eq!(
        limits::consume_position(&env, &alice),
        Err(errors::AdminLimitError::PositionsAtCap)
    );
}

/// `consume_subscription` succeeds under cap and emits event.
#[test]
fn limits_consume_subscription_succeeds_and_emits() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 3, 3, 2).unwrap();

    limits::consume_subscription(&env, &alice).unwrap();
    let usage = limits::get_account_usage(&env, &alice);
    assert_eq!(usage.subscriptions, 1);

    let matches = events_with_topic(&env, "subscription_consumed");
    assert_eq!(matches.len(), 1);
}

/// `consume_subscription` fails at cap.
#[test]
fn limits_consume_subscription_fails_at_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 3, 3, 2).unwrap();

    limits::consume_subscription(&env, &alice).unwrap();
    limits::consume_subscription(&env, &alice).unwrap();
    assert_eq!(
        limits::consume_subscription(&env, &alice),
        Err(errors::AdminLimitError::SubscriptionsAtCap)
    );
}

// ===========================================================================
// Per-account limits — release operations
// ===========================================================================

/// `release_bet` panics via `require_auth()` when the caller has not
/// authorized. Uses `catch_unwind` matching the existing admin test pattern.
#[test]
fn limits_release_bet_requires_account_auth() {
    let env = Env::default();
    let alice = Address::generate(&env);

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        limits::release_bet(&env, &alice);
    }));
    assert!(
        res.is_err(),
        "release_bet must require auth and panic when caller does not authorize"
    );
}

/// `release_bet` decrements the counter and emits event.
#[test]
fn limits_release_bet_decrements_and_emits() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 5, 5, 5).unwrap();

    limits::consume_bet(&env, &alice).unwrap();
    limits::consume_bet(&env, &alice).unwrap();
    limits::release_bet(&env, &alice).unwrap();

    let usage = limits::get_account_usage(&env, &alice);
    assert_eq!(usage.bets, 1);

    let matches = events_with_topic(&env, "bet_released");
    assert_eq!(matches.len(), 1);
}

/// `release_bet` fails when counter is zero.
#[test]
fn limits_release_bet_fails_on_underflow() {
    let env = Env::default();
    env.mock_all_auths();
    let alice = Address::generate(&env);

    assert_eq!(
        limits::release_bet(&env, &alice),
        Err(errors::AdminLimitError::CounterUnderflow)
    );
}

/// `release_position` decrements the counter.
#[test]
fn limits_release_position_decrements() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 5, 5, 5).unwrap();

    limits::consume_position(&env, &alice).unwrap();
    limits::consume_position(&env, &alice).unwrap();
    limits::release_position(&env, &alice).unwrap();

    let usage = limits::get_account_usage(&env, &alice);
    assert_eq!(usage.positions, 1);
}

/// `release_position` fails on underflow.
#[test]
fn limits_release_position_fails_on_underflow() {
    let env = Env::default();
    env.mock_all_auths();
    let alice = Address::generate(&env);

    assert_eq!(
        limits::release_position(&env, &alice),
        Err(errors::AdminLimitError::CounterUnderflow)
    );
}

/// `release_subscription` decrements and emits.
#[test]
fn limits_release_subscription_decrements_and_emits() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 5, 5, 5).unwrap();

    limits::consume_subscription(&env, &alice).unwrap();
    limits::release_subscription(&env, &alice).unwrap();

    let usage = limits::get_account_usage(&env, &alice);
    assert_eq!(usage.subscriptions, 0);

    let matches = events_with_topic(&env, "subscription_released");
    assert_eq!(matches.len(), 1);
}

/// `release_subscription` fails on underflow.
#[test]
fn limits_release_subscription_fails_on_underflow() {
    let env = Env::default();
    env.mock_all_auths();
    let alice = Address::generate(&env);

    assert_eq!(
        limits::release_subscription(&env, &alice),
        Err(errors::AdminLimitError::CounterUnderflow)
    );
}

// ===========================================================================
// Per-account limits — can_* dry-run checks
// ===========================================================================

/// `can_place_bet` returns `true` when under cap.
#[test]
fn limits_can_place_bet_returns_true_under_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 2, 2, 2).unwrap();

    assert!(limits::can_place_bet(&env, &alice));
    limits::consume_bet(&env, &alice).unwrap();
    assert!(limits::can_place_bet(&env, &alice));
    limits::consume_bet(&env, &alice).unwrap();
    assert!(!limits::can_place_bet(&env, &alice));
}

/// `can_open_position` returns `false` when at cap.
#[test]
fn limits_can_open_position_returns_false_at_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 2, 1, 2).unwrap();

    limits::consume_position(&env, &alice).unwrap();
    assert!(!limits::can_open_position(&env, &alice));
}

/// `can_subscribe` returns `false` when at cap.
#[test]
fn limits_can_subscribe_returns_false_at_cap() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 2, 2, 1).unwrap();

    limits::consume_subscription(&env, &alice).unwrap();
    assert!(!limits::can_subscribe(&env, &alice));
}

/// `can_*` checks don't require auth or admin init.
#[test]
fn limits_can_checks_no_auth_required() {
    let env = Env::default();
    let alice = Address::generate(&env);

    assert!(limits::can_place_bet(&env, &alice));
    assert!(limits::can_open_position(&env, &alice));
    assert!(limits::can_subscribe(&env, &alice));
}

// ===========================================================================
// Per-account limits — isolated accounts
// ===========================================================================

/// Multiple accounts have independent counters.
#[test]
fn limits_accounts_are_independent() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 2, 2, 2).unwrap();
    limits::set_account_limits(&env, &admin, &bob, 4, 4, 4).unwrap();

    limits::consume_bet(&env, &alice).unwrap();
    limits::consume_bet(&env, &bob).unwrap();
    limits::consume_bet(&env, &bob).unwrap();

    let alice_usage = limits::get_account_usage(&env, &alice);
    let bob_usage = limits::get_account_usage(&env, &bob);
    assert_eq!(alice_usage.bets, 1);
    assert_eq!(bob_usage.bets, 2);

    // Bob still has room, alice only has one more slot.
    assert!(limits::can_place_bet(&env, &alice));
    assert!(limits::can_place_bet(&env, &bob));
    limits::consume_bet(&env, &bob).unwrap();
    assert!(limits::can_place_bet(&env, &bob));
}

/// Setting limits after usage has accumulated correctly enforces new caps.
#[test]
fn limits_set_after_usage_enforces_new_caps() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);

    // Let alice accumulate under generous caps.
    limits::set_account_limits(&env, &admin, &alice, 10, 10, 10).unwrap();
    limits::consume_bet(&env, &alice).unwrap();
    limits::consume_bet(&env, &alice).unwrap();
    limits::consume_bet(&env, &alice).unwrap();

    // Tighten the cap below current usage.
    limits::set_account_limits(&env, &admin, &alice, 1, 10, 10).unwrap();

    // alice already has 3 bets > new cap of 1 — next consume must fail.
    assert_eq!(
        limits::consume_bet(&env, &alice),
        Err(errors::AdminLimitError::BetsAtCap)
    );
}

// ===========================================================================
// Per-account limits — default caps fallback
// ===========================================================================

/// When no per-account caps are set, accounts use the global default.
#[test]
fn limits_falls_back_to_default_when_no_override() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);

    // Set a non-trivial global default.
    limits::set_default_limits(&env, &admin, 3, 3, 3).unwrap();

    // Consume up to the default cap.
    limits::consume_bet(&env, &alice).unwrap();
    limits::consume_bet(&env, &alice).unwrap();
    limits::consume_bet(&env, &alice).unwrap();

    assert_eq!(
        limits::consume_bet(&env, &alice),
        Err(errors::AdminLimitError::BetsAtCap)
    );
}

/// Changing the global default doesn't affect accounts with explicit overrides.
#[test]
fn limits_default_change_does_not_affect_overrides() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);

    limits::set_account_limits(&env, &admin, &alice, 5, 5, 5).unwrap();
    limits::set_default_limits(&env, &admin, 1, 1, 1).unwrap();

    // alice should still have her override of 5.
    let caps = limits::get_account_limits(&env, &alice);
    assert_eq!(caps.max_bets, 5);
}

// ===========================================================================
// Per-account limits — consume → release → consume cycle
// ===========================================================================

/// A full consume/release cycle returns to zero and allows re-consumption.
#[test]
fn limits_consume_release_cycle() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let alice = Address::generate(&env);
    admin::init(&env, &admin);
    limits::set_account_limits(&env, &admin, &alice, 3, 3, 3).unwrap();

    limits::consume_bet(&env, &alice).unwrap();
    limits::consume_bet(&env, &alice).unwrap();
    limits::consume_bet(&env, &alice).unwrap();
    assert_eq!(
        limits::consume_bet(&env, &alice),
        Err(errors::AdminLimitError::BetsAtCap)
    );

    limits::release_bet(&env, &alice).unwrap();
    limits::release_bet(&env, &alice).unwrap();

    // Now we can consume again.
    assert!(limits::can_place_bet(&env, &alice));
    limits::consume_bet(&env, &alice).unwrap();
    limits::consume_bet(&env, &alice).unwrap();
    assert_eq!(
        limits::consume_bet(&env, &alice),
        Err(errors::AdminLimitError::BetsAtCap)
    );
}

/// `AccountLimits::uniform` constructor simplifies test setup.
#[test]
fn limits_account_limits_uniform_constructor() {
    let caps = limits::AccountLimits::uniform(7);
    assert_eq!(caps.max_bets, 7);
    assert_eq!(caps.max_positions, 7);
    assert_eq!(caps.max_subscriptions, 7);
}
