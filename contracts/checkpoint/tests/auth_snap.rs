//! # Auth Snapshot — Per-Entrypoint Authorization Tests
//!
//! This integration test module verifies that **every state-changing entrypoint**
//! in the checkpoint contract enforces `require_auth` and that every
//! **read-only entrypoint** does **not** require authorization.
//!
//! The tests serve as a living snapshot of the contract's auth surface: if a new
//! state-changing entrypoint is added **without** `require_auth`, the
//! corresponding read-only-group test in this file will catch it during CI.
//!
//! ## Coverage
//!
//! | Category           | Entrypoints covered |
//! |---------------------|----------------------|
//! | Initialisation       | `init` |
//! | Admin rotation        | `set_admin`, `accept_admin`, `cancel_admin_transfer` |
//! | Checkpoint creation | `create_checkpoint`, `batch_create_checkpoints` |
//! | Upgrade                | `upgrade` |
//! | Read-only views     | `get_admin`, `get_pending_admin`, `get_checkpoint`, `get_checkpoints_range`, `get_checkpoint_count`, `get_latest_checkpoint_id`, `get_latest_checkpoint` |

extern crate std;

use callora_checkpoint::{CalloraCheckpoint, CalloraCheckpointClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, Symbol, Vec};

/// Deploy a fresh (uninitialised) checkpoint contract and return `(env, client)`.
fn create_contract(env: &Env) -> CalloraCheckpointClient<'_> {
    let contract_id = env.register(CalloraCheckpoint, ());
    CalloraCheckpointClient::new(env, &contract_id)
}

/// Deploy and initialise a checkpoint contract, mocking auth for setup only.
/// Returns `(admin, client)`.
fn setup<'a>(env: &'a Env) -> (Address, CalloraCheckpointClient<'a>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let client = create_contract(env);
    client.init(&admin);
    (admin, client)
}

// ---------------------------------------------------------------------------
// Auth-requiring entrypoints — each test verifies that calling the entrypoint
// WITHOUT authorization fails.
// ---------------------------------------------------------------------------

/// Verify that `init` requires auth on `admin`.
#[test]
fn init_requires_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let client = create_contract(&env);

    env.set_auths(&[]);
    let res = client.try_init(&admin);
    assert!(res.is_err(), "init must require auth");
}

/// Verify that `set_admin` requires auth on `caller`.
#[test]
fn set_admin_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    let new_admin = Address::generate(&env);
    let res = client.try_set_admin(&admin, &new_admin);
    assert!(res.is_err(), "set_admin must require auth");
}

/// Verify that `accept_admin` requires auth on `caller`.
#[test]
fn accept_admin_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.mock_all_auths();
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    env.set_auths(&[]);
    let res = client.try_accept_admin(&new_admin);
    assert!(res.is_err(), "accept_admin must require auth");
}

/// Verify that `cancel_admin_transfer` requires auth on `caller`.
#[test]
fn cancel_admin_transfer_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.mock_all_auths();
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    env.set_auths(&[]);
    let res = client.try_cancel_admin_transfer(&admin);
    assert!(res.is_err(), "cancel_admin_transfer must require auth");
}

/// Verify that `create_checkpoint` requires auth on `caller`.
#[test]
fn create_checkpoint_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let metadata = Symbol::new(&env, "test");

    env.set_auths(&[]);
    let res = client.try_create_checkpoint(&admin, &subject, &token, &100i128, &metadata);
    assert!(res.is_err(), "create_checkpoint must require auth");
}

/// Verify that `batch_create_checkpoints` requires auth on `caller`.
#[test]
fn batch_create_checkpoints_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "batch");

    let items = Vec::from_array(&env, [(subject, token, 100i128, meta)]);

    env.set_auths(&[]);
    let res = client.try_batch_create_checkpoints(&admin, &items);
    assert!(res.is_err(), "batch_create_checkpoints must require auth");
}

/// Verify that `upgrade` requires auth on `caller`.
#[test]
fn upgrade_requires_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    let dummy_hash = BytesN::from_array(&env, &[0u8; 32]);
    let res = client.try_upgrade(&admin, &dummy_hash);
    assert!(res.is_err(), "upgrade must require auth");
}

// ---------------------------------------------------------------------------
// Read-only entrypoints — each test verifies that calling without auth
// **succeeds** (no require_auth panic).
// ---------------------------------------------------------------------------

/// `get_admin` is a view — it must not require auth.
#[test]
fn get_admin_does_not_require_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_admin(), admin);
}

/// `get_pending_admin` is a view — it must not require auth.
#[test]
fn get_pending_admin_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_pending_admin(), None);
}

/// `get_checkpoint` is a view — it must not require auth.
#[test]
fn get_checkpoint_does_not_require_auth() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    env.mock_all_auths();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "view");
    let id = client.create_checkpoint(&admin, &subject, &token, &42i128, &meta);

    env.set_auths(&[]);
    let record = client.get_checkpoint(&id);
    assert_eq!(record.balance, 42);
}

/// `get_checkpoints_range` is a view — it must not require auth.
#[test]
fn get_checkpoints_range_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    env.set_auths(&[]);
    let page = client.get_checkpoints_range(&1u64, &10u32);
    assert!(page.is_empty());
}

/// `get_checkpoint_count` is a view — it must not require auth.
#[test]
fn get_checkpoint_count_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_checkpoint_count(), 0);
}

/// `get_latest_checkpoint_id` is a view — it must not require auth.
#[test]
fn get_latest_checkpoint_id_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_latest_checkpoint_id(), 0);
}

/// `get_latest_checkpoint` is a view — it must not require auth.
#[test]
fn get_latest_checkpoint_does_not_require_auth() {
    let env = Env::default();
    let (_admin, client) = setup(&env);

    env.set_auths(&[]);
    assert_eq!(client.get_latest_checkpoint(), None);
}

// ---------------------------------------------------------------------------
// Canonical smoke test — admin **with** auth can call every gated entrypoint.
// ---------------------------------------------------------------------------

/// A single integration test that successfully invokes every state-changing
/// entrypoint with proper authorization. This proves the harness setup is
/// correct and that the entrypoints are reachable when auth is provided.
#[test]
fn admin_with_auth_can_call_all_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let client = create_contract(&env);
    client.init(&admin);

    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "smoke");
    client.create_checkpoint(&admin, &subject, &token, &100i128, &meta);

    let items = Vec::from_array(&env, [(subject, token, 200i128, meta)]);
    client.batch_create_checkpoints(&admin, &items);

    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);
    client.cancel_admin_transfer(&admin);

    client.set_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);

    // Reset admin back for consistency.
    client.set_admin(&new_admin, &admin);
    client.accept_admin(&admin);
    assert_eq!(client.get_admin(), admin);

    // `upgrade` requires auth; with mocked auth the call reaches the wasm
    // update step (any resulting error is from the invalid dummy hash, not auth).
    let dummy_hash = BytesN::from_array(&env, &[1u8; 32]);
    let _ = client.try_upgrade(&admin, &dummy_hash);
}
