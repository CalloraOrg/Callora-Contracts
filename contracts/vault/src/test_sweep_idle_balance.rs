/// Tests for the `sweep_idle_balance` entrypoint.
///
/// Covers:
/// - Owner-only auth enforcement
/// - Blocked when paused
/// - Settlement destination not configured
/// - Revenue pool destination not configured
/// - Partial sweep to settlement
/// - Partial sweep to revenue pool
/// - Zero-amount rejection
/// - Amount equal to full balance (drain)
/// - Amount exceeds balance rejection
/// - Event shape verification
/// - Multi-sweep balance consistency
extern crate std;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, IntoVal};
use super::*;

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
    use callora_settlement::CalloraSettlement;
    let settlement_address = env.register(CalloraSettlement, ());
    let settlement_client =
        callora_settlement::CalloraSettlementClient::new(env, &settlement_address);
    env.mock_all_auths();
    settlement_client.init(admin, vault_address);
    settlement_address
}

#[allow(clippy::type_complexity)]
fn setup_vault(
    env: &Env,
) -> (
    Address,
    CalloraVaultClient<'_>,
    Address,
    Address,
    token::Client<'_>,
    token::StellarAssetClient<'_>,
) {
    let owner = Address::generate(env);
    let (vault_address, client) = create_vault(env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(env, &owner);
    env.mock_all_auths();
    usdc_admin.mint(&vault_address, &10_000);
    client.init(&owner, &usdc, &Some(10_000), &None, &None, &None, &None);
    (vault_address, client, owner, usdc, usdc_client, usdc_admin)
}

// ---------------------------------------------------------------------------
// Auth tests
// ---------------------------------------------------------------------------

#[test]
fn sweep_rejects_non_owner() {
    let env = Env::default();
    let (vault_address, client, owner, _usdc, _usdc_client, _usdc_admin) = setup_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    env.mock_all_auths();
    client.set_settlement(&owner, &settlement);
    let intruder = Address::generate(&env);
    env.mock_all_auths_allowing_non_root_auth();
    let result = client.try_sweep_idle_balance(&intruder, &SweepDestination::Settlement, &500);
    assert_eq!(result, Err(Ok(VaultError::Unauthorized)));
}

#[test]
#[should_panic]
fn sweep_requires_owner_auth() {
    let env = Env::default();
    let (vault_address, client, owner, _usdc, _usdc_client, _usdc_admin) = setup_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    env.mock_all_auths();
    client.set_settlement(&owner, &settlement);
    // No auth mock for the sweep call -> should panic
    client.sweep_idle_balance(&owner, &SweepDestination::Settlement, &500);
}

// ---------------------------------------------------------------------------
// Pause tests
// ---------------------------------------------------------------------------

#[test]
fn sweep_blocked_when_paused() {
    let env = Env::default();
    let (vault_address, client, owner, _usdc, _usdc_client, _usdc_admin) = setup_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    env.mock_all_auths();
    client.set_settlement(&owner, &settlement);
    client.pause(&owner);
    let result = client.try_sweep_idle_balance(&owner, &SweepDestination::Settlement, &500);
    assert_eq!(result, Err(Ok(VaultError::Paused)));
}

// ---------------------------------------------------------------------------
// Destination-not-configured tests
// ---------------------------------------------------------------------------

#[test]
fn sweep_settlement_not_configured() {
    let env = Env::default();
    let (_vault_address, client, owner, _usdc, _usdc_client, _usdc_admin) = setup_vault(&env);
    env.mock_all_auths();
    let result = client.try_sweep_idle_balance(&owner, &SweepDestination::Settlement, &500);
    assert_eq!(result, Err(Ok(VaultError::DestinationNotConfigured)));
}

#[test]
fn sweep_revenue_pool_not_configured() {
    let env = Env::default();
    let (_vault_address, client, owner, _usdc, _usdc_client, _usdc_admin) = setup_vault(&env);
    env.mock_all_auths();
    let result = client.try_sweep_idle_balance(&owner, &SweepDestination::RevenuePool, &500);
    assert_eq!(result, Err(Ok(VaultError::DestinationNotConfigured)));
}

// ---------------------------------------------------------------------------
// Amount validation tests
// ---------------------------------------------------------------------------

#[test]
fn sweep_zero_amount_rejected() {
    let env = Env::default();
    let (vault_address, client, owner, _usdc, _usdc_client, _usdc_admin) = setup_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    env.mock_all_auths();
    client.set_settlement(&owner, &settlement);
    let result = client.try_sweep_idle_balance(&owner, &SweepDestination::Settlement, &0);
    assert_eq!(result, Err(Ok(VaultError::AmountNotPositive)));
}

#[test]
fn sweep_negative_amount_rejected() {
    let env = Env::default();
    let (vault_address, client, owner, _usdc, _usdc_client, _usdc_admin) = setup_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    env.mock_all_auths();
    client.set_settlement(&owner, &settlement);
    let result = client.try_sweep_idle_balance(&owner, &SweepDestination::Settlement, &-1);
    assert_eq!(result, Err(Ok(VaultError::AmountNotPositive)));
}

#[test]
fn sweep_exceeds_balance_rejected() {
    let env = Env::default();
    let (vault_address, client, owner, _usdc, _usdc_client, _usdc_admin) = setup_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    env.mock_all_auths();
    client.set_settlement(&owner, &settlement);
    let result = client.try_sweep_idle_balance(&owner, &SweepDestination::Settlement, &10_001);
    assert_eq!(result, Err(Ok(VaultError::InsufficientBalance)));
}

// ---------------------------------------------------------------------------
// Happy-path tests
// ---------------------------------------------------------------------------

#[test]
fn sweep_partial_to_settlement() {
    let env = Env::default();
    let (vault_address, client, owner, _usdc, usdc_client, _usdc_admin) = setup_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    env.mock_all_auths();
    client.set_settlement(&owner, &settlement);
    let new_balance = client.sweep_idle_balance(&owner, &SweepDestination::Settlement, &3_000);
    assert_eq!(new_balance, 7_000);
    assert_eq!(client.balance(), 7_000);
    assert_eq!(usdc_client.balance(&settlement), 3_000);
    assert_eq!(usdc_client.balance(&vault_address), 7_000);
}

#[test]
fn sweep_partial_to_revenue_pool() {
    let env = Env::default();
    let (vault_address, client, owner, _usdc, usdc_client, _usdc_admin) = setup_vault(&env);
    let revenue_pool = Address::generate(&env);
    env.mock_all_auths();
    client.propose_revenue_pool(&Some(revenue_pool.clone()));
    client.accept_revenue_pool();
    let new_balance = client.sweep_idle_balance(&owner, &SweepDestination::RevenuePool, &2_500);
    assert_eq!(new_balance, 7_500);
    assert_eq!(client.balance(), 7_500);
    assert_eq!(usdc_client.balance(&revenue_pool), 2_500);
    assert_eq!(usdc_client.balance(&vault_address), 7_500);
}

#[test]
fn sweep_full_balance_drain() {
    let env = Env::default();
    let (vault_address, client, owner, _usdc, usdc_client, _usdc_admin) = setup_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    env.mock_all_auths();
    client.set_settlement(&owner, &settlement);
    let new_balance = client.sweep_idle_balance(&owner, &SweepDestination::Settlement, &10_000);
    assert_eq!(new_balance, 0);
    assert_eq!(client.balance(), 0);
    assert_eq!(usdc_client.balance(&settlement), 10_000);
    assert_eq!(usdc_client.balance(&vault_address), 0);
}

#[test]
fn sweep_emits_event() {
    extern crate std;
    use soroban_sdk::testutils::Events as _;
    let env = Env::default();
    let (vault_address, client, owner, _usdc, _usdc_client, _usdc_admin) = setup_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    env.mock_all_auths();
    client.set_settlement(&owner, &settlement);
    client.sweep_idle_balance(&owner, &SweepDestination::Settlement, &1_000);
    let events = env.events().all();
    let swept_event = events
        .iter()
        .find(|(contract, topics, _)| {
            if *contract != vault_address {
                return false;
            }
            if topics.len() < 1 {
                return false;
            }
            let t0: soroban_sdk::Symbol = topics.get(0).unwrap().into_val(&env);
            t0 == soroban_sdk::Symbol::new(&env, "swept")
        })
        .expect("swept event not found");
    let data: SweptEventData = swept_event.2.into_val(&env);
    assert_eq!(data.destination, SweepDestination::Settlement);
    assert_eq!(data.amount, 1_000);
    assert_eq!(data.new_balance, 9_000);
}

#[test]
fn sweep_balance_consistency_after_multiple_sweeps() {
    let env = Env::default();
    let (vault_address, client, owner, _usdc, usdc_client, _usdc_admin) = setup_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    env.mock_all_auths();
    client.set_settlement(&owner, &settlement);
    client.sweep_idle_balance(&owner, &SweepDestination::Settlement, &2_000);
    client.sweep_idle_balance(&owner, &SweepDestination::Settlement, &3_000);
    assert_eq!(client.balance(), 5_000);
    assert_eq!(usdc_client.balance(&vault_address), 5_000);
    assert_eq!(usdc_client.balance(&settlement), 5_000);
}
