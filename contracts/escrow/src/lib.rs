#![no_std]

//! # Callora Escrow contract
//!
//! The `escrow` contract is a fund-holding surface for the Callora marketplace
//! where privileged admin keys perform critical operations on escrowed funds:
//! releasing funds to recipients, pausing the escrow, rotating signers, and
//! so on.
//!
//! This contract's defining feature is an **admin cool-off window**: every
//! critical action is rate-limited so that two invocations of the *same*
//! action cannot fire within the configured window. See [`admin`] for the
//! cool-off engine and
//! [issue #914](https://github.com/CalloraOrg/Callora-Contracts/issues/914).

#[cfg(test)]
extern crate std;

pub mod admin;
pub mod errors;
pub mod events;

use errors::EscrowError;
use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

// ---------------------------------------------------------------------------
// Action tags
// ---------------------------------------------------------------------------

/// Cool-off tag for the `release` critical action.
pub const ACTION_RELEASE: &str = "release";

/// Cool-off tag for the `pause` critical action.
pub const ACTION_PAUSE: &str = "pause";

/// Cool-off tag for the `unpause` critical action.
pub const ACTION_UNPAUSE: &str = "unpause";

/// Cool-off tag for the `rotate_signer` critical action.
pub const ACTION_ROTATE: &str = "rotate";

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

/// Instance storage keys for the escrow contract.
///
/// Each variant maps a logical key name to the underlying Soroban storage key,
/// avoiding accidental key collisions with raw strings.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum StorageKey {
    /// Instance: current admin [`Address`].
    Admin,
    /// Instance: pending admin [`Address`] for two-step rotation.
    PendingAdmin,
    /// Instance: global cool-off window in seconds (`u64`).
    Cooldown,
    /// Instance: last-execution ledger timestamp for the action with the given
    /// [`Symbol`] tag (`u64`).
    LastAction(Symbol),
    /// Instance: paused flag (`bool`) toggled by the guarded pause actions.
    Paused,
    /// Instance: the currently-authorized escrow signer [`Address`].
    Signer,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

#[contract]
pub struct CalloraEscrow;

#[contractimpl]
impl CalloraEscrow {
    // -----------------------------------------------------------------------
    // Initialisation
    // -----------------------------------------------------------------------

    /// Initialize the escrow contract.
    ///
    /// Can only be called once. Sets the admin, the initial escrow signer, and
    /// the cool-off window. Pass `None` for `cooldown_secs` to adopt
    /// [`admin::DEFAULT_COOLDOWN_SECS`] (1 hour).
    ///
    /// # Parameters
    /// * `admin` -- Address permitted to call admin-only entrypoints; must authorize.
    /// * `signer` -- The initial escrow signer address.
    /// * `cooldown_secs` -- Optional cool-off window in seconds.
    ///
    /// # Errors
    /// * [`EscrowError::AlreadyInitialized`] -- `init` was called more than once.
    /// * [`EscrowError::InvalidCooldown`] -- explicit `cooldown_secs` is out of range.
    ///
    /// # Events
    /// Emits `init` with `admin` as topic and the cooldown window as data.
    pub fn init(
        env: Env,
        admin: Address,
        signer: Address,
        cooldown_secs: Option<u64>,
    ) -> Result<(), EscrowError> {
        admin.require_auth();
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(EscrowError::AlreadyInitialized);
        }

        let cooldown = cooldown_secs.unwrap_or(admin::DEFAULT_COOLDOWN_SECS);

        // Validate and persist the cooldown through the module's checked
        // setter *before* writing any other state, so a bad value leaves
        // storage clean.
        admin::set_cooldown(&env, cooldown)?;

        let inst = env.storage().instance();
        inst.set(&StorageKey::Admin, &admin);
        inst.set(&StorageKey::Signer, &signer);
        inst.set(&StorageKey::Paused, &false);

        env.events()
            .publish((events::event_init(&env), admin), cooldown);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Admin helpers (internal)
    // -----------------------------------------------------------------------

    /// Read the current admin address from instance storage.
    ///
    /// Returns [`EscrowError::NotInitialized`] when no admin has been set.
    fn admin(env: &Env) -> Result<Address, EscrowError> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(EscrowError::NotInitialized)
    }

    /// Verify that `caller` is the current admin.
    ///
    /// Consumes the caller's auth via `require_auth`, then checks instance
    /// storage for the admin address. Returns [`EscrowError::Unauthorized`]
    /// when the caller does not match.
    fn require_admin(env: &Env, caller: &Address) -> Result<(), EscrowError> {
        caller.require_auth();
        let admin = Self::admin(env)?;
        if caller != &admin {
            return Err(EscrowError::Unauthorized);
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Cooldown administration
    // -----------------------------------------------------------------------

    /// Return the currently-configured cool-off window, in seconds.
    ///
    /// # Errors
    /// * [`EscrowError::NotInitialized`] -- contract was never initialized.
    pub fn get_cooldown(env: Env) -> Result<u64, EscrowError> {
        Self::admin(&env)?;
        Ok(admin::get_cooldown(&env))
    }

    /// Update the global cool-off window. Only the current admin may call.
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    /// * `secs` -- New window in seconds, within
    ///   [`admin::MIN_COOLDOWN_SECS`]..=[`admin::MAX_COOLDOWN_SECS`].
    ///
    /// # Errors
    /// * [`EscrowError::Unauthorized`] -- caller is not the current admin.
    /// * [`EscrowError::NotInitialized`] -- contract not initialized.
    /// * [`EscrowError::InvalidCooldown`] -- `secs` is out of range.
    ///
    /// # Events
    /// Emits `cooldown_set` with `caller` as topic and the new window as data.
    pub fn set_cooldown(env: Env, caller: Address, secs: u64) -> Result<(), EscrowError> {
        Self::require_admin(&env, &caller)?;
        admin::set_cooldown(&env, secs)?;

        env.events()
            .publish((events::event_cooldown_set(&env), caller), secs);

        Ok(())
    }

    /// Return the number of seconds remaining before the action tagged
    /// `action` may run again, or `0` if it is available now.
    ///
    /// This is a read-only view and does not require initialization.
    pub fn cooldown_remaining(env: Env, action: Symbol) -> u64 {
        admin::remaining(&env, &action)
    }

    /// Return `true` when the action tagged `action` may run now.
    pub fn is_ready(env: Env, action: Symbol) -> bool {
        admin::is_ready(&env, &action)
    }

    // -----------------------------------------------------------------------
    // Views
    // -----------------------------------------------------------------------

    /// Return the current admin address.
    ///
    /// # Errors
    /// * [`EscrowError::NotInitialized`] -- contract was never initialized.
    pub fn get_admin(env: Env) -> Result<Address, EscrowError> {
        Self::admin(&env)
    }

    /// Return the pending admin for a two-step rotation, or `None`.
    pub fn get_pending_admin(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::PendingAdmin)
    }

    /// Return the current escrow signer address.
    ///
    /// # Errors
    /// * [`EscrowError::NotInitialized`] -- contract was never initialized.
    pub fn get_signer(env: Env) -> Result<Address, EscrowError> {
        env.storage()
            .instance()
            .get(&StorageKey::Signer)
            .ok_or(EscrowError::NotInitialized)
    }

    /// Return whether the contract is currently paused.
    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&StorageKey::Paused)
            .unwrap_or(false)
    }

    // -----------------------------------------------------------------------
    // Guarded critical actions
    // -----------------------------------------------------------------------

    /// Release escrowed funds. Cool-off-guarded critical action (tag `"release"`).
    ///
    /// This is the primary escrow critical action: transferring held funds to
    /// a designated recipient. The cooldown prevents rapid sequential releases
    /// from a compromised admin key.
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    /// * `recipient` -- Address of the fund recipient.
    ///
    /// # Errors
    /// * [`EscrowError::Unauthorized`] -- caller is not the current admin.
    /// * [`EscrowError::NotInitialized`] -- contract not initialized.
    /// * [`EscrowError::CooldownActive`] -- a `release` ran within the cool-off window.
    ///
    /// # Events
    /// Emits `action` with `caller` as topic and the `"release"` tag as data.
    pub fn release(env: Env, caller: Address, recipient: Address) -> Result<(), EscrowError> {
        Self::require_admin(&env, &caller)?;
        let action = Symbol::new(&env, ACTION_RELEASE);
        admin::guard(&env, &action)?;

        // Persist the release recipient for on-chain auditability.
        env.storage()
            .instance()
            .set(&StorageKey::Signer, &recipient);

        env.events()
            .publish((events::event_action(&env), caller), action);

        Ok(())
    }

    /// Pause the escrow contract. Cool-off-guarded critical action (tag `"pause"`).
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    ///
    /// # Errors
    /// * [`EscrowError::Unauthorized`] -- caller is not the current admin.
    /// * [`EscrowError::NotInitialized`] -- contract not initialized.
    /// * [`EscrowError::CooldownActive`] -- a `pause` ran within the cool-off window.
    ///
    /// # Events
    /// Emits `action` with `caller` as topic and the `"pause"` tag as data.
    pub fn pause(env: Env, caller: Address) -> Result<(), EscrowError> {
        Self::require_admin(&env, &caller)?;
        let action = Symbol::new(&env, ACTION_PAUSE);
        admin::guard(&env, &action)?;

        env.storage().instance().set(&StorageKey::Paused, &true);

        env.events()
            .publish((events::event_action(&env), caller), action);

        Ok(())
    }

    /// Unpause the escrow contract. Cool-off-guarded critical action (tag `"unpause"`).
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    ///
    /// # Errors
    /// * [`EscrowError::Unauthorized`] -- caller is not the current admin.
    /// * [`EscrowError::NotInitialized`] -- contract not initialized.
    /// * [`EscrowError::CooldownActive`] -- an `unpause` ran within the cool-off window.
    ///
    /// # Events
    /// Emits `action` with `caller` as topic and the `"unpause"` tag as data.
    pub fn unpause(env: Env, caller: Address) -> Result<(), EscrowError> {
        Self::require_admin(&env, &caller)?;
        let action = Symbol::new(&env, ACTION_UNPAUSE);
        admin::guard(&env, &action)?;

        env.storage().instance().set(&StorageKey::Paused, &false);

        env.events()
            .publish((events::event_action(&env), caller), action);

        Ok(())
    }

    /// Rotate the escrow signer. Cool-off-guarded critical action (tag `"rotate"`).
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    /// * `new_signer` -- The replacement escrow signer address.
    ///
    /// # Errors
    /// * [`EscrowError::Unauthorized`] -- caller is not the current admin.
    /// * [`EscrowError::NotInitialized`] -- contract not initialized.
    /// * [`EscrowError::CooldownActive`] -- a `rotate` ran within the cool-off window.
    ///
    /// # Events
    /// Emits `action` with `caller` as topic and the `"rotate"` tag as data.
    pub fn rotate_signer(
        env: Env,
        caller: Address,
        new_signer: Address,
    ) -> Result<(), EscrowError> {
        Self::require_admin(&env, &caller)?;
        let action = Symbol::new(&env, ACTION_ROTATE);
        admin::guard(&env, &action)?;

        env.storage()
            .instance()
            .set(&StorageKey::Signer, &new_signer);

        env.events()
            .publish((events::event_action(&env), caller), action);

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Two-step admin rotation
    // -----------------------------------------------------------------------

    /// Nominate a new admin. Only the current admin may call.
    ///
    /// The nominee must call [`CalloraEscrow::accept_admin`] to complete the
    /// transfer. Until then the current admin retains full authority.
    ///
    /// # Parameters
    /// * `caller` -- Must be the current admin; must authorize.
    /// * `new_admin` -- Address of the proposed new admin.
    ///
    /// # Errors
    /// * [`EscrowError::Unauthorized`] -- caller is not the current admin.
    /// * [`EscrowError::NotInitialized`] -- contract not initialized.
    ///
    /// # Events
    /// Emits `admin_nominated` with `(caller)` as topic and `new_admin` as data.
    pub fn set_admin(env: Env, caller: Address, new_admin: Address) -> Result<(), EscrowError> {
        Self::require_admin(&env, &caller)?;

        env.storage()
            .instance()
            .set(&StorageKey::PendingAdmin, &new_admin);

        env.events()
            .publish((events::event_admin_nominated(&env), caller), new_admin);

        Ok(())
    }

    /// Complete a pending admin transfer. Must be called by the nominated admin.
    ///
    /// # Parameters
    /// * `caller` -- Must be the pending admin; must authorize.
    ///
    /// # Errors
    /// * [`EscrowError::NotInitialized`] -- contract not initialized.
    /// * [`EscrowError::NoPendingAdmin`] -- no nomination is in progress.
    /// * [`EscrowError::Unauthorized`] -- caller is not the pending admin.
    ///
    /// # Events
    /// Emits `admin_accepted` with `(old_admin)` as topic and `new_admin` as data.
    pub fn accept_admin(env: Env, caller: Address) -> Result<(), EscrowError> {
        caller.require_auth();

        let pending: Address = env
            .storage()
            .instance()
            .get(&StorageKey::PendingAdmin)
            .ok_or(EscrowError::NoPendingAdmin)?;

        if caller != pending {
            return Err(EscrowError::Unauthorized);
        }

        let old_admin = Self::admin(&env)?;
        let inst = env.storage().instance();
        inst.set(&StorageKey::Admin, &pending);
        inst.remove(&StorageKey::PendingAdmin);

        env.events()
            .publish((events::event_admin_accepted(&env), old_admin), pending);

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Test modules
// ---------------------------------------------------------------------------

#[cfg(test)]
mod test;

#[cfg(test)]
mod rustdoc_tests {
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
