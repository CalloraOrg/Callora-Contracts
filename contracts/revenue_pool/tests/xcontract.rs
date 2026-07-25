extern crate std;

// Cross-contract call safety tests for `batch_distribute`.
//
// This module tests the behavior of `batch_distribute` when the USDC token
// contract (the callee) fails during cross-contract calls. It verifies that
// the batch_distribute operation is atomic - if any transfer fails (panics),
// the entire batch is rolled back with no partial transfers or events emitted.
//
// Tests cover:
// - Transfer panics on first, middle, and last payment leg
// - Transfer panics after N successful transfers (configurable via `fail_after`)
// - Atomicity: no balances changed, no events emitted on any failure
// - Success case with real USDC token (Stellar Asset Contract)
// - Typed error returns for empty and oversized batches
// - Validation errors: insufficient balance, duplicate recipient, exceeds max_distribute

use callora_revenue_pool::{RevenuePool, RevenuePoolClient, RevenuePoolError, MAX_BATCH_SIZE};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::token;
use soroban_sdk::{contract, contractimpl, Address, Env, Map, Symbol, TryFromVal, Vec};

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

// The #[contract] macro generates MockFailingTokenClient for us

// ---------------------------------------------------------------------------
// Test Helpers
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
    client.init(&admin.clone());
    (address, client)
}

// Helper to create a real USDC token (Stellar Asset Contract)
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

fn mint_mock_token(
    token_client: &MockFailingTokenClient,
    admin: &Address,
    to: &Address,
    amount: i128,
) {
    token_client.mint(admin, to, &amount);
}

fn fund_pool(usdc_admin_client: &token::StellarAssetClient, pool_address: &Address, amount: i128) {
    usdc_admin_client.mint(pool_address, &amount);
}

// ---------------------------------------------------------------------------
// Cross-contract call safety tests for batch_distribute
// ---------------------------------------------------------------------------

#[test]
fn batch_distribute_atomic_on_transfer_panic_first_leg() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);

    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);

    mint_mock_token(&token_client, &admin, &pool_addr, 1000);

    // Configure token to fail on first transfer
    token_client.set_fail_transfer(&admin, &true);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 300));
    payments.push_back((dev2.clone(), 200));

    let result = pool_client.try_batch_distribute(&admin, &payments);
    assert!(
        result.is_err(),
        "batch_distribute should fail when transfer panics"
    );

    assert_eq!(
        token_client.balance(&pool_addr),
        1000,
        "pool balance unchanged after failed transfer"
    );
    assert_eq!(token_client.balance(&dev1), 0, "dev1 received nothing");
    assert_eq!(token_client.balance(&dev2), 0, "dev2 received nothing");

    let events = env.events().all();
    let mut batch_count = 0u32;
    for i in 0..events.len() {
        let e = events.get(i).unwrap();
        if let Ok(topic) = Symbol::try_from_val(&env, &e.1.get(0).unwrap()) {
            if topic == Symbol::new(&env, "batch_distribute") {
                batch_count += 1;
            }
        }
    }
    assert_eq!(
        batch_count, 0,
        "no batch_distribute events emitted on failure"
    );
}

#[test]
fn batch_distribute_atomic_on_transfer_panic_second_leg() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let dev3 = Address::generate(&env);

    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);

    mint_mock_token(&token_client, &admin, &pool_addr, 1000);

    // Configure token to fail after 1 transfer (so second transfer fails)
    token_client.set_fail_after(&admin, &1);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 300));
    payments.push_back((dev2.clone(), 200));
    payments.push_back((dev3.clone(), 100));

    let result = pool_client.try_batch_distribute(&admin, &payments);
    assert!(
        result.is_err(),
        "batch_distribute should fail when second transfer panics"
    );

    assert_eq!(
        token_client.balance(&pool_addr),
        1000,
        "pool balance unchanged after failed transfer"
    );
    assert_eq!(
        token_client.balance(&dev1),
        0,
        "dev1 received nothing (atomic rollback)"
    );
    assert_eq!(token_client.balance(&dev3), 0, "dev3 received nothing");

    let events = env.events().all();
    let mut batch_count = 0u32;
    for i in 0..events.len() {
        let e = events.get(i).unwrap();
        if let Ok(topic) = Symbol::try_from_val(&env, &e.1.get(0).unwrap()) {
            if topic == Symbol::new(&env, "batch_distribute") {
                batch_count += 1;
            }
        }
    }
    assert_eq!(
        batch_count, 0,
        "no batch_distribute events emitted on partial failure"
    );
}

#[test]
fn batch_distribute_atomic_on_transfer_panic_middle_leg() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let dev3 = Address::generate(&env);
    let dev4 = Address::generate(&env);
    let dev5 = Address::generate(&env);

    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);

    mint_mock_token(&token_client, &admin, &pool_addr, 5000);

    // Configure token to fail after 2 transfers (so third transfer fails)
    token_client.set_fail_after(&admin, &2);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 500));
    payments.push_back((dev2.clone(), 500));
    payments.push_back((dev3.clone(), 500));
    payments.push_back((dev4.clone(), 500));
    payments.push_back((dev5.clone(), 500));

    let result = pool_client.try_batch_distribute(&admin, &payments);
    assert!(
        result.is_err(),
        "batch_distribute should fail when middle transfer panics"
    );

    assert_eq!(
        token_client.balance(&pool_addr),
        5000,
        "pool balance unchanged after failed transfer"
    );
    assert_eq!(
        token_client.balance(&dev1),
        0,
        "dev1 received nothing (atomic rollback)"
    );
    assert_eq!(token_client.balance(&dev5), 0, "dev5 received nothing");

    let events = env.events().all();
    let mut batch_count = 0u32;
    for i in 0..events.len() {
        let e = events.get(i).unwrap();
        if let Ok(topic) = Symbol::try_from_val(&env, &e.1.get(0).unwrap()) {
            if topic == Symbol::new(&env, "batch_distribute") {
                batch_count += 1;
            }
        }
    }
    assert_eq!(
        batch_count, 0,
        "no batch_distribute events emitted on partial failure"
    );
}

#[test]
fn batch_distribute_atomic_on_transfer_panic_last_leg() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let dev3 = Address::generate(&env);

    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);

    mint_mock_token(&token_client, &admin, &pool_addr, 3000);

    // Configure token to fail after 2 transfers (so third/last transfer fails)
    token_client.set_fail_after(&admin, &2);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 1000));
    payments.push_back((dev2.clone(), 1000));
    payments.push_back((dev3.clone(), 1000));

    let result = pool_client.try_batch_distribute(&admin, &payments);
    assert!(
        result.is_err(),
        "batch_distribute should fail when last transfer panics"
    );

    assert_eq!(
        token_client.balance(&pool_addr),
        3000,
        "pool balance unchanged after failed transfer"
    );
    assert_eq!(
        token_client.balance(&dev1),
        0,
        "dev1 received nothing (atomic rollback)"
    );
    assert_eq!(token_client.balance(&dev3), 0, "dev3 received nothing");

    let events = env.events().all();
    let mut batch_count = 0u32;
    for i in 0..events.len() {
        let e = events.get(i).unwrap();
        if let Ok(topic) = Symbol::try_from_val(&env, &e.1.get(0).unwrap()) {
            if topic == Symbol::new(&env, "batch_distribute") {
                batch_count += 1;
            }
        }
    }
    assert_eq!(
        batch_count, 0,
        "no batch_distribute events emitted on partial failure"
    );
}

#[test]
fn batch_distribute_succeeds_when_all_transfers_succeed() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);

    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client, token_admin) = create_usdc(&env, &admin);

    pool_client.init(&admin, &token_addr);
    fund_pool(&token_admin, &pool_addr, 1000);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 300));
    payments.push_back((dev2.clone(), 200));

    let result = pool_client.try_batch_distribute(&admin, &payments);
    assert_eq!(
        result,
        Ok(Ok(())),
        "batch_distribute should succeed when all transfers succeed"
    );

    // Balance checks verify the transfers happened correctly
    // Note: Event capture in integration tests has known limitations
    assert_eq!(token_client.balance(&pool_addr), 500);
    assert_eq!(token_client.balance(&dev1), 300);
    assert_eq!(token_client.balance(&dev2), 200);
}

#[test]
fn batch_distribute_returns_typed_error_on_empty_batch() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (_, pool_client) = create_pool(&env);
    let (token_addr, _token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);

    let payments: Vec<(Address, i128)> = Vec::new(&env);
    let result = pool_client.try_batch_distribute(&admin, &payments);
    assert_eq!(result, Err(Ok(RevenuePoolError::BatchEmpty)));
}

#[test]
fn batch_distribute_returns_typed_error_on_oversized_batch() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (_, pool_client) = create_pool(&env);
    let (token_addr, _token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    for _ in 0..=MAX_BATCH_SIZE {
        payments.push_back((Address::generate(&env), 1));
    }

    let result = pool_client.try_batch_distribute(&admin, &payments);
    assert_eq!(result, Err(Ok(RevenuePoolError::BatchTooLarge)));
}

#[test]
fn batch_distribute_atomic_on_insufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);

    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);

    mint_mock_token(&token_client, &admin, &pool_addr, 100);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 60));
    payments.push_back((dev2.clone(), 60)); // total 120 > balance 100

    let result = pool_client.try_batch_distribute(&admin, &payments);
    assert!(
        result.is_err(),
        "batch_distribute should fail with insufficient balance"
    );

    assert_eq!(token_client.balance(&pool_addr), 100);
    assert_eq!(token_client.balance(&dev1), 0);
    assert_eq!(token_client.balance(&dev2), 0);

    let events = env.events().all();
    let mut batch_count = 0u32;
    for i in 0..events.len() {
        let e = events.get(i).unwrap();
        if let Ok(topic) = Symbol::try_from_val(&env, &e.1.get(0).unwrap()) {
            if topic == Symbol::new(&env, "batch_distribute") {
                batch_count += 1;
            }
        }
    }
    assert_eq!(batch_count, 0);
}

#[test]
fn batch_distribute_atomic_on_duplicate_recipient() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);

    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);

    mint_mock_token(&token_client, &admin, &pool_addr, 1000);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 100));
    payments.push_back((dev1.clone(), 200)); // duplicate

    let result = pool_client.try_batch_distribute(&admin, &payments);
    assert!(
        result.is_err(),
        "batch_distribute should fail with duplicate recipient"
    );

    assert_eq!(token_client.balance(&pool_addr), 1000);
    assert_eq!(token_client.balance(&dev1), 0);

    let events = env.events().all();
    let mut batch_count = 0u32;
    for i in 0..events.len() {
        let e = events.get(i).unwrap();
        if let Ok(topic) = Symbol::try_from_val(&env, &e.1.get(0).unwrap()) {
            if topic == Symbol::new(&env, "batch_distribute") {
                batch_count += 1;
            }
        }
    }
    assert_eq!(batch_count, 0);
}

#[test]
fn batch_distribute_atomic_on_exceeds_max_distribute() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);

    let (pool_addr, pool_client) = create_pool(&env);
    let (token_addr, token_client) = create_mock_token(&env, &admin);

    pool_client.init(&admin, &token_addr);
    pool_client.set_max_distribute(&admin, &500);

    mint_mock_token(&token_client, &admin, &pool_addr, 1000);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 600)); // exceeds max_distribute of 500

    let result = pool_client.try_batch_distribute(&admin, &payments);
    assert!(
        result.is_err(),
        "batch_distribute should fail when exceeding max_distribute"
    );

    assert_eq!(token_client.balance(&pool_addr), 1000);
    assert_eq!(token_client.balance(&dev1), 0);

    let events = env.events().all();
    let mut batch_count = 0u32;
    for i in 0..events.len() {
        let e = events.get(i).unwrap();
        if let Ok(topic) = Symbol::try_from_val(&env, &e.1.get(0).unwrap()) {
            if topic == Symbol::new(&env, "batch_distribute") {
                batch_count += 1;
            }
        }
    }
    assert_eq!(batch_count, 0);
}
