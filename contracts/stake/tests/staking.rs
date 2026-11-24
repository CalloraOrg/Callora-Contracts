#![cfg(test)]

extern crate std;

use callora_stake::{
    CalloraStake, CalloraStakeClient, StakeError, StakeStorageKey, PERSISTENT_TTL_THRESHOLD,
};
use soroban_sdk::{
    testutils::{Address as _, Events, Ledger},
    token::{Client as TokenClient, StellarAssetClient},
    Address, Env, Symbol, TryFromVal,
};

fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let token_admin = Address::generate(&env);
    let token_id = env.register_stellar_asset_contract_v2(token_admin).address();

    let contract_id = env.register(CalloraStake, ());
    let client = CalloraStakeClient::new(&env, &contract_id);
    client.init(&admin, &token_id);

    (env, admin, token_id, contract_id)
}

fn mint(env: &Env, token_id: &Address, to: &Address, amount: i128) {
    StellarAssetClient::new(env, token_id).mint(to, &amount);
}

fn token_balance(env: &Env, token_id: &Address, addr: &Address) -> i128 {
    TokenClient::new(env, token_id).balance(addr)
}

#[test]
fn init_sets_token_and_rejects_reinit() {
    let (env, admin, token_id, contract_id) = setup();
    let client = CalloraStakeClient::new(&env, &contract_id);

    assert_eq!(client.get_token(), token_id);
    assert_eq!(client.total_staked(), 0);

    let err = client.try_init(&admin, &token_id).unwrap_err().unwrap();
    assert_eq!(err, StakeError::AlreadyInitialized);
}

#[test]
fn uninitialized_calls_fail_with_not_initialized() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(CalloraStake, ());
    let client = CalloraStakeClient::new(&env, &contract_id);
    let staker = Address::generate(&env);

    assert_eq!(
        client.try_get_token().unwrap_err().unwrap(),
        StakeError::NotInitialized
    );
    assert_eq!(
        client.try_stake(&staker, &100).unwrap_err().unwrap(),
        StakeError::NotInitialized
    );
    assert_eq!(
        client.try_unstake(&staker, &100).unwrap_err().unwrap(),
        StakeError::NotInitialized
    );
    assert_eq!(client.get_stake(&staker), 0);
    assert_eq!(client.total_staked(), 0);
}

#[test]
fn happy_path_stake_partial_and_full_unstake_with_events() {
    let (env, _admin, token_id, contract_id) = setup();
    let client = CalloraStakeClient::new(&env, &contract_id);
    let staker = Address::generate(&env);

    mint(&env, &token_id, &staker, 1_000);

    // Stake 600
    let bal = client.stake(&staker, &600);
    assert_eq!(bal, 600);
    assert_eq!(client.get_stake(&staker), 600);
    assert_eq!(client.total_staked(), 600);
    assert_eq!(token_balance(&env, &token_id, &staker), 400);
    assert_eq!(token_balance(&env, &token_id, &contract_id), 600);

    // Verify staked event with versioned topic shape
    let events = env.events().all();
    let last = events.last().unwrap();
    assert_eq!(last.0, contract_id);
    let topic0 = Symbol::try_from_val(&env, &last.1.get(0).unwrap()).unwrap();
    let topic1 = Symbol::try_from_val(&env, &last.1.get(1).unwrap()).unwrap();
    let topic2 = Address::try_from_val(&env, &last.1.get(2).unwrap()).unwrap();
    assert_eq!(topic0, Symbol::new(&env, "staked"));
    assert_eq!(topic1, Symbol::new(&env, "callora_v1"));
    assert_eq!(topic2, staker);

    // Partial unstake 250
    let rem = client.unstake(&staker, &250);
    assert_eq!(rem, 350);
    assert_eq!(client.get_stake(&staker), 350);
    assert_eq!(client.total_staked(), 350);
    assert_eq!(token_balance(&env, &token_id, &staker), 650);
    assert_eq!(token_balance(&env, &token_id, &contract_id), 350);

    // Full unstake remaining 350 -> removes persistent entry
    let rem = client.unstake(&staker, &350);
    assert_eq!(rem, 0);
    assert_eq!(client.get_stake(&staker), 0);
    assert_eq!(client.total_staked(), 0);
    assert_eq!(token_balance(&env, &token_id, &staker), 1_000);
    assert_eq!(token_balance(&env, &token_id, &contract_id), 0);

    // Verify persistent entry was removed when balance reached 0
    env.as_contract(&contract_id, || {
        assert!(!env
            .storage()
            .persistent()
            .has(&StakeStorageKey::StakerBalance(staker.clone())));
    });

    // Verify unstaked event with versioned topic shape
    let events = env.events().all();
    let last = events.last().unwrap();
    assert_eq!(last.0, contract_id);
    let topic0 = Symbol::try_from_val(&env, &last.1.get(0).unwrap()).unwrap();
    let topic1 = Symbol::try_from_val(&env, &last.1.get(1).unwrap()).unwrap();
    let topic2 = Address::try_from_val(&env, &last.1.get(2).unwrap()).unwrap();
    assert_eq!(topic0, Symbol::new(&env, "unstaked"));
    assert_eq!(topic1, Symbol::new(&env, "callora_v1"));
    assert_eq!(topic2, staker);
}

#[test]
fn multiple_stakers_and_conservation() {
    let (env, _admin, token_id, contract_id) = setup();
    let client = CalloraStakeClient::new(&env, &contract_id);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    let carol = Address::generate(&env);

    mint(&env, &token_id, &alice, 1_000);
    mint(&env, &token_id, &bob, 2_000);
    mint(&env, &token_id, &carol, 500);

    client.stake(&alice, &400);
    assert_eq!(
        client.total_staked(),
        token_balance(&env, &token_id, &contract_id)
    );

    client.stake(&bob, &1_500);
    assert_eq!(
        client.total_staked(),
        token_balance(&env, &token_id, &contract_id)
    );

    // Alice unstakes before Carol stakes
    client.unstake(&alice, &100);
    assert_eq!(client.get_stake(&alice), 300);
    assert_eq!(client.get_stake(&bob), 1_500);
    assert_eq!(client.total_staked(), 1_800);
    assert_eq!(token_balance(&env, &token_id, &contract_id), 1_800);

    // Carol stakes, then Bob unstakes completely
    client.stake(&carol, &500);
    client.unstake(&bob, &1_500);

    assert_eq!(client.get_stake(&alice), 300);
    assert_eq!(client.get_stake(&bob), 0);
    assert_eq!(client.get_stake(&carol), 500);
    assert_eq!(client.total_staked(), 800);
    assert_eq!(token_balance(&env, &token_id, &contract_id), 800);
}

#[test]
fn repeated_stake_and_unstake_to_zero() {
    let (env, _admin, token_id, contract_id) = setup();
    let client = CalloraStakeClient::new(&env, &contract_id);
    let staker = Address::generate(&env);

    mint(&env, &token_id, &staker, 500);

    for _ in 0..3 {
        assert_eq!(client.stake(&staker, &200), 200);
        assert_eq!(client.stake(&staker, &300), 500);
        assert_eq!(client.unstake(&staker, &500), 0);
        assert_eq!(client.get_stake(&staker), 0);
        assert_eq!(client.total_staked(), 0);
        assert_eq!(token_balance(&env, &token_id, &staker), 500);
        assert_eq!(token_balance(&env, &token_id, &contract_id), 0);
    }
}

#[test]
fn invalid_amounts_and_over_withdraw_fail_without_state_change() {
    let (env, _admin, token_id, contract_id) = setup();
    let client = CalloraStakeClient::new(&env, &contract_id);
    let staker = Address::generate(&env);

    mint(&env, &token_id, &staker, 1_000);

    // Stake 0 and negative
    assert_eq!(
        client.try_stake(&staker, &0).unwrap_err().unwrap(),
        StakeError::InvalidAmount
    );
    assert_eq!(
        client.try_stake(&staker, &-50).unwrap_err().unwrap(),
        StakeError::InvalidAmount
    );

    // Unstake 0 and negative
    assert_eq!(
        client.try_unstake(&staker, &0).unwrap_err().unwrap(),
        StakeError::InvalidAmount
    );
    assert_eq!(
        client.try_unstake(&staker, &-10).unwrap_err().unwrap(),
        StakeError::InvalidAmount
    );

    // Unstake when balance is 0
    assert_eq!(
        client.try_unstake(&staker, &1).unwrap_err().unwrap(),
        StakeError::InsufficientStake
    );

    // Stake 300, then try to unstake 301
    client.stake(&staker, &300);
    assert_eq!(
        client.try_unstake(&staker, &301).unwrap_err().unwrap(),
        StakeError::InsufficientStake
    );

    // State unchanged after failed over-withdraw
    assert_eq!(client.get_stake(&staker), 300);
    assert_eq!(client.total_staked(), 300);
    assert_eq!(token_balance(&env, &token_id, &contract_id), 300);
    assert_eq!(token_balance(&env, &token_id, &staker), 700);
}

#[test]
fn overflow_adjacent_amounts_are_handled_safely() {
    let (env, _admin, token_id, contract_id) = setup();
    let client = CalloraStakeClient::new(&env, &contract_id);

    let alice = Address::generate(&env);
    let bob = Address::generate(&env);

    mint(&env, &token_id, &alice, i128::MAX);
    client.stake(&alice, &i128::MAX);
    assert_eq!(client.get_stake(&alice), i128::MAX);
    assert_eq!(client.total_staked(), i128::MAX);

    // Any additional stake by Alice or Bob overflows checked_add
    mint(&env, &token_id, &bob, 1);
    assert_eq!(
        client.try_stake(&bob, &1).unwrap_err().unwrap(),
        StakeError::Overflow
    );
    assert_eq!(client.get_stake(&bob), 0);
    assert_eq!(client.total_staked(), i128::MAX);

    // Alice can unstake i128::MAX cleanly back to 0
    assert_eq!(client.unstake(&alice, &i128::MAX), 0);
    assert_eq!(client.get_stake(&alice), 0);
    assert_eq!(client.total_staked(), 0);
}

#[test]
#[should_panic]
fn unauthorized_stake_panics_without_auth() {
    let (env, _admin, token_id, contract_id) = setup();
    let client = CalloraStakeClient::new(&env, &contract_id);
    let staker = Address::generate(&env);

    mint(&env, &token_id, &staker, 500);

    // Clear mocked auths so require_auth() rejects
    env.set_auths(&[]);
    client.stake(&staker, &100);
}

#[test]
#[should_panic]
fn unauthorized_unstake_panics_without_auth() {
    let (env, _admin, token_id, contract_id) = setup();
    let client = CalloraStakeClient::new(&env, &contract_id);
    let staker = Address::generate(&env);

    mint(&env, &token_id, &staker, 500);
    client.stake(&staker, &200);

    // Clear mocked auths so require_auth() rejects
    env.set_auths(&[]);
    client.unstake(&staker, &100);
}

#[test]
fn ttl_survives_ledger_advance_past_threshold() {
    let (env, _admin, token_id, contract_id) = setup();
    let client = CalloraStakeClient::new(&env, &contract_id);
    let staker = Address::generate(&env);

    mint(&env, &token_id, &staker, 1_000);
    client.stake(&staker, &400);

    // Advance ledger past the persistent TTL threshold (30 days),
    // still within the 60-day extension target.
    let advance = PERSISTENT_TTL_THRESHOLD + 1_000;
    env.ledger().with_mut(|li| {
        li.sequence_number += advance;
    });

    // Views should bump TTL and still read the exact balance and total.
    assert_eq!(client.get_stake(&staker), 400);
    assert_eq!(client.total_staked(), 400);
    assert_eq!(client.get_token(), token_id);

    // Advance again by another threshold + 1_000 — proves the view bumped TTL!
    env.ledger().with_mut(|li| {
        li.sequence_number += advance;
    });

    assert_eq!(client.get_stake(&staker), 400);
    assert_eq!(client.unstake(&staker, &400), 0);
    assert_eq!(client.total_staked(), 0);
    assert_eq!(token_balance(&env, &token_id, &staker), 1_000);
}
