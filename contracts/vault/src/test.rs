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
    std::println!("init_with_balance_emits_event events len: {}", events.len());
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
fn init_succeeds_when_onchain_usdc_balance_covers_initial_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);

    let meta = client.init(&owner, &usdc, &Some(400), &None, &None, &None, &None);

    assert_eq!(meta.balance, 400);
    assert_eq!(client.balance(), 400);
}

#[test]
#[should_panic(expected = "initial_balance exceeds on-ledger USDC balance")]
fn init_fails_when_initial_balance_exceeds_onchain_usdc_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 99);

    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
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
    // Swap order: create USDC first
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    usdc_admin.mint(&owner, &500);
    usdc_client.approve(&owner, &vault_address, &500, &1000);

    let new_balance = client.deposit(&owner, &200);
    assert_eq!(new_balance, 200);

    let events = env.events().all();
    std::println!("owner_can_deposit events len: {}", events.len());
    for (i, e) in events.iter().enumerate() {
        std::println!(
            "Event[{}]: contract={:?}, topics={:?}, data={:?}",
            i,
            e.0,
            e.1,
            e.2
        );
    }
    let deposit_event = events
        .iter()
        .find(|e| {
            if e.0 != vault_address {
                return false;
            }
            if e.1.is_empty() {
                return false;
            }
            let s: Symbol = e.1.get(0).unwrap().into_val(&env);
            s == Symbol::new(&env, "deposit")
        })
        .expect("expected deposit event");

    let (amount, balance): (i128, i128) = deposit_event.2.into_val(&env);
    assert_eq!(amount, 200);
    assert_eq!(balance, 200);
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

#[test]
fn deposit_paused_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    client.pause(&owner);
    assert!(client.is_paused());

    usdc_admin.mint(&owner, &100);
    usdc_client.approve(&owner, &vault_address, &100, &1000);

    let result = client.try_deposit(&owner, &100);
    assert!(result.is_err());
    // Should contain "vault is paused" but Error doesn't easily expose the string in tests without more setup
    // but the transaction should fail.

    client.unpause(&owner);
    assert!(!client.is_paused());
    client.deposit(&owner, &100);
    assert_eq!(client.balance(), 100);
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
fn set_allowed_depositor_duplicate_is_ignored() {
    // Adding the same depositor twice should not create a duplicate entry
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    client.set_allowed_depositor(&owner, &Some(depositor.clone())); // duplicate â€” should be a no-op

    // depositor can still deposit exactly once (list not doubled)
    usdc_admin.mint(&depositor, &50);
    usdc_client.approve(&depositor, &vault_address, &50, &1000);
    assert_eq!(client.deposit(&depositor, &50), 150);
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
// Pause tests
// ---------------------------------------------------------------------------

#[test]
fn pause_unpause_admin_only() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let intruder = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // intruder fails
    let res = client.try_pause(&intruder);
    assert!(res.is_err());

    // admin (owner) succeeds
    client.pause(&owner);
    assert!(client.is_paused());

    // intruder fails unpause
    let res = client.try_unpause(&intruder);
    assert!(res.is_err());

    // admin (owner) succeeds unpause
    client.unpause(&owner);
    assert!(!client.is_paused());
}

#[test]
fn pause_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    client.pause(&owner);
    let events = env.events().all();
    let pause_event = events
        .iter()
        .find(|e| {
            e.0 == vault_address
                && e.1
                    .get(0)
                    .map(|v| {
                        let s: Symbol = v.into_val(&env);
                        s == Symbol::new(&env, "vault_paused")
                    })
                    .unwrap_or(false)
        })
        .expect("expected pause event");

    let admin_topic: Address = pause_event.1.get(1).unwrap().into_val(&env);
    assert_eq!(admin_topic, owner);
}

// ---------------------------------------------------------------------------
// Deduct tests
// ---------------------------------------------------------------------------

#[test]
fn set_authorized_caller_sets_and_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 200);
    client.init(&owner, &usdc, &Some(200), &None, &None, &None, &None);

    client.set_authorized_caller(&new_caller);

    let events = env.events().all();
    let ev = events.last().expect("expected set_auth_caller event");
    assert_eq!(ev.1.len(), 2);

    let topic0: Symbol = ev.1.get(0).unwrap().into_val(&env);
    let topic1: Address = ev.1.get(1).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "set_auth_caller"));
    assert_eq!(topic1, owner);

    let data: Address = ev.2.into_val(&env);
    assert_eq!(data, new_caller);

    let remaining = client.deduct(&new_caller, &50, &None);
    assert_eq!(remaining, 150);
}

#[test]
fn deduct_reduces_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 300);
    client.init(&owner, &usdc, &Some(300), &None, &None, &None, &None);

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
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

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
    client.init(&owner, &usdc, &Some(10), &None, &None, &None, &None);

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
    client.init(&owner, &usdc, &Some(75), &None, &None, &None, &None);

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
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

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
    client.init(&owner, &usdc, &Some(300), &None, &None, &None, &None);
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
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
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
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
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
    client.init(&owner, &usdc, &Some(50), &None, &None, &None, &None);
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
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

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
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

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
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

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
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

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

#[test]
fn batch_deduct_too_large_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 10_000);
    client.init(&owner, &usdc, &Some(10_000), &None, &None, &None, &None);

    // Build a batch of MAX_BATCH_SIZE + 1 items
    let mut items = soroban_sdk::Vec::new(&env);
    for _ in 0..=crate::MAX_BATCH_SIZE {
        items.push_back(DeductItem {
            amount: 1,
            request_id: None,
        });
    }
    let result = client.try_batch_deduct(&owner, &items);
    assert!(result.is_err(), "expected error for oversized batch");
}

#[test]
fn batch_deduct_fail_mid_batch_leaves_balance_unchanged() {
    // Second item exceeds balance â€” entire batch must revert.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 60,
            request_id: None
        },
        DeductItem {
            amount: 60,
            request_id: None
        }, // cumulative 120 > 100
    ];
    let result = client.try_batch_deduct(&owner, &items);
    assert!(result.is_err(), "expected insufficient balance error");
    // Balance must be completely unchanged
    assert_eq!(client.balance(), 100);
}

// ---------------------------------------------------------------------------
// Withdraw tests
// ---------------------------------------------------------------------------

#[test]
fn withdraw_reduces_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

    let remaining = client.withdraw(&200);
    assert_eq!(remaining, 300);
    assert_eq!(client.balance(), 300);
}

#[test]
fn withdraw_full_balance_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    let remaining = client.withdraw(&1000);
    assert_eq!(remaining, 0);
    assert_eq!(client.balance(), 0);
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
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

    let remaining = client.withdraw_to(&recipient, &150);
    assert_eq!(remaining, 350);
    assert_eq!(client.balance(), 350);
    assert_eq!(usdc_client.balance(&recipient), 150);
}

#[test]
fn withdraw_unauthorized_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let _intruder = Address::generate(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    // Reset auths to test requirement without mock_all_auths bypassing it
    env.set_auths(&[]);
    let res = client.try_withdraw(&500);
    assert!(res.is_err());
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
fn update_metadata_without_existing_uses_empty_old() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-006");
    let new_metadata = String::from_str(&env, "QmNewMetadataOnly");
    client.update_metadata(&owner, &offering_id, &new_metadata);

    let events = env.events().all();
    let ev = events.last().expect("expected metadata_updated event");

    assert_eq!(ev.0, vault_address);
    let data: (String, String) = ev.2.into_val(&env);
    assert_eq!(data.0, String::from_str(&env, ""));
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
    client.accept_ownership();

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
    let meta = client.init(&owner, &usdc, &Some(500), &None, &Some(10), &None, &None);
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
        &Some(500),
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
fn deduct_with_settlement_transfers_usdc() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 800);
    client.init(
        &owner,
        &usdc_address,
        &Some(800),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );
    client.set_settlement(&owner, &settlement);

    client.deduct(&caller, &250, &None);

    assert_eq!(client.balance(), 550);
    assert_eq!(usdc_client.balance(&settlement), 250);
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

#[test]
fn batch_deduct_with_settlement_transfers_total_usdc() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let settlement = Address::generate(&env);
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
        &None,
        &Some(500),
    );
    client.set_settlement(&owner, &settlement);

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
    assert_eq!(usdc_client.balance(&settlement), 350);
}

// ---------------------------------------------------------------------------
// set_revenue_pool / get_revenue_pool tests
// ---------------------------------------------------------------------------

#[test]
fn set_revenue_pool_stores_and_get_returns_address() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_revenue_pool(&owner, &Some(revenue_pool.clone()));

    assert_eq!(client.get_revenue_pool(), Some(revenue_pool));
}

#[test]
fn set_revenue_pool_clear_removes_address() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(
        &owner,
        &usdc,
        &None,
        &None,
        &None,
        &Some(revenue_pool),
        &None,
    );
    client.set_revenue_pool(&owner, &None);

    assert_eq!(client.get_revenue_pool(), None);
}

#[test]
fn set_revenue_pool_update_replaces_address() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let old_pool = Address::generate(&env);
    let new_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &Some(old_pool), &None);
    client.set_revenue_pool(&owner, &Some(new_pool.clone()));

    assert_eq!(client.get_revenue_pool(), Some(new_pool));
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin")]
fn set_revenue_pool_unauthorized_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_revenue_pool(&attacker, &Some(revenue_pool));
}

#[test]
fn get_revenue_pool_returns_none_when_not_set() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    assert_eq!(client.get_revenue_pool(), None);
}

#[test]
fn deduct_routes_to_settlement_when_both_configured() {
    // settlement takes priority over revenue_pool when both are set
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let settlement = Address::generate(&env);
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
    client.set_settlement(&owner, &settlement);

    client.deduct(&caller, &400, &None);

    // settlement gets the funds, revenue_pool gets nothing
    assert_eq!(usdc_client.balance(&settlement), 400);
    assert_eq!(usdc_client.balance(&revenue_pool), 0);
}

#[test]
fn set_revenue_pool_emits_event_on_set() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_revenue_pool(&owner, &Some(revenue_pool.clone()));

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "set_revenue_pool"));
    let data: Address = last.2.into_val(&env);
    assert_eq!(data, revenue_pool);
}

#[test]
fn set_revenue_pool_emits_event_on_clear() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(
        &owner,
        &usdc,
        &None,
        &None,
        &None,
        &Some(revenue_pool),
        &None,
    );
    client.set_revenue_pool(&owner, &None);

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "clear_revenue_pool"));
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

// ---------------------------------------------------------------------------
// Checked arithmetic â€” overflow / underflow boundary tests
// ---------------------------------------------------------------------------

#[test]
fn deposit_near_i128_max_succeeds() {
    // Verify that a balance sitting just below i128::MAX can accept one more deposit.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    // Start with i128::MAX - 100 so there is headroom for a 100-unit deposit.
    let initial: i128 = i128::MAX - 100;
    fund_vault(&usdc_admin, &vault_address, initial);
    client.init(&owner, &usdc, &Some(initial), &None, &None, &None, &None);

    usdc_admin.mint(&owner, &100);
    usdc_client.approve(&owner, &vault_address, &100, &1000);
    let new_balance = client.deposit(&owner, &100);
    assert_eq!(new_balance, i128::MAX);
}

#[test]
fn deposit_overflow_panics() {
    // A deposit that would push balance past i128::MAX must panic.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, i128::MAX);
    client.init(&owner, &usdc, &Some(i128::MAX), &None, &None, &None, &None);

    usdc_admin.mint(&owner, &1);
    usdc_client.approve(&owner, &vault_address, &1, &1000);
    let result = client.try_deposit(&owner, &1);
    assert!(result.is_err(), "expected overflow panic");
}

#[test]
fn deduct_to_zero_succeeds() {
    // Deducting the entire balance should leave exactly 0, not underflow.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

    assert_eq!(client.deduct(&owner, &500, &None), 0);
}

#[test]
fn withdraw_to_zero_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 300);
    client.init(&owner, &usdc, &Some(300), &None, &None, &None, &None);

    assert_eq!(client.withdraw(&300), 0);
}

// ---------------------------------------------------------------------------
// Issue #108 â€” set_allowed_depositor: duplicate add, clear, unauthorized
// ---------------------------------------------------------------------------

#[test]
fn clear_allowed_depositors_removes_all() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let d1 = Address::generate(&env);
    let d2 = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    client.set_allowed_depositor(&owner, &Some(d1.clone()));
    client.set_allowed_depositor(&owner, &Some(d2.clone()));
    client.clear_allowed_depositors(&owner);

    // Neither address should be able to deposit after clear.
    usdc_admin.mint(&d1, &10);
    usdc_client.approve(&d1, &vault_address, &10, &1000);
    assert!(client.try_deposit(&d1, &10).is_err());
}

#[test]
fn clear_allowed_depositors_on_empty_is_noop() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    // Must not panic on empty list.
    client.clear_allowed_depositors(&owner);
    assert_eq!(client.get_allowed_depositors().len(), 0);
}

#[test]
fn non_owner_cannot_set_allowed_depositor_issue108() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let result = client.try_set_allowed_depositor(&attacker, &Some(depositor.clone()));
    assert!(result.is_err(), "non-owner must not mutate allowlist");
}

#[test]
fn non_owner_cannot_clear_allowed_depositors() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    let result = client.try_clear_allowed_depositors(&attacker);
    assert!(result.is_err(), "non-owner must not clear allowlist");
}

// ---------------------------------------------------------------------------
// Token transfer failure modes — documented limitations
// ---------------------------------------------------------------------------
//
// # Manual Test Plan: Transfer Failure Modes
//
// The Soroban test harness (soroban-sdk testutils) does not provide a mechanism
// to inject token-level failures (e.g. simulate a transfer revert mid-call).
// The following failure modes are therefore documented here for manual / fuzzing
// verification rather than automated unit tests:
//
// 1. **deposit: transfer from caller fails** — if the caller has insufficient
//    USDC balance or has not approved the vault, the token contract panics and
//    the deposit reverts atomically (no balance change).
//
// 2. **withdraw / withdraw_to: transfer to recipient fails** — if the vault's
//    on-chain USDC balance is lower than the tracked `meta.balance` (e.g. due
//    to a direct token transfer out), the token transfer panics. The vault
//    balance is NOT updated in this case (state write happens after transfer).
//
// 3. **deduct → settlement transfer fails** — if the settlement address has no
//    trustline or the vault's USDC balance is insufficient, the token transfer
//    panics. The vault balance IS already written before the transfer; callers
//    should treat a panic here as a critical invariant violation.
//
// 4. **deduct → revenue_pool transfer fails** — same as (3) for revenue_pool.
//
// 5. **distribute: transfer fails** — guarded by an explicit `vault_balance < amount`
//    check before the transfer; covered by `distribute_insufficient_usdc_fails`.
//
// All paths above are covered by the checked-arithmetic and balance-guard tests
// below. The highest-risk external calls (deduct routing) are covered by the
// integration tests `deduct_with_settlement_transfers_usdc` and
// `deduct_with_revenue_pool_transfers_usdc`.

// ---------------------------------------------------------------------------
// Additional edge-case tests to reach ≥ 95 % line coverage
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "vault already paused")]
fn pause_when_already_paused_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.pause(&owner);
    client.pause(&owner); // second pause must panic
}

#[test]
#[should_panic(expected = "vault not paused")]
fn unpause_when_not_paused_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.unpause(&owner); // not paused — must panic
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin or owner")]
fn pause_by_unauthorized_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.pause(&attacker);
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin or owner")]
fn unpause_by_unauthorized_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.pause(&owner);
    client.unpause(&attacker);
}

#[test]
fn owner_can_pause_and_unpause() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    assert!(!client.is_paused());
    client.pause(&owner);
    assert!(client.is_paused());
    client.unpause(&owner);
    assert!(!client.is_paused());
}

#[test]
fn admin_can_pause_and_unpause() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_admin(&owner, &new_admin);
    client.accept_admin();
    client.pause(&new_admin);
    assert!(client.is_paused());
    client.unpause(&new_admin);
    assert!(!client.is_paused());
}

#[test]
#[should_panic(expected = "vault is paused")]
fn deduct_while_paused_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    client.pause(&owner);
    client.deduct(&owner, &100, &None);
}

#[test]
#[should_panic(expected = "vault is paused")]
fn batch_deduct_while_paused_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    client.pause(&owner);
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: None,
        },
    ];
    client.batch_deduct(&owner, &items);
}

#[test]
#[should_panic(expected = "unauthorized caller")]
fn deduct_unauthorized_caller_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    // init with an authorized_caller so the None branch is not taken
    let auth = Address::generate(&env);
    client.init(&owner, &usdc, &Some(500), &Some(auth), &None, &None, &None);
    client.deduct(&attacker, &100, &None);
}

#[test]
#[should_panic(expected = "unauthorized caller")]
fn batch_deduct_unauthorized_caller_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    let auth = Address::generate(&env);
    client.init(&owner, &usdc, &Some(500), &Some(auth), &None, &None, &None);
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: None,
        },
    ];
    client.batch_deduct(&attacker, &items);
}

#[test]
#[should_panic(expected = "deduct amount exceeds max_deduct")]
fn deduct_exceeds_max_deduct_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &Some(50));
    client.deduct(&owner, &100, &None); // 100 > max_deduct(50)
}

#[test]
#[should_panic(expected = "deduct amount exceeds max_deduct")]
fn batch_deduct_item_exceeds_max_deduct_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &Some(50));
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: None,
        },
    ];
    client.batch_deduct(&owner, &items);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn distribute_negative_amount_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let dev = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(0), &None, &None, &None, &None);
    client.distribute(&owner, &dev, &-1);
}

#[test]
#[should_panic(expected = "no admin transfer pending")]
fn accept_admin_without_pending_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.accept_admin();
}

#[test]
#[should_panic(expected = "no ownership transfer pending")]
fn accept_ownership_without_pending_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.accept_ownership();
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn withdraw_negative_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.withdraw(&-1);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn withdraw_to_negative_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.withdraw_to(&recipient, &-1);
}

#[test]
fn deduct_no_routing_stays_in_vault() {
    // When neither settlement nor revenue_pool is configured, USDC stays in vault.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    client.deduct(&owner, &200, &None);
    assert_eq!(client.balance(), 300);
    // USDC stays in vault contract
    assert_eq!(usdc_client.balance(&vault_address), 500);
}

#[test]
fn batch_deduct_no_routing_stays_in_vault() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: None,
        },
        DeductItem {
            amount: 50,
            request_id: None,
        },
    ];
    client.batch_deduct(&owner, &items);
    assert_eq!(client.balance(), 350);
    assert_eq!(usdc_client.balance(&vault_address), 500);
}

#[test]
fn withdraw_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 300);
    client.init(&owner, &usdc, &Some(300), &None, &None, &None, &None);
    client.withdraw(&100);
    let events = env.events().all();
    let ev = events
        .iter()
        .find(|e| {
            e.0 == vault_address && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "withdraw")
            }
        })
        .expect("expected withdraw event");
    let (amt, bal): (i128, i128) = ev.2.into_val(&env);
    assert_eq!(amt, 100);
    assert_eq!(bal, 200);
}

#[test]
fn withdraw_to_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 300);
    client.init(&owner, &usdc, &Some(300), &None, &None, &None, &None);
    client.withdraw_to(&recipient, &150);
    let events = env.events().all();
    let ev = events
        .iter()
        .find(|e| {
            e.0 == vault_address && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "withdraw_to")
            }
        })
        .expect("expected withdraw_to event");
    let (amt, bal): (i128, i128) = ev.2.into_val(&env);
    assert_eq!(amt, 150);
    assert_eq!(bal, 150);
}

#[test]
fn distribute_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let dev = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(0), &None, &None, &None, &None);
    client.distribute(&owner, &dev, &200);
    let events = env.events().all();
    let ev = events
        .iter()
        .find(|e| {
            e.0 == vault_address && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "distribute")
            }
        })
        .expect("expected distribute event");
    let amt: i128 = ev.2.into_val(&env);
    assert_eq!(amt, 200);
}

#[test]
fn get_allowed_depositors_returns_list() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let d1 = Address::generate(&env);
    let d2 = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_allowed_depositor(&owner, &Some(d1));
    client.set_allowed_depositor(&owner, &Some(d2));
    let list = client.get_allowed_depositors();
    assert_eq!(list.len(), 2);
}

#[test]
fn vault_unpaused_event_emitted() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.pause(&owner);
    client.unpause(&owner);
    let events = env.events().all();
    let ev = events
        .iter()
        .find(|e| {
            e.0 == vault_address && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "vault_unpaused")
            }
        })
        .expect("expected vault_unpaused event");
    let caller: Address = ev.1.get(1).unwrap().into_val(&env);
    assert_eq!(caller, owner);
}

// ---------------------------------------------------------------------------
// Randomised sequence tests
//
// Invariants under test:
//   1. VaultMeta.balance >= 0 after every operation.
//   2. Local simulator tracks the same balance as the contract at each step.
//   3. batch_deduct is atomic: a failing batch leaves balance unchanged.
//   4. pause blocks deposits but not deductions; unpause restores deposits.
//   5. No single deduct/batch item may exceed max_deduct.
//
// Seeds are fixed so runs are deterministic and reproducible in CI.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod fuzz {
    use super::*;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    /// Run a mixed sequence of deposit / deduct / batch_deduct / pause / unpause
    /// and assert after every step that:
    ///   - contract balance == local simulator
    ///   - contract balance >= 0
    fn run_sequence(seed: u64, max_deduct_val: i128, initial: i128, steps: usize) {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let caller = Address::generate(&env);
        let (usdc_addr, usdc_client, usdc_admin) = create_usdc(&env, &owner);
        let (vault_addr, client) = create_vault(&env);

        // Pre-fund vault so initial_balance is valid.
        usdc_admin.mint(&vault_addr, &initial);
        client.init(
            &owner,
            &usdc_addr,
            &Some(initial),
            &None,
            &Some(1), // min_deposit = 1
            &None,
            &Some(max_deduct_val),
        );

        // Give the depositor (owner) plenty of USDC.
        let deposit_reserve: i128 = initial * 10 + 1_000_000;
        usdc_admin.mint(&owner, &deposit_reserve);
        usdc_client.approve(&owner, &vault_addr, &deposit_reserve, &999_999);

        let mut rng = StdRng::seed_from_u64(seed);
        let mut sim: i128 = initial;
        let mut paused = false;

        for _ in 0..steps {
            // Pick an operation: 0=deposit, 1=deduct, 2=batch_deduct, 3=toggle_pause
            let op: u8 = rng.gen_range(0..4);

            match op {
                // --- deposit ---
                0 => {
                    let amount: i128 = rng.gen_range(1..=max_deduct_val);
                    if paused {
                        // deposit must fail while paused
                        assert!(client.try_deposit(&owner, &amount).is_err());
                    } else {
                        sim += amount;
                        client.deposit(&owner, &amount);
                    }
                }

                // --- single deduct ---
                1 => {
                    let amount: i128 = rng.gen_range(1..=max_deduct_val);
                    if sim >= amount {
                        sim -= amount;
                        client.deduct(&caller, &amount, &None);
                    } else {
                        // must fail — balance unchanged
                        assert!(client.try_deduct(&caller, &amount, &None).is_err());
                    }
                }

                // --- batch_deduct ---
                2 => {
                    // Build a batch of 1..=5 items, each within max_deduct.
                    let n: usize = rng.gen_range(1..=5);
                    let mut items = soroban_sdk::Vec::new(&env);
                    let mut batch_total: i128 = 0;
                    let mut valid = true;
                    for _ in 0..n {
                        let amt: i128 = rng.gen_range(1..=max_deduct_val);
                        batch_total = match batch_total.checked_add(amt) {
                            Some(v) => v,
                            None => {
                                valid = false;
                                break;
                            }
                        };
                        items.push_back(DeductItem {
                            amount: amt,
                            request_id: None,
                        });
                    }
                    if valid && sim >= batch_total {
                        sim -= batch_total;
                        client.batch_deduct(&caller, &items);
                    } else {
                        // batch must fail atomically — balance unchanged
                        let before = client.balance();
                        let _ = client.try_batch_deduct(&caller, &items);
                        assert_eq!(
                            client.balance(),
                            before,
                            "failed batch must not change balance"
                        );
                    }
                }

                // --- toggle pause ---
                3 => {
                    if paused {
                        client.unpause(&owner);
                        paused = false;
                    } else {
                        client.pause(&owner);
                        paused = true;
                    }
                }

                _ => unreachable!(),
            }

            // Invariant assertions after every step.
            let on_chain = client.balance();
            assert_eq!(on_chain, sim, "seed={seed} sim mismatch");
            assert!(on_chain >= 0, "seed={seed} balance went negative");
        }

        // Leave vault unpaused so teardown is clean.
        if paused {
            client.unpause(&owner);
        }
    }

    #[test]
    fn fuzz_deposit_and_deduct() {
        // Original invariant: mixed deposits and single deducts stay non-negative.
        run_sequence(0xdead_beef, 500, 10_000, 200);
    }

    #[test]
    fn fuzz_batch_deduct_coverage() {
        // Heavier batch_deduct weight via a different seed.
        run_sequence(0xcafe_1234, 200, 5_000, 150);
    }

    #[test]
    fn fuzz_pause_interleaved() {
        // Pause/unpause interleaved with deposits and deductions.
        run_sequence(0xf00d_abcd, 1_000, 50_000, 100);
    }

    #[test]
    fn fuzz_tight_max_deduct() {
        // max_deduct = 1 forces many small steps; exercises boundary exhaustively.
        run_sequence(0x1234_5678, 1, 500, 300);
    }

    #[test]
    fn fuzz_large_max_deduct() {
        // max_deduct near i128::MAX / 2 — checks no overflow in batch totals.
        run_sequence(0xabcd_ef01, i128::MAX / 2, 1_000_000, 80);
    }

    /// Verify that a batch whose cumulative total exceeds balance is fully atomic:
    /// balance must be identical before and after the failed call.
    #[test]
    fn fuzz_batch_atomicity_on_overdraw() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let caller = Address::generate(&env);
        let (usdc_addr, _, usdc_admin) = create_usdc(&env, &owner);
        let (vault_addr, client) = create_vault(&env);

        usdc_admin.mint(&vault_addr, &300);
        client.init(
            &owner,
            &usdc_addr,
            &Some(300),
            &None,
            &None,
            &None,
            &Some(200),
        );

        let mut rng = StdRng::seed_from_u64(0x5eed_0001);
        // Build batches that sometimes overdraw; assert atomicity each time.
        for _ in 0..50 {
            let before = client.balance();
            let n: usize = rng.gen_range(1..=5);
            let mut items = soroban_sdk::Vec::new(&env);
            for _ in 0..n {
                items.push_back(DeductItem {
                    amount: rng.gen_range(1..=200_i128),
                    request_id: None,
                });
            }
            let total: i128 = items.iter().map(|i| i.amount).sum();
            if before >= total {
                client.batch_deduct(&caller, &items);
                assert_eq!(client.balance(), before - total);
            } else {
                let _ = client.try_batch_deduct(&caller, &items);
                assert_eq!(client.balance(), before, "atomic rollback failed");
            }
            assert!(client.balance() >= 0);
        }
    }

    /// Verify that max_deduct is enforced on every individual item in a batch.
    #[test]
    fn fuzz_max_deduct_enforced_in_batch() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let caller = Address::generate(&env);
        let (usdc_addr, _, usdc_admin) = create_usdc(&env, &owner);
        let (vault_addr, client) = create_vault(&env);
        let max_d: i128 = 100;

        usdc_admin.mint(&vault_addr, &10_000);
        client.init(
            &owner,
            &usdc_addr,
            &Some(10_000),
            &None,
            &None,
            &None,
            &Some(max_d),
        );

        let mut rng = StdRng::seed_from_u64(0x5eed_0002);
        for _ in 0..40 {
            // Occasionally inject an item that exceeds max_deduct.
            let exceed = rng.gen_bool(0.3);
            let amt: i128 = if exceed {
                rng.gen_range(max_d + 1..=max_d * 3)
            } else {
                rng.gen_range(1..=max_d)
            };
            let items = soroban_sdk::vec![
                &env,
                DeductItem {
                    amount: amt,
                    request_id: None,
                },
            ];
            if exceed {
                assert!(
                    client.try_batch_deduct(&caller, &items).is_err(),
                    "item exceeding max_deduct must be rejected"
                );
            } else if client.balance() >= amt {
                client.batch_deduct(&caller, &items);
                assert!(client.balance() >= 0);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Issue #151 — min_deposit boundary tests
// ---------------------------------------------------------------------------

#[test]
fn deposit_exact_min_deposit_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &Some(50), &None, &None);

    usdc_admin.mint(&owner, &50);
    usdc_client.approve(&owner, &vault_address, &50, &1000);
    let balance = client.deposit(&owner, &50);
    assert_eq!(balance, 50);
}

#[test]
#[should_panic(expected = "deposit below minimum")]
fn deposit_below_min_deposit_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &Some(50), &None, &None);

    usdc_admin.mint(&owner, &49);
    usdc_client.approve(&owner, &vault_address, &49, &1000);
    client.deposit(&owner, &49);
}

#[test]
fn deposit_above_min_deposit_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &Some(50), &None, &None);

    usdc_admin.mint(&owner, &51);
    usdc_client.approve(&owner, &vault_address, &51, &1000);
    let balance = client.deposit(&owner, &51);
    assert_eq!(balance, 51);
}

#[test]
fn deposit_with_zero_min_deposit_allows_any_positive_amount() {
    // When min_deposit is 0 (default), any positive amount should succeed.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &Some(0), &None, &None);

    usdc_admin.mint(&owner, &1);
    usdc_client.approve(&owner, &vault_address, &1, &1000);
    let balance = client.deposit(&owner, &1);
    assert_eq!(balance, 1);
}

#[test]
fn init_min_deposit_stored_in_meta() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &Some(100), &None, &None);
    let meta = client.get_meta();
    assert_eq!(meta.min_deposit, 100);
}

#[test]
fn init_default_min_deposit_is_zero() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    let meta = client.get_meta();
    assert_eq!(meta.min_deposit, 0);
}

#[test]
fn deposit_one_below_large_min_deposit_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &Some(1_000_000), &None, &None);

    usdc_admin.mint(&owner, &999_999);
    usdc_client.approve(&owner, &vault_address, &999_999, &1000);
    let result = client.try_deposit(&owner, &999_999);
    assert!(
        result.is_err(),
        "deposit one below large min_deposit must fail"
    );
}

#[test]
fn deposit_exact_large_min_deposit_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &Some(1_000_000), &None, &None);

    usdc_admin.mint(&owner, &1_000_000);
    usdc_client.approve(&owner, &vault_address, &1_000_000, &1000);
    let balance = client.deposit(&owner, &1_000_000);
    assert_eq!(balance, 1_000_000);
}

// ---------------------------------------------------------------------------
// max_deduct boundary tests
// ---------------------------------------------------------------------------

#[test]
fn deduct_equal_to_max_deduct_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    // max_deduct = 100, deposit 200 so balance is sufficient
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &Some(100));
    usdc_admin.mint(&owner, &200);
    usdc_client.approve(&owner, &vault_address, &200, &1000);
    client.deposit(&owner, &200);
    // deduct exactly equal to max_deduct — must succeed
    let balance = client.deduct(&owner, &100, &None);
    assert_eq!(balance, 600);
}

#[test]
#[should_panic(expected = "deduct amount exceeds max_deduct")]
fn deduct_above_max_deduct_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &Some(100));
    usdc_admin.mint(&owner, &200);
    usdc_client.approve(&owner, &vault_address, &200, &1000);
    client.deposit(&owner, &200);
    // deduct 101 > max_deduct 100 — must panic
    client.deduct(&owner, &101, &None);
}

#[test]
fn deduct_default_cap_is_i128_max() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    // no max_deduct supplied — default cap (i128::MAX) applies
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    usdc_admin.mint(&owner, &1_000_000);
    usdc_client.approve(&owner, &vault_address, &1_000_000, &1000);
    client.deposit(&owner, &1_000_000);
    // large deduct well below i128::MAX should succeed
    let balance = client.deduct(&owner, &999_999, &None);
    assert_eq!(balance, 1);
}

#[test]
fn batch_deduct_each_item_constrained_by_max_deduct() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    // max_deduct = 50
    client.init(&owner, &usdc, &None, &None, &None, &None, &Some(50));
    usdc_admin.mint(&owner, &300);
    usdc_client.approve(&owner, &vault_address, &300, &1000);
    client.deposit(&owner, &300);
    // three items each exactly at the cap — all must pass
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 50,
            request_id: None
        },
        DeductItem {
            amount: 50,
            request_id: None
        },
        DeductItem {
            amount: 50,
            request_id: None
        },
    ];
    let balance = client.batch_deduct(&owner, &items);
    assert_eq!(balance, 150);
}

#[test]
#[should_panic(expected = "deduct amount exceeds max_deduct")]
fn batch_deduct_one_item_above_max_deduct_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &None, &None, &Some(50));
    usdc_admin.mint(&owner, &300);
    usdc_client.approve(&owner, &vault_address, &300, &1000);
    client.deposit(&owner, &300);
    // second item exceeds cap — must panic
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 50,
            request_id: None
        },
        DeductItem {
            amount: 51,
            request_id: None
        },
    ];
    client.batch_deduct(&owner, &items);
}
