use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the fee contract.
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
    /// Fee rate exceeds maximum allowed basis points.
    FeeTooHigh = 6,
    /// Insufficient balance to satisfy the request.
    InsufficientBalance = 7,
    /// Recipient address is invalid (contract itself or zero address).
    InvalidRecipient = 8,
}
