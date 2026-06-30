//! Emergency drain types and constants for the revenue pool.
//!
//! The emergency drain allows the admin to propose, execute, and cancel a
//! timelocked USDC drain to a designated address (typically the treasury).

use soroban_sdk::{contracttype, Address};

/// Mandatory delay between proposing and executing an emergency drain (24 hours).
pub const EMERGENCY_DRAIN_TIMELOCK_SECONDS: u64 = 86_400;

pub(crate) const EMERGENCY_DRAIN_KEY: &str = "emergency_drain";

/// Immutable snapshot stored for a pending emergency drain proposal.
///
/// Captures the destination, amount, proposal timestamp, and the earliest
/// ledger timestamp at which the drain may be executed.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingEmergencyDrain {
    /// Address that will receive the drained USDC.
    pub to: Address,
    /// Amount of USDC in base units to drain.
    pub amount: i128,
    /// Ledger timestamp when the proposal was created.
    pub proposed_at: u64,
    /// Earliest ledger timestamp at which the drain may be executed.
    pub execute_after: u64,
}
