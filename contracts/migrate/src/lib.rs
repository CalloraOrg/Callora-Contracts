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
    ///
    /// `admin` must authorize the call. `initial_version` is the version of
    /// the legacy state currently stored by the previous deployment.
    ///
    /// # Errors
    /// * [`MigrationError::AlreadyInitialized`] — called more than once.
    /// * [`MigrationError::InvalidInitialVersion`] — `initial_version > u32::MAX - 1024`.
    pub fn init(
        env: Env,
        admin: Address,
        initial_version: u32,
    ) -> Result<(), MigrationError> {
        admin.require_auth();
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(MigrationError::AlreadyInitialized);
        }
        if initial_version > MAX_INITIAL_VERSION {
            return Err(MigrationError::InvalidInitialVersion);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::Version, &initial_version);
        Ok(())
    }

    /// Reshape legacy emergency state and advance the migration version.
    ///
    /// The operation requires administrator authorization and is atomic from
    /// the caller's perspective: the current record and version are written
    /// only after all validation succeeds. The target must be exactly one
    /// greater than `expected_version`, preventing skipped migrations and
    /// replay of an already completed migration.
    ///
    /// # Errors
    /// * [`MigrationError::NotInitialized`] — `init` not called.
    /// * [`MigrationError::Unauthorized`] — caller is not admin.
    /// * [`MigrationError::VersionMismatch`] — `expected_version` ≠ stored version.
    /// * [`MigrationError::VersionOverflow`] — version increment would overflow `u32::MAX`.
    /// * [`MigrationError::InvalidTargetVersion`] — `target_version ≠ expected_version + 1`.
    /// * [`MigrationError::AlreadyMigrated`] — current data already present.
    /// * [`MigrationError::LegacyStateMissing`] — no legacy data in storage.
    /// * [`MigrationError::InvalidLegacyBalance`] — `legacy.balance < 0`.
    pub fn migrate(
        env: Env,
        caller: Address,
        expected_version: u32,
        target_version: u32,
    ) -> Result<CurrentData, MigrationError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(MigrationError::NotInitialized)?;
        if caller != admin {
            return Err(MigrationError::Unauthorized);
        }

        let version: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::Version)
            .ok_or(MigrationError::NotInitialized)?;
        if version != expected_version {
            return Err(MigrationError::VersionMismatch);
        }
        // Use checked_add to guard against version counter overflow.
        let next_version = expected_version
            .checked_add(1)
            .ok_or(MigrationError::VersionOverflow)?;
        if target_version != next_version {
            return Err(MigrationError::InvalidTargetVersion);
        }
        if env.storage().instance().has(&StorageKey::Current) {
            return Err(MigrationError::AlreadyMigrated);
        }

        let legacy: LegacyData = env
            .storage()
            .instance()
            .get(&StorageKey::Legacy)
            .ok_or(MigrationError::LegacyStateMissing)?;
        if legacy.balance < 0 {
            return Err(MigrationError::InvalidLegacyBalance);
        }

        let current = CurrentData {
            balance: legacy.balance,
            last_updated: legacy.last_updated,
            reserved: 0,
        };
        env.storage()
            .instance()
            .set(&StorageKey::Current, &current);
        env.storage()
            .instance()
            .set(&StorageKey::Version, &target_version);
        Ok(current)
    }

    /// Authorize a contract upgrade for the current migration version.
    ///
    /// This is a guard only; it does not call `update_current_contract_wasm`.
    /// The deployment tool must consume the returned authorization state and
    /// perform the platform upgrade in a separate transaction.
    ///
    /// # Errors
    /// * [`MigrationError::NotInitialized`] — `init` not called.
    /// * [`MigrationError::Unauthorized`] — caller is not admin.
    /// * [`MigrationError::VersionMismatch`] — `target_version` ≠ stored version.
    /// * [`MigrationError::WasmHashZero`] — `wasm_hash` is all-zero bytes.
    pub fn authorize_upgrade(
        env: Env,
        caller: Address,
        target_version: u32,
        wasm_hash: BytesN<32>,
    ) -> Result<(), MigrationError> {
        caller.require_auth();
        let admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(MigrationError::NotInitialized)?;
        if caller != admin {
            return Err(MigrationError::Unauthorized);
        }
        let version: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::Version)
            .ok_or(MigrationError::NotInitialized)?;
        if target_version != version {
            return Err(MigrationError::VersionMismatch);
        }
        // Reject an all-zero hash — almost certainly a programming mistake.
        if wasm_hash == BytesN::from_array(&env, &[0u8; 32]) {
            return Err(MigrationError::WasmHashZero);
        }
        env.storage()
            .instance()
            .set(&StorageKey::AuthorizedUpgrade, &(target_version, wasm_hash));
        Ok(())
    }

    /// Return the reshaped emergency state, if migration has completed.
    pub fn get_current(env: Env) -> Option<CurrentData> {
        env.storage().instance().get(&StorageKey::Current)
    }

    /// Return the stored migration version, if initialized.
    pub fn version(env: Env) -> Option<u32> {
        env.storage().instance().get(&StorageKey::Version)
    }

    /// Check whether a hash is authorized for the current migration version.
    pub fn is_upgrade_authorized(env: Env, wasm_hash: BytesN<32>) -> bool {
        let version: Option<u32> = env.storage().instance().get(&StorageKey::Version);
        let authorization: Option<(u32, BytesN<32>)> =
            env.storage().instance().get(&StorageKey::AuthorizedUpgrade);
        match (version, authorization) {
            (Some(version), Some((authorized_version, authorized_hash))) => {
                version == authorized_version && wasm_hash == authorized_hash
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
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
        assert!(client.try_authorize_upgrade(&admin, &7, &zero_hash).is_err());
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
}
