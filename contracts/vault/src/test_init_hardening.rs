use crate::{CalloraVault, CalloraVaultClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
#[should_panic(expected = "vault already initialized")]
fn test_double_initialization_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let usdc = Address::generate(&env);
    env.mock_all_auths();

    let addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &addr);

    // First init
    client.init(&owner, &usdc, &Some(0), &None, &None, &None, &None);
    // Second init should panic
    client.init(&owner, &usdc, &Some(0), &None, &None, &None, &None);
}

#[test]
#[should_panic(expected = "usdc_token cannot be vault address")]
fn test_init_usdc_self_address_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    env.mock_all_auths();

    let addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &addr);

    // Passing vault's own address as USDC token
    client.init(&owner, &addr, &Some(0), &None, &None, &None, &None);
}

#[test]
#[should_panic(expected = "revenue_pool cannot be vault address")]
fn test_init_revenue_pool_self_address_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let usdc = Address::generate(&env);
    env.mock_all_auths();

    let addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &addr);

    // Passing vault's own address as Revenue Pool
    client.init(
        &owner,
        &usdc,
        &Some(0),
        &None,
        &None,
        &Some(addr.clone()),
        &None,
    );
}

#[test]
#[should_panic(expected = "min_deposit must be non-negative")]
fn test_init_negative_min_deposit_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let usdc = Address::generate(&env);
    env.mock_all_auths();

    let addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &addr);

    client.init(&owner, &usdc, &Some(0), &None, &Some(-10), &None, &None);
}

#[test]
#[should_panic(expected = "max_deduct must be positive")]
fn test_init_zero_max_deduct_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let usdc = Address::generate(&env);
    env.mock_all_auths();

    let addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &addr);

    client.init(&owner, &usdc, &Some(0), &None, &None, &None, &Some(0));
}

#[test]
#[should_panic(expected = "max_deduct must be positive")]
fn test_init_negative_max_deduct_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let usdc = Address::generate(&env);
    env.mock_all_auths();

    let addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &addr);

    client.init(&owner, &usdc, &Some(0), &None, &None, &None, &Some(-50));
}

#[test]
#[should_panic(expected = "min_deposit cannot exceed max_deduct")]
fn test_init_min_deposit_exceeds_max_deduct_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let usdc = Address::generate(&env);
    env.mock_all_auths();

    let addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &addr);

    client.init(&owner, &usdc, &Some(0), &None, &Some(100), &None, &Some(50));
}

#[test]
fn test_init_validates_successfully() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let usdc = Address::generate(&env);
    let pool = Address::generate(&env);
    env.mock_all_auths();

    let addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &addr);

    // Test valid initialization with valid parameters
    client.init(
        &owner,
        &usdc,
        &Some(0),
        &None,
        &Some(10),
        &Some(pool.clone()),
        &Some(50),
    );

    assert_eq!(client.get_admin(), owner);
}
