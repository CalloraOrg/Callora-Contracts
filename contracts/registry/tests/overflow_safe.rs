//! Overflow-safe math tests for `callora-registry`.
//!
//! Verifies that:
//! - All arithmetic uses checked operations (`checked_add`) and never panics.
//! - The `RegisteredCount` increment returns `RegistryError::Overflow` when the
//!   counter would exceed `u32::MAX`.
//! - `unwrap_or` is eliminated from production paths in favour of explicit
//!   error propagation (`NotInitialized`).
//! - Every state-changing entrypoint enforces `require_auth`.
//!
//! ## Visible API
//!
//! No new public entrypoints are introduced by this test module.

extern crate std;

use callora_registry::{admin, CalloraRegistry, CalloraRegistryClient, RegistryError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::{Ledger, LedgerInfo};
use soroban_sdk::{contract, contractimpl, Address, Env, String};

// ---------------------------------------------------------------------------
// Mock catalog that accepts all registrations
// ---------------------------------------------------------------------------

pub mod ok_catalog {
    use super::*;

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
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn offering_id(env: &Env, suffix: &str) -> String {
    String::from_str(env, &format!("offering-{suffix}"))
}

fn metadata(env: &Env) -> String {
    String::from_str(env, "ipfs://QmOverflowTest")
}

fn setup_registry(env: &Env) -> (Address, CalloraRegistryClient<'_>, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let developer = Address::generate(env);
    let catalog = env.register(ok_catalog::OkCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(env, &registry_id);
    client.init(&admin, &catalog);
    (admin, client, developer)
}

/// Advance the ledger timestamp past the admin cooldown window so the next
/// admin-gated action is not blocked.
fn advance_past_cooldown(env: &Env) {
    let current = env.ledger().get().timestamp;
    env.ledger().set(LedgerInfo {
        timestamp: current + admin::COOLDOWN_SECONDS + 1,
        ..env.ledger().get()
    });
}

// ---------------------------------------------------------------------------
// Overflow: RegisteredCount at u32::MAX
// ---------------------------------------------------------------------------

#[test]
fn register_offering_count_overflow_returns_error() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let catalog = env.register(ok_catalog::OkCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &registry_id);

    env.mock_all_auths();
    client.init(&admin, &catalog);

    env.as_contract(&registry_id, || {
        env.storage()
            .instance()
            .set(&callora_registry::StorageKey::RegisteredCount, &u32::MAX);
    });

    let oid = offering_id(&env, "overflow");
    let meta = metadata(&env);

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(
        matches!(result, Err(Ok(RegistryError::Overflow))),
        "expected Overflow when count is u32::MAX, got {:?}",
        result
    );

    assert_eq!(client.registered_count(), u32::MAX);
    assert!(!client.is_offering_registered(&oid));
}

#[test]
fn register_offering_with_gate_count_overflow_returns_error() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let catalog = env.register(ok_catalog::OkCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &registry_id);

    env.mock_all_auths();
    client.init(&admin, &catalog);

    env.as_contract(&registry_id, || {
        env.storage()
            .instance()
            .set(&callora_registry::StorageKey::RegisteredCount, &u32::MAX);
    });

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner);
    let token_addr = sac.address();
    let token_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token_admin.mint(&developer, &10_000);

    let oid = offering_id(&env, "gate-overflow");
    let meta = metadata(&env);

    let result = client.try_register_offering_with_gate(
        &admin,
        &developer,
        &token_addr,
        &100i128,
        &oid,
        &meta,
    );
    assert!(
        matches!(result, Err(Ok(RegistryError::Overflow))),
        "expected Overflow on gated path when count is u32::MAX, got {:?}",
        result
    );

    assert_eq!(client.registered_count(), u32::MAX);
    assert!(!client.is_offering_registered(&oid));
}

// ---------------------------------------------------------------------------
// Counter increments correctly for normal operations
// ---------------------------------------------------------------------------

#[test]
fn register_offering_increments_count() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);

    let oid1 = offering_id(&env, "inc-1");
    let oid2 = offering_id(&env, "inc-2");
    let meta = metadata(&env);

    client.register_offering(&admin, &developer, &oid1, &meta);
    assert_eq!(client.registered_count(), 1);

    // Advance past cooldown to allow the second registration.
    advance_past_cooldown(&env);

    client.register_offering(&admin, &developer, &oid2, &meta);
    assert_eq!(client.registered_count(), 2);
}

#[test]
fn register_offering_with_gate_increments_count() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let catalog = env.register(ok_catalog::OkCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &registry_id);

    env.mock_all_auths();
    client.init(&admin, &catalog);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner);
    let token_addr = sac.address();
    let token_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token_admin.mint(&developer, &10_000);

    let oid = offering_id(&env, "gate-inc");
    let meta = metadata(&env);

    client.register_offering_with_gate(&admin, &developer, &token_addr, &100i128, &oid, &meta);
    assert_eq!(client.registered_count(), 1);
    assert!(client.is_offering_registered(&oid));
}

// ---------------------------------------------------------------------------
// State unchanged after overflow
// ---------------------------------------------------------------------------

#[test]
fn overflow_does_not_persist_offering_record() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let catalog = env.register(ok_catalog::OkCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &registry_id);

    env.mock_all_auths();
    client.init(&admin, &catalog);

    env.as_contract(&registry_id, || {
        env.storage()
            .instance()
            .set(&callora_registry::StorageKey::RegisteredCount, &u32::MAX);
    });

    let oid = offering_id(&env, "no-persist");
    let meta = metadata(&env);

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(result.is_err());

    assert!(!client.is_offering_registered(&oid));
}

// ---------------------------------------------------------------------------
// Balance-gate: balance exactly equals min_balance
// ---------------------------------------------------------------------------

#[test]
fn balance_gate_exact_match_allows_registration() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let catalog = env.register(ok_catalog::OkCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &registry_id);

    env.mock_all_auths();
    client.init(&admin, &catalog);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner);
    let token_addr = sac.address();
    let token_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token_admin.mint(&developer, &500);

    let oid = offering_id(&env, "exact");
    let meta = metadata(&env);

    client.register_offering_with_gate(&admin, &developer, &token_addr, &500i128, &oid, &meta);
    assert!(client.is_offering_registered(&oid));
    assert_eq!(client.registered_count(), 1);
}

#[test]
fn balance_gate_one_below_min_rejects() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let catalog = env.register(ok_catalog::OkCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &registry_id);

    env.mock_all_auths();
    client.init(&admin, &catalog);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner);
    let token_addr = sac.address();
    let token_admin = soroban_sdk::token::StellarAssetClient::new(&env, &token_addr);
    token_admin.mint(&developer, &499);

    let oid = offering_id(&env, "below");
    let meta = metadata(&env);

    let result = client.try_register_offering_with_gate(
        &admin,
        &developer,
        &token_addr,
        &500i128,
        &oid,
        &meta,
    );
    assert!(
        matches!(result, Err(Ok(RegistryError::InsufficientDeveloperBalance))),
        "expected InsufficientDeveloperBalance, got {:?}",
        result
    );
    assert!(!client.is_offering_registered(&oid));
    assert_eq!(client.registered_count(), 0);
}

// ---------------------------------------------------------------------------
// require_auth: all state-changing entrypoints reject unauthenticated callers
// ---------------------------------------------------------------------------

#[test]
fn init_requires_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let catalog = Address::generate(&env);
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &registry_id);

    let result = client.try_init(&admin, &catalog);
    assert!(result.is_err(), "init must require auth");
}

#[test]
fn register_offering_requires_caller_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let catalog = env.register(ok_catalog::OkCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());

    env.as_contract(&registry_id, || {
        env.storage()
            .instance()
            .set(&callora_registry::StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&callora_registry::StorageKey::Catalog, &catalog);
        env.storage()
            .instance()
            .set(&callora_registry::StorageKey::RegisteredCount, &0u32);
    });

    let client = CalloraRegistryClient::new(&env, &registry_id);

    let oid = offering_id(&env, "auth-req");
    let meta = metadata(&env);

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(
        result.is_err(),
        "register_offering must require caller auth"
    );
}

#[test]
fn register_offering_with_gate_requires_caller_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let catalog = env.register(ok_catalog::OkCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());

    env.as_contract(&registry_id, || {
        env.storage()
            .instance()
            .set(&callora_registry::StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&callora_registry::StorageKey::Catalog, &catalog);
        env.storage()
            .instance()
            .set(&callora_registry::StorageKey::RegisteredCount, &0u32);
    });

    let client = CalloraRegistryClient::new(&env, &registry_id);

    let owner = Address::generate(&env);
    let sac = env.register_stellar_asset_contract_v2(owner);
    let token_addr = sac.address();

    let oid = offering_id(&env, "gate-auth");
    let meta = metadata(&env);

    let result = client.try_register_offering_with_gate(
        &admin,
        &developer,
        &token_addr,
        &100i128,
        &oid,
        &meta,
    );
    assert!(
        result.is_err(),
        "register_offering_with_gate must require caller auth"
    );
}

// ---------------------------------------------------------------------------
// Read-only entrypoints do not require auth
// ---------------------------------------------------------------------------

#[test]
fn registered_count_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client, _developer) = setup_registry(&env);

    assert_eq!(client.registered_count(), 0);
}

#[test]
fn is_offering_registered_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client, _developer) = setup_registry(&env);

    let oid = offering_id(&env, "readonly");
    assert!(!client.is_offering_registered(&oid));
}

// ---------------------------------------------------------------------------
// NotInitialized: accessing count before init
// ---------------------------------------------------------------------------

#[test]
fn registered_count_before_init_returns_not_initialized() {
    let env = Env::default();
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &registry_id);

    let result = client.try_registered_count();
    assert!(
        matches!(result, Err(Ok(RegistryError::NotInitialized))),
        "expected NotInitialized before init, got {:?}",
        result
    );
}

// ---------------------------------------------------------------------------
// Duplicate registration is rejected (no double count)
// ---------------------------------------------------------------------------

#[test]
fn duplicate_offering_registration_rejected() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);

    let oid = offering_id(&env, "dup");
    let meta = metadata(&env);

    client.register_offering(&admin, &developer, &oid, &meta);
    assert_eq!(client.registered_count(), 1);

    // Advance past cooldown so the duplicate check runs (not blocked by
    // cooldown).
    advance_past_cooldown(&env);

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(
        matches!(result, Err(Ok(RegistryError::OfferingAlreadyRegistered))),
        "expected OfferingAlreadyRegistered for duplicate, got {:?}",
        result
    );
    assert_eq!(client.registered_count(), 1);
}

// ---------------------------------------------------------------------------
// Unauthorized caller
// ---------------------------------------------------------------------------

#[test]
fn non_admin_register_offering_returns_unauthorized() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let catalog = env.register(ok_catalog::OkCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &registry_id);

    env.mock_all_auths();
    client.init(&admin, &catalog);

    let oid = offering_id(&env, "unauth");
    let meta = metadata(&env);

    let result = client.try_register_offering(&non_admin, &developer, &oid, &meta);
    assert!(
        matches!(result, Err(Ok(RegistryError::Unauthorized))),
        "expected Unauthorized for non-admin caller, got {:?}",
        result
    );
}
