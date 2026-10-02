//! Per-entrypoint auth snapshot tests for `callora-distribute`.
//!
//! Every state-changing entrypoint must require admin auth.
//! Every read-only entrypoint must succeed without auth.
//!
//! These tests exist to catch regressions where an auth guard is accidentally
//! removed or bypassed.

#![cfg(test)]

extern crate std;

use callora_distribute::{Distribute, DistributeClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, BytesN, Env};

fn create_contract(env: &Env) -> DistributeClient<'_> {
    let contract_id = env.register(Distribute, ());
    DistributeClient::new(env, &contract_id)
}

/// Returns an initialized contract with a funded USDC token.
fn setup(env: &Env) -> (Address, Address, DistributeClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let client = create_contract(env);
    client.init(&admin, &usdc_addr);
    (admin, usdc_addr, client)
}

// ---------------------------------------------------------------------------
// State-changing entrypoints: must require auth
// ---------------------------------------------------------------------------

#[test]
fn set_admin_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    let new_admin = Address::generate(&env);
    env.set_auths(&[]);
    let res = client.try_set_admin(&admin, &new_admin);
    assert!(res.is_err(), "set_admin must require auth");
}

#[test]
fn accept_admin_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    let new_admin = Address::generate(&env);
    env.mock_all_auths();
    client.set_admin(&admin, &new_admin);

    env.set_auths(&[]);
    let res = client.try_accept_admin(&new_admin);
    assert!(res.is_err(), "accept_admin must require auth");
}

#[test]
fn claim_admin_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    let new_admin = Address::generate(&env);
    env.mock_all_auths();
    client.set_admin(&admin, &new_admin);

    env.set_auths(&[]);
    let res = client.try_claim_admin(&new_admin);
    assert!(res.is_err(), "claim_admin must require auth");
}

#[test]
fn cancel_admin_transfer_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    let new_admin = Address::generate(&env);
    env.mock_all_auths();
    client.set_admin(&admin, &new_admin);

    env.set_auths(&[]);
    let res = client.try_cancel_admin_transfer(&admin);
    assert!(res.is_err(), "cancel_admin_transfer must require auth");
}

#[test]
fn pause_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_pause(&admin);
    assert!(res.is_err(), "pause must require auth");
}

#[test]
fn unpause_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.mock_all_auths();
    client.pause(&admin);

    env.set_auths(&[]);
    let res = client.try_unpause(&admin);
    assert!(res.is_err(), "unpause must require auth");
}

#[test]
fn set_max_distribute_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_set_max_distribute(&admin, &1_000);
    assert!(res.is_err(), "set_max_distribute must require auth");
}

#[test]
fn distribute_requires_auth() {
    let env = Env::default();
    let (admin, usdc, client) = setup(&env);

    let recipient = Address::generate(&env);
    let usdc_sac = token::StellarAssetClient::new(&env, &usdc);
    let contract_addr = env.register(Distribute, ());
    usdc_sac.mint(&contract_addr, &1_000);

    // Re-initialize on the second contract instance just to have a funded one.
    // For this test we just verify auth failure, no actual transfer needed.
    env.set_auths(&[]);
    let res = client.try_distribute(&admin, &recipient, &100);
    assert!(res.is_err(), "distribute must require auth");
}

#[test]
fn batch_distribute_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    let recipient = Address::generate(&env);
    let mut payments = soroban_sdk::Vec::new(&env);
    payments.push_back((recipient, 100i128));

    env.set_auths(&[]);
    let res = client.try_batch_distribute(&admin, &payments);
    assert!(res.is_err(), "batch_distribute must require auth");
}

#[test]
fn upgrade_requires_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    let dummy = BytesN::from_array(&env, &[0u8; 32]);
    env.set_auths(&[]);
    let res = client.try_upgrade(&admin, &dummy);
    assert!(res.is_err(), "upgrade must require auth");
}

// ---------------------------------------------------------------------------
// Read-only entrypoints: must succeed without auth
// ---------------------------------------------------------------------------

#[test]
fn get_admin_does_not_require_auth() {
    let env = Env::default();
    let (admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    let returned_admin = client.get_admin();
    assert_eq!(
        returned_admin, admin,
        "get_admin must return the admin without auth"
    );
}

#[test]
fn get_usdc_token_does_not_require_auth() {
    let env = Env::default();
    let (_admin, usdc, client) = setup(&env);

    env.set_auths(&[]);
    let returned = client.get_usdc_token();
    assert_eq!(returned, usdc, "get_usdc_token must not require auth");
}

#[test]
fn get_paused_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    assert!(!client.get_paused(), "get_paused must not require auth");
}

#[test]
fn get_max_distribute_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    let _ = client.get_max_distribute();
}

#[test]
fn get_max_batch_size_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    let _ = client.get_max_batch_size();
}

#[test]
fn get_pending_admin_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(
        client.get_pending_admin(),
        None,
        "get_pending_admin must not require auth"
    );
}

#[test]
fn get_version_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(
        client.get_version(),
        None,
        "get_version must not require auth"
    );
}

#[test]
fn balance_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _usdc, client) = setup(&env);

    env.set_auths(&[]);
    let _ = client.balance();
}

// ---------------------------------------------------------------------------
// Full happy-path smoke test (admin with auth can use all entrypoints)
// ---------------------------------------------------------------------------

#[test]
fn admin_with_auth_can_use_all_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);

    // Fund the contract.
    let usdc_sac = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_sac.mint(&contract_addr, &10_000);

    // Verify views.
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_usdc_token(), usdc_addr);
    assert!(!client.get_paused());
    assert_eq!(client.balance(), 10_000);

    // Set distribution cap.
    client.set_max_distribute(&admin, &5_000);
    assert_eq!(client.get_max_distribute(), 5_000);

    // Single distribute.
    let recipient = Address::generate(&env);
    client.distribute(&admin, &recipient, &1_000);
    assert_eq!(client.balance(), 9_000);

    // Batch distribute.
    let r2 = Address::generate(&env);
    let r3 = Address::generate(&env);
    let mut payments = soroban_sdk::Vec::new(&env);
    payments.push_back((r2, 500i128));
    payments.push_back((r3, 500i128));
    client.batch_distribute(&admin, &payments);
    assert_eq!(client.balance(), 8_000);

    // Pause / unpause.
    client.pause(&admin);
    assert!(client.get_paused());
    client.unpause(&admin);
    assert!(!client.get_paused());

    // Admin rotation.
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));
    client.accept_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
    assert_eq!(client.get_pending_admin(), None);
}

/// Verify that `batch_distribute` accepts a `Vec<(Address, i128)>` — the concrete
/// Soroban SDK type — not an import alias (`SorobanVec`). This is the regression
/// test for issue #1171.
#[test]
fn batch_distribute_accepts_soroban_vec_type() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);
    client.init(&admin, &usdc_addr);

    let usdc_sac = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_sac.mint(&contract_addr, &10_000);

    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);

    // Use soroban_sdk::Vec directly — this must compile without the alias workaround.
    let mut payments: soroban_sdk::Vec<(Address, i128)> = soroban_sdk::Vec::new(&env);
    payments.push_back((r1, 100i128));
    payments.push_back((r2, 200i128));

    client.batch_distribute(&admin, &payments);
    assert_eq!(client.balance(), 9_700);
}
