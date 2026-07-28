use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the Callora Freeze contract.
///
/// The numeric discriminants in this enum are part of the contract interface and
/// must remain stable over time. Callers and indexers may branch on these `u32`
/// codes instead of parsing panic strings.
///
/// | Code | Variant            | Meaning                                         |
/// |------|--------------------|-------------------------------------------------|
/// | 1    | NotInitialized     | Contract has not been initialized yet           |
/// | 2    | AlreadyInitialized | `init` was called more than once                |
/// | 3    | Unauthorized       | Caller is not authorized for the operation      |
/// | 4    | AlreadyFrozen      | Contract state is already in frozen status      |
/// | 5    | NotFrozen          | Contract state is not currently frozen          |
/// | 6    | Overflow           | Arithmetic operation overflowed                 |
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FreezeError {
    /// Contract has not been initialized yet (code 1).
    NotInitialized = 1,
    /// Contract has already been initialized (code 2).
    AlreadyInitialized = 2,
    /// Caller is not authorized for this operation (code 3).
    Unauthorized = 3,
    /// Contract is already frozen (code 4).
    AlreadyFrozen = 4,
    /// Contract is not currently frozen (code 5).
    NotFrozen = 5,
    /// Arithmetic overflow detected (code 6).
    Overflow = 6,
}
