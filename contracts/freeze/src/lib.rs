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
//! # Events
//!
//! Every state-changing entrypoint emits exactly one event. Topic[1] is always
//! `"callora_v1"` for version filtering.
//!
//! | Event topic            | Entrypoint            | Data                                         |
//! |------------------------|-----------------------|----------------------------------------------|
//! | `freeze_initialized`   | `init`                | `()`                                         |
//! | `freeze_set`           | `freeze` / `freeze_until` | `FreezeSetEvent { reason, frozen_at, freeze_until }` |
//! | `freeze_cleared`       | `unfreeze`            | `()`                                         |
//! | `freeze_operator_set`  | `set_freeze_operator` | `FreezeOperatorSetEvent { old, new }`        |
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
pub mod events;

pub use errors::FreezeError;
pub use events::{FreezeOperatorSetEvent, FreezeSetEvent};

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
// TTL policy
// ---------------------------------------------------------------------------

/// Approximate number of ledgers per day on Stellar (5 seconds per ledger).
pub const LEDGERS_PER_DAY: u32 = 17_280;

/// Threshold at which instance storage TTL is extended (~30 days).
///
/// All freeze state lives in instance storage, so the entrypoint-level
/// [`CalloraFreeze::bump_instance`] helper keeps the instance alive.
pub const INSTANCE_BUMP_THRESHOLD: u32 = LEDGERS_PER_DAY * 30;

/// Target TTL when instance storage is extended (~60 days).
pub const INSTANCE_BUMP_AMOUNT: u32 = LEDGERS_PER_DAY * 60;

// ---------------------------------------------------------------------------
// Storage keys
// ---------------------------------------------------------------------------

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    Frozen,
    FreezeOperator,
    /// The `Symbol` reason supplied to the last successful `freeze()` call.
    FreezeReason,
    /// The ledger timestamp (`env.ledger().timestamp()`) of the last successful
    /// `freeze()` call.
    FreezeTimestamp,
    /// Optional ledger timestamp at which a timed freeze (`freeze_until`)
    /// lapses automatically. Absent for indefinite freezes.
    FreezeUntil,
}

// ---------------------------------------------------------------------------
// View types
// ---------------------------------------------------------------------------

/// Snapshot returned by [`CalloraFreeze::get_freeze_status`].
///
/// Combines the current frozen flag, the persisted reason, and the timestamp
/// into a single call, avoiding multiple round-trips for off-chain monitors.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FreezeStatus {
    /// `true` when the circuit-breaker is active.
    pub frozen: bool,
    /// The reason label from the last `freeze()` call, or `None` if the
    /// contract was never frozen or has been unfrozen and the field cleared.
    pub reason: Option<Symbol>,
    /// The ledger timestamp of the last `freeze()` call, or `None` if the
    /// contract was never frozen or has been unfrozen and the field cleared.
    pub frozen_at: Option<u64>,
    /// The ledger timestamp at which a timed freeze lapses, or `None` for an
    /// indefinite freeze (or before the first freeze / after `unfreeze`).
    ///
    /// When `Some(t)`, the freeze is active only while
    /// `env.ledger().timestamp() < t`; at `now >= t` the freeze has lapsed and
    /// `frozen` reports `false` even though the stored flag is still set.
    /// This field is additive: indexers that ignore unknown fields are
    /// unaffected, but deserializers expecting the old three-field layout
    /// must be updated.
    pub until: Option<u64>,
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
    /// Extend the instance storage TTL.
    ///
    /// All freeze state (admin, frozen flag, operator, reason, timestamp)
    /// lives in instance storage. Calling this on every entrypoint — including
    /// read-only views — keeps the instance from archiving due to inactivity,
    /// mirroring the vault/escrow convention.
    fn bump_instance(env: &Env) {
        env.storage()
            .instance()
            .extend_ttl(INSTANCE_BUMP_THRESHOLD, INSTANCE_BUMP_AMOUNT);
    }

    /// Initialise the contract with an `admin` address.
    ///
    /// Emits a `freeze_initialized` event on success.
    ///
    /// # Arguments
    /// * `admin` — Address authorised to freeze, unfreeze, and set the freeze
    ///   operator.
    ///
    /// # Errors
    /// * [`FreezeError::AlreadyInitialized`] — admin already set.
    pub fn init(env: Env, admin: Address) -> Result<(), FreezeError> {
        admin.require_auth();
        Self::bump_instance(&env);
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(FreezeError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::Frozen, &false);
        events::emit_freeze_initialized(&env, &admin);
        Ok(())
    }

    /// Return the stored admin address.
    ///
    /// # Errors
    /// * [`FreezeError::NotInitialized`] — `init` has not been called.
    pub fn get_admin(env: Env) -> Result<Address, FreezeError> {
        Self::bump_instance(&env);
        env.storage()
            .instance()
            .get(&DataKey::Admin)
            .ok_or(FreezeError::NotInitialized)
    }

    /// Return `true` if the contract is currently frozen.
    ///
    /// A timed freeze (`freeze_until`) reports `false` once
    /// `env.ledger().timestamp() >= until`, without requiring a state write.
    pub fn is_frozen(env: Env) -> bool {
        Self::bump_instance(&env);
        Self::frozen_active(&env)
    }

    /// Read the persisted frozen flag and apply the timed-freeze lapse rule.
    ///
    /// Returns `false` when the flag is unset, or when a `FreezeUntil`
    /// deadline exists and `now >= until`. No storage writes are performed,
    /// so a lapsed freeze remains "frozen" in storage until the next
    /// `freeze` / `unfreeze` overwrites or clears it.
    fn frozen_active(env: &Env) -> bool {
        let frozen = env
            .storage()
            .instance()
            .get::<_, bool>(&DataKey::Frozen)
            .unwrap_or(false);
        if !frozen {
            return false;
        }
        match env.storage().instance().get::<_, u64>(&DataKey::FreezeUntil) {
            Some(until) => env.ledger().timestamp() < until,
            None => true,
        }
    }

    /// Return a snapshot of the current freeze state, last reason, and
    /// timestamp in one call.
    ///
    /// `reason` and `frozen_at` are populated while the contract is frozen and
    /// cleared on `unfreeze`. They are both `None` before the first `freeze`
    /// and after any subsequent `unfreeze`.
    ///
    /// # Errors
    /// * [`FreezeError::NotInitialized`] — `init` has not been called.
    pub fn get_freeze_status(env: Env) -> Result<FreezeStatus, FreezeError> {
        Self::bump_instance(&env);
        if !env.storage().instance().has(&DataKey::Admin) {
            return Err(FreezeError::NotInitialized);
        }
        let frozen = Self::frozen_active(&env);
        let reason: Option<Symbol> = env.storage().instance().get(&DataKey::FreezeReason);
        let frozen_at: Option<u64> = env.storage().instance().get(&DataKey::FreezeTimestamp);
        let until: Option<u64> = env.storage().instance().get(&DataKey::FreezeUntil);
        Ok(FreezeStatus {
            frozen,
            reason,
            frozen_at,
            until,
        })
    }

    /// Activate the circuit-breaker.
    ///
    /// Persists `reason` and the current ledger timestamp, then emits a
    /// `freeze_set` event.
    ///
    /// The admin or the configured freeze operator may call.
    ///
    /// # Arguments
    /// * `caller` — Must be the admin or freeze operator; must authorise.
    /// * `reason` — Opaque label persisted in storage and emitted in the event
    ///   for off-chain indexers and the `get_freeze_status` view.
    ///
    /// # Errors
    /// * [`FreezeError::Unauthorized`] — caller is neither admin nor operator.
    /// * [`FreezeError::AlreadyFrozen`] — contract is already frozen.
    pub fn freeze(env: Env, caller: Address, reason: Symbol) -> Result<(), FreezeError> {
        Self::do_freeze(env, caller, reason, None)
    }

    /// Activate the circuit-breaker for a limited time.
    ///
    /// Identical to [`CalloraFreeze::freeze`] except the freeze lapses
    /// automatically once `env.ledger().timestamp() >= until`: `is_frozen`
    /// and `get_freeze_status` then report the contract as unfrozen with no
    /// admin action and no state write required. A lapsed freeze does not
    /// block a subsequent `freeze` / `freeze_until` call.
    ///
    /// `unfreeze` on a lapsed freeze returns [`FreezeError::NotFrozen`],
    /// since the circuit-breaker is already inactive; the stale stored flag
    /// is simply overwritten by the next successful `freeze`.
    ///
    /// # Arguments
    /// * `caller` — Must be the admin or freeze operator; must authorise.
    /// * `reason` — Opaque label persisted in storage and emitted in the event.
    /// * `until` — Ledger timestamp deadline; must be strictly greater than
    ///   `env.ledger().timestamp()`.
    ///
    /// # Errors
    /// * [`FreezeError::Unauthorized`] — caller is neither admin nor operator.
    /// * [`FreezeError::AlreadyFrozen`] — contract is already frozen.
    /// * [`FreezeError::FreezeUntilNotInFuture`] — `until <= now`.
    pub fn freeze_until(
        env: Env,
        caller: Address,
        reason: Symbol,
        until: u64,
    ) -> Result<(), FreezeError> {
        Self::do_freeze(env, caller, reason, Some(until))
    }

    /// Shared implementation for `freeze` (indefinite) and `freeze_until`
    /// (timed). `until = None` clears any previously stored deadline.
    fn do_freeze(
        env: Env,
        caller: Address,
        reason: Symbol,
        until: Option<u64>,
    ) -> Result<(), FreezeError> {
        caller.require_auth();
        Self::bump_instance(&env);
        let admin = Self::get_admin(env.clone())?;
        let operator: Option<Address> = env.storage().instance().get(&DataKey::FreezeOperator);

        let is_authorized = caller == admin || operator.map_or(false, |op| caller == op);
        if !is_authorized {
            return Err(FreezeError::Unauthorized);
        }
        if let Some(deadline) = until {
            if deadline <= env.ledger().timestamp() {
                return Err(FreezeError::FreezeUntilNotInFuture);
            }
        }
        if Self::frozen_active(&env) {
            return Err(FreezeError::AlreadyFrozen);
        }

        let frozen_at = env.ledger().timestamp();
        env.storage().instance().set(&DataKey::Frozen, &true);
        env.storage()
            .instance()
            .set(&DataKey::FreezeReason, &reason);
        env.storage()
            .instance()
            .set(&DataKey::FreezeTimestamp, &frozen_at);
        match until {
            Some(deadline) => env
                .storage()
                .instance()
                .set(&DataKey::FreezeUntil, &deadline),
            None => env.storage().instance().remove(&DataKey::FreezeUntil),
        }

        events::emit_freeze_set(&env, &caller, reason, frozen_at, until);
        Ok(())
    }

    /// Deactivate the circuit-breaker. Only the admin may call.
    ///
    /// Clears the persisted reason and timestamp, then emits a `freeze_cleared`
    /// event.
    ///
    /// # Errors
    /// * [`FreezeError::Unauthorized`] — caller is not the admin.
    /// * [`FreezeError::NotFrozen`] — contract is not currently frozen.
    pub fn unfreeze(env: Env, caller: Address) -> Result<(), FreezeError> {
        caller.require_auth();
        Self::bump_instance(&env);
        let admin = Self::get_admin(env.clone())?;
        if caller != admin {
            return Err(FreezeError::Unauthorized);
        }
        if !Self::frozen_active(&env) {
            return Err(FreezeError::NotFrozen);
        }
        env.storage().instance().set(&DataKey::Frozen, &false);
        env.storage().instance().remove(&DataKey::FreezeReason);
        env.storage().instance().remove(&DataKey::FreezeTimestamp);
        env.storage().instance().remove(&DataKey::FreezeUntil);

        events::emit_freeze_cleared(&env, &caller);
        Ok(())
    }

    /// Set or replace the freeze operator.
    ///
    /// The operator may call `freeze` but has no authority to `unfreeze`,
    /// set the operator, or exercise any other admin-only power.
    ///
    /// Pass `None` to revoke the operator role.
    ///
    /// Emits a `freeze_operator_set` event with the previous and new operator
    /// addresses (either may be `None`).
    ///
    /// # Errors
    /// * [`FreezeError::Unauthorized`] — caller is not the admin.
    pub fn set_freeze_operator(
        env: Env,
        caller: Address,
        operator: Option<Address>,
    ) -> Result<(), FreezeError> {
        caller.require_auth();
        Self::bump_instance(&env);
        let admin = Self::get_admin(env.clone())?;
        if caller != admin {
            return Err(FreezeError::Unauthorized);
        }

        let old_operator: Option<Address> = env.storage().instance().get(&DataKey::FreezeOperator);

        match operator.clone() {
            Some(ref op) => env.storage().instance().set(&DataKey::FreezeOperator, op),
            None => env.storage().instance().remove(&DataKey::FreezeOperator),
        }

        events::emit_freeze_operator_set(&env, &caller, old_operator, operator);
        Ok(())
    }

    /// Return the configured freeze operator, or `None` if unset.
    pub fn get_freeze_operator(env: Env) -> Option<Address> {
        Self::bump_instance(&env);
        env.storage().instance().get(&DataKey::FreezeOperator)
    }
}

#[cfg(test)]
mod test;
mod test;
