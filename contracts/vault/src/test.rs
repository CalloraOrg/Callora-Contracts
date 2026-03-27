extern crate std;

use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{token, Address, Env, IntoVal, String, Symbol};

use super::*;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

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

/// Mint `amount` USDC directly to `vault_address` (simulates pre-funded vault).
fn fund_vault(
    usdc_admin_client: &token::StellarAssetClient,
    vault_address: &Address,
    amount: i128,
) {
    usdc_admin_client.mint(vault_address, &amount);
}

// ---------------------------------------------------------------------------
// Init tests
// ---------------------------------------------------------------------------

#[test]
fn init_with_balance_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    let events = env.events().all();
    let last = events.last().expect("expected at least one event");

    assert_eq!(last.0, vault_address);
    let topics = &last.1;
    assert_eq!(topics.len(), 2);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "init"));
    assert_eq!(topic1, owner);

    let data: i128 = last.2.into_val(&env);
    assert_eq!(data, 1000);
}

#[test]
fn init_defaults_balance_to_zero() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    assert_eq!(client.balance(), 0);
}

#[test]
fn init_sets_owner_and_min_deposit() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    let meta = client.init(&owner, &usdc, &Some(500), &None, &Some(10), &None, &None);

    assert_eq!(meta.balance, 500);
    assert_eq!(meta.owner, owner);
    assert_eq!(meta.min_deposit, 10);
    assert_eq!(client.balance(), 500);
    assert_eq!(client.get_admin(), owner);
}

#[test]
fn double_init_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let result = client.try_init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    assert!(result.is_err(), "expected error on second init");
}

// ---------------------------------------------------------------------------
// get_meta / balance tests
// ---------------------------------------------------------------------------

#[test]
fn get_meta_returns_correct_state() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

    let meta = client.get_meta();
    assert_eq!(meta.balance, 500);
    assert_eq!(meta.owner, owner);
    assert_eq!(client.balance(), 500);
}

#[test]
fn get_meta_before_init_fails() {
    let env = Env::default();
    let (_, client) = create_vault(&env);
    assert!(client.try_get_meta().is_err(), "expected error before init");
}

// ---------------------------------------------------------------------------
// Admin tests
// ---------------------------------------------------------------------------

#[test]
fn get_admin_returns_owner_after_init() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    assert_eq!(client.get_admin(), owner);
}

#[test]
fn set_admin_two_step_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    client.set_admin(&owner, &new_admin);
    assert_eq!(client.get_admin(), owner); // Still old admin

    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn set_admin_unauthorized_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let intruder = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let result = client.try_set_admin(&intruder, &new_admin);
    assert!(
        result.is_err(),
        "expected error when non-admin calls set_admin"
    );
}

// ---------------------------------------------------------------------------
// Deposit tests
// ---------------------------------------------------------------------------

#[test]
fn owner_can_deposit() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

    // Mint USDC to owner then approve the vault
    usdc_admin.mint(&owner, &200);
    usdc_client.approve(&owner, &vault_address, &200, &1000);

    let new_balance = client.deposit(&owner, &200);
    assert_eq!(new_balance, 700);
    assert_eq!(client.balance(), 700);
}

#[test]
fn allowed_depositor_can_deposit() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));

    usdc_admin.mint(&depositor, &200);
    usdc_client.approve(&depositor, &vault_address, &200, &1000);
    let returned = client.deposit(&depositor, &200);

    assert_eq!(returned, 300);
    assert_eq!(client.balance(), 300);
}

#[test]
#[should_panic(expected = "unauthorized: only owner or allowed depositor can deposit")]
fn unauthorized_address_cannot_deposit() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    client.deposit(&unauthorized, &50);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn deposit_zero_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.deposit(&owner, &0);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn deposit_negative_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.deposit(&owner, &-50);
}

#[test]
fn deposit_below_minimum_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &Some(50), &None, &None);
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));

    usdc_admin.mint(&depositor, &30);
    usdc_client.approve(&depositor, &vault_address, &30, &1000);
    let result = client.try_deposit(&depositor, &30);
    assert!(result.is_err(), "expected error for deposit below minimum");
}

#[test]
fn deposit_at_minimum_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &Some(50), &None, &None);
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));

    usdc_admin.mint(&depositor, &50);
    usdc_client.approve(&depositor, &vault_address, &50, &1000);
    let new_balance = client.deposit(&depositor, &50);
    assert_eq!(new_balance, 150);
}

// ---------------------------------------------------------------------------
// Allowed depositor management tests
// ---------------------------------------------------------------------------

#[test]
fn owner_can_set_and_clear_allowed_depositor() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    // Set depositor
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    usdc_admin.mint(&depositor, &50);
    usdc_client.approve(&depositor, &vault_address, &50, &1000);
    client.deposit(&depositor, &50);
    assert_eq!(client.balance(), 150);

    // Clear depositor
    client.set_allowed_depositor(&owner, &None);

    // Owner can still deposit
    usdc_admin.mint(&owner, &25);
    usdc_client.approve(&owner, &vault_address, &25, &1000);
    client.deposit(&owner, &25);
    assert_eq!(client.balance(), 175);
}

#[test]
#[should_panic(expected = "unauthorized: owner only")]
fn non_owner_cannot_set_allowed_depositor() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let non_owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.set_allowed_depositor(&non_owner, &Some(depositor));
}

#[test]
#[should_panic(expected = "unauthorized: only owner or allowed depositor can deposit")]
fn deposit_after_depositor_cleared_is_rejected() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    client.set_allowed_depositor(&owner, &None);

    usdc_admin.mint(&depositor, &50);
    usdc_client.approve(&depositor, &vault_address, &50, &1000);
    client.deposit(&depositor, &50);
}

// ---------------------------------------------------------------------------
// Deduct tests
// ---------------------------------------------------------------------------

#[test]
fn deduct_reduces_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 300);
    client.init(
        &owner,
        &usdc,
        &Some(300),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );

    let returned = client.deduct(&caller, &50, &None);
    assert_eq!(returned, 250);
    assert_eq!(client.balance(), 250);
}

#[test]
fn deduct_with_request_id() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc,
        &Some(1000),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );

    let remaining = client.deduct(&caller, &100, &Some(Symbol::new(&env, "req123")));
    assert_eq!(remaining, 900);
}

#[test]
fn deduct_insufficient_balance_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 10);
    client.init(
        &owner,
        &usdc,
        &Some(10),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );

    let result = client.try_deduct(&caller, &100, &None);
    assert!(result.is_err(), "expected error for insufficient balance");
}

#[test]
fn deduct_exact_balance_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 75);
    client.init(
        &owner,
        &usdc,
        &Some(75),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );

    let remaining = client.deduct(&caller, &75, &None);
    assert_eq!(remaining, 0);
    assert_eq!(client.balance(), 0);
}

#[test]
fn deduct_event_contains_request_id() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(
        &owner,
        &usdc,
        &Some(500),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );

    let request_id = Symbol::new(&env, "api_call_42");
    client.deduct(&caller, &150, &Some(request_id.clone()));

    let events = env.events().all();
    let ev = events.last().expect("expected deduct event");

    let topic0: Symbol = ev.1.get(0).unwrap().into_val(&env);
    let topic1: Address = ev.1.get(1).unwrap().into_val(&env);
    let topic2: Symbol = ev.1.get(2).unwrap().into_val(&env);

    assert_eq!(topic0, Symbol::new(&env, "deduct"));
    assert_eq!(topic1, caller);
    assert_eq!(topic2, request_id);

    let (emitted_amount, remaining): (i128, i128) = ev.2.into_val(&env);
    assert_eq!(emitted_amount, 150);
    assert_eq!(remaining, 350);
}

#[test]
fn deduct_event_no_request_id_uses_empty_symbol() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 300);
    client.init(
        &owner,
        &usdc,
        &Some(300),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );
    client.deduct(&caller, &100, &None);

    let events = env.events().all();
    let ev = events.last().expect("expected deduct event");

    let topic0: Symbol = ev.1.get(0).unwrap().into_val(&env);
    let topic2: Symbol = ev.1.get(2).unwrap().into_val(&env);

    assert_eq!(topic0, Symbol::new(&env, "deduct"));
    assert_eq!(topic2, Symbol::new(&env, ""));
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn deduct_zero_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(
        &owner,
        &usdc,
        &Some(500),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );
    client.deduct(&caller, &0, &None);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn deduct_negative_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(
        &owner,
        &usdc,
        &Some(100),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );
    client.deduct(&caller, &-50, &None);
}

#[test]
#[should_panic(expected = "insufficient balance")]
fn deduct_exceeds_balance_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 50);
    client.init(
        &owner,
        &usdc,
        &Some(50),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );
    client.deduct(&caller, &100, &None);
}

#[test]
fn balance_unchanged_after_failed_deduct() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(
        &owner,
        &usdc,
        &Some(100),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );

    let _ = client.try_deduct(&caller, &200, &None);
    assert_eq!(client.balance(), 100);
}

// ---------------------------------------------------------------------------
// Batch deduct tests
// ---------------------------------------------------------------------------

#[test]
fn batch_deduct_multiple_items() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc,
        &Some(1000),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: Some(Symbol::new(&env, "req1"))
        },
        DeductItem {
            amount: 200,
            request_id: None
        },
        DeductItem {
            amount: 50,
            request_id: Some(Symbol::new(&env, "req2"))
        },
    ];

    let remaining = client.batch_deduct(&caller, &items);
    assert_eq!(remaining, 650);
    assert_eq!(client.balance(), 650);
}

#[test]
fn batch_deduct_events_contain_request_ids() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc,
        &Some(1000),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );

    let rid_a = Symbol::new(&env, "batch_a");
    let rid_b = Symbol::new(&env, "batch_b");
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 200,
            request_id: Some(rid_a.clone())
        },
        DeductItem {
            amount: 300,
            request_id: Some(rid_b.clone())
        },
    ];
    client.batch_deduct(&caller, &items);

    let all_events = env.events().all();
    // Last two events are the two deduct events
    let len = all_events.len();
    let ev_a = all_events.get(len - 2).unwrap();
    let ev_b = all_events.get(len - 1).unwrap();

    let req_a: Symbol = ev_a.1.get(2).unwrap().into_val(&env);
    let req_b: Symbol = ev_b.1.get(2).unwrap().into_val(&env);
    assert_eq!(req_a, rid_a);
    assert_eq!(req_b, rid_b);

    let (amt_a, _): (i128, i128) = ev_a.2.into_val(&env);
    let (amt_b, _): (i128, i128) = ev_b.2.into_val(&env);
    assert_eq!(amt_a, 200);
    assert_eq!(amt_b, 300);
}

#[test]
fn batch_deduct_insufficient_balance_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 50,
            request_id: None
        },
        DeductItem {
            amount: 80,
            request_id: None
        },
    ];

    let result = client.try_batch_deduct(&caller, &items);
    assert!(result.is_err(), "expected error for batch overdraw");
    // Balance must be unchanged on failure
    assert_eq!(client.balance(), 100);
}

#[test]
fn batch_deduct_empty_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(
        &owner,
        &usdc,
        &Some(100),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );

    let items: soroban_sdk::Vec<DeductItem> = soroban_sdk::vec![&env];
    let result = client.try_batch_deduct(&caller, &items);
    assert!(result.is_err(), "expected error for empty batch");
}

#[test]
fn batch_deduct_zero_amount_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 0,
            request_id: None
        }
    ];
    let result = client.try_batch_deduct(&caller, &items);
    assert!(result.is_err(), "expected error for zero amount item");
}

// ---------------------------------------------------------------------------
// Withdraw tests
// ---------------------------------------------------------------------------

#[test]
fn withdraw_reduces_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

    let remaining = client.withdraw(&200);
    assert_eq!(remaining, 300);
    assert_eq!(client.balance(), 300);
}

#[test]
fn withdraw_insufficient_balance_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let result = client.try_withdraw(&500);
    assert!(result.is_err(), "expected error for insufficient balance");
}

#[test]
fn withdraw_zero_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let result = client.try_withdraw(&0);
    assert!(result.is_err(), "expected error for zero amount");
}

#[test]
fn withdraw_to_reduces_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

    let remaining = client.withdraw_to(&recipient, &150);
    assert_eq!(remaining, 350);
    assert_eq!(client.balance(), 350);
    assert_eq!(usdc_client.balance(&recipient), 150);
}

#[test]
fn withdraw_to_insufficient_balance_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let result = client.try_withdraw_to(&recipient, &500);
    assert!(result.is_err(), "expected error for insufficient balance");
}

// ---------------------------------------------------------------------------
// Transfer ownership tests
// ---------------------------------------------------------------------------

#[test]
fn transfer_ownership_two_step_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    client.transfer_ownership(&new_owner);
    let meta = client.get_meta();
    assert_eq!(meta.owner, owner); // Still old owner

    client.accept_ownership();
    let meta2 = client.get_meta();
    assert_eq!(meta2.owner, new_owner);
}

#[test]
fn transfer_ownership_emits_events() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.transfer_ownership(&new_owner);

    let events = env.events().all();
    let nomad_ev = events
        .iter()
        .find(|e| {
            e.0 == vault_address && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "ownership_nominated")
            }
        })
        .expect("expected ownership_nominated event");

    let old_n: Address = nomad_ev.1.get(1).unwrap().into_val(&env);
    let new_n: Address = nomad_ev.1.get(2).unwrap().into_val(&env);
    assert_eq!(old_n, owner);
    assert_eq!(new_n, new_owner);

    client.accept_ownership();
    let events2 = env.events().all();
    let accept_ev = events2
        .iter()
        .find(|e| {
            e.0 == vault_address && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "ownership_accepted")
            }
        })
        .expect("expected ownership_accepted event");

    let old_a: Address = accept_ev.1.get(1).unwrap().into_val(&env);
    let new_a: Address = accept_ev.1.get(2).unwrap().into_val(&env);
    assert_eq!(old_a, owner);
    assert_eq!(new_a, new_owner);
}

#[test]
#[should_panic(expected = "new_owner must be different from current owner")]
fn transfer_ownership_same_address_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.transfer_ownership(&owner);
}

// ---------------------------------------------------------------------------
// Distribute tests
// ---------------------------------------------------------------------------

#[test]
fn distribute_transfers_usdc_to_recipient() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&admin, &usdc, &Some(0), &None, &None, &None, &None);

    client.distribute(&admin, &developer, &300);

    assert_eq!(usdc_client.balance(&developer), 300);
    assert_eq!(usdc_client.balance(&vault_address), 700);
}

#[test]
fn distribute_unauthorized_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let intruder = Address::generate(&env);
    let developer = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &admin);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&admin, &usdc, &Some(0), &None, &None, &None, &None);

    let result = client.try_distribute(&intruder, &developer, &300);
    assert!(result.is_err(), "expected error when non-admin distributes");
}

#[test]
fn distribute_insufficient_usdc_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &admin);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&admin, &usdc, &Some(0), &None, &None, &None, &None);

    let result = client.try_distribute(&admin, &developer, &500);
    assert!(result.is_err(), "expected error for insufficient USDC");
}

#[test]
fn distribute_zero_amount_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &admin);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&admin, &usdc, &Some(0), &None, &None, &None, &None);

    let result = client.try_distribute(&admin, &developer, &0);
    assert!(result.is_err(), "expected error for zero amount");
}

// ---------------------------------------------------------------------------
// Offering metadata tests
// ---------------------------------------------------------------------------

#[test]
fn set_and_retrieve_metadata() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-001");
    let metadata = String::from_str(&env, "QmXoypizjW3WknFiJnKLwHCnL72vedxjQkDDP1mXWo6uco");

    let result = client.set_metadata(&owner, &offering_id, &metadata);
    assert_eq!(result, metadata);

    let retrieved = client.get_metadata(&offering_id);
    assert_eq!(retrieved, Some(metadata));
}

#[test]
fn set_metadata_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-002");
    let metadata = String::from_str(
        &env,
        "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
    );
    client.set_metadata(&owner, &offering_id, &metadata);

    let events = env.events().all();
    let ev = events.last().expect("expected metadata_set event");

    assert_eq!(ev.0, vault_address);
    let topics = &ev.1;
    assert_eq!(topics.len(), 3);

    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: String = topics.get(1).unwrap().into_val(&env);
    let topic2: Address = topics.get(2).unwrap().into_val(&env);

    assert_eq!(topic0, Symbol::new(&env, "metadata_set"));
    assert_eq!(topic1, offering_id);
    assert_eq!(topic2, owner);

    let data: String = ev.2.into_val(&env);
    assert_eq!(data, metadata);
}

#[test]
fn update_metadata_and_verify() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-003");
    let old_metadata = String::from_str(&env, "QmOldMetadata123");
    let new_metadata = String::from_str(&env, "QmNewMetadata456");

    client.set_metadata(&owner, &offering_id, &old_metadata);
    let result = client.update_metadata(&owner, &offering_id, &new_metadata);
    assert_eq!(result, new_metadata);

    let retrieved = client.get_metadata(&offering_id);
    assert_eq!(retrieved, Some(new_metadata));
}

#[test]
fn update_metadata_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-004");
    let old_metadata = String::from_str(&env, "https://example.com/old.json");
    let new_metadata = String::from_str(&env, "https://example.com/new.json");

    client.set_metadata(&owner, &offering_id, &old_metadata);
    client.update_metadata(&owner, &offering_id, &new_metadata);

    let events = env.events().all();
    let ev = events.last().expect("expected metadata_updated event");

    assert_eq!(ev.0, vault_address);
    let topics = &ev.1;
    assert_eq!(topics.len(), 3);

    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: String = topics.get(1).unwrap().into_val(&env);
    let topic2: Address = topics.get(2).unwrap().into_val(&env);

    assert_eq!(topic0, Symbol::new(&env, "metadata_updated"));
    assert_eq!(topic1, offering_id);
    assert_eq!(topic2, owner);

    let data: (String, String) = ev.2.into_val(&env);
    assert_eq!(data.0, old_metadata);
    assert_eq!(data.1, new_metadata);
}

#[test]
#[should_panic(expected = "unauthorized: owner only")]
fn unauthorized_cannot_set_metadata() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-005");
    let metadata = String::from_str(&env, "QmSomeMetadata");
    client.set_metadata(&unauthorized, &offering_id, &metadata);
}

#[test]
fn set_metadata_max_length_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "a".repeat(64).as_str());
    let metadata = String::from_str(&env, "b".repeat(256).as_str());

    client.set_metadata(&owner, &offering_id, &metadata);
    assert_eq!(client.get_metadata(&offering_id), Some(metadata));
}

#[test]
#[should_panic(expected = "metadata exceeds max length")]
fn set_metadata_exceeds_length_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "off-1");
    let metadata = String::from_str(&env, "b".repeat(257).as_str());

    client.set_metadata(&owner, &offering_id, &metadata);
}

#[test]
#[should_panic(expected = "offering_id exceeds max length")]
fn set_offering_id_exceeds_length_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "a".repeat(65).as_str());
    let metadata = String::from_str(&env, "meta");

    client.set_metadata(&owner, &offering_id, &metadata);
}

#[test]
fn update_metadata_max_length_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-update");
    let metadata = String::from_str(&env, "b".repeat(256).as_str());

    client.set_metadata(&owner, &offering_id, &String::from_str(&env, "old"));
    client.update_metadata(&owner, &offering_id, &metadata);
    assert_eq!(client.get_metadata(&offering_id), Some(metadata));
}

#[test]
fn metadata_remains_after_ownership_transfer() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "off-transfer");
    let metadata = String::from_str(&env, "ipfs://cid123");
    client.set_metadata(&owner, &offering_id, &metadata);

    client.transfer_ownership(&new_owner);

    // Metadata should still be accessible
    assert_eq!(client.get_metadata(&offering_id), Some(metadata.clone()));

    // Old owner should no longer be able to update it
    let update_res =
        client.try_update_metadata(&owner, &offering_id, &String::from_str(&env, "new"));
    assert!(update_res.is_err());

    // New owner should be able to update it
    client.update_metadata(&new_owner, &offering_id, &String::from_str(&env, "new"));
    assert_eq!(
        client.get_metadata(&offering_id),
        Some(String::from_str(&env, "new"))
    );
}

// ---------------------------------------------------------------------------
// Full lifecycle test
// ---------------------------------------------------------------------------

#[test]
fn vault_full_lifecycle() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let caller = Address::generate(&env);
    let recipient = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();

    // Init with 500 balance, min_deposit = 10
    fund_vault(&usdc_admin, &vault_address, 500);
    let meta = client.init(
        &owner,
        &usdc,
        &Some(500),
        &Some(caller.clone()),
        &Some(10),
        &None,
        &None,
    );
    assert_eq!(meta.balance, 500);
    assert_eq!(meta.owner, owner);
    assert_eq!(client.balance(), 500);
    assert_eq!(client.get_admin(), owner);

    // Allow depositor and deposit 200
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    usdc_admin.mint(&depositor, &200);
    usdc_client.approve(&depositor, &vault_address, &200, &1000);
    let after_deposit = client.deposit(&depositor, &200);
    assert_eq!(after_deposit, 700);
    assert_eq!(client.balance(), 700);

    // Batch deduct 100 + 50 + 25 = 175
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: Some(Symbol::new(&env, "r1"))
        },
        DeductItem {
            amount: 50,
            request_id: None
        },
        DeductItem {
            amount: 25,
            request_id: Some(Symbol::new(&env, "r3"))
        },
    ];
    let after_batch = client.batch_deduct(&caller, &items);
    assert_eq!(after_batch, 525);
    assert_eq!(client.balance(), 525);

    // Single deduct
    let after_deduct = client.deduct(&caller, &25, &Some(Symbol::new(&env, "r4")));
    assert_eq!(after_deduct, 500);

    // Admin change
    client.set_admin(&owner, &new_admin);
    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);

    // Withdraw to recipient
    let after_withdraw_to = client.withdraw_to(&recipient, &100);
    assert_eq!(after_withdraw_to, 400);
    assert_eq!(client.balance(), 400);

    // Withdraw to owner
    let after_withdraw = client.withdraw(&50);
    assert_eq!(after_withdraw, 350);
    assert_eq!(client.balance(), 350);

    let final_meta = client.get_meta();
    assert_eq!(final_meta.balance, 350);
    assert_eq!(final_meta.owner, owner);
    assert_eq!(final_meta.min_deposit, 10);
}

// ---------------------------------------------------------------------------
// Revenue pool integration tests
// ---------------------------------------------------------------------------

#[test]
fn init_with_revenue_pool_stores_address() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(
        &owner,
        &usdc,
        &Some(500), &None,
        &None,
        &None,
        &Some(revenue_pool.clone()),
        &None,
    );

    assert_eq!(client.balance(), 500);
}

#[test]
fn deduct_with_revenue_pool_transfers_usdc() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc_address,
        &Some(1000),
        &Some(caller.clone()),
        &None,
        &Some(revenue_pool.clone()),
        &None,
    );

    client.deduct(&caller, &300, &None);

    assert_eq!(client.balance(), 700);
    assert_eq!(usdc_client.balance(&revenue_pool), 300);
}

#[test]
fn batch_deduct_with_revenue_pool_transfers_total_usdc() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc_address,
        &Some(1000),
        &Some(caller.clone()),
        &None,
        &Some(revenue_pool.clone()),
        &None,
    );

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 200,
            request_id: None
        },
        DeductItem {
            amount: 150,
            request_id: None
        },
    ];
    client.batch_deduct(&caller, &items);

    assert_eq!(client.balance(), 650);
    assert_eq!(usdc_client.balance(&revenue_pool), 350);
}

// ---------------------------------------------------------------------------
// set_settlement / get_settlement tests
// ---------------------------------------------------------------------------

#[test]
fn set_settlement_stores_and_get_returns_address() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_settlement(&owner, &settlement);

    assert_eq!(client.get_settlement(), settlement);
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin")]
fn set_settlement_unauthorized_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_settlement(&attacker, &settlement);
}

#[test]
#[should_panic(expected = "settlement address not set")]
fn get_settlement_before_set_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.get_settlement();
}

#[test]
fn test_clear_allowed_depositors() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    client.set_allowed_depositor(&owner, &None);

    usdc_admin.mint(&depositor, &50);
    usdc_client.approve(&depositor, &vault_address, &50, &1000);
    let result = client.try_deposit(&depositor, &50);
    assert!(result.is_err());
}

#[test]
fn test_set_authorized_caller() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let auth_caller = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    client.set_authorized_caller(&auth_caller);
    let meta = client.get_meta();
    assert_eq!(meta.authorized_caller, Some(auth_caller));
}

#[test]
fn test_deduct_with_settlement_success() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc_address,
        &Some(1000),
        &None,
        &None,
        &None,
        &None,
    );
    client.set_settlement(&owner, &settlement);

    client.deduct(&owner, &300, &None);

    assert_eq!(client.balance(), 700);
    assert_eq!(usdc_client.balance(&settlement), 300);
}
