//! Admin management module for the revenue pool contract.
//!
//! Implements two-step admin transfer and emergency pause guardian.
//!
//! # Two-step admin transfer
//!
//! Admin transfers follow a propose-then-accept pattern:
//! 1. Current admin calls `set_admin` to nominate a pending admin.
//! 2. The nominated admin calls `claim_admin` to accept the role.
//!
//! This prevents accidental transfers to an unreachable or mistyped address.
//!
//! # Pause guardian
//!
//! The admin may designate a separate pause guardian address. The guardian
//! can call `pause` in an emergency to halt USDC distributions but:
//! - Cannot `unpause` (only admin can)
//! - Cannot exercise any other admin power (set_admin, set_pause_guardian,
//!   clear_pause_guardian, distribute, etc.)
//!
//! Pause state and guardian are preserved across admin rotations.

use soroban_sdk::{Address, Env, Symbol};

// ---------------------------------------------------------------------------
// Storage keys (kept internal — only this module reads/writes these keys)
// ---------------------------------------------------------------------------

/// Instance storage key for the pool's pause flag.
pub(crate) const PAUSED_KEY: &str = "paused";

/// Instance storage key for the emergency pause guardian address.
pub(crate) const PAUSE_GUARDIAN_KEY: &str = "pause_guardian";

/// Instance storage key for the pending admin during a two-step transfer.
pub(crate) const PENDING_ADMIN_KEY: &str = "pending_admin";

// ---------------------------------------------------------------------------
// Pause state
// ---------------------------------------------------------------------------

/// Read the pool's paused flag. Returns `false` before `init` or if never set.
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get::<_, bool>(&Symbol::new(env, PAUSED_KEY))
        .unwrap_or(false)
}

/// Write the pool's paused flag.
pub fn set_paused(env: &Env, paused: bool) {
    env.storage()
        .instance()
        .set(&Symbol::new(env, PAUSED_KEY), &paused);
}

// ---------------------------------------------------------------------------
// Pause guardian
// ---------------------------------------------------------------------------

/// Return the current pause guardian, or `None` if none is set.
pub fn get_pause_guardian(env: &Env) -> Option<Address> {
    env.storage()
        .instance()
        .get(&Symbol::new(env, PAUSE_GUARDIAN_KEY))
}

/// Store a new pause guardian. Overwrites any previous guardian.
pub fn set_guardian(env: &Env, guardian: &Address) {
    env.storage()
        .instance()
        .set(&Symbol::new(env, PAUSE_GUARDIAN_KEY), guardian);
}

/// Remove the current pause guardian.
pub fn clear_guardian(env: &Env) {
    env.storage()
        .instance()
        .remove(&Symbol::new(env, PAUSE_GUARDIAN_KEY));
}

// ---------------------------------------------------------------------------
// Pending admin (two-step transfer)
// ---------------------------------------------------------------------------

/// Return the pending admin, or `None` if no transfer is in progress.
pub fn get_pending_admin(env: &Env) -> Option<Address> {
    env.storage()
        .instance()
        .get(&Symbol::new(env, PENDING_ADMIN_KEY))
}

/// Store a pending admin address (step 1 of two-step transfer).
pub fn set_pending_admin(env: &Env, pending: &Address) {
    env.storage()
        .instance()
        .set(&Symbol::new(env, PENDING_ADMIN_KEY), pending);
}

/// Remove the pending admin (after a successful transfer or cancellation).
pub fn clear_pending_admin(env: &Env) {
    env.storage()
        .instance()
        .remove(&Symbol::new(env, PENDING_ADMIN_KEY));
}
