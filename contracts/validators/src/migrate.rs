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
//! All keys below use **Instance** tier storage (`env.storage().instance()`).
//!
//! | Key | Added by | Contents |
//! |-----|----------|----------|
//! | `Admin` | this module | `Address` — admin who can call `migrate_v1_to_v2` and `authorize_upgrade` |
//! | `StorageVersion` | this module | `u32` — `2` after V1→V2 migration |
//! | `AuthorisedUpgrade` | this module | `(u32, BytesN<32>)` — (version, wasm_hash) authorisation guard |
//!
//! See `docs/storage.md` for the full tier rationale, TTL strategy, and
//! per-key design notes.
//!
//! ## Security
//!
//! - `migrate_v1_to_v2` calls `caller.require_auth()` and verifies the caller
//!   matches the stored admin address.
//! - Re-running after `StorageVersion == 2` is a safe no-op.
//! - No `unwrap()` in production paths.

use soroban_sdk::{contracttype, Address, BytesN, Env, Symbol};

/// Minimum remaining TTL (in ledgers) before the instance storage entry is
/// bumped when [`storage_version`] is read.
///
/// Set to ~30 days at 5 s/ledger (`17_280 ledgers/day × 30 days`).  If the
/// contract instance has fewer ledgers of life remaining than this threshold
/// the bump in [`storage_version`] will extend it.  Because every migration
/// and upgrade path calls [`storage_version`], this guarantees the instance
/// storage — including the `Admin` and `StorageVersion` keys — cannot be
/// pruned while the contract is actively used.
pub const INSTANCE_BUMP_THRESHOLD: u32 = 17_280 * 30;
/// TTL extension amount (in ledgers) applied to the instance storage entry
/// when the remaining TTL drops below [`INSTANCE_BUMP_THRESHOLD`].
///
/// Set to ~60 days at 5 s/ledger (`17_280 ledgers/day × 60 days`).  The 2×
/// multiplier over the threshold means a contract that is touched at least
/// once per 30-day window will keep its instance storage alive indefinitely
/// without requiring explicit TTL management from the admin.
pub const INSTANCE_BUMP_AMOUNT: u32 = 17_280 * 60;

/// Storage-layout version before migration (absent / no version tracking).
pub const STORAGE_VERSION_V1: u32 = 1;
/// Storage-layout version set after the V1 → V2 migration completes.
pub const STORAGE_VERSION_V2: u32 = 2;

/// Storage keys for the validators migration and upgrade infrastructure.
///
/// **All keys use the Instance storage tier** (`env.storage().instance()`).
/// Every variant below is a singleton — at most one value exists per key for
/// the contract's lifetime.  Instance storage is chosen over Persistent or
/// Temporary because:
///
/// - The keys fit comfortably inside the contract instance ledger entry.
/// - `Admin` and `StorageVersion` are read on every admin / upgrade path and
///   must never be independently evictable.
/// - No per-user or bulk data warrants a separate Persistent ledger entry.
/// - No transient scratchpad or reentrancy-guard behaviour requires Temporary
///   storage.
///
/// See `docs/storage.md` for the full tier rationale and TTL strategy.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    /// Admin address authorised to call [`migrate_v1_to_v2`] and
    /// [`authorize_upgrade`].  Stored as `Address`.
    ///
    /// Must be written by the initialisation routine of whatever contract
    /// embeds the validators library; calling admin-gated functions before
    /// this key is set will panic with `"require_admin: admin not set"`.
    ///
    /// **Instance-tier rationale**: Admin is a singleton configuration value
    /// consulted on every admin / upgrade entrypoint.  Using Instance storage
    /// guarantees TTL-bumped survival alongside the contract instance; a
    /// Persistent-tier admin key could be pruned independently and silently
    /// brick all admin-gated functions.
    Admin,
    /// Storage-layout version marker.  Stored as `u32`.
    ///
    /// Absence of this key is semantically equivalent to
    /// [`STORAGE_VERSION_V1`].  `migrate_*` functions compare this value to
    /// decide whether a migration needs to run, ensuring idempotency across
    /// repeated invocations.
    ///
    /// **Instance-tier rationale**: The version marker participates in the
    /// same "always-hot" lifecycle as the admin key — any migration or
    /// upgrade flow reads it first.  Persistent storage would force callers
    /// to manage a second ledger entry's TTL independently.  Temporary
    /// storage is inappropriate because a version marker must survive across
    /// ledger closes.
    StorageVersion,
    /// Authorised upgrade WASM hash paired with the target storage version.
    /// Stored as a 2-tuple `(u32, BytesN<32>)` where the first element is the
    /// expected storage version and the second is the pre-authorised WASM
    /// deployment hash.
    ///
    /// Written by [`authorize_upgrade`] and consumed (out-of-band) by the
    /// deployment tool that actually performs the platform-level
    /// `update_current_contract_wasm` call.
    ///
    /// **Instance-tier rationale**: AuthorisedUpgrade is a short-lived guard
    /// written and consumed within a single deployment cycle.  It piggybacks
    /// on the instance TTL bump from [`storage_version`] and, being a
    /// singleton, never needs the dedicated Peristent ledger entry footprint
    /// that high-cardinality per-user data would require.
    AuthorisedUpgrade,
}

// ─── Public query ─────────────────────────────────────────────────────────────

/// Return the current storage-layout version from Instance storage.
///
/// # TTL side-effect
///
/// Before reading, this function bumps the contract instance storage entry's
/// TTL using [`INSTANCE_BUMP_THRESHOLD`] and [`INSTANCE_BUMP_AMOUNT`].  Since
/// every migration and upgrade path consults the storage version first, this
/// keeps `Admin`, `StorageVersion`, and `AuthorisedUpgrade` alive as long as
/// the contract is actively used, without the admin needing to perform
/// explicit TTL maintenance.
///
/// # Return value
///
/// - [`STORAGE_VERSION_V1`] when the `StorageVersion` key is absent
///   (pre-migration state — no migrations have ever been applied).
/// - [`STORAGE_VERSION_V2`] once [`migrate_v1_to_v2`] has completed
///   successfully.
/// - Future schema bumps will return values ≥ 3.
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
        let stored: (u32, BytesN<32>) = env
            .storage()
            .instance()
            .get(&StorageKey::AuthorisedUpgrade)
            .unwrap();
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

    #[test]
    fn instance_storage_admin_round_trip() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let other = Address::generate(&env);
        assert!(env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::Admin)
            .is_none());
        env.storage().instance().set(&StorageKey::Admin, &admin);
        let stored: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .expect("Admin key should be present after set");
        assert_eq!(stored, admin);
        assert_ne!(stored, other);
        env.storage().instance().remove(&StorageKey::Admin);
        assert!(env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::Admin)
            .is_none());
    }

    #[test]
    fn instance_storage_version_round_trip() {
        let env = Env::default();
        assert!(env
            .storage()
            .instance()
            .get::<_, u32>(&StorageKey::StorageVersion)
            .is_none());
        env.storage()
            .instance()
            .set(&StorageKey::StorageVersion, &STORAGE_VERSION_V2);
        let stored: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::StorageVersion)
            .expect("StorageVersion key should be present after set");
        assert_eq!(stored, STORAGE_VERSION_V2);
        env.storage()
            .instance()
            .set(&StorageKey::StorageVersion, &99u32);
        let overwritten: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::StorageVersion)
            .unwrap();
        assert_eq!(overwritten, 99);
    }

    #[test]
    fn instance_storage_authorised_upgrade_round_trip() {
        let env = Env::default();
        assert!(env
            .storage()
            .instance()
            .get::<_, (u32, BytesN<32>)>(&StorageKey::AuthorisedUpgrade)
            .is_none());
        let version = 7u32;
        let hash = BytesN::from_array(&env, &[0xABu8; 32]);
        env.storage()
            .instance()
            .set(&StorageKey::AuthorisedUpgrade, &(version, hash.clone()));
        let stored: (u32, BytesN<32>) = env
            .storage()
            .instance()
            .get(&StorageKey::AuthorisedUpgrade)
            .expect("AuthorisedUpgrade key should be present after set");
        assert_eq!(stored.0, version);
        assert_eq!(stored.1, hash);
        let other_hash = BytesN::from_array(&env, &[0xCDu8; 32]);
        env.storage()
            .instance()
            .set(&StorageKey::AuthorisedUpgrade, &(version, other_hash.clone()));
        let updated: (u32, BytesN<32>) = env
            .storage()
            .instance()
            .get(&StorageKey::AuthorisedUpgrade)
            .unwrap();
        assert_eq!(updated.1, other_hash);
        assert_ne!(updated.1, hash);
    }

    #[test]
    fn storage_version_defaults_to_v1_when_absent() {
        let env = Env::default();
        assert_eq!(storage_version(&env), STORAGE_VERSION_V1);
    }

    #[test]
    fn storage_version_returns_written_v2() {
        let env = Env::default();
        env.storage()
            .instance()
            .set(&StorageKey::StorageVersion, &STORAGE_VERSION_V2);
        assert_eq!(storage_version(&env), STORAGE_VERSION_V2);
    }

    #[test]
    #[should_panic(expected = "require_admin: admin not set")]
    fn require_admin_panics_when_admin_absent() {
        let env = Env::default();
        let caller = Address::generate(&env);
        super::require_admin(&env, &caller);
    }

    #[test]
    fn require_admin_accepts_matching_admin() {
        let env = Env::default();
        let admin = Address::generate(&env);
        env.storage().instance().set(&StorageKey::Admin, &admin);
        super::require_admin(&env, &admin);
    }

    #[test]
    #[should_panic(expected = "require_admin: caller is not the admin")]
    fn require_admin_rejects_non_admin() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let imposter = Address::generate(&env);
        env.storage().instance().set(&StorageKey::Admin, &admin);
        super::require_admin(&env, &imposter);
    }

    #[test]
    fn migrate_v1_to_v2_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&StorageKey::Admin, &admin);
        migrate_v1_to_v2(&env, &admin);
        let events = env.events().all();
        assert_eq!(events.len(), 1);
        let (topic, payload): ((Symbol,), u32) =
            events.into_iter().next().unwrap().tuple();
        assert_eq!(topic.0, Symbol::new(&env, "mig_v1_v2_done"));
        assert_eq!(payload, STORAGE_VERSION_V2);
    }

    #[test]
    fn migrate_v1_to_v2_is_idempotent_no_extra_events() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&StorageKey::Admin, &admin);
        migrate_v1_to_v2(&env, &admin);
        migrate_v1_to_v2(&env, &admin);
        migrate_v1_to_v2(&env, &admin);
        assert_eq!(env.events().all().len(), 1);
        assert_eq!(storage_version(&env), STORAGE_VERSION_V2);
    }

    #[test]
    fn authorize_upgrade_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&StorageKey::Admin, &admin);
        let hash = BytesN::from_array(&env, &[0x42u8; 32]);
        authorize_upgrade(&env, &admin, STORAGE_VERSION_V1, hash.clone());
        let events = env.events().all();
        assert_eq!(events.len(), 1);
        let (topic, payload): ((Symbol, BytesN<32>), u32) =
            events.into_iter().next().unwrap().tuple();
        assert_eq!(topic.0, Symbol::new(&env, "upg_authorised"));
        assert_eq!(topic.1, hash);
        assert_eq!(payload, STORAGE_VERSION_V1);
    }

    #[test]
    fn ttl_constants_match_documentation() {
        let ledgers_per_day = 17_280u32;
        assert_eq!(INSTANCE_BUMP_THRESHOLD, ledgers_per_day * 30);
        assert_eq!(INSTANCE_BUMP_AMOUNT, ledgers_per_day * 60);
        assert!(INSTANCE_BUMP_AMOUNT > INSTANCE_BUMP_THRESHOLD);
    }

    #[test]
    fn storage_keys_do_not_collide_in_instance() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0xEEu8; 32]);
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::StorageVersion, &STORAGE_VERSION_V2);
        env.storage()
            .instance()
            .set(&StorageKey::AuthorisedUpgrade, &(STORAGE_VERSION_V2, hash.clone()));
        let stored_admin: Address = env.storage().instance().get(&StorageKey::Admin).unwrap();
        let stored_ver: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::StorageVersion)
            .unwrap();
        let stored_upg: (u32, BytesN<32>) = env
            .storage()
            .instance()
            .get(&StorageKey::AuthorisedUpgrade)
            .unwrap();
        assert_eq!(stored_admin, admin);
        assert_eq!(stored_ver, STORAGE_VERSION_V2);
        assert_eq!(stored_upg.0, STORAGE_VERSION_V2);
        assert_eq!(stored_upg.1, hash);
    }

    #[test]
    fn storage_version_handles_future_versions() {
        let env = Env::default();
        env.storage()
            .instance()
            .set(&StorageKey::StorageVersion, &42u32);
        assert_eq!(storage_version(&env), 42);
        assert!(storage_version(&env) > STORAGE_VERSION_V2);
    }

    #[test]
    fn migrate_skips_when_version_already_higher() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::StorageVersion, &99u32);
        migrate_v1_to_v2(&env, &admin);
        let stored: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::StorageVersion)
            .unwrap();
        assert_eq!(stored, 99);
        assert_eq!(env.events().all().len(), 0);
    }

    // ─── Tier segregation tests ──────────────────────────────────────────────

    #[test]
    fn tier_segregation_migrate_writes_instance_only() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&StorageKey::Admin, &admin);
        migrate_v1_to_v2(&env, &admin);
        assert_eq!(
            env.storage()
                .instance()
                .get::<_, u32>(&StorageKey::StorageVersion)
                .unwrap(),
            STORAGE_VERSION_V2
        );
        assert!(env
            .storage()
            .persistent()
            .get::<_, u32>(&StorageKey::StorageVersion)
            .is_none());
        assert!(env
            .storage()
            .temporary()
            .get::<_, u32>(&StorageKey::StorageVersion)
            .is_none());
    }

    #[test]
    fn tier_segregation_authorize_upgrade_writes_instance_only() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&StorageKey::Admin, &admin);
        let hash = BytesN::from_array(&env, &[0x55u8; 32]);
        authorize_upgrade(&env, &admin, STORAGE_VERSION_V1, hash.clone());
        let stored: (u32, BytesN<32>) = env
            .storage()
            .instance()
            .get(&StorageKey::AuthorisedUpgrade)
            .unwrap();
        assert_eq!(stored.0, STORAGE_VERSION_V1);
        assert_eq!(stored.1, hash);
        assert!(env
            .storage()
            .persistent()
            .get::<_, (u32, BytesN<32>)>(&StorageKey::AuthorisedUpgrade)
            .is_none());
        assert!(env
            .storage()
            .temporary()
            .get::<_, (u32, BytesN<32>)>(&StorageKey::AuthorisedUpgrade)
            .is_none());
    }

    #[test]
    fn tier_segregation_admin_reads_from_instance_only() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let fake_admin = Address::generate(&env);
        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().persistent().set(&StorageKey::Admin, &fake_admin);
        env.storage().temporary().set(&StorageKey::Admin, &fake_admin);
        super::require_admin(&env, &admin);
    }

    #[test]
    fn tier_segregation_storage_version_reads_from_instance_only() {
        let env = Env::default();
        env.storage()
            .instance()
            .set(&StorageKey::StorageVersion, &STORAGE_VERSION_V2);
        env.storage()
            .persistent()
            .set(&StorageKey::StorageVersion, &999u32);
        env.storage()
            .temporary()
            .set(&StorageKey::StorageVersion, &888u32);
        assert_eq!(storage_version(&env), STORAGE_VERSION_V2);
    }

    #[test]
    fn persistent_tier_can_store_storagekey_discriminants_but_is_unused() {
        let env = Env::default();
        let admin = Address::generate(&env);
        env.storage().persistent().set(&StorageKey::Admin, &admin);
        assert_eq!(
            env.storage()
                .persistent()
                .get::<_, Address>(&StorageKey::Admin)
                .unwrap(),
            admin
        );
        assert!(env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::Admin)
            .is_none());
    }

    #[test]
    fn temporary_tier_can_store_storagekey_discriminants_but_is_unused() {
        let env = Env::default();
        env.storage()
            .temporary()
            .set(&StorageKey::StorageVersion, &42u32);
        assert_eq!(
            env.storage()
                .temporary()
                .get::<_, u32>(&StorageKey::StorageVersion)
                .unwrap(),
            42
        );
        assert_eq!(storage_version(&env), STORAGE_VERSION_V1);
    }

    // ─── TTL bump path coverage ──────────────────────────────────────────────

    #[test]
    fn storage_version_repeated_calls_are_idempotent() {
        let env = Env::default();
        env.storage()
            .instance()
            .set(&StorageKey::StorageVersion, &STORAGE_VERSION_V2);
        for _ in 0..10 {
            assert_eq!(storage_version(&env), STORAGE_VERSION_V2);
        }
    }

    #[test]
    fn storage_version_repeated_calls_with_absent_key_returns_v1() {
        let env = Env::default();
        for _ in 0..5 {
            assert_eq!(storage_version(&env), STORAGE_VERSION_V1);
        }
        assert!(env
            .storage()
            .instance()
            .get::<_, u32>(&StorageKey::StorageVersion)
            .is_none());
    }

    // ─── AuthorisedUpgrade overwrite coverage ────────────────────────────────

    #[test]
    fn authorize_upgrade_overwrites_previous_authorisation() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        env.storage().instance().set(&StorageKey::Admin, &admin);
        let hash_v1 = BytesN::from_array(&env, &[0x11u8; 32]);
        authorize_upgrade(&env, &admin, STORAGE_VERSION_V1, hash_v1.clone());
        migrate_v1_to_v2(&env, &admin);
        let hash_v2 = BytesN::from_array(&env, &[0x22u8; 32]);
        authorize_upgrade(&env, &admin, STORAGE_VERSION_V2, hash_v2.clone());
        let stored: (u32, BytesN<32>) = env
            .storage()
            .instance()
            .get(&StorageKey::AuthorisedUpgrade)
            .unwrap();
        assert_eq!(stored.0, STORAGE_VERSION_V2);
        assert_eq!(stored.1, hash_v2);
        assert_ne!(stored.1, hash_v1);
    }

    // ─── StorageKey enum exhaustiveness ──────────────────────────────────────

    #[test]
    fn storagekey_enum_has_exactly_three_variants() {
        let env = Env::default();
        let address = Address::generate(&env);
        let hash = BytesN::from_array(&env, &[0u8; 32]);
        let keys = [
            StorageKey::Admin,
            StorageKey::StorageVersion,
            StorageKey::AuthorisedUpgrade,
        ];
        assert_eq!(keys.len(), 3);
        env.storage().instance().set(&keys[0], &address);
        env.storage().instance().set(&keys[1], &7u32);
        env.storage().instance().set(&keys[2], &(1u32, hash));
        for k in keys.iter() {
            match k {
                StorageKey::Admin => {
                    let _: Address = env.storage().instance().get(k).unwrap();
                }
                StorageKey::StorageVersion => {
                    let _: u32 = env.storage().instance().get(k).unwrap();
                }
                StorageKey::AuthorisedUpgrade => {
                    let _: (u32, BytesN<32>) = env.storage().instance().get(k).unwrap();
                }
            }
        }
    }
}
