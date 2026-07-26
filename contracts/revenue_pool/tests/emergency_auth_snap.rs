//! # Emergency Drain Auth Snapshot — Per‑Entrypoint Authorization Tests
//!
//! This integration test module verifies that **every state‑changing entrypoint**
//! in the emergency drain subsystem enforces `require_auth` on the admin caller,
//! and that the **read‑only view** does **not** require authorization.
//!
//! The tests serve as a living snapshot of the emergency drain auth surface: if a
//! new state‑changing entrypoint is added **without** `require_auth`, or if an
//! existing entrypoint's auth requirement is accidentally removed, the
//! corresponding test in this file will catch it during CI.
//!
//! ## Coverage
//!
//! | Entrypoint                     | Auth Required | Auth On     | Test Function                          |
//! |--------------------------------|---------------|-------------|----------------------------------------|
//! | `propose_emergency_drain`      | Yes           | `caller`    | `propose_emergency_drain_requires_auth` |
//! | `execute_emergency_drain`      | Yes           | `caller`    | `execute_emergency_drain_requires_auth` |
//! | `cancel_emergency_drain`       | Yes           | `caller`    | `cancel_emergency_drain_requires_auth`  |
//! | `get_pending_emergency_drain`  | No            | —           | `get_pending_emergency_drain_no_auth`   |

extern crate std;

use callora_revenue_pool::{RevenuePool, RevenuePoolClient};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token;
use soroban_sdk::{Address, Env};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Deploy a Stellar Asset Contract (SAC) token for USDC and return its address
/// together with a regular client and an admin (mint‑capable) client.
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

/// Deploy the `RevenuePool` contract and return its on‑ledger address together
/// with a typed client.
fn create_pool(env: &Env) -> (Address, RevenuePoolClient<'_>) {
    let address = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &address);
    (address, client)
}

/// Mint `amount` USDC into `pool_address` (requires the admin SAC client).
fn fund_pool(usdc_admin_client: &token::StellarAssetClient, pool_address: &Address, amount: i128) {
    usdc_admin_client.mint(pool_address, &amount);
}

/// Initialise the pool with a fresh admin + USDC token, fully mocking auth for
/// the setup call. Returns `(admin, pool_address, client, usdc_admin_client)`.
fn setup_pool(
    env: &Env,
) -> (
    Address,
    Address,
    RevenuePoolClient<'_>,
    token::StellarAssetClient<'_>,
) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let (pool_addr, client) = create_pool(env);
    let (usdc_addr, _usdc_client, usdc_admin) = create_usdc(env, &admin);
    client.init(&admin, &usdc_addr);
    (admin, pool_addr, client, usdc_admin)
}

// ---------------------------------------------------------------------------
// State‑changing entrypoints — each test verifies that calling the entrypoint
// WITHOUT authorization fails with a require_auth panic.
// ---------------------------------------------------------------------------

/// Verify that `propose_emergency_drain` requires auth on `caller`.
///
/// The `caller` must be the current admin and must authorize the call.
/// Without authorization, the call must fail at the `require_auth` check.
#[test]
fn propose_emergency_drain_requires_auth() {
    let env = Env::default();
    let (admin, pool_addr, client, usdc_admin) = setup_pool(&env);

    // Fund the pool so the proposal would succeed if auth passed.
    env.mock_all_auths();
    fund_pool(&usdc_admin, &pool_addr, 10_000);

    let treasury = Address::generate(&env);

    // Strip all authorizations — the call must fail at require_auth.
    env.set_auths(&[]);
    let res = client.try_propose_emergency_drain(&admin, &treasury, &5_000_i128);
    assert!(res.is_err(), "propose_emergency_drain must require auth on caller");
}

/// Verify that `execute_emergency_drain` requires auth on `caller`.
///
/// The `caller` must be the current admin and must authorize the call.
/// Without authorization, the call must fail at the `require_auth` check.
#[test]
fn execute_emergency_drain_requires_auth() {
    let env = Env::default();
    let (admin, pool_addr, client, usdc_admin) = setup_pool(&env);

    // Propose a drain first (with mocked auth).
    env.mock_all_auths();
    fund_pool(&usdc_admin, &pool_addr, 10_000);
    let treasury = Address::generate(&env);
    client.propose_emergency_drain(&admin, &treasury, &5_000_i128);

    // Advance the ledger beyond the 24‑h timelock so execution would succeed if auth passed.
    env.ledger().set_timestamp(86_401);

    // Strip all authorizations — the call must fail at require_auth.
    env.set_auths(&[]);
    let res = client.try_execute_emergency_drain(&admin);
    assert!(res.is_err(), "execute_emergency_drain must require auth on caller");
}

/// Verify that `cancel_emergency_drain` requires auth on `caller`.
///
/// The `caller` must be the current admin and must authorize the call.
/// Without authorization, the call must fail at the `require_auth` check.
#[test]
fn cancel_emergency_drain_requires_auth() {
    let env = Env::default();
    let (admin, pool_addr, client, usdc_admin) = setup_pool(&env);

    // Propose a drain first (with mocked auth).
    env.mock_all_auths();
    fund_pool(&usdc_admin, &pool_addr, 10_000);
    let treasury = Address::generate(&env);
    client.propose_emergency_drain(&admin, &treasury, &5_000_i128);

    // Strip all authorizations — the call must fail at require_auth.
    env.set_auths(&[]);
    let res = client.try_cancel_emergency_drain(&admin);
    assert!(res.is_err(), "cancel_emergency_drain must require auth on caller");
}

// ---------------------------------------------------------------------------
// Non‑admin rejection tests — verify that a non‑admin caller is rejected even
// WITH authorization. This ensures the admin check (`require_admin`) is also
// enforced in addition to `require_auth`.
// ---------------------------------------------------------------------------

/// Verify that `propose_emergency_drain` rejects non‑admin callers.
///
/// Even if a non‑admin authorizes the call, it must be rejected by the
/// `require_admin` check inside the entrypoint.
#[test]
fn propose_emergency_drain_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_addr, _usdc_client, usdc_admin) = create_usdc(&env, &admin);
    client.init(&admin, &usdc_addr);
    fund_pool(&usdc_admin, &pool_addr, 10_000);

    let outsider = Address::generate(&env);
    let treasury = Address::generate(&env);

    // Non‑admin authorizes but should be rejected by require_admin.
    let res = client.try_propose_emergency_drain(&outsider, &treasury, &5_000_i128);
    assert!(
        res.is_err(),
        "propose_emergency_drain must reject non-admin caller"
    );
    // The error should be the unauthorized admin error, not an auth error.
    // We can't easily inspect the error message in try_* but the panic path
    // would have been "unauthorized: caller is not admin".
}

/// Verify that `execute_emergency_drain` rejects non‑admin callers.
///
/// Even if a non‑admin authorizes the call, it must be rejected by the
/// `require_admin` check inside the entrypoint.
#[test]
fn execute_emergency_drain_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_addr, _usdc_client, usdc_admin) = create_usdc(&env, &admin);
    client.init(&admin, &usdc_addr);
    fund_pool(&usdc_admin, &pool_addr, 10_000);

    let treasury = Address::generate(&env);
    client.propose_emergency_drain(&admin, &treasury, &5_000_i128);
    env.ledger().set_timestamp(86_401);

    let outsider = Address::generate(&env);

    // Non‑admin authorizes but should be rejected by require_admin.
    let res = client.try_execute_emergency_drain(&outsider);
    assert!(
        res.is_err(),
        "execute_emergency_drain must reject non-admin caller"
    );
}

/// Verify that `cancel_emergency_drain` rejects non‑admin callers.
///
/// Even if a non‑admin authorizes the call, it must be rejected by the
/// `require_admin` check inside the entrypoint.
#[test]
fn cancel_emergency_drain_rejects_non_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_addr, _usdc_client, usdc_admin) = create_usdc(&env, &admin);
    client.init(&admin, &usdc_addr);
    fund_pool(&usdc_admin, &pool_addr, 10_000);

    let treasury = Address::generate(&env);
    client.propose_emergency_drain(&admin, &treasury, &5_000_i128);

    let outsider = Address::generate(&env);

    // Non‑admin authorizes but should be rejected by require_admin.
    let res = client.try_cancel_emergency_drain(&outsider);
    assert!(
        res.is_err(),
        "cancel_emergency_drain must reject non-admin caller"
    );
}

// ---------------------------------------------------------------------------
// Read‑only entrypoints — each test verifies that calling without auth
// succeeds (no require_auth panic).
// ---------------------------------------------------------------------------

/// Verify that `get_pending_emergency_drain` does not require auth.
///
/// This is a view function; it must not call `require_auth` on any parameter.
/// It should succeed even when no authorization is provided.
#[test]
fn get_pending_emergency_drain_no_auth() {
    let env = Env::default();
    let (admin, pool_addr, client, usdc_admin) = setup_pool(&env);

    // Before any proposal, the view returns None without auth.
    env.set_auths(&[]);
    let pending = client.get_pending_emergency_drain();
    assert_eq!(pending, None, "view must return None before proposal");

    // Propose a drain (with auth) to change state.
    env.mock_all_auths();
    fund_pool(&usdc_admin, &pool_addr, 10_000);
    let treasury = Address::generate(&env);
    client.propose_emergency_drain(&admin, &treasury, &5_000_i128);

    // After state change, the view still must not require auth.
    env.set_auths(&[]);
    let pending = client.get_pending_emergency_drain();
    assert!(pending.is_some(), "view must return Some after proposal");
    let drain = pending.unwrap();
    assert_eq!(drain.to, treasury);
    assert_eq!(drain.amount, 5_000);
}

// ---------------------------------------------------------------------------
// Canonical smoke test — admin WITH auth can call every gated entrypoint.
// ---------------------------------------------------------------------------

/// A single integration test that successfully invokes every state‑changing
/// emergency entrypoint with proper authorization. This proves the harness
/// setup is correct and that the entrypoints are reachable when auth is provided.
#[test]
fn admin_with_auth_can_call_all_emergency_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_addr, _usdc_client, usdc_admin) = create_usdc(&env, &admin);
    client.init(&admin, &usdc_addr);
    fund_pool(&usdc_admin, &pool_addr, 50_000);

    let treasury = Address::generate(&env);

    // --- propose_emergency_drain ---
    client.propose_emergency_drain(&admin, &treasury, &1_000);
    let pending = client.get_pending_emergency_drain().unwrap();
    assert_eq!(pending.to, treasury);
    assert_eq!(pending.amount, 1_000);

    // --- cancel_emergency_drain ---
    client.cancel_emergency_drain(&admin);
    assert_eq!(client.get_pending_emergency_drain(), None);

    // --- propose_emergency_drain (again) ---
    client.propose_emergency_drain(&admin, &treasury, &1_000);
    assert!(client.get_pending_emergency_drain().is_some());

    // --- execute_emergency_drain (after timelock) ---
    env.ledger().set_timestamp(86_401);
    client.execute_emergency_drain(&admin);
    assert_eq!(client.get_pending_emergency_drain(), None);
}