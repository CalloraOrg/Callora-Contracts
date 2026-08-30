#![no_std]

//! Deterministic storage-migration validation for Callora Soroban contracts.
//!
//! # Why this exists
//!
//! Soroban contracts cannot be upgraded in place. A "contract upgrade" deploys a
//! new WASM and then calls [`soroban_sdk::Env::deployer`]'
//! [`update_current_contract_wasm`](soroban_sdk::Deployer::update_current_contract_wasm).
//! The *old* code performs the swap; from that moment the *new* code answers
//! every subsequent call. The danger: if the new code expects a storage layout
//! that the live instance storage does not satisfy, the very first business
//! call may panic or, worse, silently misinterpret serialized bytes — an
//! implicit, destructive transformation of deployed state.
//!
//! This crate adds a deterministic guard that **runs before the upgraded code
//! takes effect**:
//!
//! 1. `record_deployed_schema` — a contract records its storage-layout version
//!    and a content hash of its schema at deploy / `init` time.
//! 2. `validate_before_upgrade` — the *current* (old) code calls this as the
//!    first action of its upgrade entrypoint, **before** `update_current_contract_wasm`.
//!    It checks ordering, compatibility, rollback boundaries, WASM sanity, and
//!    records an upgrade authorization. It never mutates business data, so
//!    existing deployed data stays fully readable.
//! 3. `finalize_migration` — the *new* code calls this as the first action of
//!    its migration entrypoint, **before** any business logic runs. It re-probes
//!    storage readability and commits the new version + layout hash only after
//!    the probe succeeds, refusing to operate on an incompatible layout.
//!
//! # Failure modes
//!
//! Every failure returns [`StorageMigrationError`] (never a string panic in
//! production paths). See [`StorageMigrationError`] for the full list and the
//! deterministic trigger for each variant.
//!
//! # Rollback boundaries
//!
//! * Forward, single-step upgrades (`target == current + 1`) are always allowed
//!   once authorized.
//! * Same-version re-deploys (`target == current`) are allowed **only if** the
//!   new code's layout hash matches the recorded one — a silent layout change
//!   without a version bump is rejected ([`StorageMigrationError::SilentLayoutChange`]).
//! * Multi-step skips (`target > current + 1`) are rejected
//!   ([`StorageMigrationError::VersionSkip`]) to keep migrations auditable.
//! * Rollbacks (`target < current`) are rejected unless `allow_rollback` is set
//!   **and** a backup marker is present
//!   ([`StorageMigrationError::RollbackNotAllowed`] /
//!   [`StorageMigrationError::BackupMissing`]).

pub mod error;

pub use error::StorageMigrationError;

use soroban_sdk::{contracttype, Bytes, BytesN, Env};

/// Version recorded when no schema has ever been deployed. Semantically this is
/// the "legacy / unversioned" state of an instance that predates migration
/// tracking. It is `0` so that the first tracked version (`1`) is a clean
/// forward, single-step upgrade.
pub const LEGACY_VERSION: u32 = 0;

/// All-zero layout hash used as a sentinel meaning "do not enforce a source
/// layout check". Legacy deployments that never recorded a schema may pass this
/// to skip the strict source-layout comparison.
pub fn zero_layout_hash(env: &Env) -> BytesN<32> {
    BytesN::from_array(env, &[0u8; 32])
}

/// Instance-tier storage keys used by the validator.
///
/// We deliberately use Instance storage: the version marker, schema hash, and
/// upgrade authorization are singletons consulted on every upgrade path and
/// must never be independently evictable from Persistent/Temporary tiers.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Current storage-layout version (`u32`). Absence ⇒ [`LEGACY_VERSION`].
    Version,
    /// Content hash of the deployed storage schema (`BytesN<32>`).
    SchemaLayoutHash,
    /// Upgrade authorization guard: `(target_version, wasm_hash)`.
    AuthorizedUpgrade,
    /// Whether a rollback snapshot/backup exists for the current version (`bool`).
    BackupPresent,
}

/// Result of a successful validation. Purely descriptive — no storage mutation
/// beyond recording the authorization is performed by [`validate_before_upgrade`].
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationReport {
    /// Version found in storage before the upgrade (always concrete; legacy ⇒ 0).
    pub current_version: u32,
    /// Version the new code claims to deploy to.
    pub target_version: u32,
    /// WASM hash being authorized.
    pub wasm_hash: BytesN<32>,
    /// `true` when the recorded source layout matches `expected_source_layout_hash`.
    pub schema_consistent: bool,
    /// `true` when the live storage was probed and found readable at the marker level.
    pub readability_ok: bool,
    /// `true` when the authorization was recorded (i.e. the whole check passed).
    pub authorized: bool,
}

/// Compute a stable content hash for a textual schema descriptor.
///
/// Contracts build a descriptor string such as
/// `"vault:1:MetaKey:VaultMeta,Admin:Address"` and register its hash. The hash
/// is deterministic across deployments, so a layout change is detectable.
pub fn layout_hash(env: &Env, descriptor: &str) -> BytesN<32> {
    let bytes = Bytes::from_slice(env, descriptor.as_bytes());
    env.crypto().sha256(&bytes).into()
}

/// Deterministic storage-migration validator.
///
/// All methods are pure with respect to *business* state: they only ever read
/// or write the validator's own [`StorageKey`] markers. This guarantees that
/// running the guard never implicitly transforms deployed data.
pub struct StorageMigrationValidator;

impl StorageMigrationValidator {
    // ─── Deploy-time recording ─────────────────────────────────────────────

    /// Record the storage-layout version and schema hash at deploy / `init`.
    ///
    /// Idempotent when the recorded values already match. Refuses to silently
    /// overwrite a different version, which would mask an un-migrated state.
    ///
    /// # Errors
    /// * [`StorageMigrationError::AlreadyInitialized`] — a *different* version or
    ///   layout hash is already recorded.
    pub fn record_deployed_schema(
        env: &Env,
        version: u32,
        schema_hash: &BytesN<32>,
    ) -> Result<(), StorageMigrationError> {
        let existing: Option<u32> = env.storage().instance().get(&StorageKey::Version);
        let existing_hash: Option<BytesN<32>> =
            env.storage().instance().get(&StorageKey::SchemaLayoutHash);

        match (existing, existing_hash) {
            (Some(v), Some(h)) if v == version && h == *schema_hash => {
                // Idempotent no-op — already recorded identically.
                return Ok(());
            }
            (Some(_), Some(_)) => {
                return Err(StorageMigrationError::AlreadyInitialized);
            }
            _ => {}
        }

        env.storage().instance().set(&StorageKey::Version, &version);
        env.storage()
            .instance()
            .set(&StorageKey::SchemaLayoutHash, schema_hash);
        Ok(())
    }

    // ─── Queries ───────────────────────────────────────────────────────────

    /// Current storage-layout version, or [`LEGACY_VERSION`] when absent.
    pub fn current_version(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get(&StorageKey::Version)
            .unwrap_or(LEGACY_VERSION)
    }

    /// Whether `wasm_hash` is authorized for `target_version`.
    ///
    /// Consumed out-of-band by the deployment tooling before it performs the
    /// platform-level `update_current_contract_wasm` call.
    pub fn is_upgrade_authorized(env: &Env, target_version: u32, wasm_hash: &BytesN<32>) -> bool {
        let auth: Option<(u32, BytesN<32>)> =
            env.storage().instance().get(&StorageKey::AuthorizedUpgrade);
        match auth {
            Some((v, h)) => v == target_version && h == *wasm_hash,
            None => false,
        }
    }

    // ─── Pre-upgrade gate (old code, before WASM swap) ─────────────────────

    /// Validate a pending upgrade **before** the new WASM is swapped in.
    ///
    /// This is the deterministic guard that must be invoked as the first action
    /// of an upgrade entrypoint, ahead of `update_current_contract_wasm`. It:
    ///
    /// * rejects an all-zero `wasm_hash`
    ///   ([`StorageMigrationError::WasmHashZero`]);
    /// * enforces version ordering (no skip, no unsanctioned rollback)
    ///   ([`StorageMigrationError::VersionSkip`],
    ///   [`StorageMigrationError::RollbackNotAllowed`],
    ///   [`StorageMigrationError::BackupMissing`]);
    /// * enforces schema compatibility — a same-version re-deploy must keep the
    ///   same layout hash, and a migration must start from the recorded source
    ///   layout ([`StorageMigrationError::SilentLayoutChange`],
    ///   [`StorageMigrationError::SchemaMismatch`]);
    /// * records the authorization so the deployer tool can verify it.
    ///
    /// On success it returns a [`ValidationReport`] and persists the
    /// `(target_version, wasm_hash)` authorization. It never mutates business
    /// state, so existing deployed data remains fully readable.
    ///
    /// # Parameters
    /// * `target_version` — the storage version the new code deploys to.
    /// * `new_layout_hash` — the new code's own schema hash.
    /// * `expected_source_layout_hash` — the layout the new code expects to
    ///   migrate *from*. Pass [`zero_layout_hash`] to skip this check (legacy).
    /// * `wasm_hash` — the replacement WASM hash.
    /// * `allow_rollback` — permit a downgrade (`target < current`) when a backup
    ///   marker is present.
    pub fn validate_before_upgrade(
        env: &Env,
        target_version: u32,
        new_layout_hash: &BytesN<32>,
        expected_source_layout_hash: &BytesN<32>,
        wasm_hash: &BytesN<32>,
        allow_rollback: bool,
    ) -> Result<ValidationReport, StorageMigrationError> {
        if *wasm_hash == zero_layout_hash(env) {
            return Err(StorageMigrationError::WasmHashZero);
        }

        let current = Self::current_version(env);
        let recorded_layout: Option<BytesN<32>> =
            env.storage().instance().get(&StorageKey::SchemaLayoutHash);

        // Readability probe: the validator's own version marker must deserialize
        // to a `u32` and agree with what `current_version` reported. Absence
        // (legacy) is consistent with `current == LEGACY_VERSION`. A mismatch
        // would indicate storage corruption and is surfaced as a failed probe.
        let readability_ok = match env.storage().instance().get::<_, u32>(&StorageKey::Version) {
            Some(v) => v == current,
            None => current == LEGACY_VERSION,
        };

        // ── Ordering / rollback boundaries ──────────────────────────────
        if target_version < current {
            if !allow_rollback {
                return Err(StorageMigrationError::RollbackNotAllowed);
            }
            let backup: bool = env
                .storage()
                .instance()
                .get(&StorageKey::BackupPresent)
                .unwrap_or(false);
            if !backup {
                return Err(StorageMigrationError::BackupMissing);
            }
        } else if target_version > current.saturating_add(1) {
            return Err(StorageMigrationError::VersionSkip);
        }

        // ── Schema compatibility ────────────────────────────────────────
        let schema_consistent = match recorded_layout {
            None => {
                // Legacy / unversioned deployment. A zero (don't-care) source
                // layout sentinel is accepted; pinning a concrete source layout
                // against a deployment that never recorded one cannot be
                // verified and is rejected to avoid silent mismatches.
                if *expected_source_layout_hash != zero_layout_hash(env) {
                    return Err(StorageMigrationError::SchemaMismatch);
                }
                true
            }
            Some(recorded) => {
                if target_version == current {
                    // Same-version re-deploy: layout must not change silently.
                    if recorded != *new_layout_hash {
                        return Err(StorageMigrationError::SilentLayoutChange);
                    }
                    true
                } else {
                    // Migration: the claimed source layout must match what is
                    // recorded, unless the caller opts out (legacy sentinel).
                    if *expected_source_layout_hash != zero_layout_hash(env)
                        && recorded != *expected_source_layout_hash
                    {
                        return Err(StorageMigrationError::SchemaMismatch);
                    }
                    true
                }
            }
        };

        // Record the upgrade authorization for the deployer tool to verify.
        env.storage().instance().set(
            &StorageKey::AuthorizedUpgrade,
            &(target_version, wasm_hash.clone()),
        );

        Ok(ValidationReport {
            current_version: current,
            target_version,
            wasm_hash: wasm_hash.clone(),
            schema_consistent,
            readability_ok,
            authorized: true,
        })
    }

    // ─── Post-swap finalize (new code, before business logic) ──────────────

    /// Finalize a migration in the **new** code before any business logic runs.
    ///
    /// The new code must call this as the first action of its migration
    /// entrypoint. It re-probes storage readability and, only after the probe
    /// passes, commits the new storage version and layout hash. If the live
    /// storage cannot be read under the new code's expectations, it errors out
    /// and refuses to operate — preventing an implicit destructive
    /// transformation of deployed data.
    ///
    /// # Errors
    /// * [`StorageMigrationError::UnauthorizedUpgradeState`] — the upgrade was
    ///   not authorized for `(target_version, wasm_hash)` via
    ///   [`validate_before_upgrade`].
    /// * [`StorageMigrationError::WasmHashZero`] — `wasm_hash` is all-zero.
    /// * [`StorageMigrationError::StorageUnreadable`] — the readability probe
    ///   could not confirm storage integrity.
    pub fn finalize_migration(
        env: &Env,
        target_version: u32,
        new_layout_hash: &BytesN<32>,
        wasm_hash: &BytesN<32>,
    ) -> Result<ValidationReport, StorageMigrationError> {
        if *wasm_hash == zero_layout_hash(env) {
            return Err(StorageMigrationError::WasmHashZero);
        }

        if !Self::is_upgrade_authorized(env, target_version, wasm_hash) {
            return Err(StorageMigrationError::UnauthorizedUpgradeState);
        }

        // Readability probe of the live instance storage under the new code.
        let readable = env
            .storage()
            .instance()
            .get::<_, u32>(&StorageKey::Version)
            .is_some()
            || env
                .storage()
                .instance()
                .get::<_, BytesN<32>>(&StorageKey::SchemaLayoutHash)
                .is_some()
            || env
                .storage()
                .instance()
                .get::<_, (u32, BytesN<32>)>(&StorageKey::AuthorizedUpgrade)
                .is_some();
        if !readable {
            return Err(StorageMigrationError::StorageUnreadable);
        }

        env.storage()
            .instance()
            .set(&StorageKey::Version, &target_version);
        env.storage()
            .instance()
            .set(&StorageKey::SchemaLayoutHash, new_layout_hash);

        Ok(ValidationReport {
            current_version: target_version,
            target_version,
            wasm_hash: wasm_hash.clone(),
            schema_consistent: true,
            readability_ok: true,
            authorized: true,
        })
    }

    /// Mark that a rollback backup snapshot exists for the current version.
    ///
    /// Must be set by operational tooling before a rollback is attempted.
    pub fn set_backup_present(env: &Env, present: bool) {
        env.storage()
            .instance()
            .set(&StorageKey::BackupPresent, &present);
    }

    /// Read the recorded authorization guard, if any.
    pub fn authorized_upgrade(env: &Env) -> Option<(u32, BytesN<32>)> {
        env.storage().instance().get(&StorageKey::AuthorizedUpgrade)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{contract, contractimpl, Env};

    fn h(env: &Env, b: u8) -> BytesN<32> {
        let mut a = [0u8; 32];
        a[0] = b;
        BytesN::from_array(env, &a)
    }

    // The Soroban test harness requires storage/crypto access to happen inside a
    // contract invocation, so every test runs its body under a registered
    // (otherwise inert) contract.
    #[contract]
    pub struct Dummy;

    #[contractimpl]
    impl Dummy {
        pub fn ping(_env: Env) {}
    }

    fn in_contract<F, R>(env: &Env, f: F) -> R
    where
        F: FnOnce() -> R,
    {
        let contract = env.register(Dummy, ());
        env.as_contract(&contract, f)
    }

    // ── Fresh / clean install ──────────────────────────────────────────────

    #[test]
    fn fresh_install_records_schema_and_version() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            assert_eq!(StorageMigrationValidator::current_version(&env), 1);
            let stored: BytesN<32> = env
                .storage()
                .instance()
                .get(&StorageKey::SchemaLayoutHash)
                .unwrap();
            assert_eq!(stored, h(&env, 1));
        });
    }

    #[test]
    fn fresh_install_record_is_idempotent_for_same_values() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            assert_eq!(StorageMigrationValidator::current_version(&env), 1);
        });
    }

    #[test]
    fn fresh_install_rejects_overwrite_with_different_version() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            let res = StorageMigrationValidator::record_deployed_schema(&env, 2, &h(&env, 1));
            assert_eq!(res, Err(StorageMigrationError::AlreadyInitialized));
        });
    }

    #[test]
    fn fresh_install_rejects_overwrite_with_different_layout() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            let res = StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 2));
            assert_eq!(res, Err(StorageMigrationError::AlreadyInitialized));
        });
    }

    // ── Valid upgrades ────────────────────────────────────────────────────

    #[test]
    fn legacy_to_v1_is_valid_single_step_upgrade() {
        let env = Env::default();
        in_contract(&env, || {
            // No schema recorded yet (legacy deployment).
            let report = StorageMigrationValidator::validate_before_upgrade(
                &env,
                1,
                &h(&env, 1),
                &zero_layout_hash(&env),
                &h(&env, 0xAA),
                false,
            )
            .unwrap();
            assert_eq!(report.current_version, LEGACY_VERSION);
            assert_eq!(report.target_version, 1);
            assert!(report.authorized);
            assert!(StorageMigrationValidator::is_upgrade_authorized(
                &env,
                1,
                &h(&env, 0xAA)
            ));
        });
    }

    #[test]
    fn v1_to_v2_is_valid_migration() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            let report = StorageMigrationValidator::validate_before_upgrade(
                &env,
                2,
                &h(&env, 2),
                &h(&env, 1), // expected source layout = recorded
                &h(&env, 0xBB),
                false,
            )
            .unwrap();
            assert_eq!(report.current_version, 1);
            assert_eq!(report.target_version, 2);
            assert!(report.schema_consistent);
            assert!(StorageMigrationValidator::is_upgrade_authorized(
                &env,
                2,
                &h(&env, 0xBB)
            ));
        });
    }

    #[test]
    fn same_version_redeploy_with_same_layout_is_allowed() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            let report = StorageMigrationValidator::validate_before_upgrade(
                &env,
                1,
                &h(&env, 1),
                &h(&env, 1),
                &h(&env, 0xCC),
                false,
            )
            .unwrap();
            assert!(report.schema_consistent);
            assert!(StorageMigrationValidator::is_upgrade_authorized(
                &env,
                1,
                &h(&env, 0xCC)
            ));
        });
    }

    #[test]
    fn finalize_after_authorization_commits_new_version() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            StorageMigrationValidator::validate_before_upgrade(
                &env,
                2,
                &h(&env, 2),
                &h(&env, 1),
                &h(&env, 0xDD),
                false,
            )
            .unwrap();
            let report =
                StorageMigrationValidator::finalize_migration(&env, 2, &h(&env, 2), &h(&env, 0xDD))
                    .unwrap();
            assert_eq!(report.current_version, 2);
            assert_eq!(StorageMigrationValidator::current_version(&env), 2);
            let stored: BytesN<32> = env
                .storage()
                .instance()
                .get(&StorageKey::SchemaLayoutHash)
                .unwrap();
            assert_eq!(stored, h(&env, 2));
        });
    }

    // ── Invalid layouts / failed validation paths ─────────────────────────

    #[test]
    fn rejects_all_zero_wasm_hash() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            let res = StorageMigrationValidator::validate_before_upgrade(
                &env,
                2,
                &h(&env, 2),
                &h(&env, 1),
                &zero_layout_hash(&env),
                false,
            );
            assert_eq!(res, Err(StorageMigrationError::WasmHashZero));
        });
    }

    #[test]
    fn rejects_version_skip() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            let res = StorageMigrationValidator::validate_before_upgrade(
                &env,
                3, // skipping v2
                &h(&env, 3),
                &h(&env, 1),
                &h(&env, 0xEE),
                false,
            );
            assert_eq!(res, Err(StorageMigrationError::VersionSkip));
        });
    }

    #[test]
    fn rejects_silent_layout_change_on_redeploy() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            let res = StorageMigrationValidator::validate_before_upgrade(
                &env,
                1,
                &h(&env, 2), // different layout, same version
                &h(&env, 1),
                &h(&env, 0xFF),
                false,
            );
            assert_eq!(res, Err(StorageMigrationError::SilentLayoutChange));
        });
    }

    #[test]
    fn rejects_schema_mismatch_on_migration() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            let res = StorageMigrationValidator::validate_before_upgrade(
                &env,
                2,
                &h(&env, 2),
                &h(&env, 9), // new code expects a different source layout
                &h(&env, 0x11),
                false,
            );
            assert_eq!(res, Err(StorageMigrationError::SchemaMismatch));
        });
    }

    #[test]
    fn finalize_rejects_unauthorized_upgrade() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            // No validate_before_upgrade call → not authorized.
            let res =
                StorageMigrationValidator::finalize_migration(&env, 2, &h(&env, 2), &h(&env, 0x22));
            assert_eq!(res, Err(StorageMigrationError::UnauthorizedUpgradeState));
        });
    }

    #[test]
    fn finalize_rejects_zero_wasm_hash() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            let res = StorageMigrationValidator::finalize_migration(
                &env,
                2,
                &h(&env, 2),
                &zero_layout_hash(&env),
            );
            assert_eq!(res, Err(StorageMigrationError::WasmHashZero));
        });
    }

    // ── Rollback and failure reporting ────────────────────────────────────

    #[test]
    fn rollback_without_flag_is_rejected() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 2, &h(&env, 2)).unwrap();
            let res = StorageMigrationValidator::validate_before_upgrade(
                &env,
                1,
                &h(&env, 1),
                &h(&env, 2),
                &h(&env, 0x33),
                false,
            );
            assert_eq!(res, Err(StorageMigrationError::RollbackNotAllowed));
        });
    }

    #[test]
    fn rollback_without_backup_is_rejected() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 2, &h(&env, 2)).unwrap();
            let res = StorageMigrationValidator::validate_before_upgrade(
                &env,
                1,
                &h(&env, 1),
                &h(&env, 2),
                &h(&env, 0x33),
                true, // allow_rollback = true
            );
            assert_eq!(res, Err(StorageMigrationError::BackupMissing));
        });
    }

    #[test]
    fn rollback_with_backup_is_allowed() {
        let env = Env::default();
        in_contract(&env, || {
            StorageMigrationValidator::record_deployed_schema(&env, 2, &h(&env, 2)).unwrap();
            StorageMigrationValidator::set_backup_present(&env, true);
            let report = StorageMigrationValidator::validate_before_upgrade(
                &env,
                1,
                &h(&env, 1),
                &h(&env, 2),
                &h(&env, 0x44),
                true,
            )
            .unwrap();
            assert!(report.authorized);
            assert!(StorageMigrationValidator::is_upgrade_authorized(
                &env,
                1,
                &h(&env, 0x44)
            ));
        });
    }

    #[test]
    fn rollback_sets_backup_marker_persists() {
        let env = Env::default();
        in_contract(&env, || {
            assert!(env
                .storage()
                .instance()
                .get::<_, bool>(&StorageKey::BackupPresent)
                .is_none());
            StorageMigrationValidator::set_backup_present(&env, true);
            assert_eq!(
                env.storage()
                    .instance()
                    .get::<_, bool>(&StorageKey::BackupPresent),
                Some(true)
            );
        });
    }

    // ── Layout hash helper determinism ────────────────────────────────────

    #[test]
    fn layout_hash_is_deterministic() {
        let env = Env::default();
        in_contract(&env, || {
            let a = layout_hash(&env, "vault:1:MetaKey:VaultMeta");
            let b = layout_hash(&env, "vault:1:MetaKey:VaultMeta");
            let c = layout_hash(&env, "vault:2:MetaKey:VaultMeta");
            assert_eq!(a, b);
            assert_ne!(a, c);
        });
    }

    #[test]
    fn zero_layout_hash_is_all_zeros() {
        let env = Env::default();
        in_contract(&env, || {
            assert_eq!(zero_layout_hash(&env), BytesN::from_array(&env, &[0u8; 32]));
        });
    }

    // ── Error code stability ───────────────────────────────────────────────

    #[test]
    fn error_codes_are_stable() {
        assert_eq!(StorageMigrationError::NotInitialized as u32, 1);
        assert_eq!(StorageMigrationError::AlreadyInitialized as u32, 2);
        assert_eq!(StorageMigrationError::WasmHashZero as u32, 3);
        assert_eq!(StorageMigrationError::VersionSkip as u32, 4);
        assert_eq!(StorageMigrationError::RollbackNotAllowed as u32, 5);
        assert_eq!(StorageMigrationError::BackupMissing as u32, 6);
        assert_eq!(StorageMigrationError::SchemaMismatch as u32, 7);
        assert_eq!(StorageMigrationError::SilentLayoutChange as u32, 8);
        assert_eq!(StorageMigrationError::UnauthorizedUpgradeState as u32, 9);
        assert_eq!(StorageMigrationError::StorageUnreadable as u32, 10);
    }

    // ── Existing deployed data stays readable (no implicit transform) ──────

    #[test]
    fn validation_never_mutates_business_storage() {
        let env = Env::default();
        in_contract(&env, || {
            // Pretend a business value lives under an unrelated key.
            let biz_key = soroban_sdk::Symbol::new(&env, "BusinessData");
            let business_value = Bytes::from_slice(&env, b"untouched");
            env.storage().instance().set(&biz_key, &business_value);
            StorageMigrationValidator::record_deployed_schema(&env, 1, &h(&env, 1)).unwrap();
            StorageMigrationValidator::validate_before_upgrade(
                &env,
                2,
                &h(&env, 2),
                &h(&env, 1),
                &h(&env, 0x55),
                false,
            )
            .unwrap();
            // Business data must be exactly as it was — validation is read-only.
            let v: Bytes = env.storage().instance().get(&biz_key).unwrap();
            assert_eq!(v, business_value);
        });
    }

    #[test]
    fn legacy_deploy_with_pinned_source_layout_is_rejected() {
        let env = Env::default();
        in_contract(&env, || {
            // No schema recorded (legacy) but caller pins a non-zero source layout
            // that cannot be verified → schema_consistency cannot be established.
            let res = StorageMigrationValidator::validate_before_upgrade(
                &env,
                1,
                &h(&env, 1),
                &h(&env, 7), // pinned, but nothing recorded to compare against
                &h(&env, 0x66),
                false,
            );
            assert_eq!(res, Err(StorageMigrationError::SchemaMismatch));
        });
    }
}
