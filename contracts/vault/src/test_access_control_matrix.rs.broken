#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, BytesN, Env, Symbol, Vec};

use super::*;

fn create_usdc<'a>(env: &'a Env, admin: &Address) -> (Address, token::StellarAssetClient<'a>) {
    let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract_address.address();
    (
        address.clone(),
        token::StellarAssetClient::new(env, &address),
    )
}

fn create_vault(env: &Env) -> (Address, CalloraVaultClient<'_>) {
    let address = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &address);
    (address, client)
}

#[test]
fn test_entrypoints_require_auth() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc, usdc_client) = create_usdc(&env, &owner);

    // init parameters: owner, usdc, initial_balance, authorized_caller, min_deposit, revenue_pool, max_deduct, settlement
    client.init(
        &owner,
        &usdc,
        &Some(0i128),
        &Some(owner.clone()),
        &Some(1i128),
        &None::<Address>,
        &1000i128,
        &settlement,
    );
    usdc_client.mint(&vault_addr, &1000);

    let mut items: Vec<(i128, u64)> = Vec::new(&env);
    items.push_back((10i128, 1u64));

    env.set_auths(&[]);

    assert!(client.try_deposit(&owner, &50).is_err());
    assert!(client.try_deduct(&owner, &10, &1u64).is_err());
    assert!(client.try_batch_deduct(&owner, &items).is_err());
    assert!(client.try_set_authorized_caller(&owner, &owner).is_err());
    assert!(client.try_pause(&owner).is_err());
    assert!(client.try_unpause(&owner).is_err());
    assert!(client.try_set_max_deduct(&owner, &200).is_err());
    assert!(client
        .try_set_settlement(&owner, &Address::generate(&env))
        .is_err());
    assert!(client.try_set_reserve_cap(&owner, &usdc, &200).is_err());
    assert!(client.try_add_address(&owner, &recipient).is_err());
    assert!(client.try_clear_all(&owner).is_err());
    assert!(client
        .try_prune_processed_requests(&owner, &Vec::<soroban_sdk::Symbol>::new(&env))
        .is_err());
    assert!(client.try_set_timelock_window(&owner, &86_400u64).is_err());
    assert!(client.try_propose_pause(&owner).is_err());
    assert!(client.try_execute_pause(&owner).is_err());
    assert!(client.try_cancel_pause(&owner).is_err());
    assert!(client
        .try_propose_upgrade(&owner, &BytesN::from_array(&env, &[0u8; 32]))
        .is_err());
    assert!(client.try_execute_upgrade(&owner).is_err());
    assert!(client.try_cancel_upgrade(&owner).is_err());
    assert!(client.try_propose_sweep(&owner, &recipient, &10).is_err());
    assert!(client.try_execute_sweep(&owner).is_err());
    assert!(client.try_cancel_sweep(&owner).is_err());
}

#[test]
fn test_allowlist_entrypoints_reject_non_owner() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let settlement = Address::generate(&env);
    let attacker = Address::generate(&env);
    let target = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc, usdc_client) = create_usdc(&env, &owner);

    client.init(
        &owner,
        &usdc,
        &Some(0i128),
        &Some(owner.clone()),
        &Some(1i128),
        &None::<Address>,
        &1000i128,
        &settlement,
    );
    usdc_client.mint(&vault_addr, &1000);

    let result = client.try_add_address(&attacker, &target);
    assert!(result.is_err());

    let result = client.try_clear_all(&attacker);
    assert!(result.is_err());

    assert!(client.try_add_address(&owner, &target).is_ok());
    assert!(client.try_clear_all(&owner).is_ok());
}
