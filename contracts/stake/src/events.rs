//! Event topic Symbol constructors and publishers for the Callora Stake contract.
//!
//! Uses the canonical versioned topic shape `(event_name, event_version_v1, ...)`
//! shared across Callora contracts.

use soroban_sdk::{Address, Env, Symbol};

/// Returns the Symbol for the canonical event version marker `"callora_v1"`.
pub fn event_version_v1(env: &Env) -> Symbol {
    Symbol::new(env, "callora_v1")
}

/// Returns the Symbol for the `"init"` event topic.
pub fn event_init(env: &Env) -> Symbol {
    Symbol::new(env, "init")
}

/// Returns the Symbol for the `"staked"` event topic.
pub fn event_staked(env: &Env) -> Symbol {
    Symbol::new(env, "staked")
}

/// Returns the Symbol for the `"unstaked"` event topic.
pub fn event_unstaked(env: &Env) -> Symbol {
    Symbol::new(env, "unstaked")
}

/// Publish the `"init"` event with versioned topics.
pub fn publish_init(env: &Env, admin: &Address, token: &Address) {
    env.events().publish(
        (event_init(env), event_version_v1(env)),
        (admin.clone(), token.clone()),
    );
}

/// Publish the `"staked"` event with versioned topics.
///
/// Topics: `("staked", "callora_v1", staker)`
/// Data: `(amount, new_staker_balance, new_total_staked)`
pub fn publish_staked(
    env: &Env,
    staker: &Address,
    amount: i128,
    new_balance: i128,
    new_total: i128,
) {
    env.events().publish(
        (event_staked(env), event_version_v1(env), staker.clone()),
        (amount, new_balance, new_total),
    );
}

/// Publish the `"unstaked"` event with versioned topics.
///
/// Topics: `("unstaked", "callora_v1", staker)`
/// Data: `(amount, new_staker_balance, new_total_staked)`
pub fn publish_unstaked(
    env: &Env,
    staker: &Address,
    amount: i128,
    new_balance: i128,
    new_total: i128,
) {
    env.events().publish(
        (event_unstaked(env), event_version_v1(env), staker.clone()),
        (amount, new_balance, new_total),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::Env;

    #[test]
    fn test_event_version_v1_bytes() {
        let env = Env::default();
        assert_eq!(event_version_v1(&env), Symbol::new(&env, "callora_v1"));
    }

    #[test]
    fn test_event_init_bytes() {
        let env = Env::default();
        assert_eq!(event_init(&env), Symbol::new(&env, "init"));
    }

    #[test]
    fn test_event_staked_bytes() {
        let env = Env::default();
        assert_eq!(event_staked(&env), Symbol::new(&env, "staked"));
    }

    #[test]
    fn test_event_unstaked_bytes() {
        let env = Env::default();
        assert_eq!(event_unstaked(&env), Symbol::new(&env, "unstaked"));
    }
}
