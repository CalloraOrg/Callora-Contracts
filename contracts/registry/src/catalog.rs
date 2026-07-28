//! External catalog interface invoked by the registry during offering registration.
//!
//! Production deployments point `init`'s `catalog` address at a contract that
//! persists offering metadata (for example an on-chain catalog or vault hook).
//! Integration tests swap in mock callees that revert or panic to exercise
//! cross-contract failure safety.

use soroban_sdk::{contractclient, Address, Env, String};

/// Cross-contract callee surface used by [`crate::CalloraRegistry::register_offering`].
#[contractclient(name = "OfferingCatalogClient")]
pub trait OfferingCatalog {
    /// Publish or anchor `metadata` for `offering_id` on behalf of `registry`.
    fn put_offering(
        env: Env,
        registry: Address,
        offering_id: String,
        metadata: String,
    );
}
