#![cfg(test)]

use crate::{RefundContract, RefundContractClient, RefundError, RefundStatus};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{Address, Env, Symbol};

fn setup() -> (Env, Address, RefundContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.init(&admin, &250, &100);
    (env, admin, client)
}

#[test]
fn test_init() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    client.init(&admin, &250, &100);

    let config = client.get_config();
    assert_eq!(config.fee_bps, 250);
    assert_eq!(config.min_refund_amount, 100);

    let stored_admin = client.get_admin();
    assert_eq!(stored_admin, admin);
}

#[test]
fn test_init_already_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    client.init(&admin, &250, &100);
    let result = client.try_init(&admin, &250, &100);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::AlreadyInitialized);
}

#[test]
fn test_init_fee_too_high() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let result = client.try_init(&admin, &10_001, &100);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::FeeTooHigh);
}

#[test]
fn test_init_invalid_min_amount() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    let result = client.try_init(&admin, &250, &-1);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::InvalidAmount);
}

#[test]
fn test_request_refund() {
    let (env, _admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));

    assert_eq!(request_id, 1);

    let request = client.get_refund_request(&request_id);
    assert_eq!(request.id, 1);
    assert_eq!(request.requester, requester);
    assert_eq!(request.token, token);
    assert_eq!(request.amount, 500);
    assert_eq!(request.status, RefundStatus::Pending);
}

#[test]
fn test_request_refund_amount_too_low() {
    let (env, _admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    // Min amount is 100, trying 50
    let result = client.try_request_refund(&requester, &token, &50, &Symbol::new(&env, "test"));
    assert_eq!(result.unwrap_err().unwrap(), RefundError::AmountTooLow);
}

#[test]
fn test_request_refund_invalid_amount() {
    let (env, _admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    let result = client.try_request_refund(&requester, &token, &0, &Symbol::new(&env, "test"));
    assert_eq!(result.unwrap_err().unwrap(), RefundError::InvalidAmount);

    let result = client.try_request_refund(&requester, &token, &-100, &Symbol::new(&env, "test"));
    assert_eq!(result.unwrap_err().unwrap(), RefundError::InvalidAmount);
}

#[test]
fn test_approve_refund() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));

    client.approve_refund(&admin, &request_id);

    let request = client.get_refund_request(&request_id);
    assert_eq!(request.status, RefundStatus::Approved);
}

#[test]
fn test_approve_refund_unauthorized() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);
    let fake_admin = Address::generate(&env);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));

    let result = client.try_approve_refund(&fake_admin, &request_id);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::Unauthorized);
}

#[test]
fn test_approve_refund_not_found() {
    let (env, admin, client) = setup();

    let result = client.try_approve_refund(&admin, &999);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::NotFound);
}

#[test]
fn test_approve_refund_invalid_status() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    client.approve_refund(&admin, &request_id);

    // Try to approve again
    let result = client.try_approve_refund(&admin, &request_id);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::InvalidStatus);
}

#[test]
fn test_reject_refund() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));

    client.reject_refund(&admin, &request_id);

    let request = client.get_refund_request(&request_id);
    assert_eq!(request.status, RefundStatus::Rejected);
    assert!(request.processed_at.is_some());
}

#[test]
fn test_reject_refund_unauthorized() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);
    let fake_admin = Address::generate(&env);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));

    let result = client.try_reject_refund(&fake_admin, &request_id);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::Unauthorized);
}

#[test]
fn test_process_refund() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    client.approve_refund(&admin, &request_id);

    client.process_refund(&admin, &request_id);

    let request = client.get_refund_request(&request_id);
    assert_eq!(request.status, RefundStatus::Processed);
    assert!(request.processed_at.is_some());

    let total = client.get_total_refunds();
    assert_eq!(total, 500);
}

#[test]
fn test_process_refund_invalid_status() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    // Not approved yet - should fail

    let result = client.try_process_refund(&admin, &request_id);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::InvalidStatus);
}

#[test]
fn test_update_config() {
    let (env, admin, client) = setup();

    client.update_config(&admin, &500, &200);

    let config = client.get_config();
    assert_eq!(config.fee_bps, 500);
    assert_eq!(config.min_refund_amount, 200);
}

#[test]
fn test_update_config_unauthorized() {
    let (env, admin, client) = setup();
    let fake_admin = Address::generate(&env);

    let result = client.try_update_config(&fake_admin, &500, &200);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::Unauthorized);
}

#[test]
fn test_update_config_fee_too_high() {
    let (env, admin, client) = setup();

    let result = client.try_update_config(&admin, &10_001, &200);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::FeeTooHigh);
}

#[test]
fn test_update_config_invalid_amount() {
    let (env, admin, client) = setup();

    let result = client.try_update_config(&admin, &500, &-1);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::InvalidAmount);
}

#[test]
fn test_get_total_refunds() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    client.approve_refund(&admin, &request_id);
    client.process_refund(&admin, &request_id);

    let total = client.get_total_refunds();
    assert_eq!(total, 500);
}

#[test]
fn test_get_refund_counter() {
    let (env, _admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    client.request_refund(&requester, &token, &300, &Symbol::new(&env, "test2"));

    let counter = client.get_refund_counter();
    assert_eq!(counter, 2);
}