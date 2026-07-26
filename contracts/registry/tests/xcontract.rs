//! Cross-contract call safety tests for `callora-registry`.
//!
//! Verifies that when an external callee reverts or panics during registration,
//! the registry does not persist partial state (`RegisteredCount`, offering
//! records, or events implying success).
//!
//! ## Visible API
//!
//! No new public entrypoints are introduced by this test module. It exercises
//! existing registration paths that invoke:
//!
//! - `OfferingCatalog::put_offering` (catalog contract)
//! - `token::Client::balance` (SEP-41 token) in the balance-gated path

extern crate std;

use callora_registry::{CalloraRegistry, CalloraRegistryClient, RegistryError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token;
use soroban_sdk::{contract, contractimpl, Address, Env, String};

// ---------------------------------------------------------------------------
// Mock callees
// ---------------------------------------------------------------------------

#[contract]
pub struct OkCatalog;

#[contractimpl]
impl OkCatalog {
    pub fn put_offering(
        _env: Env,
        _registry: Address,
        _offering_id: String,
        _metadata: String,
    ) {
    }
}

#[contract]
pub struct PanickingCatalog;

#[contractimpl]
impl PanickingCatalog {
    pub fn put_offering(
        _env: Env,
        _registry: Address,
        _offering_id: String,
        _metadata: String,
    ) {
        panic!("catalog callee panic");
    }
}

#[contract]
pub struct RevertCatalog;

#[contractimpl]
impl RevertCatalog {
    pub fn put_offering(_env: Env, _registry: Address, _offering_id: String, _metadata: String) {
        panic!("catalog callee revert");
    }
}

#[contract]
pub struct PanickingToken;

#[contractimpl]
impl PanickingToken {
    pub fn balance(_env: Env, _id: Address) -> i128 {
        panic!("token balance panic");
    }

    pub fn transfer(_env: Env, _from: Address, _to: Address, _amount: i128) {
        // unused stub for interface completeness in tests
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn offering_id(env: &Env, suffix: &str) -> String {
    String::from_str(env, &format!("offering-{suffix}"))
}

fn metadata(env: &Env) -> String {
    String::from_str(env, "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi")
}

fn setup_registry(
    env: &Env,
    catalog: Address,
) -> (Address, CalloraRegistryClient<'_>, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let developer = Address::generate(env);
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(env, &registry_id);
    client.init(&admin, &catalog);
    (admin, client, developer)
}

// ---------------------------------------------------------------------------
// Catalog callee failure
// ---------------------------------------------------------------------------

#[test]
fn register_offering_catalog_panic_leaves_registry_clean() {
    let env = Env::default();
    let catalog = env.register(PanickingCatalog, ());
    let (admin, client, developer) = setup_registry(&env, catalog);

    let oid = offering_id(&env, "panic");
    let meta = metadata(&env);

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(result.is_err(), "catalog panic must fail registration");

    assert_eq!(client.registered_count(), 0);
    assert!(!client.is_offering_registered(&oid));
}

#[test]
fn register_offering_catalog_revert_leaves_registry_clean() {
    let env = Env::default();
    let catalog = env.register(RevertCatalog, ());
    let (admin, client, developer) = setup_registry(&env, catalog);

    let oid = offering_id(&env, "revert");
    let meta = metadata(&env);

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(result.is_err(), "catalog revert must fail registration");

    assert_eq!(client.registered_count(), 0);
    assert!(!client.is_offering_registered(&oid));
}

#[test]
fn register_offering_success_after_healthy_catalog() {
    let env = Env::default();
    let catalog = env.register(OkCatalog, ());
    let (admin, client, developer) = setup_registry(&env, catalog);

    let oid = offering_id(&env, "ok");
    let meta = metadata(&env);

    client.register_offering(&admin, &developer, &oid, &meta);

    assert_eq!(client.registered_count(), 1);
    assert!(client.is_offering_registered(&oid));
    let record = client.get_offering(&oid);
    assert_eq!(record.developer, developer);
    assert_eq!(record.metadata, meta);
}

// ---------------------------------------------------------------------------
// Token balance callee failure (balance-gated registration)
// ---------------------------------------------------------------------------

#[test]
fn balance_gate_token_panic_leaves_registry_clean() {
    let env = Env::default();
    let catalog = env.register(OkCatalog, ());
    let (admin, client, developer) = setup_registry(&env, catalog);

    let token_addr = env.register(PanickingToken, ());

    let oid = offering_id(&env, "token-panic");
    let meta = metadata(&env);

    let result = client.try_register_offering_with_balance_gate(
        &admin,
        &developer,
        &token_addr,
        &100i128,
        &oid,
        &meta,
    );
    assert!(result.is_err(), "token balance panic must abort registration");

    assert_eq!(client.registered_count(), 0);
    assert!(!client.is_offering_registered(&oid));
}

#[test]
fn balance_gate_catalog_panic_after_balance_read_leaves_registry_clean() {
    let env = Env::default();
    let catalog = env.register(PanickingCatalog, ());
    let (admin, client, developer) = setup_registry(&env, catalog);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner.clone());
    let token_addr = sac.address();
    let token_admin = token::StellarAssetClient::new(&env, &token_addr);
    token_admin.mint(&developer, &1_000);

    let oid = offering_id(&env, "gate-panic");
    let meta = metadata(&env);

    let result = client.try_register_offering_with_balance_gate(
        &admin,
        &developer,
        &token_addr,
        &100i128,
        &oid,
        &meta,
    );
    assert!(result.is_err(), "catalog panic must fail gated registration");

    assert_eq!(client.registered_count(), 0);
    assert!(!client.is_offering_registered(&oid));
}

#[test]
fn balance_gate_success_commits_registry_state() {
    let env = Env::default();
    let catalog = env.register(OkCatalog, ());
    let (admin, client, developer) = setup_registry(&env, catalog);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner);
    let token_addr = sac.address();
    let token_admin = token::StellarAssetClient::new(&env, &token_addr);
    token_admin.mint(&developer, &500);

    let oid = offering_id(&env, "gate-ok");
    let meta = metadata(&env);

    client.register_offering_with_balance_gate(
        &admin,
        &developer,
        &token_addr,
        &100i128,
        &oid,
        &meta,
    );

    assert_eq!(client.registered_count(), 1);
    assert!(client.is_offering_registered(&oid));
}

#[test]
fn balance_gate_insufficient_balance_does_not_call_catalog() {
    let env = Env::default();
    let catalog = env.register(PanickingCatalog, ());
    let (admin, client, developer) = setup_registry(&env, catalog);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner);
    let token_addr = sac.address();

    let oid = offering_id(&env, "low-balance");
    let meta = metadata(&env);

    let result = client.try_register_offering_with_balance_gate(
        &admin,
        &developer,
        &token_addr,
        &100i128,
        &oid,
        &meta,
    );
    assert!(
        matches!(
            result,
            Ok(Err(RegistryError::InsufficientDeveloperBalance))
        ),
        "expected InsufficientDeveloperBalance, got {:?}",
        result
    );

    assert_eq!(client.registered_count(), 0);
    assert!(!client.is_offering_registered(&oid));
}
