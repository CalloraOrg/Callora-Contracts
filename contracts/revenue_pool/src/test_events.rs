//! Focused event tests for revenue_pool structured events
//!
//! Tests ensuring all 21 lifecycle events are properly structured.

extern crate std;

use crate::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env, IntoVal, Symbol, TryFromVal};

#[test]
fn all_event_constructors_return_correct_symbols() {
    let env = Env::default();

    // Test all 21 event constructor functions return correct string symbols
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
        events::event_pause_guardian_set(&env),
        Symbol::new(&env, "pause_guardian_set")
    );
    assert_eq!(
        events::event_pause_guardian_cleared(&env),
        Symbol::new(&env, "pause_guardian_cleared")
    );
    assert_eq!(
        events::event_pause_set(&env),
        Symbol::new(&env, "pause_set")
    );
    assert_eq!(
        events::event_receive_payment(&env),
        Symbol::new(&env, "receive_payment")
    );
    assert_eq!(
        events::event_yield_deposited(&env),
        Symbol::new(&env, "yield_deposited")
    );
    assert_eq!(
        events::event_treasury_transfer_started(&env),
        Symbol::new(&env, "treasury_transfer_started")
    );
    assert_eq!(
        events::event_treasury_transfer_completed(&env),
        Symbol::new(&env, "treasury_transfer_completed")
    );
    assert_eq!(
        events::event_treasury_cancelled(&env),
        Symbol::new(&env, "treasury_cancelled")
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
        events::event_batch_distribute(&env),
        Symbol::new(&env, "batch_distribute")
    );
    assert_eq!(events::event_upgraded(&env), Symbol::new(&env, "upgraded"));
    assert_eq!(
        events::event_admin_broadcast(&env),
        Symbol::new(&env, "admin_broadcast")
    );
    assert_eq!(
        events::event_emergency_drain_proposed(&env),
        Symbol::new(&env, "emergency_drain_proposed")
    );
    assert_eq!(
        events::event_emergency_drain_executed(&env),
        Symbol::new(&env, "emergency_drain_executed")
    );
    assert_eq!(
        events::event_emergency_drain_cancelled(&env),
        Symbol::new(&env, "emergency_drain_cancelled")
    );
}

#[test]
fn init_event_structure_validation() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let pool_addr = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(&env, &pool_addr);

    client.init(&admin, &usdc_addr);

    let events = env.events().all();
    let event = events.last().unwrap();

    // Verify event has correct topic structure [event_name, caller]
    let topics = &event.1;
    assert_eq!(topics.len(), 2);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "init"));
    assert_eq!(topic1, admin);

    // Verify event data contains usdc address
    let data: Address = event.2.into_val(&env);
    assert_eq!(data, usdc_addr);
}

#[test]
fn pause_guardian_events_validation() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let guardian = Address::generate(&env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let pool_addr = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(&env, &pool_addr);

    client.init(&admin, &usdc_addr);
    client.set_pause_guardian(&admin, &guardian);

    // Find pause_guardian_set event
    let events = env.events().all();
    let set_event = events
        .iter()
        .rev()
        .find(|e| {
            if let Ok(symbol) = Symbol::try_from_val(&env, &e.1.get(0).unwrap()) {
                symbol == Symbol::new(&env, "pause_guardian_set")
            } else {
                false
            }
        })
        .unwrap();

    let topics = &set_event.1;
    let caller: Address = topics.get(1).unwrap().into_val(&env);
    assert_eq!(caller, admin);
    let guardian_data: Address = set_event.2.into_val(&env);
    assert_eq!(guardian_data, guardian);
}
