//! Event topic Symbol constructors for the Callora Settlement contract.
//!
//! This module centralizes all event topic strings into dedicated functions,
//! ensuring byte-identity is preserved and preventing accidental topic name drift
//! across call sites.

use soroban_sdk::{Env, Symbol};

/// Returns the Symbol for the `"payment_received"` event topic.
///
/// Emitted when a payment is received from the vault or admin, crediting
/// either the global pool or a specific developer balance.
pub fn event_payment_received(env: &Env) -> Symbol {
    Symbol::new(env, "payment_received")
}

/// Returns the Symbol for the `"balance_credited"` event topic.
///
/// Emitted when a developer's balance is incremented — either via
/// `receive_payment` (single) or `batch_receive_payment` (batch).
pub fn event_balance_credited(env: &Env) -> Symbol {
    Symbol::new(env, "balance_credited")
}

/// Returns the Symbol for the `"developer_withdraw"` event topic.
///
/// Emitted when a developer successfully withdraws their accrued balance
/// as on-ledger USDC.
pub fn event_developer_withdraw(env: &Env) -> Symbol {
    Symbol::new(env, "developer_withdraw")
}

/// Returns the Symbol for the `"daily_withdraw_cap_changed"` event topic.
///
/// Emitted when the admin sets or updates a developer's daily withdrawal cap.
pub fn event_daily_withdraw_cap_changed(env: &Env) -> Symbol {
    Symbol::new(env, "daily_withdraw_cap_changed")
}

/// Returns the Symbol for the `"admin_nominated"` event topic.
///
/// Emitted when the current admin nominates a new admin via `set_admin`.
/// The nominated admin must call `accept_admin` to complete the transfer.
pub fn event_admin_nominated(env: &Env) -> Symbol {
    Symbol::new(env, "admin_nominated")
}

/// Returns the Symbol for the `"admin_accepted"` event topic.
///
/// Emitted when the pending admin accepts the admin role via `accept_admin`,
/// completing the two-step admin handover.
pub fn event_admin_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "admin_accepted")
}

/// Returns the Symbol for the `"vault_proposed"` event topic.
///
/// Emitted when the admin proposes a new vault address via `propose_vault`.
/// The proposed vault must call `accept_vault` to be activated.
pub fn event_vault_proposed(env: &Env) -> Symbol {
    Symbol::new(env, "vault_proposed")
}

/// Returns the Symbol for the `"vault_accepted"` event topic.
///
/// Emitted when the proposed vault (or admin) accepts the vault rotation
/// via `accept_vault`, completing the two-step vault update.
pub fn event_vault_accepted(env: &Env) -> Symbol {
    Symbol::new(env, "vault_accepted")
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    /// Snapshot: proves event_payment_received still maps to exactly the bytes for "payment_received".
    #[test]
    fn test_event_payment_received_bytes() {
        let env = Env::default();
        assert_eq!(event_payment_received(&env), Symbol::new(&env, "payment_received"));
    }

    /// Snapshot: proves event_balance_credited still maps to exactly the bytes for "balance_credited".
    #[test]
    fn test_event_balance_credited_bytes() {
        let env = Env::default();
        assert_eq!(event_balance_credited(&env), Symbol::new(&env, "balance_credited"));
    }

    /// Snapshot: proves event_developer_withdraw still maps to exactly the bytes for "developer_withdraw".
    #[test]
    fn test_event_developer_withdraw_bytes() {
        let env = Env::default();
        assert_eq!(event_developer_withdraw(&env), Symbol::new(&env, "developer_withdraw"));
    }

    /// Snapshot: proves event_daily_withdraw_cap_changed still maps to exactly the bytes for "daily_withdraw_cap_changed".
    #[test]
    fn test_event_daily_withdraw_cap_changed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_daily_withdraw_cap_changed(&env),
            Symbol::new(&env, "daily_withdraw_cap_changed")
        );
    }

    /// Snapshot: proves event_admin_nominated still maps to exactly the bytes for "admin_nominated".
    #[test]
    fn test_event_admin_nominated_bytes() {
        let env = Env::default();
        assert_eq!(event_admin_nominated(&env), Symbol::new(&env, "admin_nominated"));
    }

    /// Snapshot: proves event_admin_accepted still maps to exactly the bytes for "admin_accepted".
    #[test]
    fn test_event_admin_accepted_bytes() {
        let env = Env::default();
        assert_eq!(event_admin_accepted(&env), Symbol::new(&env, "admin_accepted"));
    }

    /// Snapshot: proves event_vault_proposed still maps to exactly the bytes for "vault_proposed".
    #[test]
    fn test_event_vault_proposed_bytes() {
        let env = Env::default();
        assert_eq!(event_vault_proposed(&env), Symbol::new(&env, "vault_proposed"));
    }

    /// Snapshot: proves event_vault_accepted still maps to exactly the bytes for "vault_accepted".
    #[test]
    fn test_event_vault_accepted_bytes() {
        let env = Env::default();
        assert_eq!(event_vault_accepted(&env), Symbol::new(&env, "vault_accepted"));
    }
}
