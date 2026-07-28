//! # Events Lifecycle — Structured Event Emission Tests
//!
//! `src/events.rs` unit-tests that each `event_*` helper returns the expected
//! topic `Symbol` bytes, but nothing previously proved those symbols are
//! actually *published* -- with the expected topics and payload -- when the
//! corresponding entrypoint runs. This file closes that gap: one integration
//! test per checkpoint-lifecycle entrypoint, asserting the full published
//! event shape via `env.events().all()`.
//!
//! The contract-level `upgraded` event is intentionally not asserted here:
//! `update_current_contract_wasm` swaps the registered test contract to real
//! Wasm before the event is published, so the SDK test harness cannot observe
//! it via `env.events().all()` afterwards (same documented limitation as
//! `contracts/settlement/src/settlement_tests.rs::test_upgrade_and_get_version`).
//! The `upgraded` topic itself is covered by `events::tests::test_event_upgraded_bytes`.

extern crate std;

use callora_checkpoint::{CalloraCheckpoint, CalloraCheckpointClient, CheckpointRecord};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env, IntoVal, Symbol, Vec};

/// Deploy and initialise a fresh checkpoint contract, returning `(env, admin, client)`.
fn setup() -> (Env, Address, CalloraCheckpointClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraCheckpoint, ());
    let client = CalloraCheckpointClient::new(&env, &contract_id);
    client.init(&admin);
    (env, admin, client)
}

/// The most recent event published by `contract_id`, as `(topics, data)`.
fn last_event(env: &Env, contract_id: &Address) -> (Vec<soroban_sdk::Val>, soroban_sdk::Val) {
    let events = env.events().all();
    let (_, topics, data) = events
        .iter()
        .rev()
        .find(|e| &e.0 == contract_id)
        .expect("expected at least one event from this contract");
    (topics, data)
}

#[test]
fn init_emits_init_event() {
    let (env, admin, client) = setup();

    let (topics, data) = last_event(&env, &client.address);
    assert_eq!(topics.len(), 2);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "init"));
    assert_eq!(topic1, admin);

    let payload: () = data.into_val(&env);
    assert_eq!(payload, ());
}

#[test]
fn create_checkpoint_emits_checkpoint_created_event() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let metadata = Symbol::new(&env, "monthly_close");

    let id = client.create_checkpoint(&admin, &subject, &token, &1000i128, &metadata);

    let (topics, data) = last_event(&env, &client.address);
    assert_eq!(topics.len(), 2);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "checkpoint_created"));
    assert_eq!(topic1, subject);

    let record: CheckpointRecord = data.into_val(&env);
    assert_eq!(record.id, id);
    assert_eq!(record.subject, subject);
    assert_eq!(record.token, token);
    assert_eq!(record.balance, 1000);
    assert_eq!(record.metadata, metadata);
}

#[test]
fn batch_create_checkpoints_emits_one_event_per_item() {
    let (env, admin, client) = setup();
    let subject_a = Address::generate(&env);
    let subject_b = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "audit");

    let items = Vec::from_array(
        &env,
        [
            (subject_a.clone(), token.clone(), 500i128, meta.clone()),
            (subject_b.clone(), token.clone(), 750i128, meta.clone()),
        ],
    );
    client.batch_create_checkpoints(&admin, &items);

    let created_events: std::vec::Vec<_> = env
        .events()
        .all()
        .iter()
        .filter(|e| {
            if e.0 != client.address || e.1.is_empty() {
                return false;
            }
            let topic0: Symbol = e.1.get(0).unwrap().into_val(&env);
            topic0 == Symbol::new(&env, "checkpoint_created")
        })
        .collect();
    assert_eq!(created_events.len(), 2);

    let subjects: [&Address; 2] = [&subject_a, &subject_b];
    let balances: [i128; 2] = [500, 750];
    for (i, event) in created_events.iter().enumerate() {
        let topic1: Address = event.1.get(1).unwrap().into_val(&env);
        assert_eq!(&topic1, subjects[i]);

        let record: CheckpointRecord = event.2.into_val(&env);
        assert_eq!(record.subject, *subjects[i]);
        assert_eq!(record.balance, balances[i]);
        assert_eq!(record.metadata, meta);
    }
}

#[test]
fn set_admin_emits_admin_nominated_event() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);

    let (topics, data) = last_event(&env, &client.address);
    assert_eq!(topics.len(), 3);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    let topic2: Address = topics.get(2).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_nominated"));
    assert_eq!(topic1, admin);
    assert_eq!(topic2, new_admin);

    let payload: Address = data.into_val(&env);
    assert_eq!(payload, new_admin);
}

#[test]
fn accept_admin_emits_admin_accepted_event() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    client.accept_admin(&new_admin);

    let (topics, data) = last_event(&env, &client.address);
    assert_eq!(topics.len(), 3);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    let topic2: Address = topics.get(2).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_accepted"));
    assert_eq!(topic1, admin);
    assert_eq!(topic2, new_admin);

    let payload: Address = data.into_val(&env);
    assert_eq!(payload, new_admin);
}

#[test]
fn cancel_admin_transfer_emits_admin_cancelled_event() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    client.cancel_admin_transfer(&admin);

    let (topics, data) = last_event(&env, &client.address);
    assert_eq!(topics.len(), 3);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    let topic2: Address = topics.get(2).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_cancelled"));
    assert_eq!(topic1, admin);
    assert_eq!(topic2, new_admin);

    let payload: () = data.into_val(&env);
    assert_eq!(payload, ());
}
