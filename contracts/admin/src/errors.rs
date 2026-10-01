//! Typed error codes for the Callora Admin per-account limits surface.
//!
//! The numeric discriminants in this enum are part of the contract interface
//! and must remain stable over time. Off-chain indexers and SDK integrators
//! branch on these `u32` codes instead of parsing panic strings.
//!
//! | Code | Variant            | Meaning                                                     |
//! |------|--------------------|-------------------------------------------------------------|
//! | 1    | NotInitialized     | Admin contract has not been initialized yet                 |
//! | 2    | Unauthorized       | Caller is not authorized for the requested operation        |
//! | 3    | InvalidLimit       | One or more per-account caps exceeds [`MAX_CAP`]            |
//! | 4    | BetsAtCap          | Account's open bet count is already at the configured cap   |
//! | 5    | PositionsAtCap     | Account's open position count is already at the configured cap |
//! | 6    | SubscriptionsAtCap | Account's active subscription count is at the configured cap |
//! | 7    | CounterUnderflow   | `release_*` was called when the corresponding counter was 0 |
//! | 8    | Overflow           | `checked_add` overflow detected on a counter increment      |
//!
//! All variants implement [`Copy`] + [`PartialEq`] so they can be returned by
//! value or compared in tests without allocation.

use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the per-account admin limits
/// surface.
///
/// Every public function in [`crate::limits`] that can fail returns one of
/// these variants instead of panicking, giving callers a deterministic
/// branch target.
#[contracterror]
#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum AdminLimitError {
    /// Admin contract has not been initialized yet (code 1).
    NotInitialized = 1,
    /// Caller is not authorized for the requested operation (code 2).
    Unauthorized = 2,
    /// One or more per-account caps exceeds [`crate::limits::MAX_CAP`] or is
    /// otherwise invalid (code 3).
    InvalidLimit = 3,
    /// Account's open bet count is already at the configured cap (code 4).
    ///
    /// Returned by [`crate::limits::consume_bet`] when an increment would
    /// push the account's bet counter above `max_bets`.
    BetsAtCap = 4,
    /// Account's open position count is already at the configured cap (code 5).
    ///
    /// Returned by [`crate::limits::consume_position`] when an increment
    /// would push the account's position counter above `max_positions`.
    PositionsAtCap = 5,
    /// Account's active subscription count is already at the configured cap (code 6).
    ///
    /// Returned by [`crate::limits::consume_subscription`] when an increment
    /// would push the account's subscription counter above
    /// `max_subscriptions`.
    SubscriptionsAtCap = 6,
    /// `release_*` was called when the corresponding counter was already
    /// zero (code 7).
    CounterUnderflow = 7,
    /// `checked_add` overflow detected on a counter increment (code 8).
    ///
    /// Returned in the (effectively unreachable) case that the per-account
    /// counter arithmetic saturates `u32::MAX` — surfaces a stable error
    /// rather than panicking so callers can handle the edge case gracefully.
    Overflow = 8,
}
