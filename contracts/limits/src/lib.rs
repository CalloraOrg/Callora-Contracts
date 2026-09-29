#![no_std]
//! Limits auth-surface registry for Callora contracts.
//!
//! # Purpose
//! This crate does **not** re-implement limit math. It documents the
//! cross-contract **limits** entrypoint set so
//! [`tests/auth_snap.rs`](../tests/auth_snap.rs) can snapshot which calls
//! require `require_auth` and catch regressions via CI diffs.
//!
//! # State-changing entrypoints (must require auth)
//!
//! | Contract | Entrypoint | Acting address |
//! |----------|------------|----------------|
//! | Settlement | `set_developer_min_balance` | admin (`caller`) |
//! | Settlement | `set_minimum_balance` | admin (`caller`) — alias path into `limits` |
//! | Settlement | `set_daily_withdraw_cap` | admin (`caller`) |
//! | Revenue pool | `set_max_distribute` | admin (`caller`) |
//! | Vault | `set_reserve_cap` | owner (`caller`) — covered when vault builds |
//!
//! # Read-only entrypoints (must NOT require auth)
//!
//! | Contract | Entrypoint |
//! |----------|------------|
//! | Settlement | `get_developer_min_balance` |
//! | Settlement | `get_minimum_balance` |
//! | Settlement | `get_daily_withdraw_cap` |
//! | Settlement | `get_withdrawal_today` |
//! | Revenue pool | `get_max_distribute` |
//! | Vault | `get_reserve_cap` — covered when vault builds |
