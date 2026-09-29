//! Semantic error variants for the emergency migration contract.
//!
//! All numeric discriminants are part of the public contract interface and
//! **must remain stable**. Never renumber existing variants; add new ones at
//! the end with the next available code.
//!
//! | Code | Variant                  | When raised                                            |
//! |------|--------------------------|--------------------------------------------------------|
//! | 1    | `NotInitialized`         | Entrypoint called before `init`                        |
//! | 2    | `AlreadyInitialized`     | `init` called more than once                           |
//! | 3    | `Unauthorized`           | Caller is not the configured administrator             |
//! | 4    | `LegacyStateMissing`     | No legacy state found in storage                       |
//! | 5    | `InvalidLegacyBalance`   | Legacy balance is negative (invalid precondition)      |
//! | 6    | `VersionMismatch`        | Supplied `expected_version` ≠ stored version           |
//! | 7    | `InvalidTargetVersion`   | `target_version` is not exactly `expected_version + 1` |
//! | 8    | `AlreadyMigrated`        | Migration has already been performed                   |
//! | 9    | `UpgradeNotAuthorized`   | No authorized upgrade for the requested version        |
//! | 10   | `VersionOverflow`        | Version counter would exceed `u32::MAX`                |
//! | 11   | `InvalidInitialVersion`  | `initial_version` is unreasonably large (> `u32::MAX - 1024`) |
//! | 12   | `WasmHashZero`           | Supplied WASM hash is all-zero bytes (likely a mistake) |
//! | 13   | `MigrationDataCorrupted` | Stored current data fails internal consistency checks  |

use soroban_sdk::contracterror;

/// Typed, machine-readable errors returned by the migration contract entrypoints.
///
/// Callers and indexers can branch on the `u32` code embedded in the
/// `ScError::Contract` envelope instead of matching on panic strings.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum MigrationError {
    // ── Initialization ────────────────────────────────────────────────────
    /// The contract has not been initialized.
    ///
    /// Call [`EmergencyMigration::init`] before any other entrypoint.
    NotInitialized = 1,

    /// Initialization was attempted more than once.
    ///
    /// The contract stores its admin during `init`; a second call is rejected.
    AlreadyInitialized = 2,

    // ── Authorization ─────────────────────────────────────────────────────
    /// The caller is not the configured administrator.
    ///
    /// All state-changing entrypoints check that the caller equals the admin
    /// address stored at initialization time.
    Unauthorized = 3,

    // ── Legacy state ──────────────────────────────────────────────────────
    /// No legacy state was found in storage.
    ///
    /// The migration contract expects the previous deployment to have written
    /// a [`LegacyData`] record under [`StorageKey::Legacy`] before `migrate`
    /// is called. This error is returned when that key is absent.
    LegacyStateMissing = 4,

    /// The legacy balance is negative, which indicates data corruption or an
    /// invalid deployment state.
    ///
    /// Migration is refused when `legacy.balance < 0` to prevent introducing
    /// a negative balance into the new storage layout.
    InvalidLegacyBalance = 5,

    // ── Version gating ────────────────────────────────────────────────────
    /// The supplied `expected_version` does not match the stored version.
    ///
    /// This guard prevents running a migration step out of order and serves
    /// as a concurrency safety check: if two operators race to call `migrate`,
    /// only the first succeeds.
    VersionMismatch = 6,

    /// The requested `target_version` is not exactly `expected_version + 1`.
    ///
    /// The contract enforces a strictly-monotonic, no-skip version sequence.
    /// Skipping versions or providing a target equal to or below the current
    /// version raises this error.
    InvalidTargetVersion = 7,

    // ── Idempotency guards ────────────────────────────────────────────────
    /// Migration has already been performed for the requested state.
    ///
    /// Once the reshaped [`CurrentData`] record is written to storage, further
    /// `migrate` calls are rejected to ensure the reshape runs at most once.
    AlreadyMigrated = 8,

    // ── Upgrade guard ─────────────────────────────────────────────────────
    /// No upgrade has been authorized for the requested version.
    ///
    /// [`EmergencyMigration::authorize_upgrade`] must be called first with the
    /// correct `target_version` and `wasm_hash` before a deployment tool may
    /// proceed with the platform-level WASM swap.
    UpgradeNotAuthorized = 9,

    // ── New semantic variants (codes 10+) ─────────────────────────────────
    /// The migration version counter would overflow `u32::MAX`.
    ///
    /// This is a defensive overflow guard on the `checked_add(1)` that
    /// advances the version. In practice, version numbers should never
    /// approach `u32::MAX`.
    VersionOverflow = 10,

    /// The `initial_version` supplied to `init` is unreasonably large.
    ///
    /// Values greater than `u32::MAX - 1024` are rejected to reserve headroom
    /// for version increments and to guard against accidental very-large
    /// initial versions that would immediately overflow during migration.
    InvalidInitialVersion = 11,

    /// The supplied WASM hash is all zero bytes.
    ///
    /// An all-zero hash is almost certainly a programming mistake (e.g. a
    /// zero-initialized buffer). Rejecting it early prevents accidentally
    /// authorizing an upgrade to an invalid WASM artifact.
    WasmHashZero = 12,

    /// The stored current data fails internal consistency checks.
    ///
    /// Raised when a post-migration view finds that `current.reserved < 0`
    /// or that `current.balance` differs from what was recorded at migration
    /// time — indicating unexpected direct-storage tampering.
    MigrationDataCorrupted = 13,
}
