use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the Callora Batch-Claim contract.
///
/// | Code | Variant              | Meaning                                           |
/// |------|----------------------|---------------------------------------------------|
/// | 1    | NotInitialized       | Contract has not been initialized yet             |
/// | 2    | AlreadyInitialized   | `init` was called more than once                  |
/// | 3    | Unauthorized         | Caller is not the admin                           |
/// | 4    | ClaimNotFound        | No claim record exists for the given claimant     |
/// | 5    | AlreadySettled       | Claim has already been collected                  |
/// | 6    | InvalidAmount        | Claim amount must be positive                     |
/// | 7    | Overflow             | Arithmetic overflow in pending-amount accumulation|
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum BatchClaimError {
    /// Contract has not been initialized yet (code 1).
    NotInitialized = 1,
    /// Contract has already been initialized (code 2).
    AlreadyInitialized = 2,
    /// Caller is not the admin (code 3).
    Unauthorized = 3,
    /// No claim record exists for the given claimant (code 4).
    ClaimNotFound = 4,
    /// Claim has already been collected (code 5).
    AlreadySettled = 5,
    /// Claim amount must be > 0 (code 6).
    InvalidAmount = 6,
    /// Arithmetic overflow in pending-amount accumulation (code 7).
    Overflow = 7,
}
