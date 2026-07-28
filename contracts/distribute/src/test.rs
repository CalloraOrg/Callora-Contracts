extern crate std;

use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env, IntoVal, Symbol};

use super::*;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_distribute(env: &Env) -> (Address, CalloraDistributeClient<'_>) {
    let address = env.register(CalloraDistribute, ());
    let client = CalloraDistributeClient::new(env, &address);
    (address, client)
}

fn setup(env: &Env) -> (Address, CalloraDistributeClient<'_>) {
    let admin = Address::generate(env);
    let (_, client) = create_distribute(env);
    env.mock_all_auths();
    client.init(&admin, &10);
    (admin, client)
}

// ===========================================================================
// init
// ===========================================================================

#[test]
fn init_sets_admin_and_cap() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let (_, client) = create_distribute(&env);

    env.mock_all_auths();
    client.init(&admin, &50);

    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_global_cap(), 50);
}

#[test]
fn init_emits_event() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let (addr, client) = create_distribute(&env);

    env.mock_all_auths();
    client.init(&admin, &25);

    let events = env.events().all();
    let last = events.last().expect("expected at least one event");
    assert_eq!(last.0, addr);
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "init"));
    let data: u32 = last.2.into_val(&env);
    assert_eq!(data, 25);
}

#[test]
fn init_rejects_double_init() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let (_, client) = create_distribute(&env);

    env.mock_all_auths();
    client.init(&admin, &10);

    let err = client.try_init(&admin, &10).unwrap_err();
    assert_eq!(err, Ok(DistributeError::AlreadyInitialized));
}

#[test]
fn init_rejects_zero_cap() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let (_, client) = create_distribute(&env);

    env.mock_all_auths();
    let err = client.try_init(&admin, &0).unwrap_err();
    assert_eq!(err, Ok(DistributeError::CapNotPositive));
}

// ===========================================================================
// get_admin / get_global_cap / get_version / is_paused
// ===========================================================================

#[test]
fn get_admin_panics_before_init() {
    let env = Env::default();
    let (_, client) = create_distribute(&env);
    let err = client.try_get_admin().unwrap_err();
    assert_eq!(err, Ok(DistributeError::NotInitialized));
}

#[test]
fn is_paused_defaults_to_false() {
    let env = Env::default();
    let (_, client) = create_distribute(&env);
    assert!(!client.is_paused());
}

#[test]
fn get_version_none_before_upgrade() {
    let env = Env::default();
    let (_, client) = create_distribute(&env);
    assert_eq!(client.get_version(), None);
}

// ===========================================================================
// open
// ===========================================================================

#[test]
fn open_increments_count() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    let new_count = client.open(&admin, &account, &cat);
    assert_eq!(new_count, 1);
    assert_eq!(client.get_account_count(&account), 1);
    assert_eq!(client.get_account_category_count(&account, &cat), 1);
}

#[test]
fn open_emits_event() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "position");

    client.open(&admin, &account, &cat);

    let events = env.events().all();
    let last = events.last().expect("expected at least one event");
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "open"));
}

#[test]
fn open_rejects_when_paused() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    client.pause(&admin);
    let err = client.try_open(&admin, &account, &cat).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Paused));
}

#[test]
fn open_rejects_unauthorized_caller() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let rando = Address::generate(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    let err = client.try_open(&rando, &account, &cat).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Unauthorized));
}

#[test]
fn open_rejects_at_cap() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    for _ in 0..10 {
        client.open(&admin, &account, &cat);
    }
    let err = client.try_open(&admin, &account, &cat).unwrap_err();
    assert_eq!(err, Ok(DistributeError::AccountLimitExceeded));
}

#[test]
fn open_allows_after_close() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "subscription");

    for _ in 0..10 {
        client.open(&admin, &account, &cat);
    }
    client.close(&admin, &account, &cat);
    let new_count = client.open(&admin, &account, &cat);
    assert_eq!(new_count, 10);
}

// ===========================================================================
// close
// ===========================================================================

#[test]
fn close_decrements_count() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    client.open(&admin, &account, &cat);
    let new_count = client.close(&admin, &account, &cat);
    assert_eq!(new_count, 0);
    assert_eq!(client.get_account_count(&account), 0);
    assert_eq!(client.get_account_category_count(&account, &cat), 0);
}

#[test]
fn close_rejects_empty_state() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    let err = client.try_close(&admin, &account, &cat).unwrap_err();
    assert_eq!(err, Ok(DistributeError::AccountStateEmpty));
}

#[test]
fn close_rejects_when_paused() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    client.open(&admin, &account, &cat);
    client.pause(&admin);
    let err = client.try_close(&admin, &account, &cat).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Paused));
}

#[test]
fn close_rejects_unauthorized() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let rando = Address::generate(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    client.open(&admin, &account, &cat);
    let err = client.try_close(&rando, &account, &cat).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Unauthorized));
}

// ===========================================================================
// batch_open
// ===========================================================================

#[test]
fn batch_open_increments_counts() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    let items = Vec::from_array(
        &env,
        [
            BatchItem {
                account: a.clone(),
                category: cat.clone(),
            },
            BatchItem {
                account: b.clone(),
                category: cat.clone(),
            },
        ],
    );
    client.batch_open(&admin, &items);
    assert_eq!(client.get_account_count(&a), 1);
    assert_eq!(client.get_account_count(&b), 1);
}

#[test]
fn batch_open_rejects_empty() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let items = Vec::new(&env);
    let err = client.try_batch_open(&admin, &items).unwrap_err();
    assert_eq!(err, Ok(DistributeError::BatchEmpty));
}

#[test]
fn batch_open_rejects_over_limit() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    // Build a batch of 51 items using push_back on a Soroban Vec
    let mut items: Vec<BatchItem> = Vec::new(&env);
    for _ in 0..51 {
        items.push_back(BatchItem {
            account: account.clone(),
            category: cat.clone(),
        });
    }
    let err = client.try_batch_open(&admin, &items).unwrap_err();
    assert_eq!(err, Ok(DistributeError::BatchTooLarge));
}

#[test]
fn batch_open_rejects_when_paused() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    client.pause(&admin);
    let items = Vec::from_array(
        &env,
        [BatchItem {
            account,
            category: cat,
        }],
    );
    let err = client.try_batch_open(&admin, &items).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Paused));
}

#[test]
fn batch_open_rejects_unauthorized() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let rando = Address::generate(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    let items = Vec::from_array(
        &env,
        [BatchItem {
            account,
            category: cat,
        }],
    );
    let err = client.try_batch_open(&rando, &items).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Unauthorized));
}

#[test]
fn batch_open_rejects_when_item_exceeds_cap() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    for _ in 0..10 {
        client.open(&admin, &account, &cat);
    }
    let items = Vec::from_array(
        &env,
        [BatchItem {
            account,
            category: cat,
        }],
    );
    let err = client.try_batch_open(&admin, &items).unwrap_err();
    assert_eq!(err, Ok(DistributeError::AccountLimitExceeded));
}

// ===========================================================================
// batch_close
// ===========================================================================

#[test]
fn batch_close_decrements_counts() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let cat = Symbol::new(&env, "position");

    client.open(&admin, &a, &cat);
    client.open(&admin, &b, &cat);

    let items = Vec::from_array(
        &env,
        [
            BatchItem {
                account: a.clone(),
                category: cat.clone(),
            },
            BatchItem {
                account: b.clone(),
                category: cat.clone(),
            },
        ],
    );
    client.batch_close(&admin, &items);
    assert_eq!(client.get_account_count(&a), 0);
    assert_eq!(client.get_account_count(&b), 0);
}

#[test]
fn batch_close_rejects_empty() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let items = Vec::new(&env);
    let err = client.try_batch_close(&admin, &items).unwrap_err();
    assert_eq!(err, Ok(DistributeError::BatchEmpty));
}

#[test]
fn batch_close_rejects_when_paused() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    client.open(&admin, &account, &cat);
    client.pause(&admin);

    let items = Vec::from_array(
        &env,
        [BatchItem {
            account,
            category: cat,
        }],
    );
    let err = client.try_batch_close(&admin, &items).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Paused));
}

#[test]
fn batch_close_rejects_unauthorized() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let rando = Address::generate(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    client.open(&admin, &account, &cat);
    let items = Vec::from_array(
        &env,
        [BatchItem {
            account,
            category: cat,
        }],
    );
    let err = client.try_batch_close(&rando, &items).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Unauthorized));
}

#[test]
fn batch_close_rejects_empty_account_state() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    let items = Vec::from_array(
        &env,
        [BatchItem {
            account,
            category: cat,
        }],
    );
    let err = client.try_batch_close(&admin, &items).unwrap_err();
    assert_eq!(err, Ok(DistributeError::AccountStateEmpty));
}

// ===========================================================================
// set_global_cap
// ===========================================================================

#[test]
fn set_global_cap_updates() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    client.set_global_cap(&admin, &20);
    assert_eq!(client.get_global_cap(), 20);
}

#[test]
fn set_global_cap_emits_event() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    client.set_global_cap(&admin, &20);

    let events = env.events().all();
    let last = events.last().expect("expected event");
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "set_global_cap"));
}

#[test]
fn set_global_cap_rejects_zero() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let err = client.try_set_global_cap(&admin, &0).unwrap_err();
    assert_eq!(err, Ok(DistributeError::CapNotPositive));
}

#[test]
fn set_global_cap_rejects_unauthorized() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let rando = Address::generate(&env);

    let err = client.try_set_global_cap(&rando, &20).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Unauthorized));
}

#[test]
fn set_global_cap_enforces_new_cap() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    client.set_global_cap(&admin, &3);
    for _ in 0..3 {
        client.open(&admin, &account, &cat);
    }
    let err = client.try_open(&admin, &account, &cat).unwrap_err();
    assert_eq!(err, Ok(DistributeError::AccountLimitExceeded));
}

// ===========================================================================
// pause / unpause
// ===========================================================================

#[test]
fn pause_sets_flag() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    client.pause(&admin);
    assert!(client.is_paused());
}

#[test]
fn pause_emits_event() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    client.pause(&admin);

    let events = env.events().all();
    let last = events.last().expect("expected event");
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "paused"));
}

#[test]
fn pause_rejects_already_paused() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    client.pause(&admin);
    let err = client.try_pause(&admin).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Paused));
}

#[test]
fn pause_rejects_unauthorized() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let rando = Address::generate(&env);

    let err = client.try_pause(&rando).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Unauthorized));
}

#[test]
fn unpause_clears_flag() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    client.pause(&admin);
    client.unpause(&admin);
    assert!(!client.is_paused());
}

#[test]
fn unpause_emits_event() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    client.pause(&admin);
    client.unpause(&admin);

    let events = env.events().all();
    let last = events.last().expect("expected event");
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "unpaused"));
}

#[test]
fn unpause_rejects_not_paused() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let err = client.try_unpause(&admin).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Paused));
}

#[test]
fn unpause_rejects_unauthorized() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let rando = Address::generate(&env);

    client.pause(&admin);
    let err = client.try_unpause(&rando).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Unauthorized));
}

// ===========================================================================
// admin transfer
// ===========================================================================

#[test]
fn set_admin_stores_pending() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin));
}

#[test]
fn set_admin_emits_event() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);

    let events = env.events().all();
    let last = events.last().expect("expected event");
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_nominated"));
}

#[test]
fn set_admin_rejects_same_as_current() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    let err = client.try_set_admin(&admin, &admin).unwrap_err();
    assert_eq!(err, Ok(DistributeError::NewAdminSameAsCurrent));
}

#[test]
fn set_admin_rejects_unauthorized() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let rando = Address::generate(&env);

    let err = client.try_set_admin(&rando, &rando).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Unauthorized));
}

#[test]
fn accept_admin_completes_transfer() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);
    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn accept_admin_emits_event() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);
    client.accept_admin();

    let events = env.events().all();
    let last = events.last().expect("expected event");
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_accepted"));
}

#[test]
fn accept_admin_rejects_no_pending() {
    let env = Env::default();
    let (_, client) = setup(&env);
    let err = client.try_accept_admin().unwrap_err();
    assert_eq!(err, Ok(DistributeError::NoAdminTransferPending));
}

#[test]
fn cancel_admin_transfer_removes_pending() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);
    client.cancel_admin_transfer(&admin);
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn cancel_admin_transfer_emits_event() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);
    client.cancel_admin_transfer(&admin);

    let events = env.events().all();
    let last = events.last().expect("expected event");
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_cancelled"));
}

#[test]
fn cancel_admin_transfer_rejects_no_pending() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let err = client.try_cancel_admin_transfer(&admin).unwrap_err();
    assert_eq!(err, Ok(DistributeError::NoAdminTransferPending));
}

#[test]
fn cancel_admin_transfer_rejects_unauthorized() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let new_admin = Address::generate(&env);
    let rando = Address::generate(&env);

    client.set_admin(&admin, &new_admin);
    let err = client.try_cancel_admin_transfer(&rando).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Unauthorized));
}

// ===========================================================================
// get_state / get_account_count / get_account_category_count
// ===========================================================================

#[test]
fn get_state_reflects_count() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "subscription");

    assert_eq!(client.get_state(&account).count, 0);
    client.open(&admin, &account, &cat);
    assert_eq!(client.get_state(&account).count, 1);
    client.open(&admin, &account, &cat);
    assert_eq!(client.get_state(&account).count, 2);
}

#[test]
fn category_counts_are_independent() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let bet = Symbol::new(&env, "bet");
    let pos = Symbol::new(&env, "position");
    let sub = Symbol::new(&env, "subscription");

    client.open(&admin, &account, &bet);
    client.open(&admin, &account, &bet);
    client.open(&admin, &account, &pos);

    assert_eq!(client.get_account_category_count(&account, &bet), 2);
    assert_eq!(client.get_account_category_count(&account, &pos), 1);
    assert_eq!(client.get_account_category_count(&account, &sub), 0);
}

// ===========================================================================
// broadcast
// ===========================================================================

#[test]
fn broadcast_emits_event() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let msg = soroban_sdk::String::from_str(&env, "emergency");

    client.broadcast(&admin, &Severity::Crit, &msg);

    let events = env.events().all();
    let last = events.last().expect("expected event");
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "admin_broadcast"));
}

#[test]
fn broadcast_rejects_unauthorized() {
    let env = Env::default();
    let (_admin, client) = setup(&env);
    let rando = Address::generate(&env);
    let msg = soroban_sdk::String::from_str(&env, "test");

    let err = client
        .try_broadcast(&rando, &Severity::Info, &msg)
        .unwrap_err();
    assert_eq!(err, Ok(DistributeError::Unauthorized));
}

#[test]
fn broadcast_rejects_empty_message() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let msg = soroban_sdk::String::from_str(&env, "");

    let err = client
        .try_broadcast(&admin, &Severity::Info, &msg)
        .unwrap_err();
    assert_eq!(err, Ok(DistributeError::BatchEmpty));
}

// ===========================================================================
// Multiple accounts — independent state
// ===========================================================================

#[test]
fn accounts_are_independent() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    client.open(&admin, &a, &cat);
    client.open(&admin, &a, &cat);
    client.open(&admin, &b, &cat);

    assert_eq!(client.get_account_count(&a), 2);
    assert_eq!(client.get_account_count(&b), 1);

    client.close(&admin, &a, &cat);
    assert_eq!(client.get_account_count(&a), 1);
    assert_eq!(client.get_account_count(&b), 1);
}

#[test]
fn open_close_cycle_multiple_accounts_batch() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let a = Address::generate(&env);
    let b = Address::generate(&env);
    let cat = Symbol::new(&env, "position");

    let opens = Vec::from_array(
        &env,
        [
            BatchItem {
                account: a.clone(),
                category: cat.clone(),
            },
            BatchItem {
                account: b.clone(),
                category: cat.clone(),
            },
            BatchItem {
                account: a.clone(),
                category: cat.clone(),
            },
        ],
    );
    client.batch_open(&admin, &opens);
    assert_eq!(client.get_account_count(&a), 2);
    assert_eq!(client.get_account_count(&b), 1);

    let closes = Vec::from_array(
        &env,
        [
            BatchItem {
                account: a.clone(),
                category: cat.clone(),
            },
            BatchItem {
                account: a.clone(),
                category: cat.clone(),
            },
            BatchItem {
                account: b.clone(),
                category: cat.clone(),
            },
        ],
    );
    client.batch_close(&admin, &closes);
    assert_eq!(client.get_account_count(&a), 0);
    assert_eq!(client.get_account_count(&b), 0);
}

// ===========================================================================
// Pause/Resume — focused lifecycle tests for batch operations
// ===========================================================================

/// Pause blocks batch_open; unpause restores it.
#[test]
fn batch_open_blocked_while_paused_unpause_restores() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    // Initially works
    let items = Vec::from_array(
        &env,
        [BatchItem {
            account: account.clone(),
            category: cat.clone(),
        }],
    );
    client.batch_open(&admin, &items);
    assert_eq!(client.get_account_count(&account), 1);

    // Pause -> blocked
    client.pause(&admin);
    let items2 = Vec::from_array(
        &env,
        [BatchItem {
            account: account.clone(),
            category: cat.clone(),
        }],
    );
    let err = client.try_batch_open(&admin, &items2).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Paused));
    assert_eq!(client.get_account_count(&account), 1, "count unchanged while paused");

    // Unpause -> restored
    client.unpause(&admin);
    let items3 = Vec::from_array(
        &env,
        [BatchItem {
            account: account.clone(),
            category: cat.clone(),
        }],
    );
    client.batch_open(&admin, &items3);
    assert_eq!(client.get_account_count(&account), 2);
}

/// Pause blocks batch_close; unpause restores it.
#[test]
fn batch_close_blocked_while_paused_unpause_restores() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    let items = Vec::from_array(
        &env,
        [BatchItem {
            account: account.clone(),
            category: cat.clone(),
        }],
    );
    client.batch_open(&admin, &items);
    assert_eq!(client.get_account_count(&account), 1);

    // Pause -> blocked
    client.pause(&admin);
    let close_items = Vec::from_array(
        &env,
        [BatchItem {
            account: account.clone(),
            category: cat.clone(),
        }],
    );
    let err = client.try_batch_close(&admin, &close_items).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Paused));
    assert_eq!(client.get_account_count(&account), 1, "count unchanged while paused");

    // Unpause -> restored
    client.unpause(&admin);
    client.batch_close(&admin, &close_items);
    assert_eq!(client.get_account_count(&account), 0);
}

/// Admin-set_global_cap remains available while paused (admin config functions).
#[test]
fn admin_config_available_while_paused() {
    let env = Env::default();
    let (admin, client) = setup(&env);

    client.pause(&admin);
    assert!(client.is_paused());

    // Admin config should work
    client.set_global_cap(&admin, &20);
    assert_eq!(client.get_global_cap(), 20);
}

/// Pause → unpause → pause cycle works correctly.
#[test]
fn multiple_pause_unpause_cycles() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    // Cycle 1
    client.pause(&admin);
    assert!(client.is_paused());
    client.unpause(&admin);
    assert!(!client.is_paused());

    // batch_open works after cycle 1
    let items = Vec::from_array(
        &env,
        [BatchItem {
            account: account.clone(),
            category: cat.clone(),
        }],
    );
    client.batch_open(&admin, &items);
    assert_eq!(client.get_account_count(&account), 1);

    // Cycle 2
    client.pause(&admin);
    assert!(client.is_paused());
    let err = client.try_batch_open(&admin, &items).unwrap_err();
    assert_eq!(err, Ok(DistributeError::Paused));

    client.unpause(&admin);
    assert!(!client.is_paused());
    client.batch_open(&admin, &items);
    assert_eq!(client.get_account_count(&account), 2);
}

/// Read-only views remain accessible while paused.
#[test]
fn reads_work_while_paused() {
    let env = Env::default();
    let (admin, client) = setup(&env);
    let account = Address::generate(&env);
    let cat = Symbol::new(&env, "bet");

    client.open(&admin, &account, &cat);
    client.open(&admin, &account, &cat);

    client.pause(&admin);
    assert!(client.is_paused());

    // All reads must work while paused
    assert_eq!(client.get_account_count(&account), 2);
    assert_eq!(client.get_account_category_count(&account, &cat), 2);
    assert_eq!(client.get_global_cap(), 10);
    assert!(client.is_paused());
    assert_eq!(client.get_admin(), admin);
}
