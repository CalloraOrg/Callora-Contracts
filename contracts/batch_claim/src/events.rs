//! Event topic Symbol constructors for the Callora Batch-Claim contract.
//!
//! All event topic strings are defined here to keep XDR byte layout stable
//! and prevent accidental renames from silently breaking off-chain indexers.

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"bc_init"` event topic.
///
/// Emitted once during [`crate::CalloraBatchClaim::init`].
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "bc_init")
}

/// Returns the Symbol for the `"claim_added"` event topic.
///
/// Emitted when a new claim is created or an existing one is incremented by
/// [`crate::CalloraBatchClaim::add_claim`].
pub fn event_claim_added(env: &Env) -> Symbol {
    Symbol::new(env, "claim_added")
}

/// Returns the Symbol for the `"claims_settled"` event topic.
///
/// Emitted for each claimant during [`crate::CalloraBatchClaim::batch_claim`].
pub fn event_claims_settled(env: &Env) -> Symbol {
    Symbol::new(env, "claims_settled")
}

/// Returns the Symbol for the `"claim_cancelled"` event topic.
///
/// Emitted when a pending claim is cancelled by
/// [`crate::CalloraBatchClaim::cancel_claim`].
pub fn event_claim_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "claim_cancelled")
}
