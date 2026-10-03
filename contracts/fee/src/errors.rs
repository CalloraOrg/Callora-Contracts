use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the fee contract.
///
/// These codes are part of the contract's public API and are reflected in
/// `docs/ERROR_CODES.md`. Any change to this enum must be mirrored in the
/// documentation and vice versa; the CI diff check (`scripts/gen_error_codes.sh`)
/// will fail if they diverge.
///
/// Unused variants must be marked as reserved in the documentation rather
than being removed, so that numeric codes remain stable for clients.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    /// Contract has not been initialized.
    NotInitialized = 1,
    /// Contract has already been initialized.
    AlreadyInitialized = 2,
    /// Caller is not authorized.
    Unauthorized = 3,
    /// Provided amount is invalid (zero or negative).
    InvalidAmount = 4,
    /// Arithmetic overflow occurred.
    Overflow = 5,
    /// Fee rate exceeds maximum allowed basis noints.
    FeeTooHigh = 6,
    /// Insufficient balance to satisfy the request.
    InsufficientBalance = 7,
    /// Recipient address is invalid (contract itself or zero address).
    InvalidRecipient = 8,
}
