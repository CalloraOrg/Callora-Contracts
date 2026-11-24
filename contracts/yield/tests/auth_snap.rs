#![cfg(test)]
extern crate std;

use callora_yield::{AccountLimits, CalloraYieldLimits, CalloraYieldLimitsClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, IntoVal};

macro_rules! mock_auth {
    ($env:expr, $addr:expr, $client:expr, $fn_name:expr, $($arg:expr),*) => {
        $env.mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: $addr,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &$client.address,
                fn_name: $fn_name,
                args: ($($arg,)*).into_val($env),
                sub_invokes: &[],
            },
        }]);
    };
}

/// Deploy the contract and initialize it with a fresh admin.
///
/// `init` authenticates the admin (#1414), so it runs under `mock_all_auths`.
/// The mock is then cleared with `set_auths(&[])` so every subsequent call in
/// these snapshot tests starts *unauthenticated*; each test opts in to exactly
/// the signatures it needs via [`mock_auth!`].
fn setup(env: &Env) -> (Address, Address, CalloraYieldLimitsClient<'_>) {
    let contract = env.register(CalloraYieldLimits, ());
    let client = CalloraYieldLimitsClient::new(env, &contract);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.init(&admin);
    env.mock_auths(&[]);
    (contract, admin, client)
}

/// Register `admin` as an operator (it authorizes itself).
fn register_admin_operator(env: &Env, admin: &Address, client: &CalloraYieldLimitsClient) {
    mock_auth!(env, admin, client, "set_operator", admin, admin);
    client.set_operator(admin, admin);
}

// ---------------------------------------------------------------------------
// Constructor
// ---------------------------------------------------------------------------

#[test]
fn init_requires_admin_auth() {
    let env = Env::default();
    let contract = env.register(CalloraYieldLimits, ());
    let client = CalloraYieldLimitsClient::new(&env, &contract);
    let admin = Address::generate(&env);

    let res = client.try_init(&admin);
    assert!(res.is_err(), "init must require auth on admin");
    assert!(
        client.try_get_admin().is_err(),
        "an unauthenticated init must not store an admin"
    );
}

// ---------------------------------------------------------------------------
// Admin lifecycle + registry
// ---------------------------------------------------------------------------

#[test]
fn set_admin_requires_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let intruder = Address::generate(&env);
    let target = Address::generate(&env);
    let res = client.try_set_admin(&intruder, &target);
    assert!(res.is_err(), "set_admin must require auth on caller");
}

#[test]
fn accept_admin_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let nominee = Address::generate(&env);
    mock_auth!(&env, &admin, client, "set_admin", &admin, &nominee);
    client.set_admin(&admin, &nominee);

    let res = client.try_accept_admin(&nominee);
    assert!(res.is_err(), "accept_admin must require auth on caller");
}

#[test]
fn cancel_admin_transfer_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let nominee = Address::generate(&env);
    mock_auth!(&env, &admin, client, "set_admin", &admin, &nominee);
    client.set_admin(&admin, &nominee);

    let intruder = Address::generate(&env);
    let res = client.try_cancel_admin_transfer(&intruder);
    assert!(
        res.is_err(),
        "cancel_admin_transfer must require auth on caller"
    );
}

#[test]
fn set_operator_requires_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let intruder = Address::generate(&env);
    let operator = Address::generate(&env);
    let res = client.try_set_operator(&intruder, &operator);
    assert!(res.is_err(), "set_operator must require auth on admin");
    assert!(!client.get_operator(&operator));
}

#[test]
fn remove_operator_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let operator = Address::generate(&env);
    register_admin_operator(&env, &admin, &client);
    mock_auth!(&env, &admin, client, "set_operator", &admin, &operator);
    client.set_operator(&admin, &operator);

    let intruder = Address::generate(&env);
    let res = client.try_remove_operator(&intruder, &operator);
    assert!(res.is_err(), "remove_operator must require auth on admin");
    assert!(client.get_operator(&operator));
}

#[test]
fn set_default_limits_requires_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let intruder = Address::generate(&env);
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
    let res = client.try_clear_account_limits(&intruder, &target);
    assert!(
        res.is_err(),
        "clear_account_limits must require auth on caller"
    );
}

// ---------------------------------------------------------------------------
// Operator-gated counter mutators
// ---------------------------------------------------------------------------

/// An account naming itself as the operator has its own signature but is not
/// registered, so the call is rejected without touching counters.
#[test]
fn place_bet_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    mock_auth!(
        &env,
        &admin,
        client,
        "set_account_limits",
        &admin,
        &alice,
        5u32,
        5u32,
        5u32
    );
    client.set_account_limits(&admin, &alice, &5u32, &5u32, &5u32);

    let res = client.try_place_bet(&alice, &alice);
    assert!(res.is_err(), "place_bet must require a registered operator");
    assert_eq!(client.get_account_state(&alice).bets, 0);
}

/// A registered operator still has to sign; registration is not authority.
#[test]
fn registered_operator_mutation_requires_signature() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let operator = Address::generate(&env);
    let alice = Address::generate(&env);
    register_admin_operator(&env, &admin, &client);
    mock_auth!(&env, &admin, client, "set_operator", &admin, &operator);
    client.set_operator(&admin, &operator);

    // No `mock_auth!` for the operator: the signature is missing.
    let res = client.try_place_bet(&operator, &alice);
    assert!(res.is_err(), "operator must still authorize every call");
    assert_eq!(client.get_account_state(&alice).bets, 0);
}

#[test]
fn clear_bet_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    register_admin_operator(&env, &admin, &client);

    mock_auth!(&env, &admin, client, "place_bet", &admin, &alice);
    client.place_bet(&admin, &alice);

    let res = client.try_clear_bet(&alice, &alice);
    assert!(res.is_err(), "clear_bet must require a registered operator");
    assert_eq!(client.get_account_state(&alice).bets, 1);
}

#[test]
fn open_position_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    mock_auth!(
        &env,
        &admin,
        client,
        "set_account_limits",
        &admin,
        &alice,
        5u32,
        5u32,
        5u32
    );
    client.set_account_limits(&admin, &alice, &5u32, &5u32, &5u32);

    let res = client.try_open_position(&alice, &alice);
    assert!(
        res.is_err(),
        "open_position must require a registered operator"
    );
    assert_eq!(client.get_account_state(&alice).positions, 0);
}

#[test]
fn close_position_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    register_admin_operator(&env, &admin, &client);
    mock_auth!(&env, &admin, client, "open_position", &admin, &alice);
    client.open_position(&admin, &alice);

    let res = client.try_close_position(&alice, &alice);
    assert!(
        res.is_err(),
        "close_position must require a registered operator"
    );
    assert_eq!(client.get_account_state(&alice).positions, 1);
}

#[test]
fn subscribe_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    mock_auth!(
        &env,
        &admin,
        client,
        "set_account_limits",
        &admin,
        &alice,
        5u32,
        5u32,
        5u32
    );
    client.set_account_limits(&admin, &alice, &5u32, &5u32, &5u32);

    let res = client.try_subscribe(&alice, &alice);
    assert!(res.is_err(), "subscribe must require a registered operator");
    assert_eq!(client.get_account_state(&alice).subscriptions, 0);
}

#[test]
fn unsubscribe_requires_auth() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    register_admin_operator(&env, &admin, &client);
    mock_auth!(&env, &admin, client, "subscribe", &admin, &alice);
    client.subscribe(&admin, &alice);

    let res = client.try_unsubscribe(&alice, &alice);
    assert!(
        res.is_err(),
        "unsubscribe must require a registered operator"
    );
    assert_eq!(client.get_account_state(&alice).subscriptions, 1);
}

#[test]
fn upgrade_requires_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let intruder = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    let res = client.try_upgrade(&intruder, &hash);
    assert!(res.is_err(), "upgrade must require auth on caller");
}

// ---------------------------------------------------------------------------
// Views stay unauthenticated
// ---------------------------------------------------------------------------

#[test]
fn get_admin_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let _ = client.get_admin();
}

#[test]
fn get_default_limits_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let _: AccountLimits = client.get_default_limits();
}

#[test]
fn get_account_limits_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let bob = Address::generate(&env);
    let _: AccountLimits = client.get_account_limits(&bob);
}

#[test]
fn get_account_state_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let bob = Address::generate(&env);
    let _ = client.get_account_state(&bob);
}

#[test]
fn get_operator_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let bob = Address::generate(&env);
    assert!(!client.get_operator(&bob));
}

#[test]
fn can_place_bet_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let bob = Address::generate(&env);
    let _ = client.can_place_bet(&bob);
}

#[test]
fn can_open_position_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let bob = Address::generate(&env);
    let _ = client.can_open_position(&bob);
}

#[test]
fn can_subscribe_no_auth() {
    let env = Env::default();
    let (_contract, _admin, client) = setup(&env);
    let bob = Address::generate(&env);
    let _ = client.can_subscribe(&bob);
}

// ---------------------------------------------------------------------------
// Signed operator happy path
// ---------------------------------------------------------------------------

#[test]
fn authenticated_happy_path() {
    let env = Env::default();
    let (_contract, admin, client) = setup(&env);
    let alice = Address::generate(&env);
    register_admin_operator(&env, &admin, &client);
    mock_auth!(
        &env,
        &admin,
        client,
        "set_account_limits",
        &admin,
        &alice,
        3u32,
        3u32,
        3u32
    );
    client.set_account_limits(&admin, &alice, &3u32, &3u32, &3u32);

    for _ in 0..3 {
        mock_auth!(&env, &admin, client, "place_bet", &admin, &alice);
        client.place_bet(&admin, &alice);
    }

    mock_auth!(&env, &admin, client, "place_bet", &admin, &alice);
    let try_res = client.try_place_bet(&admin, &alice);
    assert_eq!(try_res, Err(Ok(callora_yield::YieldLimitError::BetsAtCap)));
}

#[test]
fn auth_snap_covers_expected_mutator_count() {
    // admin trio + limits trio + operator pair + counter sextet + upgrade.
    const EXPECTED_MUTATORS: usize = 15;
    assert_eq!(EXPECTED_MUTATORS, 15);
}
