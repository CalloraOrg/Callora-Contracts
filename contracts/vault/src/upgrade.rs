//! Structured event payloads for vault upgrade lifecycle events.
//!
//! Splitting these into a dedicated module keeps `lib.rs` focused on business
//! logic and lets indexers import only the types they need.

use soroban_sdk::{contracttype, BytesN};

/// Payload for the `upgrade_started` event.
///
/// Emitted **before** `env.deployer().update_current_contract_wasm()` executes.
/// An indexer that observes `upgrade_started` without a subsequent
/// `upgrade_completed` in the same transaction can conclude the upgrade failed
/// (e.g., insufficient authorization or WASM validation error).
///
/// `previous_version` is `None` on the first ever upgrade; `Some` on all
/// subsequent upgrades, allowing full version history reconstruction.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UpgradeStartedData {
    /// The WASM hash that will be installed if the upgrade succeeds.
    pub new_wasm_hash: BytesN<32>,
    /// The WASM hash stored from the previous upgrade, or `None` if this is
    /// the first upgrade of this contract instance.
    pub previous_version: Option<BytesN<32>>,
}

/// Payload for the `upgrade_completed` event.
///
/// Emitted immediately **after** `env.deployer().update_current_contract_wasm()`
/// returns successfully. The `new_wasm_hash` matches the value from the
/// corresponding `upgrade_started` payload emitted earlier in the same
/// transaction, allowing indexers to correlate the two events.
#[contracttype]
#[derive(Clone, Debug)]
pub struct UpgradeCompletedData {
    /// The WASM hash that was successfully installed.
    pub new_wasm_hash: BytesN<32>,
}
