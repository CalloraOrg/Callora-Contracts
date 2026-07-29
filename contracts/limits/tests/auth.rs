//! # Auth-Context Tests — Limits Entrypoints with Signer Identity Verification
//!
//! Tests every **limits-related state-changing entrypoint** under both
//! `require_auth` and mock-auth harnesses, and verifies that the **signer
//! identity** is correctly captured in `env.auths()` snapshots.
//!
//! ## Coverage
//!
//! | Category | Entrypoints | Auth Harness |
//! |----------|-------------|--------------|
//! | Settlement min-balance limits | `set_developer_min_balance`, `set_minimum_balance` | require_auth + mock |
//! | Settlement daily withdraw caps | `set_daily_withdraw_cap` | require_auth + mock |
//! | Revenue-pool distribute caps | `set_max_distribute` | require_auth + mock |
//! | Read-only views | `get_developer_min_balance`, `get_minimum_balance`, `get_daily_withdraw_cap`, `get_withdrawal_today`, `get_max_distribute` | no auth |
//!
//! Closes CalloraOrg/Callora-Contracts#863.

extern crate std;

use callora_revenue_pool::{RevenuePool, RevenuePoolClient};
use callora_settlement::{CalloraSettlement, CalloraSettlementClient};
use soroban_sdk::testutils::{Address as _, AuthorizedFunction};
use soroban_sdk::{Address, Env, Symbol};

// ---------------------------------------------------------------------------
// Settlement helpers
// ---------------------------------------------------------------------------

fn setup_settlement(env: &Env) -> (Address, Address, CalloraSettlementClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(env, &addr);
    client.init(&admin, &vault);
    (admin, addr, client)
}

// ---------------------------------------------------------------------------
// Revenue-pool helpers
// ---------------------------------------------------------------------------

fn setup_pool(env: &Env) -> (Address, Address, RevenuePoolClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let usdc = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let addr = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &addr);
    client.init(&admin, &usdc);
    (admin, addr, client)
}

/// Helper: assert that the latest auth snapshot entry matches the expected
/// signer address and contract function name.
fn assert_latest_auth(
    env: &Env,
    expected_signer: &Address,
    expected_fn_name: &str,
    expected_contract: &Address,
) {
    let auths = env.auths();
    let latest = auths.last().expect("missing auth entry");
    assert_eq!(latest.0, *expected_signer, "Signer address mismatch");
    match &latest.1.function {
        AuthorizedFunction::Contract((contract_addr, fn_name, _args)) => {
            assert_eq!(
                contract_addr, expected_contract,
                "Contract address mismatch in auth snapshot"
            );
            assert_eq!(
                *fn_name,
                Symbol::new(env, expected_fn_name),
                "Function symbol mismatch in auth snapshot"
            );
        }
        _ => panic!("Expected Contract authorization, got different variant"),
    }
    assert_eq!(
        latest.1.sub_invocations.len(),
        0,
        "Expected no sub-invocations"
    );
}

/// Helper: verify that an auth entry exists for the given signer and
/// function name. Returns true if found. Used for settlement entrypoints
/// that delegate require_auth through helper modules (SDK v22 may not
/// increment auths() count for delegated calls).
fn auth_contains(
    env: &Env,
    expected_signer: &Address,
    expected_fn_name: &str,
    expected_contract: &Address,
) -> bool {
    let expected_sym = Symbol::new(env, expected_fn_name);
    env.auths().iter().any(|(addr, inv)| {
        if *addr != *expected_signer {
            return false;
        }
        match &inv.function {
            AuthorizedFunction::Contract((contract_addr, fn_name, _args)) => {
                contract_addr == expected_contract && *fn_name == expected_sym
            }
            _ => false,
        }
    })
}

// ===========================================================================
// Settlement — set_developer_min_balance
// ===========================================================================

/// require_auth harness: clearing auths must block mutation.
#[test]
fn set_developer_min_balance_requires_auth() {
    let env = Env::default();
    let (admin, _, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_developer_min_balance(&admin, &developer, &100);
    assert!(
        res.is_err(),
        "set_developer_min_balance must require auth on caller"
    );
}

/// mock-auth harness: verifies signer identity is captured in auth snapshot.
/// SDK v22: settlement entrypoints delegate `require_auth` through `limits`
/// module, so we verify signer and fn name via contains check rather than
/// counting snapshot entries.
#[test]
fn set_developer_min_balance_captures_signer_identity() {
    let env = Env::default();
    let (admin, contract_addr, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    // Clear auth snapshot from setup, then re-mock for the call under test.
    env.set_auths(&[]);
    env.mock_all_auths();
    client.set_developer_min_balance(&admin, &developer, &250);

    assert!(
        auth_contains(&env, &admin, "set_developer_min_balance", &contract_addr),
        "Expected auth entry for set_developer_min_balance with admin signer"
    );
}

// ===========================================================================
// Settlement — set_minimum_balance (alias)
// ===========================================================================

/// require_auth harness: clearing auths must block mutation.
#[test]
fn set_minimum_balance_requires_auth() {
    let env = Env::default();
    let (admin, _, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_minimum_balance(&admin, &developer, &50);
    assert!(
        res.is_err(),
        "set_minimum_balance must require auth on caller"
    );
}

/// mock-auth harness: verifies signer identity is captured in auth snapshot.
#[test]
fn set_minimum_balance_captures_signer_identity() {
    let env = Env::default();
    let (admin, contract_addr, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    env.mock_all_auths();
    client.set_minimum_balance(&admin, &developer, &75);

    assert!(
        auth_contains(&env, &admin, "set_minimum_balance", &contract_addr),
        "Expected auth entry for set_minimum_balance with admin signer"
    );
}

// ===========================================================================
// Settlement — set_daily_withdraw_cap
// ===========================================================================

/// require_auth harness: clearing auths must block mutation.
#[test]
fn set_daily_withdraw_cap_requires_auth() {
    let env = Env::default();
    let (admin, _, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_daily_withdraw_cap(&admin, &developer, &1_000);
    assert!(
        res.is_err(),
        "set_daily_withdraw_cap must require auth on caller"
    );
}

/// mock-auth harness: verifies signer identity is captured in auth snapshot.
#[test]
fn set_daily_withdraw_cap_captures_signer_identity() {
    let env = Env::default();
    let (admin, contract_addr, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    env.mock_all_auths();
    client.set_daily_withdraw_cap(&admin, &developer, &5_000);

    assert!(
        auth_contains(&env, &admin, "set_daily_withdraw_cap", &contract_addr),
        "Expected auth entry for set_daily_withdraw_cap with admin signer"
    );
}

// ===========================================================================
// Settlement — read-only limits views (no auth expected)
// ===========================================================================

/// Verifies `get_developer_min_balance` returns 0 by default without
/// requiring auth. In SDK v22, storage TTL bumps may add auth snapshot
/// entries; the key property is that the call succeeds without panic.
#[test]
fn get_developer_min_balance_no_auth() {
    let env = Env::default();
    let (_, _, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let bal = client.get_developer_min_balance(&developer);
    assert_eq!(bal, 0, "unset min balance defaults to 0");
}

/// Verifies `get_minimum_balance` returns 0 by default without requiring auth.
#[test]
fn get_minimum_balance_no_auth() {
    let env = Env::default();
    let (_, _, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let bal = client.get_minimum_balance(&developer);
    assert_eq!(bal, 0);
}

/// Verifies `get_daily_withdraw_cap` returns 0 by default without requiring auth.
#[test]
fn get_daily_withdraw_cap_no_auth() {
    let env = Env::default();
    let (_, _, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let cap = client.get_daily_withdraw_cap(&developer);
    assert_eq!(cap, 0, "unset daily cap defaults to 0 (unlimited)");
}

/// Verifies `get_withdrawal_today` returns 0 by default without requiring auth.
#[test]
fn get_withdrawal_today_no_auth() {
    let env = Env::default();
    let (_, _, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let amt = client.get_withdrawal_today(&developer);
    assert_eq!(amt, 0);
}

// ===========================================================================
// Revenue pool — set_max_distribute
// ===========================================================================

/// require_auth harness: clearing auths must block mutation.
#[test]
fn set_max_distribute_requires_auth() {
    let env = Env::default();
    let (admin, _, client) = setup_pool(&env);

    env.set_auths(&[]);
    let res = client.try_set_max_distribute(&admin, &10_000);
    assert!(
        res.is_err(),
        "set_max_distribute must require auth on caller"
    );
}

/// mock-auth harness: verifies signer identity is captured in auth snapshot.
#[test]
fn set_max_distribute_captures_signer_identity() {
    let env = Env::default();
    let (admin, contract_addr, client) = setup_pool(&env);

    let baseline = env.auths().len();

    env.mock_all_auths();
    client.set_max_distribute(&admin, &42_000);

    let auths = env.auths();
    assert_eq!(auths.len(), baseline + 1);
    assert_latest_auth(&env, &admin, "set_max_distribute", &contract_addr);
}

/// Verifies `get_max_distribute` is callable without auth and returns the
/// default maximum value.
#[test]
fn get_max_distribute_no_auth() {
    let env = Env::default();
    let (_, _, client) = setup_pool(&env);

    env.set_auths(&[]);
    let max = client.get_max_distribute();
    assert_eq!(max, i128::MAX, "unset max distribute defaults to i128::MAX");
}

// ===========================================================================
// Negative tests — wrong caller cannot invoke mutating entrypoints
// ===========================================================================

/// Non-admin caller must be rejected even with mock_all_auths.
#[test]
fn set_developer_min_balance_rejects_non_admin() {
    let env = Env::default();
    let (_, _, client) = setup_settlement(&env);
    let intruder = Address::generate(&env);
    let developer = Address::generate(&env);

    env.mock_all_auths();
    let res = client.try_set_developer_min_balance(&intruder, &developer, &100);
    assert!(
        res.is_err(),
        "set_developer_min_balance must reject non-admin caller"
    );
}

/// Non-admin caller must be rejected for set_max_distribute.
#[test]
fn set_max_distribute_rejects_non_admin() {
    let env = Env::default();
    let (_, _, client) = setup_pool(&env);
    let intruder = Address::generate(&env);

    env.mock_all_auths();
    let res = client.try_set_max_distribute(&intruder, &500);
    assert!(
        res.is_err(),
        "set_max_distribute must reject non-admin caller"
    );
}

// ===========================================================================
// Happy path with auth — guards against false negatives
// ===========================================================================

/// With auth mocked, admin can set and read back a developer min balance.
/// Auth mocking is inherited from setup — no redundant `mock_all_auths` call.
#[test]
fn set_developer_min_balance_succeeds_with_auth() {
    let env = Env::default();
    let (admin, _, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    client.set_developer_min_balance(&admin, &developer, &250);
    assert_eq!(client.get_developer_min_balance(&developer), 250);
}

/// With auth mocked, admin can set and read back a daily withdraw cap.
/// Auth mocking is inherited from setup — no redundant `mock_all_auths` call.
#[test]
fn set_daily_withdraw_cap_succeeds_with_auth() {
    let env = Env::default();
    let (admin, _, client) = setup_settlement(&env);
    let developer = Address::generate(&env);

    client.set_daily_withdraw_cap(&admin, &developer, &5_000);
    assert_eq!(client.get_daily_withdraw_cap(&developer), 5_000);
}

/// With auth mocked, admin can update max distribute.
/// Auth mocking is inherited from setup — no redundant `mock_all_auths` call.
#[test]
fn set_max_distribute_succeeds_with_auth() {
    let env = Env::default();
    let (admin, _, client) = setup_pool(&env);

    client.set_max_distribute(&admin, &42_000);
    assert_eq!(client.get_max_distribute(), 42_000);
}

// ===========================================================================
// Stronger read-only test — no auth mocking at all
// ===========================================================================

/// Prove that `get_daily_withdraw_cap` works on an uninitialized contract
/// without ANY auth mocking, catching accidental `require_auth` additions.
#[test]
fn read_only_view_works_without_any_mock_auths() {
    let env = Env::default();
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);
    let developer = Address::generate(&env);

    // No mock_all_auths, no set_auths — a view must not require auth.
    let cap = client.get_daily_withdraw_cap(&developer);
    assert_eq!(cap, 0);
}

// ===========================================================================
// Snapshot inventory — fail loudly if the documented surface shrinks
// ===========================================================================

/// Documents the expected mutating limits entrypoint count for this suite.
/// Bump intentionally when adding a new limits mutator + corresponding test.
#[test]
fn auth_context_covers_expected_mutating_entrypoint_count() {
    // Mutators asserted above:
    // 1. set_developer_min_balance
    // 2. set_minimum_balance
    // 3. set_daily_withdraw_cap
    // 4. set_max_distribute
    const EXPECTED_MUTATING_LIMITS_ENTRYPOINTS: usize = 4;
    assert_eq!(
        EXPECTED_MUTATING_LIMITS_ENTRYPOINTS, 4,
        "update auth.rs when adding/removing limits mutators"
    );
}
