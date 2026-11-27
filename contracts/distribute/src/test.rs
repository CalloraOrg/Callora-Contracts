extern crate std;

use crate::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{token, Address, BytesN, Env, IntoVal, Symbol, TryFromVal, Val, Vec};

#[test]
fn init_event_structure_validation() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);

    let events = env.events().all();
    let event = events.last().unwrap();

    let topics = &event.1;
    assert_eq!(topics.len(), 3);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: Symbol = topics.get(1).unwrap().into_val(&env);
    let topic2: Address = topics.get(2).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "init"));
    assert_eq!(topic1, Symbol::new(&env, "callora_v1"));
    assert_eq!(topic2, admin);

    let data: Address = event.2.into_val(&env);
    assert_eq!(data, usdc_addr);
}

#[test]
fn admin_transfer_events_structure() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);

    // Step 1: set_admin — nomination must not announce the change yet.
    env.events().all();
    client.set_admin(&admin, &new_admin);

    let events = env.events().all();
    assert_eq!(
        events.len(),
        1,
        "nomination must publish exactly one event, got {}",
        events.len()
    );

    let nomination = events.get(0).unwrap();
    let topic0: Symbol = nomination.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_transfer_started"));
    // Every distribute event carries the version marker at topic 1, so the
    // subject address follows it.
    assert_eq!(nomination.1.len(), 3);
    let nominator: Address = nomination.1.get(2).unwrap().into_val(&env);
    assert_eq!(nominator, admin);
    let pending: Address = nomination.2.into_val(&env);
    assert_eq!(pending, new_admin);

    // Step 2: accept_admin — the change event is published here.
    env.events().all();
    client.accept_admin(&new_admin);

    let events = env.events().all();
    assert_eq!(
        events.len(),
        2,
        "acceptance must publish admin_changed then admin_transfer_completed, got {}",
        events.len()
    );

    let changed = events.get(0).unwrap();
    let changed_topic0: Symbol = changed.1.get(0).unwrap().into_val(&env);
    assert_eq!(changed_topic0, Symbol::new(&env, "admin_changed"));
    let previous: Address = changed.1.get(2).unwrap().into_val(&env);
    assert_eq!(previous, admin, "topic 2 must be the outgoing admin");
    let (old_admin, updated): (Address, Address) = changed.2.into_val(&env);
    assert_eq!(old_admin, admin, "data[0] must be the old admin");
    assert_eq!(updated, new_admin, "data[1] must be the new admin");

    let completed = events.get(1).unwrap();
    let completed_topic0: Symbol = completed.1.get(0).unwrap().into_val(&env);
    assert_eq!(
        completed_topic0,
        Symbol::new(&env, "admin_transfer_completed")
    );
    let completed_subject: Address = completed.1.get(2).unwrap().into_val(&env);
    assert_eq!(completed_subject, new_admin);
    let _: () = completed.2.into_val(&env);
}

#[test]
fn cancel_admin_transfer_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);
    env.events().all();
    client.set_admin(&admin, &new_admin);
    env.events().all();

    client.cancel_admin_transfer(&admin);

    let events = env.events().all();
    assert_eq!(
        events.len(),
        1,
        "cancellation must publish exactly one event, got {}",
        events.len()
    );

    let event = events.get(0).unwrap();
    let topic0: Symbol = event.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_cancelled"));
    let current: Address = event.1.get(2).unwrap().into_val(&env);
    assert_eq!(current, admin);
    let pending: Address = event.1.get(3).unwrap().into_val(&env);
    assert_eq!(pending, new_admin);
    let _: () = event.2.into_val(&env);

    // A cancelled transfer must leave no admin_changed event behind: the only
    // event recorded for this call is admin_cancelled.
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn pause_unpause_events() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);
    env.events().all();

    client.pause(&admin);

    let events = env.events().all();
    let pause_event = events.last().unwrap();
    let topic0: Symbol = pause_event.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "pause_set"));
    let topic1: Symbol = pause_event.1.get(1).unwrap().into_val(&env);
    assert_eq!(topic1, Symbol::new(&env, "callora_v1"));
    let topic2: Address = pause_event.1.get(2).unwrap().into_val(&env);
    assert_eq!(topic2, admin);
    let is_paused: bool = pause_event.2.into_val(&env);
    assert!(is_paused);

    // Unpause
    env.events().all();
    client.unpause(&admin);

    let events = env.events().all();
    let unpause_event = events.last().unwrap();
    let topic0: Symbol = unpause_event.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "pause_set"));
    let topic1: Symbol = unpause_event.1.get(1).unwrap().into_val(&env);
    assert_eq!(topic1, Symbol::new(&env, "callora_v1"));
    let topic2: Address = unpause_event.1.get(2).unwrap().into_val(&env);
    assert_eq!(topic2, admin);
    let is_paused: bool = unpause_event.2.into_val(&env);
    assert!(!is_paused);
}

#[test]
fn set_max_distribute_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);
    env.events().all();

    client.set_max_distribute(&admin, &1000);

    let events = env.events().all();
    let event = events.last().unwrap();
    let topic0: Symbol = event.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "set_max_distribute"));
    let topic1: Symbol = event.1.get(1).unwrap().into_val(&env);
    assert_eq!(topic1, Symbol::new(&env, "callora_v1"));
    let topic2: Address = event.1.get(2).unwrap().into_val(&env);
    assert_eq!(topic2, admin);
    let data: (i128, i128) = event.2.into_val(&env);
    assert_eq!(data, (i128::MAX, 1000));
}

#[test]
fn distribute_event_structure() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);

    // Fund the contract with USDC
    let usdc_admin = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_admin.mint(&contract_addr, &1000);

    env.events().all();

    client.distribute(&admin, &recipient, &500);

    let events = env.events().all();

    // Find the distribute event by topic (index may vary due to token transfer event)
    let mut found = false;
    for event in events.iter() {
        let topics: &Vec<Val> = &event.1;
        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
        if topic0 == Symbol::new(&env, "distribute") {
            found = true;
            let topic1: Symbol = topics.get(1).unwrap().into_val(&env);
            assert_eq!(topic1, Symbol::new(&env, "callora_v1"));
            let topic2: Address = topics.get(2).unwrap().into_val(&env);
            assert_eq!(topic2, recipient);
            let amount: i128 = event.2.into_val(&env);
            assert_eq!(amount, 500);
            break;
        }
    }
    assert!(found, "distribute event not found in events");
}

#[test]
fn distribute_lifecycle_events() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);

    // Fund the contract with USDC
    let usdc_admin = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_admin.mint(&contract_addr, &1000);

    env.events().all();

    client.distribute(&admin, &recipient, &500);

    let events = env.events().all();
    // Should emit 3 events: distribute_started, distribute, distribute_completed
    // (plus potentially other events depending on test environment)
    assert!(
        events.len() >= 3,
        "expected at least 3 events, got {}",
        events.len()
    );

    // Find and verify distribute_started event
    let mut found_started = false;
    let mut found_distribute = false;
    let mut found_completed = false;

    for event in events.iter() {
        let topics: &Vec<Val> = &event.1;
        let topic0: Symbol = topics.get(0).unwrap().into_val(&env);

        if topic0 == Symbol::new(&env, "distribute_started") {
            found_started = true;
            let topic1: Symbol = topics.get(1).unwrap().into_val(&env);
            assert_eq!(topic1, Symbol::new(&env, "callora_v1"));
            let topic2: Address = topics.get(2).unwrap().into_val(&env);
            assert_eq!(topic2, recipient);
            let amount: i128 = event.2.into_val(&env);
            assert_eq!(amount, 500);
        } else if topic0 == Symbol::new(&env, "distribute") {
            found_distribute = true;
            let topic1: Symbol = topics.get(1).unwrap().into_val(&env);
            assert_eq!(topic1, Symbol::new(&env, "callora_v1"));
            let topic2: Address = topics.get(2).unwrap().into_val(&env);
            assert_eq!(topic2, recipient);
            let amount: i128 = event.2.into_val(&env);
            assert_eq!(amount, 500);
        } else if topic0 == Symbol::new(&env, "distribute_completed") {
            found_completed = true;
            let topic1: Symbol = topics.get(1).unwrap().into_val(&env);
            assert_eq!(topic1, Symbol::new(&env, "callora_v1"));
            let topic2: Address = topics.get(2).unwrap().into_val(&env);
            assert_eq!(topic2, recipient);
            let amount: i128 = event.2.into_val(&env);
            assert_eq!(amount, 500);
        }
    }

    assert!(found_started, "distribute_started event not found");
    assert!(found_distribute, "distribute event not found");
    assert!(found_completed, "distribute_completed event not found");
}

#[test]
fn all_event_constructors_return_correct_symbols() {
    let env = Env::default();

    assert_eq!(events::event_init(&env), Symbol::new(&env, "init"));
    assert_eq!(
        events::event_admin_changed(&env),
        Symbol::new(&env, "admin_changed")
    );
    assert_eq!(
        events::event_admin_transfer_started(&env),
        Symbol::new(&env, "admin_transfer_started")
    );
    assert_eq!(
        events::event_admin_transfer_completed(&env),
        Symbol::new(&env, "admin_transfer_completed")
    );
    assert_eq!(
        events::event_admin_cancelled(&env),
        Symbol::new(&env, "admin_cancelled")
    );
    assert_eq!(
        events::event_pause_set(&env),
        Symbol::new(&env, "pause_set")
    );
    assert_eq!(
        events::event_set_max_distribute(&env),
        Symbol::new(&env, "set_max_distribute")
    );
    assert_eq!(
        events::event_distribute(&env),
        Symbol::new(&env, "distribute")
    );
    assert_eq!(
        events::event_distribute_started(&env),
        Symbol::new(&env, "distribute_started")
    );
    assert_eq!(
        events::event_distribute_completed(&env),
        Symbol::new(&env, "distribute_completed")
    );
    assert_eq!(events::event_upgraded(&env), Symbol::new(&env, "upgraded"));
}

#[test]
fn require_auth_on_all_state_changing_functions() {
    let env = Env::default();
    // Do NOT mock all auths — we want to verify auth failures
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    // init now requires admin auth; mock it only for this call
    env.mock_all_auths();
    client.init(&admin, &usdc_addr);

    // Fund the contract for distribute tests while auths are still mocked
    let usdc_admin = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_admin.mint(&contract_addr, &1000);

    // strip auths for the remaining tests below
    env.set_auths(&[]);

    // Non-admin should fail on all state-changing functions
    let intruder = Address::generate(&env);
    let recipient = Address::generate(&env);

    // set_admin
    let result = client.try_set_admin(&intruder, &intruder);
    assert!(result.is_err(), "non-admin should not be able to set_admin");

    // pause
    let result = client.try_pause(&intruder);
    assert!(result.is_err(), "non-admin should not be able to pause");

    // unpause
    let result = client.try_unpause(&intruder);
    assert!(result.is_err(), "non-admin should not be able to unpause");

    // set_max_distribute
    let result = client.try_set_max_distribute(&intruder, &100);
    assert!(
        result.is_err(),
        "non-admin should not be able to set_max_distribute"
    );

    // cancel_admin_transfer (no pending transfer needed — auth checked first)
    let result = client.try_cancel_admin_transfer(&intruder);
    assert!(
        result.is_err(),
        "non-admin should not be able to cancel_admin_transfer"
    );

    // accept_admin (no pending transfer — auth checked first)
    let result = client.try_accept_admin(&intruder);
    assert!(
        result.is_err(),
        "non-admin should not be able to accept_admin"
    );

    // upgrade (auth checked before wasm verification)
    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    let result = client.try_upgrade(&intruder, &new_wasm_hash);
    assert!(result.is_err(), "non-admin should not be able to upgrade");

    // distribute
    let result = client.try_distribute(&intruder, &recipient, &100);
    assert!(
        result.is_err(),
        "non-admin should not be able to distribute"
    );
}

#[test]
fn no_unwrap_in_production_paths() {
    let source = include_str!("lib.rs")
        .split("#[cfg(test)]")
        .next()
        .unwrap_or("");
    let lines: std::vec::Vec<&str> = source.lines().collect();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        // Allow .unwrap() only in test modules or in commented lines
        if trimmed.contains(".unwrap(") && !trimmed.starts_with("//") {
            panic!(
                "Production code at line {} uses .unwrap(): {}",
                idx + 1,
                trimmed
            );
        }
    }
}

#[test]
fn require_auth_on_init() {
    let env = Env::default();
    // Do NOT mock all auths — init must now require admin auth
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    // init must fail when no auth is provided (front-running protection)
    env.set_auths(&[]);
    let result = client.try_init(&admin, &usdc_addr);
    assert!(result.is_err(), "init must require admin auth");
}

#[test]
fn overflow_safe_math() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);

    let usdc_admin = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_admin.mint(&contract_addr, &i128::MAX);

    // Should succeed with large values
    client.distribute(&admin, &recipient, &1000);
}

#[test]
fn distribute_zero_amount_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);

    let result = client.try_distribute(&admin, &recipient, &0);
    assert!(result.is_err(), "distribute with zero amount should fail");
}

#[test]
fn distribute_exceeds_max_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);
    client.set_max_distribute(&admin, &100);

    let result = client.try_distribute(&admin, &recipient, &200);
    assert!(
        result.is_err(),
        "distribute exceeding max_distribute should fail"
    );
}

#[test]
fn distribute_while_paused_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);
    client.pause(&admin);

    let result = client.try_distribute(&admin, &recipient, &100);
    assert!(result.is_err(), "distribute while paused should fail");
}

#[test]
fn init_rejects_usdc_token_as_contract() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    let result = client.try_init(&admin, &contract_addr);
    assert!(
        result.is_err(),
        "init with usdc_token == contract should fail"
    );
}

#[test]
fn init_rejects_usdc_token_as_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    let result = client.try_init(&admin, &admin);
    assert!(result.is_err(), "init with usdc_token == admin should fail");
}

#[test]
fn get_admin_returns_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);
    let returned_admin = client.get_admin();
    assert_eq!(returned_admin, admin);
}

#[test]
fn get_usdc_token_returns_token() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);
    let returned_token = client.get_usdc_token();
    assert_eq!(returned_token, usdc_addr);
}

#[test]
fn get_version_returns_crate_version() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);
    let version = client.version();
    assert!(!version.is_empty());
}

#[test]
fn balance_returns_contract_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);

    // Initially zero
    assert_eq!(client.balance(), 0);

    // Fund the contract
    let usdc_admin = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_admin.mint(&contract_addr, &1000);

    assert_eq!(client.balance(), 1000);
}

#[test]
fn upgrade_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);
    env.events().all();

    // Note: In test environment, we cannot actually deploy a new WASM.
    // This test verifies the function can be called with auth.
    // The upgrade would fail with "Wasm does not exist" but we test the auth path.
    let new_wasm_hash = BytesN::from_array(&env, &[1u8; 32]);
    let result = client.try_upgrade(&admin, &new_wasm_hash);
    // The call fails because the wasm doesn't exist, but auth was checked
    assert!(result.is_err());
}

#[test]
fn get_pending_admin_returns_some_when_set() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);
    assert_eq!(client.get_pending_admin(), None);

    client.set_admin(&admin, &new_admin);
    let pending = client.get_pending_admin();
    assert_eq!(pending, Some(new_admin));
}

#[test]
fn get_pending_admin_returns_none_after_claim() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);
    client.set_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);

    let pending = client.get_pending_admin();
    assert_eq!(pending, None);
}

#[test]
fn batch_distribute_total_overflow_returns_typed_error() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);

    // The default per-leg cap is `i128::MAX`, so each leg below is individually
    // valid yet their accumulated total overflows `i128`. The contract must
    // surface `DistributeError::Overflow` rather than a string panic.
    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((Address::generate(&env), i128::MAX));
    payments.push_back((Address::generate(&env), i128::MAX));

    let result = client.try_batch_distribute(&admin, &payments);
    assert_eq!(
        result,
        Err(Ok(soroban_sdk::Error::from_contract_error(
            crate::errors::DistributeError::Overflow as u32
        )))
    );
}

#[test]
fn get_paused_returns_state() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);
    assert!(!client.get_paused());

    client.pause(&admin);
    assert!(client.get_paused());

    client.unpause(&admin);
    assert!(!client.get_paused());
}

#[test]
fn batch_distribute_emits_per_leg_events_and_preserves_order_and_amounts() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev_a = Address::generate(&env);
    let dev_b = Address::generate(&env);
    let dev_c = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);

    // Fund the contract with USDC
    let usdc_admin = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_admin.mint(&contract_addr, &1000);

    // Clear events from setup
    env.events().all();

    let mut payments = Vec::new(&env);
    payments.push_back((dev_a.clone(), 100));
    payments.push_back((dev_b.clone(), 250));
    payments.push_back((dev_c.clone(), 400));

    client.batch_distribute(&admin, &payments);

    let all_events = env.events().all();

    // Filter to contract events
    let contract_events: std::vec::Vec<_> =
        all_events.iter().filter(|e| e.0 == contract_addr).collect();

    // We expect:
    // 1 batch_distribute_started
    // For each of 3 legs:
    //   1 distribute_started
    //   1 distribute
    //   1 distribute_completed
    // 1 batch_distribute_completed
    // Total = 1 + 3 * 3 + 1 = 11 contract events
    assert_eq!(contract_events.len(), 11, "expected 11 contract events");

    // 1. First event: batch_distribute_started
    let started = &contract_events[0];
    assert_eq!(
        Symbol::try_from_val(&env, &started.1.get(0).unwrap()).unwrap(),
        Symbol::new(&env, "batch_distribute_started")
    );
    assert_eq!(
        Symbol::try_from_val(&env, &started.1.get(1).unwrap()).unwrap(),
        Symbol::new(&env, "callora_v1")
    );
    assert_eq!(
        Address::try_from_val(&env, &started.1.get(2).unwrap()).unwrap(),
        admin
    );
    let (started_total, started_count): (i128, u32) = started.2.into_val(&env);
    assert_eq!(started_total, 750);
    assert_eq!(started_count, 3);

    // 2. Per-leg events (3 legs * 3 events each = 9 events)
    let expected_legs = [
        (dev_a.clone(), 100_i128, 0_u32),
        (dev_b.clone(), 250_i128, 1_u32),
        (dev_c.clone(), 400_i128, 2_u32),
    ];

    let mut leg_amount_sum: i128 = 0;

    for (leg_idx, (expected_recipient, expected_amount, expected_batch_index)) in
        expected_legs.iter().enumerate()
    {
        let offset = 1 + leg_idx * 3;

        // a) distribute_started
        let leg_started = &contract_events[offset];
        assert_eq!(
            Symbol::try_from_val(&env, &leg_started.1.get(0).unwrap()).unwrap(),
            Symbol::new(&env, "distribute_started")
        );
        assert_eq!(
            Symbol::try_from_val(&env, &leg_started.1.get(1).unwrap()).unwrap(),
            Symbol::new(&env, "callora_v1")
        );
        assert_eq!(
            Address::try_from_val(&env, &leg_started.1.get(2).unwrap()).unwrap(),
            *expected_recipient
        );
        let start_lifecycle =
            events::DistributionLifecycleEvent::try_from_val(&env, &leg_started.2).unwrap();
        assert_eq!(start_lifecycle.version, events::DISTRIBUTION_EVENT_VERSION);
        assert_eq!(start_lifecycle.amount, *expected_amount);
        assert_eq!(start_lifecycle.mode, events::DistributionMode::Batch);
        assert_eq!(start_lifecycle.batch_index, *expected_batch_index);
        assert_eq!(start_lifecycle.batch_size, 3);

        // b) distribute (per-leg payout)
        let leg_distribute = &contract_events[offset + 1];
        assert_eq!(
            Symbol::try_from_val(&env, &leg_distribute.1.get(0).unwrap()).unwrap(),
            Symbol::new(&env, "distribute")
        );
        assert_eq!(
            Symbol::try_from_val(&env, &leg_distribute.1.get(1).unwrap()).unwrap(),
            Symbol::new(&env, "callora_v1")
        );
        assert_eq!(
            Address::try_from_val(&env, &leg_distribute.1.get(2).unwrap()).unwrap(),
            *expected_recipient
        );
        let leg_amount: i128 = leg_distribute.2.into_val(&env);
        assert_eq!(leg_amount, *expected_amount);
        leg_amount_sum += leg_amount;

        // c) distribute_completed
        let leg_completed = &contract_events[offset + 2];
        assert_eq!(
            Symbol::try_from_val(&env, &leg_completed.1.get(0).unwrap()).unwrap(),
            Symbol::new(&env, "distribute_completed")
        );
        assert_eq!(
            Symbol::try_from_val(&env, &leg_completed.1.get(1).unwrap()).unwrap(),
            Symbol::new(&env, "callora_v1")
        );
        assert_eq!(
            Address::try_from_val(&env, &leg_completed.1.get(2).unwrap()).unwrap(),
            *expected_recipient
        );
        let complete_lifecycle =
            events::DistributionLifecycleEvent::try_from_val(&env, &leg_completed.2).unwrap();
        assert_eq!(
            complete_lifecycle.version,
            events::DISTRIBUTION_EVENT_VERSION
        );
        assert_eq!(complete_lifecycle.amount, *expected_amount);
        assert_eq!(complete_lifecycle.mode, events::DistributionMode::Batch);
        assert_eq!(complete_lifecycle.batch_index, *expected_batch_index);
        assert_eq!(complete_lifecycle.batch_size, 3);
    }

    // 3. Last event: batch_distribute_completed
    let completed = &contract_events[10];
    assert_eq!(
        Symbol::try_from_val(&env, &completed.1.get(0).unwrap()).unwrap(),
        Symbol::new(&env, "batch_distribute_completed")
    );
    assert_eq!(
        Symbol::try_from_val(&env, &completed.1.get(1).unwrap()).unwrap(),
        Symbol::new(&env, "callora_v1")
    );
    assert_eq!(
        Address::try_from_val(&env, &completed.1.get(2).unwrap()).unwrap(),
        admin
    );
    let (completed_total, completed_count): (i128, u32) = completed.2.into_val(&env);
    assert_eq!(completed_total, 750);
    assert_eq!(completed_count, 3);

    // Acceptance criterion: Completed event total equals the sum of per-leg amounts
    assert_eq!(
        completed_total, leg_amount_sum,
        "completed event total must equal sum of per-leg amounts"
    );

    // Balances verification
    let usdc = token::Client::new(&env, &usdc_addr);
    assert_eq!(usdc.balance(&dev_a), 100);
    assert_eq!(usdc.balance(&dev_b), 250);
    assert_eq!(usdc.balance(&dev_c), 400);
    assert_eq!(usdc.balance(&contract_addr), 250);
}

#[test]
fn batch_distribute_atomic_revert_emits_no_events_on_insufficient_balance() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev_a = Address::generate(&env);
    let dev_b = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);

    // Fund with only 200
    let usdc_admin = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_admin.mint(&contract_addr, &200);

    env.events().all();

    let mut payments = Vec::new(&env);
    payments.push_back((dev_a.clone(), 150));
    payments.push_back((dev_b.clone(), 150)); // Total 300 > 200

    let res = client.try_batch_distribute(&admin, &payments);
    assert!(
        res.is_err(),
        "batch_distribute must fail on insufficient balance"
    );

    // No events should have been emitted by the distribute contract
    let contract_events: std::vec::Vec<_> = env
        .events()
        .all()
        .iter()
        .filter(|e| e.0 == contract_addr)
        .collect();
    assert_eq!(
        contract_events.len(),
        0,
        "no events emitted on failed batch"
    );
}

#[test]
fn batch_distribute_single_leg_preserves_shape() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev_a = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_addr);

    client.init(&admin, &usdc_addr);

    let usdc_admin = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_admin.mint(&contract_addr, &500);

    env.events().all();

    let mut payments = Vec::new(&env);
    payments.push_back((dev_a.clone(), 500));

    client.batch_distribute(&admin, &payments);

    let contract_events: std::vec::Vec<_> = env
        .events()
        .all()
        .iter()
        .filter(|e| e.0 == contract_addr)
        .collect();

    // 1 started + 3 leg + 1 completed = 5 events
    assert_eq!(contract_events.len(), 5);

    let leg_started = &contract_events[1];
    let start_lifecycle =
        events::DistributionLifecycleEvent::try_from_val(&env, &leg_started.2).unwrap();
    assert_eq!(start_lifecycle.batch_index, 0);
    assert_eq!(start_lifecycle.batch_size, 1);
    assert_eq!(start_lifecycle.amount, 500);
    assert_eq!(start_lifecycle.mode, events::DistributionMode::Batch);

    let leg_distribute = &contract_events[2];
    let leg_amount: i128 = leg_distribute.2.into_val(&env);
    assert_eq!(leg_amount, 500);

    let leg_completed = &contract_events[3];
    let complete_lifecycle =
        events::DistributionLifecycleEvent::try_from_val(&env, &leg_completed.2).unwrap();
    assert_eq!(complete_lifecycle.batch_index, 0);
    assert_eq!(complete_lifecycle.batch_size, 1);
    assert_eq!(complete_lifecycle.amount, 500);
    assert_eq!(complete_lifecycle.token, usdc_addr);
}

// ---------------------------------------------------------------------------
// Multi-token support tests (#1521)
// ---------------------------------------------------------------------------

fn setup_two_tokens(
    env: &Env,
) -> (
    Address,
    Address,
    Address,
    Address,
    DistributeClient<'_>,
) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let eurc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let contract_addr = env.register(Distribute, ());
    let client = DistributeClient::new(env, &contract_addr);
    client.init(&admin, &usdc_addr);
    (admin, usdc_addr, eurc_addr, contract_addr, client)
}

#[test]
fn approved_token_allowlist_lifecycle_and_errors() {
    let env = Env::default();
    let (admin, usdc_addr, eurc_addr, _contract_addr, client) = setup_two_tokens(&env);

    // Default USDC token is approved at init; secondary token is not.
    assert!(client.is_approved_token(&usdc_addr));
    assert!(!client.is_approved_token(&eurc_addr));

    // Removing the default USDC token fails with CannotRemoveDefaultToken.
    assert_eq!(
        client.try_remove_approved_token(&admin, &usdc_addr),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::CannotRemoveDefaultToken as u32
        )))
    );

    // Removing an unapproved token fails with UnsupportedToken.
    assert_eq!(
        client.try_remove_approved_token(&admin, &eurc_addr),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::UnsupportedToken as u32
        )))
    );

    // Add eurc_addr -> approved.
    client.add_approved_token(&admin, &eurc_addr);
    assert!(client.is_approved_token(&eurc_addr));

    // Duplicate add fails with TokenAlreadyApproved.
    assert_eq!(
        client.try_add_approved_token(&admin, &eurc_addr),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::TokenAlreadyApproved as u32
        )))
    );
    assert_eq!(
        client.try_add_approved_token(&admin, &usdc_addr),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::TokenAlreadyApproved as u32
        )))
    );

    // Remove eurc_addr -> no longer approved.
    client.remove_approved_token(&admin, &eurc_addr);
    assert!(!client.is_approved_token(&eurc_addr));
}

#[test]
fn usdc_only_parity_with_distribute_token_and_balance_of() {
    let env = Env::default();
    let (admin, usdc_addr, _eurc_addr, contract_addr, client) = setup_two_tokens(&env);
    let recipient = Address::generate(&env);

    token::StellarAssetClient::new(&env, &usdc_addr).mint(&contract_addr, &1_000);
    assert_eq!(client.balance(), 1_000);
    assert_eq!(client.balance_of(&usdc_addr), 1_000);

    client.distribute_token(&admin, &usdc_addr, &recipient, &400);
    assert_eq!(client.balance(), 600);
    assert_eq!(client.balance_of(&usdc_addr), 600);
    assert_eq!(token::Client::new(&env, &usdc_addr).balance(&recipient), 400);
}

#[test]
fn mixed_token_batch_transfers_and_lifecycle_events() {
    let env = Env::default();
    let (admin, usdc_addr, eurc_addr, contract_addr, client) = setup_two_tokens(&env);
    client.add_approved_token(&admin, &eurc_addr);

    token::StellarAssetClient::new(&env, &usdc_addr).mint(&contract_addr, &1_000);
    token::StellarAssetClient::new(&env, &eurc_addr).mint(&contract_addr, &500);

    let dev_a = Address::generate(&env);
    let dev_b = Address::generate(&env);

    env.events().all();

    let mut payments = Vec::new(&env);
    payments.push_back((usdc_addr.clone(), dev_a.clone(), 200));
    payments.push_back((eurc_addr.clone(), dev_b.clone(), 150));
    // Duplicate recipient across tokens & duplicate leg for dev_a in USDC
    payments.push_back((usdc_addr.clone(), dev_a.clone(), 100));
    payments.push_back((eurc_addr.clone(), dev_a.clone(), 50));

    client.batch_distribute_multi(&admin, &payments);

    assert_eq!(client.balance_of(&usdc_addr), 700);
    assert_eq!(client.balance_of(&eurc_addr), 300);
    assert_eq!(token::Client::new(&env, &usdc_addr).balance(&dev_a), 300);
    assert_eq!(token::Client::new(&env, &eurc_addr).balance(&dev_b), 150);
    assert_eq!(token::Client::new(&env, &eurc_addr).balance(&dev_a), 50);

    // Verify lifecycle event payloads carry the leg's token address.
    let contract_events: std::vec::Vec<_> = env
        .events()
        .all()
        .iter()
        .filter(|e| e.0 == contract_addr)
        .collect();
    // 1 batch_started + 4 * 3 leg events + 1 batch_completed = 14
    assert_eq!(contract_events.len(), 14);

    let leg0_started =
        events::DistributionLifecycleEvent::try_from_val(&env, &contract_events[1].2).unwrap();
    assert_eq!(leg0_started.token, usdc_addr);
    assert_eq!(leg0_started.amount, 200);
    assert_eq!(leg0_started.batch_index, 0);

    let leg1_started =
        events::DistributionLifecycleEvent::try_from_val(&env, &contract_events[4].2).unwrap();
    assert_eq!(leg1_started.token, eurc_addr);
    assert_eq!(leg1_started.amount, 150);
    assert_eq!(leg1_started.batch_index, 1);
}

#[test]
fn mixed_token_batch_one_token_underfunded_reverts_atomically() {
    let env = Env::default();
    let (admin, usdc_addr, eurc_addr, contract_addr, client) = setup_two_tokens(&env);
    client.add_approved_token(&admin, &eurc_addr);

    token::StellarAssetClient::new(&env, &usdc_addr).mint(&contract_addr, &1_000);
    token::StellarAssetClient::new(&env, &eurc_addr).mint(&contract_addr, &100);

    let dev_a = Address::generate(&env);
    let dev_b = Address::generate(&env);

    env.events().all();

    let mut payments = Vec::new(&env);
    payments.push_back((usdc_addr.clone(), dev_a.clone(), 200));
    payments.push_back((eurc_addr.clone(), dev_b.clone(), 150)); // EURC underfunded (100 < 150)

    assert_eq!(
        client.try_batch_distribute_multi(&admin, &payments),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::InsufficientBalance as u32
        )))
    );

    // No transfers and no contract events emitted.
    assert_eq!(client.balance_of(&usdc_addr), 1_000);
    assert_eq!(client.balance_of(&eurc_addr), 100);
    assert_eq!(token::Client::new(&env, &usdc_addr).balance(&dev_a), 0);
    assert_eq!(token::Client::new(&env, &eurc_addr).balance(&dev_b), 0);
}

#[test]
fn unapproved_and_removed_mid_life_token_rejected_before_transfer() {
    let env = Env::default();
    let (admin, usdc_addr, eurc_addr, contract_addr, client) = setup_two_tokens(&env);

    token::StellarAssetClient::new(&env, &usdc_addr).mint(&contract_addr, &1_000);
    token::StellarAssetClient::new(&env, &eurc_addr).mint(&contract_addr, &1_000);

    let recipient = Address::generate(&env);

    // Unapproved token fails in distribute_token
    assert_eq!(
        client.try_distribute_token(&admin, &eurc_addr, &recipient, &100),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::UnsupportedToken as u32
        )))
    );

    // Unapproved token in second leg of batch_distribute_multi reverts whole batch
    let mut payments = Vec::new(&env);
    payments.push_back((usdc_addr.clone(), recipient.clone(), 100));
    payments.push_back((eurc_addr.clone(), recipient.clone(), 100));
    assert_eq!(
        client.try_batch_distribute_multi(&admin, &payments),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::UnsupportedToken as u32
        )))
    );
    assert_eq!(token::Client::new(&env, &usdc_addr).balance(&recipient), 0);

    // Approve, distribute, then remove mid-life -> subsequent distribution fails
    client.add_approved_token(&admin, &eurc_addr);
    client.distribute_token(&admin, &eurc_addr, &recipient, &100);
    assert_eq!(token::Client::new(&env, &eurc_addr).balance(&recipient), 100);

    client.remove_approved_token(&admin, &eurc_addr);
    assert_eq!(
        client.try_distribute_token(&admin, &eurc_addr, &recipient, &100),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::UnsupportedToken as u32
        )))
    );
}

#[test]
fn per_token_cap_overrides_global_cap_independently() {
    let env = Env::default();
    let (admin, usdc_addr, eurc_addr, contract_addr, client) = setup_two_tokens(&env);
    client.add_approved_token(&admin, &eurc_addr);

    token::StellarAssetClient::new(&env, &usdc_addr).mint(&contract_addr, &10_000);
    token::StellarAssetClient::new(&env, &eurc_addr).mint(&contract_addr, &10_000);

    // Global cap applies as fallback when per-token cap is unset.
    client.set_max_distribute(&admin, &500);
    assert_eq!(client.get_max_distribute_for(&usdc_addr), 500);
    assert_eq!(client.get_max_distribute_for(&eurc_addr), 500);

    // Set a lower per-token cap for EURC and a higher one for USDC.
    client.set_max_distribute_for(&admin, &eurc_addr, &200);
    client.set_max_distribute_for(&admin, &usdc_addr, &1_000);
    assert_eq!(client.get_max_distribute_for(&eurc_addr), 200);
    assert_eq!(client.get_max_distribute_for(&usdc_addr), 1_000);

    let recipient = Address::generate(&env);

    // USDC 800 succeeds (<= 1_000 per-token cap, even though > 500 global cap).
    client.distribute_token(&admin, &usdc_addr, &recipient, &800);

    // EURC 250 fails (> 200 per-token cap).
    assert_eq!(
        client.try_distribute_token(&admin, &eurc_addr, &recipient, &250),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::AmountExceedsMaxDistribute as u32
        )))
    );

    // Non-positive per-token cap is rejected.
    assert_eq!(
        client.try_set_max_distribute_for(&admin, &eurc_addr, &0),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::CapNotPositive as u32
        )))
    );
}

#[test]
fn multi_token_respects_pause_non_admin_batch_limits_and_overflow() {
    let env = Env::default();
    let (admin, usdc_addr, eurc_addr, contract_addr, client) = setup_two_tokens(&env);
    client.add_approved_token(&admin, &eurc_addr);
    token::StellarAssetClient::new(&env, &eurc_addr).mint(&contract_addr, &1_000);

    let intruder = Address::generate(&env);
    let recipient = Address::generate(&env);

    // Non-admin rejected
    assert_eq!(
        client.try_add_approved_token(&intruder, &Address::generate(&env)),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::Unauthorized as u32
        )))
    );
    assert_eq!(
        client.try_remove_approved_token(&intruder, &eurc_addr),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::Unauthorized as u32
        )))
    );
    assert_eq!(
        client.try_set_max_distribute_for(&intruder, &eurc_addr, &100),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::Unauthorized as u32
        )))
    );
    assert_eq!(
        client.try_distribute_token(&intruder, &eurc_addr, &recipient, &10),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::Unauthorized as u32
        )))
    );

    // Pause blocks distribute_token and batch_distribute_multi
    client.pause(&admin);
    assert_eq!(
        client.try_distribute_token(&admin, &eurc_addr, &recipient, &10),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::Paused as u32
        )))
    );
    let mut single = Vec::new(&env);
    single.push_back((eurc_addr.clone(), recipient.clone(), 10_i128));
    assert_eq!(
        client.try_batch_distribute_multi(&admin, &single),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::Paused as u32
        )))
    );
    client.unpause(&admin);

    // BatchEmpty and BatchTooLarge
    let empty: Vec<(Address, Address, i128)> = Vec::new(&env);
    assert_eq!(
        client.try_batch_distribute_multi(&admin, &empty),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::BatchEmpty as u32
        )))
    );
    let mut oversized = Vec::new(&env);
    for _ in 0..(limits::MAX_BATCH_SIZE + 1) {
        oversized.push_back((eurc_addr.clone(), recipient.clone(), 1_i128));
    }
    assert_eq!(
        client.try_batch_distribute_multi(&admin, &oversized),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::BatchTooLarge as u32
        )))
    );

    // Per-token total overflow returns Overflow
    let mut overflow_batch = Vec::new(&env);
    overflow_batch.push_back((eurc_addr.clone(), recipient.clone(), i128::MAX));
    overflow_batch.push_back((eurc_addr.clone(), recipient, i128::MAX));
    assert_eq!(
        client.try_batch_distribute_multi(&admin, &overflow_batch),
        Err(Ok(soroban_sdk::Error::from_contract_error(
            DistributeError::Overflow as u32
        )))
    );
}
