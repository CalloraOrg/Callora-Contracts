extern crate std;

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env};

use super::*;
use callora_settlement::CalloraSettlement;

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract_address.address();
    let client = token::Client::new(env, &address);
    let admin_client = token::StellarAssetClient::new(env, &address);
    (address, client, admin_client)
}

fn create_vault(env: &Env) -> (Address, CalloraVaultClient<'_>) {
    let address = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &address);
    (address, client)
}

fn create_settlement(env: &Env, admin: &Address, vault_address: &Address) -> Address {
    let settlement_address = env.register(CalloraSettlement, ());
    let settlement_client =
        callora_settlement::CalloraSettlementClient::new(env, &settlement_address);
    env.mock_all_auths();
    settlement_client.init(admin, vault_address);
    settlement_address
}

#[test]
fn simulate_deduct_matches_real_call() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    let settlement = create_settlement(&env, &owner, &vault_addr);

    env.mock_all_auths();
    token::StellarAssetClient::new(&env, &usdc).mint(&vault_addr, &500);
    client.init(
        &owner,
        &usdc,
        &Some(500),
        &Some(caller.clone()),
        &Some(1),
        &None,
        &Some(250),
        &Some(settlement),
    );

    let expected = client.simulate_deduct(&caller, &100, &9u64);
    assert_eq!(expected, 400);
    client.deduct(&caller, &100, &9u64);
    assert_eq!(client.balance(), 400);

    let items = soroban_sdk::Vec::from_array(&env, [(50_i128, 10_u64), (75_i128, 11_u64)]);
    let expected_batch = client.simulate_batch_deduct(&caller, &items);
    assert_eq!(expected_batch, 275);
    client.batch_deduct(&caller, &items);
    assert_eq!(client.balance(), 275);
}

#[test]
fn simulate_rejects_unauthorized_caller_like_real_deduct() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let stranger = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    let settlement = create_settlement(&env, &owner, &vault_addr);

    env.mock_all_auths();
    token::StellarAssetClient::new(&env, &usdc).mint(&vault_addr, &500);
    client.init(
        &owner,
        &usdc,
        &Some(500),
        &Some(caller.clone()),
        &Some(1),
        &None,
        &Some(250),
        &Some(settlement),
    );

    let sim = client.try_simulate_deduct(&stranger, &10, &1u64);
    assert!(matches!(sim, Err(Ok(VaultError::Unauthorized))));
    let real = client.try_deduct(&stranger, &10, &1u64);
    assert!(matches!(real, Err(Ok(VaultError::Unauthorized))));

    let batch_items = soroban_sdk::Vec::from_array(&env, [(10_i128, 1_u64)]);
    let batch_sim = client.try_simulate_batch_deduct(&stranger, &batch_items);
    assert!(matches!(batch_sim, Err(Ok(VaultError::Unauthorized))));
    let batch_real = client.try_batch_deduct(&stranger, &batch_items);
    assert!(matches!(batch_real, Err(Ok(VaultError::Unauthorized))));
}
