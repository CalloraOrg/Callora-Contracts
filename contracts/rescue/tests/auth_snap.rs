#![cfg(test)]

extern crate std;

use callora_rescue::{CalloraRescue, CalloraRescueClient, RescueError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn create_contract(env: &Env) -> CalloraRescueClient<'_> {
    let contract_id = env.register(CalloraRescue, ());
    CalloraRescueClient::new(env, &contract_id)
}

fn setup(env: &Env) -> (Address, CalloraRescueClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let client = create_contract(env);
    client.init(&admin);
    (admin, client)
}

#[test]
fn init_requires_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let client = create_contract(&env);

    env.set_auths(&[]);
    let res = client.try_init(&admin);
    assert!(res.is_err(), "init must require auth");
}

#[test]
fn rescue_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    let token = Address::generate(&env);
    let to = Address::generate(&env);
    let res = client.try_rescue(&admin, &token, &to, &100_i128);
    assert!(res.is_err(), "rescue must require auth");
}

#[test]
fn rescue_capped_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    let token = Address::generate(&env);
    let to = Address::generate(&env);
    let res = client.try_rescue_capped(&admin, &token, &to, &100_i128, &1000_i128);
    assert!(res.is_err(), "rescue_capped must require auth");
}

#[test]
fn total_rescued_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    env.set_auths(&[]);
    let total = client.total_rescued();
    assert_eq!(total, 0);
}

#[test]
fn get_admin_does_not_require_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn admin_with_auth_can_call_all_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let client = create_contract(&env);
    client.init(&admin);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.total_rescued(), 0);

    let token = Address::generate(&env);
    let to = Address::generate(&env);
    let _ = client.try_rescue(&admin, &token, &to, &100_i128);
    let _ = client.try_rescue_capped(&admin, &token, &to, &50_i128, &1000_i128);
}
