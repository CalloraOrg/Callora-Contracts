//! Adversarial authorization regression tests for the upgrade entry points
//! (`set_cooldown`, `check_and_record_upgrade`) — issues #1058 and #1226.
//!
//! Every upgrade entry point must fail closed when the caller has not
//! authorized the invocation. `set_cooldown` must additionally reject any
//! caller that is not the stored admin, and any out-of-bounds cooldown, without
//! mutating state or emitting events.
//! `check_and_record_upgrade` must likewise reject any caller that is not the
//! stored admin, so a non-admin signer cannot keep resetting the cooldown.

extern crate std;

use callora_upgrade::admin::{
    DEFAULT_COOLDOWN_SECONDS, MAX_COOLDOWN_SECONDS, MIN_COOLDOWN_SECONDS,
};
use callora_upgrade::errors::UpgradeError;
use callora_upgrade::events;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, TryFromVal};

// ---------------------------------------------------------------------------
// Thin harness contract that delegates to the `admin` upgrade module so the
// module's entry points are exercised through the real Soroban XCF layer.
// ---------------------------------------------------------------------------

#[contract]
pub struct UpgradeHarness;

#[contractimpl]
impl UpgradeHarness {
    pub fn init_admin(env: Env, admin: Address) -> Result<(), UpgradeError> {
        callora_upgrade::admin::init_admin(&env, &admin)
    }

    pub fn get_admin(env: Env) -> Option<Address> {
        callora_upgrade::admin::get_admin(&env)
    }

    pub fn set_cooldown(env: Env, caller: Address, cooldown: u64) -> Result<(), UpgradeError> {
        callora_upgrade::admin::set_cooldown(&env, &caller, cooldown)
    }

    pub fn get_cooldown(env: Env) -> u64 {
        callora_upgrade::admin::get_cooldown(&env)
    }

    pub fn check_and_record_upgrade(env: Env, caller: Address) -> Result<(), UpgradeError> {
        callora_upgrade::admin::check_and_record_upgrade(&env, &caller)
    }

    pub fn get_last_upgrade_time(env: Env) -> Option<u64> {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "last_upg_tm"))
    }
}

/// Harness with no admin stored.
fn harness(env: &Env) -> (UpgradeHarnessClient<'_>, Address, Address) {
    let id = env.register(UpgradeHarness, ());
    let client = UpgradeHarnessClient::new(env, &id);
    let caller = Address::generate(env);
    (client, id, caller)
}

/// Harness with an admin stored. Auths are mocked for setup only; callers that
/// need the "no signature" case clear them with `env.set_auths(&[])`.
fn harness_with_admin(env: &Env) -> (UpgradeHarnessClient<'_>, Address, Address) {
    env.mock_all_auths();
    let (client, id, admin) = harness(env);
    client.init_admin(&admin);
    (client, id, admin)
}

// ---------------------------------------------------------------------------
// Authentication (fail closed without a signature)
// ---------------------------------------------------------------------------

/// Even the real admin is rejected when the call is not authorized.
#[test]
fn set_cooldown_requires_auth() {
    let env = Env::default();
    let (client, _id, admin) = harness_with_admin(&env);
    env.set_auths(&[]);

    let result = client.try_set_cooldown(&admin, &7_200);
    assert!(
        result.is_err(),
        "set_cooldown must reject an unauthenticated caller"
    );
    assert_eq!(client.get_cooldown(), DEFAULT_COOLDOWN_SECONDS);
}

/// `check_and_record_upgrade` without caller authorization must fail and must
/// not record a last-upgrade timestamp (no partial state).
#[test]
fn check_and_record_upgrade_requires_auth() {
    let env = Env::default();
    // Deliberately no `mock_all_auths()`.
    let (client, _id, caller) = harness(&env);

    let result = client.try_check_and_record_upgrade(&caller);
    assert!(
        result.is_err(),
        "check_and_record_upgrade must reject an unauthenticated caller"
    );
    assert!(client.get_last_upgrade_time().is_none());
}

/// The admin works when authorized; a different identity without auth is rejected.
#[test]
fn second_caller_still_requires_its_own_auth() {
    let env = Env::default();
    let (client, _id, admin) = harness_with_admin(&env);
    let other = Address::generate(&env);

    client.set_cooldown(&admin, &MIN_COOLDOWN_SECONDS);
    assert_eq!(client.get_cooldown(), MIN_COOLDOWN_SECONDS);
    client.check_and_record_upgrade(&admin);

    env.set_auths(&[]);
    assert!(client.try_check_and_record_upgrade(&other).is_err());
}

// ---------------------------------------------------------------------------
// Admin gate for `check_and_record_upgrade` (issue #1234)
// ---------------------------------------------------------------------------

/// A non-admin who DOES sign must be rejected, must not move `last_upg_tm`,
/// and must not emit any events.
#[test]
fn non_admin_cannot_check_and_record_upgrade() {
    let env = Env::default();
    let (client, _id, _admin) = harness_with_admin(&env); // mock_all_auths active
    let attacker = Address::generate(&env);

    let res = client.try_check_and_record_upgrade(&attacker);
    assert_eq!(res, Err(Ok(UpgradeError::Unauthorized)));
    assert!(client.get_last_upgrade_time().is_none());
    assert_eq!(env.events().all().len(), 0);
}

/// The admin's first call succeeds and emits `upgrade_started` then
/// `upgrade_recorded`, in that order.
#[test]
fn admin_first_check_and_record_upgrade_succeeds() {
    let env = Env::default();
    let (client, id, admin) = harness_with_admin(&env);

    client.check_and_record_upgrade(&admin);
    assert!(client.get_last_upgrade_time().is_some());

    let all = env.events().all();
    assert_eq!(all.len(), 2);
    let (e0, t0, _) = all.get(0).unwrap();
    let (e1, t1, _) = all.get(1).unwrap();
    assert_eq!(e0, id);
    assert_eq!(e1, id);
    assert_eq!(
        Symbol::try_from_val(&env, &t0.get(0).unwrap()).unwrap(),
        events::event_upgrade_started(&env)
    );
    assert_eq!(
        Symbol::try_from_val(&env, &t1.get(0).unwrap()).unwrap(),
        events::event_upgrade_recorded(&env)
    );
}

/// A second admin call inside the cooldown window is rejected and emits nothing.
#[test]
fn admin_within_cooldown_is_rejected() {
    let env = Env::default();
    let (client, _id, admin) = harness_with_admin(&env);

    client.check_and_record_upgrade(&admin);
    let before = client.get_last_upgrade_time();
    let events_before = env.events().all().len();

    let res = client.try_check_and_record_upgrade(&admin);
    assert_eq!(res, Err(Ok(UpgradeError::CooldownNotElapsed)));
    assert_eq!(client.get_last_upgrade_time(), before);
    assert_eq!(env.events().all().len(), events_before);
}

/// Once the cooldown elapses the admin can record again.
#[test]
fn admin_after_cooldown_succeeds() {
    let env = Env::default();
    let (client, _id, admin) = harness_with_admin(&env);

    client.check_and_record_upgrade(&admin);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_COOLDOWN_SECONDS + 1);
    client.check_and_record_upgrade(&admin);
}

/// Griefing proof-of-concept: an attacker calling every hour must not lock the
/// admin out. After the cooldown elapses from the admin's last successful
/// record, the admin can upgrade again.
#[test]
fn attacker_cannot_grief_admin_out_of_upgrades() {
    let env = Env::default();
    let (client, _id, admin) = harness_with_admin(&env);
    let attacker = Address::generate(&env);

    client.check_and_record_upgrade(&admin);
    let admin_time = client.get_last_upgrade_time().unwrap();

    // Attacker hammers the entry point every hour for a full cooldown window.
    let mut t = env.ledger().timestamp();
    for _ in 0..(DEFAULT_COOLDOWN_SECONDS / 3_600 + 1) {
        t += 3_600;
        env.ledger().set_timestamp(t);
        let res = client.try_check_and_record_upgrade(&attacker);
        assert_eq!(res, Err(Ok(UpgradeError::Unauthorized)));
    }
    assert_eq!(client.get_last_upgrade_time(), Some(admin_time));

    // Admin can still upgrade once the cooldown from its own record elapses.
    env.ledger()
        .set_timestamp(admin_time + DEFAULT_COOLDOWN_SECONDS + 1);
    client.check_and_record_upgrade(&admin);
}

/// With no admin stored, `check_and_record_upgrade` fails closed.
#[test]
fn check_and_record_upgrade_before_init_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _id, caller) = harness(&env);

    let res = client.try_check_and_record_upgrade(&caller);
    assert_eq!(res, Err(Ok(UpgradeError::NotInitialized)));
    assert!(client.get_last_upgrade_time().is_none());
    assert_eq!(env.events().all().len(), 0);
}

/// A custom cooldown is reflected in the `upgrade_started` event data.
#[test]
fn custom_cooldown_is_reflected_in_upgrade_started_event() {
    let env = Env::default();
    let (client, _id, admin) = harness_with_admin(&env);

    client.set_cooldown(&admin, &7_200);
    client.check_and_record_upgrade(&admin);

    let (_, topics, data) = env.events().all().get(0).unwrap();
    assert_eq!(
        Symbol::try_from_val(&env, &topics.get(0).unwrap()).unwrap(),
        events::event_upgrade_started(&env)
    );
    let cooldown = u64::try_from_val(&env, &data).unwrap();
    assert_eq!(cooldown, 7_200);
}

// ---------------------------------------------------------------------------
// Admin gate (issue #1226)
// ---------------------------------------------------------------------------

/// A non-admin who DOES sign must still be rejected, including the exploit
/// case from the issue: zeroing the cooldown.
#[test]
fn non_admin_cannot_set_cooldown() {
    let env = Env::default();
    let (client, _id, _admin) = harness_with_admin(&env); // mock_all_auths active
    let attacker = Address::generate(&env);

    for value in [0u64, 1, MIN_COOLDOWN_SECONDS, 7_200, MAX_COOLDOWN_SECONDS] {
        let res = client.try_set_cooldown(&attacker, &value);
        assert_eq!(res, Err(Ok(UpgradeError::Unauthorized)), "value {value}");
    }
    assert_eq!(client.get_cooldown(), DEFAULT_COOLDOWN_SECONDS);
}

/// A rejected non-admin attempt must not change an already-configured cooldown.
#[test]
fn non_admin_attempt_does_not_change_existing_cooldown() {
    let env = Env::default();
    let (client, _id, admin) = harness_with_admin(&env);
    let attacker = Address::generate(&env);

    client.set_cooldown(&admin, &7_200);
    let res = client.try_set_cooldown(&attacker, &0);
    assert_eq!(res, Err(Ok(UpgradeError::Unauthorized)));
    assert_eq!(client.get_cooldown(), 7_200);
}

/// With no admin stored, nobody can set the cooldown (fail closed).
#[test]
fn set_cooldown_before_init_is_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _id, caller) = harness(&env);

    let res = client.try_set_cooldown(&caller, &7_200);
    assert_eq!(res, Err(Ok(UpgradeError::NotInitialized)));
    assert_eq!(client.get_cooldown(), DEFAULT_COOLDOWN_SECONDS);
}

/// The admin can only be installed once.
#[test]
fn init_admin_twice_is_rejected() {
    let env = Env::default();
    let (client, _id, admin) = harness_with_admin(&env);
    let other = Address::generate(&env);

    let res = client.try_init_admin(&other);
    assert_eq!(res, Err(Ok(UpgradeError::AlreadyInitialized)));
    assert_eq!(client.get_admin(), Some(admin));
}

/// `init_admin` requires the admin's own signature.
#[test]
fn init_admin_requires_auth() {
    let env = Env::default();
    let (client, _id, admin) = harness(&env);
    assert!(client.try_init_admin(&admin).is_err());
    assert_eq!(client.get_admin(), None);
}

// ---------------------------------------------------------------------------
// Bounds (issue #1226)
// ---------------------------------------------------------------------------

#[test]
fn cooldown_below_min_or_above_max_is_rejected() {
    let env = Env::default();
    let (client, _id, admin) = harness_with_admin(&env);

    for value in [
        0u64,
        1,
        MIN_COOLDOWN_SECONDS - 1,
        MAX_COOLDOWN_SECONDS + 1,
        u64::MAX,
    ] {
        let res = client.try_set_cooldown(&admin, &value);
        assert_eq!(res, Err(Ok(UpgradeError::InvalidCooldown)), "value {value}");
    }
    assert_eq!(client.get_cooldown(), DEFAULT_COOLDOWN_SECONDS);
}

#[test]
fn cooldown_boundary_values_are_accepted() {
    let env = Env::default();
    let (client, _id, admin) = harness_with_admin(&env);

    client.set_cooldown(&admin, &MIN_COOLDOWN_SECONDS);
    assert_eq!(client.get_cooldown(), MIN_COOLDOWN_SECONDS);

    client.set_cooldown(&admin, &MAX_COOLDOWN_SECONDS);
    assert_eq!(client.get_cooldown(), MAX_COOLDOWN_SECONDS);
}

/// Compile-time check: if the default ever leaves the allowed range, the build fails.
const _: () = {
    assert!(DEFAULT_COOLDOWN_SECONDS >= MIN_COOLDOWN_SECONDS);
    assert!(DEFAULT_COOLDOWN_SECONDS <= MAX_COOLDOWN_SECONDS);
};

// ---------------------------------------------------------------------------
// Event carries the old value (issue #1226)
// ---------------------------------------------------------------------------

/// Decode the most recent event as (emitter, topic0, topic1, (old, new)).
fn last_cooldown_event(env: &Env) -> (Address, Symbol, Address, (u64, u64)) {
    let (emitter, topics, data) = env.events().all().last().unwrap();
    let topic0 = Symbol::try_from_val(env, &topics.get(0).unwrap()).unwrap();
    let topic1 = Address::try_from_val(env, &topics.get(1).unwrap()).unwrap();
    let payload = <(u64, u64)>::try_from_val(env, &data).unwrap();
    (emitter, topic0, topic1, payload)
}

#[test]
fn cooldown_set_event_includes_old_value() {
    let env = Env::default();
    let (client, id, admin) = harness_with_admin(&env);

    // First change: old value is the default.
    client.set_cooldown(&admin, &7_200);
    assert_eq!(
        last_cooldown_event(&env),
        (
            id.clone(),
            events::event_cooldown_set(&env),
            admin.clone(),
            (DEFAULT_COOLDOWN_SECONDS, 7_200)
        )
    );

    // Second change: old value is the previously set one.
    client.set_cooldown(&admin, &10_800);
    assert_eq!(
        last_cooldown_event(&env),
        (id, events::event_cooldown_set(&env), admin, (7_200, 10_800))
    );
}
