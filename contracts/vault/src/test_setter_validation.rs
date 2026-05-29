extern crate std;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env};
use super::*;
fn create_usdc(env: &Env, admin: &Address) -> (Address, token::StellarAssetClient) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (addr.clone(), token::StellarAssetClient::new(env, &addr))
}
fn create_vault(env: &Env) -> (Address, CalloraVaultClient) {
    let addr = env.register(CalloraVault, ());
    (addr, CalloraVaultClient::new(env, &addr))
}
fn setup(env: &Env) -> (Address, CalloraVaultClient, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let (vault_addr, client) = create_vault(env);
    let (usdc, _) = create_usdc(env, &admin);
    client.init(&admin, &usdc, &None, &None, &None, &None, &None);
    (vault_addr, client, usdc, admin)
}
#[test]
#[should_panic(expected = "settlement cannot be vault address")]
fn set_settlement_vault_address_panics() {
    let env = Env::default();
    let (vault_addr, client, _, admin) = setup(&env);
    client.set_settlement(&admin, &vault_addr);
}
#[test]
fn set_settlement_vault_address_try_returns_err() {
    let env = Env::default();
    let (vault_addr, client, _, admin) = setup(&env);
    assert!(client.try_set_settlement(&admin, &vault_addr).is_err());
}
#[test]
#[should_panic(expected = "settlement cannot be usdc_token address")]
fn set_settlement_usdc_address_panics() {
    let env = Env::default();
    let (_, client, usdc, admin) = setup(&env);
    client.set_settlement(&admin, &usdc);
}
#[test]
fn set_settlement_usdc_address_try_returns_err() {
    let env = Env::default();
    let (_, client, usdc, admin) = setup(&env);
    assert!(client.try_set_settlement(&admin, &usdc).is_err());
}
#[test]
#[should_panic(expected = "settlement cannot equal revenue_pool address")]
fn set_settlement_equals_revenue_pool_panics() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let pool = Address::generate(&env);
    client.set_revenue_pool(&admin, &Some(pool.clone()));
    client.set_settlement(&admin, &pool);
}
#[test]
fn set_settlement_equals_revenue_pool_try_returns_err() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let pool = Address::generate(&env);
    client.set_revenue_pool(&admin, &Some(pool.clone()));
    assert!(client.try_set_settlement(&admin, &pool).is_err());
}
#[test]
fn set_settlement_valid_address_succeeds() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let s = Address::generate(&env);
    client.set_settlement(&admin, &s);
    assert_eq!(client.get_settlement(), s);
}
#[test]
#[should_panic(expected = "revenue_pool cannot be vault address")]
fn set_revenue_pool_vault_address_panics() {
    let env = Env::default();
    let (vault_addr, client, _, admin) = setup(&env);
    client.set_revenue_pool(&admin, &Some(vault_addr));
}
#[test]
fn set_revenue_pool_vault_address_try_returns_err() {
    let env = Env::default();
    let (vault_addr, client, _, admin) = setup(&env);
    assert!(client.try_set_revenue_pool(&admin, &Some(vault_addr)).is_err());
}
#[test]
#[should_panic(expected = "revenue_pool cannot be usdc_token address")]
fn set_revenue_pool_usdc_address_panics() {
    let env = Env::default();
    let (_, client, usdc, admin) = setup(&env);
    client.set_revenue_pool(&admin, &Some(usdc));
}
#[test]
fn set_revenue_pool_usdc_address_try_returns_err() {
    let env = Env::default();
    let (_, client, usdc, admin) = setup(&env);
    assert!(client.try_set_revenue_pool(&admin, &Some(usdc)).is_err());
}
#[test]
#[should_panic(expected = "revenue_pool cannot equal settlement address")]
fn set_revenue_pool_equals_settlement_panics() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let s = Address::generate(&env);
    client.set_settlement(&admin, &s);
    client.set_revenue_pool(&admin, &Some(s));
}
#[test]
fn set_revenue_pool_equals_settlement_try_returns_err() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let s = Address::generate(&env);
    client.set_settlement(&admin, &s);
    assert!(client.try_set_revenue_pool(&admin, &Some(s)).is_err());
}
#[test]
fn set_revenue_pool_valid_address_succeeds() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let pool = Address::generate(&env);
    client.set_revenue_pool(&admin, &Some(pool.clone()));
    assert_eq!(client.get_revenue_pool(), Some(pool));
}
#[test]
fn set_revenue_pool_none_clears_pool() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let pool = Address::generate(&env);
    client.set_revenue_pool(&admin, &Some(pool));
    client.set_revenue_pool(&admin, &None);
    assert_eq!(client.get_revenue_pool(), None);
}