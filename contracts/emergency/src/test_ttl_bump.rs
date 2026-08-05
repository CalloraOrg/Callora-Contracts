#![cfg(test)]
//! Focused tests for the TTL-bump-on-read pattern in the emergency contract
//! (issue #709).
//!
//! Every hot read path (`capabilities`, `get_current`, `version`,
//! `is_upgrade_authorised`) must call
//! `env.storage().instance().extend_ttl(INSTANCE_BUMP_THRESHOLD,
//! INSTANCE_BUMP_AMOUNT)` so that a frequently-queried contract does not
//! archive due to infrequent writes.
//!
//! # Test strategy
//! 1. Register and initialise the contract (write-path bump sets initial TTL).
//! 2. Advance the ledger sequence to bring instance TTL *below* the bump
//!    threshold (i.e. `INSTANCE_BUMP_AMOUNT - INSTANCE_BUMP_THRESHOLD + 10`
//!    ledgers past init).
//! 3. Assert that `get_ttl()` is now below `INSTANCE_BUMP_THRESHOLD`.
//! 4. Call the hot read path under test.
//! 5. Assert that `get_ttl()` is back to `INSTANCE_BUMP_AMOUNT`.

extern crate std;

use crate::migrate::{
    EmergencyMigrate, EmergencyMigrateClient, LegacyEmergency, StorageKey,
    INSTANCE_BUMP_AMOUNT, INSTANCE_BUMP_THRESHOLD, LEDGERS_PER_DAY,
    PERSISTENT_BUMP_AMOUNT, PERSISTENT_BUMP_THRESHOLD,
};
use crate::{CalloraEmergency, CalloraEmergencyClient};
use soroban_sdk::testutils::storage::Instance as _;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, BytesN, Env};

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Register and initialise `EmergencyMigrate` at version 1, pre-staging
/// legacy data.  Returns `(admin, contract_address)`.
fn setup_migrate(env: &Env) -> (Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract = env.register(EmergencyMigrate, ());
    let client = EmergencyMigrateClient::new(env, &contract);
    client.init(&admin, &1);

    env.as_contract(&contract, || {
        env.storage().instance().set(
            &StorageKey::Legacy,
            &LegacyEmergency {
                balance: 1_000,
                last_updated: 42,
            },
        );
    });

    (admin, contract)
}

/// Advance the ledger so that instance TTL drops below `INSTANCE_BUMP_THRESHOLD`.
fn drain_instance_ttl(env: &Env) {
    let seq = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq + INSTANCE_BUMP_AMOUNT - INSTANCE_BUMP_THRESHOLD + 10);
}

// ---------------------------------------------------------------------------
// TTL-constant smoke tests
// ---------------------------------------------------------------------------

#[test]
fn ttl_constants_have_expected_values() {
    // 17,280 ledgers/day at 5 s/ledger
    assert_eq!(LEDGERS_PER_DAY, 17_280);
    // threshold = 30 days, amount = 60 days
    assert_eq!(INSTANCE_BUMP_THRESHOLD, LEDGERS_PER_DAY * 30);
    assert_eq!(INSTANCE_BUMP_AMOUNT, LEDGERS_PER_DAY * 60);
    // persistent mirrors instance
    assert_eq!(PERSISTENT_BUMP_THRESHOLD, INSTANCE_BUMP_THRESHOLD);
    assert_eq!(PERSISTENT_BUMP_AMOUNT, INSTANCE_BUMP_AMOUNT);
}

// ---------------------------------------------------------------------------
// capabilities() — views module / CalloraEmergency facade
// ---------------------------------------------------------------------------

#[test]
fn capabilities_bumps_instance_ttl() {
    let env = Env::default();
    // Register the CalloraEmergency facade (no init needed — pure bitmap).
    let contract = env.register(CalloraEmergency, ());
    let client = CalloraEmergencyClient::new(&env, &contract);

    // Advance past the threshold to reduce TTL.
    drain_instance_ttl(&env);

    let ttl_before = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert!(
        ttl_before < INSTANCE_BUMP_THRESHOLD,
        "pre-condition: TTL should be below threshold before call"
    );

    // Hot read — should bump.
    let caps = client.capabilities();
    assert_ne!(caps, 0, "capabilities() must return a non-zero bitmap");

    let ttl_after = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert_eq!(
        ttl_after, INSTANCE_BUMP_AMOUNT,
        "capabilities() must restore instance TTL to INSTANCE_BUMP_AMOUNT"
    );
}

#[test]
fn capabilities_is_idempotent_across_calls() {
    let env = Env::default();
    let contract = env.register(CalloraEmergency, ());
    let client = CalloraEmergencyClient::new(&env, &contract);

    // Multiple calls must return the same value and not panic.
    assert_eq!(client.capabilities(), client.capabilities());
}

// ---------------------------------------------------------------------------
// get_current() — EmergencyMigrate hot read path
// ---------------------------------------------------------------------------

#[test]
fn get_current_bumps_instance_ttl_before_migration() {
    let env = Env::default();
    let (_admin, contract) = setup_migrate(&env);
    let client = EmergencyMigrateClient::new(&env, &contract);

    drain_instance_ttl(&env);

    let ttl_before = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert!(ttl_before < INSTANCE_BUMP_THRESHOLD);

    // Pre-migration: returns None but MUST still bump.
    let result = client.get_current();
    assert_eq!(result, None);

    let ttl_after = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert_eq!(
        ttl_after, INSTANCE_BUMP_AMOUNT,
        "get_current() must restore instance TTL to INSTANCE_BUMP_AMOUNT"
    );
}

#[test]
fn get_current_bumps_instance_ttl_after_migration() {
    let env = Env::default();
    let (admin, contract) = setup_migrate(&env);
    let client = EmergencyMigrateClient::new(&env, &contract);

    // Run the migration so Current is populated.
    let migrated = client.migrate(&admin, &1, &2);
    assert_eq!(migrated.balance, 1_000);

    drain_instance_ttl(&env);

    let ttl_before = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert!(ttl_before < INSTANCE_BUMP_THRESHOLD);

    let result = client.get_current();
    assert!(result.is_some(), "should return migrated state");

    let ttl_after = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert_eq!(
        ttl_after, INSTANCE_BUMP_AMOUNT,
        "get_current() must restore instance TTL to INSTANCE_BUMP_AMOUNT after migration"
    );
}

// ---------------------------------------------------------------------------
// version() — EmergencyMigrate hot read path
// ---------------------------------------------------------------------------

#[test]
fn version_bumps_instance_ttl() {
    let env = Env::default();
    let (_admin, contract) = setup_migrate(&env);
    let client = EmergencyMigrateClient::new(&env, &contract);

    drain_instance_ttl(&env);

    let ttl_before = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert!(ttl_before < INSTANCE_BUMP_THRESHOLD);

    let ver = client.version();
    assert_eq!(ver, Some(1));

    let ttl_after = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert_eq!(
        ttl_after, INSTANCE_BUMP_AMOUNT,
        "version() must restore instance TTL to INSTANCE_BUMP_AMOUNT"
    );
}

#[test]
fn version_bumps_instance_ttl_before_init() {
    let env = Env::default();
    // Uninitialised contract — version() returns None but must still bump.
    let contract = env.register(EmergencyMigrate, ());
    let client = EmergencyMigrateClient::new(&env, &contract);

    drain_instance_ttl(&env);

    let result = client.version();
    assert_eq!(result, None);

    let ttl_after = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert_eq!(
        ttl_after, INSTANCE_BUMP_AMOUNT,
        "version() must bump TTL even when contract is uninitialised"
    );
}

// ---------------------------------------------------------------------------
// is_upgrade_authorised() — EmergencyMigrate hot read path
// ---------------------------------------------------------------------------

#[test]
fn is_upgrade_authorised_bumps_instance_ttl_when_false() {
    let env = Env::default();
    let (_admin, contract) = setup_migrate(&env);
    let client = EmergencyMigrateClient::new(&env, &contract);
    let hash = BytesN::from_array(&env, &[0xAA; 32]);

    drain_instance_ttl(&env);

    let ttl_before = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert!(ttl_before < INSTANCE_BUMP_THRESHOLD);

    // No authorisation recorded yet — returns false but must still bump.
    assert!(!client.is_upgrade_authorised(&hash));

    let ttl_after = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert_eq!(
        ttl_after, INSTANCE_BUMP_AMOUNT,
        "is_upgrade_authorised() must restore instance TTL even when returning false"
    );
}

#[test]
fn is_upgrade_authorised_bumps_instance_ttl_when_true() {
    let env = Env::default();
    let (admin, contract) = setup_migrate(&env);
    let client = EmergencyMigrateClient::new(&env, &contract);
    let hash = BytesN::from_array(&env, &[0xBB; 32]);

    client.authorize_upgrade(&admin, &1, &hash);

    drain_instance_ttl(&env);

    let ttl_before = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert!(ttl_before < INSTANCE_BUMP_THRESHOLD);

    assert!(client.is_upgrade_authorised(&hash));

    let ttl_after = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert_eq!(
        ttl_after, INSTANCE_BUMP_AMOUNT,
        "is_upgrade_authorised() must restore instance TTL when returning true"
    );
}

#[test]
fn is_upgrade_authorised_bumps_ttl_when_uninitialised() {
    let env = Env::default();
    // Completely fresh contract — returns false, must still bump.
    let contract = env.register(EmergencyMigrate, ());
    let client = EmergencyMigrateClient::new(&env, &contract);
    let hash = BytesN::from_array(&env, &[0xCC; 32]);

    drain_instance_ttl(&env);

    assert!(!client.is_upgrade_authorised(&hash));

    let ttl_after = env.as_contract(&contract, || env.storage().instance().get_ttl());
    assert_eq!(
        ttl_after, INSTANCE_BUMP_AMOUNT,
        "is_upgrade_authorised() must bump TTL on uninitialised contract"
    );
}

// ---------------------------------------------------------------------------
// Archival-prevention end-to-end scenario
// ---------------------------------------------------------------------------

/// Verify that repeated view-only polling keeps the contract alive well past
/// the original write-path TTL window.
///
/// 1. Init (write-path bump → `INSTANCE_BUMP_AMOUNT` ledgers TTL).
/// 2. Advance by `INSTANCE_BUMP_AMOUNT - 1` (almost expired).
/// 3. Call `version()` — read-path bump resets TTL to `INSTANCE_BUMP_AMOUNT`.
/// 4. Advance by another `INSTANCE_BUMP_THRESHOLD + 5` (past original expiry).
/// 5. Call `get_current()` — must succeed (contract still alive).
#[test]
fn repeated_view_calls_prevent_archival() {
    let env = Env::default();
    let (admin, contract) = setup_migrate(&env);
    let client = EmergencyMigrateClient::new(&env, &contract);

    // Migrate so get_current has something to return.
    client.migrate(&admin, &1, &2);

    let seq_init = env.ledger().sequence();

    // Advance to just before original expiry.
    env.ledger()
        .set_sequence_number(seq_init + INSTANCE_BUMP_AMOUNT - 1);

    // Read-path bump: version() resets TTL.
    assert_eq!(client.version(), Some(2));

    // Advance past the *original* TTL deadline.
    let seq_after_bump = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq_after_bump + INSTANCE_BUMP_THRESHOLD + 5);

    // Contract must still be alive — read-path bump saved it.
    let current = client.get_current();
    assert!(
        current.is_some(),
        "contract archived despite read-path TTL bump"
    );
    assert_eq!(current.unwrap().balance, 1_000);
}
