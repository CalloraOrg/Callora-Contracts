#![cfg(test)]

extern crate std;

use callora_stake::{CalloraStake, CalloraStakeClient, SUPPORTED_CAPABILITIES};
use soroban_sdk::Env;

fn create_contract(env: &Env) -> CalloraStakeClient<'_> {
    let contract_id = env.register(CalloraStake, ());
    CalloraStakeClient::new(env, &contract_id)
}

#[test]
fn capabilities_does_not_require_auth() {
    let env = Env::default();
    let client = create_contract(&env);

    env.set_auths(&[]);
    let caps = client.capabilities();
    assert_eq!(caps, SUPPORTED_CAPABILITIES);
}

#[test]
fn capabilities_returns_nonzero() {
    let env = Env::default();
    let client = create_contract(&env);

    assert_ne!(client.capabilities(), 0);
}

#[test]
fn capabilities_equals_supported_capabilities_constant() {
    let env = Env::default();
    let client = create_contract(&env);

    assert_eq!(client.capabilities(), SUPPORTED_CAPABILITIES);
}
