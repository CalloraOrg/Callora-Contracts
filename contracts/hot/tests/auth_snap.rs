#![cfg(test)]

extern crate std;

use callora_hot::{CalloraHot, CalloraHotClient};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, Symbol};

fn create_contract(env: &Env) -> CalloraHotClient<'_> {
    let contract_id = env.register(CalloraHot, ());
    CalloraHotClient::new(env, &contract_id)
}

fn setup(env: &Env) -> (Address, CalloraHotClient<'_>) {
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);
    let admin = Address::generate(env);
    let signer = Address::generate(env);
    let client = create_contract(env);
    client.init(&admin, &signer, &None);
    (admin, client)
}

#[test]
fn init_requires_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let signer = Address::generate(&env);
    let client = create_contract(&env);

    env.set_auths(&[]);
    let res = client.try_init(&admin, &signer, &None);
    assert!(res.is_err(), "init must require auth");
}

#[test]
fn set_cooldown_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_set_cooldown(&admin, &300);
    assert!(res.is_err(), "set_cooldown must require auth");
}

#[test]
fn pause_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_pause(&admin);
    assert!(res.is_err(), "pause must require auth");
}

#[test]
fn unpause_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.mock_all_auths();
    client.set_cooldown(&admin, &1);
    client.pause(&admin);
    env.ledger().set_timestamp(1_000_001);

    env.set_auths(&[]);
    let res = client.try_unpause(&admin);
    assert!(res.is_err(), "unpause must require auth");
}

#[test]
fn rotate_signer_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    let new_signer = Address::generate(&env);
    let res = client.try_rotate_signer(&admin, &new_signer);
    assert!(res.is_err(), "rotate_signer must require auth");
}

#[test]
fn set_admin_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    let new_admin = Address::generate(&env);
    let res = client.try_set_admin(&admin, &new_admin);
    assert!(res.is_err(), "set_admin must require auth");
}

#[test]
fn accept_admin_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.mock_all_auths();
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    env.set_auths(&[]);
    let res = client.try_accept_admin(&new_admin);
    assert!(res.is_err(), "accept_admin must require auth");
}

#[test]
fn get_cooldown_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    env.set_auths(&[]);
    let cd = client.get_cooldown();
    assert_eq!(cd, 3600);
}

#[test]
fn get_admin_does_not_require_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn get_pending_admin_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn get_signer_does_not_require_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.mock_all_auths();
    let new_signer = Address::generate(&env);
    client.set_cooldown(&admin, &1);
    client.rotate_signer(&admin, &new_signer);

    env.set_auths(&[]);
    assert_eq!(client.get_signer(), new_signer);
}

#[test]
fn is_paused_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    env.set_auths(&[]);
    assert!(!client.is_paused());
}

#[test]
fn cooldown_remaining_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    env.set_auths(&[]);
    let remaining = client.cooldown_remaining(&Symbol::new(&env, "pause"));
    assert_eq!(remaining, 0);
}

#[test]
fn is_ready_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    env.set_auths(&[]);
    assert!(client.is_ready(&Symbol::new(&env, "pause")));
}

#[test]
fn admin_with_auth_can_call_all_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let admin = Address::generate(&env);
    let signer = Address::generate(&env);
    let client = create_contract(&env);
    client.init(&admin, &signer, &Some(1));

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_signer(), signer);

    client.pause(&admin);
    assert!(client.is_paused());

    env.ledger().set_timestamp(1_000_001);
    client.unpause(&admin);
    assert!(!client.is_paused());

    let new_signer = Address::generate(&env);
    client.rotate_signer(&admin, &new_signer);
    assert_eq!(client.get_signer(), new_signer);

    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
}
