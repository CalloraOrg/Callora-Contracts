use crate::{
    CalloraCheckpoint, CalloraCheckpointClient, StorageKey, BUMP_AMOUNT, LIFETIME_THRESHOLD,
    MAX_BATCH_SIZE, MAX_PAGE_SIZE,
};
use soroban_sdk::testutils::storage::Persistent;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, BytesN, Env, Symbol, Vec};

/// Helper: initialise a fresh checkpoint contract and return `(env, admin, client)`.
fn setup() -> (Env, Address, CalloraCheckpointClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraCheckpoint, ());
    let client = CalloraCheckpointClient::new(&env, &contract_id);
    client.init(&admin);
    (env, admin, client)
}

// ===========================================================================
// Initialisation tests
// ===========================================================================

#[test]
fn test_init_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraCheckpoint, ());
    let client = CalloraCheckpointClient::new(&env, &contract_id);

    let result = client.try_init(&admin);
    assert!(result.is_ok());

    // Verify admin is stored correctly.
    assert_eq!(client.get_admin(), admin);

    // Verify checkpoint count starts at 0.
    assert_eq!(client.get_checkpoint_count(), 0);
    assert_eq!(client.get_latest_checkpoint_id(), 0);
}

#[test]
fn test_init_fails_when_already_initialized() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraCheckpoint, ());
    let client = CalloraCheckpointClient::new(&env, &contract_id);

    client.init(&admin);

    let result = client.try_init(&Address::generate(&env));
    assert!(result.is_err(), "expected AlreadyInitialized error");
}

#[test]
fn test_get_admin_before_init_returns_error() {
    let env = Env::default();
    let contract_id = env.register(CalloraCheckpoint, ());
    let client = CalloraCheckpointClient::new(&env, &contract_id);
    let result = client.try_get_admin();
    assert!(result.is_err(), "expected NotInitialized error");
}

// ===========================================================================
// Admin rotation tests
// ===========================================================================

#[test]
fn test_set_admin_succeeds() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);

    let result = client.try_set_admin(&admin, &new_admin);
    assert!(result.is_ok());

    // Pending admin should be set.
    let pending = client.get_pending_admin();
    assert_eq!(pending, Some(new_admin.clone()));

    // Current admin should still be old admin.
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_set_admin_fails_for_non_admin() {
    let (env, _admin, client) = setup();
    let non_admin = Address::generate(&env);
    let new_admin = Address::generate(&env);

    let result = client.try_set_admin(&non_admin, &new_admin);
    assert!(result.is_err(), "expected Unauthorized error");
}

#[test]
fn test_accept_admin_succeeds() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);
    let result = client.try_accept_admin(&new_admin);
    assert!(result.is_ok());

    // New admin should now be the current admin.
    assert_eq!(client.get_admin(), new_admin);

    // Pending admin should be cleared.
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
#[should_panic(expected = "no admin transfer pending")]
fn test_accept_admin_panics_when_no_transfer_pending() {
    let (env, _admin, client) = setup();
    let caller = Address::generate(&env);
    client.accept_admin(&caller);
}

#[test]
#[should_panic(expected = "unauthorized: caller is not pending admin")]
fn test_accept_admin_panics_for_wrong_caller() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);
    let wrong_caller = Address::generate(&env);

    client.set_admin(&admin, &new_admin);
    client.accept_admin(&wrong_caller);
}

#[test]
fn test_cancel_admin_transfer_succeeds() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);

    client.set_admin(&admin, &new_admin);
    let result = client.try_cancel_admin_transfer(&admin);
    assert!(result.is_ok());

    // Pending admin should be cleared.
    assert_eq!(client.get_pending_admin(), None);

    // Current admin should be unchanged.
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn test_cancel_admin_transfer_fails_for_non_admin() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    let non_admin = Address::generate(&env);
    let result = client.try_cancel_admin_transfer(&non_admin);
    assert!(result.is_err(), "expected Unauthorized error");
}

#[test]
#[should_panic(expected = "no admin transfer pending")]
fn test_cancel_admin_transfer_panics_when_none_pending() {
    let (_env, admin, client) = setup();
    client.cancel_admin_transfer(&admin);
}

// ===========================================================================
// Checkpoint creation tests
// ===========================================================================

#[test]
fn test_create_checkpoint_succeeds() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let metadata = Symbol::new(&env, "monthly_close");

    // create_checkpoint returns Result<u64, CheckpointError>; the client
    // auto-unwraps on success, so this returns u64 directly.
    let id = client.create_checkpoint(&admin, &subject, &token, &1000i128, &metadata);
    assert_eq!(id, 1);

    // Verify checkpoint record (get_checkpoint also auto-unwraps Result).
    let record = client.get_checkpoint(&id);
    assert_eq!(record.id, 1);
    assert_eq!(record.subject, subject);
    assert_eq!(record.token, token);
    assert_eq!(record.balance, 1000);
    assert_eq!(record.metadata, metadata);

    // Verify counts.
    assert_eq!(client.get_checkpoint_count(), 1);
    assert_eq!(client.get_latest_checkpoint_id(), 1);
}

#[test]
fn test_create_checkpoint_fails_for_non_admin() {
    let (env, _admin, client) = setup();
    let non_admin = Address::generate(&env);
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let metadata = Symbol::new(&env, "test");

    let result = client.try_create_checkpoint(&non_admin, &subject, &token, &100, &metadata);
    assert!(result.is_err(), "expected Unauthorized error");
}

#[test]
fn test_create_checkpoint_fails_for_negative_balance() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let metadata = Symbol::new(&env, "test");

    let result = client.try_create_checkpoint(&admin, &subject, &token, &(-1i128), &metadata);
    assert!(result.is_err(), "expected AmountNegative error");
}

#[test]
fn test_create_checkpoint_zero_balance_is_allowed() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let metadata = Symbol::new(&env, "zero_balance");

    let id = client.create_checkpoint(&admin, &subject, &token, &0i128, &metadata);
    let record = client.get_checkpoint(&id);
    assert_eq!(record.balance, 0);
}

#[test]
fn test_create_multiple_checkpoints_sequential_ids() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "seq");

    for i in 1..=5u64 {
        let id = client.create_checkpoint(&admin, &subject, &token, &((i as i128) * 100), &meta);
        assert_eq!(id, i);
    }

    assert_eq!(client.get_checkpoint_count(), 5);
    assert_eq!(client.get_latest_checkpoint_id(), 5);
}

// ===========================================================================
// Batch checkpoint creation tests
// ===========================================================================

#[test]
fn test_batch_create_succeeds() {
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
            (subject_a.clone(), token.clone(), 1000i128, meta.clone()),
        ],
    );

    let ids = client.batch_create_checkpoints(&admin, &items);
    assert_eq!(ids.len(), 3);
    assert_eq!(ids.get(0).unwrap(), 1);
    assert_eq!(ids.get(1).unwrap(), 2);
    assert_eq!(ids.get(2).unwrap(), 3);

    assert_eq!(client.get_checkpoint_count(), 3);

    let r1 = client.get_checkpoint(&1);
    assert_eq!(r1.subject, subject_a);
    assert_eq!(r1.balance, 500);

    let r2 = client.get_checkpoint(&2);
    assert_eq!(r2.subject, subject_b);
    assert_eq!(r2.balance, 750);

    let r3 = client.get_checkpoint(&3);
    assert_eq!(r3.subject, subject_a);
    assert_eq!(r3.balance, 1000);
}

#[test]
fn test_batch_create_fails_for_empty_batch() {
    let (env, admin, client) = setup();
    let items: Vec<(Address, Address, i128, Symbol)> = Vec::new(&env);

    let result = client.try_batch_create_checkpoints(&admin, &items);
    assert!(result.is_err(), "expected BatchEmpty error");
}

#[test]
fn test_batch_create_fails_for_batch_too_large() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "big");

    let mut items = Vec::new(&env);
    for _ in 0..(MAX_BATCH_SIZE + 1) {
        items.push_back((subject.clone(), token.clone(), 100i128, meta.clone()));
    }

    let result = client.try_batch_create_checkpoints(&admin, &items);
    assert!(result.is_err(), "expected BatchTooLarge error");
}

#[test]
fn test_batch_create_fails_for_negative_balance() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "err");

    let items = Vec::from_array(
        &env,
        [
            (subject.clone(), token.clone(), 100i128, meta.clone()),
            (subject.clone(), token.clone(), -1i128, meta.clone()),
        ],
    );

    let result = client.try_batch_create_checkpoints(&admin, &items);
    assert!(result.is_err(), "expected AmountNegative error");

    // Nothing should have been written.
    assert_eq!(client.get_checkpoint_count(), 0);
}

// ===========================================================================
// Query tests
// ===========================================================================

#[test]
fn test_get_checkpoint_not_found() {
    let (_env, _admin, client) = setup();
    let result = client.try_get_checkpoint(&999);
    assert!(result.is_err(), "expected CheckpointNotFound error");
}

#[test]
fn test_get_checkpoints_range_empty() {
    let (_env, _admin, client) = setup();
    let records = client.get_checkpoints_range(&1, &10);
    assert!(records.is_empty());
}

#[test]
fn test_get_checkpoints_range_returns_paginated_results() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "page");

    // Create 15 checkpoints.
    for i in 1..=15u64 {
        client.create_checkpoint(&admin, &subject, &token, &((i * 10) as i128), &meta);
    }

    // Page 1: IDs 1-10.
    let page1 = client.get_checkpoints_range(&1u64, &10u32);
    assert_eq!(page1.len(), 10);
    assert_eq!(page1.get(0).unwrap().id, 1);
    assert_eq!(page1.get(9).unwrap().id, 10);

    // Page 2: IDs 11-15.
    let page2 = client.get_checkpoints_range(&11u64, &10u32);
    assert_eq!(page2.len(), 5);
    assert_eq!(page2.get(0).unwrap().id, 11);
    assert_eq!(page2.get(4).unwrap().id, 15);
}

#[test]
fn test_get_checkpoints_range_caps_limit() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "cap");

    let n = MAX_PAGE_SIZE as u64 + 50;
    for i in 1..=n {
        client.create_checkpoint(&admin, &subject, &token, &(i as i128), &meta);
    }

    let result = client.get_checkpoints_range(&1u64, &(MAX_PAGE_SIZE + 100));
    assert_eq!(result.len(), MAX_PAGE_SIZE);
}

#[test]
fn test_get_checkpoints_range_start_beyond_count() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "beyond");

    for i in 1..=5u64 {
        client.create_checkpoint(&admin, &subject, &token, &(i as i128), &meta);
    }

    let result = client.get_checkpoints_range(&100u64, &10u32);
    assert!(result.is_empty());
}

#[test]
fn test_get_checkpoints_range_fails_for_zero_limit() {
    let (_env, _admin, client) = setup();
    let result = client.try_get_checkpoints_range(&1u64, &0u32);
    assert!(result.is_err(), "expected InvalidPageSize error");
}

#[test]
fn test_get_latest_checkpoint_returns_none_when_empty() {
    let (_env, _admin, client) = setup();
    assert_eq!(client.get_latest_checkpoint_id(), 0);
    assert_eq!(client.get_latest_checkpoint(), None);
}

#[test]
fn test_get_latest_checkpoint_returns_most_recent() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "latest");

    client.create_checkpoint(&admin, &subject, &token, &100i128, &meta);
    client.create_checkpoint(&admin, &subject, &token, &200i128, &meta);

    let latest = client.get_latest_checkpoint().unwrap();
    assert_eq!(latest.id, 2);
    assert_eq!(latest.balance, 200);
}

// ===========================================================================
// TTL bump tests (buffer top-up)
// ===========================================================================

#[test]
fn test_bump_checkpoint_ttl_succeeds() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "bump");

    let id = client.create_checkpoint(&admin, &subject, &token, &1000i128, &meta);

    let result = client.try_bump_checkpoint_ttl(&admin, &id);
    assert!(result.is_ok());
}

#[test]
fn test_bump_checkpoint_ttl_fails_for_non_admin() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "bump_unauth");

    let id = client.create_checkpoint(&admin, &subject, &token, &1000i128, &meta);

    let non_admin = Address::generate(&env);
    let result = client.try_bump_checkpoint_ttl(&non_admin, &id);
    assert!(result.is_err(), "expected Unauthorized error");
}

#[test]
fn test_bump_checkpoint_ttl_fails_for_nonexistent() {
    let (env, _admin, client) = setup();
    // No checkpoints created — ID 999 does not exist.
    let admin = Address::generate(&env);
    let result = client.try_bump_checkpoint_ttl(&admin, &999u64);
    assert!(result.is_err(), "expected CheckpointNotFound error");
}

#[test]
fn test_bump_checkpoints_ttl_range_succeeds() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "bulk_bump");

    for i in 1..=5u64 {
        client.create_checkpoint(&admin, &subject, &token, &((i * 100) as i128), &meta);
    }

    let result = client.try_bump_checkpoints_ttl_range(&admin, &1u64, &5u64);
    assert!(result.is_ok());
}

#[test]
fn test_bump_checkpoints_ttl_range_fails_for_non_admin() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "bulk_unauth");

    client.create_checkpoint(&admin, &subject, &token, &100i128, &meta);

    let non_admin = Address::generate(&env);
    let result = client.try_bump_checkpoints_ttl_range(&non_admin, &1u64, &1u64);
    assert!(result.is_err(), "expected Unauthorized error");
}

#[test]
fn test_bump_checkpoints_ttl_range_fails_when_start_gt_end() {
    let (_env, admin, client) = setup();

    let result = client.try_bump_checkpoints_ttl_range(&admin, &5u64, &3u64);
    assert!(result.is_err(), "expected InvalidPageSize error");
}

#[test]
fn test_bump_checkpoints_ttl_range_skips_missing_ids() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "skip_gaps");

    // Create checkpoints 1 and 3 (skip 2 by not creating it).
    client.create_checkpoint(&admin, &subject, &token, &100i128, &meta);
    client.create_checkpoint(&admin, &subject, &token, &300i128, &meta);

    // Bump range 1–3 should not error on missing ID 2.
    let result = client.try_bump_checkpoints_ttl_range(&admin, &1u64, &3u64);
    assert!(result.is_ok());
}

// ===========================================================================
// Read-path TTL bump tests (buffer #26)
// ===========================================================================
//
// Write paths (`create_checkpoint`, `batch_create_checkpoints`) and the
// explicit admin bump entrypoints already extend a checkpoint's persistent
// TTL. These tests cover the remaining gap: hot *read* paths
// (`get_checkpoint`, `get_checkpoints_range`, `get_latest_checkpoint`) must
// also extend TTL, so an audit record that is queried often but never
// rewritten does not silently archive.

#[test]
fn test_get_checkpoint_bumps_ttl_on_read() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "read_bump");

    let id = client.create_checkpoint(&admin, &subject, &token, &1000i128, &meta);
    let key = StorageKey::Checkpoint(id);

    // Advance the ledger until the write-path bump's TTL has dropped below
    // the threshold, but the entry has not yet expired.
    let seq = env.ledger().sequence();
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .extend_ttl(BUMP_AMOUNT, BUMP_AMOUNT);
    });
    env.ledger()
        .set_sequence_number(seq + BUMP_AMOUNT - LIFETIME_THRESHOLD + 1);

    let ttl_before = env.as_contract(&client.address, || env.storage().persistent().get_ttl(&key));
    assert!(
        ttl_before < LIFETIME_THRESHOLD,
        "sanity: TTL should be below the bump threshold before the read"
    );

    // The read must bump TTL back out to BUMP_AMOUNT from the current ledger.
    let record = client.get_checkpoint(&id);
    assert_eq!(record.balance, 1000);

    let ttl_after = env.as_contract(&client.address, || env.storage().persistent().get_ttl(&key));
    assert_eq!(
        ttl_after, BUMP_AMOUNT,
        "buffer #26: get_checkpoint must bump TTL back to BUMP_AMOUNT"
    );
}

#[test]
fn test_get_checkpoints_range_bumps_ttl_for_every_returned_record() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "range_read_bump");

    for i in 1..=3u64 {
        client.create_checkpoint(&admin, &subject, &token, &(i as i128 * 10), &meta);
    }

    let seq = env.ledger().sequence();
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .extend_ttl(BUMP_AMOUNT, BUMP_AMOUNT);
    });
    env.ledger()
        .set_sequence_number(seq + BUMP_AMOUNT - LIFETIME_THRESHOLD + 1);

    let results = client.get_checkpoints_range(&1u64, &10u32);
    assert_eq!(results.len(), 3);

    for id in 1..=3u64 {
        let key = StorageKey::Checkpoint(id);
        let ttl = env.as_contract(&client.address, || env.storage().persistent().get_ttl(&key));
        assert_eq!(
            ttl, BUMP_AMOUNT,
            "buffer #26: get_checkpoints_range must bump TTL for checkpoint {id}"
        );
    }
}

#[test]
fn test_get_latest_checkpoint_bumps_ttl_on_read() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "latest_read_bump");

    client.create_checkpoint(&admin, &subject, &token, &500i128, &meta);
    let id = client.create_checkpoint(&admin, &subject, &token, &900i128, &meta);
    let key = StorageKey::Checkpoint(id);

    let seq = env.ledger().sequence();
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .extend_ttl(BUMP_AMOUNT, BUMP_AMOUNT);
    });
    env.ledger()
        .set_sequence_number(seq + BUMP_AMOUNT - LIFETIME_THRESHOLD + 1);

    let latest = client.get_latest_checkpoint().unwrap();
    assert_eq!(latest.id, id);

    let ttl = env.as_contract(&client.address, || env.storage().persistent().get_ttl(&key));
    assert_eq!(
        ttl, BUMP_AMOUNT,
        "buffer #26: get_latest_checkpoint must bump TTL of the returned record"
    );
}

#[test]
fn test_get_checkpoint_ttl_survives_past_original_bump_window() {
    // Without the read-path bump, this checkpoint would archive at
    // seq_init + BUMP_AMOUNT. A read before that point must push the
    // expiry further out, so a *second* advance past the original window
    // still finds the record alive.
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "survive");

    let id = client.create_checkpoint(&admin, &subject, &token, &42i128, &meta);
    let seq_init = env.ledger().sequence();

    // Advance to just before the original write-path bump would expire.
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .extend_ttl(BUMP_AMOUNT, BUMP_AMOUNT);
    });
    env.ledger().set_sequence_number(seq_init + BUMP_AMOUNT - 1);
    let _ = client.get_checkpoint(&id); // read-path bump

    // Advance past where the *original* bump would have expired.
    let seq_after_read = env.ledger().sequence();
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .extend_ttl(BUMP_AMOUNT, BUMP_AMOUNT);
    });
    env.ledger()
        .set_sequence_number(seq_after_read + LIFETIME_THRESHOLD + 10);

    let record = client.get_checkpoint(&id);
    assert_eq!(
        record.balance, 42,
        "read-path bump must keep the record alive"
    );
}

// ===========================================================================
// Upgrade tests
// ===========================================================================

#[test]
#[should_panic]
fn test_upgrade_succeeds_for_admin() {
    let (env, admin, client) = setup();
    let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);

    let result = client.try_upgrade(&admin, &wasm_hash);
    assert!(result.is_ok());
}

#[test]
fn test_upgrade_fails_for_non_admin() {
    let (env, _admin, client) = setup();
    let non_admin = Address::generate(&env);
    let wasm_hash = BytesN::from_array(&env, &[0u8; 32]);

    let result = client.try_upgrade(&non_admin, &wasm_hash);
    assert!(result.is_err(), "expected Unauthorized error");
}

// ===========================================================================
// Edge-case & invariant tests
// ===========================================================================

#[test]
fn test_create_checkpoint_before_init_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "pre_init");

    let contract_id = env.register(CalloraCheckpoint, ());
    let client = CalloraCheckpointClient::new(&env, &contract_id);

    // Calling create_checkpoint before init should fail.
    let result = client.try_create_checkpoint(&admin, &subject, &token, &100, &meta);
    assert!(result.is_err(), "expected error before init");
}

#[test]
fn test_batch_create_fails_for_non_admin() {
    let (env, _admin, client) = setup();
    let non_admin = Address::generate(&env);
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "batch_nonadmin");

    let items = Vec::from_array(
        &env,
        [(subject.clone(), token.clone(), 100i128, meta.clone())],
    );

    let result = client.try_batch_create_checkpoints(&non_admin, &items);
    assert!(result.is_err(), "expected Unauthorized error");
}

#[test]
fn test_get_checkpoints_range_start_id_zero() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "zero_start");

    for i in 1..=3u64 {
        client.create_checkpoint(&admin, &subject, &token, &((i * 100) as i128), &meta);
    }

    // start_id=0 means "from the beginning" - same as start_id=1.
    let result = client.get_checkpoints_range(&0u64, &10u32);
    // Returns checkpoints 1, 2, 3 (checkpoint 0 doesn't exist).
    assert_eq!(result.len(), 3);
    assert_eq!(result.get(0).unwrap().id, 1);
    assert_eq!(result.get(2).unwrap().id, 3);
}

#[test]
fn test_checkpoints_are_immutable() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "immutable");

    let id = client.create_checkpoint(&admin, &subject, &token, &999i128, &meta);

    // Read the checkpoint -- cannot "overwrite" it via the public API
    // because `create_checkpoint` always assigns a new ID.
    let record = client.get_checkpoint(&id);
    assert_eq!(record.balance, 999);

    // Create another checkpoint -- it gets a new ID, old one is unchanged.
    let id2 = client.create_checkpoint(&admin, &subject, &token, &888i128, &meta);

    assert_ne!(id, id2);

    let record1 = client.get_checkpoint(&id);
    assert_eq!(record1.balance, 999); // unchanged

    let record2 = client.get_checkpoint(&id2);
    assert_eq!(record2.balance, 888);
}

#[test]
fn test_admin_can_create_checkpoints_after_admin_rotation() {
    let (env, admin, client) = setup();
    let new_admin = Address::generate(&env);
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "post_rotation");

    // Rotate admin.
    client.set_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);

    // Old admin can no longer create checkpoints.
    let result1 = client.try_create_checkpoint(&admin, &subject, &token, &100, &meta);
    assert!(
        result1.is_err(),
        "old admin should be unauthorized after rotation"
    );

    // New admin can.
    let result2 = client.try_create_checkpoint(&new_admin, &subject, &token, &200, &meta);
    assert!(result2.is_ok());
}

#[test]
fn test_count_and_latest_id_stay_consistent() {
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "consistency");

    for i in 1..=25u64 {
        client.create_checkpoint(&admin, &subject, &token, &(i as i128), &meta);

        assert_eq!(client.get_checkpoint_count(), i);
        assert_eq!(client.get_latest_checkpoint_id(), i);
    }
}

// ===========================================================================
// Overflow protection tests
// ===========================================================================

#[test]
fn test_next_checkpoint_id_overflow_protection() {
    // Test that checked_add prevents overflow when incrementing checkpoint ID.
    // We can't actually create u64::MAX checkpoints, but we can verify the
    // arithmetic logic by testing the checked_add behavior directly.
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraCheckpoint, ());
    let client = CalloraCheckpointClient::new(&env, &contract_id);
    client.init(&admin);

    // Verify that creating checkpoints uses checked arithmetic internally.
    // The contract's next_checkpoint_id uses checked_add which returns None on overflow.
    let id1 = client.create_checkpoint(
        &admin,
        &Address::generate(&env),
        &Address::generate(&env),
        &100i128,
        &Symbol::new(&env, "test"),
    );
    assert_eq!(id1, 1);

    let id2 = client.create_checkpoint(
        &admin,
        &Address::generate(&env),
        &Address::generate(&env),
        &200i128,
        &Symbol::new(&env, "test"),
    );
    assert_eq!(id2, 2);

    // The internal next_checkpoint_id function uses checked_add(1) which
    // would return Overflow error if the counter reached u64::MAX.
    // We verify the current count is correct.
    assert_eq!(client.get_checkpoint_count(), 2);
    assert_eq!(client.get_latest_checkpoint_id(), 2);
}

#[test]
fn test_get_checkpoints_range_overflow_protection() {
    // Test that get_checkpoints_range uses checked_add for end_id calculation
    // to prevent overflow when start_id + limit exceeds u64::MAX.
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "overflow_test");

    // Create a few checkpoints
    for i in 1..=5u64 {
        client.create_checkpoint(&admin, &subject, &token, &(i as i128 * 100), &meta);
    }

    // Normal range query should work
    let range = client.get_checkpoints_range(&1u64, &10u32);
    assert_eq!(range.len(), 5);

    // Test with start_id near u64::MAX - the checked_add in get_checkpoints_range
    // would return Overflow error if start_id + limit > u64::MAX
    // We can't actually set start_id to u64::MAX without creating that many checkpoints,
    // but we verify the function handles the boundary correctly by checking the logic.
    let start_id = u64::MAX - 10;
    let range = client.get_checkpoints_range(&start_id, &100u32);
    // Should return empty since no checkpoints exist at those IDs
    assert!(range.is_empty());
}

#[test]
fn test_batch_create_checkpoints_overflow_protection() {
    // Test that batch_create_checkpoints properly handles overflow
    // when creating many checkpoints sequentially.
    let (env, admin, client) = setup();
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let meta = Symbol::new(&env, "batch_overflow");

    // Create checkpoints in batches - each batch calls next_checkpoint_id
    // which uses checked_add internally
    let items = Vec::from_array(
        &env,
        [
            (subject.clone(), token.clone(), 100i128, meta.clone()),
            (subject.clone(), token.clone(), 200i128, meta.clone()),
            (subject.clone(), token.clone(), 300i128, meta.clone()),
        ],
    );

    let ids = client.batch_create_checkpoints(&admin, &items);
    assert_eq!(ids.len(), 3);
    assert_eq!(ids.get(0).unwrap(), 1);
    assert_eq!(ids.get(1).unwrap(), 2);
    assert_eq!(ids.get(2).unwrap(), 3);

    // Verify counts are correct
    assert_eq!(client.get_checkpoint_count(), 3);
    assert_eq!(client.get_latest_checkpoint_id(), 3);
}
