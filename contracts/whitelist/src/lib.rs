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

mod errors;
pub use errors::WhitelistError;

pub mod admin;
pub mod events;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol, Vec};

/// Instance / persistent storage keys for the whitelist contract.
///
/// Per-address membership (`Member`, `MemberGeneration`) lives in **persistent**
/// storage so `is_whitelisted` is O(1) and does not load instance storage on
/// every call. The legacy `WhitelistList` Vec remains in instance storage only
/// during migration; it is drained by `migrate_list` and then removed.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Contract owner address (instance).
    WhitelistOwner,
    /// Current admin address (defaults to owner at init, instance).
    WhitelistAdmin,
    /// Pending admin address awaiting acceptance (two-step transfer, instance).
    WhitelistPendingAdmin,
    /// Per-address membership flag `bool` (persistent).
    Member(Address),
    /// Per-address clear-generation marker `u64` (persistent), used by
    /// `clear_page` to invalidate all members without scanning.
    MemberGeneration(Address),
    /// Total number of whitelisted addresses `u32` (instance).
    MemberCount,
    /// Clear-all generation counter `u64` (instance). Incremented by
    /// `clear_page` when the whitelist becomes empty; compared against
    /// `MemberGeneration` in `is_whitelisted`.
    ClearGeneration,
    /// Ordered enumeration index `Vec<Address>` (instance). Used only by
    /// `get_whitelist_page`; not consulted by `is_whitelisted`.
    WhitelistIndex,
    /// Legacy whitelist Vec `Vec<Address>` (instance). Present only before
    /// migration completes; removed by `migrate_list`.
    WhitelistList,
    /// Migration completion marker `bool` (instance). Set to `true` once the
    /// legacy `WhitelistList` has been fully drained.
    MigrationComplete,
    /// Global cool-off window between critical whitelist admin executions (instance).
    WhitelistAdminCooldown,
    /// Audit record for the most recently executed critical whitelist admin action (instance).
    WhitelistLastCriticalAction,
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

/// Maximum number of addresses returned in a single `get_whitelist_page` call.
pub const MAX_PAGE: u32 = 100;

/// Default TTL (in ledger seqs) for persistent member entries.
pub const MEMBER_TTL: u32 = 50_000;

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
    /// may only be initialized once.
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
            .set(&StorageKey::MemberCount, &0u32);
        env.storage()
            .instance()
            .set(&StorageKey::ClearGeneration, &0u64);

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
    /// - [`WhitelistError::AdminCooldownActive`] if another action's cool-off is still active.
    /// - [`WhitelistError::AddressAlreadyInWhitelist`] if the address is already whitelisted.
    pub fn add_address(env: Env, caller: Address, address: Address) -> Result<(), WhitelistError> {
        // `require_admin` performs `caller.require_auth()`; authorizing twice in
        // the same invocation frame is rejected by the host (`Auth::ExistingValue`).
        Self::require_admin(&env, &caller)?;

        admin::guard(&env, Symbol::new(&env, "add_address"))?;

        // Check persistent membership first (O(1)).
        let member: bool = env
            .storage()
            .persistent()
            .get(&StorageKey::Member(address.clone()))
            .unwrap_or(false);
        if member {
            return Err(WhitelistError::AddressAlreadyInWhitelist);
        }

        // Write membership to persistent storage (O(1)).
        env.storage()
            .persistent()
            .set(&StorageKey::Member(address.clone()), &true);
        env.storage().persistent().extend_ttl(
            &StorageKey::Member(address.clone()),
            MEMBER_TTL,
            MEMBER_TTL,
        );

        // Record the clear generation at time of insertion.
        let gen: u64 = env
            .storage()
            .instance()
            .get(&StorageKey::ClearGeneration)
            .unwrap_or(0);
        env.storage()
            .persistent()
            .set(&StorageKey::MemberGeneration(address.clone()), &gen);
        env.storage().persistent().extend_ttl(
            &StorageKey::MemberGeneration(address.clone()),
            MEMBER_TTL,
            MEMBER_TTL,
        );

        // Update instance counter.
        let inst = env.storage().instance();
        let count: u32 = inst.get(&StorageKey::MemberCount).unwrap_or(0);
        let new_count = count
            .checked_add(1)
            .ok_or(WhitelistError::MemberCountOverflow)?;
        inst.set(&StorageKey::MemberCount, &new_count);

        // Maintain enumeration index.
        let mut index: Vec<Address> = inst
            .get(&StorageKey::WhitelistIndex)
            .unwrap_or_else(|| Vec::new(&env));
        if !index.iter().any(|a| a == address) {
            index.push_back(address.clone());
            inst.set(&StorageKey::WhitelistIndex, &index);
        }

        // Also append to legacy WhitelistList for migration compatibility.
        let mut legacy: Vec<Address> = inst
            .get(&StorageKey::WhitelistList)
            .unwrap_or_else(|| Vec::new(&env));
        if !legacy.iter().any(|a| a == address) {
            legacy.push_back(address.clone());
            inst.set(&StorageKey::WhitelistList, &legacy);
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
    /// # Parameters
    /// - `caller` — Must be the current admin.
    /// - `address` — Address to remove from the whitelist.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    /// - [`WhitelistError::AdminCooldownActive`] if another action's cool-off is still active.
    /// - [`WhitelistError::AddressNotInWhitelist`] if the address is not whitelisted.
    pub fn remove_address(
        env: Env,
        caller: Address,
        address: Address,
    ) -> Result<(), WhitelistError> {
        Self::require_admin(&env, &caller)?;

        admin::guard(&env, Symbol::new(&env, "remove_address"))?;

        // Check persistent membership.
        let member: bool = env
            .storage()
            .persistent()
            .get(&StorageKey::Member(address.clone()))
            .unwrap_or(false);
        if !member {
            return Err(WhitelistError::AddressNotInWhitelist);
        }

        // Clear membership from persistent storage.
        env.storage()
            .persistent()
            .remove(&StorageKey::Member(address.clone()));
        env.storage()
            .persistent()
            .remove(&StorageKey::MemberGeneration(address.clone()));

        // Update instance counter.
        let inst = env.storage().instance();
        let count: u32 = inst.get(&StorageKey::MemberCount).unwrap_or(0);
        let new_count = count.saturating_sub(1);
        inst.set(&StorageKey::MemberCount, &new_count);

        // Remove from enumeration index (manual filter).
        let old_index: Vec<Address> = inst
            .get(&StorageKey::WhitelistIndex)
            .unwrap_or_else(|| Vec::new(&env));
        let mut new_index: Vec<Address> = Vec::new(&env);
        for a in old_index.iter() {
            if a != &address {
                new_index.push_back(a.clone());
            }
        }
        inst.set(&StorageKey::WhitelistIndex, &new_index);

        // Remove from legacy WhitelistList.
        let mut legacy: Vec<Address> = inst
            .get(&StorageKey::WhitelistList)
            .unwrap_or_else(|| Vec::new(&env));
        let mut new_legacy: Vec<Address> = Vec::new(&env);
        for a in legacy.iter() {
            if a != &address {
                new_legacy.push_back(a.clone());
            }
        }
        if new_legacy.is_empty() {
            inst.remove(&StorageKey::WhitelistList);
        } else {
            inst.set(&StorageKey::WhitelistList, &new_legacy);
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
    /// This operation is idempotent — calling it on an empty whitelist succeeds
    /// but still arms the cool-off window. Uses `clear_page` internally with a
    /// large limit to fully drain the whitelist in one call.
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    /// - [`WhitelistError::AdminCooldownActive`] if another action's cool-off is still active.
    pub fn clear_all(env: Env, caller: Address) -> Result<(), WhitelistError> {
        Self::require_admin(&env, &caller)?;

        admin::guard(&env, Symbol::new(&env, "clear_all"))?;

        // Drain all pages until empty.
        loop {
            let cleared = Self::clear_page_inner(&env, &caller, u32::MAX);
            if cleared == 0 {
                break;
            }
        }

        Self::bump_instance_ttl(&env);
        env.events().publish(
            (
                events::event_whitelist_cleared(&env),
                events::event_version_v1(&env),
                caller,
            ),
            0u32,
        );
        Ok(())
    }

    /// Clear whitelisted entries with bounded cost (admin only, cooldown-gated).
    ///
    /// Removes up to `limit` members per call. Repeated calls until the
    /// whitelist is empty fully clear the whitelist. Each call is bounded
    /// by `limit` persistent removals, avoiding instance-storage size limits.
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    /// - `limit` — Maximum number of members to clear this call.
    ///
    /// # Returns
    /// Number of addresses cleared this call.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    /// - [`WhitelistError::AdminCooldownActive`] if another action's cool-off is still active.
    pub fn clear_page(env: Env, caller: Address, limit: u32) -> Result<u32, WhitelistError> {
        Self::require_admin(&env, &caller)?;

        admin::guard(&env, Symbol::new(&env, "clear_page"))?;

        let cleared = Self::clear_page_inner(&env, &caller, limit);
        Self::bump_instance_ttl(&env);
        Ok(cleared)
    }

    /// Internal bounded clear: removes up to `limit` members from persistent
    /// and instance storage. Does not perform auth or cooldown checks.
    fn clear_page_inner(env: &Env, _caller: &Address, limit: u32) -> u32 {
        if limit == 0 {
            return 0;
        }

        let inst = env.storage().instance();
        let index: Vec<Address> = inst
            .get(&StorageKey::WhitelistIndex)
            .unwrap_or_else(|| Vec::new(env));

        let to_clear = index.len().min(limit);
        let mut cleared: u32 = 0;
        let mut remaining: Vec<Address> = Vec::new(env);

        for (i, addr) in index.iter().enumerate() {
            let idx = i as u32;
            if idx < to_clear {
                env.storage()
                    .persistent()
                    .remove(&StorageKey::Member(addr.clone()));
                env.storage()
                    .persistent()
                    .remove(&StorageKey::MemberGeneration(addr.clone()));
                cleared += 1;
            } else {
                remaining.push_back(addr.clone());
            }
        }

        inst.set(&StorageKey::WhitelistIndex, &remaining);
        // Keep legacy WhitelistList in sync with WhitelistIndex so the
        // deprecated get_whitelist returns consistent data during migration.
        inst.set(&StorageKey::WhitelistList, &remaining);
        inst.set(&StorageKey::MemberCount, &remaining.len() as u32);

        if remaining.is_empty() {
            let gen: u64 = inst.get(&StorageKey::ClearGeneration).unwrap_or(0);
            inst.set(&StorageKey::ClearGeneration, &(gen + 1));
        }

        cleared
    }

    /// Migrate the legacy `WhitelistList` Vec into per-address persistent
    /// entries in chunks (admin only, cooldown-gated).
    ///
    /// Each call drains up to `limit` addresses from the legacy instance
    /// storage Vec and writes them as persistent `Member` entries. When the
    /// legacy Vec is empty, it is removed and migration is marked complete.
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    /// - `limit` — Maximum number of legacy entries to migrate this call.
    ///
    /// # Returns
    /// Number of addresses migrated this call.
    ///
    /// # Errors
    /// - [`WhitelistError::Unauthorized`] if `caller` is not the admin.
    /// - [`WhitelistError::NotInitialized`] if the contract has not been initialized.
    /// - [`WhitelistError::AdminCooldownActive`] if another action's cool-off is still active.
    pub fn migrate_list(env: Env, caller: Address, limit: u32) -> Result<u32, WhitelistError> {
        Self::require_admin(&env, &caller)?;

        admin::guard(&env, Symbol::new(&env, "migrate_list"))?;

        if !env.storage().instance().has(&StorageKey::WhitelistList) {
            return Ok(0);
        }

        let inst = env.storage().instance();
        let legacy: Vec<Address> = inst
            .get(&StorageKey::WhitelistList)
            .unwrap_or_else(|| Vec::new(&env));

        let mut remaining: Vec<Address> = Vec::new(&env);
        let mut count_migrated: u32 = 0;
        let mut migrated: u32 = 0;

        for addr in legacy.iter() {
            if migrated < limit {
                let member: bool = env
                    .storage()
                    .persistent()
                    .get(&StorageKey::Member(addr.clone()))
                    .unwrap_or(false);
                if !member {
                    env.storage()
                        .persistent()
                        .set(&StorageKey::Member(addr.clone()), &true);
                    env.storage().persistent().extend_ttl(
                        &StorageKey::Member(addr.clone()),
                        MEMBER_TTL,
                        MEMBER_TTL,
                    );
                    let gen: u64 = inst.get(&StorageKey::ClearGeneration).unwrap_or(0u64);
                    env.storage()
                        .persistent()
                        .set(&StorageKey::MemberGeneration(addr.clone()), &gen);
                    env.storage().persistent().extend_ttl(
                        &StorageKey::MemberGeneration(addr.clone()),
                        MEMBER_TTL,
                        MEMBER_TTL,
                    );
                    count_migrated += 1;
                }
                migrated += 1;
            } else {
                remaining.push_back(addr.clone());
            }
        }

        if remaining.is_empty() {
            inst.remove(&StorageKey::WhitelistList);
            inst.set(&StorageKey::MigrationComplete, &true);
            // Also clear WhitelistIndex when migration fully drains legacy Vec.
            inst.set(&StorageKey::WhitelistIndex, &Vec::new(env));
            inst.set(&StorageKey::MemberCount, &0u32);
        } else {
            inst.set(&StorageKey::WhitelistList, &remaining);
        }

        // Keep WhitelistIndex in sync with persisted members after migration.
        let mut index: Vec<Address> = Vec::new(env);
        for addr in remaining.iter() {
            index.push_back(addr.clone());
        }
        inst.set(&StorageKey::WhitelistIndex, &index);

        let count: u32 = inst.get(&StorageKey::MemberCount).unwrap_or(0);
        inst.set(
            &StorageKey::MemberCount,
            &count.saturating_add(count_migrated),
        );

        Self::bump_instance_ttl(&env);
        Ok(count_migrated)
    }

    /// Check whether an address is in the whitelist.
    ///
    /// No authentication required — this is a public read-only view function.
    ///
    /// Costs at most two persistent storage reads (`Member` + `MemberGeneration`)
    /// plus one instance read (`ClearGeneration`). During an unfinished migration,
    /// also consults the legacy `WhitelistList` Vec.
    ///
    /// # Returns
    /// `true` if the address is present in the whitelist, `false` otherwise.
    pub fn is_whitelisted(env: Env, address: Address) -> bool {
        // Fast path: check persistent member entry.
        let member: bool = env
            .storage()
            .persistent()
            .get(&StorageKey::Member(address.clone()))
            .unwrap_or(false);

        if !member {
            // During migration, fall back to legacy Vec.
            if env.storage().instance().has(&StorageKey::WhitelistList) {
                let list: Vec<Address> = env
                    .storage()
                    .instance()
                    .get(&StorageKey::WhitelistList)
                    .unwrap_or_else(|| Vec::new(&env));
                if list.iter().any(|a| a == address) {
                    return true;
                }
            }
            return false;
        }

        // Verify the member is still valid under the current clear generation.
        let gen: u64 = env
            .storage()
            .instance()
            .get(&StorageKey::ClearGeneration)
            .unwrap_or(0);
        let member_gen: u64 = env
            .storage()
            .persistent()
            .get(&StorageKey::MemberGeneration(address.clone()))
            .unwrap_or(0);

        if member_gen != gen {
            return false;
        }

        // Extend TTL on read.
        env.storage().persistent().extend_ttl(
            &StorageKey::Member(address.clone()),
            MEMBER_TTL,
            MEMBER_TTL,
        );
        env.storage().persistent().extend_ttl(
            &StorageKey::MemberGeneration(address.clone()),
            MEMBER_TTL,
            MEMBER_TTL,
        );

        true
    }

    /// Return the current whitelist (deprecated).
    ///
    /// Reads from the legacy `WhitelistList` instance-storage key for
    /// backward compatibility during migration. Capped at [`MAX_PAGE`].
    ///
    /// # Deprecated
    /// Use [`get_whitelist_page`] instead. This method loads the full
    /// enumeration index and is subject to instance-storage size limits.
    ///
    /// # Returns
    /// `Vec<Address>` containing up to `MAX_PAGE` whitelisted addresses.
    #[deprecated(note = "Use get_whitelist_page instead")]
    pub fn get_whitelist(env: Env) -> Vec<Address> {
        Self::bump_instance_ttl(&env);
        let list: Vec<Address> = env
            .storage()
            .instance()
            .get(&StorageKey::WhitelistList)
            .unwrap_or_else(|| Vec::new(&env));
        if list.len() > MAX_PAGE as usize {
            let mut truncated: Vec<Address> = Vec::new(&env);
            for (i, addr) in list.iter().enumerate() {
                if i >= MAX_PAGE as usize {
                    break;
                }
                truncated.push_back(addr.clone());
            }
            truncated
        } else {
            list
        }
    }

    /// Get a paginated page of whitelisted addresses (admin only).
    ///
    /// Returns up to `limit` addresses starting at index `start` in the
    /// enumeration index. `limit` is capped at [`MAX_PAGE`].
    ///
    /// # Parameters
    /// - `caller` — Must be the current admin.
    /// - `start` — Zero-based offset into the whitelist index.
    /// - `limit` — Maximum number of addresses to return (capped at MAX_PAGE).
    ///
    /// # Returns
    /// `Vec<Address>` containing up to `limit` whitelisted addresses.
    pub fn get_whitelist_page(
        env: Env,
        caller: Address,
        start: u32,
        limit: u32,
    ) -> Result<Vec<Address>, WhitelistError> {
        Self::require_admin(&env, &caller)?;

        let index: Vec<Address> = env
            .storage()
            .instance()
            .get(&StorageKey::WhitelistIndex)
            .unwrap_or_else(|| Vec::new(&env));

        if limit == 0 || start >= index.len() {
            return Ok(Vec::new(&env));
        }

        let effective_limit = limit.min(MAX_PAGE);
        let end = start.saturating_add(effective_limit).min(index.len());

        let mut result = Vec::new(&env);
        for (i, addr) in index.iter().enumerate() {
            let idx = i as u32;
            if idx >= end {
                break;
            }
            if idx >= start {
                result.push_back(addr.clone());
            }
        }
        Ok(result)
    }

    /// Return the number of whitelisted addresses.
    ///
    /// No authentication required — this is a public read-only view function.
    pub fn get_member_count(env: Env) -> u32 {
        env.storage()
            .instance()
            .get(&StorageKey::MemberCount)
            .unwrap_or(0)
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

        client.add_address(&admin, &addr1).unwrap();
        env.ledger().set_timestamp(1_000_001);
        client.add_address(&admin, &addr2).unwrap();

        assert_eq!(client.get_member_count(), 2);

        env.ledger().set_timestamp(1_000_002);
        client.clear_all(&admin).unwrap();
        assert_eq!(client.get_member_count(), 0);
    }

    #[test]
    fn test_is_whitelisted_returns_false_for_uninit() {
        let env = Env::default();
        let addr = Address::generate(&env);

        let contract_id = env.register(CalloraWhitelist, ());
        let client = CalloraWhitelistClient::new(&env, &contract_id);

        // Before init, whitelist should be empty
        assert_eq!(client.get_member_count(), 0);
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
        assert_eq!(WhitelistError::MemberCountOverflow as u32, 6);

        assert_eq!(WhitelistError::AdminCooldownActive as u32, 49);
        assert_eq!(WhitelistError::InvalidAdminCooldown as u32, 50);
        assert_eq!(WhitelistError::NoAdminTransferPending as u32, 51);
        assert_eq!(WhitelistError::NewAdminSameAsCurrent as u32, 52);
    }

    // -----------------------------------------------------------------------
    // Issue #1421: Per-address persistent membership tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_add_then_is_whitelisted_count_plus_one() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr1 = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        client.add_address(&admin, &addr1).unwrap();
        assert!(client.is_whitelisted(addr1.clone()));
        assert_eq!(client.get_member_count(), 1);
    }

    #[test]
    fn test_add_duplicate_returns_already_in_whitelist() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr1 = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        client.add_address(&admin, &addr1.clone()).unwrap();
        env.ledger().set_timestamp(1_000_001);
        let result = client.try_add_address(&admin, &addr1);
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::AddressAlreadyInWhitelist)
        );
        assert_eq!(client.get_member_count(), 1);
    }

    #[test]
    fn test_remove_non_member_returns_not_in_whitelist() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr1 = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        let result = client.try_remove_address(&admin, &addr1);
        assert_eq!(
            result.unwrap_err(),
            Ok(WhitelistError::AddressNotInWhitelist)
        );
    }

    #[test]
    fn test_is_whitelisted_flat_with_1000_members() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        // Add 1000 members.
        for _ in 0..1000u32 {
            let a = Address::generate(&env);
            client.add_address(&admin, &a).unwrap();
            env.ledger().set_timestamp(env.ledger().timestamp() + 1);
        }

        assert_eq!(client.get_member_count(), 1000);
    }

    #[test]
    fn test_get_whitelist_page_walks_all_members() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        // Add 50 members.
        let mut expected: Vec<Address> = Vec::new(&env);
        for _ in 0..50u32 {
            let a = Address::generate(&env);
            client.add_address(&admin, &a.clone()).unwrap();
            env.ledger().set_timestamp(env.ledger().timestamp() + 1);
            expected.push_back(a);
        }

        // Walk all pages.
        let mut found: Vec<Address> = Vec::new(&env);
        let mut start: u32 = 0;
        let page_size: u32 = 10;

        loop {
            let page = client.get_whitelist_page(&admin, start, page_size).unwrap();
            if page.is_empty() {
                break;
            }
            for a in page.iter() {
                found.push_back(a.clone());
            }
            start += page_size;
        }

        // Every member appears exactly once.
        assert_eq!(found.len(), expected.len());
        for e in expected.iter() {
            assert!(found.iter().any(|f| f == e));
        }
    }

    #[test]
    fn test_migrate_list_300_in_chunks_of_100() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        // Populate legacy WhitelistList with 300 entries directly in instance storage.
        let mut legacy: Vec<Address> = Vec::new(&env);
        for _ in 0..300u32 {
            let a = Address::generate(&env);
            legacy.push_back(a);
        }
        env.storage()
            .instance()
            .set(&StorageKey::WhitelistList, &legacy);

        // Migrate in chunks of 100.
        let m1 = client.migrate_list(&admin, 100).unwrap();
        let m2 = client.migrate_list(&admin, 100).unwrap();
        let m3 = client.migrate_list(&admin, 100).unwrap();

        assert_eq!(m1 + m2 + m3, 300);
        assert_eq!(client.get_member_count(), 300);

        // Legacy key removed.
        assert!(!env.storage().instance().has(&StorageKey::WhitelistList));

        // All 300 members are in persistent storage.
        let mut found = 0u32;
        for a in legacy.iter() {
            let m: bool = env
                .storage()
                .persistent()
                .get(&StorageKey::Member(a.clone()))
                .unwrap_or(false);
            if m {
                found += 1;
            }
        }
        assert_eq!(found, 300);
    }

    #[test]
    fn test_clear_page_makes_all_not_whitelisted() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        let mut members: Vec<Address> = Vec::new(&env);
        for _ in 0..20u32 {
            let a = Address::generate(&env);
            client.add_address(&admin, &a.clone()).unwrap();
            env.ledger().set_timestamp(env.ledger().timestamp() + 1);
            members.push_back(a);
        }
        assert_eq!(client.get_member_count(), 20);

        // Clear in pages of 5 until empty.
        let mut total_cleared: u32 = 0;
        loop {
            let cleared = client.clear_page(&admin, 5).unwrap();
            total_cleared += cleared;
            if cleared == 0 {
                break;
            }
        }

        assert_eq!(total_cleared, 20);
        assert_eq!(client.get_member_count(), 0);

        for m in members.iter() {
            assert!(!client.is_whitelisted(m.clone()));
        }
    }

    #[test]
    fn test_set_admin_emits_one_nominated_event() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        let events_before = env.events().all();
        let count_before = events_before.len();

        client.set_admin(&admin, &new_admin.clone()).unwrap();

        let events_after = env.events().all();
        let nominated_count = events_after
            .iter()
            .skip(count_before)
            .filter(|e| {
                let (topics, _data) = e;
                topics.iter().any(|t| {
                    if let soroban_sdk::Val::Symbol(s) = t {
                        s == Symbol::new(&env, "admin_nominated")
                    } else {
                        false
                    }
                })
            })
            .count();

        assert_eq!(nominated_count, 1);
    }

    #[test]
    fn test_member_ttl_extended_on_add_and_read() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_000_000);
        let admin = Address::generate(&env);
        let addr1 = Address::generate(&env);

        let client = deploy_whitelist(&env, &admin);
        client.set_admin_cooldown(&admin, &1);

        client.add_address(&admin, &addr1.clone()).unwrap();

        // Check TTL after add.
        let ttl_after_add = env.storage().get_ttl(&StorageKey::Member(addr1.clone()));
        assert!(ttl_after_add > 0);

        // Read triggers TTL extension.
        client.is_whitelisted(addr1.clone());

        // Check TTL after read.
        let ttl_after_read = env.storage().get_ttl(&StorageKey::Member(addr1.clone()));
        assert!(ttl_after_read >= MEMBER_TTL);
    }
}
