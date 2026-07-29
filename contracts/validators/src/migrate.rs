//! Storage migration sketch for validators.
//!
//! This module establishes the standard migration infrastructure — version
//! tracking, admin gating, and idempotency — so that the validators crate
//! follows the same pattern as other Callora contracts (see
//! `contracts/settlement/src/migrate.rs`).  No storage keys existed before
//! this module, so the V1 → V2 migration is structurally a no-op that
//! transitions from "version absent" to `STORAGE_VERSION_V2` without
//! transforming any data.
//!
//! ## Storage layout
//!
//! | Key | Added by | Contents |
//! |-----|----------|----------|
//! | `StorageVersion` | this module | `u32` — `2` after migration |
//! | `Admin` | this module | `Address` — admin who can call `migrate` |
//!
//! ## Security
//!
//! - `migrate_v1_to_v2` calls `caller.require_auth()` and verifies the caller
//!   matches the stored admin address.
//! - Re-running after `StorageVersion == 2` is a safe no-op.
//! - No `unwrap()` in production paths.

use soroban_sdk::{contracttype, Address, Env, Symbol};

/// Ledger-bump threshold for instance storage (~30 days at 5s/ledger).
pub const INSTANCE_BUMP_THRESHOLD: u32 = 17_280 * 30;
/// Ledger-bump amount for instance storage (~60 days).
pub const INSTANCE_BUMP_AMOUNT: u32 = 17_280 * 60;

/// Storage-layout version before migration (absent / no version tracking).
pub const STORAGE_VERSION_V1: u32 = 1;
/// Storage-layout version set after the V1 → V2 migration completes.
pub const STORAGE_VERSION_V2: u32 = 2;

/// Instance storage keys.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    /// Admin address authorised to run migration.
    Admin,
    /// Storage-layout version marker (`u32`).
    StorageVersion,
}

// ─── Public query ─────────────────────────────────────────────────────────────

/// Return the current storage-layout version.
///
/// Returns [`STORAGE_VERSION_V1`] when the `StorageVersion` key is absent
/// (pre-migration).  Returns [`STORAGE_VERSION_V2`] once [`migrate_v1_to_v2`]
/// has completed.
pub fn storage_version(env: &Env) -> u32 {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    env.storage()
        .instance()
        .get(&StorageKey::StorageVersion)
        .unwrap_or(STORAGE_VERSION_V1)
}

// ─── Public entry points ──────────────────────────────────────────────────────

/// One-shot V1 → V2 storage migration (admin only).
///
/// Because the validators crate held no persistent state before this migration,
/// the function simply records `STORAGE_VERSION_V2` and emits a completion
/// event.  Future migrations should replace actual data transformation here.
///
/// # Arguments
///
/// * `caller` — Must be the stored admin; `caller.require_auth()` is invoked.
///
/// # Panics
///
/// | Condition | Error |
/// |-----------|-------|
/// | Caller is not the admin | contract panic (`require_auth` failure) |
///
/// # Idempotency
///
/// Returns immediately when `StorageVersion >= STORAGE_VERSION_V2`.
pub fn migrate_v1_to_v2(env: &Env, caller: &Address) {
    caller.require_auth();
    require_admin(env, caller);

    if storage_version(env) >= STORAGE_VERSION_V2 {
        return;
    }

    env.storage()
        .instance()
        .set(&StorageKey::StorageVersion, &STORAGE_VERSION_V2);

    env.storage()
        .instance()
        .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);

    env.events()
        .publish((Symbol::new(env, "mig_v1_v2_done"),), STORAGE_VERSION_V2);
}

// ─── Internal helpers ─────────────────────────────────────────────────────────

/// Panic with a contract error if the admin key is absent or if `caller` is
/// not the stored admin.
///
/// # Note
///
/// Panics use raw strings rather than a dedicated error enum variant because
/// the validators crate is a pure library consumed by other contracts; each
/// parent contract should map these into its own error type.  When migrating
/// an actual contract that owns its own error enum, replace the `panic!()`
/// calls below with `env.panic_with_error(MyError::NotInitialized)` /
/// `env.panic_with_error(MyError::Unauthorized)`.
fn require_admin(env: &Env, caller: &Address) {
    let admin: Address = env
        .storage()
        .instance()
        .get(&StorageKey::Admin)
        .unwrap_or_else(|| panic!("require_admin: admin not set"));
    if caller != &admin {
        panic!("require_admin: caller is not the admin");
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the module-level constants have the expected values.
    #[test]
    fn constants_have_expected_values() {
        assert_eq!(STORAGE_VERSION_V1, 1);
        assert_eq!(STORAGE_VERSION_V2, 2);
    }
}
