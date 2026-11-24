#![no_std]

//! Emergency migration contract for the GrantFox FWC26 campaign.
//!
//! This contract is intentionally small and conservative. It reshapes the
//! legacy emergency state into the current representation exactly once and
//! advances a monotonic migration version. The migration itself is guarded by
//! the configured administrator and cannot be replayed or skipped.
//!
//! # Visible API
//!
//! * [`EmergencyMigration::init`] configures the migration administrator and
//!   starting version.
//! * [`EmergencyMigration::migrate`] reshapes staged legacy data and advances
//!   the version by one.
//! * [`EmergencyMigration::authorize_upgrade`] records the version authorized
//!   for a subsequent contract upgrade.
//! * [`EmergencyMigration::get_current`] and [`EmergencyMigration::version`]
//!   are unauthenticated views.
//!
//! Legacy data is expected to have been written by the previous deployment
//! under [`StorageKey::Legacy`]. No public entrypoint is provided to write
//! that value, preventing an arbitrary caller from supplying migration input.
//!
//! # Error handling
//! All entrypoints return [`MigrationError`] instead of panicking with strings.
//! See [`errors`] for the full table of semantic variants and their codes.

pub mod errors;

pub use errors::MigrationError;

use callora_storage_migration::{
    AuthorizeOptions, MigrateOptions, MigrationKeys, StorageMigrationError, StubError,
};
use soroban_sdk::{contract, contractimpl, contracttype, Address, BytesN, Env};

/// Maximum `initial_version` accepted by `init`.
///
/// Values above this threshold are rejected with [`MigrationError::InvalidInitialVersion`]
/// to preserve headroom for future version increments.
const MAX_INITIAL_VERSION: u32 = u32::MAX - 1024;

/// Storage keys used by the migration contract.
#[derive(Clone)]
#[contracttype]
pub enum StorageKey {
    Admin,
    Version,
    Legacy,
    Current,
    AuthorizedUpgrade,
}

impl From<StubError> for MigrationError {
    fn from(err: StubError) -> Self {
        match err {
            StubError::NotInitialized => MigrationError::NotInitialized,
            StubError::AlreadyInitialized => MigrationError::AlreadyInitialized,
            StubError::Unauthorized => MigrationError::Unauthorized,
            StubError::LegacyStateMissing => MigrationError::LegacyStateMissing,
            StubError::InvalidLegacyBalance => MigrationError::InvalidLegacyBalance,
            StubError::VersionMismatch => MigrationError::VersionMismatch,
            StubError::InvalidTargetVersion => MigrationError::InvalidTargetVersion,
            StubError::AlreadyMigrated => MigrationError::AlreadyMigrated,
            StubError::UpgradeNotAuthorized => MigrationError::UpgradeNotAuthorized,
            StubError::Overflow => MigrationError::VersionOverflow,
            StubError::InvalidInitialVersion => MigrationError::InvalidInitialVersion,
            StubError::WasmHashZero => MigrationError::WasmHashZero,
        }
    }
}

const KEYS: MigrationKeys<StorageKey> = MigrationKeys::new(
    StorageKey::Admin,
    StorageKey::Version,
    StorageKey::Legacy,
    StorageKey::Current,
    StorageKey::AuthorizedUpgrade,
);

/// Data layout written by the legacy deployment.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct LegacyData {
    /// Aggregate emergency balance.
    pub balance: i128,
    /// Timestamp at which the legacy state was last changed.
    pub last_updated: u64,
}

/// Current data layout after the emergency migration.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct CurrentData {
    /// Aggregate emergency balance preserved from the legacy layout.
    pub balance: i128,
    /// Timestamp preserved from the legacy layout.
    pub last_updated: u64,
    /// Newly introduced reserve field, initialized to zero during reshape.
    pub reserved: i128,
}

/// Emergency data migration and upgrade guard.
#[contract]
pub struct EmergencyMigration;

#[contractimpl]
impl EmergencyMigration {
    /// Initialize the migration guard.
    pub fn init(env: Env, admin: Address, initial_version: u32) -> Result<(), MigrationError> {
        KEYS.init(&env, &admin, initial_version, Some(MAX_INITIAL_VERSION))
            .map_err(Into::into)
    }

    /// Reshape legacy emergency state and advance the migration version.
    pub fn migrate(
        env: Env,
        caller: Address,
        expected_version: u32,
        target_version: u32,
    ) -> Result<CurrentData, MigrationError> {
        KEYS.migrate(
            &env,
            &caller,
            expected_version,
            target_version,
            &MigrateOptions::default(),
            |legacy: LegacyData| {
                if legacy.balance < 0 {
                    return Err(StubError::InvalidLegacyBalance);
                }
                Ok(CurrentData {
                    balance: legacy.balance,
                    last_updated: legacy.last_updated,
                    reserved: 0,
                })
            },
        )
        .map_err(Into::into)
    }

    /// Authorize a contract upgrade for the current migration version.
    pub fn authorize_upgrade(
        env: Env,
        caller: Address,
        target_version: u32,
        wasm_hash: BytesN<32>,
    ) -> Result<(), MigrationError> {
        KEYS.authorize_upgrade(
            &env,
            &caller,
            target_version,
            &wasm_hash,
            &AuthorizeOptions {
                reject_zero_hash: true,
                event_topic: None,
            },
        )
        .map_err(Into::into)
    }

    /// Return the reshaped emergency state, if migration has completed.
    pub fn get_current(env: Env) -> Option<CurrentData> {
        KEYS.get_current(&env)
    }

    /// Return the stored migration version, if initialized.
    pub fn version(env: Env) -> Option<u32> {
        KEYS.version(&env)
    }

    /// Check whether a hash is authorized for the current migration version.
    pub fn is_upgrade_authorized(env: Env, wasm_hash: BytesN<32>) -> bool {
        KEYS.is_upgrade_authorized(&env, &wasm_hash)
    }

    /// Deterministic storage-migration validation that must run **before** the
    /// upgraded code is swapped in via `update_current_contract_wasm`.
    pub fn validate_before_upgrade(
        env: Env,
        caller: Address,
        target_version: u32,
        wasm_hash: BytesN<32>,
        allow_rollback: bool,
    ) -> Result<(), StorageMigrationError> {
        KEYS.validate_before_upgrade(&env, &caller, target_version, &wasm_hash, allow_rollback)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use callora_storage_migration::StorageMigrationValidator;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::Env;

    fn setup(env: &Env) -> (Address, Address) {
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
        (admin, contract)
    }

    #[test]
    fn migration_reshapes_data_and_advances_version() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);

        let result = client.migrate(&admin, &7, &8);
        assert_eq!(result.balance, 500);
        assert_eq!(result.last_updated, 42);
        assert_eq!(result.reserved, 0);
        assert_eq!(client.version(), Some(8));
        assert_eq!(client.get_current(), Some(result));
    }

    #[test]
    fn backward_compatible_storage_layout_read() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        let hash = BytesN::from_array(&env, &[0xAA; 32]);

        env.as_contract(&contract, || {
            env.storage().instance().set(&StorageKey::Admin, &admin);
            env.storage().instance().set(&StorageKey::Version, &7u32);
            env.storage().instance().set(
                &StorageKey::Current,
                &CurrentData {
                    balance: 500,
                    last_updated: 42,
                    reserved: 0,
                },
            );
            env.storage()
                .instance()
                .set(&StorageKey::AuthorizedUpgrade, &(7u32, hash.clone()));
        });

        assert_eq!(client.version(), Some(7));
        assert_eq!(
            client.get_current(),
            Some(CurrentData {
                balance: 500,
                last_updated: 42,
                reserved: 0,
            })
        );
        assert!(client.is_upgrade_authorized(&hash));
    }

    #[test]
    fn migration_cannot_be_replayed() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        client.migrate(&admin, &7, &8);
        // After migration, AlreadyMigrated must be raised (version is 8 so
        // expected=8,target=9 passes version checks but current data exists).
        assert!(client.try_migrate(&admin, &8, &9).is_err());
    }

    #[test]
    fn migration_rejects_wrong_version() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        assert!(client.try_migrate(&admin, &6, &7).is_err());
    }

    #[test]
    fn migration_rejects_negative_balance() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        env.as_contract(&contract, || {
            env.storage().instance().set(
                &StorageKey::Legacy,
                &LegacyData {
                    balance: -1,
                    last_updated: 42,
                },
            );
        });
        assert!(client.try_migrate(&admin, &7, &8).is_err());
    }

    #[test]
    fn migration_rejects_invalid_target_version() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        // target = expected + 2 (skip not allowed)
        assert!(client.try_migrate(&admin, &7, &9).is_err());
    }

    #[test]
    fn upgrade_guard_requires_current_version_and_matching_hash() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        let hash = BytesN::from_array(&env, &[9; 32]);

        // Wrong version
        assert!(client.try_authorize_upgrade(&admin, &6, &hash).is_err());

        // Correct version
        client.authorize_upgrade(&admin, &7, &hash);
        assert!(client.is_upgrade_authorized(&hash));
        assert!(!client.is_upgrade_authorized(&BytesN::from_array(&env, &[8; 32])));
    }

    #[test]
    fn authorize_upgrade_rejects_zero_hash() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
        assert!(client
            .try_authorize_upgrade(&admin, &7, &zero_hash)
            .is_err());
    }

    #[test]
    fn init_rejects_unreasonably_large_initial_version() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract = env.register(EmergencyMigration, ());
        let client = EmergencyMigrationClient::new(&env, &contract);
        // u32::MAX - 512 > MAX_INITIAL_VERSION (u32::MAX - 1024)
        assert!(client.try_init(&admin, &(u32::MAX - 512)).is_err());
    }

    #[test]
    fn init_rejects_double_init() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        assert!(client.try_init(&admin, &0).is_err());
    }

    #[test]
    fn state_changing_entrypoints_require_auth() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let contract = env.register(EmergencyMigration, ());
        let client = EmergencyMigrationClient::new(&env, &contract);
        env.set_auths(&[]);
        assert!(client.try_init(&admin, &0).is_err());
    }

    #[test]
    fn not_initialized_errors_before_init() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract = env.register(EmergencyMigration, ());
        let client = EmergencyMigrationClient::new(&env, &contract);

        // migrate before init
        assert!(client.try_migrate(&admin, &0, &1).is_err());

        // authorize_upgrade before init
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        assert!(client.try_authorize_upgrade(&admin, &0, &hash).is_err());
    }

    #[test]
    fn unauthorized_caller_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let intruder = Address::generate(&env);
        let contract = env.register(EmergencyMigration, ());
        let client = EmergencyMigrationClient::new(&env, &contract);
        client.init(&admin, &0);

        assert!(client.try_migrate(&intruder, &0, &1).is_err());
    }

    #[test]
    fn missing_legacy_state_is_rejected() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract = env.register(EmergencyMigration, ());
        let client = EmergencyMigrationClient::new(&env, &contract);
        // init without planting legacy data
        client.init(&admin, &0);
        assert!(client.try_migrate(&admin, &0, &1).is_err());
    }

    // ── MigrationError variant coverage ───────────────────────────────────

    #[test]
    fn error_variants_have_stable_codes() {
        assert_eq!(MigrationError::NotInitialized as u32, 1);
        assert_eq!(MigrationError::AlreadyInitialized as u32, 2);
        assert_eq!(MigrationError::Unauthorized as u32, 3);
        assert_eq!(MigrationError::LegacyStateMissing as u32, 4);
        assert_eq!(MigrationError::InvalidLegacyBalance as u32, 5);
        assert_eq!(MigrationError::VersionMismatch as u32, 6);
        assert_eq!(MigrationError::InvalidTargetVersion as u32, 7);
        assert_eq!(MigrationError::AlreadyMigrated as u32, 8);
        assert_eq!(MigrationError::UpgradeNotAuthorized as u32, 9);
        assert_eq!(MigrationError::VersionOverflow as u32, 10);
        assert_eq!(MigrationError::InvalidInitialVersion as u32, 11);
        assert_eq!(MigrationError::WasmHashZero as u32, 12);
        assert_eq!(MigrationError::MigrationDataCorrupted as u32, 13);
    }

    #[test]
    fn pre_upgrade_validation_records_authorization() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract = env.register(EmergencyMigration, ());
        let client = EmergencyMigrationClient::new(&env, &contract);
        client.init(&admin, &7);

        let hash = BytesN::from_array(&env, &[9u8; 32]);
        // Runs as a pre-upgrade gate and records an authorization guard.
        client.validate_before_upgrade(&admin, &7, &hash, &false);
        // The recorded authorization can be checked by off-chain tooling.
        let authorized = env.as_contract(&contract, || {
            StorageMigrationValidator::is_upgrade_authorized(&env, 7, &hash)
        });
        assert!(authorized);
    }

    #[test]
    fn pre_upgrade_validation_rejects_zero_wasm_hash() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);

        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
        let res = client.try_validate_before_upgrade(&admin, &7, &zero_hash, &false);
        assert_eq!(
            res.unwrap_err().unwrap(),
            StorageMigrationError::WasmHashZero
        );
    }

    #[test]
    fn pre_upgrade_validation_requires_auth() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        let hash = BytesN::from_array(&env, &[9u8; 32]);

        env.set_auths(&[]);
        assert!(client
            .try_validate_before_upgrade(&admin, &7, &hash, &false)
            .is_err());
    }

    #[test]
    fn pre_upgrade_validation_rejects_non_admin() {
        let env = Env::default();
        let (_admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        let intruder = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[9u8; 32]);

        let res = client.try_validate_before_upgrade(&intruder, &7, &hash, &false);
        assert_eq!(
            res.unwrap_err().unwrap(),
            StorageMigrationError::Unauthorized
        );
    }

    /// Regression proof for the forge described in issue #1415.
    ///
    /// On `main`, an unauthenticated `validate_before_upgrade(v, H')` wrote
    /// `H'` into the same `AuthorizedUpgrade` slot read by
    /// `is_upgrade_authorized`, overwriting the admin's real authorization.
    /// The entrypoint is now admin-only and the validator keys are namespaced,
    /// so the attacker call fails and the admin's hash remains authorized.
    #[test]
    fn forged_validation_cannot_overwrite_admin_authorization() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        let intruder = Address::generate(&env);
        let good_hash = BytesN::from_array(&env, &[3u8; 32]);
        let evil_hash = BytesN::from_array(&env, &[4u8; 32]);

        client.authorize_upgrade(&admin, &7, &good_hash);
        assert!(client.is_upgrade_authorized(&good_hash));

        let res = client.try_validate_before_upgrade(&intruder, &7, &evil_hash, &false);
        assert_eq!(
            res.unwrap_err().unwrap(),
            StorageMigrationError::Unauthorized
        );

        // The admin's authorization is intact and the forged hash is not valid.
        assert!(client.is_upgrade_authorized(&good_hash));
        assert!(!client.is_upgrade_authorized(&evil_hash));
        let validator_evil = env.as_contract(&contract, || {
            StorageMigrationValidator::is_upgrade_authorized(&env, 7, &evil_hash)
        });
        assert!(!validator_evil);
    }

    /// Error precedence: auth → admin → hash → version.
    #[test]
    fn validation_error_precedence_is_auth_admin_hash_version() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        let intruder = Address::generate(&env);
        let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
        let good_hash = BytesN::from_array(&env, &[9u8; 32]);

        // 1. Missing auth wins over everything.
        env.set_auths(&[]);
        assert!(client
            .try_validate_before_upgrade(&admin, &9, &zero_hash, &false)
            .is_err());
        env.mock_all_auths();

        // 2. Non-admin is rejected before the hash / version checks.
        let res = client.try_validate_before_upgrade(&intruder, &9, &zero_hash, &false);
        assert_eq!(
            res.unwrap_err().unwrap(),
            StorageMigrationError::Unauthorized
        );

        // 3. Admin + zero hash + bad version: hash is checked first.
        let res = client.try_validate_before_upgrade(&admin, &9, &zero_hash, &false);
        assert_eq!(
            res.unwrap_err().unwrap(),
            StorageMigrationError::WasmHashZero
        );

        // 4. Admin + good hash + skipped version: version check runs last.
        let res = client.try_validate_before_upgrade(&admin, &9, &good_hash, &false);
        assert_eq!(
            res.unwrap_err().unwrap(),
            StorageMigrationError::VersionSkip
        );
    }
}
