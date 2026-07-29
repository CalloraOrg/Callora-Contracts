//! Tests for the per-developer freeze entrypoint.
//!
//! Covers:
//! - Admin can freeze and unfreeze a developer
//! - Non-admin cannot freeze or unfreeze
//! - `is_developer_frozen` returns correct state
//! - Double-freeze returns `DeveloperFrozen`
//! - Unfreeze without prior freeze returns `DeveloperNotFrozen`

#![cfg(test)]

use crate::{CalloraSettlement, CalloraSettlementClient};
use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};

/// Build a minimal settlement contract; return `(env, contract_id, admin)`.
fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    let contract_id = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &contract_id);
    client.init(&admin, &vault);
    (env, contract_id, admin)
}

#[test]
fn freeze_and_unfreeze_roundtrip() {
    let (env, contract_id, admin) = setup();
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
    let (env, contract_id, admin) = setup();
    let developer = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract_id);

    client.freeze_developer(&admin, &developer, &Symbol::new(&env, "audit"));
    // Second freeze should panic with DeveloperFrozen (error code 30)
    client.freeze_developer(&admin, &developer, &Symbol::new(&env, "second"));
}

#[test]
#[should_panic(expected = "Contract, #31")]
fn unfreeze_without_prior_freeze_panics() {
    let (env, contract_id, admin) = setup();
    let developer = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract_id);

    // Try to unfreeze without freeze - should panic with DeveloperNotFrozen (31)
    client.unfreeze_developer(&admin, &developer);
}

#[test]
#[should_panic(expected = "Contract, #32")]
fn non_admin_cannot_freeze() {
    let (env, contract_id, _admin) = setup();
    let attacker = Address::generate(&env);
    let developer = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract_id);

    // Non-admin freeze should panic with FreezeUnauthorized (32)
    client.freeze_developer(&attacker, &developer, &Symbol::new(&env, "malicious"));
}

#[test]
#[should_panic(expected = "Contract, #32")]
fn non_admin_cannot_unfreeze() {
    let (env, contract_id, admin) = setup();
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
    let (env, contract_id, _admin) = setup();
    let developer = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract_id);

    assert!(!client.is_developer_frozen(&developer));
}
