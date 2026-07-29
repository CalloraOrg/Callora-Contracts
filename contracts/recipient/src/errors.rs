use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the Recipient Registry contract.
///
/// Numeric discriminants are part of the contract interface and must remain
/// stable. Callers and indexers may branch on these `u32` codes instead of
/// parsing panic strings.
#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd, Ord, Eq)]
#[repr(u32)]
pub enum RecipientError {
    /// Contract has not been initialized yet (code 1).
    NotInitialized = 1,
    /// `init` was called more than once (code 2).
    AlreadyInitialized = 2,
    /// Caller is not the configured admin (code 3).
    Unauthorized = 3,
    /// A recipient with the given name is already registered (code 4).
    AlreadyRegistered = 4,
    /// No recipient exists with the given name (code 5).
    NotFound = 5,
    /// The recipient name is empty or exceeds the maximum length (code 6).
    InvalidName = 6,
    /// Arithmetic overflow detected (code 7).
    Overflow = 7,
}
