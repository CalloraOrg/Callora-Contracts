#![cfg(test)]

use crate::{
    ContractError, FeeContract, FeeContractClient, INSTANCE_BUMP_AMOUNT, INSTANCE_BUMP_THRESHOLD,
    LEDGERS_PER_DAY,
};
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

fn setup() -> (Env, Address, FeeContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register_contract(None, FeeContract);
    let client = FeeContractClient::new(&env, &contract_id);
    (env, admin, client)
}

#[test]
fn test_init_bumps_instance_ttl() {
    let (env, admin, client) = setup();

    client.init(&admin);
    let ttl = env.storage().instance().get_ttl();
    assert_eq!(ttl, INSTANCE_BUMP_AMOUNT);
}

#[test]
fn test_set_fee_bumps_instance_ttl() {
    let (env, admin, client) = setup();
    client.init(&admin);

    // Advance sequence number to reduce instance TTL below threshold.
    let seq = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq + INSTANCE_BUMP_AMOUNT - INSTANCE_BUMP_THRESHOLD + 10);

    let ttl_before = env.storage().instance().get_ttl();
    assert!(ttl_before < INSTANCE_BUMP_THRESHOLD);

    client.set_fee(&admin, &500);

    let ttl_after = env.storage().instance().get_ttl();
    assert_eq!(ttl_after, INSTANCE_BUMP_AMOUNT);
}

#[test]
fn test_deposit_and_withdraw_bump_instance_ttl() {
    let (env, admin, client) = setup();
    client.init(&admin);
    let caller = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Advance sequence number to reduce instance TTL below threshold.
    let seq = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq + INSTANCE_BUMP_AMOUNT - INSTANCE_BUMP_THRESHOLD + 10);

    let ttl_before = env.storage().instance().get_ttl();
    assert!(ttl_before < INSTANCE_BUMP_THRESHOLD);

    client.deposit(&caller, &1_000);

    let ttl_after_deposit = env.storage().instance().get_ttl();
    assert_eq!(ttl_after_deposit, INSTANCE_BUMP_AMOUNT);

    // Advance again
    let seq2 = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq2 + INSTANCE_BUMP_AMOUNT - INSTANCE_BUMP_THRESHOLD + 10);

    client.withdraw(&admin, &recipient, &300);

    let ttl_after_withdraw = env.storage().instance().get_ttl();
    assert_eq!(ttl_after_withdraw, INSTANCE_BUMP_AMOUNT);
}

#[test]
fn test_read_paths_bump_instance_ttl() {
    let (env, admin, client) = setup();
    client.init(&admin);
    let caller = Address::generate(&env);
    client.deposit(&caller, &1_000);

    // 1. get_admin bumps TTL
    let seq = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq + INSTANCE_BUMP_AMOUNT - INSTANCE_BUMP_THRESHOLD + 10);
    assert!(env.storage().instance().get_ttl() < INSTANCE_BUMP_THRESHOLD);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(env.storage().instance().get_ttl(), INSTANCE_BUMP_AMOUNT);

    // 2. get_fee_config bumps TTL
    let seq2 = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq2 + INSTANCE_BUMP_AMOUNT - INSTANCE_BUMP_THRESHOLD + 10);
    assert!(env.storage().instance().get_ttl() < INSTANCE_BUMP_THRESHOLD);
    let _ = client.get_fee_config();
    assert_eq!(env.storage().instance().get_ttl(), INSTANCE_BUMP_AMOUNT);

    // 3. get_accumulated bumps TTL
    let seq3 = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq3 + INSTANCE_BUMP_AMOUNT - INSTANCE_BUMP_THRESHOLD + 10);
    assert!(env.storage().instance().get_ttl() < INSTANCE_BUMP_THRESHOLD);
    assert_eq!(client.get_accumulated(), 1_000);
    assert_eq!(env.storage().instance().get_ttl(), INSTANCE_BUMP_AMOUNT);
}

#[test]
fn test_ttl_constants() {
    assert_eq!(LEDGERS_PER_DAY, 17_280);
    assert_eq!(INSTANCE_BUMP_THRESHOLD, 17_280 * 30);
    assert_eq!(INSTANCE_BUMP_AMOUNT, 17_280 * 60);
}

#[test]
fn test_isolation_between_contract_instances() {
    let env = Env::default();
    env.mock_all_auths();

    let admin1 = Address::generate(&env);
    let contract_id1 = env.register_contract(None, FeeContract);
    let client1 = FeeContractClient::new(&env, &contract_id1);

    let admin2 = Address::generate(&env);
    let contract_id2 = env.register_contract(None, FeeContract);
    let client2 = FeeContractClient::new(&env, &contract_id2);

    client1.init(&admin1);
    client2.init(&admin2);

    let user = Address::generate(&env);
    client1.deposit(&user, &500);

    assert_eq!(client1.get_accumulated(), 500);
    assert_eq!(client2.get_accumulated(), 0);
    assert_eq!(client1.get_admin(), admin1);
    assert_eq!(client2.get_admin(), admin2);
}
