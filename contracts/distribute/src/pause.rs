//! Administrative pause control for distribution entrypoints.
//!
//! The pause flag is stored in instance storage using the contract's canonical
//! [`StorageKey::Paused`](crate::limits::StorageKey::Paused). State-changing
//! pause controls require authorization from the supplied caller and verify
//! that caller is the configured contract administrator. Read-only inspection
//! does not require authorization.
//!
//! # Integration
//!
//! This module is re-exported as `pub mod pause` in the crate root and may be
//! used by contract methods or by external consumers who need low-level pause
//! access without going through the full [`CalloraDistribute`] contract API.

use soroban_sdk::{Address, Env};

use crate::limits::StorageKey;

/// Pauses distribution state changes.
///
/// The caller must authenticate and must equal the administrator stored during
/// contract initialization. Calling this function more than once is harmless
/// (idempotent — the flag remains `true`).
///
/// # Panics
/// - If the caller has not authorized the invocation (`require_auth`).
/// - If the caller is not the configured admin (`"unauthorized"`).
/// - If the contract has not been initialized (`"contract is not initialized"`).
pub fn pause(env: &Env, caller: &Address) {
    caller.require_auth();
    require_admin(env, caller);
    env.storage().instance().set(&StorageKey::Paused, &true);
}

/// Resumes distribution state changes.
///
/// The caller must authenticate and must equal the configured administrator.
/// Calling this function while the contract is already active is harmless
/// (idempotent — the flag remains `false`).
///
/// # Panics
/// - If the caller has not authorized the invocation.
/// - If the caller is not the configured admin (`"unauthorized"`).
/// - If the contract has not been initialized.
pub fn resume(env: &Env, caller: &Address) {
    caller.require_auth();
    require_admin(env, caller);
    env.storage().instance().set(&StorageKey::Paused, &false);
}

/// Returns whether distribution state changes are currently paused.
///
/// This view does not require authorization and remains available while the
/// contract is paused. Returns `false` if the flag has never been set (i.e.
/// before `init` or in the default unpaused state).
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&StorageKey::Paused)
        .unwrap_or(false)
}

/// Rejects a state-changing distribution operation while paused.
///
/// `batch_open`, `batch_close`, and individual `open`/`close` must invoke this
/// guard before performing any persistent state mutation. The guard performs
/// no authorization check; authorization remains the responsibility of the
/// calling entrypoint.
///
/// # Panics
/// - `"contract is paused"` — the circuit breaker is active.
pub fn require_not_paused(env: &Env) {
    if is_paused(env) {
        panic!("contract is paused");
    }
}

fn require_admin(env: &Env, caller: &Address) {
    let admin = env
        .storage()
        .instance()
        .get::<_, Address>(&StorageKey::Admin)
        .unwrap_or_else(|| panic!("contract is not initialized"));

    if admin != *caller {
        panic!("unauthorized");
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::*;
    use soroban_sdk::testutils::Address as _;

    fn setup(env: &Env) -> Address {
        let admin = Address::generate(env);
        env.storage().instance().set(&StorageKey::Admin, &admin);
        admin
    }

    #[test]
    fn pause_is_false_by_default() {
        let env = Env::default();
        assert!(!is_paused(&env));
    }

    #[test]
    fn admin_can_pause_and_resume() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = setup(&env);

        pause(&env, &admin);
        assert!(is_paused(&env));

        resume(&env, &admin);
        assert!(!is_paused(&env));
    }

    #[test]
    fn pause_requires_authentication() {
        let env = Env::default();
        let admin = setup(&env);

        let result = std::panic::catch_unwind(|| pause(&env, &admin));
        assert!(result.is_err());
        assert!(!is_paused(&env));
    }

    #[test]
    fn non_admin_cannot_pause_or_resume() {
        let env = Env::default();
        env.mock_all_auths();
        let _admin = setup(&env);
        let outsider = Address::generate(&env);

        let pause_result = std::panic::catch_unwind(|| pause(&env, &outsider));
        assert!(pause_result.is_err());
        assert!(!is_paused(&env));

        let resume_result = std::panic::catch_unwind(|| resume(&env, &outsider));
        assert!(resume_result.is_err());
    }

    #[test]
    fn distribution_guard_rejects_only_while_paused() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = setup(&env);

        assert!(std::panic::catch_unwind(|| require_not_paused(&env)).is_ok());

        pause(&env, &admin);
        assert!(std::panic::catch_unwind(|| require_not_paused(&env)).is_err());

        resume(&env, &admin);
        assert!(std::panic::catch_unwind(|| require_not_paused(&env)).is_ok());
    }

    /// Verify that the pause module uses the same storage key as the contract's
    /// `CalloraDistribute` implementation. This test writes via the module and
    /// confirms `StorageKey::Paused` is set (the contract reads from this key).
    #[test]
    fn uses_same_storage_key_as_contract() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = setup(&env);

        pause(&env, &admin);
        // Read via StorageKey::Paused — the key the contract methods use.
        let from_contract_key: bool = env
            .storage()
            .instance()
            .get(&StorageKey::Paused)
            .unwrap_or(false);
        assert!(from_contract_key, "pause.rs must write to StorageKey::Paused");

        // Read via the module's own is_paused helper.
        assert!(is_paused(&env));
    }

    /// Verify idempotent pause: calling pause when already paused does not panic.
    #[test]
    fn double_pause_is_idempotent() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = setup(&env);

        pause(&env, &admin);
        // Second pause should not panic (idempotent).
        assert!(std::panic::catch_unwind(|| pause(&env, &admin)).is_ok());
        assert!(is_paused(&env));
    }

    /// Verify idempotent resume: calling resume when not paused does not panic.
    #[test]
    fn double_resume_is_idempotent() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = setup(&env);

        // First resume when unpaused — should be harmless.
        assert!(std::panic::catch_unwind(|| resume(&env, &admin)).is_ok());
        assert!(!is_paused(&env));
    }
}

