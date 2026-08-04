//! # Auth snapshot — per-entrypoint authorization tests (yield limits)
//!
//! Snapshots the required auth surface for the on-chain
//! `CalloraYieldLimits` contract. Admin and user mutators must keep their
//! current `require_auth` behavior; read-only views must remain auth-free.

extern crate std;

use callora_yield::{AccountState, CalloraYieldLimits, CalloraYieldLimitsClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn create_contract(env: &Env) -> (Address, CalloraYieldLimitsClient<'_>) {
    env.mock_all_auths();
    let contract_id = env.register(CalloraYieldLimits, ());
    let client = CalloraYieldLimitsClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.init(&admin);
    (admin, client)
}

#[test]
fn set_default_limits_requires_auth() {
    let env = Env::default();
    let (admin, client) = create_contract(&env);

    env.set_auths(&[]);
    let res = client.try_set_default_limits(&admin, &5u32, &5u32, &5u32);
    assert!(res.is_err(), "set_default_limits must require auth");
}

#[test]
fn set_account_limits_requires_auth() {
    let env = Env::default();
    let (admin, client) = create_contract(&env);
    let account = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_account_limits(&admin, &account, &3u32, &3u32, &3u32);
    assert!(res.is_err(), "set_account_limits must require auth");
}

#[test]
fn set_admin_requires_auth() {
    let env = Env::default();
    let (admin, client) = create_contract(&env);
    let new_admin = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_admin(&admin, &new_admin);
    assert!(res.is_err(), "set_admin must require auth");
}

#[test]
fn accept_admin_requires_auth() {
    let env = Env::default();
    let (admin, client) = create_contract(&env);
    let pending = Address::generate(&env);

    env.mock_all_auths();
    client.set_admin(&admin, &pending);

    env.set_auths(&[]);
    let res = client.try_accept_admin(&pending);
    assert!(res.is_err(), "accept_admin must require auth");
}

#[test]
fn get_admin_does_not_require_auth() {
    let env = Env::default();
    let (admin, client) = create_contract(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn get_default_limits_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = create_contract(&env);

    env.set_auths(&[]);
    let limits = client.get_default_limits();
    assert_eq!(limits.max_bets, 0);
    assert_eq!(limits.max_positions, 0);
    assert_eq!(limits.max_subscriptions, 0);
}

#[test]
fn get_account_limits_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = create_contract(&env);
    let account = Address::generate(&env);

    env.set_auths(&[]);
    let limits = client.get_account_limits(&account);
    assert_eq!(limits.max_bets, 0);
    assert_eq!(limits.max_positions, 0);
    assert_eq!(limits.max_subscriptions, 0);
}

#[test]
fn get_account_state_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = create_contract(&env);
    let account = Address::generate(&env);

    env.set_auths(&[]);
    let state: AccountState = client.get_account_state(&account);
    assert_eq!(state.bets, 0);
    assert_eq!(state.positions, 0);
    assert_eq!(state.subscriptions, 0);
}

#[test]
fn can_place_bet_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = create_contract(&env);
    let account = Address::generate(&env);

    env.set_auths(&[]);
    assert!(!client.can_place_bet(&account));
}

#[test]
fn can_open_position_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = create_contract(&env);
    let account = Address::generate(&env);

    env.set_auths(&[]);
    assert!(!client.can_open_position(&account));
}

#[test]
fn can_subscribe_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = create_contract(&env);
    let account = Address::generate(&env);

    env.set_auths(&[]);
    assert!(!client.can_subscribe(&account));
}
