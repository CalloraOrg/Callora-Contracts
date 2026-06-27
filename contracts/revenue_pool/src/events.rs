//! Event topic Symbol constructors for the Callora Revenue Pool contract.
//!
//! This module centralizes all event topic strings into dedicated functions,
//! ensuring byte-identity is preserved and preventing accidental topic name drift
//! across call sites.

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"init"` event topic.
///
/// Emitted when the revenue pool is first initialized with an admin and USDC token address.
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

/// Returns the Symbol for the `"admin_changed"` event topic.
///
/// Emitted during `set_admin` alongside `admin_transfer_started` to record the
/// before/after admin intent explicitly for indexers and audit trails.
pub fn event_admin_changed(env: &Env) -> Symbol {
    Symbol::new(env, "admin_changed")
}

/// Returns the Symbol for the `"admin_transfer_started"` event topic.
///
/// Emitted when the current admin nominates a new admin via `set_admin`.
/// The nominated admin must call `claim_admin` to complete the transfer.
pub fn event_admin_transfer_started(env: &Env) -> Symbol {
    Symbol::new(env, "admin_transfer_started")
}

/// Returns the Symbol for the `"admin_transfer_completed"` event topic.
///
/// Emitted when the pending admin successfully claims ownership via `claim_admin`,
/// completing the two-step admin handover.
pub fn event_admin_transfer_completed(env: &Env) -> Symbol {
    Symbol::new(env, "admin_transfer_completed")
}

/// Returns the Symbol for the `"admin_cancelled"` event topic.
///
/// Emitted when the current admin cancels a pending admin transfer.
pub fn event_admin_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "admin_cancelled")
}

/// Returns the Symbol for the `"pause_set"` event topic.
///
/// Emitted by both `pause` (with data `true`) and `unpause` (with data `false`)
/// to signal a change in the pool's pause state.
pub fn event_pause_set(env: &Env) -> Symbol {
    Symbol::new(env, "pause_set")
}

/// Returns the Symbol for the `"receive_payment"` event topic.
///
/// Emitted when the admin calls `receive_payment` to log an incoming payment
/// from the vault for indexer alignment.
pub fn event_receive_payment(env: &Env) -> Symbol {
    Symbol::new(env, "receive_payment")
}

/// Returns the Symbol for the `"yield_deposited"` event topic.
///
/// Emitted when the treasury deposits accumulated protocol yield into the
/// revenue pool via `deposit_yield`.
pub fn event_yield_deposited(env: &Env) -> Symbol {
    Symbol::new(env, "yield_deposited")
}

/// Returns the Symbol for the `"set_max_distribute"` event topic.
///
/// Emitted when the admin updates the per-leg maximum distribute cap.
pub fn event_set_max_distribute(env: &Env) -> Symbol {
    Symbol::new(env, "set_max_distribute")
}

/// Returns the Symbol for the `"distribute"` event topic.
///
/// Emitted when the admin distributes USDC to a single developer wallet via `distribute`.
pub fn event_distribute(env: &Env) -> Symbol {
    Symbol::new(env, "distribute")
}

/// Returns the Symbol for the `"batch_distribute"` event topic.
///
/// Emitted once per payment leg during a `batch_distribute` call, after all
/// validation has passed.
pub fn event_batch_distribute(env: &Env) -> Symbol {
    Symbol::new(env, "batch_distribute")
}

/// Returns the Symbol for the `"upgraded"` event topic.
///
/// Emitted when the admin upgrades the contract to a new WASM hash via `upgrade`.
pub fn event_upgraded(env: &Env) -> Symbol {
    Symbol::new(env, "upgraded")
}

/// Returns the Symbol for the `"admin_broadcast"` event topic.
///
/// Emitted when the admin broadcasts an emergency message.
pub fn event_admin_broadcast(env: &Env) -> Symbol {
    Symbol::new(env, "admin_broadcast")
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    /// Snapshot: proves event_init still maps to exactly the bytes for "init".
    #[test]
    fn test_event_init_bytes() {
        let env = Env::default();
        assert_eq!(event_init(&env), Symbol::new(&env, "init"));
    }

    /// Snapshot: proves event_admin_changed still maps to exactly the bytes for "admin_changed".
    #[test]
    fn test_event_admin_changed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_changed(&env),
            Symbol::new(&env, "admin_changed")
        );
    }

    /// Snapshot: proves event_admin_transfer_started still maps to exactly the bytes for "admin_transfer_started".
    #[test]
    fn test_event_admin_transfer_started_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_transfer_started(&env),
            Symbol::new(&env, "admin_transfer_started")
        );
    }

    /// Snapshot: proves event_admin_transfer_completed still maps to exactly the bytes for "admin_transfer_completed".
    #[test]
    fn test_event_admin_transfer_completed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_transfer_completed(&env),
            Symbol::new(&env, "admin_transfer_completed")
        );
    }

    /// Snapshot: proves event_admin_cancelled still maps to exactly the bytes for "admin_cancelled".
    #[test]
    fn test_event_admin_cancelled_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_cancelled(&env),
            Symbol::new(&env, "admin_cancelled")
        );
    }

    /// Snapshot: proves event_pause_set still maps to exactly the bytes for "pause_set".
    #[test]
    fn test_event_pause_set_bytes() {
        let env = Env::default();
        assert_eq!(event_pause_set(&env), Symbol::new(&env, "pause_set"));
    }

    /// Snapshot: proves event_receive_payment still maps to exactly the bytes for "receive_payment".
    #[test]
    fn test_event_receive_payment_bytes() {
        let env = Env::default();
        assert_eq!(
            event_receive_payment(&env),
            Symbol::new(&env, "receive_payment")
        );
    }

    /// Snapshot: proves event_yield_deposited still maps to exactly the bytes for "yield_deposited".
    #[test]
    fn test_event_yield_deposited_bytes() {
        let env = Env::default();
        assert_eq!(
            event_yield_deposited(&env),
            Symbol::new(&env, "yield_deposited")
        );
    }

    /// Snapshot: proves event_set_max_distribute still maps to exactly the bytes for "set_max_distribute".
    #[test]
    fn test_event_set_max_distribute_bytes() {
        let env = Env::default();
        assert_eq!(
            event_set_max_distribute(&env),
            Symbol::new(&env, "set_max_distribute")
        );
    }

    /// Snapshot: proves event_distribute still maps to exactly the bytes for "distribute".
    #[test]
    fn test_event_distribute_bytes() {
        let env = Env::default();
        assert_eq!(event_distribute(&env), Symbol::new(&env, "distribute"));
    }

    /// Snapshot: proves event_batch_distribute still maps to exactly the bytes for "batch_distribute".
    #[test]
    fn test_event_batch_distribute_bytes() {
        let env = Env::default();
        assert_eq!(
            event_batch_distribute(&env),
            Symbol::new(&env, "batch_distribute")
        );
    }

    /// Snapshot: proves event_upgraded still maps to exactly the bytes for "upgraded".
    #[test]
    fn test_event_upgraded_bytes() {
        let env = Env::default();
        assert_eq!(event_upgraded(&env), Symbol::new(&env, "upgraded"));
    }

    /// Snapshot: proves event_admin_broadcast still maps to exactly the bytes for "admin_broadcast".
    #[test]
    fn test_event_admin_broadcast_bytes() {
        let env = Env::default();
        assert_eq!(event_admin_broadcast(&env), Symbol::new(&env, "admin_broadcast"));
    }
}
