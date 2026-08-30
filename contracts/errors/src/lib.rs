#![no_std]

use soroban_sdk::{Address, Env, String, contract, contracterror, contractimpl, contracttype};

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    Overflow = 4,
}

#[contracttype]
pub enum DataKey {
    Admin,
    ErrorReg(u32),
    RecentErr(Address),
}

#[contract]
pub struct ErrorsContract;

#[contractimpl]
impl ErrorsContract {
    pub fn init(env: Env, admin: Address) -> Result<(), Error> {
        admin.require_auth();

        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        Ok(())
    }

    pub fn register_error(env: Env, admin: Address, code: u32, desc: String) -> Result<(), Error> {
        admin.require_auth();

        let stored_admin: Address = env
            .storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(Error::NotInitialized)?;
        if admin != stored_admin {
            return Err(Error::Unauthorized);
        }

        env.storage()
            .persistent()
            .set(&DataKey::ErrorReg(code), &desc);
        Ok(())
    }

    pub fn log_error(env: Env, user: Address, code: u32) -> Result<(), Error> {
        user.require_auth();

        // Overflow-safe: checked_add prevents silent wrap at u32::MAX.
        // This is the only arithmetic path in the contract; all other
        // operations are storage reads/writes and comparisons.
        let _safe_calc = code.checked_add(1).ok_or(Error::Overflow)?;

        env.storage()
            .temporary()
            .set(&DataKey::RecentErr(user.clone()), &code);

        env.storage()
            .temporary()
            .extend_ttl(&DataKey::RecentErr(user), 100, 100);

        Ok(())
    }
}

#[cfg(test)]
mod test;

pub mod ns {
    pub use callora_helpers::{
        ContractNamespace, KeyCategory, KeyOwnershipMarker, NamespacedKey, NamespacedStorage,
        ReadResult, accounting_key, config_key, ephemeral_key, idempotency_key, migration_key,
        state_key,
    };

    pub const CONTRACT_NS: ContractNamespace = ContractNamespace::Errors;

    #[inline]
    pub fn storage(env: &soroban_sdk::Env) -> NamespacedStorage<'_> {
        NamespacedStorage::new(env, CONTRACT_NS)
    }
}
