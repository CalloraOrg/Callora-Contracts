//! Thin re-export of the settlement contract migration API.
//!
//! This crate exists solely so the cargo-fuzz target at
//! `contracts/migrate/fuzz/` can depend on a dedicated crate without pulling in
//! the full settlement contract as a `cdylib`.

#![no_std]

pub use callora_settlement::migrate;
pub use callora_settlement::{
    CalloraSettlement, CalloraSettlementClient, SettlementError, StorageKey, MAX_BATCH_SIZE,
};
