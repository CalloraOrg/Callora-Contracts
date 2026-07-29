use soroban_sdk::Env;

/// Bit 0 — Stake/unstake: users can stake tokens to earn rewards and unstake
/// after a cooldown period.
pub const CAP_STAKE_UNSTAKE: u64 = 1 << 0;

/// Bit 1 — Delegation: users can delegate their stake to a validator or
/// delegatee who validates on their behalf.
pub const CAP_DELEGATION: u64 = 1 << 1;

/// Bit 2 — Rewards: stakers accrue rewards distributed proportionally to
/// their stake amount and duration.
pub const CAP_REWARDS: u64 = 1 << 2;

/// Bit 3 — Slashing: validators who misbehave can have their stake slashed,
/// burning a portion as penalty.
pub const CAP_SLASHING: u64 = 1 << 3;

/// Bit 4 — Withdraw timelock: unstaked amounts are subject to a timelock
/// before they can be withdrawn, providing a security buffer.
pub const CAP_WITHDRAW_TIMELOCK: u64 = 1 << 4;

/// Bit 5 — Stake view: clients can query current stake balances, total
/// staked, and per-user breakdowns.
pub const CAP_STAKE_VIEW: u64 = 1 << 5;

// Bits 6–63 reserved for future stake capabilities.

/// Bitmask of all stake capabilities supported by this version.
pub const SUPPORTED_CAPABILITIES: u64 = CAP_STAKE_UNSTAKE
    | CAP_DELEGATION
    | CAP_REWARDS
    | CAP_SLASHING
    | CAP_WITHDRAW_TIMELOCK
    | CAP_STAKE_VIEW;

pub fn capabilities(_env: &Env) -> u64 {
    SUPPORTED_CAPABILITIES
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn capabilities_equals_supported_mask() {
        let env = Env::default();
        assert_eq!(capabilities(&env), SUPPORTED_CAPABILITIES);
    }

    #[test]
    fn reserved_bits_are_clear() {
        let env = Env::default();
        let caps = capabilities(&env);
        assert_eq!(caps & !((1u64 << 6) - 1), 0);
    }

    #[test]
    fn capability_flags_are_distinct_single_bits() {
        let flags = [
            CAP_STAKE_UNSTAKE,
            CAP_DELEGATION,
            CAP_REWARDS,
            CAP_SLASHING,
            CAP_WITHDRAW_TIMELOCK,
            CAP_STAKE_VIEW,
        ];

        // Every flag must be a nonzero power of two (exactly one bit set).
        for flag in flags {
            assert_ne!(flag, 0);
            assert_eq!(flag & (flag - 1), 0, "flag {flag:#x} is not a single bit");
        }

        // No two flags may share a bit.
        let mut seen: u64 = 0;
        for flag in flags {
            assert_eq!(seen & flag, 0, "flag {flag:#x} overlaps a previous flag");
            seen |= flag;
        }
    }

    #[test]
    fn supported_capabilities_is_exact_union_of_flags() {
        let union = CAP_STAKE_UNSTAKE
            | CAP_DELEGATION
            | CAP_REWARDS
            | CAP_SLASHING
            | CAP_WITHDRAW_TIMELOCK
            | CAP_STAKE_VIEW;
        assert_eq!(SUPPORTED_CAPABILITIES, union);
        assert_eq!(SUPPORTED_CAPABILITIES.count_ones(), 6);
    }

    #[test]
    fn capabilities_is_deterministic_across_calls() {
        let env = Env::default();
        let first = capabilities(&env);
        let second = capabilities(&env);
        assert_eq!(first, second);
    }
}
