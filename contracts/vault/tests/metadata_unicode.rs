use callora_vault::{CalloraVault, CalloraVaultClient, VaultError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String};

fn setup(env: &Env) -> (CalloraVaultClient<'_>, Address) {
    env.mock_all_auths();
    let owner = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_addr);
    let usdc = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    (client, owner)
}

#[test]
fn set_metadata_rejects_unicode_confusables_and_invisible_controls() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let offering_id = String::from_str(&env, "off1");

    for metadata in [
        "meta\u{200b}data",
        "meta\u{200d}data",
        "meta\u{202e}data",
        "раypal",
        "cafe\u{0301}",
    ] {
        let result =
            client.try_set_metadata(&owner, &offering_id, &String::from_str(&env, metadata));
        assert_eq!(result, Err(Ok(VaultError::MetadataInvalid)));
    }
}

#[test]
fn update_metadata_rejects_unicode_confusables_and_invisible_controls() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let offering_id = String::from_str(&env, "off1");
    client.set_metadata(&owner, &offering_id, &String::from_str(&env, "metadata"));

    let result = client.try_update_metadata(
        &owner,
        &offering_id,
        &String::from_str(&env, "new\u{200d}metadata"),
    );
    assert_eq!(result, Err(Ok(VaultError::MetadataInvalid)));

    let confusable_offering_id = String::from_str(&env, "оff1");
    let result = client.try_update_metadata(
        &owner,
        &confusable_offering_id,
        &String::from_str(&env, "metadata"),
    );
    assert_eq!(result, Err(Ok(VaultError::OfferingIdInvalid)));
}

#[test]
fn visible_ascii_metadata_is_stored_as_canonical_nfc() {
    let env = Env::default();
    let (client, owner) = setup(&env);
    let offering_id = String::from_str(&env, "off1");
    let metadata = String::from_str(
        &env,
        "ipfs://bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
    );

    let result = client.set_metadata(&owner, &offering_id, &metadata);
    assert_eq!(result, metadata);
    assert_eq!(client.get_metadata(&offering_id), Some(metadata));
}
