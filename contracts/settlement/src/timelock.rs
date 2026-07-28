//! Timelock state and storage helpers for developer balance migrations.

use soroban_sdk::{contracttype, Address, Env};

use crate::{StorageKey, PERSISTENT_BUMP_AMOUNT, PERSISTENT_BUMP_THRESHOLD};

/// Mandatory delay between proposing and executing a balance migration.
pub const DEVELOPER_MIGRATION_TIMELOCK_SECONDS: u64 = 86_400;

/// Immutable approval snapshot stored for a pending developer migration.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingDeveloperMigration {
    pub from: Address,
    pub to: Address,
    pub amount: i128,
    pub proposed_at: u64,
    pub execute_after: u64,
}

/// Read a pending migration and refresh its storage lifetime.
pub(crate) fn get_pending_migration(
    env: &Env,
    from: &Address,
) -> Option<PendingDeveloperMigration> {
    let key = StorageKey::PendingDeveloperMigration(from.clone());
    if env.storage().persistent().has(&key) {
        env.storage().persistent().extend_ttl(
            &key,
            PERSISTENT_BUMP_THRESHOLD,
            PERSISTENT_BUMP_AMOUNT,
        );
    }
    env.storage().persistent().get(&key)
}

/// Persist a pending migration and refresh its storage lifetime.
pub(crate) fn set_pending_migration(env: &Env, migration: &PendingDeveloperMigration) {
    let key = StorageKey::PendingDeveloperMigration(migration.from.clone());
    env.storage().persistent().set(&key, migration);
    env.storage().persistent().extend_ttl(&key, 50_000, 50_000);
}

/// Consume a successfully executed proposal to make replay impossible.
pub(crate) fn remove_pending_migration(env: &Env, from: &Address) {
    env.storage()
        .persistent()
        .remove(&StorageKey::PendingDeveloperMigration(from.clone()));
}
