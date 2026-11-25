//! Stake migration stub — data reshape + upgrade guard.
//!
//! This module provides a minimal, security-hardened migration path for
//! stake state.  It reshapes legacy stake data into the current layout
//! and enforces a monotonic, version-gated upgrade authorisation so that a
//! follow-on WASM deployment cannot be installed against stale state.
//!
//! ## Design principles
//!
//! * **Single-shot migration** — the reshape runs at most once per version.
//! * **Admin-gated** — every state-changing entrypoint calls
//!   `caller.require_auth()` and verifies the caller is the configured
//!   administrator.
//! * **Overflow-safe** — all arithmetic uses `checked_*` methods; no
//!   `unwrap()` on production paths.
//! * **Idempotent** — re-running a completed migration is a safe no-op.
//! * **Upgrade guard** — an authorised WASM hash must be recorded for the
//!   current migration version before the deployment tool should proceed.
//!
//! ## API overview
//!
//! | Entrypoint             | Auth  | Mutates state | Description                               |
//! |------------------------|-------|---------------|-------------------------------------------|
//! | `init`                 | Yes   | Yes           | Configure admin and starting version      |
//! | `migrate`              | Yes   | Yes           | Reshape legacy → current stake data       |
//! | `authorize_upgrade`    | Yes   | Yes           | Record authorised WASM hash for version   |
//! | `get_current`          | No    | No            | Return reshaped state (if migrated)       |
//! | `version`              | No    | No            | Return stored migration version           |
//! | `is_upgrade_authorised`| No    | No            | Check whether a hash is authorised        |
//!
//! Legacy data is expected to have been pre-staged under
//! [`StorageKey::Legacy`] by the previous deployment.  No public entrypoint
//! writes that key, preventing arbitrary callers from supplying migration input.
//!
//! ## Data layout changes
//!
//! | Key           | Legacy                           | Current                           |
//! |---------------|----------------------------------|-----------------------------------|
//! | `LegacyStake` | `LegacyStake { total_staked }`   | read, reshaped, removed           |
//! | `CurrentStake`| —                                | written with added `reserve` field|
//! | `Version`     | absent                           | set to `target_version`           |
//!
//! The `reserve` field defaults to zero during the reshape and is available
//! for use by future stake accounting.
//!
//! ## Events
//!
//! | Function             | Topic           | Topics           | Data             |
//! |----------------------|-----------------|------------------|------------------|
//! | `migrate`            | `stake_migrated`| `(topic)`        | `target_version` |
//! | `authorize_upgrade`  | `upg_authorised`| `(topic, hash)`  | `target_version` |

use callora_storage_migration::{AuthorizeOptions, MigrateOptions, MigrationKeys, StubError};
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, Address, BytesN, Env, Symbol,
};

// ─── Storage keys ─────────────────────────────────────────────────────────────

/// Storage keys used by the stake migration contract.
#[derive(Clone)]
#[contracttype]
pub enum StorageKey {
    /// Configured administrator address.
    Admin,
    /// Monotonic migration version counter.
    Version,
    /// Legacy stake data written by the previous deployment.
    Legacy,
    /// Reshaped current stake state (populated after migration).
    Current,
    /// Authorised WASM hash for the current version; stores `(version, hash)`.
    AuthorisedUpgrade,
}

// ─── Data types ───────────────────────────────────────────────────────────────

/// Stake data written by the legacy deployment.
///
/// The legacy layout stored only aggregate `total_staked` with a
/// `last_checkpoint` ledger-sequence field.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct LegacyStake {
    /// Aggregate total stake carried forward from the previous deployment.
    pub total_staked: i128,
    /// Ledger sequence when the legacy state was last written.
    pub last_checkpoint: u32,
}

/// Current stake data layout after a successful migration.
///
/// The `reserve` field is newly introduced and initialised to zero.  Future
/// versions may use this slot for protocol-level reserve accounting.
#[derive(Clone, Debug, Eq, PartialEq)]
#[contracttype]
pub struct CurrentStake {
    /// Aggregate total stake preserved from the legacy layout.
    pub total_staked: i128,
    /// Ledger sequence preserved from the legacy layout.
    pub last_checkpoint: u32,
    /// Newly introduced protocol reserve, initialised to zero during reshape.
    pub reserve: i128,
}

// ─── Errors ───────────────────────────────────────────────────────────────────

/// Stable, machine-readable error codes for the stake migration contract.
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[contracterror]
#[repr(u32)]
pub enum StakeMigrateError {
    /// The contract has not been initialised.
    NotInitialized = 1,
    /// Initialisation was attempted more than once.
    AlreadyInitialized = 2,
    /// The caller is not the configured administrator.
    Unauthorized = 3,
    /// No legacy stake state was found in storage.
    LegacyStateMissing = 4,
    /// The legacy `total_staked` value is invalid (negative).
    InvalidLegacyBalance = 5,
    /// The supplied source version does not match the stored version.
    VersionMismatch = 6,
    /// The requested target version is not exactly `expected_version + 1`.
    InvalidTargetVersion = 7,
    /// Migration has already been performed for this version.
    AlreadyMigrated = 8,
    /// No upgrade has been authorised for the current version.
    UpgradeNotAuthorized = 9,
    /// Arithmetic overflow detected in a checked operation.
    Overflow = 10,
    /// The supplied WASM hash is all-zero bytes (almost certainly a mistake).
    WasmHashZero = 11,
}

// ─── Event symbol helpers ─────────────────────────────────────────────────────

/// Returns the `Symbol` for the `"stake_migrated"` event topic.
#[inline]
fn event_stake_migrated(env: &Env) -> Symbol {
    Symbol::new(env, "stake_migrated")
}

/// Returns the `Symbol` for the `"upg_authorised"` event topic.
#[inline]
fn event_upgrade_authorised(env: &Env) -> Symbol {
    Symbol::new(env, "upg_authorised")
}

/// Ledger threshold below which the instance TTL is extended on migration.
const INSTANCE_TTL_THRESHOLD: u32 = 100;

/// Ledger count to extend the instance TTL to on migration.
const INSTANCE_TTL_EXTEND_TO: u32 = 10_000;

impl From<StubError> for StakeMigrateError {
    fn from(err: StubError) -> Self {
        match err {
            StubError::NotInitialized => StakeMigrateError::NotInitialized,
            StubError::AlreadyInitialized => StakeMigrateError::AlreadyInitialized,
            StubError::Unauthorized => StakeMigrateError::Unauthorized,
            StubError::LegacyStateMissing => StakeMigrateError::LegacyStateMissing,
            StubError::InvalidLegacyBalance => StakeMigrateError::InvalidLegacyBalance,
            StubError::VersionMismatch => StakeMigrateError::VersionMismatch,
            StubError::InvalidTargetVersion => StakeMigrateError::InvalidTargetVersion,
            StubError::AlreadyMigrated => StakeMigrateError::AlreadyMigrated,
            StubError::UpgradeNotAuthorized => StakeMigrateError::UpgradeNotAuthorized,
            StubError::Overflow => StakeMigrateError::Overflow,
            StubError::InvalidInitialVersion => StakeMigrateError::NotInitialized,
            StubError::WasmHashZero => StakeMigrateError::WasmHashZero,
        }
    }
}

const KEYS: MigrationKeys<StorageKey> = MigrationKeys::new(
    StorageKey::Admin,
    StorageKey::Version,
    StorageKey::Legacy,
    StorageKey::Current,
    StorageKey::AuthorisedUpgrade,
);

// ─── Contract ─────────────────────────────────────────────────────────────────

/// Stake data migration and upgrade guard.
///
/// Deploy this contract **before** the main upgrade to ensure stale stake
/// state is reshaped and the new WASM hash is authorised.
#[contract]
pub struct CalloraStakeMigrate;

#[contractimpl]
impl CalloraStakeMigrate {
    /// Initialise the stake migration guard.
    pub fn init(env: Env, admin: Address, initial_version: u32) -> Result<(), StakeMigrateError> {
        KEYS.init(&env, &admin, initial_version, None)
            .map_err(Into::into)
    }

    /// Reshape legacy stake state and advance the migration version.
    pub fn migrate(
        env: Env,
        caller: Address,
        expected_version: u32,
        target_version: u32,
    ) -> Result<CurrentStake, StakeMigrateError> {
        KEYS.migrate(
            &env,
            &caller,
            expected_version,
            target_version,
            &MigrateOptions {
                ttl_extension: Some((INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO)),
                remove_legacy: true,
                event_topic: Some(event_stake_migrated(&env)),
            },
            |legacy: LegacyStake| {
                if legacy.total_staked < 0 {
                    return Err(StubError::InvalidLegacyBalance);
                }
                Ok(CurrentStake {
                    total_staked: legacy.total_staked,
                    last_checkpoint: legacy.last_checkpoint,
                    reserve: 0,
                })
            },
        )
        .map_err(Into::into)
    }

    /// Authorise a contract upgrade for the current migration version.
    pub fn authorize_upgrade(
        env: Env,
        caller: Address,
        target_version: u32,
        wasm_hash: BytesN<32>,
    ) -> Result<(), StakeMigrateError> {
        KEYS.authorize_upgrade(
            &env,
            &caller,
            target_version,
            &wasm_hash,
            &AuthorizeOptions {
                reject_zero_hash: true,
                event_topic: Some(event_upgrade_authorised(&env)),
            },
        )
        .map_err(Into::into)
    }

    /// Return the reshaped stake state, if migration has completed.
    pub fn get_current(env: Env) -> Option<CurrentStake> {
        KEYS.get_current(&env)
    }

    /// Return the stored migration version, if initialised.
    pub fn version(env: Env) -> Option<u32> {
        KEYS.version(&env)
    }

    /// Check whether a WASM hash is authorised for the current migration version.
    pub fn is_upgrade_authorised(env: Env, wasm_hash: BytesN<32>) -> bool {
        KEYS.is_upgrade_authorized(&env, &wasm_hash)
    }
}
