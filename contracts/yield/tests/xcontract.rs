//! Cross-contract call safety tests for yield deposits (`deposit_yield`).
//!
//! Verifies that when the USDC token callee reverts or panics during
//! `RevenuePool::deposit_yield`, the revenue pool does not persist partial
//! state (cumulative yield metric, balances, or `yield_deposited` events).
//!
//! ## Visible API
//!
//! No new public entrypoints are introduced. These tests exercise the existing
//! `deposit_yield` path, which invokes `token::Client::transfer` on the
//! configured USDC contract.

extern crate std;

use callora_revenue_pool::{RevenuePool, RevenuePoolClient};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::token;
use soroban_sdk::{contract, contractimpl, Address, Env, Map, Symbol, TryFromVal};

// ---------------------------------------------------------------------------
// Mock failing SEP-41 token
// ---------------------------------------------------------------------------

#[contract]
pub struct MockFailingToken;

#[contractimpl]
impl MockFailingToken {
    pub fn init(env: Env, admin: Address) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "admin"), &admin);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "fail_transfer"), &false);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "fail_after"), &0u32);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "transfer_count"), &0u32);
        let balances: Map<Address, i128> = Map::new(&env);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "balances"), &balances);
    }

    pub fn set_fail_transfer(env: Env, caller: Address, fail: bool) {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "admin"))
            .unwrap();
        assert_eq!(caller, admin, "unauthorized");
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "fail_transfer"), &fail);
    }

    pub fn set_fail_after(env: Env, caller: Address, count: u32) {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "admin"))
            .unwrap();
        assert_eq!(caller, admin, "unauthorized");
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "fail_after"), &count);
    }

    pub fn mint(env: Env, admin: Address, to: Address, amount: i128) {
        admin.require_auth();
        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "admin"))
            .unwrap();
        assert_eq!(admin, stored_admin, "unauthorized");
        let mut balances: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "balances"))
            .unwrap();
        let balance = balances.get(to.clone()).unwrap_or(0);
        balances.set(to, balance + amount);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "balances"), &balances);
    }

    pub fn transfer(env: Env, from: Address, to: Address, amount: i128) {
        from.require_auth();

        let fail_transfer: bool = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "fail_transfer"))
            .unwrap_or(false);
        let fail_after: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "fail_after"))
            .unwrap_or(0);
        let mut transfer_count: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "transfer_count"))
            .unwrap_or(0);

        transfer_count += 1;
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "transfer_count"), &transfer_count);

        if fail_transfer || (fail_after > 0 && transfer_count > fail_after) {
            panic!("mock token transfer failed intentionally");
        }

        let mut balances: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "balances"))
            .unwrap();
        let from_balance = balances.get(from.clone()).unwrap_or(0);
        let to_balance = balances.get(to.clone()).unwrap_or(0);

        assert!(from_balance >= amount, "insufficient balance");
        balances.set(from, from_balance - amount);
        balances.set(to, to_balance + amount);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "balances"), &balances);
    }

    pub fn balance(env: Env, id: Address) -> i128 {
        let balances: Map<Address, i128> = env
            .storage()
            .instance()
            .get(&Symbol::new(&env, "balances"))
            .unwrap();
        balances.get(id).unwrap_or(0)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_pool(env: &Env) -> (Address, RevenuePoolClient<'_>) {
    let address = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &address);
    (address, client)
}

fn create_mock_token<'a>(
    env: &'a Env,
    admin: &'a Address,
) -> (Address, MockFailingTokenClient<'a>) {
    let address = env.register(MockFailingToken, ());
    let client = MockFailingTokenClient::new(env, &address);
    client.init(admin);
    (address, client)
}

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

fn count_yield_deposited_events(env: &Env) -> u32 {
    let events = env.events().all();
    let mut count = 0u32;
    for i in 0..events.len() {
        let e = events.get(i).unwrap();
        if let Ok(topic) = Symbol::try_from_val(env, &e.1.get(0).unwrap()) {
            if topic == Symbol::new(env, "yield_deposited") {
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Cross-contract call safety tests for deposit_yield
// ---------------------------------------------------------------------------

#[test]
fn deposit_yield_atomic_on_transfer_panic() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);
    token_client.mint(&admin, &admin, &1_000);
    token_client.set_fail_transfer(&admin, &true);

    let source = Symbol::new(&env, "fees");
    let result = pool_client.try_deposit_yield(&admin, &400, &source);
    assert!(
        result.is_err(),
        "deposit_yield should fail when token transfer panics"
    );

    assert_eq!(
        token_client.balance(&admin),
        1_000,
        "treasury balance unchanged after failed transfer"
    );
    assert_eq!(
        token_client.balance(&pool_addr),
        0,
        "pool received nothing after failed transfer"
    );
    assert_eq!(
        pool_client.get_cumulative_yield_deposited(),
        0,
        "cumulative yield metric must not update on callee failure"
    );
    assert_eq!(
        count_yield_deposited_events(&env),
        0,
        "no yield_deposited events emitted on failure"
    );
}

#[test]
fn deposit_yield_atomic_on_insufficient_balance_panic() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);
    // Treasury holds less than the deposit amount → transfer panics.
    token_client.mint(&admin, &admin, &100);

    let source = Symbol::new(&env, "fees");
    let result = pool_client.try_deposit_yield(&admin, &400, &source);
    assert!(
        result.is_err(),
        "deposit_yield should fail when treasury lacks funds"
    );

    assert_eq!(token_client.balance(&admin), 100);
    assert_eq!(token_client.balance(&pool_addr), 0);
    assert_eq!(pool_client.get_cumulative_yield_deposited(), 0);
    assert_eq!(count_yield_deposited_events(&env), 0);
}

#[test]
fn deposit_yield_succeeds_when_transfer_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client, token_admin) = create_usdc(&env, &admin);

    pool_client.init(&admin, &token_addr);
    token_admin.mint(&admin, &1_000);

    let source = Symbol::new(&env, "yield");
    let result = pool_client.try_deposit_yield(&admin, &400, &source);
    assert_eq!(
        result,
        Ok(Ok(())),
        "deposit_yield should succeed when token transfer succeeds"
    );

    assert_eq!(token_client.balance(&admin), 600);
    assert_eq!(token_client.balance(&pool_addr), 400);
    assert_eq!(pool_client.get_cumulative_yield_deposited(), 400);
}

#[test]
fn deposit_yield_accumulates_after_prior_success_then_recovers_from_panic() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);
    token_client.mint(&admin, &admin, &1_000);

    let fees = Symbol::new(&env, "fees");
    let yield_src = Symbol::new(&env, "yield");

    // First deposit succeeds.
    let ok = pool_client.try_deposit_yield(&admin, &250, &fees);
    assert_eq!(ok, Ok(Ok(())));
    assert_eq!(pool_client.get_cumulative_yield_deposited(), 250);
    assert_eq!(token_client.balance(&pool_addr), 250);

    // Second deposit panics in the token callee.
    token_client.set_fail_transfer(&admin, &true);
    let failed = pool_client.try_deposit_yield(&admin, &150, &yield_src);
    assert!(failed.is_err());
    assert_eq!(
        pool_client.get_cumulative_yield_deposited(),
        250,
        "prior cumulative yield must be preserved after failed deposit"
    );
    assert_eq!(token_client.balance(&pool_addr), 250);
    assert_eq!(token_client.balance(&admin), 750);

    // Recover: disable failure and deposit again.
    token_client.set_fail_transfer(&admin, &false);
    let recovered = pool_client.try_deposit_yield(&admin, &150, &yield_src);
    assert_eq!(recovered, Ok(Ok(())));
    assert_eq!(pool_client.get_cumulative_yield_deposited(), 400);
    assert_eq!(token_client.balance(&pool_addr), 400);
    assert_eq!(token_client.balance(&admin), 600);
}

#[test]
fn deposit_yield_rejects_zero_amount_without_token_call_side_effects() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);
    token_client.mint(&admin, &admin, &1_000);

    let source = Symbol::new(&env, "fees");
    let result = pool_client.try_deposit_yield(&admin, &0, &source);
    assert!(result.is_err(), "zero amount must be rejected");

    assert_eq!(token_client.balance(&admin), 1_000);
    assert_eq!(token_client.balance(&pool_addr), 0);
    assert_eq!(pool_client.get_cumulative_yield_deposited(), 0);
    assert_eq!(count_yield_deposited_events(&env), 0);
}
