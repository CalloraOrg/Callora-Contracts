extern crate std;

use super::*;
use callora_storage_migration::StorageMigrationError;
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

/// Initialization records version 1 (v1) and the real non-zero storage layout hash.
#[test]
fn init_records_real_layout_hash_and_v1() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc_address, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);

    let expected_layout = callora_storage_migration::layout_hash(&env, STORAGE_LAYOUT_DESCRIPTOR);
    let zero_layout = callora_storage_migration::zero_layout_hash(&env);
    assert_ne!(expected_layout, zero_layout);
    assert_eq!(
        client.get_storage_migration_version(),
        STORAGE_MIGRATION_VERSION
    );
    assert_eq!(client.get_storage_layout_hash(), Some(expected_layout));
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
    assert_eq!(client.get_storage_migration_version(), 2);
}

/// Two sequential upgrades (v1 -> v2 -> v3) succeed both via default `upgrade`
/// and via explicit `upgrade_with_layout` across evolving layout descriptors.
#[test]
fn two_sequential_upgrades_v1_to_v2_to_v3_succeed() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc_address, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    assert_eq!(client.get_storage_migration_version(), 1);

    let layout_v1 = client.current_layout_hash();
    let layout_v2 = callora_storage_migration::layout_hash(
        &env,
        "revenue_pool:v2:admin,usdc,paused,emergency_paused,pending_admin,pause_guardian,max_distribute,cumulative_yield,version,emergency_drain",
    );
    let layout_v3 = callora_storage_migration::layout_hash(
        &env,
        "revenue_pool:v3:admin,usdc,paused,emergency_paused,pending_admin,pause_guardian,max_distribute,cumulative_yield,version,emergency_drain,extra_field",
    );

    let wasm_v2 = env
        .deployer()
        .upload_contract_wasm(soroban_sdk::Bytes::from_slice(
            &env,
            &[0, 97, 115, 109, 1, 0, 0, 0],
        ));
    client.upgrade_with_layout(&admin, &wasm_v2, &2, &layout_v1, &layout_v2);
    assert_eq!(client.get_storage_migration_version(), 2);
    assert_eq!(client.get_storage_layout_hash(), Some(layout_v2.clone()));
    assert_eq!(client.get_version(), Some(wasm_v2));

    let wasm_v3 = env
        .deployer()
        .upload_contract_wasm(soroban_sdk::Bytes::new(&env));
    client.upgrade_with_layout(&admin, &wasm_v3, &3, &layout_v2, &layout_v3);
    assert_eq!(client.get_storage_migration_version(), 3);
    assert_eq!(client.get_storage_layout_hash(), Some(layout_v3));
    assert_eq!(client.get_version(), Some(wasm_v3));
}

/// Calling `upgrade` twice sequentially also advances v1 -> v2 -> v3 cleanly.
#[test]
fn sequential_default_upgrades_advance_v1_to_v2_to_v3() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc_address, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    assert_eq!(client.get_storage_migration_version(), 1);

    let wasm_v2 = env
        .deployer()
        .upload_contract_wasm(soroban_sdk::Bytes::new(&env));
    client.upgrade(&admin, &wasm_v2);
    assert_eq!(client.get_storage_migration_version(), 2);

    let wasm_v3 = env
        .deployer()
        .upload_contract_wasm(soroban_sdk::Bytes::from_slice(
            &env,
            &[0, 97, 115, 109, 1, 0, 0, 0],
        ));
    client.upgrade(&admin, &wasm_v3);
    assert_eq!(client.get_storage_migration_version(), 3);
    assert_eq!(client.get_version(), Some(wasm_v3));
}

/// An upgrade with a mismatched `expected_source_layout` fails with `SchemaMismatch`.
#[test]
fn upgrade_with_mismatched_source_layout_fails_with_schema_mismatch() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc_address, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);

    let wrong_source_layout =
        callora_storage_migration::layout_hash(&env, "revenue_pool:wrong_source_schema");
    let target_layout = client.current_layout_hash();
    let new_hash = env
        .deployer()
        .upload_contract_wasm(soroban_sdk::Bytes::new(&env));

    let res =
        client.try_upgrade_with_layout(&admin, &new_hash, &2, &wrong_source_layout, &target_layout);
    assert_eq!(
        res,
        Err(Ok(soroban_sdk::Error::from_contract_error(
            StorageMigrationError::SchemaMismatch as u32
        )))
    );
    assert_eq!(client.get_storage_migration_version(), 1);
    assert_eq!(client.get_version(), None);
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
