use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the freeze contract.
#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum ContractError {
    NotInitialized = 1,
    AlreadyInitialized = 2,
    Unauthorized = 3,
    AlreadyFrozen = 4,
    NotFrozen = 5,
    InvalidState = 6,
    Overflow = 7,
}
