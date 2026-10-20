//! Yield deposit surface for Callora.
//!
//! Protocol yield deposits are implemented by
//! [`callora_revenue_pool::RevenuePool::deposit_yield`]. This crate hosts the
//! cross-contract call safety integration tests under `tests/xcontract.rs`.
//!
//! # Per-account limits surface (Issue #842 / task b#017)
//!
//! As of issue #842, this crate exposes a per-account state-cap surface that
//! prevents a single account from farming yield-bets, yield-positions, or
//! yield-subscriptions beyond a configurable maximum:
//!
//! - [`limits::CalloraYieldLimits`] — the on-chain Soroban contract that
//!   enforces per-account caps.
//! - [`limits::AccountLimits`] — the caps struct (`max_bets`, `max_positions`,
//!   `max_subscriptions`).
//! - [`limits::AccountState`] — the live counters (`bets`, `positions`,
//!   `subscriptions`).
//! - [`errors::YieldLimitError`] — stable `u32` error codes returned by the
//!   contract.
//!
//! Counter mutations are operator-gated (#1414): the caller must be a
//! registered operator and the cap is enforced on the account named in the
//! call. The surface is otherwise backward compatible with prior versions: the
//! existing `RevenuePool` re-exports remain unchanged and no prior storage key
//! discriminant was reused.

#![no_std]

pub use callora_revenue_pool::{RevenuePool, RevenuePoolClient};

pub mod errors;
pub mod events;
pub mod limits;
pub mod views;

pub use errors::YieldLimitError;
pub use limits::{AccountLimits, AccountState, CalloraYieldLimits, CalloraYieldLimitsClient};

#[cfg(test)]
mod test_limits;
