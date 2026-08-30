#![no_std]

extern crate alloc;

pub mod snapshot_diff;
pub mod storage_namespace;

pub use storage_namespace::{
    accounting_key, config_key, ephemeral_key, idempotency_key, migration_key, state_key,
    ContractNamespace, KeyCategory, KeyOwnershipMarker, KeyTtlPolicy, NamespacedKey,
    NamespacedStorage, ReadResult,
};
