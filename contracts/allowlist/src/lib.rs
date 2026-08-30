#![no_std]

pub mod ns {
    pub use callora_helpers::{
        accounting_key, config_key, ephemeral_key, idempotency_key, migration_key, state_key,
        ContractNamespace, KeyCategory, KeyOwnershipMarker, NamespacedKey, NamespacedStorage,
        ReadResult,
    };

    pub const CONTRACT_NS: ContractNamespace = ContractNamespace::Allowlist;

    #[inline]
    pub fn storage(env: &soroban_sdk::Env) -> NamespacedStorage<'_> {
        NamespacedStorage::new(env, CONTRACT_NS)
    }
}
