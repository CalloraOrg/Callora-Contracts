#[cfg(test)]
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
    assert_eq!(caps, 0);
}

#[test]
fn capabilities_returns_zero() {
    let env = Env::default();
    let client = create_contract(&env);

    assert_eq!(client.capabilities(), 0);
}

#[test]
fn capabilities_are_subset_of_reserved_mask() {
    let env = Env::default();
    let client = create_contract(&env);

    let caps = client.capabilities();
    assert_eq!(caps & !SUPPORTED_CAPABILITIES, 0);
}

/// Every set bit must map to an exported entrypoint on the generated client.
/// Today the contract exports only `capabilities`, which is not a stake feature
/// bit, so the expected mask is 0. This test keeps the advertised bits in
/// step with the actual entrypoints as they are added.
#[test]
fn capability_bits_map_to_entrypoints() {
    let env = Env::default();
    let client = create_contract(&env);

    let caps = client.capabilities();

    // Bit 0 -> stake/unstake entrypoints must exist in the generated client.
    // None are exported today, so the bit must be clear.
    assert_eq!(caps & (1 << 0), 0);
    // Bit 1 -> delegation entrypoint must exist.
    assert_eq!(caps & (1 << 1), 0);
    // Bit 2 -> rewards entrypoint must exist.
    assert_eq!(caps & (1 << 2), 0);
    // Bit 3 -> slashing entrypoint must exist.
    assert_eq!(caps & (1 << 3), 0);
    // Bit 4 -> withdraw timelock entrypoint must exist.
    assert_eq!(caps & (1 << 4), 0);
    // Bit 5 -> stake view entrypoint must exist.
    assert_eq!(caps & (1 << 5), 0);
}