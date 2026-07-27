//! Freeze (circuit-breaker) harness for Callora contracts.
//!
//! # What “freeze” means here
//! There is no standalone `freeze` WASM today. Freeze maps to the revenue-pool
//! **pause circuit-breaker** (`pause` / `unpause` / `is_paused`), which blocks
//! `distribute` and `batch_distribute` while active. Vault exposes an analogous
//! pause surface; this crate targets the compiling revenue-pool entrypoints.
//!
//! # Fuzzing
//! See `fuzz/targets/main.rs` — a `cargo-fuzz` target that feeds malformed
//! operation sequences into freeze/unfreeze and asserts safety invariants.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

pub mod errors;
pub use errors::ContractError;

/// One step in a freeze/unfreeze fuzz sequence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreezeOp {
    /// Attempt pause as admin.
    FreezeAsAdmin,
    /// Attempt pause as configured guardian.
    FreezeAsGuardian,
    /// Attempt pause as an unauthorized outsider.
    FreezeAsOutsider,
    /// Attempt unpause as admin.
    UnfreezeAsAdmin,
    /// Attempt unfreeze as outsider (must fail).
    UnfreezeAsOutsider,
    /// Attempt distribute while possibly frozen (malformed amount allowed).
    Distribute { amount: i128 },
    /// Toggle / clear guardian mid-sequence.
    SetGuardian,
    /// Clear guardian.
    ClearGuardian,
}

impl FreezeOp {
    /// Decode a raw fuzzer byte into an operation (covers all variants).
    pub fn from_byte(b: u8, amount_lo: u8, amount_hi: u8) -> Self {
        let amount = i128::from(u16::from_be_bytes([amount_lo, amount_hi]));
        match b % 8 {
            0 => Self::FreezeAsAdmin,
            1 => Self::FreezeAsGuardian,
            2 => Self::FreezeAsOutsider,
            3 => Self::UnfreezeAsAdmin,
            4 => Self::UnfreezeAsOutsider,
            5 => Self::Distribute { amount },
            6 => Self::SetGuardian,
            _ => Self::ClearGuardian,
        }
    }

    /// Decode a byte slice into a bounded operation list.
    pub fn decode_sequence(data: &[u8], max_ops: usize) -> Vec<Self> {
        let mut ops = Vec::new();
        let mut i = 0;
        while i < data.len() && ops.len() < max_ops {
            let b = data[i];
            let lo = data.get(i + 1).copied().unwrap_or(0);
            let hi = data.get(i + 2).copied().unwrap_or(0);
            ops.push(Self::from_byte(b, lo, hi));
            i = i.saturating_add(3);
            if i == 0 {
                break;
            }
        }
        ops
    }
}

/// Maximum operations executed per fuzz / unit-test invocation.
pub const MAX_FREEZE_OPS: usize = 64;
