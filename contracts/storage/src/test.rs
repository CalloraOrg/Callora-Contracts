#![cfg(test)]
extern crate std;

use super::*;
use soroban_sdk::{testutils::Address as _, Env};

#[test]
fn test_storage_tiers() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(StorageContract, ());
    let client = StorageContractClient::new(&env, &contract_id);

    let admin = Address::generate(&env);
    let user = Address::generate(&env);

    // Test Instance Storage
    assert_eq!(client.init(&admin), ());
    let res = client.try_init(&admin);
    assert!(res.is_err());

    // Test Persistent Storage
    client.increment_balance(&admin, &user, &100);
    assert_eq!(client.get_balance(&user), 100);

    // Overflow check
    let res = client.try_increment_balance(&admin, &user, &i128::MAX);
    assert!(res.is_err());

    // Test Temporary Storage
    client.mark_request(&user, &42);
    assert_eq!(client.is_request_marked(&42), true);
    let res = client.try_mark_request(&user, &42);
    assert!(res.is_err());
}
