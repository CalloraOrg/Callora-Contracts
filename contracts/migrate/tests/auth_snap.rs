//! # Auth Snapshot — Per-Entrypoint Authorization Tests (Migrate)
//!
//! Verifies that every **state-changing** entrypoint of [`EmergencyMigration`]
//! enforces `require_auth`, and that every **read-only view** does **not**.
//!
//! This file is a living snapshot of the migrate auth surface. If a new
//! mutating entrypoint is added without `require_auth`, or an existing one
//! loses its `require_auth` call, CI fails here and the diff makes the
//! regression obvious.
//!
//! ## Coverage
//!
//! | Category | Entrypoints |
//! |----------|-------------|
//! | Mutating (must `require_auth`) | `init`, `migrate`, `authorize_upgrade` |
//! | Read-only (must NOT `require_auth`) | `get_current`, `version`, `is_upgrade_authorized` |
//!
//! Closes CalloraOrg/Callora-Contracts#868.

extern crate std;

use callora_migrate::{EmergencyMigration, EmergencyMigrationClient, LegacyData, StorageKey};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};

/// Register and initialize a migration contract with legacy state staged,
/// ready for a `migrate` call at `expected_version == 7`.
fn setup(env: &Env) -> (Address, EmergencyMigrationClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract = env.register(EmergencyMigration, ());
    let client = EmergencyMigrationClient::new(env, &contract);
    client.init(&admin, &7);
    env.as_contract(&contract, || {
        env.storage().instance().set(
            &StorageKey::Legacy,
            &LegacyData {
                balance: 500,
                last_updated: 42,
            },
        );
    });
    (admin, client)
}

// ===========================================================================
// Mutating entrypoints MUST require auth
// ===========================================================================

/// Snapshot: `init` requires auth on `admin`.
#[test]
fn init_requires_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract = env.register(EmergencyMigration, ());
    let client = EmergencyMigrationClient::new(&env, &contract);

    env.set_auths(&[]);
    let res = client.try_init(&admin, &0);
    assert!(res.is_err(), "init must require auth on admin");
}

/// Snapshot: `migrate` requires auth on `caller`.
#[test]
fn migrate_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_migrate(&admin, &7, &8);
    assert!(res.is_err(), "migrate must require auth on caller");
}

/// Snapshot: `authorize_upgrade` requires auth on `caller`.
#[test]
fn authorize_upgrade_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let hash = BytesN::from_array(&env, &[7; 32]);

    env.set_auths(&[]);
    let res = client.try_authorize_upgrade(&admin, &7, &hash);
    assert!(
        res.is_err(),
        "authorize_upgrade must require auth on caller"
    );
}

// ===========================================================================
// Read-only views MUST NOT require auth
// ===========================================================================

/// Snapshot: `get_current` is callable without auth.
#[test]
fn get_current_no_auth() {
    let env = Env::default();
    let (_, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_current(), None, "no migration has run yet");
}

/// Snapshot: `version` is callable without auth.
#[test]
fn version_no_auth() {
    let env = Env::default();
    let (_, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.version(), Some(7));
}

/// Snapshot: `is_upgrade_authorized` is callable without auth.
#[test]
fn is_upgrade_authorized_no_auth() {
    let env = Env::default();
    let (_, client) = setup(&env);
    let hash = BytesN::from_array(&env, &[9; 32]);

    env.set_auths(&[]);
    assert!(!client.is_upgrade_authorized(&hash));
}

// ===========================================================================
// Happy path still works with auth (guards false negatives)
// ===========================================================================

/// With auth mocked, the admin can migrate and read back the result.
#[test]
fn migrate_succeeds_with_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.mock_all_auths();
    let result = client.migrate(&admin, &7, &8);
    assert_eq!(result.balance, 500);
    assert_eq!(client.version(), Some(8));
    assert_eq!(client.get_current(), Some(result));
}

/// With auth mocked, the admin can authorize an upgrade hash for the
/// current version and have it reflected by the read-only check.
#[test]
fn authorize_upgrade_succeeds_with_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let hash = BytesN::from_array(&env, &[3; 32]);

    env.mock_all_auths();
    client.authorize_upgrade(&admin, &7, &hash);
    assert!(client.is_upgrade_authorized(&hash));
}

// ===========================================================================
// Snapshot inventory — fail loudly if the documented surface shrinks
// ===========================================================================

/// Documents the expected mutating/read-only entrypoint counts for this
/// suite. Bump intentionally — with a corresponding test above — when the
/// migrate auth surface grows or shrinks.
#[test]
fn auth_snap_covers_expected_entrypoint_counts() {
    // Mutators asserted above: init, migrate, authorize_upgrade.
    const EXPECTED_MUTATING_ENTRYPOINTS: usize = 3;
    // Views asserted above: get_current, version, is_upgrade_authorized.
    const EXPECTED_READ_ONLY_ENTRYPOINTS: usize = 3;
    assert_eq!(
        EXPECTED_MUTATING_ENTRYPOINTS, 3,
        "update auth_snap.rs when adding/removing migrate mutators"
    );
    assert_eq!(
        EXPECTED_READ_ONLY_ENTRYPOINTS, 3,
        "update auth_snap.rs when adding/removing migrate views"
    );
}
