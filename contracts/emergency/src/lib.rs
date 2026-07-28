#![no_std]
//! Callora emergency capability surface.
//!
//! This crate exposes a **read-only** [`capabilities`] bitmap so clients can
//! detect which emergency features a deployment supports without parsing
//! version strings or querying contract state:
//!
//! ```ignore
//! let caps = client.capabilities();
//! if caps & CAP_EMERGENCY_DRAIN_PROPOSE != 0 {
//!     // safe to call propose_emergency_drain
//! }
//! ```
//!
//! # Bit layout
//! | Bit | Constant | Feature |
//! |-----|----------|---------|
//! | 0   | [`CAP_EMERGENCY_PAUSE`]         | Halt all state-changing entrypoints |
//! | 1   | [`CAP_EMERGENCY_UNPAUSE`]        | Restore normal operations |
//! | 2   | [`CAP_EMERGENCY_DRAIN_PROPOSE`]  | Open a time-locked drain proposal |
//! | 3   | [`CAP_EMERGENCY_DRAIN_EXECUTE`]  | Execute proposal after 24-h timelock |
//! | 4   | [`CAP_EMERGENCY_DRAIN_CANCEL`]   | Cancel a pending proposal |
//! | 5   | [`CAP_PENDING_DRAIN_VIEW`]       | Read in-flight proposal without auth |
//!
//! Bits 6–63 are reserved and always zero.

mod views;

pub use views::{
    capabilities, ALL_CAPABILITIES, CAP_EMERGENCY_DRAIN_CANCEL, CAP_EMERGENCY_DRAIN_EXECUTE,
    CAP_EMERGENCY_DRAIN_PROPOSE, CAP_EMERGENCY_PAUSE, CAP_EMERGENCY_UNPAUSE, CAP_PENDING_DRAIN_VIEW,
};

use soroban_sdk::{contract, contractimpl, Env};

/// Thin contract facade that exposes emergency capability discovery on-chain.
///
/// The actual emergency logic (pause, drain, etc.) lives in the vault and
/// revenue-pool contracts. This entrypoint provides a stable
/// `capabilities()` view that clients and monitors can query to detect
/// which emergency features are available without inspecting ABI versions.
#[contract]
pub struct CalloraEmergency;

#[contractimpl]
impl CalloraEmergency {
    /// Return the emergency-feature capability bitmap for this deployment.
    ///
    /// Each set bit signals a supported emergency feature. Bits are assigned
    /// once and never reused, so clients can rely on them across upgrades.
    ///
    /// Pure view: no auth required, no storage writes, no TTL bump.
    pub fn capabilities(env: Env) -> u64 {
        views::capabilities(&env)
    }
}
