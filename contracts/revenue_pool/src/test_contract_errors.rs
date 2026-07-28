extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token;
use soroban_sdk::{Address, Env, String, Symbol, Vec};

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract.address();
    (
        address.clone(),
        token::Client::new(env, &address),
        token::StellarAssetClient::new(env, &address),
    )
}

fn setup_pool(
    env: &Env,
) -> (
    Address,
    Address,
    RevenuePoolClient<'_>,
    token::Client<'_>,
    token::StellarAssetClient<'_>,
) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let pool = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &pool);
    let (usdc, usdc_client, usdc_admin) = create_usdc(env, &admin);
    client.init(&admin, &usdc);
    (admin, pool, client, usdc_client, usdc_admin)
}

#[test]
fn initialization_errors_are_typed() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pool = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(&env, &pool);
    let (usdc, _, _) = create_usdc(&env, &admin);

    assert_eq!(
        client.try_get_admin(),
        Err(Ok(RevenuePoolError::NotInitialized))
    );

    assert_eq!(
        client.try_init(&admin, &pool),
        Err(Ok(RevenuePoolError::InvalidUsdcToken))
    );
    assert_eq!(
        client.try_init(&admin, &admin),
        Err(Ok(RevenuePoolError::InvalidUsdcToken))
    );

    client.init(&admin, &usdc);
    assert_eq!(
        client.try_init(&admin, &usdc),
        Err(Ok(RevenuePoolError::AlreadyInitialized))
    );
}

#[test]
fn admin_transfer_errors_are_typed() {
    let env = Env::default();
    let (admin, _, client, _, _) = setup_pool(&env);
    let attacker = Address::generate(&env);
    let candidate = Address::generate(&env);

    assert_eq!(
        client.try_set_admin(&attacker, &candidate),
        Err(Ok(RevenuePoolError::Unauthorized))
    );
    assert_eq!(
        client.try_claim_admin(&candidate),
        Err(Ok(RevenuePoolError::NoAdminTransferPending))
    );

    client.set_admin(&admin, &candidate);
    assert_eq!(
        client.try_claim_admin(&attacker),
        Err(Ok(RevenuePoolError::Unauthorized))
    );
}

#[test]
fn pause_state_errors_are_typed() {
    let env = Env::default();
    let (admin, _, client, _, _) = setup_pool(&env);
    let attacker = Address::generate(&env);
    let recipient = Address::generate(&env);

    assert_eq!(
        client.try_clear_pause_guardian(&admin),
        Err(Ok(RevenuePoolError::NoPauseGuardian))
    );
    assert_eq!(
        client.try_unpause(&admin),
        Err(Ok(RevenuePoolError::NotPaused))
    );
    assert_eq!(
        client.try_pause(&attacker),
        Err(Ok(RevenuePoolError::Unauthorized))
    );

    client.pause(&admin);
    assert_eq!(
        client.try_pause(&admin),
        Err(Ok(RevenuePoolError::AlreadyPaused))
    );
    assert_eq!(
        client.try_distribute(&admin, &recipient, &1),
        Err(Ok(RevenuePoolError::Paused))
    );
}

#[test]
fn single_distribution_errors_are_typed() {
    let env = Env::default();
    let (admin, pool, client, _, _) = setup_pool(&env);
    let recipient = Address::generate(&env);

    assert_eq!(
        client.try_distribute(&admin, &recipient, &0),
        Err(Ok(RevenuePoolError::AmountNotPositive))
    );

    client.set_max_distribute(&admin, &10);
    assert_eq!(
        client.try_distribute(&admin, &recipient, &11),
        Err(Ok(RevenuePoolError::AmountExceedsMaxDistribute))
    );
    assert_eq!(
        client.try_distribute(&admin, &pool, &1),
        Err(Ok(RevenuePoolError::InvalidRecipient))
    );
    assert_eq!(
        client.try_distribute(&admin, &recipient, &1),
        Err(Ok(RevenuePoolError::InsufficientBalance))
    );
}

#[test]
fn batch_distribution_errors_are_typed_and_atomic() {
    let env = Env::default();
    let (admin, pool, client, usdc, usdc_admin) = setup_pool(&env);
    let recipient = Address::generate(&env);

    let empty: Vec<(Address, i128)> = Vec::new(&env);
    assert_eq!(
        client.try_batch_distribute(&admin, &empty),
        Err(Ok(RevenuePoolError::BatchEmpty))
    );

    let mut oversized: Vec<(Address, i128)> = Vec::new(&env);
    for _ in 0..=MAX_BATCH_SIZE {
        oversized.push_back((Address::generate(&env), 1));
    }
    assert_eq!(
        client.try_batch_distribute(&admin, &oversized),
        Err(Ok(RevenuePoolError::BatchTooLarge))
    );

    let mut duplicate: Vec<(Address, i128)> = Vec::new(&env);
    duplicate.push_back((recipient.clone(), 1));
    duplicate.push_back((recipient.clone(), 1));
    assert_eq!(
        client.try_batch_distribute(&admin, &duplicate),
        Err(Ok(RevenuePoolError::DuplicateRecipient))
    );

    let mut overflowing: Vec<(Address, i128)> = Vec::new(&env);
    overflowing.push_back((Address::generate(&env), i128::MAX));
    overflowing.push_back((Address::generate(&env), 1));
    assert_eq!(
        client.try_batch_distribute(&admin, &overflowing),
        Err(Ok(RevenuePoolError::Overflow))
    );

    usdc_admin.mint(&pool, &100);
    let recipient_balance = usdc.balance(&recipient);
    let mut invalid_late_leg: Vec<(Address, i128)> = Vec::new(&env);
    invalid_late_leg.push_back((recipient.clone(), 25));
    invalid_late_leg.push_back((pool.clone(), 25));
    assert_eq!(
        client.try_batch_distribute(&admin, &invalid_late_leg),
        Err(Ok(RevenuePoolError::InvalidRecipient))
    );
    assert_eq!(usdc.balance(&pool), 100);
    assert_eq!(usdc.balance(&recipient), recipient_balance);
}

#[test]
fn cap_and_broadcast_errors_are_typed() {
    let env = Env::default();
    let (admin, _, client, _, _) = setup_pool(&env);

    assert_eq!(
        client.try_set_max_distribute(&admin, &0),
        Err(Ok(RevenuePoolError::MaxDistributeNotPositive))
    );

    let empty = String::from_str(&env, "");
    assert_eq!(
        client.try_broadcast(&admin, &Severity::Info, &empty),
        Err(Ok(RevenuePoolError::MessageEmpty))
    );

    let long_text = "x".repeat((MAX_MESSAGE_LEN + 1) as usize);
    let long_message = String::from_str(&env, &long_text);
    assert_eq!(
        client.try_broadcast(&admin, &Severity::Info, &long_message),
        Err(Ok(RevenuePoolError::MessageTooLong))
    );
}

#[test]
fn emergency_drain_errors_are_typed() {
    let env = Env::default();
    env.ledger().set_timestamp(1_700_000_000);
    let (admin, pool, client, _, _) = setup_pool(&env);
    let treasury = Address::generate(&env);

    assert_eq!(
        client.try_execute_emergency_drain(&admin),
        Err(Ok(RevenuePoolError::NoPendingEmergencyDrain))
    );
    assert_eq!(
        client.try_propose_emergency_drain(&admin, &treasury, &0),
        Err(Ok(RevenuePoolError::AmountNotPositive))
    );
    assert_eq!(
        client.try_propose_emergency_drain(&admin, &pool, &1),
        Err(Ok(RevenuePoolError::InvalidRecipient))
    );

    client.propose_emergency_drain(&admin, &treasury, &1);
    assert_eq!(
        client.try_execute_emergency_drain(&admin),
        Err(Ok(RevenuePoolError::TimelockNotExpired))
    );

    env.ledger()
        .set_timestamp(1_700_000_000 + EMERGENCY_DRAIN_TIMELOCK_SECONDS);
    assert_eq!(
        client.try_execute_emergency_drain(&admin),
        Err(Ok(RevenuePoolError::InsufficientBalance))
    );
}

#[test]
fn emergency_drain_timelock_overflow_is_typed() {
    let env = Env::default();
    env.ledger().set_timestamp(u64::MAX);
    let (admin, _, client, _, _) = setup_pool(&env);
    let treasury = Address::generate(&env);

    assert_eq!(
        client.try_propose_emergency_drain(&admin, &treasury, &1),
        Err(Ok(RevenuePoolError::Overflow))
    );
}

#[test]
fn cumulative_yield_overflow_is_typed() {
    let env = Env::default();
    let (admin, pool, client, _, usdc_admin) = setup_pool(&env);
    let source = Symbol::new(&env, "fees");

    env.as_contract(&pool, || {
        env.storage().instance().set(
            &Symbol::new(&env, CUMULATIVE_YIELD_DEPOSITED_KEY),
            &i128::MAX,
        );
    });
    usdc_admin.mint(&admin, &1);

    assert_eq!(
        client.try_deposit_yield(&admin, &1, &source),
        Err(Ok(RevenuePoolError::Overflow))
    );
}
