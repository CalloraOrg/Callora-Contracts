#![cfg(test)]
extern crate std;
use super::*;
use soroban_sdk::{testutils::Address as _, Address, Env, Vec, token};

#[test]
fn batch_distribute_success() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);
    
    client.init(&admin, &usdc_addr);
    let usdc = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc.mint(&contract_addr, &1000);
    
    let mut payments = Vec::new(&env);
    let to1 = Address::generate(&env);
    let to2 = Address::generate(&env);
    payments.push_back(PaymentLeg { to: to1.clone(), amount: 100 });
    payments.push_back(PaymentLeg { to: to2.clone(), amount: 200 });
    
    client.batch_distribute(&admin, &payments);
    
    let token_client = token::Client::new(&env, &usdc_addr);
    assert_eq!(token_client.balance(&to1), 100);
    assert_eq!(token_client.balance(&to2), 200);
    assert_eq!(token_client.balance(&contract_addr), 700);
}

#[test]
#[should_panic(expected = "duplicate recipient in batch")]
fn batch_distribute_rejects_duplicates() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);
    
    client.init(&admin, &usdc_addr);
    let usdc = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc.mint(&contract_addr, &1000);
    
    let mut payments = Vec::new(&env);
    let to1 = Address::generate(&env);
    payments.push_back(PaymentLeg { to: to1.clone(), amount: 100 });
    payments.push_back(PaymentLeg { to: to1.clone(), amount: 200 });
    
    client.batch_distribute(&admin, &payments);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn batch_distribute_rejects_zero_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);
    
    client.init(&admin, &usdc_addr);
    let usdc = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc.mint(&contract_addr, &1000);
    
    let mut payments = Vec::new(&env);
    let to1 = Address::generate(&env);
    payments.push_back(PaymentLeg { to: to1.clone(), amount: 0 });
    
    client.batch_distribute(&admin, &payments);
}
