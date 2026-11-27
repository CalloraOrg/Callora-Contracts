//! Stable error codes for the emergency pause coordinator.

use soroban_sdk::contracterror;

/// Typed errors returned by [`crate::CalloraEmergency`].
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum EmergencyError {
    /// The coordinator has not been initialised.
    NotInitialized = 1,
    /// `init` was called more than once.
    AlreadyInitialized = 2,
    /// The caller is not the admin (or guardian, where allowed).
    Unauthorized = 3,
    /// The target address is invalid (for example, the coordinator itself).
    InvalidTarget = 4,
    /// The contract is already in the registry.
    AlreadyRegistered = 5,
    /// The contract is not in the registry.
    NotRegistered = 6,
    /// The registry already holds [`crate::MAX_REGISTRY_SIZE`] contracts.
    RegistryFull = 7,
    /// No admin transfer is pending.
    NoPendingAdmin = 8,
    /// The proposed admin is invalid (for example, the current admin).
    InvalidAdmin = 9,
}
