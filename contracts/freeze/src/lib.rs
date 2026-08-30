//! Freeze (circuit-breaker) harness for Callora contracts.
//!
//! This crate provides the [`CalloraFreeze`] contract that wraps the revenue-pool
//! **pause circuit-breaker** (`freeze` / `unfreeze` / `is_frozen`), which blocks
//! `distribute` and `batch_distribute` while active. Every state-changing
//! entrypoint calls `require_auth` on the acting `Address`.
//!
//! # Auth Model
//!
//! | Entrypoint            | Authorized by                         |
//! |-----------------------|---------------------------------------|
//! | `init`                | `admin.require_auth()`               |
//! | `freeze`              | `caller.require_auth()` — admin or freeze operator |
//! | `unfreeze`            | `caller.require_auth()` — admin only |
//! | `set_freeze_operator` | `caller.require_auth()` — admin only |
//!
//! # Fuzzing
//!
//! See `fuzz/targets/main.rs` — a `cargo-fuzz` target that feeds malformed
//! operation sequences into freeze/unfreeze and asserts safety invariants.
//!
//! See also `tests/malformed_freeze.rs` for manual freeze-scenario tests
//! against the underlying `RevenuePool` contract.

#![no_std]

extern crate alloc;

use alloc::vec::Vec;

use soroban_sdk::{contract, contractimpl, contracttype, Address, Env, Symbol};

pub mod errors;
pub use errors::FreezeError;

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

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Frozen,
    FreezeOperator,
}

// ---------------------------------------------------------------------------
// Contract
// ---------------------------------------------------------------------------

/// Callora circuit-breaker contract.
///
/// Wraps a pause/unpause surface with `require_auth` on every state-changing
/// entrypoint.
#[contract]
pub struct CalloraFreeze;

#[contractimpl]
impl CalloraFreeze {
    /// Initialise the contract with an `admin` address.
    ///
    /// # Arguments
    /// * `admin` — Address authorised to freeze, unfreeze, and set the freeze
    ///   operator.
    ///
    /// # Errors
    /// * [`FreezeError::AlreadyInitialized`] — admin already set.
    pub fn init(env: Env, admin: Address) -> Result<(), FreezeError> {
        admin.require_auth();
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(FreezeError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Frozen, &false);
        Ok(())
    }

    /// Return the stored admin address.
    ///
    /// # Errors
    /// * [`FreezeError::NotInitialized`] — `init` has not been called.
    pub fn get_admin(env: Env) -> Result<Address, FreezeError> {
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(FreezeError::NotInitialized)
    }

    /// Return `true` if the contract is currently frozen.
    pub fn is_frozen(env: Env) -> bool {
        env.storage()
            .instance()
            .get::<_, bool>(&DataKey::Frozen)
            .unwrap_or(false)
    }

    /// Activate the circuit-breaker.
    ///
    /// The admin or the configured freeze operator may call.
    ///
    /// # Arguments
    /// * `caller` — Must be the admin or freeze operator; must authorise.
    /// * `_reason` — Opaque label emitted in the event for off-chain indexers.
    ///
    /// # Errors
    /// * [`FreezeError::Unauthorized`] — caller is neither admin nor operator.
    /// * [`FreezeError::AlreadyFrozen`] — contract is already frozen.
    pub fn freeze(env: Env, caller: Address, _reason: Symbol) -> Result<(), FreezeError> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone())?;
        let operator: Option<Address> = env.storage().instance().get(&DataKey::FreezeOperator);

        let is_authorized = caller == admin || operator.map_or(false, |op| caller == op);
        if !is_authorized {
            return Err(FreezeError::Unauthorized);
        }
        if Self::is_frozen(env.clone()) {
            return Err(FreezeError::AlreadyFrozen);
        }
        env.storage().instance().set(&DataKey::Frozen, &true);
        Ok(())
    }

    /// Deactivate the circuit-breaker. Only the admin may call.
    ///
    /// # Errors
    /// * [`FreezeError::Unauthorized`] — caller is not the admin.
    /// * [`FreezeError::NotFrozen`] — contract is not currently frozen.
    pub fn unfreeze(env: Env, caller: Address) -> Result<(), FreezeError> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone())?;
        if caller != admin {
            return Err(FreezeError::Unauthorized);
        }
        if !Self::is_frozen(env.clone()) {
            return Err(FreezeError::NotFrozen);
        }
        env.storage().instance().set(&DataKey::Frozen, &false);
        Ok(())
    }

    /// Set or replace the freeze operator.
    ///
    /// The operator may call `freeze` but has no authority to `unfreeze`,
    /// set the operator, or exercise any other admin-only power.
    ///
    /// Pass `None` to revoke the operator role.
    ///
    /// # Errors
    /// * [`FreezeError::Unauthorized`] — caller is not the admin.
    pub fn set_freeze_operator(
        env: Env,
        caller: Address,
        operator: Option<Address>,
    ) -> Result<(), FreezeError> {
        caller.require_auth();
        let admin = Self::get_admin(env.clone())?;
        if caller != admin {
            return Err(FreezeError::Unauthorized);
        }
        match operator {
            Some(op) => env.storage().instance().set(&DataKey::FreezeOperator, &op),
            None => env.storage().instance().remove(&DataKey::FreezeOperator),
        }
        Ok(())
    }

    /// Return the configured freeze operator, or `None` if unset.
    pub fn get_freeze_operator(env: Env) -> Option<Address> {
        env.storage().instance().get(&DataKey::FreezeOperator)
    }
}

#[cfg(test)]
mod test;

pub mod ns {
    pub use callora_helpers::{
        accounting_key, config_key, ephemeral_key, idempotency_key, migration_key, state_key,
        ContractNamespace, KeyCategory, KeyOwnershipMarker, NamespacedKey, NamespacedStorage,
        ReadResult,
    };

    pub const CONTRACT_NS: ContractNamespace = ContractNamespace::Freeze;

    #[inline]
    pub fn storage(env: &soroban_sdk::Env) -> NamespacedStorage<'_> {
        NamespacedStorage::new(env, CONTRACT_NS)
    }
}
