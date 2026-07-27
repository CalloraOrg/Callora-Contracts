//! Yield deposit surface for Callora.
//!
//! Protocol yield deposits are implemented by
//! [`callora_revenue_pool::RevenuePool::deposit_yield`]. This crate hosts the
//! cross-contract call safety integration tests under `tests/xcontract.rs`.

#![no_std]

pub use callora_revenue_pool::{RevenuePool, RevenuePoolClient};
