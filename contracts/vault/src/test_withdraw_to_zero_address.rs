//! Focused tests for withdraw_to zero-address recipient validation.
//!
//! Verifies that `withdraw_to` rejects zero-address recipients with
//! `VaultError::ZeroAddressRecipient` (error code 37).

extern crate std;

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, BytesN, Env};

use super::*;

// ---------------------------------------------------------------------------
// Test helpers (copied from test.rs for isolation)
// ---------------------------------------------------------------------------

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

fn fund_vault(
    usdc_admin_client: &token::StellarAssetClient,
    vault_address: &Address,
    amount: i128,
) {
    usdc_admin_client.mint(vault_address, &amount);
}

/// Create a zero address (contract address with all zero bytes).
fn zero_address(env: &Env) -> Address {
    let zero_bytes = BytesN::<32>::from_array(env, &[0u8; 32]);
    Address::from_contract_id(&zero_bytes)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Verify that withdraw_to rejects zero-address recipient.
#[test]
fn withdraw_to_zero_address_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    let zero_addr = zero_address(&env);
    let result = client.try_withdraw_to(&zero_addr, &100);

    assert!(result.is_err(), "expected error for zero-address recipient");
    // Verify the specific error code (37 = ZeroAddressRecipient)
    let err = result.unwrap_err();
    match err {
        Ok(vault_err) => {
            assert_eq!(vault_err, VaultError::ZeroAddressRecipient);
        }
        Err(_) => panic!("expected VaultError::ZeroAddressRecipient"),
    }
}

/// Verify that withdraw_to succeeds with a valid non-zero recipient.
#[test]
fn withdraw_to_valid_address_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    let remaining = client.withdraw_to(&recipient, &100);

    assert_eq!(remaining, 900);
    assert_eq!(client.balance(), 900);
    assert_eq!(usdc_client.balance(&recipient), 100);
}

/// Verify that zero-address check happens before amount validation.
#[test]
fn withdraw_to_zero_address_checked_before_amount() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    // Even with an invalid amount (0), zero-address should be rejected first
    let zero_addr = zero_address(&env);
    let result = client.try_withdraw_to(&zero_addr, &0);

    assert!(result.is_err(), "expected error for zero-address recipient");
    let err = result.unwrap_err();
    match err {
        Ok(vault_err) => {
            // Zero-address check should happen before amount validation
            assert_eq!(vault_err, VaultError::ZeroAddressRecipient);
        }
        Err(_) => panic!("expected VaultError::ZeroAddressRecipient"),
    }
}

/// Verify that withdraw_to to zero address fails even when paused.
/// (Since withdraw is allowed when paused for emergency recovery,
/// zero-address validation should still apply.)
#[test]
fn withdraw_to_zero_address_fails_even_when_paused() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    // Pause the vault
    client.pause(&owner);
    assert!(client.is_paused());

    let zero_addr = zero_address(&env);
    let result = client.try_withdraw_to(&zero_addr, &100);

    assert!(result.is_err(), "expected error for zero-address recipient even when paused");
    let err = result.unwrap_err();
    match err {
        Ok(vault_err) => {
            assert_eq!(vault_err, VaultError::ZeroAddressRecipient);
        }
        Err(_) => panic!("expected VaultError::ZeroAddressRecipient"),
    }
}

/// Verify that a valid recipient still works after a failed zero-address attempt.
#[test]
fn withdraw_to_valid_after_zero_address_rejection() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    // First, try zero address (should fail)
    let zero_addr = zero_address(&env);
    let result = client.try_withdraw_to(&zero_addr, &100);
    assert!(result.is_err());

    // Balance should be unchanged
    assert_eq!(client.balance(), 1000);

    // Now try valid recipient (should succeed)
    let remaining = client.withdraw_to(&recipient, &100);
    assert_eq!(remaining, 900);
    assert_eq!(usdc_client.balance(&recipient), 100);
}
