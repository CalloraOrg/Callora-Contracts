use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the Callora Allowlist contract.
///
/// The numeric discriminants in this enum are part of the contract interface and
/// must remain stable over time. Callers may branch on these `u32` codes instead
/// of parsing panic strings.
///
/// | Code | Variant              | Meaning                                              |
/// |------|----------------------|------------------------------------------------------|
/// | 1    | NotInitialized       | Contract has not been initialized                    |
/// | 2    | AlreadyInitialized   | `init` was called more than once                     |
/// | 3    | Unauthorized         | Caller is not authorized for the operation           |
/// | 4    | AlreadyPresent       | Address is already on the allowlist                  |
/// | 5    | NotPresent           | Address is not on the allowlist                      |
/// | 6    | InvalidPage          | Page size is outside `1 ..= MAX_PAGE_SIZE`           |
/// | 7    | Overflow             | Index or count arithmetic overflowed                 |
/// | 8    | NoPendingAdmin       | No admin transfer is pending                         |
#[contracterror]
#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum AllowlistError {
    /// Contract has not been initialized yet (code 1).
    NotInitialized = 1,
    /// Contract has already been initialized (code 2).
    AlreadyInitialized = 2,
    /// Caller is not authorized for this operation (code 3).
    Unauthorized = 3,
    /// Address is already on the allowlist (code 4).
    AlreadyPresent = 4,
    /// Address is not on the allowlist (code 5).
    NotPresent = 5,
    /// Requested page size is zero or exceeds `MAX_PAGE_SIZE` (code 6).
    InvalidPage = 6,
    /// Index or count arithmetic overflowed (code 7).
    Overflow = 7,
    /// No admin transfer is pending (code 8).
    NoPendingAdmin = 8,
}
