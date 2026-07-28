//! Focused tests for per-call minimum-amount enforcement on `propose_sweep`.
//!
//! `deposit`, `deduct`, and `batch_deduct` already reject amounts below the
//! vault's configured `min_deposit` floor. `propose_sweep` previously only
//! rejected non-positive amounts, leaving it as the one value-moving
//! entrypoint that could still move a sub-unit/dust amount out of the vault.
//! These tests cover the new `VaultError::BelowMinTransferAmount` guard and
//! confirm it does not disturb the existing positive-amount check or the
//! normal propose/execute sweep flow.

#![cfg(test)]

use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{token, Address, Env};

use super::*;
use crate::timelock::DEFAULT_TIMELOCK_SECONDS;

fn create_usdc<'a>(env: &'a Env, admin: &Address) -> (Address, token::StellarAssetClient<'a>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (addr.clone(), token::StellarAssetClient::new(env, &addr))
}

/// Registers a vault with a custom `min_deposit` so boundary values are easy
/// to reason about, and mocks all auths so tests can focus on amount checks.
fn setup(env: &Env, min_deposit: i128) -> (Address, CalloraVaultClient<'_>, Address, Address) {
    env.ledger().set_timestamp(1_700_000_000);
    let owner = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_addr);
    let (usdc, _) = create_usdc(env, &owner);
    env.mock_all_auths();
    client.init(
        &owner,
        &usdc,
        &0,
        &owner,
        &min_deposit,
        &None,
        &1_000_000,
        &Address::generate(env),
    );
    (owner, client, usdc, vault_addr)
}

#[test]
fn propose_sweep_rejects_amount_below_min_deposit() {
    let env = Env::default();
    let (owner, client, _usdc, _vault_addr) = setup(&env, 100);
    let recipient = Address::generate(&env);

    let res = client.try_propose_sweep(&owner, &recipient, &99);
    assert_eq!(
        res.unwrap_err().unwrap(),
        VaultError::BelowMinTransferAmount
    );
    assert!(client.get_pending_sweep().is_none());
}

#[test]
fn propose_sweep_accepts_amount_at_min_deposit_boundary() {
    let env = Env::default();
    let (owner, client, _usdc, _vault_addr) = setup(&env, 100);
    let recipient = Address::generate(&env);

    client.propose_sweep(&owner, &recipient, &100);
    let pending = client.get_pending_sweep().expect("proposal recorded");
    assert_eq!(pending.amount, 100);
}

#[test]
fn propose_sweep_rejects_zero_amount_before_min_check() {
    let env = Env::default();
    let (owner, client, _usdc, _vault_addr) = setup(&env, 100);
    let recipient = Address::generate(&env);

    // Zero must fail with AmountNotPositive, not BelowMinTransferAmount —
    // the positive check runs first regardless of the configured minimum.
    let res = client.try_propose_sweep(&owner, &recipient, &0);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::AmountNotPositive);
}

#[test]
fn propose_sweep_rejects_negative_amount() {
    let env = Env::default();
    let (owner, client, _usdc, _vault_addr) = setup(&env, 100);
    let recipient = Address::generate(&env);

    let res = client.try_propose_sweep(&owner, &recipient, &-5);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::AmountNotPositive);
}

#[test]
fn propose_sweep_with_default_min_deposit_still_rejects_sub_unit_amount() {
    // min_deposit = 5 means "1" is a sub-unit (dust) amount relative to it.
    let env = Env::default();
    let (owner, client, _usdc, _vault_addr) = setup(&env, 5);
    let recipient = Address::generate(&env);

    let res = client.try_propose_sweep(&owner, &recipient, &1);
    assert_eq!(
        res.unwrap_err().unwrap(),
        VaultError::BelowMinTransferAmount
    );
}

#[test]
fn execute_sweep_still_moves_funds_for_amount_at_or_above_minimum() {
    let env = Env::default();
    let (owner, client, usdc_addr, vault_addr) = setup(&env, 100);
    let recipient = Address::generate(&env);
    let usdc_asset = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_asset.mint(&vault_addr, &1_000);

    client.propose_sweep(&owner, &recipient, &500);
    env.ledger()
        .set_timestamp(env.ledger().timestamp() + DEFAULT_TIMELOCK_SECONDS + 1);
    client.execute_sweep(&owner);

    let usdc = token::Client::new(&env, &usdc_addr);
    assert_eq!(usdc.balance(&recipient), 500);
    assert!(client.get_pending_sweep().is_none());
}
