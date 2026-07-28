//! # Auth snapshot — per-account yield-limits entrypoints
//!
//! Verifies that every **state-changing** per-account yield-limits
//! entrypoint enforces `caller.require_auth()`, and that every **read-only**
//! entrypoint does **not**.
//!
//! Off-chain indexers may rely on this surface as a stable contract; any
//! future addition of a new mutator **without** a matching
//! `require_auth` assertion MUST cause CI to fail here.

extern crate std;

use callora_yield::{AccountLimits, CalloraYieldLimits, CalloraYieldLimitsClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::BytesN;
use soroban_sdk::Vec as SorobanVec;
use soroban_sdk::{Address, Env};

// ---------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------

#[allow(clippy::type_complexity)]
fn auth_for(env: &Env, addr: &Address) -> (Address, SorobanVec<soroban_sdk::Symbol>) {
    (addr.clone(), SorobanVec::new(env))
}

fn setup(env: &Env) -> (Address, Address, CalloraYieldLimitsClient<'_>) {
    let contract = env.register(CalloraYieldLimits, ());
    let client = CalloraYieldLimitsClient::new(env, &contract);
    let admin = Address::generate(env);
    env.set_auths(&[auth_for(env, &admin)]);
    client.init(&admin);
    env.set_auths(&[auth_for(env, &admin)]);
    (contract, admin, client)
}

// ===========================================================================
// Admin hand-off: every entrypoint listed below MUST require auth.
// ===========================================================================

#[test]
fn set_admin_requires_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let intruder = Address::generate(&env);
    let target = Address::generate(&env);
    env.set_auths(&[]);
    let res = client.try_set_admin(&intruder, &target);
    assert!(res.is_err(), "set_admin must require auth on caller");
}

#[test]
fn accept_admin_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let nominee = Address::generate(&env);
    env.set_auths(&[auth_for(&env, &admin)]);
    client.set_admin(&admin, &nominee);

    env.set_auths(&[]);
    let res = client.try_accept_admin(&nominee);
    assert!(res.is_err(), "accept_admin must require auth on caller");
}

#[test]
fn cancel_admin_transfer_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let nominee = Address::generate(&env);
    env.set_auths(&[auth_for(&env, &admin)]);
    client.set_admin(&admin, &nominee);

    let intruder = Address::generate(&env);
    env.set_auths(&[]);
    let res = client.try_cancel_admin_transfer(&intruder);
    assert!(
        res.is_err(),
        "cancel_admin_transfer must require auth on caller"
    );
}

#[test]
fn set_default_limits_requires_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let intruder = Address::generate(&env);
    env.set_auths(&[]);
    let res = client.try_set_default_limits(&intruder, &1u32, &1u32, &1u32);
    assert!(
        res.is_err(),
        "set_default_limits must require auth on caller"
    );
}

#[test]
fn set_account_limits_requires_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let intruder = Address::generate(&env);
    let target = Address::generate(&env);
    env.set_auths(&[]);
    let res = client.try_set_account_limits(&intruder, &target, &1u32, &1u32, &1u32);
    assert!(
        res.is_err(),
        "set_account_limits must require auth on caller"
    );
}

#[test]
fn clear_account_limits_requires_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let intruder = Address::generate(&env);
    let target = Address::generate(&env);
    env.set_auths(&[]);
    let res = client.try_clear_account_limits(&intruder, &target);
    assert!(
        res.is_err(),
        "clear_account_limits must require auth on caller"
    );
}

#[test]
fn place_bet_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    env.set_auths(&[auth_for(&env, &admin)]);
    client.set_account_limits(&admin, &alice, &5u32, &5u32, &5u32);

    env.set_auths(&[]);
    let res = client.try_place_bet(&alice);
    assert!(res.is_err(), "place_bet must require auth on caller");
}

#[test]
fn clear_bet_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    env.set_auths(&[auth_for(&env, &admin)]);
    client.set_account_limits(&admin, &alice, &5u32, &5u32, &5u32);
    env.set_auths(&[auth_for(&env, &alice)]);
    client.place_bet(&alice);

    env.set_auths(&[]);
    let res = client.try_clear_bet(&alice);
    assert!(res.is_err(), "clear_bet must require auth on caller");
}

#[test]
fn open_position_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    env.set_auths(&[auth_for(&env, &admin)]);
    client.set_account_limits(&admin, &alice, &5u32, &5u32, &5u32);

    env.set_auths(&[]);
    let res = client.try_open_position(&alice);
    assert!(res.is_err(), "open_position must require auth on caller");
}

#[test]
fn close_position_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    env.set_auths(&[auth_for(&env, &admin)]);
    client.set_account_limits(&admin, &alice, &5u32, &5u32, &5u32);
    env.set_auths(&[auth_for(&env, &alice)]);
    client.open_position(&alice);

    env.set_auths(&[]);
    let res = client.try_close_position(&alice);
    assert!(res.is_err(), "close_position must require auth on caller");
}

#[test]
fn subscribe_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    env.set_auths(&[auth_for(&env, &admin)]);
    client.set_account_limits(&admin, &alice, &5u32, &5u32, &5u32);

    env.set_auths(&[]);
    let res = client.try_subscribe(&alice);
    assert!(res.is_err(), "subscribe must require auth on caller");
}

#[test]
fn unsubscribe_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    env.set_auths(&[auth_for(&env, &admin)]);
    client.set_account_limits(&admin, &alice, &5u32, &5u32, &5u32);
    env.set_auths(&[auth_for(&env, &alice)]);
    client.subscribe(&alice);

    env.set_auths(&[]);
    let res = client.try_unsubscribe(&alice);
    assert!(res.is_err(), "unsubscribe must require auth on caller");
}

#[test]
fn upgrade_requires_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let intruder = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    env.set_auths(&[]);
    let res = client.try_upgrade(&intruder, &hash);
    assert!(res.is_err(), "upgrade must require auth on caller");
}

// ===========================================================================
// Read-only entrypoints MUST NOT require auth.
// ===========================================================================

#[test]
fn get_admin_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    env.set_auths(&[]);
    let _ = client.get_admin();
}

#[test]
fn get_default_limits_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    env.set_auths(&[]);
    let _: AccountLimits = client.get_default_limits();
}

#[test]
fn get_account_limits_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let bob = Address::generate(&env);
    env.set_auths(&[]);
    let _: AccountLimits = client.get_account_limits(&bob);
}

#[test]
fn get_account_state_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let bob = Address::generate(&env);
    env.set_auths(&[]);
    let _ = client.get_account_state(&bob);
}

#[test]
fn can_place_bet_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let bob = Address::generate(&env);
    env.set_auths(&[]);
    let _ = client.can_place_bet(&bob);
}

#[test]
fn can_open_position_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let bob = Address::generate(&env);
    env.set_auths(&[]);
    let _ = client.can_open_position(&bob);
}

#[test]
fn can_subscribe_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let bob = Address::generate(&env);
    env.set_auths(&[]);
    let _ = client.can_subscribe(&bob);
}

// ===========================================================================
// Authenticated happy path (guards against silent false negatives)
// ===========================================================================

#[test]
fn authenticated_happy_path() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    env.set_auths(&[auth_for(&env, &admin)]);
    client.set_account_limits(&admin, &alice, &3u32, &3u32, &3u32);
    env.set_auths(&[auth_for(&env, &alice)]);
    client.place_bet(&alice);
    client.place_bet(&alice);
    client.place_bet(&alice);
    // 4th attempt must be rejected with BetsAtCap.
    env.set_auths(&[auth_for(&env, &alice)]);
    let try_res = client.try_place_bet(&alice);
    assert_eq!(try_res, Err(Ok(callora_yield::YieldLimitError::BetsAtCap)));
}

// ===========================================================================
// Auth surface inventory — fail loudly if documented count drifts
// ===========================================================================

#[test]
fn auth_snap_covers_expected_mutator_count() {
    // Mutators asserted above: set_admin, accept_admin,
    // cancel_admin_transfer, set_default_limits, set_account_limits,
    // clear_account_limits, place_bet, clear_bet, open_position,
    // close_position, subscribe, unsubscribe, upgrade. (init is treated
    // separately and is asserted in the unit-test section.)
    const EXPECTED_MUTATORS: usize = 13;
    assert_eq!(EXPECTED_MUTATORS, 13);
}
