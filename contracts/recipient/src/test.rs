use crate::{
    events, CalloraRecipient, CalloraRecipientClient, RecipientError, RecipientUpdatedEvent,
    StorageKey, INSTANCE_BUMP_AMOUNT, LEDGERS_PER_DAY, MAX_PAGE_SIZE, PERSISTENT_BUMP_AMOUNT,
    PERSISTENT_BUMP_THRESHOLD,
};
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{Address, Env, IntoVal, String as SorobanString, Symbol};

/// Helper to create an Env and register the contract, returning (env, admin, client).
fn setup() -> (Env, Address, CalloraRecipientClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_addr = env.register(CalloraRecipient, ());
    let client = CalloraRecipientClient::new(&env, &contract_addr);
    (env, admin, client)
}

/// Helper to create a SorobanString from a &str.
fn name(env: &Env, s: &str) -> SorobanString {
    SorobanString::from_bytes(env, s.as_bytes())
}

// ---------------------------------------------------------------------------
// Initialization tests
// ---------------------------------------------------------------------------

#[test]
fn init_sets_admin_and_count() {
    let (_env, admin, client) = setup();
    client.init(&admin);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_recipient_count(), 0u32);
}

#[test]
fn init_double_init_rejected() {
    let (_env, admin, client) = setup();
    client.init(&admin);
    let err = client.try_init(&admin);
    assert_eq!(err, Err(Ok(RecipientError::AlreadyInitialized)));
}

#[test]
fn init_requires_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_addr = env.register(CalloraRecipient, ());
    let client = CalloraRecipientClient::new(&env, &contract_addr);

    // Without mock_all_auths, require_auth should fail.
    let err = client.try_init(&admin);
    assert!(err.is_err());
}

// ---------------------------------------------------------------------------
// register_recipient tests
// ---------------------------------------------------------------------------

#[test]
fn register_recipient_happy_path() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let addr = Address::generate(&env);
    let recipient_name = name(&env, "treasury");

    client.register_recipient(&admin, &recipient_name, &addr);

    assert!(client.has_recipient(&recipient_name));
    let record = client.get_recipient(&recipient_name);
    assert_eq!(record.address, addr);
    assert_eq!(record.name, recipient_name);
    assert_eq!(client.get_recipient_count(), 1u32);
}

#[test]
fn register_recipient_duplicate_rejected() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let addr = Address::generate(&env);
    let addr2 = Address::generate(&env);
    let recipient_name = name(&env, "ops");

    client.register_recipient(&admin, &recipient_name, &addr);
    let err = client.try_register_recipient(&admin, &recipient_name, &addr2);
    assert_eq!(err, Err(Ok(RecipientError::AlreadyRegistered)));
}

#[test]
fn register_recipient_empty_name_rejected() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let addr = Address::generate(&env);
    let empty_name = name(&env, "");

    let err = client.try_register_recipient(&admin, &empty_name, &addr);
    assert_eq!(err, Err(Ok(RecipientError::InvalidName)));
}

#[test]
fn register_recipient_unauthorized() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_addr = env.register(CalloraRecipient, ());
    let client = CalloraRecipientClient::new(&env, &contract_addr);

    env.mock_all_auths();
    client.init(&admin);

    let outsider = Address::generate(&env);
    let addr = Address::generate(&env);
    let recipient_name = name(&env, "x");

    // Clear mock auths so outsider's require_auth fails.
    env.set_auths(&[]);
    let err = client.try_register_recipient(&outsider, &recipient_name, &addr);
    assert!(err.is_err());
}

// ---------------------------------------------------------------------------
// update_recipient tests
// ---------------------------------------------------------------------------

#[test]
fn update_recipient_happy_path() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let addr1 = Address::generate(&env);
    let addr2 = Address::generate(&env);
    let recipient_name = name(&env, "vendor");

    client.register_recipient(&admin, &recipient_name, &addr1);
    client.update_recipient(&admin, &recipient_name, &addr2);

    let record = client.get_recipient(&recipient_name);
    assert_eq!(record.address, addr2);
    assert_eq!(client.get_recipient_count(), 1u32);
}

#[test]
fn update_recipient_not_found() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let addr = Address::generate(&env);
    let missing = name(&env, "nope");

    let err = client.try_update_recipient(&admin, &missing, &addr);
    assert_eq!(err, Err(Ok(RecipientError::NotFound)));
}

// ---------------------------------------------------------------------------
// remove_recipient tests
// ---------------------------------------------------------------------------

#[test]
fn remove_recipient_happy_path() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let addr = Address::generate(&env);
    let recipient_name = name(&env, "temp");

    client.register_recipient(&admin, &recipient_name, &addr);
    assert_eq!(client.get_recipient_count(), 1u32);

    client.remove_recipient(&admin, &recipient_name);
    assert!(!client.has_recipient(&recipient_name));
    assert_eq!(client.get_recipient_count(), 0u32);
}

#[test]
fn remove_recipient_not_found() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let missing = name(&env, "ghost");
    let err = client.try_remove_recipient(&admin, &missing);
    assert_eq!(err, Err(Ok(RecipientError::NotFound)));
}

// ---------------------------------------------------------------------------
// View-only tests
// ---------------------------------------------------------------------------

#[test]
fn get_recipient_not_found() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let missing = name(&env, "nobody");
    let err = client.try_get_recipient(&missing);
    assert_eq!(err, Err(Ok(RecipientError::NotFound)));
}

#[test]
fn has_recipient_returns_false_for_missing() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let missing = name(&env, "nope");
    assert!(!client.has_recipient(&missing));
}

#[test]
fn get_recipient_count_starts_at_zero() {
    let (_env, admin, client) = setup();
    client.init(&admin);
    assert_eq!(client.get_recipient_count(), 0u32);
}

#[test]
fn uninitialized_contract_returns_not_initialized() {
    let env = Env::default();
    let contract_addr = env.register(CalloraRecipient, ());
    let client = CalloraRecipientClient::new(&env, &contract_addr);

    let err = client.try_get_admin();
    assert_eq!(err, Err(Ok(RecipientError::NotInitialized)));
}

// ---------------------------------------------------------------------------
// list_recipients tests
// ---------------------------------------------------------------------------

#[test]
fn list_recipients_empty() {
    let (_env, admin, client) = setup();
    client.init(&admin);

    let page = client.list_recipients(&0, &10);
    assert!(page.is_empty());
}

#[test]
fn list_recipients_single_page() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let n_a = name(&env, "alpha");
    let n_b = name(&env, "beta");
    let n_c = name(&env, "gamma");

    for n in [&n_a, &n_b, &n_c] {
        let addr = Address::generate(&env);
        client.register_recipient(&admin, n, &addr);
    }

    let page = client.list_recipients(&0, &10);
    assert_eq!(page.len(), 3);

    // All registered names must appear somewhere in the page.
    let contains =
        |needle: &SorobanString| (0..page.len()).any(|i| page.get(i).unwrap() == *needle);
    assert!(contains(&n_a), "alpha missing");
    assert!(contains(&n_b), "beta missing");
    assert!(contains(&n_c), "gamma missing");
}

#[test]
fn list_recipients_pagination() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let names = [
        name(&env, "r0"),
        name(&env, "r1"),
        name(&env, "r2"),
        name(&env, "r3"),
        name(&env, "r4"),
    ];
    for n in &names {
        let addr = Address::generate(&env);
        client.register_recipient(&admin, n, &addr);
    }

    // Page size 2, walk all pages.
    let page0 = client.list_recipients(&0, &2);
    let page1 = client.list_recipients(&2, &2);
    let page2 = client.list_recipients(&4, &2);

    assert_eq!(page0.len(), 2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 1); // only one entry left

    // Collect all results and verify no duplicates.
    let mut seen: std::vec::Vec<SorobanString> = std::vec::Vec::new();
    for page in [&page0, &page1, &page2] {
        for i in 0..page.len() {
            let entry = page.get(i).unwrap();
            assert!(
                !seen.iter().any(|s| s == &entry),
                "duplicate name across pages"
            );
            seen.push(entry);
        }
    }
    assert_eq!(seen.len(), 5);
}

#[test]
fn list_recipients_page_size_capped() {
    let (env, admin, client) = setup();
    client.init(&admin);

    // Register MAX_PAGE_SIZE + 5 entries.
    for i in 0u32..(MAX_PAGE_SIZE + 5) {
        // Build a unique name like "n000", "n001", ...
        let s = std::format!("n{:03}", i);
        let n = name(&env, &s);
        let addr = Address::generate(&env);
        client.register_recipient(&admin, &n, &addr);
    }

    // Even with a huge limit, result must not exceed MAX_PAGE_SIZE.
    let page = client.list_recipients(&0, &1000);
    assert_eq!(page.len() as u32, MAX_PAGE_SIZE);
}

#[test]
fn list_recipients_start_beyond_count_returns_empty() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let addr = Address::generate(&env);
    client.register_recipient(&admin, &name(&env, "only"), &addr);

    let page = client.list_recipients(&99, &10);
    assert!(page.is_empty());
}

#[test]
fn list_recipients_removed_names_absent() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let n_a = name(&env, "aaa");
    let n_b = name(&env, "bbb");
    let n_c = name(&env, "ccc");

    for n in [&n_a, &n_b, &n_c] {
        let addr = Address::generate(&env);
        client.register_recipient(&admin, n, &addr);
    }

    client.remove_recipient(&admin, &n_b);

    let page = client.list_recipients(&0, &10);
    assert_eq!(page.len(), 2);
    for i in 0..page.len() {
        assert_ne!(
            page.get(i).unwrap(),
            n_b,
            "removed name still appears in listing"
        );
    }
}

#[test]
fn list_recipients_no_auth_required() {
    // list_recipients must be callable without any authentication.
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract_addr = env.register(CalloraRecipient, ());
    let client = CalloraRecipientClient::new(&env, &contract_addr);

    env.mock_all_auths();
    client.init(&admin);
    let addr = Address::generate(&env);
    client.register_recipient(&admin, &name(&env, "pub"), &addr);
    env.set_auths(&[]);

    // Must succeed without any auth.
    let page = client.list_recipients(&0, &10);
    assert_eq!(page.len(), 1);
}

#[test]
fn list_recipients_zero_limit_uses_cap() {
    let (env, admin, client) = setup();
    client.init(&admin);

    for n in ["x0", "x1", "x2", "x3", "x4"] {
        let addr = Address::generate(&env);
        client.register_recipient(&admin, &name(&env, n), &addr);
    }

    // limit=0 should fall back to MAX_PAGE_SIZE, returning all 5.
    let page = client.list_recipients(&0, &0);
    assert_eq!(page.len(), 5);
}

// ---------------------------------------------------------------------------
// Issue #1411: TTL management and audit trail tests
// ---------------------------------------------------------------------------

#[test]
fn test_plan_scenario_1_after_init_instance_ttl() {
    let (env, admin, client) = setup();
    client.init(&admin);
    let ttl = env.as_contract(&client.address, || env.storage().instance().get_ttl());
    assert!(
        ttl >= INSTANCE_BUMP_AMOUNT - 1,
        "instance TTL {} must be >= {}",
        ttl,
        INSTANCE_BUMP_AMOUNT - 1
    );
}

#[test]
fn test_plan_scenario_2_after_register_persistent_ttls() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let recipient_name = name(&env, "treasury");
    let addr = Address::generate(&env);
    client.register_recipient(&admin, &recipient_name, &addr);

    let key_rec = StorageKey::Recipient(recipient_name.clone());
    let key_idx = StorageKey::NameIndex(0);
    let key_pos = StorageKey::NamePosition(recipient_name.clone());

    let (ttl_rec, ttl_idx, ttl_pos) = env.as_contract(&client.address, || {
        (
            env.storage().persistent().get_ttl(&key_rec),
            env.storage().persistent().get_ttl(&key_idx),
            env.storage().persistent().get_ttl(&key_pos),
        )
    });

    assert!(
        ttl_rec >= PERSISTENT_BUMP_AMOUNT - 1,
        "Recipient TTL {} must be >= {}",
        ttl_rec,
        PERSISTENT_BUMP_AMOUNT - 1
    );
    assert!(
        ttl_idx >= PERSISTENT_BUMP_AMOUNT - 1,
        "NameIndex TTL {} must be >= {}",
        ttl_idx,
        PERSISTENT_BUMP_AMOUNT - 1
    );
    assert!(
        ttl_pos >= PERSISTENT_BUMP_AMOUNT - 1,
        "NamePosition TTL {} must be >= {}",
        ttl_pos,
        PERSISTENT_BUMP_AMOUNT - 1
    );
}

#[test]
fn test_plan_scenario_3_advance_90_days_then_get_recipient_succeeds() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let recipient_name = name(&env, "payroll");
    let addr = Address::generate(&env);
    client.register_recipient(&admin, &recipient_name, &addr);

    // Keep instance alive at day 45 (within the 60-day instance bump window)
    let seq = env.ledger().sequence();
    env.ledger().set_sequence_number(seq + LEDGERS_PER_DAY * 45);
    let _ = client.get_admin();

    // Advance to day 90
    env.ledger().set_sequence_number(seq + LEDGERS_PER_DAY * 90);

    // Persistent Recipient was not rewritten for 90 days.
    // On main (minimum TTL ~16-30 days), this would fail because the persistent entry archived.
    // With 120-day bump, it succeeds.
    let res = client.get_recipient(&recipient_name);
    assert_eq!(res.address, addr);
    assert_eq!(res.name, recipient_name);
}

#[test]
fn test_plan_scenario_4_update_recipient_event_and_ttl() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let recipient_name = name(&env, "vendor");
    let addr1 = Address::generate(&env);
    let addr2 = Address::generate(&env);

    client.register_recipient(&admin, &recipient_name, &addr1);

    // Advance sequence past PERSISTENT_BUMP_AMOUNT - PERSISTENT_BUMP_THRESHOLD (90 days)
    // so remaining persistent TTL drops below PERSISTENT_BUMP_THRESHOLD (30 days).
    let seq = env.ledger().sequence();
    env.ledger().set_sequence_number(seq + LEDGERS_PER_DAY * 45);
    let _ = client.get_admin();

    env.ledger()
        .set_sequence_number(seq + PERSISTENT_BUMP_AMOUNT - PERSISTENT_BUMP_THRESHOLD + 10);

    let key_rec = StorageKey::Recipient(recipient_name.clone());
    let ttl_before = env.as_contract(&client.address, || {
        env.storage().persistent().get_ttl(&key_rec)
    });
    assert!(ttl_before < PERSISTENT_BUMP_THRESHOLD);

    client.update_recipient(&admin, &recipient_name, &addr2);

    // Verify event carries old and new addresses (must be read before next invocation)
    let events = env.events().all();
    let (_, topics, data) = events.last().expect("event emitted");
    let t0: Symbol = topics.get(0).unwrap().into_val(&env);
    let t1: SorobanString = topics.get(1).unwrap().into_val(&env);
    assert_eq!(t0, events::event_recipient_updated(&env));
    assert_eq!(t1, recipient_name);

    let payload: RecipientUpdatedEvent = data.into_val(&env);
    assert_eq!(payload.name, recipient_name);
    assert_eq!(payload.old_address, addr1);
    assert_eq!(payload.new_address, addr2);

    // Verify record has updated address and view emits zero events
    let record = client.get_recipient(&recipient_name);
    assert_eq!(record.address, addr2);
    assert_eq!(
        env.events().all().len(),
        0,
        "view functions must emit 0 events"
    );

    // Verify TTLs are refreshed
    let key_idx = StorageKey::NameIndex(0);
    let key_pos = StorageKey::NamePosition(recipient_name.clone());
    let (ttl_rec, ttl_idx, ttl_pos, ttl_inst) = env.as_contract(&client.address, || {
        (
            env.storage().persistent().get_ttl(&key_rec),
            env.storage().persistent().get_ttl(&key_idx),
            env.storage().persistent().get_ttl(&key_pos),
            env.storage().instance().get_ttl(),
        )
    });

    assert!(ttl_rec >= PERSISTENT_BUMP_AMOUNT - 1);
    assert!(ttl_idx >= PERSISTENT_BUMP_AMOUNT - 1);
    assert!(ttl_pos >= PERSISTENT_BUMP_AMOUNT - 1);
    assert!(ttl_inst >= INSTANCE_BUMP_AMOUNT - 1);
}

#[test]
fn test_plan_scenario_5_remove_recipient_all_three_keys_gone() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let name_alice = name(&env, "alice");
    let name_bob = name(&env, "bob");
    let addr_alice = Address::generate(&env);
    let addr_bob = Address::generate(&env);

    client.register_recipient(&admin, &name_alice, &addr_alice);
    client.register_recipient(&admin, &name_bob, &addr_bob);

    let key_alice_rec = StorageKey::Recipient(name_alice.clone());
    let key_alice_pos = StorageKey::NamePosition(name_alice.clone());
    let key_idx_1 = StorageKey::NameIndex(1);

    // Both exist
    env.as_contract(&client.address, || {
        assert!(env.storage().persistent().has(&key_alice_rec));
        assert!(env.storage().persistent().has(&key_alice_pos));
        assert!(env.storage().persistent().has(&key_idx_1));
    });

    // Remove alice (slot 0) -> bob (slot 1) swap-moves into slot 0, slot 1 removed
    client.remove_recipient(&admin, &name_alice);

    env.as_contract(&client.address, || {
        assert!(
            !env.storage().persistent().has(&key_alice_rec),
            "alice Recipient key must be gone"
        );
        assert!(
            !env.storage().persistent().has(&key_alice_pos),
            "alice NamePosition key must be gone"
        );
        assert!(
            !env.storage().persistent().has(&key_idx_1),
            "tail NameIndex slot 1 must be gone"
        );
    });

    // Remove bob (slot 0) -> tail removal
    let key_bob_rec = StorageKey::Recipient(name_bob.clone());
    let key_bob_pos = StorageKey::NamePosition(name_bob.clone());
    let key_idx_0 = StorageKey::NameIndex(0);

    client.remove_recipient(&admin, &name_bob);

    env.as_contract(&client.address, || {
        assert!(
            !env.storage().persistent().has(&key_bob_rec),
            "bob Recipient key must be gone"
        );
        assert!(
            !env.storage().persistent().has(&key_bob_pos),
            "bob NamePosition key must be gone"
        );
        assert!(
            !env.storage().persistent().has(&key_idx_0),
            "NameIndex slot 0 must be gone"
        );
    });
    assert_eq!(client.get_recipient_count(), 0u32);
}

#[test]
fn test_plan_scenario_6_list_recipients_page_only_returned_entries_bumped() {
    let (env, admin, client) = setup();
    client.init(&admin);

    let n0 = name(&env, "r0");
    let n1 = name(&env, "r1");
    let n2 = name(&env, "r2");
    let n3 = name(&env, "r3");

    for n in [&n0, &n1, &n2, &n3] {
        let addr = Address::generate(&env);
        client.register_recipient(&admin, n, &addr);
    }

    // Advance sequence past PERSISTENT_BUMP_AMOUNT - PERSISTENT_BUMP_THRESHOLD (90 days)
    // so remaining persistent TTL drops below PERSISTENT_BUMP_THRESHOLD (30 days).
    let seq = env.ledger().sequence();
    env.ledger().set_sequence_number(seq + LEDGERS_PER_DAY * 45);
    let _ = client.get_admin();

    env.ledger()
        .set_sequence_number(seq + PERSISTENT_BUMP_AMOUNT - PERSISTENT_BUMP_THRESHOLD + 10);

    // List page of size 2 (r0 and r1 returned)
    let page = client.list_recipients(&0, &2);
    assert_eq!(page.len(), 2);

    let key_r0 = StorageKey::Recipient(n0);
    let key_r1 = StorageKey::Recipient(n1);
    let key_r2 = StorageKey::Recipient(n2);
    let key_r3 = StorageKey::Recipient(n3);

    let (ttl_r0, ttl_r1, ttl_r2, ttl_r3) = env.as_contract(&client.address, || {
        (
            env.storage().persistent().get_ttl(&key_r0),
            env.storage().persistent().get_ttl(&key_r1),
            env.storage().persistent().get_ttl(&key_r2),
            env.storage().persistent().get_ttl(&key_r3),
        )
    });

    // Returned entries (r0, r1) are bumped back to full persistent bump
    assert!(
        ttl_r0 >= PERSISTENT_BUMP_AMOUNT - 1,
        "r0 TTL must be bumped"
    );
    assert!(
        ttl_r1 >= PERSISTENT_BUMP_AMOUNT - 1,
        "r1 TTL must be bumped"
    );

    // Unreturned entries (r2, r3) were NOT bumped (aged ~50 days)
    assert!(
        ttl_r2 < PERSISTENT_BUMP_AMOUNT - LEDGERS_PER_DAY * 30,
        "r2 TTL must NOT be bumped"
    );
    assert!(
        ttl_r3 < PERSISTENT_BUMP_AMOUNT - LEDGERS_PER_DAY * 30,
        "r3 TTL must NOT be bumped"
    );
}

#[test]
fn test_plan_scenario_7_unauthorized_register_or_update() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let outsider = Address::generate(&env);
    let contract_addr = env.register(CalloraRecipient, ());
    let client = CalloraRecipientClient::new(&env, &contract_addr);

    env.mock_all_auths();
    client.init(&admin);

    let r_name = name(&env, "restricted");
    let addr1 = Address::generate(&env);
    let addr2 = Address::generate(&env);

    client.register_recipient(&admin, &r_name, &addr1);

    // Clear mock auths so outsider lacks authorization
    env.set_auths(&[]);

    let err_reg = client.try_register_recipient(&outsider, &name(&env, "other"), &addr1);
    assert!(err_reg.is_err(), "unauthorized register must fail");

    let err_upd = client.try_update_recipient(&outsider, &r_name, &addr2);
    assert!(err_upd.is_err(), "unauthorized update must fail");
}

// ---------------------------------------------------------------------------
// Rustdoc coverage test
// ---------------------------------------------------------------------------

#[test]
fn every_public_fn_in_lib_has_rustdoc() {
    let source = include_str!("lib.rs")
        .split("// ---------------------------------------------------------------------------\n// Test modules")
        .next()
        .expect("lib.rs contains test module marker");
    let lines: std::vec::Vec<&str> = source.lines().collect();

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if !(trimmed.starts_with("pub fn ")
            || trimmed.starts_with("pub(crate) fn ")
            || trimmed.starts_with("pub(super) fn "))
        {
            continue;
        }

        let has_rustdoc = lines[..idx]
            .iter()
            .rev()
            .map(|candidate| candidate.trim_start())
            .find(|candidate| !candidate.is_empty())
            .map(|candidate| candidate.starts_with("///"))
            .unwrap_or(false);

        assert!(
            has_rustdoc,
            "public function on line {} is missing /// rustdoc: {}",
            idx + 1,
            trimmed
        );
    }
}
