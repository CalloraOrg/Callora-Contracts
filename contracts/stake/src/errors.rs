use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the Callora Stake contract.
///
/// The numeric discriminants in this enum are part of the contract interface and
/// must remain stable over time. Callers and indexers may branch on these `u32`
/// codes instead of parsing panic strings.
///
/// | Code | Variant            | Meaning                                               |
/// |------|--------------------|-------------------------------------------------------|
/// | 1    | NotInitialized     | Contract has not been initialized yet                 |
/// | 2    | AlreadyInitialized | `init` was called more than once                      |
/// | 3    | Unauthorized       | Caller is not authorized for this operation           |
/// | 4    | InvalidAmount      | Amount must be strictly positive (`> 0`)              |
/// | 5    | InsufficientStake  | Requested unstake amount exceeds the staker's balance |
/// | 6    | Overflow           | Arithmetic overflow or underflow detected             |
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum StakeError {
    /// Contract has not been initialized yet (code 1).
    NotInitialized = 1,
    /// `init` was called more than once (code 2).
    AlreadyInitialized = 2,
    /// Caller is not authorized for this operation (code 3).
    Unauthorized = 3,
    /// Amount must be strictly positive (`> 0`) (code 4).
    InvalidAmount = 4,
    /// Requested unstake amount exceeds the staker's balance (code 5).
    InsufficientStake = 5,
    /// Arithmetic overflow or underflow detected (code 6).
    Overflow = 6,
}
