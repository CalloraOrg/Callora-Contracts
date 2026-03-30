extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{token, vec};

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract_address.address();
    let client = token::Client::new(env, &address);
    let admin_client = token::StellarAssetClient::new(env, &address);
    (address, client, admin_client)
}

fn create_pool(env: &Env) -> (Address, SettlementClient<'_>) {
    let address = env.register(Settlement, ());
    let client = SettlementClient::new(env, &address);
    (address, client)
}

fn fund_pool(usdc_admin_client: &token::StellarAssetClient, pool_address: &Address, amount: i128) {
    usdc_admin_client.mint(pool_address, &amount);
}

#[test]
fn init_success() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_pool_addr, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.balance(), 0);
}

#[test]
#[should_panic(expected = "revenue pool already initialized")]
fn init_double_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.init(&admin, &usdc);
}

#[test]
fn distribute_success() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1_000);
    client.distribute(&admin, &developer, &400);

    assert_eq!(usdc_client.balance(&pool_addr), 600);
    assert_eq!(usdc_client.balance(&developer), 400);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn distribute_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.distribute(&admin, &developer, &0);
}

#[test]
#[should_panic(expected = "insufficient USDC balance")]
fn distribute_excess_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 100);
    client.distribute(&admin, &developer, &101);
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin")]
fn distribute_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 500);
    client.distribute(&attacker, &developer, &100);
}

#[test]
fn set_admin_transfers_control() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 300);
    client.set_admin(&admin, &new_admin);
    assert_eq!(client.get_admin(), new_admin);

    client.distribute(&new_admin, &developer, &100);
    assert_eq!(usdc_client.balance(&developer), 100);
}

#[test]
fn receive_payment_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.receive_payment(&admin, &500, &true);
    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn batch_distribute_success() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1_000);

    let payments = vec![&env, (dev1.clone(), 300), (dev2.clone(), 500)];
    client.batch_distribute(&admin, &payments);

    assert_eq!(usdc_client.balance(&pool_addr), 200);
    assert_eq!(usdc_client.balance(&dev1), 300);
    assert_eq!(usdc_client.balance(&dev2), 500);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn batch_distribute_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc_address, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);

    let payments = vec![&env, (dev1.clone(), 300), (dev2.clone(), 0)];
    client.batch_distribute(&admin, &payments);
}

#[test]
#[should_panic(expected = "insufficient USDC balance")]
fn batch_distribute_insufficient_balance_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 400);

    let payments = vec![&env, (dev1.clone(), 300), (dev2.clone(), 200)];
    client.batch_distribute(&admin, &payments);
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin")]
fn batch_distribute_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1000);

    let payments = vec![&env, (dev1.clone(), 300)];
    client.batch_distribute(&attacker, &payments);
}
