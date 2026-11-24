//! Auth snapshot for the Callora Allowlist contract.
//!
//! Every state-changing entrypoint must call `require_auth`, and every
//! read-only view must be callable without a signature. If a new mutator lands
//! without auth, this file fails in CI.

#![cfg(test)]
extern crate std;

use callora_allowlist::{CalloraAllowlist, CalloraAllowlistClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

fn setup(env: &Env) -> (Address, CalloraAllowlistClient<'_>) {
    let contract = env.register(CalloraAllowlist, ());
    let client = CalloraAllowlistClient::new(env, &contract);
    let admin = Address::generate(env);
    env.mock_all_auths();
    client.init(&admin);
    env.set_auths(&[]);
    (admin, client)
}

#[test]
fn init_requires_auth() {
    let env = Env::default();
    let contract = env.register(CalloraAllowlist, ());
    let client = CalloraAllowlistClient::new(&env, &contract);
    let admin = Address::generate(&env);

    let res = client.try_init(&admin);
    assert!(res.is_err(), "init must require auth on admin");
    assert!(client.try_get_admin().is_err());
}

#[test]
fn set_admin_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let nominee = Address::generate(&env);

    let res = client.try_set_admin(&admin, &nominee);
    assert!(res.is_err(), "set_admin must require auth on caller");
}

#[test]
fn accept_admin_requires_nominee_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let nominee = Address::generate(&env);

    env.mock_all_auths();
    client.set_admin(&admin, &nominee);
    env.set_auths(&[]);

    let res = client.try_accept_admin();
    assert!(res.is_err(), "accept_admin must require nominee auth");
    assert_eq!(client.get_pending_admin(), Some(nominee));
}

#[test]
fn cancel_admin_transfer_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let nominee = Address::generate(&env);

    env.mock_all_auths();
    client.set_admin(&admin, &nominee);
    env.set_auths(&[]);

    let res = client.try_cancel_admin_transfer(&admin);
    assert!(
        res.is_err(),
        "cancel_admin_transfer must require auth on caller"
    );
}

#[test]
fn add_address_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let address = Address::generate(&env);

    let res = client.try_add_address(&admin, &address);
    assert!(res.is_err(), "add_address must require auth on caller");
    assert!(!client.is_allowed(&address));
}

#[test]
fn remove_address_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let address = Address::generate(&env);

    env.mock_all_auths();
    client.add_address(&admin, &address);
    env.set_auths(&[]);

    let res = client.try_remove_address(&admin, &address);
    assert!(res.is_err(), "remove_address must require auth on caller");
    assert!(client.is_allowed(&address));
}

#[test]
fn clear_all_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let res = client.try_clear_all(&admin);
    assert!(res.is_err(), "clear_all must require auth on caller");
}

#[test]
fn views_do_not_require_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let address = Address::generate(&env);

    assert_eq!(client.get_admin(), admin);
    assert!(client.get_pending_admin().is_none());
    assert!(!client.is_allowed(&address));
    assert!(!client.is_authorized_depositor(&address));
    assert_eq!(client.allowlist_count(), 0);
    assert!(client.get_allowlist().is_empty());
    assert!(client.get_allowlist_page(&0, &10).is_empty());
}

#[test]
fn admin_with_auth_can_call_all_entrypoints() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    env.mock_all_auths();

    let first = Address::generate(&env);
    let second = Address::generate(&env);
    client.add_address(&admin, &first);
    client.add_address(&admin, &second);
    assert_eq!(client.allowlist_count(), 2);

    client.remove_address(&admin, &first);
    assert_eq!(client.allowlist_count(), 1);

    let nominee = Address::generate(&env);
    client.set_admin(&admin, &nominee);
    client.cancel_admin_transfer(&admin);
    client.set_admin(&admin, &nominee);
    client.accept_admin();
    assert_eq!(client.get_admin(), nominee);

    client.clear_all(&nominee);
    assert_eq!(client.allowlist_count(), 0);
}
