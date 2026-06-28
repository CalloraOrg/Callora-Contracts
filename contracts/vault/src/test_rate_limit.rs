use soroban_sdk::{testutils::Address as _, Address, Env, Symbol};
use crate::{CalloraVault, CalloraVaultClient, DeductItem, VaultError};

fn create_vault(env: &Env) -> (Address, CalloraVaultClient) {
    let contract_id = env.register_contract(None, CalloraVault);
    let client = CalloraVaultClient::new(env, &contract_id);
    (contract_id, client)
}

#[test]
fn rate_limit_bucket_enforcement() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let developer = Address::generate(&env);
    
    let (vault_address, client) = create_vault(&env);
    
    // Setup vault basics
    // We mock auths to bypass init dependencies setup (or we can use standard setup but this is isolated)
    env.mock_all_auths();
    
    let usdc = Address::generate(&env);
    client.init(&owner, &usdc, &None, &Some(caller.clone()), &None, &None, &None);
    let settlement = Address::generate(&env);
    client.set_settlement(&owner, &settlement);
    
    // Set up rate limit config
    // capacity: 100, refill_rate: 10 per ledger
    client.set_developer_rate_limit(&owner, &developer, &100, &10);
    
    // Try to deduct more than capacity -> fails
    let res = client.try_deduct(&caller, &150, &None, &u16::MAX, &developer);
    assert_eq!(res.unwrap_err().unwrap(), VaultError::RateLimited);
    
    // We cannot deduct immediately if we don't have balance in vault, but since usdc isn't mocked properly,
    // actually testing full deduct flow requires the real USDC token in tests.
    // Let's rely on standard test setup used in test.rs if we want full integration test.
}
