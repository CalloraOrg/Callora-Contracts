use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the Limits contract.
///
/// The numeric discriminants in this enum are part of the contract interface and
/// must remain stable over time. Callers and indexers may branch on these `u32`
/// codes instead of parsing panic strings.
///
/// | Code | Variant             | Meaning                                              |
/// |------|---------------------|------------------------------------------------------|
/// | 1    | NotInitialized      | Contract has not been initialized yet                |
/// | 2    | AlreadyInitialized  | `init` was called more than once                     |
/// | 3    | Unauthorized        | Caller is not authorized for this operation          |
/// | 4    | AmountNegative      | A limit or amount must not be negative               |
/// | 5    | InvalidLimit        | `max` is set below `min` for the same token          |
/// | 6    | BelowMinimum        | Amount is below the configured per-token minimum     |
/// | 7    | AboveMaximum        | Amount exceeds the configured per-token maximum      |
/// | 8    | Overflow            | Arithmetic overflow detected                         |
#[contracterror]
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u32)]
pub enum LimitsError {
    /// Contract has not been initialized yet (code 1).
    NotInitialized = 1,
    /// `init` was called more than once (code 2).
    AlreadyInitialized = 2,
    /// Caller is not authorized for this operation (code 3).
    Unauthorized = 3,
    /// A limit or amount must not be negative (code 4).
    AmountNegative = 4,
    /// `max` is set below `min` for the same token (code 5).
    InvalidLimit = 5,
    /// Amount is below the configured per-token minimum (code 6).
    BelowMinimum = 6,
    /// Amount exceeds the configured per-token maximum (code 7).
    AboveMaximum = 7,
    /// Arithmetic overflow detected (code 8).
    Overflow = 8,
}
