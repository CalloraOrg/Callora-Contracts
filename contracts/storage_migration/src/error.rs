//! Error variants for deterministic storage-migration validation.
//!
//! Every variant is a stable, semantic failure mode. Codes are fixed and must
//! not be reordered — deployment tooling and monitoring may key off them.

use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum StorageMigrationError {
    /// A required migration/validation step was invoked before initialization.
    NotInitialized = 1,
    /// A schema/version is already recorded and differs from the new value.
    AlreadyInitialized = 2,
    /// The WASM hash supplied for an upgrade is all-zero (almost certainly a bug).
    WasmHashZero = 3,
    /// The target version skips one or more intermediate migration steps.
    VersionSkip = 4,
    /// A rollback (target < current) was attempted without `allow_rollback`.
    RollbackNotAllowed = 5,
    /// A rollback was requested but no backup snapshot marker is present.
    BackupMissing = 6,
    /// The recorded source layout does not match what the new code expects to migrate from.
    SchemaMismatch = 7,
    /// A same-version re-deploy changed the storage layout without a version bump.
    SilentLayoutChange = 8,
    /// `finalize_migration` ran without a prior `validate_before_upgrade` authorization.
    UnauthorizedUpgradeState = 9,
    /// The live storage could not be read back under the new code's expectations.
    StorageUnreadable = 10,
}
