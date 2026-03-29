#![cfg(test)]

use crate::{CalloraSettlement, CalloraSettlementClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
#[should_panic(expected = "settlement contract not initialized")]
fn test_get_admin_uninitialized_panics() {
    let env = Env::default();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    client.get_admin();
}

#[test]
#[should_panic(expected = "settlement contract not initialized")]
fn test_get_vault_uninitialized_panics() {
    let env = Env::default();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    client.get_vault();
}

#[test]
#[should_panic(expected = "settlement contract not initialized")]
fn test_get_global_pool_uninitialized_panics() {
    let env = Env::default();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    client.get_global_pool();
}

#[test]
#[should_panic(expected = "settlement contract not initialized")]
fn test_get_developer_balance_uninitialized_panics() {
    let env = Env::default();
    let dev = Address::generate(&env);
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    client.get_developer_balance(&dev);
}

#[test]
#[should_panic(expected = "settlement contract not initialized")]
fn test_get_all_developer_balances_uninitialized_panics() {
    let env = Env::default();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    client.get_all_developer_balances();
}

#[test]
fn test_get_developer_balance_returns_zero_when_not_stored() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    let dev = Address::generate(&env);

    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    client.init(&admin, &vault);

    let balance = client.get_developer_balance(&dev);
    assert_eq!(balance, 0);
}
