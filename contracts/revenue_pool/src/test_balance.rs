use crate::{RevenuePool, RevenuePoolClient};
use soroban_sdk::{testutils::Address as _, Address, Env};

#[test]
#[should_panic(expected = "revenue pool not initialized")]
fn test_balance_uninitialized_panics() {
    let env = Env::default();
    let addr = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(&env, &addr);

    // Calling balance before init should panic
    client.balance();
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin")]
fn test_receive_payment_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let usdc = Address::generate(&env);
    let attacker = Address::generate(&env);

    let addr = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(&env, &addr);

    client.init(&admin, &usdc);

    // Call from unauthorized address should panic
    client.receive_payment(&attacker, &100, &false);
}
