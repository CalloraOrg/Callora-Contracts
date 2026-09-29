#![no_std]

//! # Callora Recipient Registry
//!
//! An on-chain registry where a privileged admin registers, updates, and
//! removes named payment recipients. Other Callora contracts (vault,
//! settlement, revenue_pool) can cross-reference this registry to validate
//! that a destination address is an approved recipient before executing
//! transfers.
//!
//! # Storage model
//! * **Instance** — admin address, total registered count.
//! * **Persistent** — per-recipient entries keyed by name.
//!
//! # Invariants
//! 1. `require_auth` is enforced on every state-changing entrypoint.
//! 2. Arithmetic is overflow-safe (`checked_add` / `checked_sub`).
//! 3. No `unwrap()` in production code paths.
//!
//! # Fuzzing
//! See `fuzz/targets/main.rs` for the `cargo-fuzz` target that hammers all
//! entrypoints with malformed and randomized inputs.

#[cfg(test)]
extern crate std;

pub mod errors;
pub mod events;

pub use errors::RecipientError;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, String};

/// Maximum byte length of a recipient name.
pub const MAX_NAME_LEN: u32 = 64;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// Instance and persistent storage keys for the Recipient Registry.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    /// Instance: the current admin [`Address`].
    Admin,
    /// Instance: total number of registered recipients.
    RecipientCount,
    /// Persistent: a registered recipient entry, keyed by name.
    Recipient(String),
}

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

/// A registered payment recipient.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct RecipientRecord {
    /// Human-readable name (also used as the storage key).
    pub name: String,
    /// On-ledger address of the recipient.
    pub address: Address,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

/// On-chain recipient registry contract.
///
/// Maintains a mapping of human-readable names to on-ledger addresses, gated
/// behind an admin role with `require_auth` on every mutating operation.
#[contract]
pub struct CalloraRecipient;

#[contractimpl]
impl CalloraRecipient {
    // -----------------------------------------------------------------------
    // Initialization
    // -----------------------------------------------------------------------

    /// Initialize the recipient registry with an admin.
    ///
    /// Can only be called once. The admin is the sole address permitted to
    /// register, update, or remove recipients.
    ///
    /// # Parameters
    /// * `admin` — Address of the registry administrator; must authorize.
    ///
    /// # Errors
    /// * [`RecipientError::AlreadyInitialized`] — `init` was called more than once.
    ///
    /// # Events
    /// Emits `"init"` with the admin address as data.
    pub fn init(env: Env, admin: Address) -> Result<(), RecipientError> {
        admin.require_auth();
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(RecipientError::AlreadyInitialized);
        }
        let inst = env.storage().instance();
        inst.set(&StorageKey::Admin, &admin);
        inst.set(&StorageKey::RecipientCount, &0u32);
        env.events()
            .publish((events::event_init(&env), admin), ());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    /// Read the admin address from instance storage.
    fn admin(env: &Env) -> Result<Address, RecipientError> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(RecipientError::NotInitialized)
    }

    /// Verify that `caller` is the current admin, consuming auth.
    fn require_admin(env: &Env, caller: &Address) -> Result<Address, RecipientError> {
        caller.require_auth();
        let admin = Self::admin(env)?;
        if caller != &admin {
            return Err(RecipientError::Unauthorized);
        }
        Ok(admin)
    }

    /// Validate that a recipient name is non-empty and within length bounds.
    fn validate_name(name: &String) -> Result<(), RecipientError> {
        if name.is_empty() || name.len() > MAX_NAME_LEN {
            return Err(RecipientError::InvalidName);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // State-changing entrypoints
    // -----------------------------------------------------------------------

    /// Register a new named recipient.
    ///
    /// The name must be unique, non-empty, and at most [`MAX_NAME_LEN`] bytes.
    /// The caller must be the admin and must authorize.
    ///
    /// # Parameters
    /// * `caller` — Must be the current admin; must authorize.
    /// * `name` — Unique human-readable identifier for the recipient.
    /// * `address` — On-ledger address of the recipient.
    ///
    /// # Errors
    /// * [`RecipientError::Unauthorized`] — caller is not the admin.
    /// * [`RecipientError::AlreadyRegistered`] — a recipient with this name exists.
    /// * [`RecipientError::InvalidName`] — name is empty or too long.
    ///
    /// # Events
    /// Emits `"recipient_registered"` with the name as topic and the record as data.
    pub fn register_recipient(
        env: Env,
        caller: Address,
        name: String,
        address: Address,
    ) -> Result<(), RecipientError> {
        Self::require_admin(&env, &caller)?;
        Self::validate_name(&name)?;

        let key = StorageKey::Recipient(name.clone());
        if env.storage().persistent().has(&key) {
            return Err(RecipientError::AlreadyRegistered);
        }

        let record = RecipientRecord {
            name: name.clone(),
            address,
        };
        env.storage().persistent().set(&key, &record);

        let count: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::RecipientCount)
            .ok_or(RecipientError::NotInitialized)?;
        env.storage().instance().set(
            &StorageKey::RecipientCount,
            &count.checked_add(1).ok_or(RecipientError::Overflow)?,
        );

        env.events().publish(
            (events::event_recipient_registered(&env), name),
            record,
        );
        Ok(())
    }

    /// Update the address of an existing recipient.
    ///
    /// The recipient must already be registered. The caller must be the admin
    /// and must authorize.
    ///
    /// # Parameters
    /// * `caller` — Must be the current admin; must authorize.
    /// * `name` — Name of the recipient to update.
    /// * `new_address` — Replacement on-ledger address.
    ///
    /// # Errors
    /// * [`RecipientError::Unauthorized`] — caller is not the admin.
    /// * [`RecipientError::NotFound`] — no recipient with this name exists.
    ///
    /// # Events
    /// Emits `"recipient_updated"` with the name as topic and the new record as data.
    pub fn update_recipient(
        env: Env,
        caller: Address,
        name: String,
        new_address: Address,
    ) -> Result<(), RecipientError> {
        Self::require_admin(&env, &caller)?;

        let key = StorageKey::Recipient(name.clone());
        if !env.storage().persistent().has(&key) {
            return Err(RecipientError::NotFound);
        }

        let record = RecipientRecord {
            name: name.clone(),
            address: new_address,
        };
        env.storage().persistent().set(&key, &record);

        env.events().publish(
            (events::event_recipient_updated(&env), name),
            record,
        );
        Ok(())
    }

    /// Remove a registered recipient by name.
    ///
    /// The recipient must exist. The caller must be the admin and must
    /// authorize. The recipient count is decremented with overflow-safe math.
    ///
    /// # Parameters
    /// * `caller` — Must be the current admin; must authorize.
    /// * `name` — Name of the recipient to remove.
    ///
    /// # Errors
    /// * [`RecipientError::Unauthorized`] — caller is not the admin.
    /// * [`RecipientError::NotFound`] — no recipient with this name exists.
    ///
    /// # Events
    /// Emits `"recipient_removed"` with the name as topic.
    pub fn remove_recipient(
        env: Env,
        caller: Address,
        name: String,
    ) -> Result<(), RecipientError> {
        Self::require_admin(&env, &caller)?;

        let key = StorageKey::Recipient(name.clone());
        if !env.storage().persistent().has(&key) {
            return Err(RecipientError::NotFound);
        }

        env.storage().persistent().remove(&key);

        let count: u32 = env
            .storage()
            .instance()
            .get(&StorageKey::RecipientCount)
            .ok_or(RecipientError::NotInitialized)?;
        env.storage().instance().set(
            &StorageKey::RecipientCount,
            &count.checked_sub(1).ok_or(RecipientError::Overflow)?,
        );

        env.events()
            .publish((events::event_recipient_removed(&env), name), ());
        Ok(())
    }

    // -----------------------------------------------------------------------
    // View-only entrypoints
    // -----------------------------------------------------------------------

    /// Return the admin address.
    ///
    /// # Errors
    /// * [`RecipientError::NotInitialized`] — contract was never initialized.
    pub fn get_admin(env: Env) -> Result<Address, RecipientError> {
        Self::admin(&env)
    }

    /// Return the registered recipient record for `name`, or
    /// [`RecipientError::NotFound`].
    ///
    /// Pure view: no auth, no storage writes.
    pub fn get_recipient(
        env: Env,
        name: String,
    ) -> Result<RecipientRecord, RecipientError> {
        Self::admin(&env)?; // ensure initialized
        Self::validate_name(&name)?;
        env.storage()
            .persistent()
            .get(&StorageKey::Recipient(name))
            .ok_or(RecipientError::NotFound)
    }

    /// Return whether a recipient with the given `name` is registered.
    ///
    /// Pure view: no auth, no storage writes.
    pub fn has_recipient(env: Env, name: String) -> Result<bool, RecipientError> {
        Self::admin(&env)?; // ensure initialized
        Self::validate_name(&name)?;
        Ok(env
            .storage()
            .persistent()
            .has(&StorageKey::Recipient(name)))
    }

    /// Return the total number of registered recipients.
    ///
    /// Pure view: no auth, no storage writes.
    pub fn get_recipient_count(env: Env) -> Result<u32, RecipientError> {
        if !env.storage().instance().has(&StorageKey::Admin) {
            return Err(RecipientError::NotInitialized);
        }
        Ok(env
            .storage()
            .instance()
            .get(&StorageKey::RecipientCount)
            .ok_or(RecipientError::NotInitialized)?)
    }
}

// ---------------------------------------------------------------------------
// Test modules
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;

#[cfg(test)]
mod rustdoc_tests {
    /// Verify that every public function has a `///` Rustdoc comment.
    #[test]
    fn every_public_fn_in_lib_has_rustdoc() {
        let source = include_str!("lib.rs")
            .split("// ---------------------------------------------------------------------------\n// Test modules")
            .next()
            .expect("lib.rs contains test module marker");
        let lines: std::vec::Vec<&str> = source.lines().collect();

        for (idx, line) in lines.iter().enumerate() {
            let trimmed = line.trim_start();
            if !(trimmed.starts_with("pub fn ")
                || trimmed.starts_with("pub(crate) fn ")
                || trimmed.starts_with("pub(super) fn "))
            {
                continue;
            }

            let has_rustdoc = lines[..idx]
                .iter()
                .rev()
                .map(|candidate| candidate.trim_start())
                .find(|candidate| !candidate.is_empty())
                .map(|candidate| candidate.starts_with("///"))
                .unwrap_or(false);

            assert!(
                has_rustdoc,
                "public function on line {} is missing /// rustdoc: {}",
                idx + 1,
                trimmed
            );
        }
    }
}
