//! Focused tests for the public entrypoints documented in `lib.rs`.
//!
//! Each test section maps to one or more rustdoc comments and verifies the
//! claims made there (authorization, parameter constraints, return values).

#![cfg(test)]

extern crate std;

use soroban_sdk::{testutils::Address as _, token, Address, BytesN, Env, Vec};

use crate::*;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

fn create_usdc<'a>(env: &'a Env, admin: &Address) -> (Address, token::StellarAssetClient<'a>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (addr.clone(), token::StellarAssetClient::new(env, &addr))
}

/// Build a fully initialized vault.  Returns (owner, client, usdc_addr, settlement_addr).
fn setup(env: &Env) -> (Address, CalloraVaultClient<'_>, Address, Address) {
    let owner = Address::generate(env);
    let settlement = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_addr);
    let (usdc, _) = create_usdc(env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &0, &owner, &1, &None, &10_000, &settlement);
    (owner, client, usdc, settlement)
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

/// Docs claim: init sets the owner, USDC token, balance, and paused=false.
#[test]
fn test_init_sets_state() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (usdc, _) = create_usdc(&env, &owner);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &vault_addr);

    client.init(&owner, &usdc, &0, &owner, &1, &None, &10_000, &settlement);

    assert_eq!(client.get_owner(), owner);
    assert_eq!(client.get_usdc_token(), usdc);
    assert_eq!(client.balance(), 0);
    assert!(!client.is_paused());
}

/// Docs claim: init may only succeed once.
#[test]
#[should_panic]
fn test_init_rejects_double_init() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (usdc, _) = create_usdc(&env, &owner);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &vault_addr);

    client.init(&owner, &usdc, &0, &owner, &1, &None, &10_000, &settlement);
    client.init(&owner, &usdc, &0, &owner, &1, &None, &10_000, &settlement); // must panic
}

/// Docs claim: init panics if min_deposit > max_deduct.
#[test]
#[should_panic]
fn test_init_rejects_min_deposit_exceeds_max_deduct() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (usdc, _) = create_usdc(&env, &owner);
    let client = CalloraVaultClient::new(&env, &env.register(CalloraVault, ()));
    client.init(&owner, &usdc, &0, &owner, &1_000, &None, &100, &settlement);
}

// ---------------------------------------------------------------------------
// deposit
// ---------------------------------------------------------------------------

/// Docs claim: deposit increments tracked balance and requires caller auth.
#[test]
fn test_deposit_increments_balance() {
    let env = Env::default();
    let (owner, client, usdc, _) = setup(&env);
    let usdc_client = token::StellarAssetClient::new(&env, &usdc);
    env.mock_all_auths();
    usdc_client.mint(&owner, &500);

    client.deposit(&owner, &500);
    assert_eq!(client.balance(), 500);
}

/// Docs claim: deposit is blocked when the vault is paused.
#[test]
fn test_deposit_blocked_while_paused() {
    let env = Env::default();
    let (owner, client, usdc, _) = setup(&env);
    let usdc_client = token::StellarAssetClient::new(&env, &usdc);
    env.mock_all_auths();
    usdc_client.mint(&owner, &500);
    client.pause(&owner);

    assert!(client.try_deposit(&owner, &500).is_err());
}

/// Docs claim: only owner or allowlisted depositor can deposit.
#[test]
fn test_deposit_requires_auth() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    let stranger = Address::generate(&env);

    env.set_auths(&[]);
    assert!(client.try_deposit(&stranger, &100).is_err());
}

// ---------------------------------------------------------------------------
// deduct
// ---------------------------------------------------------------------------

/// Docs claim: deduct requires caller to be the authorized caller.
#[test]
fn test_deduct_requires_authorized_caller() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    let stranger = Address::generate(&env);
    env.set_auths(&[]);
    assert!(client.try_deduct(&stranger, &10, &1u64).is_err());
}

/// Docs claim: deduct is blocked while paused.
#[test]
fn test_deduct_blocked_while_paused() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    env.mock_all_auths();
    client.pause(&owner);
    assert!(client.try_deduct(&owner, &10, &1u64).is_err());
}

// ---------------------------------------------------------------------------
// batch_deduct
// ---------------------------------------------------------------------------

/// Docs claim: batch_deduct requires caller auth and is blocked while paused.
#[test]
fn test_batch_deduct_requires_auth_and_unpaused() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    let mut items = Vec::new(&env);
    items.push_back((10i128, 1u64));

    // auth check
    env.set_auths(&[]);
    assert!(client.try_batch_deduct(&owner, &items).is_err());

    // pause check
    env.mock_all_auths();
    client.pause(&owner);
    assert!(client.try_batch_deduct(&owner, &items).is_err());
}

// ---------------------------------------------------------------------------
// set_authorized_caller
// ---------------------------------------------------------------------------

/// Docs claim: only owner can change authorized caller; auth is enforced.
#[test]
fn test_set_authorized_caller_requires_owner_auth() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    let new_caller = Address::generate(&env);
    env.set_auths(&[]);
    assert!(client.try_set_authorized_caller(&new_caller).is_err());
}

// ---------------------------------------------------------------------------
// pause / unpause / is_paused
// ---------------------------------------------------------------------------

/// Docs claim: pause sets the flag; unpause clears it; is_paused reads it.
#[test]
fn test_pause_unpause_cycle() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    env.mock_all_auths();

    assert!(!client.is_paused());
    client.pause(&owner);
    assert!(client.is_paused());
    client.unpause(&owner);
    assert!(!client.is_paused());
}

/// Docs claim: pause requires owner auth.
#[test]
fn test_pause_requires_owner_auth() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    env.set_auths(&[]);
    assert!(client.try_pause(&owner).is_err());
}

/// Docs claim: unpause requires owner auth.
#[test]
fn test_unpause_requires_owner_auth() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    env.mock_all_auths();
    client.pause(&owner);
    env.set_auths(&[]);
    assert!(client.try_unpause(&owner).is_err());
}

// ---------------------------------------------------------------------------
// balance / get_owner / get_usdc_token
// ---------------------------------------------------------------------------

/// Docs claim: balance returns the tracked amount (not on-ledger).
#[test]
fn test_balance_is_tracked() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    assert_eq!(client.balance(), 0);
}

/// Docs claim: get_owner returns the owner set at init.
#[test]
fn test_get_owner_returns_owner() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    assert_eq!(client.get_owner(), owner);
}

/// Docs claim: get_usdc_token returns the USDC address set at init.
#[test]
fn test_get_usdc_token_returns_usdc() {
    let env = Env::default();
    let (_, client, usdc, _) = setup(&env);
    assert_eq!(client.get_usdc_token(), usdc);
}

// ---------------------------------------------------------------------------
// get_max_deduct / set_max_deduct
// ---------------------------------------------------------------------------

/// Docs claim: get_max_deduct returns the configured limit.
#[test]
fn test_get_max_deduct_returns_limit() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    assert_eq!(client.get_max_deduct(), 10_000);
}

/// Docs claim: set_max_deduct requires owner auth and must be positive.
#[test]
fn test_set_max_deduct_requires_owner() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    env.set_auths(&[]);
    let stranger = Address::generate(&env);
    assert!(client.try_set_max_deduct(&stranger, &500).is_err());
}

#[test]
#[should_panic]
fn test_set_max_deduct_rejects_zero() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    env.mock_all_auths();
    client.set_max_deduct(&owner, &0);
}

// ---------------------------------------------------------------------------
// get_settlement / set_settlement
// ---------------------------------------------------------------------------

/// Docs claim: get_settlement returns the settlement address.
#[test]
fn test_get_settlement_returns_address() {
    let env = Env::default();
    let (_, client, _, settlement) = setup(&env);
    assert_eq!(client.get_settlement(), settlement);
}

/// Docs claim: set_settlement requires owner auth.
#[test]
fn test_set_settlement_requires_owner() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    env.set_auths(&[]);
    let new_settle = Address::generate(&env);
    let stranger = Address::generate(&env);
    assert!(client.try_set_settlement(&stranger, &new_settle).is_err());
}

// ---------------------------------------------------------------------------
// get_revenue_pool
// ---------------------------------------------------------------------------

/// Docs claim: get_revenue_pool returns None when not configured.
#[test]
fn test_get_revenue_pool_none_by_default() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    assert!(client.get_revenue_pool().is_none());
}

// ---------------------------------------------------------------------------
// is_authorized_depositor
// ---------------------------------------------------------------------------

/// Docs claim: owner is not in allowlist by default (allowlist is separate
/// from the owner-implicit-permission check inside deposit).
#[test]
fn test_is_authorized_depositor_defaults_false_for_stranger() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    let stranger = Address::generate(&env);
    assert!(!client.is_authorized_depositor(&stranger));
}

// ---------------------------------------------------------------------------
// get_pending_pause / get_pending_upgrade / get_pending_sweep
// ---------------------------------------------------------------------------

/// Docs claim: pending proposal views return None when nothing is staged.
#[test]
fn test_pending_proposals_none_initially() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    assert!(client.get_pending_pause().is_none());
    assert!(client.get_pending_upgrade().is_none());
    assert!(client.get_pending_sweep().is_none());
}

/// Docs claim: get_pending_pause returns Some after propose_pause.
#[test]
fn test_get_pending_pause_some_after_proposal() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    env.mock_all_auths();
    client.propose_pause(&owner);
    assert!(client.get_pending_pause().is_some());
}

/// Docs claim: get_pending_upgrade returns Some after propose_upgrade.
#[test]
fn test_get_pending_upgrade_some_after_proposal() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    env.mock_all_auths();
    let hash = BytesN::from_array(&env, &[1u8; 32]);
    client.propose_upgrade(&owner, &hash);
    assert!(client.get_pending_upgrade().is_some());
}

/// Docs claim: get_pending_sweep returns Some after propose_sweep.
#[test]
fn test_get_pending_sweep_some_after_proposal() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    env.mock_all_auths();
    let recipient = Address::generate(&env);
    client.propose_sweep(&owner, &recipient, &100);
    assert!(client.get_pending_sweep().is_some());
}

// ---------------------------------------------------------------------------
// get_admin / set_admin / accept_admin
// ---------------------------------------------------------------------------

/// Docs claim: get_admin defaults to owner at init.
#[test]
fn test_get_admin_defaults_to_owner() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    assert_eq!(client.get_admin(), owner);
}

/// Docs claim: two-step admin transfer (set_admin then accept_admin).
#[test]
fn test_admin_two_step_transfer() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    let new_admin = Address::generate(&env);
    env.mock_all_auths();

    client.set_admin(&owner, &new_admin);
    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);
}

/// Docs claim: accept_admin fails when no transfer is pending.
#[test]
fn test_accept_admin_no_pending_fails() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    env.mock_all_auths();
    assert!(client.try_accept_admin().is_err());
}

// ---------------------------------------------------------------------------
// set_reserve_cap / get_reserve_cap
// ---------------------------------------------------------------------------

/// Docs claim: get_reserve_cap returns i128::MAX when not configured.
#[test]
fn test_get_reserve_cap_default_unlimited() {
    let env = Env::default();
    let (_, client, usdc, _) = setup(&env);
    assert_eq!(client.get_reserve_cap(&usdc), i128::MAX);
}

/// Docs claim: set_reserve_cap requires owner auth; get_reserve_cap reflects it.
#[test]
fn test_set_reserve_cap_updates_limit() {
    let env = Env::default();
    let (owner, client, usdc, _) = setup(&env);
    env.mock_all_auths();
    client.set_reserve_cap(&owner, &usdc, &5_000);
    assert_eq!(client.get_reserve_cap(&usdc), 5_000);
}

/// Docs claim: set_reserve_cap requires owner auth.
#[test]
fn test_set_reserve_cap_requires_owner() {
    let env = Env::default();
    let (_, client, usdc, _) = setup(&env);
    env.set_auths(&[]);
    let stranger = Address::generate(&env);
    assert!(client
        .try_set_reserve_cap(&stranger, &usdc, &1_000)
        .is_err());
}

// ---------------------------------------------------------------------------
// capabilities
// ---------------------------------------------------------------------------

/// Docs claim: capabilities returns a non-zero bitmap; no auth required.
#[test]
fn test_capabilities_nonzero() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    assert_ne!(client.capabilities(), 0);
}

// ---------------------------------------------------------------------------
// timelock window
// ---------------------------------------------------------------------------

/// Docs claim: set_timelock_window requires admin auth and bounds are enforced.
#[test]
fn test_set_timelock_window_requires_admin() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    env.set_auths(&[]);
    let stranger = Address::generate(&env);
    assert!(client
        .try_set_timelock_window(&stranger, &86_400u64)
        .is_err());
}

#[test]
fn test_get_timelock_window_default() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    assert_eq!(
        client.get_timelock_window(),
        timelock::DEFAULT_TIMELOCK_SECONDS
    );
}

#[test]
fn test_set_and_get_timelock_window() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    env.mock_all_auths();
    client.set_timelock_window(&owner, &7_200u64);
    assert_eq!(client.get_timelock_window(), 7_200u64);
}

// ---------------------------------------------------------------------------
// propose / cancel pause flow (no timelock expiry needed for cancel)
// ---------------------------------------------------------------------------

/// Docs claim: cancel_pause clears the proposal regardless.
#[test]
fn test_cancel_pause_clears_proposal() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    env.mock_all_auths();
    client.propose_pause(&owner);
    assert!(client.get_pending_pause().is_some());
    client.cancel_pause(&owner);
    assert!(client.get_pending_pause().is_none());
}

/// Docs claim: cancel_upgrade clears the proposal.
#[test]
fn test_cancel_upgrade_clears_proposal() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    env.mock_all_auths();
    let hash = BytesN::from_array(&env, &[2u8; 32]);
    client.propose_upgrade(&owner, &hash);
    assert!(client.get_pending_upgrade().is_some());
    client.cancel_upgrade(&owner);
    assert!(client.get_pending_upgrade().is_none());
}

/// Docs claim: cancel_sweep clears the proposal.
#[test]
fn test_cancel_sweep_clears_proposal() {
    let env = Env::default();
    let (owner, client, _, _) = setup(&env);
    env.mock_all_auths();
    let recipient = Address::generate(&env);
    client.propose_sweep(&owner, &recipient, &50);
    assert!(client.get_pending_sweep().is_some());
    client.cancel_sweep(&owner);
    assert!(client.get_pending_sweep().is_none());
}

// ---------------------------------------------------------------------------
// dry_run_sweep_idle_balance
// ---------------------------------------------------------------------------

/// Docs claim: returns idle=0 when on-ledger balance == tracked balance.
#[test]
fn test_dry_run_sweep_no_idle() {
    let env = Env::default();
    let (_, client, _, _) = setup(&env);
    // tracked=0, on-ledger=0
    let preview = client.dry_run_sweep_idle_balance();
    assert_eq!(preview.idle_balance, 0);
    assert!(!preview.has_idle);
}

/// Docs claim: idle_balance == on_ledger - tracked when on_ledger > tracked.
#[test]
fn test_dry_run_sweep_detects_idle() {
    let env = Env::default();
    let (_, client, usdc, _) = setup(&env);
    let usdc_client = token::StellarAssetClient::new(&env, &usdc);
    env.mock_all_auths();
    usdc_client.mint(&client.address, &2_000); // surplus bypassing deposit

    let preview = client.dry_run_sweep_idle_balance();
    assert_eq!(preview.on_ledger_balance, 2_000);
    assert_eq!(preview.tracked_balance, 0);
    assert_eq!(preview.idle_balance, 2_000);
    assert!(preview.has_idle);
}
