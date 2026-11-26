//! Typed error codes for the Callora Yield per-account limits surface.
//!
//! The numeric discriminants in this enum are part of the contract interface
//! and must remain stable over time. Off-chain indexers and SDK integrators
//! branch on these `u32` codes instead of parsing panic strings.
//!
//! | Code | Variant               | Meaning                                                       |
//! |------|-----------------------|---------------------------------------------------------------|
//! | 1    | NotInitialized        | Contract has not been initialized yet                         |
//! | 2    | AlreadyInitialized    | `init` was called more than once                              |
//! | 3    | Unauthorized          | Caller is not authorized for the requested operation          |
//! | 4    | InvalidLimit          | One or more per-account caps are negative or otherwise invalid|
//! | 5    | BetsAtCap             | Account's open bet count is already at the configured cap     |
//! | 6    | PositionsAtCap        | Account's open position count is already at the configured cap|
//! | 7    | SubscriptionsAtCap    | Account's active subscription count is at the configured cap |
//! | 8    | CounterUnderflow      | `clear_*` was called when the corresponding counter was zero  |
//! | 9    | Overflow              | `checked_add` overflow detected on a counter increment        |
//! | 10   | UpgradeRejected       | Storage-migration validator rejected the pending upgrade      |
//! | 11   | EmptyBatch            | `batch_clear_accounts` was called with no accounts            |
//! | 12   | BatchTooLarge         | `batch_clear_accounts` exceeded the batch size cap            |
//! | 13   | BetsWindowCap         | Per-window bet cap reached                                    |
//! | 14   | PositionsWindowCap    | Per-window position cap reached                               |
//! | 15   | SubscriptionsWindowCap| Per-window subscription cap reached                          |
//!
//! All variants implement [`Copy`] + [`PartialEq`] so they can be returned by
//! value or pinned in arrays without allocation.

use soroban_sdk::contracterror;

/// Stable, machine-readable error codes for the per-account yield limits
/// surface.
#[contracterror]
#[repr(u32)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub enum YieldLimitError {
    /// Contract has not been initialized yet (code 1).
    NotInitialized = 1,
    /// Contract has already been initialized (code 2).
    AlreadyInitialized = 2,
    /// Caller is not authorized for the requested operation (code 3).
    Unauthorized = 3,
    /// One or more per-account or per-window caps are invalid (code 4).
    ///
    /// Returned by [`crate::limits::set_account_limits`],
    /// [`crate::limits::set_default_limits`], and
    /// [`crate::limits::set_window_limits`] when any cap is negative or above
    /// `MAX_CAP`, and by [`crate::limits::set_window_ledgers`] when the window
    /// length is neither `0` nor within the documented bounds.
    InvalidLimit = 4,
    /// Account's open bet count is already at the configured cap (code 5).
    ///
    /// Returned by [`crate::limits::place_bet`] when an increment would push
    /// the account's open bet counter above `max_bets`.
    BetsAtCap = 5,
    /// Account's open position count is already at the configured cap (code 6).
    ///
    /// Returned by [`crate::limits::open_position`] when an increment would
    /// push the account's open position counter above `max_positions`.
    PositionsAtCap = 6,
    /// Account's active subscription count is already at the configured cap (code 7).
    ///
    /// Returned by [`crate::limits::subscribe`] when an increment would push
    /// the active subscription counter above `max_subscriptions`.
    SubscriptionsAtCap = 7,
    /// `clear_*` was called when the corresponding counter was already zero (code 8).
    CounterUnderflow = 8,
    /// `checked_add` overflow detected on a counter increment (code 9).
    ///
    /// Returned in the (effectively unreachable) case that the per-account
    /// counter arithmetic saturates `u32::MAX` — surfaces a stable error
    /// rather than panicking so callers cannot rely on undefined behaviour.
    Overflow = 9,
    /// The storage-migration validator rejected the pending upgrade (code 10).
    ///
    /// Returned by [`crate::limits::CalloraYieldLimits::upgrade`] when the
    /// pre-upgrade guard rejects an all-zero WASM hash, a version skip, an
    /// unsanctioned rollback, or a silent storage-layout change.
    UpgradeRejected = 10,
    /// `batch_clear_accounts` was called with no accounts (code 11).
    EmptyBatch = 11,
    /// `batch_clear_accounts` exceeded [`crate::limits::MAX_BATCH_CLEAR`] (code 12).
    BatchTooLarge = 12,
    /// `place_bet` would exceed the account's per-window bet cap (code 13).
    ///
    /// Returned by [`crate::limits::place_bet`] when rolling windows are
    /// enabled and the account has already opened `max_bets` bets in the
    /// current window.
    BetsWindowCap = 13,
    /// `open_position` would exceed the account's per-window position cap (code 14).
    PositionsWindowCap = 14,
    /// `subscribe` would exceed the account's per-window subscription cap (code 15).
    SubscriptionsWindowCap = 15,
}
