#![cfg(test)]

extern crate std;

use callora_registry::{CalloraRegistry, CalloraRegistryClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, String};

fn create_contract(env: &Env) -> CalloraRegistryClient<'_> {
    let contract_id = env.register(CalloraRegistry, ());
    CalloraRegistryClient::new(env, &contract_id)
}

fn setup(env: &Env) -> (Address, Address, CalloraRegistryClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let catalog = Address::generate(env);
    let client = create_contract(env);
    client.init(&admin, &catalog);
    (admin, catalog, client)
}

#[test]
fn init_requires_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let catalog = Address::generate(&env);
    let client = create_contract(&env);

    env.set_auths(&[]);
    let res = client.try_init(&admin, &catalog);
    assert!(res.is_err(), "init must require auth");
}

#[test]
fn register_offering_requires_auth() {
    let env = Env::default();
    let (admin, _catalog, client) = setup(&env);

    env.set_auths(&[]);
    let developer = Address::generate(&env);
    let offering_id = String::from_str(&env, "offering-1");
    let metadata = String::from_str(&env, "test metadata");
    let res = client.try_register_offering(&admin, &developer, &offering_id, &metadata);
    assert!(res.is_err(), "register_offering must require auth");
}

#[test]
fn register_offering_with_gate_requires_auth() {
    let env = Env::default();
    let (admin, _catalog, client) = setup(&env);

    env.set_auths(&[]);
    let developer = Address::generate(&env);
    let token = Address::generate(&env);
    let offering_id = String::from_str(&env, "offering-2");
    let metadata = String::from_str(&env, "test metadata");
    let res = client.try_register_offering_with_gate(
        &admin,
        &developer,
        &token,
        &100_i128,
        &offering_id,
        &metadata,
    );
    assert!(
        res.is_err(),
        "register_offering_with_gate must require auth"
    );
}

#[test]
fn is_offering_registered_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _catalog, client) = setup(&env);

    let id = String::from_str(&env, "nonexistent");
    let registered = client.is_offering_registered(&id);
    assert_eq!(registered, false);
}

#[test]
fn registered_count_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _catalog, client) = setup(&env);

    let count = client.registered_count();
    assert_eq!(count, 0);
}

fn setup_with_offering(env: &Env) -> (Address, Address, String, CalloraRegistryClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let developer = Address::generate(env);
    let catalog = env.register(ok_catalog::OkCatalog, ());
    let client = create_contract(env);
    client.init(&admin, &catalog);
    let offering_id = String::from_str(env, "offering-auth");
    client.register_offering(
        &admin,
        &developer,
        &offering_id,
        &String::from_str(env, "test metadata"),
    );
    (admin, developer, offering_id, client)
}

pub mod ok_catalog {
    use soroban_sdk::{contract, contractimpl, Address, Env, String};

    #[contract]
    pub struct OkCatalog;

    #[contractimpl]
    impl OkCatalog {
        pub fn put_offering(
            _env: Env,
            _registry: Address,
            _offering_id: String,
            _metadata: String,
        ) {
        }
    }
}

#[test]
fn update_offering_requires_developer_auth() {
    let env = Env::default();
    let (_admin, developer, offering_id, client) = setup_with_offering(&env);

    env.set_auths(&[]);
    let res = client.try_update_offering(
        &developer,
        &offering_id,
        &String::from_str(&env, "new metadata"),
    );
    assert!(res.is_err(), "update_offering must require auth");
}

#[test]
fn deprecate_offering_requires_auth() {
    let env = Env::default();
    let (admin, developer, offering_id, client) = setup_with_offering(&env);

    env.set_auths(&[]);
    assert!(client
        .try_deprecate_offering(&developer, &offering_id)
        .is_err());
    assert!(client.try_deprecate_offering(&admin, &offering_id).is_err());
}

#[test]
fn transfer_entrypoints_require_auth() {
    let env = Env::default();
    let (_admin, developer, offering_id, client) = setup_with_offering(&env);
    let new_developer = Address::generate(&env);
    env.ledger()
        .with_mut(|ledger| ledger.timestamp += callora_registry::admin::COOLDOWN_SECONDS);
    client.propose_offering_transfer(&developer, &offering_id, &new_developer);

    env.set_auths(&[]);
    assert!(client
        .try_propose_offering_transfer(&developer, &offering_id, &new_developer)
        .is_err());
    assert!(client
        .try_accept_offering_transfer(&new_developer, &offering_id)
        .is_err());
    assert!(client
        .try_cancel_offering_transfer(&developer, &offering_id)
        .is_err());
}

#[test]
fn lifecycle_auth_is_recorded_for_the_acting_address() {
    let env = Env::default();
    let (_admin, developer, offering_id, client) = setup_with_offering(&env);
    let new_developer = Address::generate(&env);
    env.ledger()
        .with_mut(|ledger| ledger.timestamp += callora_registry::admin::COOLDOWN_SECONDS);

    client.propose_offering_transfer(&developer, &offering_id, &new_developer);
    assert_eq!(env.auths()[0].0, developer);

    client.accept_offering_transfer(&new_developer, &offering_id);
    assert_eq!(env.auths()[0].0, new_developer);

    client.deprecate_offering(&new_developer, &offering_id);
    assert_eq!(env.auths()[0].0, new_developer);
}

#[test]
fn is_offering_deprecated_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _developer, offering_id, client) = setup_with_offering(&env);

    env.set_auths(&[]);
    assert!(!client.is_offering_deprecated(&offering_id));
}
