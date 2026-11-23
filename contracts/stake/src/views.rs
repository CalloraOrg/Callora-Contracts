use soroban_sdk::Env;

/// Bit 0 — Stake/unstake: users can stake tokens to earn rewards and unstake
/// after a cooldown period.
///
/// Reserved bit assignment. The contract does not yet export this functionality,
/// so the bit is not set in `capabilities()`. Bit positions are never reused.
pub const CAP_STAKE_UNSTAKE: u64 = 1 << 0;

/// Bit 1 — Delegation: users can delegate their stake to a validator or
/// delegatee who validates on their behalf.
///
/// Reserved bit assignment. Bit positions are never reused.
pub const CAP_DELEGATION: u64 = 1 << 1;

/// Bit 2 — Rewards: stakers accrue rewards distributed proportionally to
/// their stake amount and duration.
///
/// Reserved bit assignment. Bit positions are never reused.
pub const CAP_REWARDS: u64 = 1 << 2;

/// Bit 3 — Slashing: validators who misbehave can have their stake slashed,
/// burning a portion as penalty.
///
/// Reserved bit assignment. Bit positions are never reused.
pub const CAP_SLASHING: u64 = 1 << 3;

/// Bit 4 — Withdraw timelock: unstaked amounts are subject to a timelock
/// before they can be withdrawn, providing a security buffer.
///
/// Reserved bit assignment. Bit positions are never reused.
pub const CAP_WITHDRAW_TIMELOCK: u64 = 1 << 4;

/// Bit 5 — Stake view: clients can query current stake balances, total
/// staked, and per-user breakdowns.
///
/// Reserved bit assignment. Bit positions are never reused.
pub const CAP_STAKE_VIEW: u64 = 1 << 5;

// Bits 6–63 reserved for future stake capabilities.

/// Bitmask of all stake capabilities reserved by this version.
///
/// This is the full set of bit assignments, not the set of implemented
/// capabilities. `capabilities()` reports only the bits backed by an exported
/// entrypoint.
pub const SUPPORTED_CAPABILITIES: u64 = CAP_STAKE_UNSTAKE
    | CAP_DELEGATION
    | CAP_REWARDS
    | CAP_SLASHING
    | CAP_WITHDRAW_TIMELOCK
    | CAP_STAKE_VIEW;

/// Bitmask of capabilities currently backed by an exported entrypoint.
///
/// Today the contract exports no stake operations, and `capabilities()` itself
/// is not a stake feature bit, so the honest value is 0.
pub const IMPLEMENTED_CAPABILITIES: u64 = 0;

pub fn capabilities(_env: &Env) -> u64 {
    IMPLEMENTED_CAPABILITIES
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn capabilities_reports_only_implemented() {
        let env = Env::default();
        assert_eq!(capabilities(&env), 0);
    }

    #[test]
    fn capabilities_subset_of_reserved_mask() {
        let env = Env::default();
        let caps = capabilities(&env);
        assert_eq!(caps & !SUPPORTED_CAPABILITIES, 0);
    }

    #[test]
    fn reserved_bits_are_clear() {
        let env = Env::default();
        let caps = capabilities(&env);
        assert_eq!(caps & !((1u64 << 6) - 1), 0);
    }
}
