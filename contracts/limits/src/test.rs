extern crate std;

use super::*;
use crate::{CalloraLimits, CalloraLimitsClient};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env, IntoVal, Symbol};

/// Register the contract and return (env, contract_id, admin) with a completed
/// `init`. All auths are mocked so tests can focus on contract logic.
fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(CalloraLimits, ());
    let client = CalloraLimitsClient::new(&env, &contract);
    let admin = Address::generate(&env);
    client.init(&admin);
    (env, contract, admin)
}

fn client<'a>(env: &Env, contract: &Address) -> CalloraLimitsClient<'a> {
    CalloraLimitsClient::new(env, contract)
}

// ── init ────────────────────────────────────────────────────────────────

#[test]
fn init_sets_admin() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    assert_eq!(c.get_admin(), admin);
}

#[test]
fn init_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(CalloraLimits, ());
    let c = client(&env, &contract);
    let admin = Address::generate(&env);
    c.init(&admin);

    let res = c.try_init(&admin);
    assert_eq!(res, Err(Ok(LimitsError::AlreadyInitialized)));
}

#[test]
fn init_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(CalloraLimits, ());
    let c = client(&env, &contract);
    let admin = Address::generate(&env);
    c.init(&admin);

    let found = env.events().all().iter().any(|e| {
        !e.1.is_empty() && {
            let sym: Symbol = e.1.get(0).unwrap().into_val(&env);
            sym == Symbol::new(&env, "init")
        }
    });
    assert!(found);
}

#[test]
fn get_admin_before_init_fails() {
    let env = Env::default();
    let contract = env.register(CalloraLimits, ());
    let c = client(&env, &contract);
    assert_eq!(c.try_get_admin(), Err(Ok(LimitsError::NotInitialized)));
}

// ── set_limit ───────────────────────────────────────────────────────────

#[test]
fn set_and_get_limit() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);

    c.set_limit(&admin, &token, &100, &1_000);
    let limit = c.get_limit(&token).unwrap();
    assert_eq!(limit.min, 100);
    assert_eq!(limit.max, 1_000);
    assert_eq!(limit.token, token);
}

#[test]
fn set_limit_overwrites_previous() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);

    c.set_limit(&admin, &token, &100, &1_000);
    c.set_limit(&admin, &token, &200, &2_000);
    let limit = c.get_limit(&token).unwrap();
    assert_eq!(limit.min, 200);
    assert_eq!(limit.max, 2_000);
}

#[test]
fn set_limit_is_independent_per_token() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token_a = Address::generate(&env);
    let token_b = Address::generate(&env);

    c.set_limit(&admin, &token_a, &10, &100);
    c.set_limit(&admin, &token_b, &50, &500);

    assert_eq!(c.get_limit(&token_a).unwrap().min, 10);
    assert_eq!(c.get_limit(&token_b).unwrap().min, 50);
}

#[test]
fn set_limit_allows_min_equals_max() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);

    c.set_limit(&admin, &token, &500, &500);
    let limit = c.get_limit(&token).unwrap();
    assert_eq!(limit.min, 500);
    assert_eq!(limit.max, 500);
}

#[test]
fn set_limit_allows_unlimited_max() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);

    c.set_limit(&admin, &token, &100, &UNLIMITED_MAX);
    assert_eq!(c.get_limit(&token).unwrap().max, UNLIMITED_MAX);
}

#[test]
fn set_limit_rejects_negative_min() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    assert_eq!(
        c.try_set_limit(&admin, &token, &-1, &100),
        Err(Ok(LimitsError::AmountNegative))
    );
}

#[test]
fn set_limit_rejects_negative_max() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    assert_eq!(
        c.try_set_limit(&admin, &token, &0, &-5),
        Err(Ok(LimitsError::AmountNegative))
    );
}

#[test]
fn set_limit_rejects_max_below_min() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    assert_eq!(
        c.try_set_limit(&admin, &token, &1_000, &100),
        Err(Ok(LimitsError::InvalidLimit))
    );
}

#[test]
fn set_limit_by_non_admin_fails() {
    let (env, contract, _admin) = setup();
    let c = client(&env, &contract);
    let intruder = Address::generate(&env);
    let token = Address::generate(&env);
    assert_eq!(
        c.try_set_limit(&intruder, &token, &0, &100),
        Err(Ok(LimitsError::Unauthorized))
    );
}

#[test]
fn set_limit_emits_event() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    c.set_limit(&admin, &token, &1, &2);

    let count = env
        .events()
        .all()
        .iter()
        .filter(|e| {
            !e.1.is_empty() && {
                let sym: Symbol = e.1.get(0).unwrap().into_val(&env);
                sym == Symbol::new(&env, "limit_set")
            }
        })
        .count();
    assert_eq!(count, 1);
}

// ── remove_limit ──────────────────────────────────────────────────────────

#[test]
fn remove_limit_clears_band() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);

    c.set_limit(&admin, &token, &10, &100);
    assert!(c.has_limit(&token));

    c.remove_limit(&admin, &token);
    assert!(!c.has_limit(&token));
    assert_eq!(c.get_limit(&token), None);
}

#[test]
fn remove_limit_on_missing_is_noop() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    // Should not panic even though nothing is configured.
    c.remove_limit(&admin, &token);
    assert!(!c.has_limit(&token));
}

#[test]
fn remove_limit_by_non_admin_fails() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    c.set_limit(&admin, &token, &10, &100);

    let intruder = Address::generate(&env);
    assert_eq!(
        c.try_remove_limit(&intruder, &token),
        Err(Ok(LimitsError::Unauthorized))
    );
}

#[test]
fn remove_limit_emits_event() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    c.remove_limit(&admin, &token);

    let count = env
        .events()
        .all()
        .iter()
        .filter(|e| {
            !e.1.is_empty() && {
                let sym: Symbol = e.1.get(0).unwrap().into_val(&env);
                sym == Symbol::new(&env, "limit_removed")
            }
        })
        .count();
    assert_eq!(count, 1);
}

// ── get_limit / has_limit ─────────────────────────────────────────────────

#[test]
fn get_limit_returns_none_when_unset() {
    let (env, contract, _admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    assert_eq!(c.get_limit(&token), None);
    assert!(!c.has_limit(&token));
}

// ── check_amount ──────────────────────────────────────────────────────────

#[test]
fn check_amount_passes_when_no_limit() {
    let (env, contract, _admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    // Any non-negative amount is fine when unrestricted.
    c.check_amount(&token, &0);
    c.check_amount(&token, &1_000_000);
}

#[test]
fn check_amount_rejects_negative() {
    let (env, contract, _admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    assert_eq!(
        c.try_check_amount(&token, &-1),
        Err(Ok(LimitsError::AmountNegative))
    );
}

#[test]
fn check_amount_within_band_passes() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    c.set_limit(&admin, &token, &100, &1_000);

    c.check_amount(&token, &100); // lower boundary
    c.check_amount(&token, &500); // middle
    c.check_amount(&token, &1_000); // upper boundary
}

#[test]
fn check_amount_below_min_fails() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    c.set_limit(&admin, &token, &100, &1_000);
    assert_eq!(
        c.try_check_amount(&token, &99),
        Err(Ok(LimitsError::BelowMinimum))
    );
}

#[test]
fn check_amount_above_max_fails() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    c.set_limit(&admin, &token, &100, &1_000);
    assert_eq!(
        c.try_check_amount(&token, &1_001),
        Err(Ok(LimitsError::AboveMaximum))
    );
}

#[test]
fn check_amount_unlimited_max_allows_large() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    c.set_limit(&admin, &token, &100, &UNLIMITED_MAX);

    c.check_amount(&token, &100);
    c.check_amount(&token, &i128::MAX);
}

#[test]
fn check_amount_unlimited_max_still_enforces_min() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    c.set_limit(&admin, &token, &100, &UNLIMITED_MAX);
    assert_eq!(
        c.try_check_amount(&token, &99),
        Err(Ok(LimitsError::BelowMinimum))
    );
}

#[test]
fn check_amount_zero_min_allows_zero() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let token = Address::generate(&env);
    c.set_limit(&admin, &token, &0, &1_000);
    c.check_amount(&token, &0);
}

// ── two-step admin rotation ───────────────────────────────────────────────

#[test]
fn admin_rotation_happy_path() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let new_admin = Address::generate(&env);

    c.set_admin(&admin, &new_admin);
    assert_eq!(c.get_pending_admin(), Some(new_admin.clone()));

    c.accept_admin(&new_admin);
    assert_eq!(c.get_admin(), new_admin);
    assert_eq!(c.get_pending_admin(), None);
}

#[test]
fn set_admin_by_non_admin_fails() {
    let (env, contract, _admin) = setup();
    let c = client(&env, &contract);
    let intruder = Address::generate(&env);
    let target = Address::generate(&env);
    assert_eq!(
        c.try_set_admin(&intruder, &target),
        Err(Ok(LimitsError::Unauthorized))
    );
}

#[test]
#[should_panic(expected = "caller is not pending admin")]
fn accept_admin_wrong_caller_panics() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let new_admin = Address::generate(&env);
    let intruder = Address::generate(&env);

    c.set_admin(&admin, &new_admin);
    c.accept_admin(&intruder);
}

#[test]
#[should_panic(expected = "no admin transfer pending")]
fn accept_admin_without_pending_panics() {
    let (env, contract, _admin) = setup();
    let c = client(&env, &contract);
    let someone = Address::generate(&env);
    c.accept_admin(&someone);
}

#[test]
fn cancel_admin_transfer_clears_pending() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let new_admin = Address::generate(&env);

    c.set_admin(&admin, &new_admin);
    c.cancel_admin_transfer(&admin);
    assert_eq!(c.get_pending_admin(), None);
    // Original admin remains in control.
    assert_eq!(c.get_admin(), admin);
}

#[test]
#[should_panic(expected = "no admin transfer pending")]
fn cancel_admin_transfer_without_pending_panics() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    c.cancel_admin_transfer(&admin);
}

#[test]
fn cancel_admin_transfer_by_non_admin_fails() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let new_admin = Address::generate(&env);
    c.set_admin(&admin, &new_admin);

    let intruder = Address::generate(&env);
    assert_eq!(
        c.try_cancel_admin_transfer(&intruder),
        Err(Ok(LimitsError::Unauthorized))
    );
}

#[test]
fn new_admin_can_set_limits_after_rotation() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let new_admin = Address::generate(&env);
    c.set_admin(&admin, &new_admin);
    c.accept_admin(&new_admin);

    let token = Address::generate(&env);
    c.set_limit(&new_admin, &token, &1, &10);
    assert_eq!(c.get_limit(&token).unwrap().max, 10);

    // Old admin no longer has authority.
    assert_eq!(
        c.try_set_limit(&admin, &token, &2, &20),
        Err(Ok(LimitsError::Unauthorized))
    );
}

#[test]
fn set_admin_emits_nominated_event() {
    let (env, contract, admin) = setup();
    let c = client(&env, &contract);
    let new_admin = Address::generate(&env);
    c.set_admin(&admin, &new_admin);

    let found = env.events().all().iter().any(|e| {
        !e.1.is_empty() && {
            let sym: Symbol = e.1.get(0).unwrap().into_val(&env);
            sym == Symbol::new(&env, "admin_nominated")
        }
    });
    assert!(found);
}

// ── upgrade ───────────────────────────────────────────────────────────────

#[test]
fn upgrade_by_non_admin_fails() {
    let (env, contract, _admin) = setup();
    let c = client(&env, &contract);
    let intruder = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    assert_eq!(
        c.try_upgrade(&intruder, &hash),
        Err(Ok(LimitsError::Unauthorized))
    );
}
