//! Admin cool-off (cooldown) module for the `escrow` contract.
//!
//! # Purpose
//!
//! The `escrow` contract exposes a set of *critical* admin actions (releasing
//! funds, pausing, rotating signers, …). Left unguarded, a compromised or
//! careless admin key could fire these actions back-to-back and inflict damage
//! far faster than an off-chain monitor could react.
//!
//! This module enforces a per-action **cool-off window**: after a critical
//! action of a given kind runs, another action of the *same kind* is rejected
//! with [`EscrowError::CooldownActive`] until the window elapses. The window
//! is measured against the ledger timestamp (wall-clock seconds), so it is
//! independent of block cadence.
//!
//! # Design
//!
//! * **Per-action tracking.** Each critical action is identified by a short
//!   [`Symbol`] tag (e.g. `"release"`, `"pause"`, `"rotate"`). The
//!   last-execution timestamp is stored per tag under
//!   [`StorageKey::LastAction`], so cooling one action never blocks an
//!   unrelated one.
//! * **Configurable window.** A single global cooldown (in seconds) is held in
//!   instance storage and bounded by
//!   [`MIN_COOLDOWN_SECS`]..=[`MAX_COOLDOWN_SECS`].
//! * **Overflow-safe.** All arithmetic uses checked/saturating operations;
//!   there are no `unwrap()` calls on production paths.
//! * **No auth here.** Authentication (`require_auth`) and authorization
//!   (admin check) live in the contract entrypoints in `lib.rs`. This module
//!   is pure cool-off bookkeeping so it can be unit-tested in isolation and
//!   reused.

use crate::errors::EscrowError;
use crate::StorageKey;
use soroban_sdk::{Env, Symbol};

// ---------------------------------------------------------------------------
// Cooldown bounds
// ---------------------------------------------------------------------------

/// Minimum accepted cooldown window, in seconds (1 second).
///
/// A zero window would defeat the purpose of the guard, so the smallest
/// meaningful value is one second.
pub const MIN_COOLDOWN_SECS: u64 = 1;

/// Maximum accepted cooldown window, in seconds (30 days).
///
/// Caps the window so a mistaken configuration cannot lock critical actions
/// out for an unreasonable length of time.
pub const MAX_COOLDOWN_SECS: u64 = 30 * 24 * 60 * 60;

/// Default cooldown window applied at `init`, in seconds (1 hour).
pub const DEFAULT_COOLDOWN_SECS: u64 = 60 * 60;

// ---------------------------------------------------------------------------
// Cooldown configuration
// ---------------------------------------------------------------------------

/// Read the currently-configured cool-off window, in seconds.
///
/// Falls back to [`DEFAULT_COOLDOWN_SECS`] when the contract has been
/// initialized but no explicit value was ever written (defensive default).
pub fn get_cooldown(env: &Env) -> u64 {
    env.storage()
        .instance()
        .get(&StorageKey::Cooldown)
        .unwrap_or(DEFAULT_COOLDOWN_SECS)
}

/// Validate and persist a new cool-off window.
///
/// # Errors
/// * [`EscrowError::InvalidCooldown`] -- `secs` is outside
///   [`MIN_COOLDOWN_SECS`]..=[`MAX_COOLDOWN_SECS`].
pub fn set_cooldown(env: &Env, secs: u64) -> Result<(), EscrowError> {
    if !(MIN_COOLDOWN_SECS..=MAX_COOLDOWN_SECS).contains(&secs) {
        return Err(EscrowError::InvalidCooldown);
    }
    env.storage().instance().set(&StorageKey::Cooldown, &secs);
    Ok(())
}

// ---------------------------------------------------------------------------
// Per-action cool-off bookkeeping
// ---------------------------------------------------------------------------

/// Return the ledger timestamp (seconds) at which the action tagged `action`
/// last ran, or `None` if it has never run.
pub fn last_action_ts(env: &Env, action: &Symbol) -> Option<u64> {
    env.storage()
        .instance()
        .get(&StorageKey::LastAction(action.clone()))
}

/// Return the earliest ledger timestamp at which the action tagged `action`
/// may next run.
///
/// This is `last_run + cooldown`, computed with saturating arithmetic so a
/// pathological configuration can never wrap. Returns `0` when the action has
/// never run (i.e. it is immediately available).
pub fn ready_at(env: &Env, action: &Symbol) -> u64 {
    match last_action_ts(env, action) {
        Some(last) => last.saturating_add(get_cooldown(env)),
        None => 0,
    }
}

/// Return the number of seconds remaining before the action tagged `action`
/// may run again. Returns `0` when the action is available now.
pub fn remaining(env: &Env, action: &Symbol) -> u64 {
    let now = env.ledger().timestamp();
    ready_at(env, action).saturating_sub(now)
}

/// Return `true` when the action tagged `action` is currently outside its
/// cool-off window (i.e. it may run now).
pub fn is_ready(env: &Env, action: &Symbol) -> bool {
    remaining(env, action) == 0
}

/// Enforce and then arm the cool-off window for a critical action.
///
/// On success the action's last-execution timestamp is set to the current
/// ledger timestamp, starting a fresh window. Call this exactly once, at the
/// top of every guarded critical entrypoint, *after* the admin check.
///
/// # Errors
/// * [`EscrowError::CooldownActive`] -- the previous invocation of this
///   action is still inside its cool-off window.
pub fn guard(env: &Env, action: &Symbol) -> Result<(), EscrowError> {
    if !is_ready(env, action) {
        return Err(EscrowError::CooldownActive);
    }
    let now = env.ledger().timestamp();
    env.storage()
        .instance()
        .set(&StorageKey::LastAction(action.clone()), &now);
    Ok(())
}
