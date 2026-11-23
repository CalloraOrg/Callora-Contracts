use crate::{RevenuePool, RevenuePoolClient, RevenuePoolError};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
fn test_balance_uninitialized_panics() {
    let env = Env::default();
    let addr = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(&env, &addr);

    // Calling balance before init should return a typed error.
    assert_eq!(
        client.try_balance(),
        Err(Ok(RevenuePoolError::NotInitialized.into()))
    );
}

#[test]
fn test_receive_payment_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let usdc = Address::generate(&env);
    let attacker = Address::generate(&env);

    let addr = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(&env, &addr);

    client.init(&admin, &usdc);

    // Calling from an unauthorized address should return a typed error.
    assert_eq!(
        client.try_receive_payment(&attacker, &100, &false),
        Err(Ok(RevenuePoolError::Unauthorized.into()))
    );
}
