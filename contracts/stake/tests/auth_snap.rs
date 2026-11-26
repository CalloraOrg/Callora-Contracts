#![cfg(test)]

extern crate std;

use callora_stake::{
    CalloraStake, CalloraStakeClient, CAP_DELEGATION, CAP_REWARDS, CAP_SLASHING,
    CAP_STAKE_UNSTAKE, CAP_STAKE_VIEW, CAP_WITHDRAW_TIMELOCK, IMPLEMENTED_CAPABILITIES,
    SUPPORTED_CAPABILITIES,
};
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
    assert_eq!(caps, IMPLEMENTED_CAPABILITIES);
}

#[test]
fn capabilities_returns_implemented_mask() {
    let env = Env::default();
    let client = create_contract(&env);

    assert_eq!(client.capabilities(), CAP_STAKE_UNSTAKE | CAP_STAKE_VIEW);
}

#[test]
fn capabilities_are_subset_of_reserved_mask() {
    let env = Env::default();
    let client = create_contract(&env);

    let caps = client.capabilities();
    assert_eq!(caps & !SUPPORTED_CAPABILITIES, 0);
}

/// Every set bit must map to an exported entrypoint on the generated client.
/// This test keeps the advertised bits in step with the actual entrypoints
/// as they are added.
#[test]
fn capability_bits_map_to_entrypoints() {
    let env = Env::default();
    let client = create_contract(&env);

    let caps = client.capabilities();

    // Bit 0 -> stake/unstake entrypoints are exported.
    assert_eq!(caps & CAP_STAKE_UNSTAKE, CAP_STAKE_UNSTAKE);
    // Bit 1 -> delegation entrypoint is not yet exported.
    assert_eq!(caps & CAP_DELEGATION, 0);
    // Bit 2 -> rewards entrypoint is not yet exported.
    assert_eq!(caps & CAP_REWARDS, 0);
    // Bit 3 -> slashing entrypoint is not yet exported.
    assert_eq!(caps & CAP_SLASHING, 0);
    // Bit 4 -> withdraw timelock entrypoint is not yet exported.
    assert_eq!(caps & CAP_WITHDRAW_TIMELOCK, 0);
    // Bit 5 -> stake view entrypoints (get_stake, total_staked, get_token) are exported.
    assert_eq!(caps & CAP_STAKE_VIEW, CAP_STAKE_VIEW);
}
