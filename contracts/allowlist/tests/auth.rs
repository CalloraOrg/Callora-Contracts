//! Auth-context tests for Callora allowlist entrypoints.
//!
//! Verifies that allowlist events correctly capture signer identity under both
//! `mock_all_auths()` (mock-auth harness) and real `require_auth()` checks.
//!
//! # Properties under test
//!
//! 1. **Signer capture under mock-auth** — with `mock_all_auths()`, every
//!    caller address passed to an allowlist entrypoint appears as topic[1] in the
//!    emitted event.
//! 2. **Signer capture under real auth** — with real `require_auth()`, the
//!    authorized signer appears as topic[1]; a non-signing caller is rejected.
//! 3. **Topic shape stability** — topic[0] is the event name Symbol,
//!    topic[1] is the caller/subject address.
//! 4. **Auth failure → no event** — when `require_auth` rejects a caller,
//!    no event is emitted.
//! 5. **Read-only entrypoints** — view functions are callable without auth.

extern crate std;

use callora_vault::{CalloraVault, CalloraVaultClient};
use soroban_sdk::testutils::{Address as _, Events};
use soroban_sdk::{token, Address, Env, IntoVal, Symbol, TryFromVal, Val, Vec as SorobanVec};

// ---------------------------------------------------------------------------
// Helpers — decode event topics into Rust types for assertion.
// ---------------------------------------------------------------------------

/// Extract topic[0] as a Symbol.
fn topic_symbol(env: &Env, topics: &SorobanVec<Val>) -> Symbol {
    Symbol::try_from_val(env, &topics.get(0).unwrap()).unwrap()
}

/// Extract topic[N] as an Address.
fn topic_address(env: &Env, topics: &SorobanVec<Val>, n: u32) -> Address {
    Address::try_from_val(env, &topics.get(n).unwrap()).unwrap()
}

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (
        addr.clone(),
        token::Client::new(env, &addr),
        token::StellarAssetClient::new(env, &addr),
    )
}

fn setup_vault(env: &Env) -> (Address, CalloraVaultClient<'_>) {
    env.mock_all_auths();
    let owner = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_addr);
    let (usdc, _, usdc_admin) = create_usdc(env, &owner);
    usdc_admin.mint(&vault_addr, &1_000);
    client.init(&owner, &usdc, &Some(1_000), &None, &None, &None, &None);
    (owner, client)
}

// ===========================================================================
// Tests — allowlist add_address
// ===========================================================================

/// Test 1: mock-auth captures signer for `add_address`.
#[test]
fn mock_auth_captures_signer_in_add_address() {
    let env = Env::default();
    env.mock_all_auths();

    let (owner, client) = setup_vault(&env);
    let depositor = Address::generate(&env);

    client.add_address(&owner, &depositor);

    let all_events = env.events().all();
    let add_events: std::vec::Vec<_> = all_events
        .iter()
        .filter(|(_, topics, _)| topic_symbol(&env, topics) == Symbol::new(&env, "allowlist_add"))
        .collect();

    assert_eq!(add_events.len(), 1, "must emit exactly 1 allowlist_add event");

    let (_emitter, topics, _data) = &add_events[0];
    assert_eq!(
        topic_symbol(&env, &topics),
        Symbol::new(&env, "allowlist_add"),
        "topic[0] must be the event name Symbol"
    );
    assert_eq!(
        topic_address(&env, &topics, 1),
        owner,
        "topic[1] must be the signer (owner) address under mock-auth"
    );
    assert_eq!(
        topic_address(&env, &topics, 2),
        depositor,
        "topic[2] must be the depositor address"
    );
}

/// Test 2: real auth captures signer for `add_address`.
#[test]
fn real_auth_captures_signer_in_add_address() {
    let env = Env::default();

    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let outsider = Address::generate(&env);

    let contract_id = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &contract_id);

    // Init with mock auth.
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    usdc_admin.mint(&contract_id, &1_000);
    client.init(&owner, &usdc, &Some(1_000), &None, &None, &None, &None);
    // Drain init events.
    let _ = env.events().all();

    // Real auth: owner calls add_address.
    let args: Vec<Val> = (&owner, &depositor).into_val(&env);
    client
        .mock_auths(&[soroban_sdk::testutils::MockAuth {
            address: &owner,
            invoke: &soroban_sdk::testutils::MockAuthInvoke {
                contract: &contract_id,
                fn_name: "add_address",
                args,
                sub_invokes: &[],
            },
        }])
        .add_address(&owner, &depositor);

    let all_events = env.events().all();
    let add_events: std::vec::Vec<_> = all_events
        .iter()
        .filter(|(_, topics, _)| topic_symbol(&env, topics) == Symbol::new(&env, "allowlist_add"))
        .collect();

    assert_eq!(add_events.len(), 1, "must emit allowlist_add under real auth");

    let (_emitter, topics, _data) = &add_events[0];
    assert_eq!(
        topic_symbol(&env, &topics),
        Symbol::new(&env, "allowlist_add"),
        "topic[0] must be the event name Symbol under real auth"
    );
    assert_eq!(
        topic_address(&env, &topics, 1),
        owner,
        "topic[1] must be the authorized owner address under real auth"
    );
    assert_eq!(
        topic_address(&env, &topics, 2),
        depositor,
        "topic[2] must be the depositor address under real auth"
    );

    // Outsider trying to add must be rejected.
    let fail_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.add_address(&outsider, &depositor);
    }));
    assert!(
        fail_result.is_err(),
        "non-owner caller must be rejected under real auth"
    );
}

// ===========================================================================
// Tests — allowlist clear_all
// ===========================================================================

/// Test 3: mock-auth captures signer for `clear_all`.
#[test]
fn mock_auth_captures_signer_in_clear_all() {
    let env = Env::default();
    env.mock_all_auths();

    let (owner, client) = setup_vault(&env);
    let depositor = Address::generate(&env);

    client.add_address(&owner, &depositor);
    // Drain previous events.
    let _ = env.events().all();

    client.clear_all(&owner);

    let all_events = env.events().all();
    let clear_events: std::vec::Vec<_> = all_events
        .iter()
        .filter(|(_, topics, _)| topic_symbol(&env, topics) == Symbol::new(&env, "allowlist_clear"))
        .collect();

    assert_eq!(clear_events.len(), 1, "must emit exactly 1 allowlist_clear event");

    let (_emitter, topics, _data) = &clear_events[0];
    assert_eq!(
        topic_symbol(&env, &topics),
        Symbol::new(&env, "allowlist_clear"),
        "topic[0] must be the event name Symbol"
    );
    assert_eq!(
        topic_address(&env, &topics, 1),
        owner,
        "topic[1] must be the signer (owner) address under mock-auth"
    );
}

/// Test 4: auth failure path emits no event.
#[test]
fn auth_failure_on_allowlist_emits_no_event() {
    let env = Env::default();
    env.mock_all_auths();

    let (owner, client) = setup_vault(&env);
    let intruder = Address::generate(&env);

    // Drain init events.
    let _ = env.events().all();

    // Intruder tries to add_address.
    let result = client.try_add_address(&intruder, &Address::generate(&env));
    assert!(
        result.is_err(),
        "intruder must be rejected"
    );

    // No events should have been emitted by the failed call.
    let events_after = env.events().all();
    assert_eq!(
        events_after.len(),
        0,
        "failed auth must not emit any events"
    );
}

// ===========================================================================
// Tests — read-only allowlist views
// ===========================================================================

/// Test 5: read-only allowlist views work without auth.
#[test]
fn read_only_allowlist_views_work_without_auth() {
    let env = Env::default();
    let (_owner, client) = setup_vault(&env);

    env.set_auths(&[]);

    let list = client.get_allowlist();
    assert!(list.is_empty(), "empty allowlist by default");

    let allowed = client.is_authorized_depositor(&Address::generate(&env));
    assert!(!allowed, "random address is not authorized");
}

// ===========================================================================
// Tests — legacy allowlist entrypoints (set_allowed_depositor)
// ===========================================================================

/// Test 6: mock-auth captures signer for legacy `set_allowed_depositor`.
#[test]
fn mock_auth_captures_signer_in_set_allowed_depositor() {
    let env = Env::default();
    env.mock_all_auths();

    let (owner, client) = setup_vault(&env);
    let depositor = Address::generate(&env);

    client.set_allowed_depositor(&owner, &Some(depositor.clone()));

    assert!(
        client.is_authorized_depositor(depositor.clone()),
        "depositor must be authorized after set_allowed_depositor"
    );
}

/// Test 7: legacy `clear_allowed_depositors` works under mock-auth.
#[test]
fn mock_auth_clear_allowed_depositors_succeeds() {
    let env = Env::default();
    env.mock_all_auths();

    let (owner, client) = setup_vault(&env);
    let depositor = Address::generate(&env);

    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    assert!(client.is_authorized_depositor(depositor.clone()));

    client.clear_allowed_depositors(&owner);
    assert!(
        !client.is_authorized_depositor(depositor.clone()),
        "depositor must NOT be authorized after clear"
    );
}

/// Test 8: non-owner is rejected by legacy allowlist entrypoints.
#[test]
fn non_owner_rejected_by_legacy_allowlist() {
    let env = Env::default();
    env.mock_all_auths();

    let (_owner, client) = setup_vault(&env);
    let intruder = Address::generate(&env);
    let depositor = Address::generate(&env);

    let result = client.try_set_allowed_depositor(&intruder, &Some(depositor));
    assert!(
        result.is_err(),
        "non-owner must be rejected by set_allowed_depositor"
    );
}
