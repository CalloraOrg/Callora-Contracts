#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env};

/// The storage key used to store the last authenticated signer.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageKey {
    /// Stores the address of the last caller who executed `reflect_auth`.
    LastSigner,
}

/// Errors returned by the Reflector contract.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// An unexpected overflow occurred.
    Overflow = 1,
}

#[contract]
pub struct ReflectorContract;

#[contractimpl]
impl ReflectorContract {
    /// A state-changing entrypoint that requires authorization from the provided `signer`.
    /// Captures and stores the signer's identity in instance storage.
    ///
    /// # Parameters
    /// - `env`: The contract environment.
    /// - `signer`: The address of the caller attempting the action.
    ///
    /// # Returns
    /// - `Result<(), Error>`: Ok on success.
    pub fn reflect_auth(env: Env, signer: Address) -> Result<(), Error> {
        // Enforce authorization for the signer.
        signer.require_auth();

        // Capture the authenticated identity in storage.
        env.storage().instance().set(&StorageKey::LastSigner, &signer);

        Ok(())
    }

    /// View function returning the last authenticated signer.
    pub fn get_last_signer(env: Env) -> Option<Address> {
        env.storage().instance().get(&StorageKey::LastSigner)
    }
}

