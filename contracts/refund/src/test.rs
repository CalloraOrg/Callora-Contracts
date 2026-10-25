#![cfg(test)]

use crate::{
    FeesWithdrawnEvent, InitializedEvent, RefundConfigUpdatedEvent, RefundContract,
    RefundContractClient, RefundError, RefundProcessedEvent, RefundRequestedEvent, RefundStatus,
    MAX_REQUESTER_REFUNDS,
};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env, IntoVal, Symbol};

fn create_token<'a>(
    env: &'a Env,
    admin: &Address,
) -> (
    Address,
    soroban_sdk::token::Client<'a>,
    soroban_sdk::token::StellarAssetClient<'a>,
) {
    let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract_address.address();
    let client = soroban_sdk::token::Client::new(env, &address);
    let admin_client = soroban_sdk::token::StellarAssetClient::new(env, &address);
    (address, client, admin_client)
}

fn has_first_topic(env: &Env, topics: &soroban_sdk::Vec<soroban_sdk::Val>, expected: &str) -> bool {
    if let Some(t0) = topics.get(0) {
        let sym: Symbol = t0.into_val(env);
        sym == Symbol::new(env, expected)
    } else {
        false
    }
}

fn setup() -> (Env, Address, RefundContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.init(&admin, &250, &100);
    (env, admin, client)
}

fn setup_with_config(
    fee_bps: u32,
    min_refund_amount: i128,
) -> (Env, Address, RefundContractClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    client.init(&admin, &fee_bps, &min_refund_amount);
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
    assert_eq!(
        result.unwrap_err().unwrap(),
        RefundError::AlreadyInitialized
    );
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
fn test_requester_refund_index_is_paginated_in_creation_order() {
    let (env, _admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    let first = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "one"));
    let second = client.request_refund(&requester, &token, &600, &Symbol::new(&env, "two"));
    let third = client.request_refund(&requester, &token, &700, &Symbol::new(&env, "three"));

    let first_page = client.get_refunds_by_requester(&requester, &0, &2);
    assert_eq!(first_page.len(), 2);
    assert_eq!(first_page.get(0).unwrap(), first);
    assert_eq!(first_page.get(1).unwrap(), second);

    let second_page = client.get_refunds_by_requester(&requester, &2, &50);
    assert_eq!(second_page.len(), 1);
    assert_eq!(second_page.get(0).unwrap(), third);
}

#[test]
fn test_requester_refund_index_is_bounded() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    for i in 0..MAX_REQUESTER_REFUNDS {
        let id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
        if i == 0 {
            client.reject_refund(&admin, &id);
        }
    }

    client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));

    let oversized_limit = MAX_REQUESTER_REFUNDS + 1;
    let page = client.get_refunds_by_requester(&requester, &0, &oversized_limit);
    assert_eq!(
        page.len(),
        MAX_REQUESTER_REFUNDS.min(crate::MAX_REQUESTER_REFUNDS_PAGE_SIZE)
    );
    assert_eq!(page.get(0).unwrap(), 2);
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
    let (env, _admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);
    let fake_admin = Address::generate(&env);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));

    let result = client.try_approve_refund(&fake_admin, &request_id);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::Unauthorized);
}

#[test]
fn test_approve_refund_not_found() {
    let (_env, admin, client) = setup();

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
    let (env, _admin, client) = setup();
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
    let token_admin = Address::generate(&env);
    let (token, token_client, token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);

    // Fund refund contract with tokens
    token_admin_client.mint(&client.address, &10_000);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    client.approve_refund(&admin, &request_id);

    client.process_refund(&admin, &request_id);

    let request = client.get_refund_request(&request_id);
    assert_eq!(request.status, RefundStatus::Processed);
    assert!(request.processed_at.is_some());

    // fee = 500 * 250 / 10000 = 12
    // net = 500 - 12 = 488
    assert_eq!(token_client.balance(&requester), 488);
    assert_eq!(client.get_accumulated_fees(&token), 12);
    assert_eq!(client.get_total_refunds_for(&token), 500);

    #[allow(deprecated)]
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
fn test_process_refund_unauthorized() {
    let (env, _admin, client) = setup();
    let fake_admin = Address::generate(&env);
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));

    let result = client.try_process_refund(&fake_admin, &request_id);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::Unauthorized);
}

#[test]
fn test_process_refund_not_found() {
    let (_env, admin, client) = setup();

    let result = client.try_process_refund(&admin, &999);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::NotFound);
}

#[test]
fn test_update_config() {
    let (_env, admin, client) = setup();

    client.update_config(&admin, &500, &200);

    let config = client.get_config();
    assert_eq!(config.fee_bps, 500);
    assert_eq!(config.min_refund_amount, 200);
}

#[test]
fn test_update_config_unauthorized() {
    let (env, _admin, client) = setup();
    let fake_admin = Address::generate(&env);

    let result = client.try_update_config(&fake_admin, &500, &200);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::Unauthorized);
}

#[test]
fn test_update_config_fee_too_high() {
    let (_env, admin, client) = setup();

    let result = client.try_update_config(&admin, &10_001, &200);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::FeeTooHigh);
}

#[test]
fn test_update_config_invalid_amount() {
    let (_env, admin, client) = setup();

    let result = client.try_update_config(&admin, &500, &-1);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::InvalidAmount);
}

#[test]
fn test_get_total_refunds() {
    let (env, admin, client) = setup();
    let token_admin = Address::generate(&env);
    let (token, _token_client, token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);

    token_admin_client.mint(&client.address, &10_000);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    client.approve_refund(&admin, &request_id);
    client.process_refund(&admin, &request_id);

    #[allow(deprecated)]
    let total = client.get_total_refunds();
    assert_eq!(total, 500);
    assert_eq!(client.get_total_refunds_for(&token), 500);
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

#[test]
fn test_initialized_event_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    client.init(&admin, &250, &100);

    let events = env.events().all();
    let event = events.last().unwrap();

    let topics = &event.1;
    assert_eq!(topics.len(), 1);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "initialized"));

    let data: InitializedEvent = event.2.into_val(&env);
    assert_eq!(data.admin, admin);
    assert_eq!(data.fee_bps, 250);
    assert_eq!(data.min_refund_amount, 100);
}

#[test]
fn test_refund_requested_event_shape() {
    let (env, _admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);
    let reason = Symbol::new(&env, "test");

    let request_id = client.request_refund(&requester, &token, &500, &reason);

    let events = env.events().all();
    let event = events.last().unwrap();

    let topics = &event.1;
    assert_eq!(topics.len(), 1);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "refund_requested"));

    let data: RefundRequestedEvent = event.2.into_val(&env);
    assert_eq!(data.request_id, request_id);
    assert_eq!(data.requester, requester);
    assert_eq!(data.token, token);
    assert_eq!(data.amount, 500);
    assert_eq!(data.reason, reason);
}

#[test]
fn test_refund_processed_event_shape_for_each_transition() {
    let (env, admin, client) = setup();
    let token_admin = Address::generate(&env);
    let (token, _token_client, token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);

    token_admin_client.mint(&client.address, &10_000);

    let request_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));

    client.approve_refund(&admin, &request_id);
    let approved: RefundProcessedEvent = env.events().all().last().unwrap().2.into_val(&env);
    assert_eq!(approved.request_id, request_id);
    assert_eq!(approved.processor, admin);
    assert_eq!(approved.amount, 500);
    assert_eq!(approved.token, token);
    assert_eq!(approved.fee, 0);
    assert_eq!(approved.net, 0);
    assert_eq!(approved.status, RefundStatus::Approved);

    client.process_refund(&admin, &request_id);
    let processed_event = env
        .events()
        .all()
        .iter()
        .rev()
        .find(|e| &e.0 == &client.address && has_first_topic(&env, &e.1, "refund_processed"))
        .unwrap();
    let processed: RefundProcessedEvent = processed_event.2.into_val(&env);
    assert_eq!(processed.request_id, request_id);
    assert_eq!(processed.processor, admin);
    assert_eq!(processed.amount, 500);
    assert_eq!(processed.token, token);
    assert_eq!(processed.fee, 12);
    assert_eq!(processed.net, 488);
    assert_eq!(processed.status, RefundStatus::Processed);

    // Reject only applies to a still-Pending request, so exercise it on a
    // second, independent request rather than the one already approved.
    let other_id = client.request_refund(&requester, &token, &200, &Symbol::new(&env, "test2"));
    client.reject_refund(&admin, &other_id);
    let rejected_event = env
        .events()
        .all()
        .iter()
        .rev()
        .find(|e| &e.0 == &client.address && has_first_topic(&env, &e.1, "refund_processed"))
        .unwrap();
    let rejected: RefundProcessedEvent = rejected_event.2.into_val(&env);
    assert_eq!(rejected.request_id, other_id);
    assert_eq!(rejected.processor, admin);
    assert_eq!(rejected.amount, 200);
    assert_eq!(rejected.token, token);
    assert_eq!(rejected.fee, 0);
    assert_eq!(rejected.net, 0);
    assert_eq!(rejected.status, RefundStatus::Rejected);

    let topics = &rejected_event.1;
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "refund_processed"));
}

#[test]
fn test_config_updated_event_shape() {
    let (env, admin, client) = setup();

    client.update_config(&admin, &500, &200);

    let events = env.events().all();
    let event = events.last().unwrap();

    let topics = &event.1;
    assert_eq!(topics.len(), 1);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "config_updated"));

    let data: RefundConfigUpdatedEvent = event.2.into_val(&env);
    assert_eq!(data.admin, admin);
    assert_eq!(data.fee_bps, 500);
    assert_eq!(data.min_refund_amount, 200);
}

// =========================================================================
// Test Plan Implementation
// =========================================================================

/// Test Plan Row 1: fee_bps = 0, refund 1 000 -> requester +1 000; Processed
#[test]
fn test_plan_row1_fee_zero_refund_payout() {
    let (env, admin, client) = setup_with_config(0, 100);
    let token_admin = Address::generate(&env);
    let (token, token_client, token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);

    token_admin_client.mint(&client.address, &1_000);

    let request_id = client.request_refund(&requester, &token, &1_000, &Symbol::new(&env, "row1"));
    client.approve_refund(&admin, &request_id);

    client.process_refund(&admin, &request_id);

    let req = client.get_refund_request(&request_id);
    assert_eq!(req.status, RefundStatus::Processed);
    assert!(req.processed_at.is_some());

    assert_eq!(token_client.balance(&requester), 1_000);
    assert_eq!(client.get_accumulated_fees(&token), 0);
    assert_eq!(client.get_total_refunds_for(&token), 1_000);
}

/// Test Plan Row 2: fee_bps = 250, refund 1 000 -> requester +975; fees +25
#[test]
fn test_plan_row2_fee_250_refund_payout_and_fees() {
    let (env, admin, client) = setup_with_config(250, 100);
    let token_admin = Address::generate(&env);
    let (token, token_client, token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);

    token_admin_client.mint(&client.address, &1_000);

    let request_id = client.request_refund(&requester, &token, &1_000, &Symbol::new(&env, "row2"));
    client.approve_refund(&admin, &request_id);

    client.process_refund(&admin, &request_id);

    let req = client.get_refund_request(&request_id);
    assert_eq!(req.status, RefundStatus::Processed);

    // 1000 * 250 / 10000 = 25; net = 975
    assert_eq!(token_client.balance(&requester), 975);
    assert_eq!(client.get_accumulated_fees(&token), 25);
    assert_eq!(client.get_total_refunds_for(&token), 1_000);
}

/// Test Plan Row 3: fee_bps = 10_000 -> net 0; documented behaviour (allow a zero transfer / 100% fee retention)
#[test]
fn test_plan_row3_fee_10000_zero_net_transfer() {
    let (env, admin, client) = setup_with_config(10_000, 100);
    let token_admin = Address::generate(&env);
    let (token, token_client, token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);

    token_admin_client.mint(&client.address, &1_000);

    let request_id = client.request_refund(&requester, &token, &1_000, &Symbol::new(&env, "row3"));
    client.approve_refund(&admin, &request_id);

    client.process_refund(&admin, &request_id);

    let req = client.get_refund_request(&request_id);
    assert_eq!(req.status, RefundStatus::Processed);

    // fee = 1000, net = 0
    assert_eq!(token_client.balance(&requester), 0);
    assert_eq!(client.get_accumulated_fees(&token), 1_000);
    assert_eq!(client.get_total_refunds_for(&token), 1_000);
}

/// Test Plan Row 4: Contract balance < net -> InsufficientBalance; status stays Approved
#[test]
fn test_plan_row4_contract_balance_below_net_insufficient_balance() {
    let (env, admin, client) = setup_with_config(250, 100);
    let token_admin = Address::generate(&env);
    let (token, token_client, token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);

    // net payout for 1 000 @ 250 bps is 975. Mint only 500 to contract.
    token_admin_client.mint(&client.address, &500);

    let request_id = client.request_refund(&requester, &token, &1_000, &Symbol::new(&env, "row4"));
    client.approve_refund(&admin, &request_id);

    let result = client.try_process_refund(&admin, &request_id);
    assert_eq!(
        result.unwrap_err().unwrap(),
        RefundError::InsufficientBalance
    );

    // Verify CEI: state is unmodified, status remains Approved, no funds moved
    let req = client.get_refund_request(&request_id);
    assert_eq!(req.status, RefundStatus::Approved);
    assert!(req.processed_at.is_none());
    assert_eq!(token_client.balance(&requester), 0);
    assert_eq!(client.get_accumulated_fees(&token), 0);
    assert_eq!(client.get_total_refunds_for(&token), 0);
}

/// Test Plan Row 5: Refunds in two tokens -> per-token totals independent
#[test]
fn test_plan_row5_refunds_in_two_tokens_independent_totals() {
    let (env, admin, client) = setup_with_config(250, 100);
    let token_admin = Address::generate(&env);
    let (token_a, token_a_client, token_a_admin_client) = create_token(&env, &token_admin);
    let (token_b, token_b_client, token_b_admin_client) = create_token(&env, &token_admin);

    token_a_admin_client.mint(&client.address, &5_000);
    token_b_admin_client.mint(&client.address, &5_000);

    let requester = Address::generate(&env);

    let id_a = client.request_refund(&requester, &token_a, &1_000, &Symbol::new(&env, "token_a"));
    let id_b = client.request_refund(&requester, &token_b, &2_000, &Symbol::new(&env, "token_b"));

    client.approve_refund(&admin, &id_a);
    client.approve_refund(&admin, &id_b);

    client.process_refund(&admin, &id_a);

    assert_eq!(client.get_total_refunds_for(&token_a), 1_000);
    assert_eq!(client.get_total_refunds_for(&token_b), 0);
    assert_eq!(client.get_accumulated_fees(&token_a), 25);
    assert_eq!(client.get_accumulated_fees(&token_b), 0);

    client.process_refund(&admin, &id_b);

    assert_eq!(client.get_total_refunds_for(&token_a), 1_000);
    assert_eq!(client.get_total_refunds_for(&token_b), 2_000);
    assert_eq!(client.get_accumulated_fees(&token_a), 25);
    assert_eq!(client.get_accumulated_fees(&token_b), 50);

    #[allow(deprecated)]
    let total_legacy = client.get_total_refunds();
    assert_eq!(total_legacy, 3_000);

    assert_eq!(token_a_client.balance(&requester), 975);
    assert_eq!(token_b_client.balance(&requester), 1_950);
}

/// Test Plan Row 6: Process a non-approved request -> InvalidStatus (unchanged)
#[test]
fn test_plan_row6_process_non_approved_request_invalid_status() {
    let (env, admin, client) = setup_with_config(250, 100);
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    // 1. Pending status
    let pending_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "pending"));
    let err = client.try_process_refund(&admin, &pending_id);
    assert_eq!(err.unwrap_err().unwrap(), RefundError::InvalidStatus);

    // 2. Rejected status
    let rejected_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "reject"));
    client.reject_refund(&admin, &rejected_id);
    let err = client.try_process_refund(&admin, &rejected_id);
    assert_eq!(err.unwrap_err().unwrap(), RefundError::InvalidStatus);

    // 3. Processed status (already processed cannot be reprocessed)
    let token_admin = Address::generate(&env);
    let (real_token, _token_client, token_admin_client) = create_token(&env, &token_admin);
    token_admin_client.mint(&client.address, &1_000);

    let proc_id = client.request_refund(&requester, &real_token, &500, &Symbol::new(&env, "proc"));
    client.approve_refund(&admin, &proc_id);
    client.process_refund(&admin, &proc_id);

    let err = client.try_process_refund(&admin, &proc_id);
    assert_eq!(err.unwrap_err().unwrap(), RefundError::InvalidStatus);
}

/// Test Plan Row 7: Overflow in the fee multiplication -> Overflow
#[test]
fn test_plan_row7_overflow_in_fee_multiplication() {
    // fee_bps = 250, min_amount = 0
    let (env, admin, client) = setup_with_config(250, 0);
    let token_admin = Address::generate(&env);
    let (token, _token_client, _token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);

    // Request with maximum i128; i128::MAX * 250 overflows i128
    let request_id = client.request_refund(
        &requester,
        &token,
        &i128::MAX,
        &Symbol::new(&env, "overflow"),
    );
    client.approve_refund(&admin, &request_id);

    let err = client.try_process_refund(&admin, &request_id);
    assert_eq!(err.unwrap_err().unwrap(), RefundError::Overflow);

    // Status remains Approved
    let req = client.get_refund_request(&request_id);
    assert_eq!(req.status, RefundStatus::Approved);
}

/// Test Plan Row 8: Event payload -> token, fee and net correct
#[test]
fn test_plan_row8_event_payload_token_fee_net() {
    let (env, admin, client) = setup_with_config(250, 100);
    let token_admin = Address::generate(&env);
    let (token, _token_client, token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);

    token_admin_client.mint(&client.address, &1_000);

    let request_id = client.request_refund(&requester, &token, &1_000, &Symbol::new(&env, "row8"));
    client.approve_refund(&admin, &request_id);

    client.process_refund(&admin, &request_id);

    let event = env
        .events()
        .all()
        .iter()
        .rev()
        .find(|e| &e.0 == &client.address && has_first_topic(&env, &e.1, "refund_processed"))
        .unwrap();

    let topics = &event.1;
    assert_eq!(topics.len(), 1);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "refund_processed"));

    let data: RefundProcessedEvent = event.2.into_val(&env);
    assert_eq!(data.request_id, request_id);
    assert_eq!(data.processor, admin);
    assert_eq!(data.amount, 1_000);
    assert_eq!(data.token, token);
    assert_eq!(data.fee, 25);
    assert_eq!(data.net, 975);
    assert_eq!(data.status, RefundStatus::Processed);
}

/// Test Plan Row 9: withdraw_fees by a non-admin -> Unauthorized
#[test]
fn test_plan_row9_withdraw_fees_by_non_admin_unauthorized() {
    let (env, _admin, client) = setup_with_config(250, 100);
    let fake_admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let token = Address::generate(&env);

    let result = client.try_withdraw_fees(&fake_admin, &token, &recipient, &100);
    assert_eq!(result.unwrap_err().unwrap(), RefundError::Unauthorized);
}

/// Comprehensive fee withdrawal tests including validations and events
#[test]
fn test_withdraw_fees_validations() {
    let (env, admin, client) = setup_with_config(250, 100);
    let token_admin = Address::generate(&env);
    let (token, _token_client, token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);
    let recipient = Address::generate(&env);

    token_admin_client.mint(&client.address, &10_000);

    let request_id =
        client.request_refund(&requester, &token, &1_000, &Symbol::new(&env, "fee_test"));
    client.approve_refund(&admin, &request_id);
    client.process_refund(&admin, &request_id);

    assert_eq!(client.get_accumulated_fees(&token), 25);

    // Validation 1: amount <= 0
    let err = client.try_withdraw_fees(&admin, &token, &recipient, &0);
    assert_eq!(err.unwrap_err().unwrap(), RefundError::InvalidAmount);
    let err = client.try_withdraw_fees(&admin, &token, &recipient, &-10);
    assert_eq!(err.unwrap_err().unwrap(), RefundError::InvalidAmount);

    // Validation 2: recipient cannot be the contract itself
    let err = client.try_withdraw_fees(&admin, &token, &client.address, &10);
    assert_eq!(err.unwrap_err().unwrap(), RefundError::InvalidRecipient);

    // Validation 3: recipient cannot be the token contract
    let err = client.try_withdraw_fees(&admin, &token, &token, &10);
    assert_eq!(err.unwrap_err().unwrap(), RefundError::InvalidRecipient);

    // Validation 4: amount > accumulated fees
    let err = client.try_withdraw_fees(&admin, &token, &recipient, &26);
    assert_eq!(err.unwrap_err().unwrap(), RefundError::InsufficientBalance);
}

/// Happy path fee withdrawal and event emission
#[test]
fn test_withdraw_fees_happy_path_and_event() {
    let (env, admin, client) = setup_with_config(250, 100);
    let token_admin = Address::generate(&env);
    let (token, token_client, token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);
    let recipient = Address::generate(&env);

    token_admin_client.mint(&client.address, &10_000);

    let request_id =
        client.request_refund(&requester, &token, &1_000, &Symbol::new(&env, "fee_test"));
    client.approve_refund(&admin, &request_id);
    client.process_refund(&admin, &request_id);

    assert_eq!(client.get_accumulated_fees(&token), 25);

    // Happy path: withdraw partial amount
    client.withdraw_fees(&admin, &token, &recipient, &15);

    // Verify FeesWithdrawnEvent immediately from the withdraw_fees invocation
    let event = env
        .events()
        .all()
        .iter()
        .rev()
        .find(|e| &e.0 == &client.address && has_first_topic(&env, &e.1, "fees_withdrawn"))
        .unwrap();
    let topic0: Symbol = event.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "fees_withdrawn"));
    let data: FeesWithdrawnEvent = event.2.into_val(&env);
    assert_eq!(data.admin, admin);
    assert_eq!(data.token, token);
    assert_eq!(data.to, recipient);
    assert_eq!(data.amount, 15);

    assert_eq!(token_client.balance(&recipient), 15);
    assert_eq!(client.get_accumulated_fees(&token), 10);

    // Withdraw remaining
    client.withdraw_fees(&admin, &token, &recipient, &10);
    assert_eq!(token_client.balance(&recipient), 25);
    assert_eq!(client.get_accumulated_fees(&token), 0);
}

/// Test withdraw_fees when contract balance is drained below accumulated fees
#[test]
fn test_withdraw_fees_insufficient_contract_balance() {
    let (env, admin, client) = setup_with_config(250, 100);
    let token_admin = Address::generate(&env);
    let (token, _token_client, token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Fund contract with 975 (exact net for 1000 refund)
    token_admin_client.mint(&client.address, &975);

    let request_id =
        client.request_refund(&requester, &token, &1_000, &Symbol::new(&env, "bal_test"));
    client.approve_refund(&admin, &request_id);
    client.process_refund(&admin, &request_id);

    // Contract has 0 balance left, but 25 accumulated fees recorded
    assert_eq!(client.get_accumulated_fees(&token), 25);

    let err = client.try_withdraw_fees(&admin, &token, &recipient, &25);
    assert_eq!(err.unwrap_err().unwrap(), RefundError::InsufficientBalance);
}

/// Regression proof: on main, `process_refund` left the requester's balance unchanged.
/// Under this fix, the requester receives net tokens upon processing.
#[test]
fn test_regression_proof_requester_balance_changes_on_process() {
    let (env, admin, client) = setup_with_config(0, 100);
    let token_admin = Address::generate(&env);
    let (token, token_client, token_admin_client) = create_token(&env, &token_admin);
    let requester = Address::generate(&env);

    token_admin_client.mint(&client.address, &1_000);

    let request_id = client.request_refund(&requester, &token, &1_000, &Symbol::new(&env, "regr"));
    client.approve_refund(&admin, &request_id);

    assert_eq!(token_client.balance(&requester), 0);

    client.process_refund(&admin, &request_id);

    // Regression check: balance MUST increase from 0 to 1_000.
    assert_eq!(token_client.balance(&requester), 1_000);
}

/// Verify uninitialized errors for new entrypoints
#[test]
fn test_uninitialized_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let token = Address::generate(&env);
    let to = Address::generate(&env);

    assert_eq!(
        client.try_process_refund(&admin, &1).unwrap_err().unwrap(),
        RefundError::NotInitialized
    );
    assert_eq!(
        client
            .try_withdraw_fees(&admin, &token, &to, &100)
            .unwrap_err()
            .unwrap(),
        RefundError::NotInitialized
    );
    assert_eq!(
        client
            .try_get_accumulated_fees(&token)
            .unwrap_err()
            .unwrap(),
        RefundError::NotInitialized
    );
    assert_eq!(
        client
            .try_get_total_refunds_for(&token)
            .unwrap_err()
            .unwrap(),
        RefundError::NotInitialized
    );
    #[allow(deprecated)]
    let uninit_total = client.try_get_total_refunds();
    assert_eq!(
        uninit_total.unwrap_err().unwrap(),
        RefundError::NotInitialized
    );
}

#[test]
fn test_plan_row1_too_many_open_requests_counter_unchanged() {
    let (env, _admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    for _ in 0..MAX_REQUESTER_REFUNDS {
        client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    }

    // 101st request when all 100 are open (Pending)
    let res = client.try_request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    assert_eq!(res.unwrap_err().unwrap(), RefundError::TooManyOpenRequests);

    // Counter unchanged: another requester gets ID 101
    let other_requester = Address::generate(&env);
    let next_id = client.request_refund(&other_requester, &token, &500, &Symbol::new(&env, "test"));
    assert_eq!(next_id, (MAX_REQUESTER_REFUNDS + 1) as u64);
}

#[test]
fn test_plan_row2_evict_processed_request_append_new_id() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let (token, _token_client, admin_token_client) = create_token(&env, &admin);

    admin_token_client.mint(&client.address, &100_000);

    let first_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    client.approve_refund(&admin, &first_id);
    client.process_refund(&admin, &first_id);

    for _ in 1..MAX_REQUESTER_REFUNDS {
        client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    }

    // 101st request should evict #1 (Processed) and append new id 101
    let new_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    assert_eq!(new_id, (MAX_REQUESTER_REFUNDS + 1) as u64);

    let page = client.get_refunds_by_requester(&requester, &0, &(MAX_REQUESTER_REFUNDS + 1));
    assert_eq!(
        page.len(),
        MAX_REQUESTER_REFUNDS.min(crate::MAX_REQUESTER_REFUNDS_PAGE_SIZE)
    );
    // Index 0 is now request 2
    assert_eq!(page.get(0).unwrap(), 2);
    // Request 101 is at the end of the requester's refunds (page 2)
    let page2 = client.get_refunds_by_requester(&requester, &50, &50);
    assert_eq!(page2.len(), 50);
    assert_eq!(page2.get(page2.len() - 1).unwrap(), new_id);
}

#[test]
fn test_plan_row3_mixed_statuses_oldest_closed_evicted() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let (token, _token_client, admin_token_client) = create_token(&env, &admin);

    admin_token_client.mint(&client.address, &100_000);

    // Request 1: Pending (Open)
    let req1 = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "req1"));
    assert_eq!(req1, 1);

    // Request 2: Approved (Open)
    let req2 = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "req2"));
    client.approve_refund(&admin, &req2);

    // Request 3: Rejected (Closed - oldest closed)
    let req3 = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "req3"));
    client.reject_refund(&admin, &req3);

    // Request 4: Processed (Closed)
    let req4 = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "req4"));
    client.approve_refund(&admin, &req4);
    client.process_refund(&admin, &req4);

    // Fill remaining up to 100 with Pending
    for _ in 4..MAX_REQUESTER_REFUNDS {
        client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    }

    // 101st request should evict req3 (oldest closed), NOT req1 (oldest open)
    let new_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "new"));
    assert_eq!(new_id, 101);

    let refunds = client.get_refunds_by_requester(&requester, &0, &100);
    // req1 and req2 must still be present!
    assert_eq!(refunds.get(0).unwrap(), 1);
    assert_eq!(refunds.get(1).unwrap(), 2);
    // req3 was evicted, so index 2 should now be req4
    assert_eq!(refunds.get(2).unwrap(), 4);
    // req3 is not in list
    let mut found_req3 = false;
    for i in 0..refunds.len() {
        if refunds.get(i).unwrap() == 3 {
            found_req3 = true;
        }
    }
    assert!(!found_req3);
}

#[test]
fn test_plan_row4_get_refunds_by_requester_after_eviction_all_open_listed() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    // Create 100 requests where request 1 is rejected, the rest 99 are open
    let first = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    client.reject_refund(&admin, &first);

    for _ in 1..MAX_REQUESTER_REFUNDS {
        client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    }

    // Eviction happens here
    let new_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));

    let page1 = client.get_refunds_by_requester(&requester, &0, &50);
    let page2 = client.get_refunds_by_requester(&requester, &50, &50);
    assert_eq!(page1.len(), 50);
    assert_eq!(page2.len(), 50);
    // Check that every open request from 2 to 101 is present across page1 and page2
    for expected_id in 2..=new_id {
        let mut found = false;
        for i in 0..page1.len() {
            if page1.get(i).unwrap() == expected_id {
                found = true;
                break;
            }
        }
        if !found {
            for i in 0..page2.len() {
                if page2.get(i).unwrap() == expected_id {
                    found = true;
                    break;
                }
            }
        }
        assert!(found, "Expected id {} to be listed", expected_id);
    }
}

#[test]
fn test_plan_row5_init_bumps_instance_ttl() {
    use soroban_sdk::testutils::storage::Instance as _;
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(RefundContract, ());
    let client = RefundContractClient::new(&env, &contract_id);

    client.init(&admin, &250, &100);

    let ttl = env.as_contract(&client.address, || env.storage().instance().get_ttl());
    assert!(
        ttl >= crate::INSTANCE_BUMP_AMOUNT - 1,
        "instance TTL {} should be >= INSTANCE_BUMP_AMOUNT - 1",
        ttl
    );
}

#[test]
fn test_plan_row7_eviction_cpu_cost_bound() {
    let (env, admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    // Make 99 open requests
    for _ in 0..(MAX_REQUESTER_REFUNDS - 1) {
        client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    }
    // Make the 100th request closed (worst case: scan 100 entries to find the closed one at the end)
    let last_id = client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    client.reject_refund(&admin, &last_id);

    // Measure budget for 101st request which performs worst-case scan across 100 records
    let cpu_before = env.cost_estimate().budget().cpu_instruction_cost();
    client.request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    let cpu_after = env.cost_estimate().budget().cpu_instruction_cost();
    let cpu_used = cpu_after - cpu_before;

    // Soroban transaction limit is 100,000,000 CPU instructions.
    // Worst-case 100-entry eviction scan consumes ~2.8M instructions (< 3.5% of tx budget).
    assert!(
        cpu_used < 3_500_000,
        "Eviction scan cpu_used {} exceeds bound of 3,500,000",
        cpu_used
    );
}

#[test]
fn test_request_refund_overflow() {
    let (env, _admin, client) = setup();
    let requester = Address::generate(&env);
    let token = Address::generate(&env);

    // Artificially set counter to u64::MAX
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&crate::types::StorageKey::RefundCounter, &u64::MAX);
    });

    let res = client.try_request_refund(&requester, &token, &500, &Symbol::new(&env, "test"));
    assert_eq!(res.unwrap_err().unwrap(), RefundError::Overflow);
}
