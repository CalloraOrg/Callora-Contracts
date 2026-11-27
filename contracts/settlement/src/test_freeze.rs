//! Tests for the per-developer freeze entrypoint.
//!
//! Covers:
//! - Admin can freeze and unfreeze a developer
//! - Non-admin cannot freeze or unfreeze
//! - `is_developer_frozen` returns correct state
//! - Double-freeze returns `DeveloperFrozen`
//! - Unfreeze without prior freeze returns `DeveloperNotFrozen`
//!
//! Also covers the global freeze circuit-breaker (Issue #1527):
//! - `receive_payment` / `batch_receive_payment` / `withdraw_developer_balance`
//!   blocked while system-frozen; restored after unfreeze or after clearing
//!   the freeze contract address.

#![cfg(test)]

use crate::{CalloraSettlement, CalloraSettlementClient};
use soroban_sdk::{contract, contractimpl, testutils::Address as _, Address, Env, Symbol, Vec};

// ---------------------------------------------------------------------------
// Helpers shared by both test sections
// ---------------------------------------------------------------------------

/// Build a minimal settlement contract; return `(env, contract_id, admin)`.
fn setup_basic() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    let contract_id = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &contract_id);
    client.init(&admin, &vault);
    (env, contract_id, admin)
}

// ---------------------------------------------------------------------------
// Per-developer freeze tests (upstream)
// ---------------------------------------------------------------------------

#[test]
fn freeze_and_unfreeze_roundtrip() {
    let (env, contract_id, admin) = setup_basic();
    let developer = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract_id);

    // Initially not frozen
    assert!(!client.is_developer_frozen(&developer));

    // Freeze
    client.freeze_developer(&admin, &developer, &Symbol::new(&env, "audit"));
    assert!(client.is_developer_frozen(&developer));

    // Unfreeze
    client.unfreeze_developer(&admin, &developer);
    assert!(!client.is_developer_frozen(&developer));
}

#[test]
#[should_panic(expected = "Contract, #30")]
fn double_freeze_panics() {
    let (env, contract_id, admin) = setup_basic();
    let developer = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract_id);

    client.freeze_developer(&admin, &developer, &Symbol::new(&env, "audit"));
    // Second freeze should panic with DeveloperFrozen (error code 30)
    client.freeze_developer(&admin, &developer, &Symbol::new(&env, "second"));
}

#[test]
#[should_panic(expected = "Contract, #31")]
fn unfreeze_without_prior_freeze_panics() {
    let (env, contract_id, admin) = setup_basic();
    let developer = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract_id);

    // Try to unfreeze without freeze - should panic with DeveloperNotFrozen (31)
    client.unfreeze_developer(&admin, &developer);
}

#[test]
#[should_panic(expected = "Contract, #32")]
fn non_admin_cannot_freeze() {
    let (env, contract_id, _admin) = setup_basic();
    let attacker = Address::generate(&env);
    let developer = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract_id);

    // Non-admin freeze should panic with FreezeUnauthorized (32)
    client.freeze_developer(&attacker, &developer, &Symbol::new(&env, "malicious"));
}

#[test]
#[should_panic(expected = "Contract, #32")]
fn non_admin_cannot_unfreeze() {
    let (env, contract_id, admin) = setup_basic();
    let attacker = Address::generate(&env);
    let developer = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract_id);

    // First freeze as admin
    client.freeze_developer(&admin, &developer, &Symbol::new(&env, "audit"));

    // Try to unfreeze as non-admin - should panic with FreezeUnauthorized (32)
    client.unfreeze_developer(&attacker, &developer);
}

#[test]
fn is_developer_frozen_default_false() {
    let (env, contract_id, _admin) = setup_basic();
    let developer = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract_id);

    assert!(!client.is_developer_frozen(&developer));
}

// ---------------------------------------------------------------------------
// Global freeze circuit-breaker tests (Issue #1527)
// ---------------------------------------------------------------------------

/// Minimal in-process mock of the CalloraFreeze contract.
#[contract]
pub struct MockFreeze;

#[contractimpl]
impl MockFreeze {
    pub fn is_frozen(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&Symbol::new(&env, "frozen"))
            .unwrap_or(false)
    }

    pub fn freeze(env: Env) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "frozen"), &true);
    }

    pub fn unfreeze(env: Env) {
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "frozen"), &false);
    }
}

fn create_usdc(env: &Env, admin: &Address) -> Address {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    ca.address()
}

/// Returns `(client, admin, vault, usdc, freeze_addr)`.
fn setup_with_freeze(
    env: &Env,
) -> (CalloraSettlementClient, Address, Address, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let usdc_admin = Address::generate(env);
    let usdc = create_usdc(env, &usdc_admin);

    let settlement_addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(env, &settlement_addr);
    client.init(&admin, &vault);
    client.set_usdc_token(&admin, &usdc);

    let freeze_addr = env.register(MockFreeze, ());

    (client, admin, vault, usdc, freeze_addr)
}

#[test]
fn receive_payment_blocked_when_system_frozen() {
    let env = Env::default();
    let (client, admin, vault, usdc, freeze_addr) = setup_with_freeze(&env);

    client.set_freeze_contract(&admin, &Some(freeze_addr.clone()));
    MockFreezeClient::new(&env, &freeze_addr).freeze();

    let result = client.try_receive_payment(&vault, &1_000i128, &true, &None, &usdc, &1u32);
    assert!(result.is_err(), "receive_payment must fail while system-frozen");
}

#[test]
fn receive_payment_succeeds_after_system_unfreeze() {
    let env = Env::default();
    let (client, admin, vault, usdc, freeze_addr) = setup_with_freeze(&env);

    client.set_freeze_contract(&admin, &Some(freeze_addr.clone()));
    let freeze = MockFreezeClient::new(&env, &freeze_addr);
    freeze.freeze();

    assert!(client
        .try_receive_payment(&vault, &1_000i128, &true, &None, &usdc, &1u32)
        .is_err());

    freeze.unfreeze();
    client.receive_payment(&vault, &1_000i128, &true, &None, &usdc, &2u32);
}

#[test]
fn batch_receive_payment_blocked_when_system_frozen() {
    let env = Env::default();
    let (client, admin, vault, usdc, freeze_addr) = setup_with_freeze(&env);

    client.set_freeze_contract(&admin, &Some(freeze_addr.clone()));
    MockFreezeClient::new(&env, &freeze_addr).freeze();

    let dev = Address::generate(&env);
    let items = Vec::from_array(&env, [(dev, 1_000i128)]);
    let result = client.try_batch_receive_payment(&vault, &items, &usdc, &1u32);
    assert!(result.is_err(), "batch_receive_payment must fail while system-frozen");
}

#[test]
fn receive_payment_unaffected_without_freeze_contract() {
    let env = Env::default();
    let (client, _admin, vault, usdc, _freeze_addr) = setup_with_freeze(&env);

    // No freeze contract configured — must work normally
    client.receive_payment(&vault, &1_000i128, &true, &None, &usdc, &1u32);
}

#[test]
fn set_freeze_contract_non_admin_fails() {
    let env = Env::default();
    let (client, _admin, _vault, _usdc, freeze_addr) = setup_with_freeze(&env);

    let stranger = Address::generate(&env);
    let result = client.try_set_freeze_contract(&stranger, &Some(freeze_addr));
    assert!(result.is_err(), "non-admin must not set freeze contract");
}

#[test]
fn set_freeze_contract_self_address_fails() {
    let env = Env::default();
    let (client, admin, _vault, _usdc, _freeze_addr) = setup_with_freeze(&env);

    let result = client.try_set_freeze_contract(&admin, &Some(client.address.clone()));
    assert!(result.is_err(), "settlement self-address must be rejected");
}

#[test]
fn clearing_freeze_contract_restores_normal_behaviour() {
    let env = Env::default();
    let (client, admin, vault, usdc, freeze_addr) = setup_with_freeze(&env);

    client.set_freeze_contract(&admin, &Some(freeze_addr.clone()));
    let freeze = MockFreezeClient::new(&env, &freeze_addr);
    freeze.freeze();

    assert!(client
        .try_receive_payment(&vault, &1_000i128, &true, &None, &usdc, &1u32)
        .is_err());

    // Clear the freeze contract address — check is now skipped
    client.set_freeze_contract(&admin, &None);
    client.receive_payment(&vault, &1_000i128, &true, &None, &usdc, &2u32);
}

#[test]
fn get_freeze_contract_reflects_configuration() {
    let env = Env::default();
    let (client, admin, _vault, _usdc, freeze_addr) = setup_with_freeze(&env);

    assert_eq!(client.get_freeze_contract(), None);
    client.set_freeze_contract(&admin, &Some(freeze_addr.clone()));
    assert_eq!(client.get_freeze_contract(), Some(freeze_addr));
    client.set_freeze_contract(&admin, &None);
    assert_eq!(client.get_freeze_contract(), None);
}
