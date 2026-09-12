#![cfg(test)]

extern crate std;

use callora_distribute::{
    CalloraDistribute, CalloraDistributeClient,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, Vec};

fn create_contract(env: &Env) -> CalloraDistributeClient<'_> {
    let contract_id = env.register(CalloraDistribute, ());
    CalloraDistributeClient::new(env, &contract_id)
}

fn setup(env: &Env) -> (Address, Address, CalloraDistributeClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let usdc = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let client = create_contract(env);
    client.init(&admin, &usdc);
    (admin, usdc, client)
}

#[test]
fn set_admin_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    let new_admin = Address::generate(&env);
    let res = client.try_set_admin(&admin, &new_admin);
    assert!(res.is_err(), "set_admin must require auth");
}

#[test]
fn accept_admin_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.mock_all_auths();
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    env.set_auths(&[]);
    let res = client.try_accept_admin(&new_admin);
    assert!(res.is_err(), "accept_admin must require auth");
}

#[test]
fn claim_admin_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.mock_all_auths();
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    env.set_auths(&[]);
    let res = client.try_claim_admin(&new_admin);
    assert!(res.is_err(), "claim_admin must require auth");
}

#[test]
fn cancel_admin_transfer_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.mock_all_auths();
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    env.set_auths(&[]);
    let res = client.try_cancel_admin_transfer(&admin);
    assert!(res.is_err(), "cancel_admin_transfer must require auth");
}

#[test]
fn pause_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_pause(&admin);
    assert!(res.is_err(), "pause must require auth");
}

#[test]
fn unpause_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.mock_all_auths();
    client.pause(&admin);

    env.set_auths(&[]);
    let res = client.try_unpause(&admin);
    assert!(res.is_err(), "unpause must require auth");
}

#[test]
fn set_max_distribute_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_set_max_distribute(&admin, &1000);
    assert!(res.is_err(), "set_max_distribute must require auth");
}

#[test]
fn distribute_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    let to = Address::generate(&env);
    let res = client.try_distribute(&admin, &to, &100);
    assert!(res.is_err(), "distribute must require auth");
}

#[test]
fn batch_distribute_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    let to = Address::generate(&env);
    let mut payments = Vec::new(&env);
    payments.push_back((to, 100));
    let res = client.try_batch_distribute(&admin, &payments);
    assert!(res.is_err(), "batch_distribute must require auth");
}

#[test]
fn upgrade_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    let dummy = BytesN::from_array(&env, &[0u8; 32]);
    let res = client.try_upgrade(&admin, &dummy);
    assert!(res.is_err(), "upgrade must require auth");
}

#[test]
fn get_admin_does_not_require_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn get_usdc_token_does_not_require_auth() {
    let env = Env::default();
    let (_admin, usdc, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_usdc_token(), usdc);
}

#[test]
fn get_pending_admin_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn get_paused_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    assert!(!client.get_paused());
}

#[test]
fn get_max_distribute_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_max_distribute(), i128::MAX);
}

#[test]
fn get_max_batch_size_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    assert!(client.get_max_batch_size() > 0);
}

#[test]
fn balance_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.balance(), 0);
}

#[test]
fn version_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    assert!(!client.version().is_empty());
}
