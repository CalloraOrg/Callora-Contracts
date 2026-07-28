//! Read-only capability views for Callora cold storage.
//!
//! Each bit in the `u64` returned by [`capabilities`] represents a distinct,
//! stable cold-storage feature. Bits are assigned once and never reassigned,
//! so clients can detect capability deltas across upgrades with simple
//! bitwise comparison:
//!
//! ```ignore
//! let before = old_client.capabilities();
//! let after = new_client.capabilities();
//! let added = after & !before;
//! let removed = before & !after;
//! ```
//!
//! # Mapping to vault cold storage
//! These bits describe features implemented by
//! `contracts/vault/src/cold_storage.rs` (hot/cold split, auto-rebalance,
//! N-of-M cold sweep). Reserved bits (7–63) are always zero.

use soroban_sdk::Env;

/// Bit 0 — Hot/cold split: vault balance is partitioned into hot + cold pools
/// with `hot + cold == tracked total`. Configured via cold-storage init.
/// Introduced: v1.0.0
pub const CAP_HOT_COLD_SPLIT: u64 = 1 << 0;

/// Bit 1 — Auto-rebalance: deposits may move excess hot funds into cold when
/// hot share drifts beyond `rebalance_threshold_bps` from `hot_bps`.
/// Introduced: v1.0.0
pub const CAP_AUTO_REBALANCE: u64 = 1 << 1;

/// Bit 2 — Multisig cold sweep: moving funds out of cold requires N-of-M
/// propose/approve (`propose_cold_sweep` / `approve_cold_sweep`).
/// Introduced: v1.0.0
pub const CAP_COLD_MULTISIG_SWEEP: u64 = 1 << 2;

/// Bit 3 — Hot/cold ratio update: target `hot_bps` (and related threshold)
/// can be updated without replacing the full signer set.
/// Introduced: v1.0.0
pub const CAP_SET_HOT_COLD_RATIO: u64 = 1 << 3;

/// Bit 4 — Cold signer set update: the N-of-M signer roster / threshold can
/// be rotated independently of the hot/cold ratio.
/// Introduced: v1.0.0
pub const CAP_SET_COLD_SIGNERS: u64 = 1 << 4;

/// Bit 5 — Cold balance view: clients can read the current `{hot, cold}`
/// accounting split (and derive `total`).
/// Introduced: v1.0.0
pub const CAP_COLD_BALANCE_VIEW: u64 = 1 << 5;

/// Bit 6 — Pending cold-sweep view: clients can inspect an in-flight
/// multisig sweep (`amount`, `destination`, `approvals`, `proposed_at`).
/// Introduced: v1.0.0
pub const CAP_PENDING_COLD_SWEEP_VIEW: u64 = 1 << 6;

// Bits 7–63 are reserved for future cold capabilities and are always zero.

/// Bitmask of all cold capabilities exposed by this version.
pub const ALL_CAPABILITIES: u64 = CAP_HOT_COLD_SPLIT
    | CAP_AUTO_REBALANCE
    | CAP_COLD_MULTISIG_SWEEP
    | CAP_SET_HOT_COLD_RATIO
    | CAP_SET_COLD_SIGNERS
    | CAP_COLD_BALANCE_VIEW
    | CAP_PENDING_COLD_SWEEP_VIEW;

/// Return the cold capability bitmap.
///
/// Pure view: ignores `_env` (no storage reads). Authentication is not
/// required. Reserved bits (7–63) are always clear.
pub fn capabilities(_env: &Env) -> u64 {
    ALL_CAPABILITIES
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn capabilities_equals_all_mask() {
        let env = Env::default();
        assert_eq!(capabilities(&env), ALL_CAPABILITIES);
    }

    #[test]
    fn reserved_bits_are_clear() {
        let env = Env::default();
        let caps = capabilities(&env);
        assert_eq!(caps & !((1u64 << 7) - 1), 0);
    }
}
