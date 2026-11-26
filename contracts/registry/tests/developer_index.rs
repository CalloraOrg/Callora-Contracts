#![cfg(test)]

use callora_registry::{CalloraRegistry, CalloraRegistryClient, RegistryError};
use soroban_sdk::{testutils::Address as _, Address, Env, String};

fn setup() -> (Env, CalloraRegistryClient<'static>, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    // `init` only stores the catalog address; a plain address is enough here.
    let catalog = Address::generate(&env);
    let contract_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &contract_id);
    client.init(&admin, &catalog);
    (env, client, admin, catalog)
}

fn s(env: &Env, v: &str) -> String {
    String::from_str(env, v)
}

#[test]
fn no_offerings_returns_empty() {
    let (env, client, _admin, _catalog) = setup();
    let dev = Address::generate(&env);
    let page = client.get_offerings_by_developer(&dev, &0, &10);
    assert_eq!(page.len(), 0);
}

#[test]
fn developers_are_isolated() {
    let (env, client, admin, _catalog) = setup();
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    client.register_offering(&admin, &a, &s(&env, "off-a1"), &s(&env, "meta"));
    client.register_offering(&admin, &b, &s(&env, "off-b1"), &s(&env, "meta"));
    client.register_offering(&admin, &a, &s(&env, "off-a2"), &s(&env, "meta"));

    let pa = client.get_offerings_by_developer(&a, &0, &10);
    assert_eq!(pa.len(), 2);
    assert_eq!(pa.get(0).unwrap().offering_id, s(&env, "off-a1"));
    assert_eq!(pa.get(1).unwrap().offering_id, s(&env, "off-a2"));

    let pb = client.get_offerings_by_developer(&b, &0, &10);
    assert_eq!(pb.len(), 1);
    assert_eq!(pb.get(0).unwrap().offering_id, s(&env, "off-b1"));
}

#[test]
fn paging_is_stable() {
    let (env, client, admin, _catalog) = setup();
    let dev = Address::generate(&env);
    for i in 0..5u32 {
        let id = match i {
            0 => "p0",
            1 => "p1",
            2 => "p2",
            3 => "p3",
            _ => "p4",
        };
        client.register_offering(&admin, &dev, &s(&env, id), &s(&env, "m"));
    }
    let first = client.get_offerings_by_developer(&dev, &0, &2);
    assert_eq!(first.len(), 2);
    assert_eq!(first.get(0).unwrap().offering_id, s(&env, "p0"));
    let second = client.get_offerings_by_developer(&dev, &2, &2);
    assert_eq!(second.get(0).unwrap().offering_id, s(&env, "p2"));
    let last = client.get_offerings_by_developer(&dev, &4, &2);
    assert_eq!(last.len(), 1);
    assert_eq!(last.get(0).unwrap().offering_id, s(&env, "p4"));
}

#[test]
fn limit_is_clamped_and_zero_is_empty() {
    let (env, client, admin, _catalog) = setup();
    let dev = Address::generate(&env);
    client.register_offering(&admin, &dev, &s(&env, "c0"), &s(&env, "m"));
    client.register_offering(&admin, &dev, &s(&env, "c1"), &s(&env, "m"));
    assert_eq!(client.get_offerings_by_developer(&dev, &0, &0).len(), 0);
    assert_eq!(client.get_offerings_by_developer(&dev, &0, &10_000).len(), 2);
}

#[test]
fn start_past_end_is_empty() {
    let (env, client, admin, _catalog) = setup();
    let dev = Address::generate(&env);
    client.register_offering(&admin, &dev, &s(&env, "e0"), &s(&env, "m"));
    assert_eq!(client.get_offerings_by_developer(&dev, &5, &10).len(), 0);
}

#[test]
fn duplicate_registration_not_indexed_twice() {
    let (env, client, admin, _catalog) = setup();
    let dev = Address::generate(&env);
    client.register_offering(&admin, &dev, &s(&env, "dup"), &s(&env, "m"));
    let res = client.try_register_offering(&admin, &dev, &s(&env, "dup"), &s(&env, "m"));
    assert_eq!(res, Err(Ok(RegistryError::OfferingAlreadyRegistered)));
    assert_eq!(client.get_offerings_by_developer(&dev, &0, &10).len(), 1);
}

#[test]
fn invalid_id_not_indexed() {
    let (env, client, admin, _catalog) = setup();
    let dev = Address::generate(&env);
    let res = client.try_register_offering(&admin, &dev, &s(&env, "BAD ID"), &s(&env, "m"));
    assert_eq!(res, Err(Ok(RegistryError::InvalidOfferingId)));
    assert_eq!(client.get_offerings_by_developer(&dev, &0, &10).len(), 0);
}
