#![cfg(test)]

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env};

use super::*;

fn create_usdc<'a>(env: &'a Env, admin: &Address) -> (Address, token::StellarAssetClient<'a>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (addr.clone(), token::StellarAssetClient::new(env, &addr))
}

fn setup(env: &Env) -> (Address, CalloraVaultClient<'_>, Address) {
    let owner = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_addr);

    let (usdc, _usdc_client) = create_usdc(env, &owner);
    env.mock_all_auths();

    client.init(
        &owner,
        &usdc,
        &0,
        &owner,
        &1,
        &None,
        &1000,
        &Address::generate(env),
    );

    (owner, client, usdc)
}

#[test]
fn test_sweep_idle_balance() {
    let env = Env::default();
    let (owner, client, usdc_addr) = setup(&env);

    let usdc = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc.mint(&client.address, &1000); // simulate 1000 on ledger

    // Tracked balance is 0 initially.
    let preview = client.dry_run_sweep_idle_balance().unwrap();
    assert_eq!(preview.on_ledger_balance, 1000);
    assert_eq!(preview.tracked_balance, 0);
    assert_eq!(preview.idle_balance, 1000);
    assert!(preview.has_idle);

    // Deposit 500
    usdc.mint(&owner, &500);
    client.deposit(&owner, &500); // this increases tracked balance to 500, on_ledger to 1500

    let preview2 = client.dry_run_sweep_idle_balance();
    assert_eq!(preview2.on_ledger_balance, 1500);
    assert_eq!(preview2.tracked_balance, 500);
    assert_eq!(preview2.idle_balance, 1000);
    assert!(preview2.has_idle);
}
