//! Emergency drain types and constants for the revenue pool.
//!
//! The emergency drain allows the admin to propose, execute, and cancel a
//! timelocked USDC drain to a designated address (typically the treasury).
//!
//! ## Security Model
//!
//! The emergency drain is a **last-resort** mechanism. It is protected by:
//!
//! 1. **Admin-only authorization**: Every state-changing entrypoint calls
//!    `caller.require_auth()` and verifies `caller == admin`. When the admin is
//!    configured as a Stellar multisig account, the native Soroban/Stellar
//!    multi-signature threshold and signer weights are enforced automatically by
//!    `require_auth`, providing Multisig + timelock protection without extra
//!    contract logic.
//!
//! 2. **Mandatory 24-hour timelock**: A `propose_emergency_drain` call stores a
//!    [`PendingEmergencyDrain`] snapshot. `execute_emergency_drain` may only
//!    succeed once `env.ledger().timestamp() >= pending.execute_after`.
//!
//! 3. **Cancellability**: The admin may cancel a pending drain at any time
//!    before execution via `cancel_emergency_drain`, which removes the pending
//!    snapshot and emits an audit event.
//!
//! 4. **Replay protection**: The pending snapshot is removed atomically when
//!    the drain is executed, so the same proposal cannot be replayed.
//!
//! 5. **Self-drain guard**: Proposing a drain to the contract's own address is
//!    rejected immediately.
//!
//! 6. **Overflow-safe timestamps**: If the proposal timestamp would overflow
//!    when the timelock offset is added, the call panics with
//!    `"timelock overflow"` rather than silently wrapping.

use soroban_sdk::{contracttype, Address};

/// Mandatory delay (in seconds) between proposing and executing an emergency
/// drain. Set to 86 400 s = 24 hours, giving operators a window to cancel
/// a fraudulent or mistaken proposal.
pub const EMERGENCY_DRAIN_TIMELOCK_SECONDS: u64 = 86_400;

/// Storage key used to persist the [`PendingEmergencyDrain`] snapshot in the
/// contract's instance storage. Kept `pub(crate)` so `lib.rs` can reference it
/// directly without re-exporting the raw string.
pub(crate) const EMERGENCY_DRAIN_KEY: &str = "emergency_drain";

/// Immutable snapshot stored for a pending emergency drain proposal.
///
/// Every field is set at proposal time and never mutated. An admin or off-chain
/// monitor can read this struct via [`RevenuePool::get_pending_emergency_drain`]
/// to verify the intent before the timelock expires.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PendingEmergencyDrain {
    /// Address that will receive the drained USDC.
    pub to: Address,
    /// Amount of USDC in base units to drain.
    pub amount: i128,
    /// Ledger timestamp (seconds since Unix epoch) when the proposal was created.
    pub proposed_at: u64,
    /// Earliest ledger timestamp at which `execute_emergency_drain` may succeed.
    ///
    /// Equals `proposed_at + EMERGENCY_DRAIN_TIMELOCK_SECONDS`.
    pub execute_after: u64,
}
