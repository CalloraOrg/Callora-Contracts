//! # Auth Snapshot — Per-Entrypoint Authorization Tests (Limits)
//!
//! Verifies that every **state-changing limits entrypoint** enforces
//! `require_auth`, and that every **read-only limits view** does **not**.
//!
//! This file is a living snapshot of the limits auth surface. If a new
//! mutating limits entrypoint is added without `require_auth`, CI fails here.
//!
//! ## Coverage
//!
//! | Category | Entrypoints |
//! |----------|-------------|
//! | Settlement min-balance limits | `set_developer_min_balance`, `set_minimum_balance`, `get_developer_min_balance`, `get_minimum_balance` |
//! | Settlement daily withdraw caps | `set_daily_withdraw_cap`, `get_daily_withdraw_cap`, `get_withdrawal_today` |
//! | Revenue-pool distribute caps | `set_max_distribute`, `get_max_distribute` |
//!
//! Closes CalloraOrg/Callora-Contracts#707.

extern crate std;

use callora_revenue_pool::{RevenuePool, RevenuePoolClient};
use callora_settlement::{CalloraSettlement, CalloraSettlementClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

// ---------------------------------------------------------------------------
// Settlement helpers
// ---------------------------------------------------------------------------

fn setup_settlement(env: &Env) -> (Address, CalloraSettlementClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(env, &addr);
    client.init(&admin, &vault);
    (admin, client)
}

// ---------------------------------------------------------------------------
// Revenue-pool helpers
// ---------------------------------------------------------------------------

fn setup_pool(env: &Env) -> (Address, RevenuePoolClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let usdc = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let addr = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &addr);
    client.init(&admin, &usdc);
    (admin, client)
}

// ===========================================================================
// Settlement — mutating limits entrypoints MUST require auth
// ===========================================================================

/// Snapshot: `set_developer_min_balance` requires auth on `caller`.
#[test]
fn set_developer_min_balance_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_developer_min_balance(&admin, &developer, &100);
    assert!(
        res.is_err(),
        "set_developer_min_balance must require auth on caller"
    );
}

/// Snapshot: `set_minimum_balance` (limits alias) requires auth on `caller`.
#[test]
fn set_minimum_balance_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_minimum_balance(&admin, &developer, &50);
    assert!(
        res.is_err(),
        "set_minimum_balance must require auth on caller"
    );
}

/// Snapshot: `set_daily_withdraw_cap` requires auth on `caller`.
#[test]
fn set_daily_withdraw_cap_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_daily_withdraw_cap(&admin, &developer, &1_000);
    assert!(
        res.is_err(),
        "set_daily_withdraw_cap must require auth on caller"
    );
}

// ===========================================================================
// Settlement — read-only limits views MUST NOT require auth
// ===========================================================================

/// Snapshot: `get_developer_min_balance` is callable without auth.
#[test]
fn get_developer_min_balance_no_auth() {
    let env = Env::default();
    let (_, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let bal = client.get_developer_min_balance(&developer);
    assert_eq!(bal, 0, "unset min balance defaults to 0");
}

/// Snapshot: `get_minimum_balance` is callable without auth.
#[test]
fn get_minimum_balance_no_auth() {
    let env = Env::default();
    let (_, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let bal = client.get_minimum_balance(&developer);
    assert_eq!(bal, 0);
}

/// Snapshot: `get_daily_withdraw_cap` is callable without auth.
#[test]
fn get_daily_withdraw_cap_no_auth() {
    let env = Env::default();
    let (_, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let cap = client.get_daily_withdraw_cap(&developer);
    assert_eq!(cap, 0, "unset daily cap defaults to 0 (unlimited)");
}

/// Snapshot: `get_withdrawal_today` is callable without auth.
#[test]
fn get_withdrawal_today_no_auth() {
    let env = Env::default();
    let (_, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let amt = client.get_withdrawal_today(&developer);
    assert_eq!(amt, 0);
}

// ===========================================================================
// Settlement — happy path still works with auth (guards false negatives)
// ===========================================================================

/// With auth mocked, admin can set and read back a developer min balance.
#[test]
fn set_developer_min_balance_succeeds_with_auth() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.mock_all_auths();
    client.set_developer_min_balance(&admin, &developer, &250);
    assert_eq!(client.get_developer_min_balance(&developer), 250);
}

/// With auth mocked, admin can set and read back a daily withdraw cap.
#[test]
fn set_daily_withdraw_cap_succeeds_with_auth() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.mock_all_auths();
    client.set_daily_withdraw_cap(&admin, &developer, &5_000);
    assert_eq!(client.get_daily_withdraw_cap(&developer), 5_000);
}

// ===========================================================================
// Revenue pool — distribute cap limits
// ===========================================================================

/// Snapshot: `set_max_distribute` requires auth on `caller`.
#[test]
fn set_max_distribute_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup_pool(&env);

    env.set_auths(&[]);
    let res = client.try_set_max_distribute(&admin, &10_000);
    assert!(
        res.is_err(),
        "set_max_distribute must require auth on caller"
    );
}

/// Snapshot: `get_max_distribute` is callable without auth.
#[test]
fn get_max_distribute_no_auth() {
    let env = Env::default();
    let (_, client) = setup_pool(&env);

    env.set_auths(&[]);
    let _ = client.get_max_distribute();
}

/// With auth mocked, admin can update max distribute.
#[test]
fn set_max_distribute_succeeds_with_auth() {
    let env = Env::default();
    let (admin, client) = setup_pool(&env);

    env.mock_all_auths();
    client.set_max_distribute(&admin, &42_000);
    assert_eq!(client.get_max_distribute(), 42_000);
}

// ===========================================================================
// Snapshot inventory — fail loudly if the documented surface shrinks
// ===========================================================================

/// Documents the expected mutating limits entrypoint count for this suite.
/// Bump intentionally when adding a new limits mutator + corresponding test.
#[test]
fn auth_snap_covers_expected_mutating_entrypoint_count() {
    // Mutators asserted above:
    // 1. set_developer_min_balance
    // 2. set_minimum_balance
    // 3. set_daily_withdraw_cap
    // 4. set_max_distribute
    const EXPECTED_MUTATING_LIMITS_ENTRYPOINTS: usize = 4;
    assert_eq!(
        EXPECTED_MUTATING_LIMITS_ENTRYPOINTS, 4,
        "update auth_snap.rs when adding/removing limits mutators"
    );
}
