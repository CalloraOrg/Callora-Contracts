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

/// `get_developer_balances_cursor` called before `init` must return
/// `NotInitialized` (it calls `get_admin` internally).
#[test]
fn get_developer_balances_cursor_before_init_returns_not_initialized() {
    let env = Env::default();
    let contract = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &contract);
    let admin = Address::generate(&env);
    assert!(is_not_initialized(
        client.try_get_developer_balances_cursor(&admin, &Address::zero(), &0, &10)
    ));
}

// ---------------------------------------------------------------------------
// version
// ---------------------------------------------------------------------------

#[test]
fn version_returns_semver_string() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let vault_addr = Address::generate(&env);
    let contract = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &contract);
    env.mock_all_auths();
    client.init(&admin, &vault_addr);
    let v = client.version();
    assert_eq!(
        v,
        soroban_sdk::String::from_str(&env, env!("CARGO_PKG_VERSION"))
    );
}

// ---------------------------------------------------------------------------
// current_checkpoint
// ---------------------------------------------------------------------------

#[test]
fn checkpoint_before_init_returns_none() {
    let env = Env::default();
    let contract = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &contract);
    assert!(client.try_current_checkpoint().unwrap().is_none());
}

#[test]
fn checkpoint_creates_snapshot() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let vault_addr = Address::generate(&env);
    let contract = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &contract);
    env.mock_all_auths();
    client.init(&admin, &vault_addr);

    assert!(client.try_current_checkpoint().unwrap().is_none());
    client.checkbox(&admin);
    let cp = client.current_checkpoint().unwrap();
    assert_eq!(cp.checkpoint_id, 1);
    assert_eq!(cp.total_pool_balance, 0);
    assert_eq!(cp.developer_count, 0);
}

#[test]
fn checkpoint_increments_id() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let vault_addr = Address::generate(&env);
    let contract = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &contract);
    env.mock_all_auths();
    client.init(&admin, &vault_addr);

    client.checkbox(&admin);
    let cp1 = client.current_checkpoint().unwrap();
    assert_eq!(cp1.checkpoint_id, 1);

    client.checkbox(&admin);
    let cp2 = client.current_checkpoint().unwrap();
    assert_eq!(cp2.checkpoint_id, 2);
}
