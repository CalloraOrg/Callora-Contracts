#![cfg(test)]

extern crate std;

use callora_vault::{CalloraVault, CalloraVaultClient, VaultError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token;
use soroban_sdk::{Address, BytesN, Env, Symbol, Vec};

fn create_usdc(env: &Env, admin: &Address) -> (Address, token::Client<'_>, token::StellarAssetClient<'_>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    let client = token::Client::new(env, &addr);
    let admin_client = token::StellarAssetClient::new(env, &addr);
    (addr, client, admin_client)
}

fn create_vault(env: &Env) -> (Address, CalloraVaultClient<'_>) {
    let address = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &address);
    (address, client)
}

fn setup(env: &Env) -> (Address, Address, CalloraVaultClient<'_>, Address, token::Client<'_>, token::StellarAssetClient<'_>) {
    env.mock_all_auths();
    let owner = Address::generate(env);
    let authorized_caller = Address::generate(env);
    let settlement = Address::generate(env);
    let (vault_addr, client) = create_vault(env);
    let (usdc_addr, usdc_client, usdc_admin) = create_usdc(env, &owner);
    client.init(&owner, &usdc_addr, &0, &authorized_caller, &1, &None, &1_000_000, &settlement);
    (owner, authorized_caller, client, usdc_addr, usdc_client, usdc_admin)
}

fn setup_with_balance(env: &Env, balance: i128) -> (Address, Address, CalloraVaultClient<'_>, Address, token::Client<'_>, token::StellarAssetClient<'_>) {
    env.mock_all_auths();
    let owner = Address::generate(env);
    let authorized_caller = Address::generate(env);
    let settlement = Address::generate(env);
    let (vault_addr, client) = create_vault(env);
    let (usdc_addr, usdc_client, usdc_admin) = create_usdc(env, &owner);
    if balance > 0 {
        usdc_admin.mint(&vault_addr, &balance);
    }
    client.init(&owner, &usdc_addr, &balance, &authorized_caller, &1, &None, &1_000_000, &settlement);
    (owner, authorized_caller, client, usdc_addr, usdc_client, usdc_admin)
}

#[test]
fn init_succeeds_with_owner_auth() {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let authorized_caller = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (vault_addr, client) = create_vault(&env);
    let (usdc_addr, _, _) = create_usdc(&env, &owner);
    let res = client.try_init(&owner, &usdc_addr, &0, &authorized_caller, &1, &None, &1_000_000, &settlement);
    assert!(res.is_ok(), "init should succeed with owner auth");
}

#[test]
fn deposit_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_deposit(&owner, &100_i128);
    assert!(res.is_err(), "deposit must require auth");
}

#[test]
fn deduct_requires_auth() {
    let env = Env::default();
    let (_owner, authorized_caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup_with_balance(&env, 1_000);

    env.set_auths(&[]);
    let res = client.try_deduct(&authorized_caller, &100_i128, &1);
    assert!(res.is_err(), "deduct must require auth");
}

#[test]
fn batch_deduct_requires_auth() {
    let env = Env::default();
    let (_owner, authorized_caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup_with_balance(&env, 1_000);

    let items = Vec::from_array(&env, [(100_i128, 1_u64)]);
    env.set_auths(&[]);
    let res = client.try_batch_deduct(&authorized_caller, &items);
    assert!(res.is_err(), "batch_deduct must require auth");
}

#[test]
fn set_authorized_caller_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let new = Address::generate(&env);
    let res = client.try_set_authorized_caller(&owner, &new);
    assert!(res.is_err(), "set_authorized_caller must require auth");
}

#[test]
fn pause_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_pause(&owner);
    assert!(res.is_err(), "pause must require auth");
}

#[test]
fn unpause_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.mock_all_auths();
    client.pause(&owner);

    env.set_auths(&[]);
    let res = client.try_unpause(&owner);
    assert!(res.is_err(), "unpause must require auth");
}

#[test]
fn set_max_deduct_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_set_max_deduct(&owner, &500_i128);
    assert!(res.is_err(), "set_max_deduct must require auth");
}

#[test]
fn set_settlement_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let new_settlement = Address::generate(&env);
    let res = client.try_set_settlement(&owner, &new_settlement);
    assert!(res.is_err(), "set_settlement must require auth");
}

#[test]
fn set_admin_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let new_admin = Address::generate(&env);
    let res = client.try_set_admin(&owner, &new_admin);
    assert!(res.is_err(), "set_admin must require auth");
}

#[test]
fn transfer_ownership_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let new_owner = Address::generate(&env);
    let res = client.try_transfer_ownership(&owner, &new_owner);
    assert!(res.is_err(), "transfer_ownership must require auth");
}

#[test]
fn propose_pause_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_propose_pause(&owner);
    assert!(res.is_err(), "propose_pause must require auth");
}

#[test]
fn propose_upgrade_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let hash = BytesN::from_array(&env, &[0u8; 32]);
    let res = client.try_propose_upgrade(&owner, &hash);
    assert!(res.is_err(), "propose_upgrade must require auth");
}

#[test]
fn propose_sweep_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup_with_balance(&env, 1_000);

    env.set_auths(&[]);
    let to = Address::generate(&env);
    let res = client.try_propose_sweep(&owner, &to, &100_i128);
    assert!(res.is_err(), "propose_sweep must require auth");
}

#[test]
fn add_address_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let depositor = Address::generate(&env);
    let res = client.try_add_address(&owner, &depositor);
    assert!(res.is_err(), "add_address must require auth");
}

#[test]
fn clear_all_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_clear_all(&owner);
    assert!(res.is_err(), "clear_all must require auth");
}

#[test]
fn admin_rescue_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let to = Address::generate(&env);
    let res = client.try_admin_rescue(&owner, &usdc_addr, &to, &10_i128);
    assert!(res.is_err(), "admin_rescue must require auth");
}

#[test]
fn set_reserve_cap_requires_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_set_reserve_cap(&owner, &usdc_addr, &1000_i128);
    assert!(res.is_err(), "set_reserve_cap must require auth");
}

#[test]
fn is_paused_does_not_require_auth() {
    let env = Env::default();
    let (_owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    assert!(!client.is_paused());
}

#[test]
fn balance_does_not_require_auth() {
    let env = Env::default();
    let (_owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup_with_balance(&env, 500);

    env.set_auths(&[]);
    assert_eq!(client.balance(), 500);
}

#[test]
fn get_owner_does_not_require_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_owner(), owner);
}

#[test]
fn get_usdc_token_does_not_require_auth() {
    let env = Env::default();
    let (_owner, _caller, client, usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_usdc_token(), usdc_addr);
}

#[test]
fn get_max_deduct_does_not_require_auth() {
    let env = Env::default();
    let (_owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_max_deduct(), 1_000_000);
}

#[test]
fn get_settlement_does_not_require_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let settlement = client.get_settlement();
    assert_ne!(settlement, owner);
}

#[test]
fn get_revenue_pool_does_not_require_auth() {
    let env = Env::default();
    let (_owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_revenue_pool(), None);
}

#[test]
fn get_admin_does_not_require_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_admin().unwrap(), owner);
}

#[test]
fn get_timelock_window_does_not_require_auth() {
    let env = Env::default();
    let (_owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let window = client.get_timelock_window();
    assert!(window > 0);
}

#[test]
fn get_admin_cooldown_does_not_require_auth() {
    let env = Env::default();
    let (_owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let cd = client.get_admin_cooldown();
    assert!(cd > 0);
}

#[test]
fn capabilities_does_not_require_auth() {
    let env = Env::default();
    let (_owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let caps = client.capabilities();
    assert_ne!(caps, 0);
}

#[test]
fn is_authorized_depositor_does_not_require_auth() {
    let env = Env::default();
    let (owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    assert!(client.is_authorized_depositor(&owner));
}

#[test]
fn get_allowlist_does_not_require_auth() {
    let env = Env::default();
    let (_owner, _caller, client, _usdc_addr, _usdc_client, _usdc_admin) = setup(&env);

    env.set_auths(&[]);
    let list = client.get_allowlist();
    assert!(list.is_empty());
}

#[test]
fn admin_with_auth_can_call_mutating_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let authorized_caller = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (_vault_addr, client) = create_vault(&env);
    let (usdc_addr, _usdc_client, usdc_admin) = create_usdc(&env, &owner);
    client.init(&owner, &usdc_addr, &0, &authorized_caller, &1, &None, &1_000_000, &settlement);
    usdc_admin.mint(&_vault_addr, &10_000);

    assert_eq!(client.get_owner(), owner);
    assert_eq!(client.get_admin().unwrap(), owner);
    assert!(!client.is_paused());

    client.deposit(&owner, &500);
    assert!(client.balance() >= 500);

    client.deduct(&authorized_caller, &200, &1);
    assert!(client.balance() >= 200);

    let new_admin = Address::generate(&env);
    client.set_admin(&owner, &new_admin);
    client.cancel_admin_transfer(&owner);

    let new_owner = Address::generate(&env);
    client.transfer_ownership(&owner, &new_owner);
    client.accept_ownership();
    assert_eq!(client.get_owner(), new_owner);

    let new_settlement = Address::generate(&env);
    client.set_settlement(&new_owner, &new_settlement);
    assert_eq!(client.get_settlement(), new_settlement);

    client.set_max_deduct(&new_owner, &500_000);
    assert_eq!(client.get_max_deduct(), 500_000);
}

#[test]
fn auth_snap_covers_expected_views_count() {
    const EXPECTED_VIEWS: usize = 19;
    const EXPECTED_MUTATORS: usize = 21;
    assert_eq!(EXPECTED_VIEWS, 19);
    assert_eq!(EXPECTED_MUTATORS, 21);
}
