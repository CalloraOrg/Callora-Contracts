#![cfg(test)]
extern crate std;
use super::*;
use soroban_sdk::{testutils::{Address as _, Ledger as _}, Address, Env, Symbol};

#[test]
fn test_default_cooldown() {
    let env = Env::default();
    let cooldown = get_cooldown(&env);
    assert_eq!(cooldown, DEFAULT_COOLDOWN_SECONDS);
}

#[test]
fn test_set_and_get_cooldown() {
    let env = Env::default();
    env.mock_all_auths();
    let caller = Address::generate(&env);
    
    set_cooldown(&env, &caller, 3600);
    assert_eq!(get_cooldown(&env), 3600);
}

#[test]
fn test_check_and_record_upgrade_first_time() {
    let env = Env::default();
    env.mock_all_auths();
    let caller = Address::generate(&env);
    
    // First time should always succeed
    let res = check_and_record_upgrade(&env, &caller);
    assert!(res.is_ok());
    
    // Last upgrade time should be set to current ledger timestamp (0 in tests by default)
    let last_time = env.storage().instance().get::<_, u64>(&Symbol::new(&env, LAST_UPGRADE_TIME_KEY)).unwrap();
    assert_eq!(last_time, env.ledger().timestamp());
}

#[test]
fn test_check_and_record_upgrade_cooldown_not_elapsed() {
    let env = Env::default();
    env.mock_all_auths();
    let caller = Address::generate(&env);
    
    // Set timestamp to 100
    env.ledger().set_timestamp(100);
    
    // First time succeeds, records timestamp 100
    assert!(check_and_record_upgrade(&env, &caller).is_ok());
    
    // Set timestamp to 100 + DEFAULT_COOLDOWN_SECONDS - 1
    env.ledger().set_timestamp(100 + DEFAULT_COOLDOWN_SECONDS - 1);
    
    // Should fail because cooldown hasn't elapsed
    let res = check_and_record_upgrade(&env, &caller);
    assert_eq!(res, Err(UpgradeError::CooldownNotElapsed));
}

#[test]
fn test_check_and_record_upgrade_cooldown_elapsed() {
    let env = Env::default();
    env.mock_all_auths();
    let caller = Address::generate(&env);
    
    env.ledger().set_timestamp(100);
    
    // First time succeeds
    assert!(check_and_record_upgrade(&env, &caller).is_ok());
    
    // Set timestamp exactly to cooldown
    env.ledger().set_timestamp(100 + DEFAULT_COOLDOWN_SECONDS);
    
    // Should succeed now
    assert!(check_and_record_upgrade(&env, &caller).is_ok());
    
    // Last upgrade time should be updated to 100 + DEFAULT_COOLDOWN_SECONDS
    let last_time = env.storage().instance().get::<_, u64>(&Symbol::new(&env, LAST_UPGRADE_TIME_KEY)).unwrap();
    assert_eq!(last_time, 100 + DEFAULT_COOLDOWN_SECONDS);
}
