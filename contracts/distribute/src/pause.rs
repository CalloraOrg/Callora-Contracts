//! Administrative pause control for distribution entrypoints.
//!
//! The pause flag is stored in instance storage. State-changing pause controls
//! require authorization from the supplied caller and verify that caller is
//! the configured contract administrator. Read-only inspection does not require
//! authorization.

use soroban_sdk::{symbol_short, Address, Env};

const ADMIN_KEY: soroban_sdk::Symbol = symbol_short!("admin");
const PAUSED_KEY: soroban_sdk::Symbol = symbol_short!("paused");

/// Pauses distribution state changes.
///
/// The caller must authenticate and must equal the administrator stored during
/// contract initialization. Calling this function more than once is harmless.
pub fn pause(env: &Env, caller: &Address) {
    caller.require_auth();
    require_admin(env, caller);
    env.storage().instance().set(&PAUSED_KEY, &true);
}

/// Resumes distribution state changes.
///
/// The caller must authenticate and must equal the configured administrator.
/// Calling this function while the contract is already active is harmless.
pub fn resume(env: &Env, caller: &Address) {
    caller.require_auth();
    require_admin(env, caller);
    env.storage().instance().set(&PAUSED_KEY, &false);
}

/// Returns whether distribution state changes are currently paused.
///
/// This view does not require authorization and remains available while the
/// contract is paused.
pub fn is_paused(env: &Env) -> bool {
    env.storage()
        .instance()
        .get(&PAUSED_KEY)
        .unwrap_or(false)
}

/// Rejects a state-changing distribution operation while paused.
///
/// `batch_distribute` must invoke this guard before performing any transfer or
/// other persistent state mutation. The guard performs no authorization check;
/// authorization remains the responsibility of the distribution entrypoint.
pub fn require_not_paused(env: &Env) {
    if is_paused(env) {
        panic!("contract is paused");
    }
}

fn require_admin(env: &Env, caller: &Address) {
    let admin = env
        .storage()
        .instance()
        .get::<_, Address>(&ADMIN_KEY)
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
        env.storage().instance().set(&ADMIN_KEY, &admin);
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
}
