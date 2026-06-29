extern crate std;

use crate::{CalloraSettlement, CalloraSettlementClient, SettlementError, StorageKey, timelock};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{token, Address, Env};

fn create_token<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract_address.address();
    let client = token::Client::new(env, &address);
    let admin_client = token::StellarAssetClient::new(env, &address);
    (address, client, admin_client)
}

#[test]
fn test_balance_migration_happy_path() {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(1_000_000);

    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    let from = Address::generate(&env);
    let to = Address::generate(&env);
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &addr);

    let (token, _, _) = create_token(&env, &admin);

    client.init(&admin, &vault);
    client.set_usdc_token(&admin, &token);

    // Initial balance for source
    client.receive_payment(&vault, &500i128, &false, &Some(from.clone()), &token);
    assert_eq!(client.get_developer_balance(&from, &token), 500);

    // Propose
    client.propose_balance_migration(&admin, &from, &to);
    let pending = client.get_balance_migration(&from).unwrap();
    assert_eq!(pending.from, from);
    assert_eq!(pending.to, to);
    assert_eq!(pending.amount, 500);
    assert_eq!(pending.execute_after, 1_000_000 + timelock::DEVELOPER_MIGRATION_TIMELOCK_SECONDS);

    // Advance time
    env.ledger().set_timestamp(pending.execute_after);

    // Execute
    client.execute_balance_migration(&admin, &from);

    assert_eq!(client.get_developer_balance(&from, &token), 0);
    assert_eq!(client.get_developer_balance(&to, &token), 500);
}
