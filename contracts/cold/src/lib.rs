#![no_std]

//! Callora cold-storage capability surface.
//!
//! The hot/cold balance split lives as an accounting partition inside the
//! vault (`contracts/vault/src/cold_storage.rs`). This crate exposes a
//! **read-only** [`views::capabilities`] bitmap so clients can detect which
//! cold features a deployment supports without parsing version strings.
//!
//! # Quick-start
//! ```ignore
//! let caps = client.capabilities();
//! if caps & CAP_COLD_MULTISIG_SWEEP != 0 {
//!     // safe to drive propose/approve cold-sweep flow
//! }
//! ```

mod views;

pub use views::{
    capabilities, ALL_CAPABILITIES, CAP_AUTO_REBALANCE, CAP_COLD_BALANCE_VIEW,
    CAP_COLD_MULTISIG_SWEEP, CAP_HOT_COLD_SPLIT, CAP_PENDING_COLD_SWEEP_VIEW, CAP_SET_COLD_SIGNERS,
    CAP_SET_HOT_COLD_RATIO,
};

use soroban_sdk::{contract, contractimpl, Env};

/// Thin contract facade that exposes cold capability discovery on-chain.
///
/// Cold accounting itself remains in the vault; this entrypoint exists so
/// integrators (and capability-delta monitors) have a stable `capabilities()`
/// view keyed to the cold feature set.
#[contract]
pub struct CalloraCold;

#[contractimpl]
impl CalloraCold {
    /// Return the cold-feature capability bitmap for this deployment.
    ///
    /// Pure view: no auth, no storage writes, no TTL bump.
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

    pub const CONTRACT_NS: ContractNamespace = ContractNamespace::Cold;

    #[inline]
    pub fn storage(env: &soroban_sdk::Env) -> NamespacedStorage<'_> {
        NamespacedStorage::new(env, CONTRACT_NS)
    }
}
