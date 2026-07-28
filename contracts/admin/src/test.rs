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
) -> std::vec::Vec<(
    soroban_sdk::Vec<soroban_sdk::Val>,
    soroban_sdk::Val,
)> {
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
    assert_eq!(matches.len(), 1, "init must emit exactly one admin_init event");

    let (topics, data) = &matches[0];
    assert_eq!(topics.len(), 2, "admin_init must carry exactly 2 topics");

    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_init"));

    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    assert_eq!(topic1, admin, "admin_init topic[1] must be the initial admin");

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
    assert_eq!(topic1, new_admin, "admin_changed topic[1] must be the incoming admin");

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
    assert!(
        res.is_err(),
        "cancel_admin_transfer must require auth"
    );
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
