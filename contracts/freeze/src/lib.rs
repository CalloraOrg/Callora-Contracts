#![no_std]

pub mod errors;
#[cfg(test)]
mod test;

use crate::errors::FreezeError;
use soroban_sdk::{
    contract, contractimpl, contracttype, symbol_short, Address, Env, Symbol,
};

/// Storage keys used by the freeze contract instance.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StorageKey {
    /// Address of the contract administrator.
    Admin,
    /// Optional designated freeze operator address.
    FreezeOperator,
    /// Boolean flag indicating whether global freeze is active.
    IsFrozen,
}

#[contract]
pub struct CalloraFreeze;

#[contractimpl]
impl CalloraFreeze {
    /// Initializes the Callora Freeze contract with an administrator address.
    ///
    /// **What**: Registers the administrative owner for the freeze control module and establishes initial unfrozen contract state.
    ///
    /// **How**: Validates cryptographic signature authorization from the `admin` address parameter, checks instance storage to ensure initialization has not already occurred, sets the stored `Admin` key, and sets `IsFrozen` state to `false`.
    ///
    /// **Why**: Contract initialization must be atomic, single-execution, and restricted to the protocol owner to establish access control boundaries prior to accepting freeze/unfreeze state updates.
    ///
    /// # Arguments
    /// * `env` - Soroban environment handle.
    /// * `admin` - Address designated as the protocol freeze administrator.
    ///
    /// # Errors
    /// * `AlreadyInitialized` (code 2) - If `init` has previously been invoked.
    pub fn init(env: Env, admin: Address) -> Result<(), FreezeError> {
        admin.require_auth();

        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(FreezeError::AlreadyInitialized);
        }

        env.storage().instance().set(&StorageKey::Admin, &admin);
        env.storage().instance().set(&StorageKey::IsFrozen, &false);

        env.events().publish(
            (symbol_short!("init"), admin),
            false,
        );

        Ok(())
    }

    /// Triggers an emergency freeze on the contract.
    ///
    /// **What**: Activates global contract freeze state to halt sensitive operations across protected system modules.
    ///
    /// **How**: Demands cryptographic signature from `caller`, verifies that `caller` matches either the stored `Admin` address or the designated `FreezeOperator`, confirms that `IsFrozen` is currently `false`, updates `IsFrozen` storage key to `true`, and emits a `frozen` event containing the caller address and reason `Symbol`.
    ///
    /// **Why**: Emergency pauses allow governance or automated security monitors (circuit breakers) to rapidly halt system operations during detected security threats, exploits, or market anomalies.
    ///
    /// # Arguments
    /// * `env` - Soroban environment handle.
    /// * `caller` - Address initiating the emergency freeze action (admin or freeze operator).
    /// * `reason` - `Symbol` identifier documenting the justification for the freeze action.
    ///
    /// # Errors
    /// * `NotInitialized` (code 1) - If the contract has not yet been initialized.
    /// * `Unauthorized` (code 3) - If caller is neither the admin nor the active freeze operator.
    /// * `AlreadyFrozen` (code 4) - If global freeze is already active.
    pub fn freeze(env: Env, caller: Address, reason: Symbol) -> Result<(), FreezeError> {
        caller.require_auth();
        Self::require_admin_or_operator(&env, &caller)?;

        if Self::is_frozen(env.clone()) {
            return Err(FreezeError::AlreadyFrozen);
        }

        env.storage().instance().set(&StorageKey::IsFrozen, &true);

        env.events().publish(
            (Symbol::new(&env, "frozen"), caller),
            reason,
        );

        Ok(())
    }

    /// Lifts an emergency freeze to restore normal operations.
    ///
    /// **What**: Deactivates contract freeze status and returns system entrypoints to active processing.
    ///
    /// **How**: Enforces cryptographic authentication for `caller`, checks that `caller` matches the stored `Admin` address, asserts that `IsFrozen` is currently `true`, mutates `IsFrozen` key to `false`, and emits an `unfrozen` event.
    ///
    /// **Why**: Unfreezing is restricted strictly to the protocol administrator (excluding standard operators) to ensure thorough security review and remediation before operational recovery.
    ///
    /// # Arguments
    /// * `env` - Soroban environment handle.
    /// * `caller` - Administrator address authorizing unfreeze recovery.
    ///
    /// # Errors
    /// * `NotInitialized` (code 1) - If the contract has not yet been initialized.
    /// * `Unauthorized` (code 3) - If caller is not the primary admin.
    /// * `NotFrozen` (code 5) - If the contract is not currently frozen.
    pub fn unfreeze(env: Env, caller: Address) -> Result<(), FreezeError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        if !Self::is_frozen(env.clone()) {
            return Err(FreezeError::NotFrozen);
        }

        env.storage().instance().set(&StorageKey::IsFrozen, &false);

        env.events().publish(
            (Symbol::new(&env, "unfrozen"), caller),
            (),
        );

        Ok(())
    }

    /// Sets or revokes a designated freeze operator address.
    ///
    /// **What**: Grants or revokes emergency freeze authority to a designated operator account.
    ///
    /// **How**: Validates cryptographic signature from `caller`, confirms `caller` is the admin, updates or removes the `FreezeOperator` storage key, and emits an `operator_set` event.
    ///
    /// **Why**: Delegated operators (such as automated automated risk bots or security multisigs) need freeze capability without holding full administrative privileges or unfreeze authority.
    ///
    /// # Arguments
    /// * `env` - Soroban environment handle.
    /// * `caller` - Admin address managing operator role assignment.
    /// * `operator` - Optional address to assign as freeze operator (`None` revokes operator status).
    ///
    /// # Errors
    /// * `NotInitialized` (code 1) - If contract is not initialized.
    /// * `Unauthorized` (code 3) - If caller is not the primary admin.
    pub fn set_freeze_operator(
        env: Env,
        caller: Address,
        operator: Option<Address>,
    ) -> Result<(), FreezeError> {
        caller.require_auth();
        Self::require_admin(&env, &caller)?;

        if let Some(ref op) = operator {
            env.storage().instance().set(&StorageKey::FreezeOperator, op);
        } else {
            env.storage().instance().remove(&StorageKey::FreezeOperator);
        }

        env.events().publish(
            (Symbol::new(&env, "op_set"), caller),
            operator,
        );

        Ok(())
    }

    /// Queries the primary administrator address.
    ///
    /// **What**: Returns the configured admin account responsible for governance and unfreeze permissions.
    ///
    /// **How**: Reads the `Admin` key from instance storage and returns `Result<Address, FreezeError>`.
    ///
    /// **Why**: Integrators, frontends, and monitoring scripts require read access to contract governance parameters.
    ///
    /// # Arguments
    /// * `env` - Soroban environment handle.
    ///
    /// # Errors
    /// * `NotInitialized` (code 1) - If contract has not been initialized.
    pub fn get_admin(env: Env) -> Result<Address, FreezeError> {
        env.storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(FreezeError::NotInitialized)
    }

    /// Queries the currently assigned freeze operator address, if any.
    ///
    /// **What**: Retrieves the delegated operator account holding emergency freeze capability.
    ///
    /// **How**: Inspects instance storage for the `FreezeOperator` key and returns `Option<Address>`.
    ///
    /// **Why**: Public view entrypoint allows auditing access rights and verifying operational bot addresses.
    ///
    /// # Arguments
    /// * `env` - Soroban environment handle.
    pub fn get_freeze_operator(env: Env) -> Option<Address> {
        env.storage()
            .instance()
            .get(&StorageKey::FreezeOperator)
    }

    /// Queries whether emergency freeze is currently active.
    ///
    /// **What**: Returns a boolean status indicating whether the global freeze is currently enforced.
    ///
    /// **How**: Reads the `IsFrozen` boolean flag from instance storage, defaulting to `false` if uninitialized.
    ///
    /// **Why**: Interfacing contracts and SDK clients query this flag to guard state-changing entrypoints during emergency incidents.
    ///
    /// # Arguments
    /// * `env` - Soroban environment handle.
    pub fn is_frozen(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&StorageKey::IsFrozen)
            .unwrap_or(false)
    }

    /// Internal helper validating that caller matches the primary admin address.
    fn require_admin(env: &Env, caller: &Address) -> Result<(), FreezeError> {
        let admin = Self::get_admin(env.clone())?;
        if caller != &admin {
            return Err(FreezeError::Unauthorized);
        }
        Ok(())
    }

    /// Internal helper validating that caller is either admin or the active freeze operator.
    fn require_admin_or_operator(env: &Env, caller: &Address) -> Result<(), FreezeError> {
        let admin = Self::get_admin(env.clone())?;
        if caller == &admin {
            return Ok(());
        }

        if let Some(operator) = Self::get_freeze_operator(env.clone()) {
            if caller == &operator {
                return Ok(());
            }
        }

        Err(FreezeError::Unauthorized)
    }
}

