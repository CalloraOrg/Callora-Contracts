#![cfg(test)]

use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, String};

#[test]
fn test_init_and_register() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, ErrorsContract);
    let client = ErrorsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    
    assert_eq!(client.init(&admin), ());
    
    assert_eq!(
        client.try_init(&admin).unwrap_err().unwrap(),
        Error::AlreadyInitialized
    );

    let desc = String::from_str(&env, "Insufficient Balance");
    assert_eq!(client.register_error(&admin, &101, &desc), ());
}

#[test]
fn test_unauthorized_registration() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, ErrorsContract);
    let client = ErrorsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fake_admin = Address::generate(&env);
    
    client.init(&admin);
    
    let desc = String::from_str(&env, "Unauthorized Action");
    
    assert_eq!(
        client.try_register_error(&fake_admin, &102, &desc).unwrap_err().unwrap(),
        Error::Unauthorized
    );
}

#[test]
fn test_log_error() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, ErrorsContract);
    let client = ErrorsContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    
    assert_eq!(client.log_error(&user, &101), ());
}

#[test]
fn test_overflow_protection() {
    let env = Env::default();
    env.mock_all_auths();
    
    let contract_id = env.register_contract(None, ErrorsContract);
    let client = ErrorsContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);
    
    assert_eq!(
        client.try_log_error(&user, &u32::MAX).unwrap_err().unwrap(),
        Error::Overflow
    );
}