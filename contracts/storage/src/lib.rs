#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env};

/// Storage keys used by the storage contract.
#[contracttype]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StorageKey {
    /// Storage tier: **Instance**
    /// Rationale: The admin address is a small, shared piece of configuration
    /// that is read frequently and is identical for all users of the contract instance.
    Admin,

    /// Storage tier: **Persistent**
    /// Rationale: User balances represent high-value user state that must not be silently archived
    /// without active tracking and bumping. Persistent storage is used to scale safely per-user.
    UserBalance(Address),

    /// Storage tier: **Temporary**
    /// Rationale: Request markers are used solely for deduplication (at-most-once semantics)
    /// within a short time window. Temporary storage avoids permanent bloat and archives cheaply.
    RequestMarker(u64),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    Overflow = 4,
    DuplicateRequest = 5,
}

#[contract]
pub struct StorageContract;

#[contractimpl]
impl StorageContract {
    /// Initializes the contract with an admin.
    pub fn init(env: Env, admin: Address) -> Result<(), Error> {
        if env.storage().instance().has(&StorageKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        env.storage().instance().set(&StorageKey::Admin, &admin);
        Ok(())
    }

    /// Increments the user's balance. Requires admin authorization.
    pub fn increment_balance(env: Env, admin: Address, user: Address, amount: i128) -> Result<(), Error> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&StorageKey::Admin)
            .ok_or(Error::NotInitialized)?;

        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        let key = StorageKey::UserBalance(user.clone());
        let current_balance: i128 = env.storage().persistent().get(&key).unwrap_or(0);

        let new_balance = current_balance.checked_add(amount).ok_or(Error::Overflow)?;

        env.storage().persistent().set(&key, &new_balance);

        Ok(())
    }

    /// Marks a request as processed. Requires user authorization.
    pub fn mark_request(env: Env, user: Address, request_id: u64) -> Result<(), Error> {
        user.require_auth();

        let key = StorageKey::RequestMarker(request_id);
        if env.storage().temporary().has(&key) {
            return Err(Error::DuplicateRequest);
        }

        env.storage().temporary().set(&key, &true);

        Ok(())
    }

    /// View function to get a user's balance.
    pub fn get_balance(env: Env, user: Address) -> i128 {
        env.storage()
            .persistent()
            .get(&StorageKey::UserBalance(user))
            .unwrap_or(0)
    }

    /// View function to check if a request is marked.
    pub fn is_request_marked(env: Env, request_id: u64) -> bool {
        env.storage()
            .temporary()
            .get(&StorageKey::RequestMarker(request_id))
            .unwrap_or(false)
    }
}

#[cfg(test)]
mod test;
