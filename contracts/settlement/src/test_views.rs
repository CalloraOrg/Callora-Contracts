use crate::{CalloraSettlement, CalloraSettlementClient, SettlementError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Error, InvokeError};

fn is_not_initialized<V, CE: Into<Error>, E: Into<Error>>(
    result: Result<Result<V, CE>, Result<E, InvokeError>>,
) -> bool {
    let expected = SettlementError::NotInitialized as u32;
    match result {
        Err(Ok(e)) => e.into().get_code() == expected,
        _ => false,
    }
}

#[test]
fn test_get_admin_uninitialized() {
    let env = Env::default();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    assert!(is_not_initialized(client.try_get_admin()));
}

#[test]
fn test_get_vault_uninitialized() {
    let env = Env::default();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    assert!(is_not_initialized(client.try_get_vault()));
}

#[test]
fn test_get_global_pool_uninitialized() {
    let env = Env::default();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    assert!(is_not_initialized(client.try_get_global_pool()));
}

#[test]
fn test_get_developer_balance_uninitialized() {
    let env = Env::default();
    let dev = Address::generate(&env);
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    assert!(is_not_initialized(
        client.try_get_developer_balance(&dev)
    ));
}

#[test]
fn test_get_all_developer_balances_uninitialized() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);
    let dummy = Address::generate(&env);

    assert!(is_not_initialized(
        client.try_get_all_developer_balances(&dummy)
    ));
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

/// `get_developer_balances_cursor` called before `init` must return
/// `NotInitialized` (it calls `get_admin` internally).
#[test]
fn test_get_developer_balances_cursor_uninitialized() {
    let env = Env::default();
    env.mock_all_auths();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);
    let dummy = Address::generate(&env);

    let result = client.try_get_developer_balances_cursor(&dummy, &None, &10u32);
    assert!(
        is_not_initialized(result),
        "expected NotInitialized before init"
    );
}
