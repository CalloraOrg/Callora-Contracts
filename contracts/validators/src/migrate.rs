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

use soroban_sdk::{contracttype, Address, BytesN, Env, Symbol};

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
    /// Authorised upgrade WASM hash (tuple of `(u32, BytesN<32>)`).
    AuthorisedUpgrade,
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

/// Authorise a contract upgrade for the current migration version.
///
/// This is a guard only; it does not call `update_current_contract_wasm`.
/// The deployment tool must consume the returned authorisation state and
/// perform the platform upgrade in a separate transaction.
///
/// # Arguments
///
/// * `caller` — Must be the stored admin; `caller.require_auth()` is invoked.
/// * `target_version` — Must match the stored migration version.
/// * `wasm_hash` — The WASM hash to authorise for deployment.
///
/// # Panics
///
/// | Condition | Error |
/// |-----------|-------|
/// | Caller is not the admin | contract panic (`require_auth` failure) |
/// | `target_version` ≠ stored version | contract panic |
pub fn authorize_upgrade(
    env: &Env,
    caller: &Address,
    target_version: u32,
    wasm_hash: BytesN<32>,
) {
    caller.require_auth();
    require_admin(env, caller);

    let version = storage_version(env);
    if target_version != version {
        panic!("authorize_upgrade: version mismatch");
    }

    env.storage().instance().set(
        &StorageKey::AuthorisedUpgrade,
        &(target_version, wasm_hash.clone()),
    );

    env.events()
        .publish((Symbol::new(env, "upg_authorised"), wasm_hash), target_version);
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
    use soroban_sdk::testutils::Address as _;

    /// Verify the module-level constants have the expected values.
    #[test]
    fn constants_have_expected_values() {
        assert_eq!(STORAGE_VERSION_V1, 1);
        assert_eq!(STORAGE_VERSION_V2, 2);
    }

    #[test]
    fn test_migrate_v1_to_v2() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        
        env.storage().instance().set(&StorageKey::Admin, &admin);
        
        assert_eq!(storage_version(&env), STORAGE_VERSION_V1);
        
        migrate_v1_to_v2(&env, &admin);
        
        assert_eq!(storage_version(&env), STORAGE_VERSION_V2);
        
        migrate_v1_to_v2(&env, &admin);
        assert_eq!(storage_version(&env), STORAGE_VERSION_V2);
    }

    #[test]
    #[should_panic(expected = "require_admin: caller is not the admin")]
    fn test_migrate_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let imposter = Address::generate(&env);
        
        env.storage().instance().set(&StorageKey::Admin, &admin);
        
        migrate_v1_to_v2(&env, &imposter);
    }

    #[test]
    fn test_authorize_upgrade() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        
        env.storage().instance().set(&StorageKey::Admin, &admin);
        
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        authorize_upgrade(&env, &admin, STORAGE_VERSION_V1, hash.clone());
        
        let stored: (u32, BytesN<32>) = env.storage().instance().get(&StorageKey::AuthorisedUpgrade).unwrap();
        assert_eq!(stored.0, STORAGE_VERSION_V1);
        assert_eq!(stored.1, hash);
    }

    #[test]
    #[should_panic(expected = "authorize_upgrade: version mismatch")]
    fn test_authorize_upgrade_mismatch() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        
        env.storage().instance().set(&StorageKey::Admin, &admin);
        
        let hash = BytesN::from_array(&env, &[1u8; 32]);
        authorize_upgrade(&env, &admin, STORAGE_VERSION_V2, hash.clone());
    }
}
