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

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, BytesN, Env};

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

/// Errors returned by the migration entrypoints.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[contracterror]
pub enum MigrationError {
    /// The contract has not been initialized.
    NotInitialized = 1,
    /// Initialization was attempted more than once.
    AlreadyInitialized = 2,
    /// The caller is not the configured administrator.
    Unauthorized = 3,
    /// No legacy state was found.
    LegacyStateMissing = 4,
    /// The legacy balance is invalid.
    InvalidLegacyBalance = 5,
    /// The supplied source version does not match the stored version.
    VersionMismatch = 6,
    /// The requested target version is not the next version.
    InvalidTargetVersion = 7,
    /// Migration has already been performed for the requested state.
    AlreadyMigrated = 8,
    /// No upgrade has been authorized for the requested version.
    UpgradeNotAuthorized = 9,
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
    pub fn init(
        env: Env,
        admin: Address,
        initial_version: u32,
    ) -> Result<(), MigrationError> {
        admin.require_auth();
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(MigrationError::AlreadyInitialized);
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
        let next_version = expected_version
            .checked_add(1)
            .ok_or(MigrationError::InvalidTargetVersion)?;
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
        assert!(client.try_migrate(&admin, &8, &9).is_err());
    }

    #[test]
    fn migration_rejects_wrong_version_and_negative_balance() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        assert!(client.try_migrate(&admin, &6, &7).is_err());

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
    fn upgrade_guard_requires_current_version_and_matching_hash() {
        let env = Env::default();
        let (admin, contract) = setup(&env);
        let client = EmergencyMigrationClient::new(&env, &contract);
        let hash = BytesN::from_array(&env, &[9; 32]);

        assert!(client.try_authorize_upgrade(&admin, &6, &hash).is_err());
        client.authorize_upgrade(&admin, &7, &hash);
        assert!(client.is_upgrade_authorized(&hash));
        assert!(!client.is_upgrade_authorized(&BytesN::from_array(&env, &[8; 32])));
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
}
