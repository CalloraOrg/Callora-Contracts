use crate::{CalloraRecipient, CalloraRecipientClient, RecipientError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String as SorobanString};

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
