#![no_std]

//! # Callora Allowlist
//!
//! Admin-managed shared allowlist. Each address is stored under its own
//! persistent key and a dense, position-keyed index provides stable
//! pagination without growing instance storage with the list.
//!
//! ## Storage model
//!
//! | Key                     | Storage    | Value                                  |
//! |-------------------------|------------|----------------------------------------|
//! | `Admin`                 | instance   | current admin                          |
//! | `PendingAdmin`          | instance   | nominated admin awaiting acceptance    |
//! | `Count`                 | instance   | number of allowlisted addresses        |
//! | `Entry(Address)`        | persistent | `true` for every allowlisted address   |
//! | `Index(u32)`            | persistent | address at a dense index position      |
//! | `Position(Address)`     | persistent | reverse lookup index -> position       |
//!
//! ## Admin rotation
//!
//! `init` sets the admin. `set_admin` nominates a successor and
//! `accept_admin` completes the transfer; `cancel_admin_transfer` aborts a
//! pending nomination. Only the current admin may nominate or cancel, and only
//! the nominee may accept.
//!
//! ## Pagination
//!
//! `get_allowlist_page(cursor, limit)` walks the dense index from `cursor`.
//! `limit` must be between `1` and [`MAX_PAGE_SIZE`]; a cursor at or past the
//! count returns an empty page. Removal uses a swap-remove, so the order of
//! entries can change when an entry is removed, but a page walk over a fixed
//! state never skips or duplicates an address.

pub mod errors;
pub mod events;

pub use errors::AllowlistError;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Vec};

/// Maximum number of entries returned by [`CalloraAllowlist::get_allowlist_page`].
pub const MAX_PAGE_SIZE: u32 = 50;

/// Approximate number of ledgers per day on Stellar (5 seconds per ledger).
pub const LEDGERS_PER_DAY: u32 = 17_280;

/// Threshold at which instance storage TTL is extended (30 days).
pub const INSTANCE_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;

/// Target TTL when instance storage is extended (60 days).
pub const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 60;

/// Threshold at which persistent storage TTL is extended (30 days).
pub const PERSISTENT_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;

/// Target TTL when persistent storage is extended (120 days).
pub const PERSISTENT_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 120;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// Instance and persistent storage keys for the allowlist.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    /// Instance: the current admin.
    Admin,
    /// Instance: the nominated admin awaiting acceptance.
    PendingAdmin,
    /// Instance: number of allowlisted addresses.
    Count,
    /// Persistent: presence marker for an allowlisted address.
    Entry(Address),
    /// Persistent: address stored at a dense index position.
    Index(u32),
    /// Persistent: reverse lookup from an address to its index position.
    Position(Address),
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

/// Admin-managed shared allowlist contract.
#[contract]
pub struct CalloraAllowlist;

#[contractimpl]
impl CalloraAllowlist {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// Initialize the allowlist with an admin (one-time setup).
    ///
    /// # Errors
    /// - [`AllowlistError::AlreadyInitialized`] if `init` was already called.
    ///
    /// # Events
    /// Emits `"init"` with the admin as topic and data.
    pub fn init(env: Env, admin: Address) -> Result<(), AllowlistError> {
        admin.require_auth();
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(AllowlistError::AlreadyInitialized);
        }

        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::Count, &0u32);
        Self::bump_instance(&env);

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

    /// Return the current admin.
    ///
    /// # Errors
    /// - [`AllowlistError::NotInitialized`] if `init` has not been called.
    pub fn get_admin(env: Env) -> Result<Address, AllowlistError> {
        Self::bump_instance(&env);
        Self::admin(&env)
    }

    /// Return the pending admin nomination, if any.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        Self::bump_instance(&env);
        env.storage()
            .instance()
            .get::<_, Address>(&StorageKey::PendingAdmin)
    }

    /// Nominate a successor admin (current admin only).
    ///
    /// The nominee must call [`Self::accept_admin`] to complete the transfer.
    ///
    /// # Errors
    /// - [`AllowlistError::Unauthorized`] if `caller` is not the admin.
    /// - [`AllowlistError::NotInitialized`] if `init` has not been called.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), AllowlistError> {
        Self::require_admin(&env, &caller)?;

        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);
        Self::bump_instance(&env);
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

    /// Accept a pending admin nomination (nominee only).
    ///
    /// # Errors
    /// - [`AllowlistError::NoPendingAdmin`] if no nomination is pending.
    pub fn accept_admin(env: Env) -> Result<(), AllowlistError> {
        let new_admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .ok_or(AllowlistError::NoPendingAdmin)?;
        new_admin.require_auth();

        env.storage().instance().set(&StorageKey::Admin, &new_admin);
        env.storage().instance().remove(&StorageKey::PendingAdmin);
        Self::bump_instance(&env);
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

    /// Cancel a pending admin nomination (current admin only).
    ///
    /// # Errors
    /// - [`AllowlistError::Unauthorized`] if `caller` is not the admin.
    /// - [`AllowlistError::NoPendingAdmin`] if no nomination is pending.
    pub fn cancel_admin_transfer(env: Env, caller: Address) -> Result<(), AllowlistError> {
        Self::require_admin(&env, &caller)?;

        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .ok_or(AllowlistError::NoPendingAdmin)?;

        env.storage().instance().remove(&StorageKey::PendingAdmin);
        Self::bump_instance(&env);
        env.events().publish(
            (
                events::event_admin_cancelled(&env),
                events::event_version_v1(&env),
                caller,
                pending.clone(),
            ),
            pending,
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Allowlist management
    // -----------------------------------------------------------------------

    /// Add an address to the allowlist (admin only).
    ///
    /// # Errors
    /// - [`AllowlistError::Unauthorized`] if `caller` is not the admin.
    /// - [`AllowlistError::AlreadyPresent`] if the address is already allowed.
    /// - [`AllowlistError::Overflow`] if the entry count would overflow.
    pub fn add_address(env: Env, caller: Address, address: Address) -> Result<(), AllowlistError> {
        Self::require_admin(&env, &caller)?;

        let entry_key = StorageKey::Entry(address.clone());
        if env.storage().persistent().has(&entry_key) {
            return Err(AllowlistError::AlreadyPresent);
        }

        let count: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::Count)
            .ok_or(AllowlistError::NotInitialized)?;
        let new_count = count.checked_add(1).ok_or(AllowlistError::Overflow)?;

        env.storage().persistent().set(&entry_key, &true);
        env.storage()
            .persistent()
            .set(&StorageKey::Index(count), &address);
        env.storage()
            .persistent()
            .set(&StorageKey::Position(address.clone()), &count);
        env.storage().instance().set(&StorageKey::Count, &new_count);
        Self::bump_entry(&env, &address, count);
        Self::bump_instance(&env);

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

    /// Remove an address from the allowlist (admin only).
    ///
    /// Uses a swap-remove: the last indexed address fills the vacated slot so
    /// the index stays dense. Enumeration order can therefore change when an
    /// entry is removed.
    ///
    /// # Errors
    /// - [`AllowlistError::Unauthorized`] if `caller` is not the admin.
    /// - [`AllowlistError::NotPresent`] if the address is not allowed.
    pub fn remove_address(
        env: Env,
        caller: Address,
        address: Address,
    ) -> Result<(), AllowlistError> {
        Self::require_admin(&env, &caller)?;

        let entry_key = StorageKey::Entry(address.clone());
        if !env.storage().persistent().has(&entry_key) {
            return Err(AllowlistError::NotPresent);
        }

        let count: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::Count)
            .ok_or(AllowlistError::NotInitialized)?;
        let new_count = count.checked_sub(1).ok_or(AllowlistError::Overflow)?;
        let removed_pos: u32 = env
            .storage()
            .persistent()
            .get(&StorageKey::Position(address.clone()))
            .ok_or(AllowlistError::NotPresent)?;

        if removed_pos < new_count {
            let tail: Address = env
                .storage()
                .persistent()
                .get(&StorageKey::Index(new_count))
                .ok_or(AllowlistError::NotPresent)?;
            env.storage()
                .persistent()
                .set(&StorageKey::Index(removed_pos), &tail);
            env.storage()
                .persistent()
                .set(&StorageKey::Position(tail.clone()), &removed_pos);
            Self::bump_entry(&env, &tail, removed_pos);
        }

        env.storage().persistent().remove(&entry_key);
        env.storage()
            .persistent()
            .remove(&StorageKey::Position(address.clone()));
        env.storage()
            .persistent()
            .remove(&StorageKey::Index(new_count));
        env.storage().instance().set(&StorageKey::Count, &new_count);
        Self::bump_instance(&env);

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

    /// Remove every address from the allowlist (admin only).
    ///
    /// The loop is bounded by the stored [`StorageKey::Count`] and walks the
    /// dense index once (`0 .. Count`), deleting the three persistent keys for
    /// each entry. Calling it on an empty list succeeds and emits an event with
    /// a zero count.
    ///
    /// # Errors
    /// - [`AllowlistError::Unauthorized`] if `caller` is not the admin.
    /// - [`AllowlistError::NotInitialized`] if `init` has not been called.
    pub fn clear_all(env: Env, caller: Address) -> Result<(), AllowlistError> {
        Self::require_admin(&env, &caller)?;

        let count: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::Count)
            .ok_or(AllowlistError::NotInitialized)?;

        for pos in 0..count {
            let address: Address = env
                .storage()
                .persistent()
                .get(&StorageKey::Index(pos))
                .ok_or(AllowlistError::NotPresent)?;
            env.storage()
                .persistent()
                .remove(&StorageKey::Entry(address.clone()));
            env.storage()
                .persistent()
                .remove(&StorageKey::Position(address));
            env.storage().persistent().remove(&StorageKey::Index(pos));
        }

        env.storage().instance().set(&StorageKey::Count, &0u32);
        Self::bump_instance(&env);
        env.events().publish(
            (
                events::event_allowlist_cleared(&env),
                events::event_version_v1(&env),
                caller,
            ),
            count,
        );
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// Return `true` when `address` is on the allowlist.
    pub fn is_allowed(env: Env, address: Address) -> bool {
        Self::bump_instance(&env);
        let key = StorageKey::Entry(address);
        let allowed = env.storage().persistent().has(&key);
        if allowed {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_BUMP_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }
        allowed
    }

    /// Alias for [`Self::is_allowed`] kept for depositor-oriented callers.
    pub fn is_authorized_depositor(env: Env, address: Address) -> bool {
        Self::is_allowed(env, address)
    }

    /// Return the number of allowlisted addresses (`0` before `init`).
    pub fn allowlist_count(env: Env) -> u32 {
        Self::bump_instance(&env);
        env.storage()
            .instance()
            .get::<_, u32>(&StorageKey::Count)
            .unwrap_or(0)
    }

    /// Return a page of allowlisted addresses.
    ///
    /// # Parameters
    /// * `cursor` — zero-based index of the first entry to return.
    /// * `limit` — number of entries to return, between `1` and
    ///   [`MAX_PAGE_SIZE`].
    ///
    /// A `cursor` at or past [`Self::allowlist_count`] returns an empty vector.
    ///
    /// # Errors
    /// - [`AllowlistError::NotInitialized`] if `init` has not been called.
    /// - [`AllowlistError::InvalidPage`] if `limit` is `0` or greater than
    ///   [`MAX_PAGE_SIZE`].
    pub fn get_allowlist_page(
        env: Env,
        cursor: u32,
        limit: u32,
    ) -> Result<Vec<Address>, AllowlistError> {
        Self::bump_instance(&env);
        if !env.storage().instance().has(&StorageKey::Admin) {
            return Err(AllowlistError::NotInitialized);
        }
        if limit == 0 || limit > MAX_PAGE_SIZE {
            return Err(AllowlistError::InvalidPage);
        }

        let count: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::Count)
            .ok_or(AllowlistError::NotInitialized)?;

        let mut page = Vec::new(&env);
        if cursor >= count {
            return Ok(page);
        }

        let end = cursor.saturating_add(limit).min(count);
        for pos in cursor..end {
            let address: Address = env
                .storage()
                .persistent()
                .get(&StorageKey::Index(pos))
                .ok_or(AllowlistError::NotPresent)?;
            Self::bump_entry(&env, &address, pos);
            page.push_back(address);
        }
        Ok(page)
    }

    /// Return every allowlisted address.
    ///
    /// Convenience view for small lists. Prefer [`Self::get_allowlist_page`]
    /// when the list can grow large, because this call reads every entry in a
    /// single invocation.
    pub fn get_allowlist(env: Env) -> Vec<Address> {
        Self::bump_instance(&env);
        let count: u32 = env
            .storage()
            .instance()
            .get::<_, u32>(&StorageKey::Count)
            .unwrap_or(0);
        let mut all = Vec::new(&env);
        for pos in 0..count {
            if let Some(address) = env
                .storage()
                .persistent()
                .get::<_, Address>(&StorageKey::Index(pos))
            {
                Self::bump_entry(&env, &address, pos);
                all.push_back(address);
            }
        }
        all
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Extend the instance storage TTL.
    fn bump_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Extend the persistent TTLs for an entry and its index slots.
    fn bump_entry(env: &Env, address: &Address, pos: u32) {
        for key in [
            StorageKey::Entry(address.clone()),
            StorageKey::Position(address.clone()),
            StorageKey::Index(pos),
        ] {
            env.storage().persistent().extend_ttl(
                &key,
                PERSISTENT_BUMP_THRESHOLD,
                PERSISTENT_BUMP_AMOUNT,
            );
        }
    }

    /// Read the admin address.
    fn admin(env: &Env) -> Result<Address, AllowlistError> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(AllowlistError::NotInitialized)
    }

    /// Authenticate `caller` and require that it is the admin.
    fn require_admin(env: &Env, caller: &Address) -> Result<Address, AllowlistError> {
        caller.require_auth();
        let admin = Self::admin(env)?;
        if caller != &admin {
            return Err(AllowlistError::Unauthorized);
        }
        Ok(admin)
    }
}

// ---------------------------------------------------------------------------
// Test modules
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;
