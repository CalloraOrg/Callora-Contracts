#![no_std]
#![allow(clippy::enum_variant_names)]
//!
//! # Callora Whitelist Contract
//!
//! Manages a whitelist of authorized addresses. Admin actions that modify the
//! whitelist — `add_address`, `remove_address`, `clear_all` — are protected by
//! a configurable **cool-off window** that prevents rapid successive operations.
//!
//! ## Cool-Off Mechanism
//!
//! Each successful state-changing admin action arms a global timer. Subsequent
//! actions are rejected until the window elapses. This gives monitors and
//! off-chain governance time to react before another whitelist mutation can occur.
//!
//! - Default window: **1 hour** (`DEFAULT_COOLDOWN_SECONDS`)
//! - Configurable range: **1 second** – **30 days**
//! - Configured via `set_admin_cooldown` (admin only)
//!
//! ## Admin Roles
//!
//! - **Owner**: set at `init`, can transfer ownership (two-step)
//! - **Admin**: defaults to owner, can be transferred (two-step)
//!
//! Both the owner and admin can manage the whitelist, but the cool-off window
//! applies regardless of which role performs the action.
//!
//! ## Storage layout (v2)
//!
//! One persistent entry per address, so `is_whitelisted` is a single keyed
//! lookup (O(1)) and instance storage stays constant-size.
//!
//! - `Whitelisted(epoch, address) -> slot` (persistent): presence == whitelisted.
//! - `WhitelistSlot(epoch, slot) -> address` (persistent): dense index used for
//!   paging and O(1) swap-remove.
//! - `WhitelistEpoch`, `WhitelistCount`, `WhitelistMigrated`,
//!   `WhitelistMigrationCursor` (instance): small scalars only.
//!
//! `clear_all` bumps the epoch and resets the count, invalidating every entry
//! in O(1). Stale entries expire via TTL.
//!
//! ## Ordering
//!
//! `remove_address` uses swap-remove, so order is insertion order only until
//! the first removal.
//!
//! ## Legacy migration
//!
//! A legacy `Vec<Address>` under `WhitelistList` is migrated in batches with
//! `migrate_legacy`. Until it completes, reads fall back to the legacy vector
//! and mutations fail with `MigrationPending`.

mod errors;
pub use errors::WhitelistError;

pub mod admin;
pub mod events;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

/// Instance / persistent storage keys for the whitelist contract.
///
/// New variants are only ever appended so existing on-chain keys keep their
/// encoding.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    /// Contract owner address.
    WhitelistOwner,
    /// Current admin address (defaults to owner at init).
    WhitelistAdmin,
    /// Pending admin address awaiting acceptance (two-step transfer).
    WhitelistPendingAdmin,
    /// LEGACY: instance vector of whitelisted addresses. Only read or removed
    /// by the migration path.
    WhitelistList,
    /// Global cool-off window between critical whitelist admin executions.
    WhitelistAdminCooldown,
    /// Audit record for the most recently executed critical whitelist admin action.
    WhitelistLastCriticalAction,
    /// Instance: current epoch (u32), bumped by `clear_all`.
    WhitelistEpoch,
    /// Instance: number of entries in the current epoch (u32).
    WhitelistCount,
    /// Instance: true once no legacy vector remains.
    WhitelistMigrated,
    /// Instance: next legacy index to migrate (u32).
    WhitelistMigrationCursor,
    /// Persistent: (epoch, address) -> slot.
    Whitelisted(u32, Address),
    /// Persistent: (epoch, slot) -> address.
    WhitelistSlot(u32, u32),
}

// ---------------------------------------------------------------------------
// TTL Constants
// ---------------------------------------------------------------------------

/// Ledgers per day at a 5-second close cadence.
const LEDGERS_PER_DAY: u32 = 17_280;

/// TTL extension trigger for instance storage keys (~30 days of ledgers at 5 s/ledger).
pub const INSTANCE_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;

/// TTL extension target for instance storage keys (~60 days of ledgers at 5 s/ledger).
pub const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 60;

/// TTL extension trigger for persistent whitelist entries (~30 days).
pub const PERSISTENT_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;

/// TTL extension target for persistent whitelist entries (~60 days).
pub const PERSISTENT_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 60;

/// Maximum entries returned by `get_whitelist_page`.
pub const MAX_PAGE_SIZE: u32 = 100;

/// Maximum legacy entries migrated per `migrate_legacy` call.
pub const MAX_MIGRATION_BATCH: u32 = 100;

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct CalloraWhitelist;

#[contractimpl]
impl CalloraWhitelist {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// Initialize the whitelist contract (one-time setup).
    ///
    /// Sets the contract owner and admin (both default to `admin`). The contract
    /// may only be initialized once. Fresh deployments never have a legacy
    /// vector, so they start fully migrated.
    ///
    /// # Parameters
    /// - `admin` — initial owner and admin address.
    ///
    /// # Errors
    /// - [`WhitelistError::AlreadyInitialized`] if `init` has already been called.
    pub fn init(env: Env, admin: Address) -> Result<(), WhitelistError> {
        admin.require_auth();
        if env.storage().instance().has(&StorageKey::WhitelistOwner) {
            return Err(WhitelistError::AlreadyInitialized);
        }

        env.storage()
            .instance()
            .set(&StorageKey::WhitelistOwner, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::WhitelistAdmin, &admin);
        env.storage()
            .instance()
            .set(&StorageKey::WhitelistMigrated, &true);

        Self::bump_instance_ttl(&env);
        env.events().publish(
            (
                events::event_init(&env),
                events::event_version_v1(&env),
                admin.clone(),
            ),
            admin,
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin management
    // -----------------------------------------------------------------------

    /// Return the current admin address.
    ///
    /// Defaults to the owner when no admin has been explicitly set.
    ///
    /// # Errors
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    pub fn get_admin(env: Env) -> Result<Address, WhitelistError> {
        Self::bump_instance_ttl(&env);
        env.storage()
            .instance()
            .get::<_, Address>(&StorageKey::WhitelistAdmin)
            .ok_or(WhitelistError::NotInitialized)
    }

    /// Initiate a two-step admin transfer (current admin only).
    ///
    /// The nominated admin must call [`accept_admin`] to complete the transfer.
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    /// - `new_admin` — Address to nominate as the next admin.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NewAdminSameAsCurrent`] if `new_admin` is the same as the current admin.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), WhitelistError> {
        Self::require_admin(&env, &caller)?;

        let current_admin = env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::WhitelistAdmin)
            .ok_or(WhitelistError::NotInitialized)?;

        if new_admin == current_admin {
            return Err(WhitelistError::NewAdminSameAsCurrent);
        }

        env.storage()
            .instance()
            .set(&StorageKey::WhitelistPendingAdmin, &new_admin);
        env.events().publish(
            (Symbol::new(&env, "admin_nominated"), caller.clone()),
            new_admin.clone(),
        );
        Self::bump_instance_ttl(&env);
        env.events().publish(
            (
                events::event_admin_nominated(&env),
                events::event_version_v1(&env),
                caller,
                new_admin.clone(),
            ),
            new_admin,
        );
        Ok(())
    }

    /// Accept a pending admin transfer (pending admin only).
    ///
    /// # Errors
    /// - [`WhitelistError::NoAdminTransferPending`] if no admin transfer has been initiated.
    pub fn accept_admin(env: Env) -> Result<(), WhitelistError> {
        let new_admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::WhitelistPendingAdmin)
            .ok_or(WhitelistError::NoAdminTransferPending)?;
        new_admin.require_auth();

        env.storage()
            .instance()
            .set(&StorageKey::WhitelistAdmin, &new_admin);
        env.storage()
            .instance()
            .remove(&StorageKey::WhitelistPendingAdmin);
        env.events().publish(
            (Symbol::new(&env, "admin_accepted"), new_admin.clone()),
            new_admin.clone(),
        );
        Self::bump_instance_ttl(&env);
        env.events().publish(
            (
                events::event_admin_accepted(&env),
                events::event_version_v1(&env),
                new_admin.clone(),
            ),
            new_admin,
        );
        Ok(())
    }

    /// Cancel a pending admin transfer (current admin only).
    ///
    /// Removes any pending admin nomination so a stale nomination cannot be
    /// accepted later. Emits an `admin_cancelled` event.
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    /// - [`WhitelistError::NoAdminTransferPending`] if no admin transfer has been initiated.
    pub fn cancel_admin_transfer(env: Env, caller: Address) -> Result<(), WhitelistError> {
        Self::require_admin(&env, &caller)?;

        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::WhitelistPendingAdmin)
            .ok_or(WhitelistError::NoAdminTransferPending)?;

        env.storage()
            .instance()
            .remove(&StorageKey::WhitelistPendingAdmin);
        env.events().publish(
            (Symbol::new(&env, "admin_cancelled"), caller.clone()),
            pending,
        );
        Self::bump_instance_ttl(&env);
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Whitelist management
    // -----------------------------------------------------------------------

    /// Add an address to the whitelist (admin only, cooldown-gated).
    ///
    /// If the address is already present, returns
    /// [`WhitelistError::AddressAlreadyInWhitelist`].
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    /// - `address` — Address to add to the whitelist.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    /// - [`WhitelistError::MigrationPending`] if a legacy vector is still being migrated.
    /// - [`WhitelistError::AdminCooldownActive`] if another action's cool-off is still active.
    /// - [`WhitelistError::AddressAlreadyInWhitelist`] if the address is already whitelisted.
    pub fn add_address(env: Env, caller: Address, address: Address) -> Result<(), WhitelistError> {
        // `require_admin` performs `caller.require_auth()`; authorizing twice in
        // the same invocation frame is rejected by the host (`Auth::ExistingValue`).
        Self::require_admin(&env, &caller)?;
        Self::require_migrated(&env)?;

        admin::guard(&env, Symbol::new(&env, "add_address"))?;

        if !Self::insert(&env, &address) {
            return Err(WhitelistError::AddressAlreadyInWhitelist);
        }

        Self::bump_instance_ttl(&env);
        env.events().publish(
            (
                events::event_address_added(&env),
                events::event_version_v1(&env),
                caller,
                address,
            ),
            (),
        );
        Ok(())
    }

    /// Remove an address from the whitelist (admin only, cooldown-gated).
    ///
    /// Uses swap-remove, so the last entry takes the removed entry's slot.
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    /// - `address` — Address to remove from the whitelist.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    /// - [`WhitelistError::MigrationPending`] if a legacy vector is still being migrated.
    /// - [`WhitelistError::AdminCooldownActive`] if another action's cool-off is still active.
    /// - [`WhitelistError::AddressNotInWhitelist`] if the address is not whitelisted.
    pub fn remove_address(
        env: Env,
        caller: Address,
        address: Address,
    ) -> Result<(), WhitelistError> {
        Self::require_admin(&env, &caller)?;
        Self::require_migrated(&env)?;

        admin::guard(&env, Symbol::new(&env, "remove_address"))?;

        if !Self::remove(&env, &address) {
            return Err(WhitelistError::AddressNotInWhitelist);
        }

        Self::bump_instance_ttl(&env);
        env.events().publish(
            (
                events::event_address_removed(&env),
                events::event_version_v1(&env),
                caller,
                address,
            ),
            (),
        );
        Ok(())
    }

    /// Remove all addresses from the whitelist (admin only, cooldown-gated).
    ///
    /// O(1): bumps the epoch so every existing entry becomes unreachable. This
    /// operation is idempotent — calling it on an empty whitelist succeeds but
    /// still arms the cool-off window.
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    /// - [`WhitelistError::MigrationPending`] if a legacy vector is still being migrated.
    /// - [`WhitelistError::AdminCooldownActive`] if another action's cool-off is still active.
    pub fn clear_all(env: Env, caller: Address) -> Result<(), WhitelistError> {
        Self::require_admin(&env, &caller)?;
        Self::require_migrated(&env)?;

        admin::guard(&env, Symbol::new(&env, "clear_all"))?;

        let cleared = Self::count(&env);
        let next = Self::epoch(&env).checked_add(1).expect("epoch overflow");
        env.storage()
            .instance()
            .set(&StorageKey::WhitelistEpoch, &next);
        env.storage()
            .instance()
            .set(&StorageKey::WhitelistCount, &0u32);

        Self::bump_instance_ttl(&env);
        env.events().publish(
            (
                events::event_whitelist_cleared(&env),
                events::event_version_v1(&env),
                caller,
            ),
            cleared,
        );
        Ok(())
    }

    /// Check whether an address is in the whitelist.
    ///
    /// No authentication required — this is a public read-only view function.
    /// O(1) keyed lookup. While a migration is pending it also falls back to
    /// the legacy vector.
    ///
    /// # Returns
    /// `true` if the address is present in the whitelist, `false` otherwise.
    pub fn is_whitelisted(env: Env, address: Address) -> bool {
        let key = StorageKey::Whitelisted(Self::epoch(&env), address.clone());
        if env.storage().persistent().has(&key) {
            return true;
        }
        if Self::is_migrated(&env) {
            return false;
        }
        Self::legacy_list(&env).contains(&address)
    }

    /// Return the current whitelist.
    ///
    /// No authentication required — this is a public read-only view function.
    /// O(n); prefer [`get_whitelist_page`] for large lists. Order is insertion
    /// order until the first removal (see swap-remove).
    ///
    /// # Returns
    /// `Vec<Address>` containing all addresses currently in the whitelist.
    /// Returns an empty vector if no whitelist has been configured.
    pub fn get_whitelist(env: Env) -> Vec<Address> {
        Self::bump_instance_ttl(&env);
        if !Self::is_migrated(&env) {
            return Self::legacy_list(&env);
        }
        let epoch = Self::epoch(&env);
        let count = Self::count(&env);
        let mut out = Vec::new(&env);
        for slot in 0..count {
            if let Some(a) = Self::address_at(&env, epoch, slot) {
                out.push_back(a);
            }
        }
        out
    }

    /// Return up to `limit` addresses starting at slot `start`.
    ///
    /// `limit` is clamped to [`MAX_PAGE_SIZE`]. Read-only, no authentication.
    pub fn get_whitelist_page(env: Env, start: u32, limit: u32) -> Vec<Address> {
        Self::bump_instance_ttl(&env);
        let limit = limit.min(MAX_PAGE_SIZE);

        if !Self::is_migrated(&env) {
            let legacy = Self::legacy_list(&env);
            let end = start.saturating_add(limit).min(legacy.len());
            if start >= end {
                return Vec::new(&env);
            }
            return legacy.slice(start..end);
        }

        let epoch = Self::epoch(&env);
        let end = start.saturating_add(limit).min(Self::count(&env));
        let mut out = Vec::new(&env);
        let mut slot = start;
        while slot < end {
            if let Some(a) = Self::address_at(&env, epoch, slot) {
                out.push_back(a);
            }
            slot += 1;
        }
        out
    }

    /// Return the number of whitelisted addresses.
    pub fn whitelist_count(env: Env) -> u32 {
        if Self::is_migrated(&env) {
            Self::count(&env)
        } else {
            Self::legacy_list(&env).len()
        }
    }

    // -----------------------------------------------------------------------
    // Legacy migration
    // -----------------------------------------------------------------------

    /// Migrate up to `limit` legacy entries into keyed persistent storage.
    ///
    /// `limit` is clamped to `1..=MAX_MIGRATION_BATCH`. Admin only. Idempotent:
    /// once migration is complete it returns `true` without changing state.
    ///
    /// # Returns
    /// `true` when the migration is complete, `false` if more batches remain.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    pub fn migrate_legacy(env: Env, caller: Address, limit: u32) -> Result<bool, WhitelistError> {
        Self::require_admin(&env, &caller)?;

        if Self::is_migrated(&env) {
            return Ok(true);
        }

        let limit = limit.clamp(1, MAX_MIGRATION_BATCH);
        let legacy = Self::legacy_list(&env);
        let cursor: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::WhitelistMigrationCursor)
            .unwrap_or(0);
        let end = cursor.saturating_add(limit).min(legacy.len());

        let mut i = cursor;
        while i < end {
            if let Some(a) = legacy.get(i) {
                Self::insert(&env, &a);
            }
            i += 1;
        }

        let done = end >= legacy.len();
        if done {
            env.storage().instance().remove(&StorageKey::WhitelistList);
            env.storage()
                .instance()
                .remove(&StorageKey::WhitelistMigrationCursor);
            env.storage()
                .instance()
                .set(&StorageKey::WhitelistMigrated, &true);
        } else {
            env.storage()
                .instance()
                .set(&StorageKey::WhitelistMigrationCursor, &end);
        }

        Self::bump_instance_ttl(&env);
        Ok(done)
    }

    /// Return whether no legacy vector remains.
    pub fn is_migration_complete(env: Env) -> bool {
        Self::is_migrated(&env)
    }

    // -----------------------------------------------------------------------
    // Cooldown management
    // -----------------------------------------------------------------------

    /// Return the configured admin cool-off window in seconds.
    ///
    /// Defaults to [`admin::DEFAULT_COOLDOWN_SECONDS`] (1 hour) when no window
    /// has been explicitly set. This read-only view requires no authorization.
    pub fn get_admin_cooldown(env: Env) -> u64 {
        admin::get_cooldown(&env)
    }

    /// Configure the admin cool-off window (admin only).
    ///
    /// # Bounds
    /// - Minimum: [`admin::MIN_COOLDOWN_SECONDS`] (1 s).
    /// - Maximum: [`admin::MAX_COOLDOWN_SECONDS`] (30 d).
    /// - Default: [`admin::DEFAULT_COOLDOWN_SECONDS`] (1 h).
    ///
    /// # Authorization
    /// `caller` must be the current admin and must authorize this invocation.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] when `caller` is not the current admin.
    /// - [`WhitelistError::NotInitialized`] when the contract has no configured admin.
    /// - [`WhitelistError::InvalidAdminCooldown`] when `seconds` is outside bounds.
    pub fn set_admin_cooldown(
        env: Env,
        caller: Address,
        seconds: u64,
    ) -> Result<(), WhitelistError> {
        Self::require_admin(&env, &caller)?;
        admin::set_cooldown(&env, seconds)?;
        Self::bump_instance_ttl(&env);
        env.events().publish(
            (
                events::event_admin_cooldown_set(&env),
                events::event_version_v1(&env),
                caller,
            ),
            seconds,
        );
        Ok(())
    }

    /// Return seconds remaining before another critical whitelist action may run.
    ///
    /// Returns `0` when no cooldown is active.
    pub fn admin_cooldown_remaining(env: Env) -> u64 {
        admin::remaining(&env)
    }

    /// Return whether a critical whitelist action may execute now.
    pub fn is_admin_action_ready(env: Env) -> bool {
        admin::is_ready(&env)
    }

    /// Return the most recently executed critical whitelist admin action, if any.
    pub fn get_last_critical_admin_action(env: Env) -> Option<admin::CriticalAdminAction> {
        admin::last_action(&env)
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Require the caller to be the current admin.
    fn require_admin(env: &Env, caller: &Address) -> Result<(), WhitelistError> {
        caller.require_auth();
        let admin = env
            .storage()
            .instance()
            .get::<_, Address>(&StorageKey::WhitelistAdmin)
            .ok_or(WhitelistError::NotInitialized)?;
        if caller != &admin {
            return Err(WhitelistError::Unauthorized);
        }
        Ok(())
    }

    /// Fail while a legacy vector is still waiting to be migrated.
    fn require_migrated(env: &Env) -> Result<(), WhitelistError> {
        if Self::is_migrated(env) {
            Ok(())
        } else {
            Err(WhitelistError::MigrationPending)
        }
    }

    fn is_migrated(env: &Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&StorageKey::WhitelistMigrated)
            .unwrap_or(false)
    }

    fn epoch(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get::<_, u32>(&StorageKey::WhitelistEpoch)
            .unwrap_or(0)
    }

    fn count(env: &Env) -> u32 {
        env.storage()
            .instance()
            .get::<_, u32>(&StorageKey::WhitelistCount)
            .unwrap_or(0)
    }

    fn legacy_list(env: &Env) -> Vec<Address> {
        env.storage()
            .instance()
            .get::<_, Vec<Address>>(&StorageKey::WhitelistList)
            .unwrap_or_else(|| Vec::new(env))
    }

    fn address_at(env: &Env, epoch: u32, slot: u32) -> Option<Address> {
        env.storage()
            .persistent()
            .get::<_, Address>(&StorageKey::WhitelistSlot(epoch, slot))
    }

    /// Insert in the current epoch. Returns false if already present.
    fn insert(env: &Env, address: &Address) -> bool {
        let epoch = Self::epoch(env);
        let key = StorageKey::Whitelisted(epoch, address.clone());
        if env.storage().persistent().has(&key) {
            return false;
        }

        let slot = Self::count(env);
        let slot_key = StorageKey::WhitelistSlot(epoch, slot);
        env.storage().persistent().set(&key, &slot);
        env.storage().persistent().set(&slot_key, address);
        Self::bump_persistent(env, &key);
        Self::bump_persistent(env, &slot_key);

        env.storage()
            .instance()
            .set(&StorageKey::WhitelistCount, &(slot + 1));
        true
    }

    /// Swap-remove in the current epoch. Returns false if absent.
    fn remove(env: &Env, address: &Address) -> bool {
        let epoch = Self::epoch(env);
        let key = StorageKey::Whitelisted(epoch, address.clone());
        let slot: u32 = match env.storage().persistent().get(&key) {
            Some(s) => s,
            None => return false,
        };

        let last = Self::count(env) - 1;
        if slot != last {
            let moved = Self::address_at(env, epoch, last).expect("index corrupted");
            let moved_key = StorageKey::Whitelisted(epoch, moved.clone());
            let slot_key = StorageKey::WhitelistSlot(epoch, slot);
            env.storage().persistent().set(&slot_key, &moved);
            env.storage().persistent().set(&moved_key, &slot);
            Self::bump_persistent(env, &slot_key);
            Self::bump_persistent(env, &moved_key);
        }
        env.storage()
            .persistent()
            .remove(&StorageKey::WhitelistSlot(epoch, last));
        env.storage().persistent().remove(&key);
        env.storage()
            .instance()
            .set(&StorageKey::WhitelistCount, &last);
        true
    }

    /// Extend persistent entry TTL when it falls below the threshold.
    #[inline]
    fn bump_persistent(env: &Env, key: &StorageKey) {
        env.storage()
            .persistent()
            .extend_ttl(key, PERSISTENT_BUMP_THRESHOLD, PERSISTENT_BUMP_AMOUNT);
    }

    /// Extend instance storage TTL to `INSTANCE_BUMP_AMOUNT` when the remaining
    /// TTL falls below `INSTANCE_BUMP_THRESHOLD`.
    #[inline]
    pub(crate) fn bump_instance_ttl(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::testutils::Address as _;
    use soroban_sdk::testutils::Ledger as _;
    use soroban_sdk::Env;

    /// Deploy a fresh whitelist contract, init with `admin`, and return
    /// `(env, admin, client)`.
    fn deploy_whitelist<'a>(env: &'a Env, admin: &Address) -> CalloraWhitelistClient<'a> {
        let contract_id = env.register(CalloraWhitelist, ());
        let client = CalloraWhitelistClient::new(env, &contract_id);
        client.init(admin);
        client
    }

    /// Put the contract into the pre-migration state: a legacy vector and
    /// `WhitelistMigrated = false`.
    fn seed_legacy(env: &Env, client: &CalloraWhitelistClient, addrs: &[Address]) {
        env.as_contract(&client.address, || {
            let mut legacy = Vec::new(env);
            for a in addrs {
                legacy.push_back(a.clone());
            }
            env.storage()
                .instance()
                .set(&StorageKey::WhitelistList, &legacy);
            env.storage()
                .instance()
                .set(&StorageKey::WhitelistMigrated, &false);
        });
    }

    // -----------------------------------------------------------------------
    // Init tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_init_sets_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    fn test_init_rejects_double_init() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        let result = client.try_init(&admin);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Admin management tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_set_and_accept_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin(&admin, &new_admin);
        client.accept_admin();
        assert_eq!(client.get_admin(), new_admin);
    }

    #[test]
    fn test_non_admin_cannot_set_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let intruder = Address::generate(&env);
        let new_admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        let result = client.try_set_admin(&intruder, &new_admin);
        assert!(result.is_err());
    }

    #[test]
    fn test_set_admin_same_as_current_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        let result = client.try_set_admin(&admin, &admin);
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::NewAdminSameAsCurrent)
        );
    }

    #[test]
    fn test_accept_admin_without_pending_fails() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        let result = client.try_accept_admin();
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::NoAdminTransferPending)
        );
    }

    // -----------------------------------------------------------------------
    // Whitelist management tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_and_check_address() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        client.add_address(&admin, &addr);
        assert!(client.is_whitelisted(&addr));
    }

    #[test]
    fn test_add_duplicate_address_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        // Use a short cooldown so we can test duplicates without long waits.
        client.set_admin_cooldown(&admin, &1);

        // First add — succeeds
        client.add_address(&admin, &addr);

        // Advance past the 1-second cooldown so we can call add_address again.
        env.ledger().set_timestamp(1_000_001);

        // Second add (same address) — should fail with AddressAlreadyInWhitelist
        let result = client.try_add_address(&admin, &addr);
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::AddressAlreadyInWhitelist)
        );
    }

    #[test]
    fn test_remove_address() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        client.add_address(&admin, &addr);
        assert!(client.is_whitelisted(&addr));

        env.ledger().set_timestamp(1_000_001);
        client.remove_address(&admin, &addr);
        assert!(!client.is_whitelisted(&addr));
    }

    #[test]
    fn test_remove_nonexistent_address_fails() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        let result = client.try_remove_address(&admin, &addr);
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::AddressNotInWhitelist)
        );
    }

    #[test]
    fn test_clear_all_removes_all_addresses() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        client.add_address(&admin, &addr1);
        env.ledger().set_timestamp(1_000_001);
        client.add_address(&admin, &addr2);

        assert_eq!(client.get_whitelist().len(), 2);

        env.ledger().set_timestamp(1_000_002);
        client.clear_all(&admin);
        assert!(client.get_whitelist().is_empty());
    }

    #[test]
    fn test_is_whitelisted_returns_false_for_uninit() {
        let env = Env::default();
        let addr = Address::generate(&env);

        let contract_id = env.register(CalloraWhitelist, ());
        let client = CalloraWhitelistClient::new(&env, &contract_id);

        // Before init, whitelist should be empty
        assert!(client.get_whitelist().is_empty());
        assert!(!client.is_whitelisted(&addr));
    }

    #[test]
    fn test_non_admin_cannot_add_address() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let intruder = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        let result = client.try_add_address(&intruder, &addr);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Cooldown integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_cooldown_blocks_consecutive_actions() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &300);

        // First action succeeds
        client.add_address(&admin, &addr1);
        assert!(client.is_whitelisted(&addr1));

        // Second action blocked by cooldown
        let result = client.try_add_address(&admin, &addr2);
        assert_eq!(result.unwrap_err(), Ok(WhitelistError::AdminCooldownActive));

        // Advance past cooldown window
        env.ledger().set_timestamp(1_000_300);
        assert!(client.is_admin_action_ready());

        // Second action now succeeds
        client.add_address(&admin, &addr2);
        assert!(client.is_whitelisted(&addr2));
    }

    #[test]
    fn test_remove_address_is_cooldown_gated() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &300);

        // Add an address first
        client.add_address(&admin, &addr);

        // Advance past cooldown before removing
        env.ledger().set_timestamp(1_000_300);
        client.remove_address(&admin, &addr);
        assert!(!client.is_whitelisted(&addr));

        // remove_address just armed cooldown — another remove on non-existent
        // address should be blocked by cooldown, not by "not found"
        let result = client.try_remove_address(&admin, &addr);
        assert_eq!(result.unwrap_err(), Ok(WhitelistError::AdminCooldownActive));
    }

    #[test]
    fn test_clear_all_is_cooldown_gated() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let addr1 = Address::generate(&env);
        let addr2 = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &300);

        client.add_address(&admin, &addr1);
        env.ledger().set_timestamp(1_000_300);
        client.add_address(&admin, &addr2);

        // Clear blocked by cooldown (just after second add)
        let result = client.try_clear_all(&admin);
        assert_eq!(result.unwrap_err(), Ok(WhitelistError::AdminCooldownActive));

        env.ledger().set_timestamp(1_000_600);
        assert!(client.is_admin_action_ready());

        client.clear_all(&admin);
        assert!(client.get_whitelist().is_empty());
    }

    #[test]
    fn test_cooldown_configuration() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);

        // Default cooldown
        assert_eq!(client.get_admin_cooldown(), admin::DEFAULT_COOLDOWN_SECONDS);

        // Set to minimum
        client.set_admin_cooldown(&admin, &admin::MIN_COOLDOWN_SECONDS);
        assert_eq!(client.get_admin_cooldown(), admin::MIN_COOLDOWN_SECONDS);

        // Set to maximum
        client.set_admin_cooldown(&admin, &admin::MAX_COOLDOWN_SECONDS);
        assert_eq!(client.get_admin_cooldown(), admin::MAX_COOLDOWN_SECONDS);

        // Out of bounds
        let result = client.try_set_admin_cooldown(&admin, &0);
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::InvalidAdminCooldown)
        );

        let result = client.try_set_admin_cooldown(&admin, &(admin::MAX_COOLDOWN_SECONDS + 1));
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::InvalidAdminCooldown)
        );
    }

    #[test]
    fn test_cooldown_remaining() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &300);

        // No action yet, cooldown should be 0
        assert_eq!(client.admin_cooldown_remaining(), 0);

        // Execute action
        client.add_address(&admin, &addr);
        assert_eq!(client.admin_cooldown_remaining(), 300);

        // Advance partway
        env.ledger().set_timestamp(1_000_100);
        assert_eq!(client.admin_cooldown_remaining(), 200);

        // Advance past window
        env.ledger().set_timestamp(1_000_300);
        assert_eq!(client.admin_cooldown_remaining(), 0);
    }

    #[test]
    fn test_get_last_critical_admin_action() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &300);

        // No action yet
        assert!(client.get_last_critical_admin_action().is_none());

        client.add_address(&admin, &addr);
        let record = client.get_last_critical_admin_action().unwrap();
        assert_eq!(record.action, Symbol::new(&env, "add_address"));
        assert_eq!(record.executed_at, 1_000_000);

        env.ledger().set_timestamp(1_000_300);
        client.remove_address(&admin, &addr);
        let record = client.get_last_critical_admin_action().unwrap();
        assert_eq!(record.action, Symbol::new(&env, "remove_address"));
    }

    #[test]
    fn test_is_admin_action_ready() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);

        let admin = Address::generate(&env);
        let addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &300);

        // No action yet — ready
        assert!(client.is_admin_action_ready());

        client.add_address(&admin, &addr);
        assert!(!client.is_admin_action_ready());

        env.ledger().set_timestamp(1_000_300);
        assert!(client.is_admin_action_ready());
    }

    #[test]
    fn test_error_code_stability() {
        assert_eq!(WhitelistError::NotInitialized as u32, 1);
        assert_eq!(WhitelistError::AlreadyInitialized as u32, 2);
        assert_eq!(WhitelistError::Unauthorized as u32, 3);
        assert_eq!(WhitelistError::AddressAlreadyInWhitelist as u32, 4);
        assert_eq!(WhitelistError::AddressNotInWhitelist as u32, 5);

        assert_eq!(WhitelistError::AdminCooldownActive as u32, 49);
        assert_eq!(WhitelistError::InvalidAdminCooldown as u32, 50);
        assert_eq!(WhitelistError::NoAdminTransferPending as u32, 51);
        assert_eq!(WhitelistError::NewAdminSameAsCurrent as u32, 52);
        assert_eq!(WhitelistError::AdminTransferCancelled as u32, 53);
        assert_eq!(WhitelistError::MigrationPending as u32, 54);
    }

    // -----------------------------------------------------------------------
    // Keyed persistent storage tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_swap_remove_keeps_remaining_addresses() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let c = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        client.add_address(&admin, &a);
        env.ledger().set_timestamp(1_000_001);
        client.add_address(&admin, &b);
        env.ledger().set_timestamp(1_000_002);
        client.add_address(&admin, &c);
        assert_eq!(client.whitelist_count(), 3);

        // Remove the first entry: the last entry (c) is swapped into its slot.
        env.ledger().set_timestamp(1_000_003);
        client.remove_address(&admin, &a);

        assert!(!client.is_whitelisted(&a));
        assert!(client.is_whitelisted(&b));
        assert!(client.is_whitelisted(&c));
        assert_eq!(client.whitelist_count(), 2);

        let list = client.get_whitelist();
        assert_eq!(list.len(), 2);
        assert!(list.contains(&b));
        assert!(list.contains(&c));
    }

    #[test]
    fn test_clear_all_bumps_epoch_and_allows_readd() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let a = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        client.add_address(&admin, &a);
        env.ledger().set_timestamp(1_000_001);
        client.clear_all(&admin);

        assert!(!client.is_whitelisted(&a));
        assert_eq!(client.whitelist_count(), 0);

        // The same address can be added again in the new epoch.
        env.ledger().set_timestamp(1_000_002);
        client.add_address(&admin, &a);
        assert!(client.is_whitelisted(&a));
        assert_eq!(client.whitelist_count(), 1);
    }

    #[test]
    fn test_get_whitelist_page() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        for i in 0..3u64 {
            env.ledger().set_timestamp(1_000_000 + i);
            client.add_address(&admin, &Address::generate(&env));
        }

        assert_eq!(client.get_whitelist_page(&0, &2).len(), 2);
        assert_eq!(client.get_whitelist_page(&2, &10).len(), 1);
        assert_eq!(client.get_whitelist_page(&5, &10).len(), 0);
    }

    // -----------------------------------------------------------------------
    // Legacy migration tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_reads_fall_back_to_legacy_list_before_migration() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let a = Address::generate(&env);
        let outsider = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        seed_legacy(&env, &client, &[a.clone()]);

        assert!(!client.is_migration_complete());
        assert!(client.is_whitelisted(&a));
        assert!(!client.is_whitelisted(&outsider));
        assert_eq!(client.whitelist_count(), 1);
    }

    #[test]
    fn test_migrate_legacy_in_batches_is_idempotent() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let a = Address::generate(&env);
        let b = Address::generate(&env);
        let c = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        seed_legacy(&env, &client, &[a.clone(), b.clone(), c.clone()]);

        // First batch of 2 leaves one entry.
        assert!(!client.migrate_legacy(&admin, &2));
        assert!(!client.is_migration_complete());

        // Second batch finishes the migration.
        assert!(client.migrate_legacy(&admin, &2));
        assert!(client.is_migration_complete());
        assert_eq!(client.whitelist_count(), 3);
        assert!(client.is_whitelisted(&a));
        assert!(client.is_whitelisted(&b));
        assert!(client.is_whitelisted(&c));

        // Calling again changes nothing.
        assert!(client.migrate_legacy(&admin, &2));
        assert_eq!(client.whitelist_count(), 3);
    }

    #[test]
    fn test_mutations_blocked_while_migration_pending() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let legacy_addr = Address::generate(&env);
        let new_addr = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);
        seed_legacy(&env, &client, &[legacy_addr.clone()]);

        let add = client.try_add_address(&admin, &new_addr);
        assert_eq!(add.unwrap_err(), Ok(WhitelistError::MigrationPending));

        let remove = client.try_remove_address(&admin, &legacy_addr);
        assert_eq!(remove.unwrap_err(), Ok(WhitelistError::MigrationPending));

        let clear = client.try_clear_all(&admin);
        assert_eq!(clear.unwrap_err(), Ok(WhitelistError::MigrationPending));

        // A rejected mutation must not arm the cool-off window.
        assert!(client.is_admin_action_ready());
    }

    #[test]
    fn test_non_admin_cannot_migrate_legacy() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let intruder = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        seed_legacy(&env, &client, &[Address::generate(&env)]);

        let result = client.try_migrate_legacy(&intruder, &10);
        assert!(result.is_err());
        assert!(!client.is_migration_complete());
    }
}
