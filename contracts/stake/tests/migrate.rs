//! Focused integration tests for `contracts/stake/src/migrate.rs` (#885).
//!
//! These tests exercise the on-chain migration entrypoints via generated
//! Soroban clients to verify correctness, security, and idempotency
//! properties of the stake migration stub.

extern crate std;

use callora_stake::migrate::{
    CalloraStakeMigrate, CalloraStakeMigrateClient, CurrentStake, LegacyStake, StorageKey,
};
use soroban_sdk::{
    testutils::{Address as _, Events as _},
    Address, BytesN, Env, Symbol,
};

// ─── Test helpers ─────────────────────────────────────────────────────────────

/// Register the contract, initialise at version `initial_version`, and
/// pre-stage legacy stake data.  Returns `(admin, contract_address)`.
fn setup(env: &Env, total_staked: i128, last_checkpoint: u32) -> (Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract = env.register(CalloraStakeMigrate, ());
    let client = CalloraStakeMigrateClient::new(env, &contract);
    client.init(&admin, &1);

    // Pre-stage legacy data as the previous deployment would have done.
    env.as_contract(&contract, || {
        env.storage().instance().set(
            &StorageKey::Legacy,
            &LegacyStake {
                total_staked,
                last_checkpoint,
            },
        );
    });

    (admin, contract)
}

// ─── Initialisation tests ─────────────────────────────────────────────────────

#[test]
fn init_stores_admin_and_version() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract = env.register(CalloraStakeMigrate, ());
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    client.init(&admin, &7);
    assert_eq!(client.version(), Some(7));
}

#[test]
fn init_rejects_double_initialisation() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract = env.register(CalloraStakeMigrate, ());
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    client.init(&admin, &1);
    let result = client.try_init(&admin, &2);
    assert!(result.is_err());
}

#[test]
fn init_requires_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract = env.register(CalloraStakeMigrate, ());
    let client = CalloraStakeMigrateClient::new(&env, &contract);
    // No mock_all_auths — caller has not authorised.
    env.set_auths(&[]);
    let result = client.try_init(&admin, &0);
    assert!(result.is_err());
}

// ─── Migration tests ──────────────────────────────────────────────────────────

#[test]
fn migrate_reshapes_data_and_advances_version() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 1_000, 42);
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    let result = client.migrate(&admin, &1, &2);
    assert_eq!(result.total_staked, 1_000);
    assert_eq!(result.last_checkpoint, 42);
    assert_eq!(result.reserve, 0);
    assert_eq!(client.version(), Some(2));
    assert_eq!(client.get_current(), Some(result));
}

#[test]
fn migrate_preserves_zero_stake() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 0, 100);
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    let result = client.migrate(&admin, &1, &2);
    assert_eq!(result.total_staked, 0);
    assert_eq!(result.last_checkpoint, 100);
    assert_eq!(result.reserve, 0);
}

#[test]
fn migrate_preserves_large_stake() {
    let env = Env::default();
    let large = i128::MAX;
    let (admin, contract) = setup(&env, large, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    let result = client.migrate(&admin, &1, &2);
    assert_eq!(result.total_staked, large);
    assert_eq!(client.version(), Some(2));
}

#[test]
fn migrate_reserve_is_always_zero_after_reshape() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 5_000, 999);
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    let result = client.migrate(&admin, &1, &2);
    assert_eq!(result.reserve, 0, "reserve must be zero-initialised");
}

#[test]
fn migrate_cannot_be_replayed() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 100, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    client.migrate(&admin, &1, &2);
    // Version is now 2; attempting 2→3 still fails because Current is set.
    let result = client.try_migrate(&admin, &2, &3);
    assert!(result.is_err());
}

#[test]
fn migrate_rejects_wrong_expected_version() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 100, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    // Stored version is 1 — supplying 0 must be rejected.
    let result = client.try_migrate(&admin, &0, &1);
    assert!(result.is_err());
}

#[test]
fn migrate_rejects_non_incrementing_target() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 100, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    // target must be exactly expected + 1; skipping to 3 is rejected.
    let result = client.try_migrate(&admin, &1, &3);
    assert!(result.is_err());
}

#[test]
fn migrate_rejects_negative_total_staked() {
    let env = Env::default();
    let (admin, contract) = setup(&env, -1, 42);
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    let result = client.try_migrate(&admin, &1, &2);
    assert!(result.is_err());
}

#[test]
fn migrate_requires_admin_authorisation() {
    let env = Env::default();
    let (_admin, contract) = setup(&env, 100, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);
    let outsider = Address::generate(&env);

    // outsider is not the admin — must be rejected.
    let result = client.try_migrate(&outsider, &1, &2);
    assert!(result.is_err());
}

#[test]
fn migrate_requires_auth_on_caller() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 100, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    env.set_auths(&[]);
    let result = client.try_migrate(&admin, &1, &2);
    assert!(result.is_err());
}

#[test]
fn migrate_fails_when_legacy_state_missing() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract = env.register(CalloraStakeMigrate, ());
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    client.init(&admin, &1);
    // No legacy data pre-staged — migrate must fail.
    let result = client.try_migrate(&admin, &1, &2);
    assert!(result.is_err());
}

#[test]
fn migrate_fails_when_not_initialised() {
    let env = Env::default();
    env.mock_all_auths();
    let caller = Address::generate(&env);
    let contract = env.register(CalloraStakeMigrate, ());
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    let result = client.try_migrate(&caller, &0, &1);
    assert!(result.is_err());
}

// ─── Upgrade-guard tests ──────────────────────────────────────────────────────

#[test]
fn authorize_upgrade_stores_hash_for_version() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 100, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);
    let hash = BytesN::from_array(&env, &[0xAA; 32]);

    client.authorize_upgrade(&admin, &1, &hash);
    assert!(client.is_upgrade_authorised(&hash));
}

#[test]
fn is_upgrade_authorised_rejects_wrong_hash() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 100, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);
    let good_hash = BytesN::from_array(&env, &[0xAA; 32]);
    let bad_hash = BytesN::from_array(&env, &[0xBB; 32]);

    client.authorize_upgrade(&admin, &1, &good_hash);
    assert!(client.is_upgrade_authorised(&good_hash));
    assert!(!client.is_upgrade_authorised(&bad_hash));
}

#[test]
fn authorize_upgrade_rejects_wrong_version() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 100, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);
    let hash = BytesN::from_array(&env, &[0xAA; 32]);

    // Stored version is 1; requesting version 2 must fail.
    let result = client.try_authorize_upgrade(&admin, &2, &hash);
    assert!(result.is_err());
}

#[test]
fn authorize_upgrade_requires_admin() {
    let env = Env::default();
    let (_admin, contract) = setup(&env, 100, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);
    let outsider = Address::generate(&env);
    let hash = BytesN::from_array(&env, &[0xAA; 32]);

    let result = client.try_authorize_upgrade(&outsider, &1, &hash);
    assert!(result.is_err());
}

#[test]
fn authorize_upgrade_requires_auth_on_caller() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 100, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);
    let hash = BytesN::from_array(&env, &[0xAA; 32]);

    env.set_auths(&[]);
    let result = client.try_authorize_upgrade(&admin, &1, &hash);
    assert!(result.is_err());
}

#[test]
fn is_upgrade_authorised_false_before_authorisation() {
    let env = Env::default();
    let (_admin, contract) = setup(&env, 100, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);
    let hash = BytesN::from_array(&env, &[0xAA; 32]);

    assert!(!client.is_upgrade_authorised(&hash));
}

#[test]
fn is_upgrade_authorised_false_when_not_initialised() {
    let env = Env::default();
    let contract = env.register(CalloraStakeMigrate, ());
    let client = CalloraStakeMigrateClient::new(&env, &contract);
    let hash = BytesN::from_array(&env, &[0xAA; 32]);

    assert!(!client.is_upgrade_authorised(&hash));
}

// ─── View tests ───────────────────────────────────────────────────────────────

#[test]
fn get_current_is_none_before_migration() {
    let env = Env::default();
    let (_admin, contract) = setup(&env, 100, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    assert_eq!(client.get_current(), None);
}

#[test]
fn version_returns_none_before_init() {
    let env = Env::default();
    let contract = env.register(CalloraStakeMigrate, ());
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    assert_eq!(client.version(), None);
}

// ─── Full lifecycle test ──────────────────────────────────────────────────────

#[test]
fn full_lifecycle_init_migrate_authorize() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 2_500_000, 12345);
    let client = CalloraStakeMigrateClient::new(&env, &contract);
    let wasm_hash = BytesN::from_array(&env, &[0xCC; 32]);

    // 1. Initialised at version 1 (done by setup).
    assert_eq!(client.version(), Some(1));

    // 2. Migrate legacy → current.
    let current = client.migrate(&admin, &1, &2);
    assert_eq!(current.total_staked, 2_500_000);
    assert_eq!(current.last_checkpoint, 12345);
    assert_eq!(current.reserve, 0);
    assert_eq!(client.version(), Some(2));
    assert_eq!(client.get_current(), Some(current));

    // 3. Authorise upgrade for post-migration version.
    client.authorize_upgrade(&admin, &2, &wasm_hash);
    assert!(client.is_upgrade_authorised(&wasm_hash));
}

// ─── Event emission tests ─────────────────────────────────────────────────────

#[test]
fn migrate_emits_stake_migrated_event() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 200, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    client.migrate(&admin, &1, &2);

    let all_events = env.events().all();
    let expected_topic = Symbol::new(&env, "stake_migrated");
    let has_event = all_events
        .into_iter()
        .any(|(_addr, topics, _data)| topics.contains(&expected_topic.to_val()));
    assert!(has_event, "stake_migrated event must be emitted");
}

#[test]
fn authorize_upgrade_emits_upg_authorised_event() {
    let env = Env::default();
    let (admin, contract) = setup(&env, 200, 1);
    let client = CalloraStakeMigrateClient::new(&env, &contract);
    let hash = BytesN::from_array(&env, &[0xDD; 32]);

    client.authorize_upgrade(&admin, &1, &hash);

    let all_events = env.events().all();
    let expected_topic = Symbol::new(&env, "upg_authorised");
    let has_event = all_events
        .into_iter()
        .any(|(_addr, topics, _data)| topics.contains(&expected_topic.to_val()));
    assert!(has_event, "upg_authorised event must be emitted");
}

// ─── Timestamp / checkpoint preservation ─────────────────────────────────────

#[test]
fn migrate_preserves_exact_checkpoint() {
    let env = Env::default();
    let checkpoint = u32::MAX;
    let (admin, contract) = setup(&env, 500, checkpoint);
    let client = CalloraStakeMigrateClient::new(&env, &contract);

    let result = client.migrate(&admin, &1, &2);
    assert_eq!(result.last_checkpoint, checkpoint);
}
