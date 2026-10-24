//! Typed error codes for the Callora Admin contract.
//!
//! The numeric discriminants in this enum are part of the contract interface
//! and must remain stable over time. Off-chain indexers and SDK integrators
//! branch on these `u32` codes instead of parsing panic strings.
//!
//! | Code | Variant               | Meaning                                                     |
//! |------|-----------------------|-------------------------------------------------------------|
//! | 1    | NotInitialized        | Admin contract has not been initialized yet                 |
//! | 2    | Unauthorized          | Caller is not authorized for the requested operation        |
//! | 3    | InvalidLimit          | One or more per-account caps exceeds [`MAX_CAP`]            |
//! | 4    | BetsAtCap             | Account's open bet count is already at the configured cap   |
//! | 5    | PositionsAtCap        | Account's open position count is already at the configured cap |
//! | 6    | SubscriptionsAtCap    | Account's active subscription count is at the configured cap |
//! | 7    | CounterUnderflow      | `release_*` was called when the corresponding counter was 0 |
//! | 8    | Overflow              | `checked_add` overflow detected on a counter increment      |
//! | 9    | AlreadyInitialized    | Admin contract has already been initialized                 |
//! | 10   | SameAdmin             | Proposed nominee matches the current admin                  |
//! | 11   | NoPendingAdmin        | No admin transfer is currently pending                      |
//! | 12   | UnauthorizedPending   | Caller is not the pending nominated admin                   |
//! | 13   | NominationExpired     | Admin rotation nomination has expired                       |
//! | 14   | TimelockNotElapsed    | Admin rotation timelock has not elapsed                     |
//!
//! All variants implement [`Copy`] + [`PartialEq`] so they can be returned by
//! value or compared in tests without allocation.

use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the Callora Admin contract.
///
/// Variants 1 through 8 preserve the released discriminants and meanings for
/// the per-account admin limits surface, while variants 9 and above define
/// stable codes for the admin transfer and rotation lifecycle.
#[contracterror]
#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum AdminError {
    /// Admin contract has not been initialized yet (code 1).
    NotInitialized = 1,
    /// Caller is not authorized for the requested operation (code 2).
    Unauthorized = 2,
    /// One or more per-account caps exceeds [`crate::limits::MAX_CAP`] or is
    /// otherwise invalid (code 3).
    InvalidLimit = 3,
    /// Account's open bet count is already at the configured cap (code 4).
    BetsAtCap = 4,
    /// Account's open position count is already at the configured cap (code 5).
    PositionsAtCap = 5,
    /// Account's active subscription count is already at the configured cap (code 6).
    SubscriptionsAtCap = 6,
    /// `release_*` was called when the corresponding counter was already
    /// zero (code 7).
    CounterUnderflow = 7,
    /// `checked_add` overflow detected on a counter increment (code 8).
    Overflow = 8,
    /// Admin contract has already been initialized (code 9).
    AlreadyInitialized = 9,
    /// Proposed nominee matches the current admin (code 10).
    SameAdmin = 10,
    /// No admin transfer is currently pending (code 11).
    NoPendingAdmin = 11,
    /// Caller is not the pending nominated admin (code 12).
    UnauthorizedPending = 12,
    /// Admin rotation nomination has expired (code 13).
    NominationExpired = 13,
    /// Admin rotation timelock has not elapsed (code 14).
    TimelockNotElapsed = 14,
}

/// Backward compatibility alias for the limits module.
pub type AdminLimitError = AdminError;
