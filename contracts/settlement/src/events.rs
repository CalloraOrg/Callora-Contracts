//! Event topic Symbol constructors and structured emit helpers for the
//! Callora Settlement contract.
//!
//! # Design
//!
//! This module is the **single source of truth** for every event emitted by the
//! settlement contract. It exposes two layers:
//!
//! 1. **Topic constructors** – `event_*(&env) -> Symbol` functions that return
//!    the stable topic Symbol for a given event. These guarantee byte-level
//!    identity and prevent accidental topic-name drift across call sites.
//!
//! 2. **Emit helpers** – `emit_*(&env, …)` functions that bundle the topic
//!    construction, data struct construction, and `env.events().publish()` call
//!    into one place. Call sites in `lib.rs`, `admin.rs`, and `limits.rs` use
//!    these helpers exclusively; no inline `Symbol::new(…)` or
//!    `env.events().publish(…)` calls appear outside this module.
//!
//! # Adding a new event
//!
//! 1. Add `pub fn event_<name>(env: &Env) -> Symbol` with a doc-comment.
//! 2. Add a corresponding `pub fn emit_<name>(env: &Env, …)` function.
//! 3. Add a snapshot test in the `#[cfg(test)] mod tests` block asserting
//!    byte-identity of the topic string.
//! 4. Update `EVENT_SCHEMA.md` and `docs/EVENT_TOPICS.md`.

use soroban_sdk::{Address, BytesN, Env, Symbol};

use crate::limits::MinBalanceChanged;
use crate::types::{
    AdminBroadcast, AdminMigrationEvent, BalanceCreditedEvent, DailyWithdrawCapChanged,
    DepositEvent, DeveloperClaimWindowChanged, DeveloperForceCreditedEvent, DeveloperWithdrawEvent,
    GlobalPool, PaymentReceivedEvent, VaultAcceptedEvent, VaultProposedEvent,
};

// ─── Topic constructors ──────────────────────────────────────────────────────

/// Returns the Symbol for the `"initialized"` event topic.
///
/// Emitted once when the settlement contract is first initialized.
pub fn event_initialized(env: &Env) -> Symbol {
    Symbol::new(env, "initialized")
}

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

/// Returns the Symbol for the `"deposit"` event topic.
///
/// Emitted when a deposit is made for a developer (alongside `balance_credited`)
/// in both `receive_payment` (`to_pool = false`) and `batch_receive_payment`.
pub fn event_deposit(env: &Env) -> Symbol {
    Symbol::new(env, "deposit")
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

/// Returns the Symbol for the `"claim_window_changed"` event topic.
///
/// Emitted when the admin sets or clears a developer's claim window.
pub fn event_developer_claim_window_changed(env: &Env) -> Symbol {
    Symbol::new(env, "claim_window_changed")
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

/// Returns the Symbol for the `"admin_cancelled"` event topic.
///
/// Emitted when the current admin cancels a pending admin transfer.
pub fn event_admin_cancelled(env: &Env) -> Symbol {
    Symbol::new(env, "admin_cancelled")
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

/// Returns the Symbol for the `"upgraded"` event topic.
///
/// Emitted when the admin upgrades the contract to a new WASM hash via `upgrade`.
pub fn event_upgraded(env: &Env) -> Symbol {
    Symbol::new(env, "upgraded")
}

/// Returns the Symbol for the `"developer_force_credited"` event topic.
///
/// Emitted when the admin force-credits a developer's balance outside the
/// normal `receive_payment` flow (e.g. correcting an error or migrating funds).
pub fn event_developer_force_credited(env: &Env) -> Symbol {
    Symbol::new(env, "developer_force_credited")
}

/// Returns the Symbol for the `"admin_broadcast"` event topic.
///
/// Emitted when the admin broadcasts an emergency message.
pub fn event_admin_broadcast(env: &Env) -> Symbol {
    Symbol::new(env, "admin_broadcast")
}

/// Returns the Symbol for the `"admin_migration_proposed"` event topic.
///
/// Emitted when the admin proposes a timelocked developer balance migration
/// via [`crate::CalloraSettlement::propose_admin_migration`]. The migration
/// becomes executable after the configured timelock window has elapsed.
pub fn event_admin_migration_proposed(env: &Env) -> Symbol {
    Symbol::new(env, "admin_migration_proposed")
}

/// Returns the Symbol for the `"admin_migration"` event topic.
///
/// Emitted when a pending developer balance migration is executed via
/// [`crate::CalloraSettlement::execute_admin_migration`]. The `from` and `to`
/// addresses are included as topics so indexers can trace balance movement.
pub fn event_admin_migration(env: &Env) -> Symbol {
    Symbol::new(env, "admin_migration")
}

/// Returns the Symbol for the `"developer_min_balance_changed"` event topic.
///
/// Emitted when the admin sets or updates a developer's minimum balance
/// threshold. A withdrawal that would leave the developer's balance below
/// this threshold is rejected.
pub fn event_developer_min_balance_changed(env: &Env) -> Symbol {
    Symbol::new(env, "developer_min_balance_changed")
}

/// Returns the Symbol for the `"metadata_removed"` event topic.
///
/// Emitted when the admin removes metadata associated with a developer or
/// offering via [`crate::CalloraSettlement::remove_metadata`].
pub fn event_metadata_removed(env: &Env) -> Symbol {
    Symbol::new(env, "metadata_removed")
}

// ─── Emit helpers ────────────────────────────────────────────────────────────

/// Emit `"initialized"` once when the settlement contract is first set up.
///
/// Topics: `(initialized, admin)`
/// Data:   `GlobalPool` snapshot captured at init time.
pub fn emit_initialized(env: &Env, admin: &Address, vault: &Address, pool: &GlobalPool) {
    env.events().publish(
        (event_initialized(env), admin.clone(), vault.clone()),
        pool.clone(),
    );
}

/// Emit `"payment_received"` when an inbound payment is routed to the global
/// pool or a developer balance.
///
/// Topics: `(payment_received, from_vault)`
/// Data:   [`PaymentReceivedEvent`]
pub fn emit_payment_received(env: &Env, caller: &Address, payload: PaymentReceivedEvent) {
    env.events()
        .publish((event_payment_received(env), caller.clone()), payload);
}

/// Emit `"balance_credited"` when a developer's balance is incremented.
///
/// Topics: `(balance_credited, developer)`
/// Data:   [`BalanceCreditedEvent`]
pub fn emit_balance_credited(env: &Env, developer: &Address, payload: BalanceCreditedEvent) {
    env.events()
        .publish((event_balance_credited(env), developer.clone()), payload);
}

/// Emit `"deposit"` alongside each developer credit (both single and batch).
///
/// Topics: `(deposit, developer)`
/// Data:   [`DepositEvent`]
pub fn emit_deposit(env: &Env, developer: &Address, payload: DepositEvent) {
    env.events()
        .publish((event_deposit(env), developer.clone()), payload);
}

/// Emit `"developer_withdraw"` when a developer withdraws accrued balance.
///
/// Topics: `(developer_withdraw, developer)`
/// Data:   [`DeveloperWithdrawEvent`]
pub fn emit_developer_withdraw(env: &Env, developer: &Address, payload: DeveloperWithdrawEvent) {
    env.events()
        .publish((event_developer_withdraw(env), developer.clone()), payload);
}

/// Emit `"daily_withdraw_cap_changed"` when the admin updates a developer's
/// daily cap.
///
/// Topics: `(daily_withdraw_cap_changed, caller)`
/// Data:   [`DailyWithdrawCapChanged`]
pub fn emit_daily_withdraw_cap_changed(
    env: &Env,
    caller: &Address,
    payload: DailyWithdrawCapChanged,
) {
    env.events().publish(
        (event_daily_withdraw_cap_changed(env), caller.clone()),
        payload,
    );
}

/// Emit `"claim_window_changed"` when a developer claim window is set or cleared.
///
/// Topics: `(claim_window_changed, developer)`
/// Data:   [`DeveloperClaimWindowChanged`]
pub fn emit_developer_claim_window_changed(
    env: &Env,
    developer: &Address,
    payload: DeveloperClaimWindowChanged,
) {
    env.events().publish(
        (event_developer_claim_window_changed(env), developer.clone()),
        payload,
    );
}

/// Emit `"admin_nominated"` when the current admin nominates a successor.
///
/// Topics: `(admin_nominated, current_admin, new_admin)`
/// Data:   `new_admin` address
pub fn emit_admin_nominated(env: &Env, current_admin: &Address, new_admin: &Address) {
    env.events().publish(
        (
            event_admin_nominated(env),
            current_admin.clone(),
            new_admin.clone(),
        ),
        new_admin.clone(),
    );
}

/// Emit `"admin_accepted"` when the pending admin finalizes the transfer.
///
/// Topics: `(admin_accepted, old_admin, new_admin)`
/// Data:   `new_admin` address
pub fn emit_admin_accepted(env: &Env, old_admin: &Address, new_admin: &Address) {
    env.events().publish(
        (
            event_admin_accepted(env),
            old_admin.clone(),
            new_admin.clone(),
        ),
        new_admin.clone(),
    );
}

/// Emit `"admin_cancelled"` when the admin cancels a pending transfer.
///
/// Topics: `(admin_cancelled, admin)`
/// Data:   `admin` address
pub fn emit_admin_cancelled(env: &Env, admin: &Address) {
    env.events()
        .publish((event_admin_cancelled(env), admin.clone()), admin.clone());
}

/// Emit `"vault_proposed"` when the admin proposes a new vault.
///
/// Topics: `(vault_proposed, admin)`
/// Data:   [`VaultProposedEvent`]
pub fn emit_vault_proposed(env: &Env, admin: &Address, payload: VaultProposedEvent) {
    env.events()
        .publish((event_vault_proposed(env), admin.clone()), payload);
}

/// Emit `"vault_accepted"` when the proposed vault rotation is accepted.
///
/// Topics: `(vault_accepted, new_vault)`
/// Data:   [`VaultAcceptedEvent`]
pub fn emit_vault_accepted(env: &Env, new_vault: &Address, payload: VaultAcceptedEvent) {
    env.events()
        .publish((event_vault_accepted(env), new_vault.clone()), payload);
}

/// Emit `"upgraded"` when the contract WASM is replaced.
///
/// Topics: `(upgraded, caller)`
/// Data:   `new_wasm_hash`
pub fn emit_upgraded(env: &Env, caller: &Address, new_wasm_hash: &BytesN<32>) {
    env.events()
        .publish((event_upgraded(env), caller.clone()), new_wasm_hash.clone());
}

/// Emit `"developer_force_credited"` when the admin manually credits a
/// developer balance outside the normal payment flow.
///
/// Topics: `(developer_force_credited, developer)`
/// Data:   [`DeveloperForceCreditedEvent`]
pub fn emit_developer_force_credited(
    env: &Env,
    developer: &Address,
    payload: DeveloperForceCreditedEvent,
) {
    env.events().publish(
        (event_developer_force_credited(env), developer.clone()),
        payload,
    );
}

/// Emit `"admin_broadcast"` when the admin sends an emergency message.
///
/// Topics: `(admin_broadcast, caller)`
/// Data:   [`AdminBroadcast`]
pub fn emit_admin_broadcast(env: &Env, caller: &Address, payload: AdminBroadcast) {
    env.events()
        .publish((event_admin_broadcast(env), caller.clone()), payload);
}

/// Emit `"admin_migration_proposed"` when a timelock'd developer balance
/// migration proposal is recorded.
///
/// Topics: `(admin_migration_proposed, from)`
/// Data:   [`crate::timelock::PendingDeveloperMigration`]
pub fn emit_admin_migration_proposed(
    env: &Env,
    from: &Address,
    payload: crate::timelock::PendingDeveloperMigration,
) {
    env.events()
        .publish((event_admin_migration_proposed(env), from.clone()), payload);
}

/// Emit `"admin_migration"` when a pending balance migration is executed.
///
/// Topics: `(admin_migration, from, to)`
/// Data:   [`AdminMigrationEvent`]
pub fn emit_admin_migration(env: &Env, from: &Address, to: &Address, payload: AdminMigrationEvent) {
    env.events().publish(
        (event_admin_migration(env), from.clone(), to.clone()),
        payload,
    );
}

/// Emit `"developer_min_balance_changed"` when the admin sets a developer's
/// minimum withdrawal threshold.
///
/// Topics: `(developer_min_balance_changed, developer)`
/// Data:   [`MinBalanceChanged`]
pub fn emit_developer_min_balance_changed(
    env: &Env,
    developer: &Address,
    payload: MinBalanceChanged,
) {
    env.events().publish(
        (event_developer_min_balance_changed(env), developer.clone()),
        payload,
    );
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    // ── Topic byte-identity snapshots ─────────────────────────────────────

    /// Every constructor must map to exactly the expected byte string.
    /// Changing any of these would be a breaking change to the on-chain
    /// interface; that is why they are explicitly snapshot-tested.

    #[test]
    fn test_event_initialized_bytes() {
        let env = Env::default();
        assert_eq!(event_initialized(&env), Symbol::new(&env, "initialized"));
    }

    #[test]
    fn test_event_payment_received_bytes() {
        let env = Env::default();
        assert_eq!(
            event_payment_received(&env),
            Symbol::new(&env, "payment_received")
        );
    }

    #[test]
    fn test_event_balance_credited_bytes() {
        let env = Env::default();
        assert_eq!(
            event_balance_credited(&env),
            Symbol::new(&env, "balance_credited")
        );
    }

    #[test]
    fn test_event_deposit_bytes() {
        let env = Env::default();
        assert_eq!(event_deposit(&env), Symbol::new(&env, "deposit"));
    }

    #[test]
    fn test_event_developer_withdraw_bytes() {
        let env = Env::default();
        assert_eq!(
            event_developer_withdraw(&env),
            Symbol::new(&env, "developer_withdraw")
        );
    }

    #[test]
    fn test_event_daily_withdraw_cap_changed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_daily_withdraw_cap_changed(&env),
            Symbol::new(&env, "daily_withdraw_cap_changed")
        );
    }

    #[test]
    fn test_event_developer_claim_window_changed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_developer_claim_window_changed(&env),
            Symbol::new(&env, "claim_window_changed")
        );
    }

    #[test]
    fn test_event_admin_nominated_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_nominated(&env),
            Symbol::new(&env, "admin_nominated")
        );
    }

    #[test]
    fn test_event_admin_accepted_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_accepted(&env),
            Symbol::new(&env, "admin_accepted")
        );
    }

    #[test]
    fn test_event_admin_cancelled_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_cancelled(&env),
            Symbol::new(&env, "admin_cancelled")
        );
    }

    #[test]
    fn test_event_vault_proposed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_vault_proposed(&env),
            Symbol::new(&env, "vault_proposed")
        );
    }

    #[test]
    fn test_event_vault_accepted_bytes() {
        let env = Env::default();
        assert_eq!(
            event_vault_accepted(&env),
            Symbol::new(&env, "vault_accepted")
        );
    }

    #[test]
    fn test_event_upgraded_bytes() {
        let env = Env::default();
        assert_eq!(event_upgraded(&env), Symbol::new(&env, "upgraded"));
    }

    #[test]
    fn test_event_developer_force_credited_bytes() {
        let env = Env::default();
        assert_eq!(
            event_developer_force_credited(&env),
            Symbol::new(&env, "developer_force_credited")
        );
    }

    #[test]
    fn test_event_admin_broadcast_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_broadcast(&env),
            Symbol::new(&env, "admin_broadcast")
        );
    }

    #[test]
    fn test_admin_migration_event_bytes() {
        let env = Env::default();
        assert_eq!(
            event_admin_migration_proposed(&env),
            Symbol::new(&env, "admin_migration_proposed")
        );
        assert_eq!(
            event_admin_migration(&env),
            Symbol::new(&env, "admin_migration")
        );
    }

    #[test]
    fn test_event_developer_min_balance_changed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_developer_min_balance_changed(&env),
            Symbol::new(&env, "developer_min_balance_changed")
        );
    }

    #[test]
    fn test_event_metadata_removed_bytes() {
        let env = Env::default();
        assert_eq!(
            event_metadata_removed(&env),
            Symbol::new(&env, "metadata_removed")
        );
    }
}
