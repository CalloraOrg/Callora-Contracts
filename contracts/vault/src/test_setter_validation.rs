extern crate std;
use super::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{token, Address, Env, IntoVal, String, Symbol};

fn create_usdc<'a>(env: &'a Env, admin: &'a Address) -> (Address, token::StellarAssetClient<'a>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (addr.clone(), token::StellarAssetClient::new(env, &addr))
}

fn create_vault(env: &Env) -> (Address, CalloraVaultClient) {
    let addr = env.register(CalloraVault, ());
    (addr.clone(), CalloraVaultClient::new(env, &addr))
}

fn setup(env: &Env) -> (Address, CalloraVaultClient, Address, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let (vault_addr, client) = create_vault(env);
    let (usdc, _) = create_usdc(env, &admin);
    client.init(&admin, &usdc, &None, &None, &None, &None, &None);
    (vault_addr, client, usdc, admin)
}

#[test]
fn set_price_offering_id_too_long() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let long_id = "a".repeat((MAX_OFFERING_ID_LEN + 1) as usize);
    client.set_price(
        &admin,
        &String::from_str(&env, &long_id),
        &String::from_str(&env, "100"),
    );
}

#[test]
fn set_price_zero_price() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    client.set_price(
        &admin,
        &String::from_str(&env, "off1"),
        &String::from_str(&env, "0"),
    );
}

#[test]
fn set_price_successful() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    client.set_price(
        &admin,
        &String::from_str(&env, "off1"),
        &String::from_str(&env, "1000"),
    );
    // Verify readback
    let stored = client.get_price(&String::from_str(&env, "off1"));
    assert_eq!(stored, Some(String::from_str(&env, "1000")));
    // Verify event emitted (using try call to capture events)
    let events = env.events().all();
    // Find price_set event
    let price_set = events.iter().find(|e| {
        let s: Symbol = e.1.get(0).unwrap().into_val(&env);
        s == Symbol::new(&env, "price_set")
    });
    assert!(price_set.is_some(), "price_set event not emitted");
}

#[test]
fn set_settlement_vault_address_fails() {
    let env = Env::default();
    let (vault_addr, client, _, admin) = setup(&env);
    let result = client.try_set_settlement(&admin, &vault_addr);
    assert!(result.is_err());
}

#[test]
fn set_settlement_usdc_address_fails() {
    let env = Env::default();
    let (_, client, usdc, admin) = setup(&env);
    let result = client.try_set_settlement(&admin, &usdc);
    assert!(result.is_err());
}

#[test]
fn set_settlement_equals_revenue_pool_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let pool = Address::generate(&env);
    // Use propose/accept two-step flow to set revenue pool
    client.propose_revenue_pool(&Some(pool.clone()));
    client.accept_revenue_pool();
    let result = client.try_set_settlement(&admin, &pool);
    assert!(result.is_err());
}

#[test]
fn set_settlement_valid_address_succeeds() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let s = Address::generate(&env);
    client.set_settlement(&admin, &s);
    assert_eq!(client.get_settlement(), s);
}

#[test]
fn set_revenue_pool_vault_address_fails() {
    let env = Env::default();
    let (vault_addr, client, _, admin) = setup(&env);
    let result = client.try_set_revenue_pool(&admin, &Some(vault_addr));
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// set_metadata input validation (length / charset hardening)
// ---------------------------------------------------------------------------

#[test]
fn set_metadata_empty_offering_id_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, ""),
        &String::from_str(&env, "valid"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}

#[test]
fn set_metadata_null_byte_in_offering_id_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, "off\x00ering"),
        &String::from_str(&env, "valid"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}

#[test]
fn set_metadata_control_char_in_offering_id_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, "off\x01ering"),
        &String::from_str(&env, "valid"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}

#[test]
fn set_metadata_leading_space_offering_id_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, " off1"),
        &String::from_str(&env, "valid"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}

#[test]
fn set_metadata_trailing_space_offering_id_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, "off1 "),
        &String::from_str(&env, "valid"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}

#[test]
fn set_metadata_whitespace_only_offering_id_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, "   "),
        &String::from_str(&env, "valid"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}

#[test]
fn set_metadata_empty_metadata_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, "off1"),
        &String::from_str(&env, ""),
    );
    assert_eq!(result, Err(Ok(VaultError::MetadataInvalid)));
}

#[test]
fn set_metadata_null_byte_in_metadata_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, "off1"),
        &String::from_str(&env, "meta\x00data"),
    );
    assert_eq!(result, Err(Ok(VaultError::MetadataInvalid)));
}

#[test]
fn set_metadata_control_char_in_metadata_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, "off1"),
        &String::from_str(&env, "meta\x1Fdata"),
    );
    assert_eq!(result, Err(Ok(VaultError::MetadataInvalid)));
}

#[test]
fn set_metadata_leading_space_metadata_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, "off1"),
        &String::from_str(&env, " metadata"),
    );
    assert_eq!(result, Err(Ok(VaultError::MetadataInvalid)));
}

#[test]
fn set_metadata_trailing_space_metadata_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, "off1"),
        &String::from_str(&env, "metadata "),
    );
    assert_eq!(result, Err(Ok(VaultError::MetadataInvalid)));
}

#[test]
fn set_metadata_whitespace_only_metadata_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_metadata(
        &admin,
        &String::from_str(&env, "off1"),
        &String::from_str(&env, "   "),
    );
    assert_eq!(result, Err(Ok(VaultError::MetadataInvalid)));
}

#[test]
fn set_metadata_exact_max_length_succeeds() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let offering_id = "a".repeat(MAX_OFFERING_ID_LEN as usize);
    let metadata = "b".repeat(MAX_METADATA_LEN as usize);
    let result = client.set_metadata(
        &admin,
        &String::from_str(&env, &offering_id),
        &String::from_str(&env, &metadata),
    );
    assert_eq!(result, String::from_str(&env, &metadata));
}

// ---------------------------------------------------------------------------
// set_price offering_id input validation (length / charset hardening)
// ---------------------------------------------------------------------------

#[test]
fn set_price_empty_offering_id_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_price(
        &admin,
        &String::from_str(&env, ""),
        &String::from_str(&env, "100"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}

#[test]
fn set_price_null_byte_in_offering_id_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_price(
        &admin,
        &String::from_str(&env, "off\x00ering"),
        &String::from_str(&env, "100"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}

#[test]
fn set_price_control_char_in_offering_id_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_price(
        &admin,
        &String::from_str(&env, "off\x01ering"),
        &String::from_str(&env, "100"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}

#[test]
fn set_price_leading_space_offering_id_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_price(
        &admin,
        &String::from_str(&env, " off1"),
        &String::from_str(&env, "100"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}

#[test]
fn set_price_trailing_space_offering_id_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_price(
        &admin,
        &String::from_str(&env, "off1 "),
        &String::from_str(&env, "100"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}

#[test]
fn set_price_whitespace_only_offering_id_fails() {
    let env = Env::default();
    let (_, client, _, admin) = setup(&env);
    let result = client.try_set_price(
        &admin,
        &String::from_str(&env, "   "),
        &String::from_str(&env, "100"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}
