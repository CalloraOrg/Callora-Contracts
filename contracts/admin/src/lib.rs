#![no_std]

pub mod admin;
pub mod errors;
pub mod events;
pub mod limits;

#[cfg(test)]
mod test;

pub mod ns {
    pub use callora_helpers::{
        accounting_key, config_key, ephemeral_key, idempotency_key, migration_key, state_key,
        ContractNamespace, KeyCategory, KeyOwnershipMarker, NamespacedKey, NamespacedStorage,
        ReadResult,
    };

    pub const CONTRACT_NS: ContractNamespace = ContractNamespace::Admin;

    #[inline]
    pub fn storage(env: &soroban_sdk::Env) -> NamespacedStorage<'_> {
        NamespacedStorage::new(env, CONTRACT_NS)
    }
}
