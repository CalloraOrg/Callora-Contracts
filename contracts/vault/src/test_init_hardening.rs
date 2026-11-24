//! Regression coverage for init hardening (issue #1395):
//!
//! - owner authorization is required, blocking front-running;
//! - a non-zero `initial_balance` must be backed by on-ledger USDC;
//! - contract-address aliasing and `settlement == revenue_pool` are rejected
//!   with typed errors;
//! - every validation runs before the first storage write, so a rejected init
//!   can be retried and the tracked-balance invariant is preserved.

extern crate std;

use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{token, Address, Env, IntoVal, Symbol};

use super::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (
        addr.clone(),
        token::Client::new(env, &addr),
        token::StellarAssetClient::new(env, &addr),
    )
}

fn create_vault(env: &Env) -> (Address, CalloraVaultClient<'_>) {
    let addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &addr);
    (addr, client)
}

fn init_args(env: &Env) -> (Address, i128, i128) {
    let settlement = Address::generate(env);
    (settlement, 1, 1_000_000_000)
}

// ---------------------------------------------------------------------------
// 1. Missing owner auth
// ---------------------------------------------------------------------------

#[test]
fn init_without_owner_auth_is_rejected() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    let settlement = Address::generate(&env);

    // No auth mocks: owner.require_auth() must fail.
    let result = client.try_init(
        &owner,
        &usdc,
        &Some(0),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement),
    );

    assert!(result.is_err(), "init without owner auth must fail");
    // The failed call must not have written the owner key.
    assert!(
        client.try_get_owner().is_err(),
        "rejected init must leave storage untouched"
    );
}

// ---------------------------------------------------------------------------
// 2. Owner-authorized success
// ---------------------------------------------------------------------------

#[test]
fn init_with_owner_auth_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    let (settlement, min_deposit, max_deduct) = init_args(&env);

    client.init(
        &owner,
        &usdc,
        &Some(0),
        &Some(owner.clone()),
        &Some(min_deposit),
        &None,
        &Some(max_deduct),
        &Some(settlement),
    );

    assert_eq!(client.get_owner(), owner);
    assert_eq!(client.balance(), 0);
}

// ---------------------------------------------------------------------------
// 3. Unbacked initial balance
// ---------------------------------------------------------------------------

#[test]
fn init_rejects_unbacked_initial_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    let settlement = Address::generate(&env);

    // Vault only holds 50 USDC but claims 100.
    usdc_admin.mint(&vault_addr, &50);

    let result = client.try_init(
        &owner,
        &usdc,
        &Some(100),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement),
    );

    assert_eq!(result, Err(Ok(VaultError::InsufficientBalance)));
    assert!(client.try_get_owner().is_err(), "no state may be written");
}

#[test]
fn init_can_retry_after_unbacked_balance_rejection() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    let settlement = Address::generate(&env);

    usdc_admin.mint(&vault_addr, &50);
    let rejected = client.try_init(
        &owner,
        &usdc,
        &Some(100),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement.clone()),
    );
    assert_eq!(rejected, Err(Ok(VaultError::InsufficientBalance)));

    // Back the vault, then initialization must succeed.
    usdc_admin.mint(&vault_addr, &50);
    client.init(
        &owner,
        &usdc,
        &Some(100),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement),
    );
    assert_eq!(client.balance(), 100);
}

// ---------------------------------------------------------------------------
// 4. Backed initial balance
// ---------------------------------------------------------------------------

#[test]
fn init_records_backed_initial_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    let settlement = Address::generate(&env);

    usdc_admin.mint(&vault_addr, &200);
    client.init(
        &owner,
        &usdc,
        &Some(200),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement),
    );

    assert_eq!(client.balance(), 200);
    assert_eq!(usdc_client.balance(&vault_addr), 200);
}

#[test]
fn init_zero_balance_skips_onledger_check() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    let settlement = Address::generate(&env);

    client.init(
        &owner,
        &usdc,
        &Some(0),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement),
    );
    assert_eq!(client.balance(), 0);
}

// ---------------------------------------------------------------------------
// 5. Address validation (typed errors)
// ---------------------------------------------------------------------------

#[test]
fn init_rejects_usdc_token_as_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let settlement = Address::generate(&env);

    let result = client.try_init(
        &owner,
        &vault_addr,
        &Some(0),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement),
    );
    assert_eq!(result, Err(Ok(VaultError::UsdcTokenCannotBeVault)));
}

#[test]
fn init_rejects_settlement_as_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    let result = client.try_init(
        &owner,
        &usdc,
        &Some(0),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(vault_addr),
    );
    assert_eq!(result, Err(Ok(VaultError::SettlementCannotBeVault)));
}

#[test]
fn init_rejects_revenue_pool_as_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    let settlement = Address::generate(&env);

    let result = client.try_init(
        &owner,
        &usdc,
        &Some(0),
        &None,
        &Some(1),
        &Some(vault_addr),
        &Some(1_000_000_000),
        &Some(settlement),
    );
    assert_eq!(result, Err(Ok(VaultError::RevenuePoolCannotBeVault)));
}

#[test]
fn init_rejects_settlement_equal_to_revenue_pool() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    let pool = Address::generate(&env);

    let result = client.try_init(
        &owner,
        &usdc,
        &Some(0),
        &None,
        &Some(1),
        &Some(pool.clone()),
        &Some(1_000_000_000),
        &Some(pool),
    );
    assert_eq!(result, Err(Ok(VaultError::SettlementEqualsRevenuePool)));
}

// ---------------------------------------------------------------------------
// 6. Second init
// ---------------------------------------------------------------------------

#[test]
fn second_init_returns_already_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    let settlement = Address::generate(&env);

    client.init(
        &owner,
        &usdc,
        &Some(0),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement.clone()),
    );

    let result = client.try_init(
        &owner,
        &usdc,
        &Some(0),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement),
    );
    assert_eq!(result, Err(Ok(VaultError::AlreadyInitialized)));
}

// ---------------------------------------------------------------------------
// 7. Front-run simulation
// ---------------------------------------------------------------------------

#[test]
fn front_runner_cannot_initialize_for_owner_then_owner_succeeds() {
    let env = Env::default();
    let attacker = Address::generate(&env);
    let victim = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &attacker);
    let settlement = Address::generate(&env);

    // Attacker attempts to initialize the victim-owned vault without the
    // victim's signature.
    let front_run = client.try_init(
        &victim,
        &usdc,
        &Some(0),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement.clone()),
    );
    assert!(front_run.is_err(), "front-run without owner auth must fail");

    // The legitimate owner can now initialize with their authorization.
    env.mock_all_auths();
    client.init(
        &victim,
        &usdc,
        &Some(0),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement),
    );
    assert_eq!(client.get_owner(), victim);
}

// ---------------------------------------------------------------------------
// 8. Invariant and event
// ---------------------------------------------------------------------------

#[test]
fn init_preserves_tracked_balance_invariant() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    let settlement = Address::generate(&env);

    // Over-fund the vault; only `initial_balance` may be recorded.
    usdc_admin.mint(&vault_addr, &250);
    client.init(
        &owner,
        &usdc,
        &Some(100),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement),
    );

    let tracked = client.balance();
    let on_ledger = usdc_client.balance(&vault_addr);
    assert_eq!(tracked, 100);
    assert!(
        on_ledger >= tracked,
        "tracked balance must never exceed on-ledger USDC"
    );
}

#[test]
fn init_emits_event_with_backed_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    let settlement = Address::generate(&env);

    usdc_admin.mint(&vault_addr, &500);
    client.init(
        &owner,
        &usdc,
        &Some(500),
        &None,
        &Some(1),
        &None,
        &Some(1_000_000_000),
        &Some(settlement),
    );

    let events = env.events().all();
    let ev = events
        .iter()
        .find(|e| {
            e.0 == vault_addr && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "init")
            }
        })
        .expect("init event not found");

    let data: i128 = ev.2.into_val(&env);
    assert_eq!(data, 500);
}

#[test]
fn error_codes_remain_stable_and_append_only() {
    assert_eq!(VaultError::AlreadyInitialized as u32, 2);
    assert_eq!(VaultError::InsufficientBalance as u32, 5);
    assert_eq!(VaultError::UsdcTokenCannotBeVault as u32, 14);
    assert_eq!(VaultError::RevenuePoolCannotBeVault as u32, 15);
    assert_eq!(VaultError::SettlementCannotBeVault as u32, 56);
    assert_eq!(VaultError::SettlementEqualsRevenuePool as u32, 59);
}
