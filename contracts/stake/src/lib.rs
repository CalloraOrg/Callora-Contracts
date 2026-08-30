#![no_std]

pub mod migrate;
mod views;

pub use views::{
    capabilities, CAP_DELEGATION, CAP_REWARDS, CAP_SLASHING, CAP_STAKE_UNSTAKE, CAP_STAKE_VIEW,
    CAP_WITHDRAW_TIMELOCK, SUPPORTED_CAPABILITIES,
};

pub use migrate::{CalloraStakeMigrate, CurrentStake, LegacyStake, StakeMigrateError, StorageKey};

use soroban_sdk::{contract, contractimpl, Env};

#[contract]
pub struct CalloraStake;

#[contractimpl]
impl CalloraStake {
    pub fn capabilities(env: Env) -> u64 {
        views::capabilities(&env)
    }
}

pub mod ns {
    pub use callora_helpers::{
        accounting_key, config_key, ephemeral_key, idempotency_key, migration_key, state_key,
        ContractNamespace, KeyCategory, KeyOwnershipMarker, NamespacedKey, NamespacedStorage,
        ReadResult,
    };

    pub const CONTRACT_NS: ContractNamespace = ContractNamespace::Stake;

    #[inline]
    pub fn storage(env: &soroban_sdk::Env) -> NamespacedStorage<'_> {
        NamespacedStorage::new(env, CONTRACT_NS)
    }
}
