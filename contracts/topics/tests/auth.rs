//! Auth-context tests for Soroban event topics.
//!
//! Verifies that event topics correctly capture signer identity under both
//! `mock_all_auths()` (mock-auth harness) and real `require_auth()` checks.
//!
//! # Properties under test
//!
//! 1. **Signer capture under mock-auth** — with `mock_all_auths()`, every
//!    caller address passed to an entrypoint appears as topic[1] in the
//!    emitted event.
//! 2. **Signer capture under real auth** — with real `require_auth()`, the
//!    authorized signer appears as topic[1]; a non-signing caller is rejected.
//! 3. **Topic shape stability** — topic[0] is the event name Symbol,
//!    topic[1] is the subject address.
//! 4. **Auth failure → no event** — when `require_auth` rejects a caller,
//!    no event is emitted.
//! 5. **Cross-contract topic consistency** — shared event names (e.g. "init")
//!    produce the same Symbol bytes across contracts.

extern crate std;

use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{Address, Env, IntoVal, Symbol, TryFromVal, Val, Vec};

// ---------------------------------------------------------------------------
// Helpers — decode event topics into Rust types for assertion.
// ---------------------------------------------------------------------------

/// Extract topic[0] as a Symbol.
fn topic_symbol(env: &Env, topics: &Vec<Val>) -> Symbol {
    Symbol::try_from_val(env, &topics.get(0).unwrap()).unwrap()
}

/// Extract topic[N] as an Address.
fn topic_address(env: &Env, topics: &Vec<Val>, n: u32) -> Address {
    Address::try_from_val(env, &topics.get(n).unwrap()).unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Test 1: verify signer identity under mock-auth harness.
///
/// Every emitted event's topic[1] should be the caller's address that was
/// passed to the entrypoint.
#[test]
fn mock_auth_captures_signer_in_event_topics() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let signer = Address::generate(&env);

    let contract_id = env.register(callora_hot::CalloraHot, ());
    let client = callora_hot::CalloraHotClient::new(&env, &contract_id);

    client.init(&admin, &signer, &Some(60u64));

    // Capture all events in one call (events().all() drains the log).
    let all_events = env.events().all();
    assert_eq!(all_events.len(), 1, "init should emit exactly 1 event");

    let (emitter, topics, _data) = all_events.get(0).unwrap();
    assert_eq!(emitter, contract_id, "event must be emitted by hot contract");

    assert_eq!(
        topic_symbol(&env, &topics),
        Symbol::new(&env, "init"),
        "topic[0] must be the event name Symbol"
    );
    assert_eq!(
        topic_address(&env, &topics, 1),
        admin,
        "topic[1] must be the signer (admin) address under mock-auth"
    );

    // set_cooldown emits a second event.
    client.set_cooldown(&admin, &120u64);

    let all_events = env.events().all();
    assert_eq!(all_events.len(), 1, "set_cooldown should emit exactly 1 event");

    let (_emitter, topics, _data) = all_events.get(0).unwrap();
    assert_eq!(
        topic_symbol(&env, &topics),
        Symbol::new(&env, "cooldown_set"),
        "topic[0] must be the event name Symbol"
    );
    assert_eq!(
        topic_address(&env, &topics, 1),
        admin,
        "topic[1] must be the signer address under mock-auth"
    );
}

/// Test 2: verify signer identity under real `require_auth()`.
#[test]
fn real_auth_captures_signer_in_event_topics() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let signer = Address::generate(&env);
    let outsider = Address::generate(&env);

    let contract_id = env.register(callora_hot::CalloraHot, ());
    let client = callora_hot::CalloraHotClient::new(&env, &contract_id);

    let args: Vec<Val> = (&admin, &signer, &Some(60u64)).into_val(&env);

    client
        .mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &admin,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "init",
                args,
                sub_invokes: &[],
            },
        }])
        .init(&admin, &signer, &Some(60u64));

    let all_events = env.events().all();
    assert!(all_events.len() >= 1, "init must emit at least 1 event under real auth");

    let init_event = all_events
        .iter()
        .find(|(emitter, topics, _)| {
            *emitter == contract_id
                && topic_symbol(&env, topics) == Symbol::new(&env, "init")
        })
        .expect("init event must be emitted under real auth");

    assert_eq!(
        topic_symbol(&env, &init_event.1),
        Symbol::new(&env, "init"),
        "topic[0] must be the event name Symbol under real auth"
    );
    assert_eq!(
        topic_address(&env, &init_event.1, 1),
        admin,
        "topic[1] must be the authorized admin address under real auth"
    );

    // Outsider trying to pause must be rejected.
    let pause_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.pause(&outsider);
    }));
    assert!(
        pause_result.is_err(),
        "non-admin caller must be rejected under real auth"
    );
}

/// Test 3: verify event topic structure (2-topic events).
#[test]
fn event_topic_structure_matches_expected_shape() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let signer = Address::generate(&env);

    let contract_id = env.register(callora_hot::CalloraHot, ());
    let client = callora_hot::CalloraHotClient::new(&env, &contract_id);

    client.init(&admin, &signer, &Some(60u64));
    // Drain init event.
    let _ = env.events().all();

    // Pause: 2-topic event (Symbol("paused"), caller) → ()
    client.pause(&admin);

    let all_events = env.events().all();
    assert_eq!(all_events.len(), 1, "pause should emit exactly 1 event");

    let (_emitter, topics, _data) = all_events.get(0).unwrap();
    assert_eq!(
        topics.len(),
        2,
        "pause must emit a 2-topic event (action, subject)"
    );
    assert_eq!(
        topic_symbol(&env, &topics),
        Symbol::new(&env, "paused"),
        "topic[0] must be 'paused'"
    );
    assert_eq!(
        topic_address(&env, &topics, 1),
        admin,
        "topic[1] must be the caller address"
    );

    // Rotate signer: emits event too.
    let new_signer = Address::generate(&env);
    client.rotate_signer(&admin, &new_signer);

    let all_events = env.events().all();
    assert_eq!(all_events.len(), 1, "rotate should emit exactly 1 event");

    let (_emitter, topics, _data) = all_events.get(0).unwrap();
    assert_eq!(
        topics.len(),
        2,
        "rotate_signer must emit a 2-topic event (action, subject)"
    );
    assert_eq!(
        topic_address(&env, &topics, 1),
        admin,
        "topic[1] must be the caller (admin) for rotate"
    );
}

/// Test 4: auth failure path emits no event.
///
/// When the admin check rejects a caller, the entrypoint must return an error
/// before any event is published.
#[test]
fn auth_failure_emits_no_event() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let signer = Address::generate(&env);
    let intruder = Address::generate(&env);

    let contract_id = env.register(callora_hot::CalloraHot, ());
    let client = callora_hot::CalloraHotClient::new(&env, &contract_id);

    client.init(&admin, &signer, &Some(60u64));

    // Drain init events so we can compare later.
    let _ = env.events().all();

    // intruder tries to pause — expected to fail as Unauthorized.
    let result = client.try_pause(&intruder);
    assert_eq!(
        result,
        Err(Ok(callora_hot::errors::HotError::Unauthorized)),
        "intruder must be rejected with Unauthorized"
    );

    // No events should have been emitted by the failed pause.
    let events_after: Vec<_> = env.events().all();
    assert_eq!(
        events_after.len(),
        0,
        "failed auth must not emit any events"
    );
}

/// Test 5: topic consistency across multiple contracts.
///
/// Verifies that the "init" event emitted by two different instances of the
/// same contract type has the same topic structure. Since `events().all()`
/// returns only the most recent invocation events, we capture each contract's
/// init event separately.
#[test]
fn cross_contract_topic_bytes_are_consistent() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let signer = Address::generate(&env);

    // First hot contract instance.
    let hot_id_a = env.register(callora_hot::CalloraHot, ());
    let hot_client_a = callora_hot::CalloraHotClient::new(&env, &hot_id_a);
    hot_client_a.init(&admin, &signer, &Some(60u64));

    // Capture hot_a init event before deploying the second contract.
    let hot_a_events = env.events().all();
    assert_eq!(hot_a_events.len(), 1, "hot_a init must emit 1 event");
    let (emit_a, topics_a, _data_a) = hot_a_events.get(0).unwrap();
    assert_eq!(emit_a, hot_id_a, "hot_a event must be from hot_a contract");
    assert_eq!(
        topic_symbol(&env, &topics_a),
        Symbol::new(&env, "init"),
        "hot_a topic[0] must be 'init' Symbol"
    );
    assert_eq!(
        topic_address(&env, &topics_a, 1),
        admin,
        "hot_a topic[1] must be admin"
    );

    // Second hot contract instance (different contract ID).
    let hot_id_b = env.register(callora_hot::CalloraHot, ());
    let hot_client_b = callora_hot::CalloraHotClient::new(&env, &hot_id_b);
    hot_client_b.init(&admin, &signer, &Some(120u64));

    // Capture hot_b init event.
    let hot_b_events = env.events().all();
    assert_eq!(hot_b_events.len(), 1, "hot_b init must emit 1 event");
    let (emit_b, topics_b, _data_b) = hot_b_events.get(0).unwrap();
    assert_eq!(emit_b, hot_id_b, "hot_b event must be from hot_b contract");
    assert_eq!(
        topic_symbol(&env, &topics_b),
        Symbol::new(&env, "init"),
        "hot_b topic[0] must be 'init' Symbol"
    );
    assert_eq!(
        topic_address(&env, &topics_b, 1),
        admin,
        "hot_b topic[1] must be admin"
    );

    // Both use the same topic[0] Symbol name and topic[1] admin address,
    // confirming cross-contract topic consistency.
    assert_eq!(
        topic_symbol(&env, &topics_a),
        topic_symbol(&env, &topics_b),
        "topic[0] must be the same Symbol across contract instances"
    );
}

/// Test 6: mock-auth vs real-auth produce identical topic values.
///
/// Removed due to cross-Env Address compatibility limitations.
/// This test verified the concept, but comparing Val/Address across separate
/// Env instances is not reliably supported by the test SDK.
#[test]
fn mock_and_real_auth_topic_structure_is_consistent() {
    // ---- Mock-auth: init under mock_all_auths ----
    let env_mock = Env::default();
    env_mock.mock_all_auths();
    let admin_mock = Address::generate(&env_mock);
    let signer_mock = Address::generate(&env_mock);
    let mock_id = env_mock.register(callora_hot::CalloraHot, ());
    let mock_client = callora_hot::CalloraHotClient::new(&env_mock, &mock_id);
    mock_client.init(&admin_mock, &signer_mock, &Some(60u64));

    let mock_events = env_mock.events().all();
    let (_mock_emitter, mock_topics, _mock_data) = mock_events.get(0).unwrap();

    // Verify mock-auth topic structure: 2 topics, name is "init"
    assert_eq!(mock_topics.len(), 2, "mock-auth init must have 2 topics");
    assert_eq!(
        topic_symbol(&env_mock, &mock_topics),
        Symbol::new(&env_mock, "init"),
        "mock-auth topic[0] must be 'init'"
    );
    assert_eq!(
        topic_address(&env_mock, &mock_topics, 1),
        admin_mock,
        "mock-auth topic[1] must be the admin"
    );

    // ---- Real-auth: init via mock_auths ----
    let env_real = Env::default();
    let admin_real = Address::generate(&env_real);
    let signer_real = Address::generate(&env_real);
    let real_id = env_real.register(callora_hot::CalloraHot, ());
    let real_client = callora_hot::CalloraHotClient::new(&env_real, &real_id);

    let real_args: Vec<Val> = (&admin_real, &signer_real, &Some(60u64)).into_val(&env_real);

    real_client
        .mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &admin_real,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &real_id,
                fn_name: "init",
                args: real_args,
                sub_invokes: &[],
            },
        }])
        .init(&admin_real, &signer_real, &Some(60u64));

    let real_events = env_real.events().all();
    let (_real_emitter, real_topics, _real_data) = real_events.get(0).unwrap();

    // Verify real-auth topic structure: 2 topics, name is "init"
    assert_eq!(real_topics.len(), 2, "real-auth init must have 2 topics");
    assert_eq!(
        topic_symbol(&env_real, &real_topics),
        Symbol::new(&env_real, "init"),
        "real-auth topic[0] must be 'init'"
    );
    assert_eq!(
        topic_address(&env_real, &real_topics, 1),
        admin_real,
        "real-auth topic[1] must be the admin"
    );

    // Both modes produce the same Symbol for topic[0] ("init") and the same
    // number of topics. The decoded Symbol values are structurally identical.
    assert_eq!(
        topic_symbol(&env_mock, &mock_topics),
        topic_symbol(&env_real, &real_topics),
        "topic[0] must be the same Symbol under both auth modes"
    );
}
