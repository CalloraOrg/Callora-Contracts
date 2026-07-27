//! Event topic Symbol constructors for the Callora Cold contract.
//!
//! This module centralizes all event topic strings into dedicated functions,
//! ensuring byte-identity is preserved and preventing accidental topic name drift
//! across call sites.

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"init"` event topic.
///
/// Emitted when the cold-storage accounting is first initialized.
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

/// Returns the Symbol for the `"config_set"` event topic.
///
/// Emitted when the cold-storage configuration (e.g. hot/cold ratio, signers)
/// is updated via [`crate::CalloraCold::set_config`].
pub fn event_config_set(env: &Env) -> Symbol {
    Symbol::new(env, "config_set")
}

/// Returns the Symbol for the `"balances_set"` event topic.
///
/// Emitted when cold-storage balance records are initialized or updated
/// during a rebalance or sweep operation.
pub fn event_balances_set(env: &Env) -> Symbol {
    Symbol::new(env, "balances_set")
}

/// Returns the Symbol for the `"cold_sweep_proposed"` event topic.
///
/// Emitted when a multisig participant proposes a cold-storage sweep
/// that requires approval from other signers before execution.
pub fn event_cold_sweep_proposed(env: &Env) -> Symbol {
    Symbol::new(env, "cold_sweep_proposed")
}

/// Returns the Symbol for the `"cold_sweep_approved"` event topic.
///
/// Emitted when a sufficient number of signers have approved a pending
/// cold-storage sweep, making it eligible for execution.
pub fn event_cold_sweep_approved(env: &Env) -> Symbol {
    Symbol::new(env, "cold_sweep_approved")
}

/// Returns the Symbol for the `"cold_sweep_executed"` event topic.
///
/// Emitted when an approved cold-storage sweep is executed, transferring
/// funds from the cold account.
pub fn event_cold_sweep_executed(env: &Env) -> Symbol {
    Symbol::new(env, "cold_sweep_executed")
}

/// Returns the Symbol for the `"admin_nominated"` event topic.
///
/// Emitted when the current admin nominates a new admin. The nominated
/// admin must call [`crate::CalloraCold::accept_admin`] to complete the transfer.
pub fn event_admin_nominated(env: &Env) -> Symbol {
    Symbol::new(env, "admin_nominated")
}

/// Returns the Symbol for the `"admin_accepted"` event topic.
///
/// Emitted when the pending admin accepts the role, completing the
/// two-step admin handover.
pub fn event_admin_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "admin_accepted")
}

/// Returns the Symbol for the `"admin_cancelled"` event topic.
///
/// Emitted when the current admin cancels a pending admin transfer.
pub fn event_admin_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "admin_cancelled")
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

    /// Snapshot: proves event_config_set still maps to exactly the bytes for "config_set".
    #[test]
    fn test_event_config_set_bytes() {
        let env = Env::default();
        assert_eq!(event_config_set(&env), Symbol::new(&env, "config_set"));
    }

    /// Snapshot: proves event_balances_set still maps to exactly the bytes for "balances_set".
    #[test]
    fn test_event_balances_set_bytes() {
        let env = Env::default();
        assert_eq!(
            event_balances_set(&env),
            Symbol::new(&env, "balances_set")
        );
    }

    /// Snapshot: proves event_cold_sweep_proposed still maps to exactly the bytes for "cold_sweep_proposed".
    #[test]
    fn test_event_cold_sweep_proposed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_cold_sweep_proposed(&env),
            Symbol::new(&env, "cold_sweep_proposed")
        );
    }

    /// Snapshot: proves event_cold_sweep_approved still maps to exactly the bytes for "cold_sweep_approved".
    #[test]
    fn test_event_cold_sweep_approved_bytes() {
        let env = Env::default();
        assert_eq!(
            event_cold_sweep_approved(&env),
            Symbol::new(&env, "cold_sweep_approved")
        );
    }

    /// Snapshot: proves event_cold_sweep_executed still maps to exactly the bytes for "cold_sweep_executed".
    #[test]
    fn test_event_cold_sweep_executed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_cold_sweep_executed(&env),
            Symbol::new(&env, "cold_sweep_executed")
        );
    }

    /// Snapshot: proves event_admin_nominated still maps to exactly the bytes for "admin_nominated".
    #[test]
    fn test_event_admin_nominated_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_nominated(&env),
            Symbol::new(&env, "admin_nominated")
        );
    }

    /// Snapshot: proves event_admin_accepted still maps to exactly the bytes for "admin_accepted".
    #[test]
    fn test_event_admin_accepted_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_accepted(&env),
            Symbol::new(&env, "admin_accepted")
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
}
