//! Tests for the atomic (all-or-nothing) `batch_distribute` semantics.
//!
//! These cover the acceptance criteria for the atomic batch-distribution work:
//! empty batches, maximum batch size, duplicate recipients, invalid recipients,
//! mid-batch failure (insufficient balance -> full revert, no partial
//! distribution), and value conservation on success.

extern crate std;

use crate::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, Vec};

/// Build an initialized, funded distribute contract.
///
/// Returns `(admin, usdc_address, contract_address, client)`. The contract
/// holds `funding` USDC minted by the asset admin.
fn setup(
    env: &Env,
    funding: i128,
) -> (Address, Address, Address, DistributeClient) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(env, &contract_addr);

    client.init(&admin, &usdc_addr);

    let usdc = token::StellarAssetClient::new(env, &usdc_addr);
    usdc.mint(&contract_addr, &funding);

    (admin, usdc_addr, contract_addr, client)
}

/// Build parallel recipient/amount vectors from `(Address, i128)` leg tuples.
fn legs(env: &Env, list: &[(Address, i128)]) -> (Vec<Address>, Vec<i128>) {
    let mut recipients = Vec::new(env);
    let mut amounts = Vec::new(env);
    for (to, amount) in list {
        recipients.push_back(to.clone());
        amounts.push_back(*amount);
    }
    (recipients, amounts)
}

#[test]
fn empty_batch_is_rejected_without_mutation() {
    let env = Env::default();
    let (admin, usdc_addr, contract_addr, client) = setup(&env, 1000);
    let to = Address::generate(&env);
    let usdc = token::Client::new(&env, &usdc_addr);
    let before = usdc.balance(&contract_addr);

    let recipients = Vec::<Address>::new(&env);
    let amounts = Vec::<i128>::new(&env);
    let result = client.try_batch_distribute(&admin, &recipients, &amounts);
    assert!(
        result.is_err(),
        "empty batch must be rejected (fail-early, no mutation)"
    );

    assert_eq!(before, usdc.balance(&contract_addr), "no state change on empty batch");
    assert_eq!(usdc.balance(&to), 0, "nobody should be paid");
}

#[test]
fn leg_count_mismatch_is_rejected() {
    let env = Env::default();
    let (admin, _, _, client) = setup(&env, 1000);

    let mut recipients = Vec::new(&env);
    recipients.push_back(Address::generate(&env));
    let amounts = Vec::<i128>::new(&env);

    let result = client.try_batch_distribute(&admin, &recipients, &amounts);
    assert!(
        result.is_err(),
        "recipients.len() != amounts.len() must be rejected"
    );
}

#[test]
fn max_batch_size_is_enforced() {
    let env = Env::default();
    let (admin, _, _, client) = setup(&env, i128::MAX);
    let mut recipients = Vec::new(&env);
    let mut amounts = Vec::new(&env);
    // One beyond MAX_BATCH_SIZE
    for _ in 0..limits::MAX_BATCH_SIZE + 1 {
        recipients.push_back(Address::generate(&env));
        amounts.push_back(1);
    }
    let result = client.try_batch_distribute(&admin, &recipients, &amounts);
    assert!(
        result.is_err(),
        "batch exceeding MAX_BATCH_SIZE must be rejected"
    );
}

#[test]
fn duplicates_are_rejected_before_any_transfer() {
    let env = Env::default();
    let (admin, usdc_addr, contract_addr, client) = setup(&env, 1000);
    let to = Address::generate(&env);

    let (recipients, amounts) = legs(
        &env,
        &[(to.clone(), 100), (to.clone(), 200), (Address::generate(&env), 300)],
    );

    let usdc = token::Client::new(&env, &usdc_addr);
    let before = usdc.balance(&contract_addr);

    let result = client.try_batch_distribute(&admin, &recipients, &amounts);
    assert!(
        result.is_err(),
        "duplicate recipient in a batch must be rejected"
    );
    assert_eq!(before, usdc.balance(&contract_addr), "no partial transfer");
    assert_eq!(usdc.balance(&to), 0, "no recipient should be paid");
}

#[test]
fn invalid_recipient_contract_self_is_rejected() {
    let env = Env::default();
    let (admin, _, contract_addr, client) = setup(&env, 1000);
    let other = Address::generate(&env);

    let (recipients, amounts) = legs(
        &env,
        &[(contract_addr.clone(), 100), (other.clone(), 200)],
    );
    let result = client.try_batch_distribute(&admin, &recipients, &amounts);
    assert!(
        result.is_err(),
        "distribution to the contract itself must be rejected"
    );
}

#[test]
fn mid_batch_failure_reverts_entire_batch() {
    let env = Env::default();
    // Only enough balance for the first leg by itself; the total (250) exceeds
    // the funded 150 -> rejected in Phase 2 before any transfer.
    let (admin, usdc_addr, contract_addr, client) = setup(&env, 150);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let usdc = token::Client::new(&env, &usdc_addr);

    let (recipients, amounts) = legs(&env, &[(a.clone(), 150), (b.clone(), 100)]);
    let before = usdc.balance(&contract_addr);

    let result = client.try_batch_distribute(&admin, &recipients, &amounts);
    assert!(
        result.is_err(),
        "batch with total exceeding balance must be rejected"
    );
    assert_eq!(before, usdc.balance(&contract_addr), "atomic: nothing transferred on failure");
    assert_eq!(usdc.balance(&a), 0, "first leg not paid (reverted)");
    assert_eq!(usdc.balance(&b), 0, "second leg not paid (reverted)");
}

#[test]
fn successful_batch_conserves_total_value() {
    let env = Env::default();
    let (admin, usdc_addr, contract_addr, client) = setup(&env, 1000);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let c = Address::generate(&env);
    let usdc = token::Client::new(&env, &usdc_addr);

    let before = usdc.balance(&contract_addr);
    let sum = 100 + 250 + 650;

    let (recipients, amounts) = legs(
        &env,
        &[(a.clone(), 100), (b.clone(), 250), (c.clone(), 650)],
    );
    client.batch_distribute(&admin, &recipients, &amounts);

    // Value conservation: contract lost exactly `sum`, recipients gained exactly `sum`.
    assert_eq!(usdc.balance(&contract_addr), before - sum);
    assert_eq!(usdc.balance(&a), 100);
    assert_eq!(usdc.balance(&b), 250);
    assert_eq!(usdc.balance(&c), 650);
    assert_eq!(
        usdc.balance(&a) + usdc.balance(&b) + usdc.balance(&c),
        sum,
        "recipients collectively received the exact batch total"
    );
}
