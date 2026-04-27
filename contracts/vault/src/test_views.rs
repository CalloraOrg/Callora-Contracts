extern crate std;

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, String};

use super::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_usdc<'a>(env: &'a Env, admin: &Address) -> (Address, token::StellarAssetClient<'a>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (addr.clone(), token::StellarAssetClient::new(env, &addr))
}

fn setup(env: &Env) -> (Address, CalloraVaultClient<'_>, Address) {
    let owner = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_addr);
    let (usdc, _) = create_usdc(env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    (owner, client, usdc)
}

// ---------------------------------------------------------------------------
// get_meta
// ---------------------------------------------------------------------------

#[test]
fn get_meta_before_init_panics() {
    let env = Env::default();
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &vault_addr);
    assert!(client.try_get_meta().is_err());
}

#[test]
fn get_meta_returns_correct_fields() {
    let env = Env::default();
    let (owner, client, _) = setup(&env);
    let meta = client.get_meta();
    assert_eq!(meta.owner, owner);
    assert_eq!(meta.balance, 0);
    assert_eq!(meta.min_deposit, DEFAULT_MIN_DEPOSIT);
    assert!(meta.authorized_caller.is_none());
}

// ---------------------------------------------------------------------------
// balance
// ---------------------------------------------------------------------------

#[test]
fn balance_before_init_panics() {
    let env = Env::default();
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &vault_addr);
    assert!(client.try_balance().is_err());
}

#[test]
fn balance_returns_zero_after_init() {
    let env = Env::default();
    let (_, client, _) = setup(&env);
    assert_eq!(client.balance(), 0);
}

// ---------------------------------------------------------------------------
// get_admin
// ---------------------------------------------------------------------------

#[test]
fn get_admin_before_init_panics() {
    let env = Env::default();
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &vault_addr);
    assert!(client.try_get_admin().is_err());
}

#[test]
fn get_admin_returns_owner_after_init() {
    let env = Env::default();
    let (owner, client, _) = setup(&env);
    assert_eq!(client.get_admin(), owner);
}

// ---------------------------------------------------------------------------
// get_usdc_token
// ---------------------------------------------------------------------------

#[test]
fn get_usdc_token_before_init_panics() {
    let env = Env::default();
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &vault_addr);
    assert!(client.try_get_usdc_token().is_err());
}

#[test]
fn get_usdc_token_returns_address_after_init() {
    let env = Env::default();
    let (_, client, usdc) = setup(&env);
    assert_eq!(client.get_usdc_token(), usdc);
}

// ---------------------------------------------------------------------------
// get_max_deduct
// ---------------------------------------------------------------------------

#[test]
fn get_max_deduct_returns_default_when_not_set() {
    let env = Env::default();
    let (_, client, _) = setup(&env);
    assert_eq!(client.get_max_deduct(), DEFAULT_MAX_DEDUCT);
}

#[test]
fn get_max_deduct_returns_configured_value() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &vault_addr);
    let (usdc, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &Some(500));
    assert_eq!(client.get_max_deduct(), 500);
}

#[test]
fn set_max_deduct_updates_max_deduct_key_and_getter() {
    let env = Env::default();
    let (_, client, _) = setup(&env);
    assert_eq!(client.get_max_deduct(), DEFAULT_MAX_DEDUCT);

    client.set_max_deduct(&250);
    assert_eq!(client.get_max_deduct(), 250);

    client.set_max_deduct(&900);
    assert_eq!(client.get_max_deduct(), 900);
}

#[test]
#[should_panic(expected = "max_deduct must be positive")]
fn set_max_deduct_rejects_non_positive_values() {
    let env = Env::default();
    let (_, client, _) = setup(&env);
    client.set_max_deduct(&0);
}

// ---------------------------------------------------------------------------
// get_settlement
// ---------------------------------------------------------------------------

#[test]
fn get_settlement_before_set_panics() {
    let env = Env::default();
    let (_, client, _) = setup(&env);
    assert!(client.try_get_settlement().is_err());
}

#[test]
fn get_settlement_returns_address_after_set() {
    let env = Env::default();
    let (owner, client, _) = setup(&env);
    let settlement = Address::generate(&env);
    client.set_settlement(&owner, &settlement);
    assert_eq!(client.get_settlement(), settlement);
}

// ---------------------------------------------------------------------------
// get_revenue_pool
// ---------------------------------------------------------------------------

#[test]
fn get_revenue_pool_returns_none_when_not_set() {
    let env = Env::default();
    let (_, client, _) = setup(&env);
    assert!(client.get_revenue_pool().is_none());
}

#[test]
fn get_revenue_pool_returns_some_after_set() {
    let env = Env::default();
    let (owner, client, _) = setup(&env);
    let pool = Address::generate(&env);
    client.set_revenue_pool(&owner, &Some(pool.clone()));
    assert_eq!(client.get_revenue_pool(), Some(pool));
}

// ---------------------------------------------------------------------------
// get_contract_addresses
// ---------------------------------------------------------------------------

#[test]
fn get_contract_addresses_after_init_only() {
    let env = Env::default();
    let (_, client, usdc) = setup(&env);
    let (got_usdc, settlement, pool) = client.get_contract_addresses();
    assert_eq!(got_usdc, Some(usdc));
    assert!(settlement.is_none());
    assert!(pool.is_none());
}

#[test]
fn get_contract_addresses_fully_configured() {
    let env = Env::default();
    let (owner, client, usdc) = setup(&env);
    let settlement = Address::generate(&env);
    let pool = Address::generate(&env);
    client.set_settlement(&owner, &settlement);
    client.set_revenue_pool(&owner, &Some(pool.clone()));
    let (got_usdc, got_settlement, got_pool) = client.get_contract_addresses();
    assert_eq!(got_usdc, Some(usdc));
    assert_eq!(got_settlement, Some(settlement));
    assert_eq!(got_pool, Some(pool));
}

// ---------------------------------------------------------------------------
// is_paused
// ---------------------------------------------------------------------------

#[test]
fn is_paused_returns_false_before_init() {
    let env = Env::default();
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &vault_addr);
    // must not panic and must return false
    assert!(!client.is_paused());
}

#[test]
fn is_paused_reflects_pause_unpause() {
    let env = Env::default();
    let (owner, client, _) = setup(&env);
    assert!(!client.is_paused());
    client.pause(&owner);
    assert!(client.is_paused());
    client.unpause(&owner);
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// is_authorized_depositor
// ---------------------------------------------------------------------------

#[test]
fn is_authorized_depositor_owner_always_true() {
    let env = Env::default();
    let (owner, client, _) = setup(&env);
    assert!(client.is_authorized_depositor(&owner));
}

#[test]
fn is_authorized_depositor_unknown_address_false() {
    let env = Env::default();
    let (_, client, _) = setup(&env);
    let stranger = Address::generate(&env);
    assert!(!client.is_authorized_depositor(&stranger));
}

#[test]
fn is_authorized_depositor_added_address_true() {
    let env = Env::default();
    let (owner, client, _) = setup(&env);
    let depositor = Address::generate(&env);
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    assert!(client.is_authorized_depositor(&depositor));
}

// ---------------------------------------------------------------------------
// get_allowed_depositors
// ---------------------------------------------------------------------------

#[test]
fn get_allowed_depositors_empty_before_any_added() {
    let env = Env::default();
    let (_, client, _) = setup(&env);
    assert_eq!(client.get_allowed_depositors().len(), 0);
}

#[test]
fn get_allowed_depositors_reflects_additions() {
    let env = Env::default();
    let (owner, client, _) = setup(&env);
    let d1 = Address::generate(&env);
    let d2 = Address::generate(&env);
    client.set_allowed_depositor(&owner, &Some(d1.clone()));
    client.set_allowed_depositor(&owner, &Some(d2.clone()));
    let list = client.get_allowed_depositors();
    assert_eq!(list.len(), 2);
    assert!(list.contains(&d1));
    assert!(list.contains(&d2));
}

// ---------------------------------------------------------------------------
// get_metadata
// ---------------------------------------------------------------------------

#[test]
fn get_metadata_returns_none_when_not_set() {
    let env = Env::default();
    let (_, client, _) = setup(&env);
    let id = String::from_str(&env, "offer1");
    assert!(client.get_metadata(&id).is_none());
}

#[test]
fn get_metadata_returns_value_after_set() {
    let env = Env::default();
    let (owner, client, _) = setup(&env);
    let id = String::from_str(&env, "offer1");
    let val = String::from_str(&env, "ipfs://abc");
    client.set_metadata(&owner, &id, &val);
    assert_eq!(client.get_metadata(&id), Some(val));
}
