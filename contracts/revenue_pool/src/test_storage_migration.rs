extern crate std;

use super::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token;
use soroban_sdk::{Address, BytesN, Env};

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

fn create_pool(env: &Env) -> (Address, RevenuePoolClient<'_>) {
    let address = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &address);
    (address, client)
}

/// A fresh, well-formed upgrade (non-zero hash, single forward step) must pass
/// the pre-upgrade migration guard and preserve all previously deployed state.
#[test]
fn upgrade_valid_wasm_hash_passes_migration_guard() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc_address, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);

    let new_hash = env
        .deployer()
        .upload_contract_wasm(soroban_sdk::Bytes::new(&env));
    // The migration guard must not reject a valid upgrade; the platform swap
    // itself is a no-op for a natively-registered test contract.
    client.upgrade(&admin, &new_hash);

    let readback: Option<BytesN<32>> = client.get_version();
    assert_eq!(readback, Some(new_hash));
}

/// The pre-upgrade storage-migration gate must reject an all-zero WASM hash
/// before any upgraded code is swapped in, regardless of admin authorization.
#[test]
fn upgrade_rejects_zero_wasm_hash_via_migration_guard() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc_address, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);

    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    let res = client.try_upgrade(&admin, &zero_hash);
    assert!(res.is_err());
}
