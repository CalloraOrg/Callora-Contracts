#![cfg(test)]

use super::*;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _, storage::Persistent as _};
use soroban_sdk::{Address, Env, IntoVal, String, Symbol};

#[test]
fn test_init_and_register() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ErrorsContract, ());
    let client = ErrorsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);

    assert_eq!(client.init(&admin), ());

    assert_eq!(
        client.try_init(&admin).unwrap_err().unwrap(),
        Error::AlreadyInitialized
    );

    let desc = String::from_str(&env, "Insufficient Balance");
    assert_eq!(client.register_error(&admin, &101, &desc), ());
}

#[test]
fn test_unauthorized_registration() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ErrorsContract, ());
    let client = ErrorsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let fake_admin = Address::generate(&env);

    client.init(&admin);

    let desc = String::from_str(&env, "Unauthorized Action");

    assert_eq!(
        client
            .try_register_error(&fake_admin, &102, &desc)
            .unwrap_err()
            .unwrap(),
        Error::Unauthorized
    );
}

#[test]
fn test_log_error() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ErrorsContract, ());
    let client = ErrorsContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    assert_eq!(client.log_error(&user, &101), ());
}

#[test]
fn test_overflow_protection() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ErrorsContract, ());
    let client = ErrorsContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    assert_eq!(
        client.try_log_error(&user, &u32::MAX).unwrap_err().unwrap(),
        Error::Overflow
    );
}

#[test]
fn test_overflow_boundary_safe() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ErrorsContract, ());
    let client = ErrorsContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    // u32::MAX - 1 is the largest code that does NOT overflow when adding 1.
    assert_eq!(client.log_error(&user, &(u32::MAX - 1)), ());
}

#[test]
fn test_overflow_zero_code() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ErrorsContract, ());
    let client = ErrorsContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    // code = 0: checked_add(1) -> Some(1), no overflow.
    assert_eq!(client.log_error(&user, &0u32), ());
}

#[test]
fn test_overflow_edge_round_trip() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ErrorsContract, ());
    let client = ErrorsContractClient::new(&env, &contract_id);
    let user = Address::generate(&env);

    // Boundary: code at max - 1 succeeds, max fails, then max - 1 still succeeds.
    assert_eq!(client.log_error(&user, &(u32::MAX - 1)), ());
    assert_eq!(
        client.try_log_error(&user, &u32::MAX).unwrap_err().unwrap(),
        Error::Overflow
    );
    assert_eq!(client.log_error(&user, &(u32::MAX - 1)), ());
}

#[test]
fn test_overflow_multiple_users_safe() {
    let env = Env::default();
    env.mock_all_auths();

    let contract_id = env.register(ErrorsContract, ());
    let client = ErrorsContractClient::new(&env, &contract_id);

    // Multiple users logging at different code values — all safe.
    for i in 0..10u32 {
        let user = Address::generate(&env);
        assert_eq!(client.log_error(&user, &(u32::MAX - 1 - i)), ());
    }
}

// ---------------------------------------------------------------------------
// Helpers for registration/update tests (#1225)
// ---------------------------------------------------------------------------

/// Initialise a contract with a known admin and return the pieces needed by
/// the tests below.
fn setup() -> (Env, Address, ErrorsContractClient<'static>, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(ErrorsContract, ());
    let client = ErrorsContractClient::new(&env, &contract_id);
    client.init(&admin);
    (env, contract_id, client, admin)
}

/// Read the stored description for `code`, if any.
fn stored_desc(env: &Env, contract_id: &Address, code: u32) -> Option<String> {
    env.as_contract(contract_id, || {
        env.storage().persistent().get(&DataKey::ErrorReg(code))
    })
}

/// Remaining TTL (in ledgers) of the persistent `ErrorReg(code)` entry.
fn reg_ttl(env: &Env, contract_id: &Address, code: u32) -> u32 {
    env.as_contract(contract_id, || {
        env.storage().persistent().get_ttl(&DataKey::ErrorReg(code))
    })
}

/// Assert that exactly one event was emitted by `contract_id` and decode it
/// into `(topic0, topic1, (code, desc))`.
fn only_event(env: &Env, contract_id: &Address) -> (Symbol, Address, (u32, String)) {
    let events = env.events().all();
    assert_eq!(events.len(), 1, "expected exactly one event");
    let event = events.get(0).unwrap();
    assert_eq!(
        event.0, *contract_id,
        "event must come from the errors contract"
    );
    let topic0: Symbol = event.1.get(0).unwrap().into_val(env);
    let topic1: Address = event.1.get(1).unwrap().into_val(env);
    let data: (u32, String) = event.2.into_val(env);
    (topic0, topic1, data)
}

fn desc_of_len(env: &Env, len: usize) -> String {
    String::from_str(env, &"x".repeat(len))
}

// ---------------------------------------------------------------------------
// Acceptance criteria (#1225): bounded descriptions
// ---------------------------------------------------------------------------

/// A description of exactly `MAX_DESC_LEN` bytes is accepted (boundary).
#[test]
fn test_register_accepts_description_at_cap() {
    let (env, contract_id, client, admin) = setup();
    let desc = desc_of_len(&env, MAX_DESC_LEN as usize);

    assert_eq!(client.register_error(&admin, &101, &desc), ());
    assert_eq!(stored_desc(&env, &contract_id, 101).unwrap(), desc);
}

/// A description of `MAX_DESC_LEN + 1` bytes is rejected with nothing written
/// and no event emitted (long descriptions rejected).
#[test]
fn test_register_rejects_long_description() {
    let (env, contract_id, client, admin) = setup();
    let desc = desc_of_len(&env, MAX_DESC_LEN as usize + 1);

    assert_eq!(
        client
            .try_register_error(&admin, &101, &desc)
            .unwrap_err()
            .unwrap(),
        Error::DescriptionTooLong
    );
    assert!(
        stored_desc(&env, &contract_id, 101).is_none(),
        "rejected registration must not write storage"
    );
    assert_eq!(
        env.events().all().len(),
        0,
        "rejected registration must not emit an event"
    );
}

/// The same cap applies on the explicit update path.
#[test]
fn test_update_rejects_long_description() {
    let (env, contract_id, client, admin) = setup();
    let original = String::from_str(&env, "original");
    client.register_error(&admin, &101, &original);

    let too_long = desc_of_len(&env, MAX_DESC_LEN as usize + 1);
    assert_eq!(
        client
            .try_update_error(&admin, &101, &too_long)
            .unwrap_err()
            .unwrap(),
        Error::DescriptionTooLong
    );
    assert_eq!(
        stored_desc(&env, &contract_id, 101).unwrap(),
        original,
        "rejected update must leave the stored description untouched"
    );
}

// ---------------------------------------------------------------------------
// Acceptance criteria (#1225): no silent overwrites
// ---------------------------------------------------------------------------

/// Re-registering an existing code is rejected and the original description
/// is preserved (overwrite without update path rejected).
#[test]
fn test_register_rejects_duplicate_and_preserves_original() {
    let (env, contract_id, client, admin) = setup();
    let first = String::from_str(&env, "first");
    client.register_error(&admin, &101, &first);

    let second = String::from_str(&env, "second");
    assert_eq!(
        client
            .try_register_error(&admin, &101, &second)
            .unwrap_err()
            .unwrap(),
        Error::AlreadyRegistered
    );
    assert_eq!(
        stored_desc(&env, &contract_id, 101).unwrap(),
        first,
        "duplicate registration must not overwrite the stored description"
    );
    assert_eq!(
        env.events().all().len(),
        0,
        "failed registration must not emit an event"
    );
}

/// `update_error` on a code that was never registered is rejected.
#[test]
fn test_update_requires_existing_code() {
    let (env, _contract_id, client, admin) = setup();

    let desc = String::from_str(&env, "phantom");
    assert_eq!(
        client
            .try_update_error(&admin, &999, &desc)
            .unwrap_err()
            .unwrap(),
        Error::NotRegistered
    );
    assert_eq!(
        env.events().all().len(),
        0,
        "failed update must not emit an event"
    );
}

/// Only a non-admin caller is rejected; the stored admin may update.
#[test]
fn test_unauthorized_update() {
    let (env, _contract_id, client, admin) = setup();
    client.register_error(&admin, &101, &String::from_str(&env, "old"));

    let fake_admin = Address::generate(&env);
    let desc = String::from_str(&env, "new");
    assert_eq!(
        client
            .try_update_error(&fake_admin, &101, &desc)
            .unwrap_err()
            .unwrap(),
        Error::Unauthorized
    );
}

// ---------------------------------------------------------------------------
// Acceptance criteria (#1225): event emitted and TTL extended
// ---------------------------------------------------------------------------

/// A successful registration emits `error_registered` with the expected
/// topics/payload and leaves the persistent entry at full TTL.
#[test]
fn test_register_emits_event_and_extends_ttl() {
    let (env, contract_id, client, admin) = setup();
    let desc = String::from_str(&env, "Insufficient Balance");

    assert_eq!(client.register_error(&admin, &101, &desc), ());

    let (topic0, topic1, (code, payload_desc)) = only_event(&env, &contract_id);
    assert_eq!(topic0, Symbol::new(&env, "error_registered"));
    assert_eq!(topic1, admin);
    assert_eq!(code, 101);
    assert_eq!(payload_desc, desc);
    assert_eq!(
        reg_ttl(&env, &contract_id, 101),
        PERSISTENT_BUMP_AMOUNT,
        "registration must extend the persistent entry to full TTL"
    );
}

/// A successful update emits `error_updated`, rewrites the value, and
/// re-extends the TTL after it has aged below the threshold.
#[test]
fn test_update_replaces_description_emits_event_and_extends_ttl() {
    let (env, contract_id, client, admin) = setup();
    let original = String::from_str(&env, "old");
    client.register_error(&admin, &101, &original);

    // Age the entry below the extension threshold.
    let seq = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq + PERSISTENT_BUMP_AMOUNT - PERSISTENT_BUMP_THRESHOLD + 10);
    let ttl_before = reg_ttl(&env, &contract_id, 101);
    assert!(
        ttl_before < PERSISTENT_BUMP_THRESHOLD,
        "precondition: TTL must be below threshold before the update"
    );

    let updated = String::from_str(&env, "new");
    assert_eq!(client.update_error(&admin, &101, &updated), ());

    // NB: the test event buffer is cleared on contract-frame entry (including
    // `as_contract`), so event assertions must run before any storage reads.
    let (topic0, topic1, (code, payload_desc)) = only_event(&env, &contract_id);
    assert_eq!(topic0, Symbol::new(&env, "error_updated"));
    assert_eq!(topic1, admin);
    assert_eq!(code, 101);
    assert_eq!(payload_desc, updated);

    assert_eq!(
        stored_desc(&env, &contract_id, 101).unwrap(),
        updated,
        "update must rewrite the stored description"
    );
    assert_eq!(
        reg_ttl(&env, &contract_id, 101),
        PERSISTENT_BUMP_AMOUNT,
        "update must re-extend the persistent entry to full TTL"
    );
}

/// Both write paths reject uninitialised callers with `NotInitialized`.
#[test]
fn test_register_and_update_require_init() {
    let env = Env::default();
    env.mock_all_auths();
    let contract_id = env.register(ErrorsContract, ());
    let client = ErrorsContractClient::new(&env, &contract_id);
    let admin = Address::generate(&env);
    let desc = String::from_str(&env, "desc");

    assert_eq!(
        client
            .try_register_error(&admin, &101, &desc)
            .unwrap_err()
            .unwrap(),
        Error::NotInitialized
    );
    assert_eq!(
        client
            .try_update_error(&admin, &101, &desc)
            .unwrap_err()
            .unwrap(),
        Error::NotInitialized
    );
}

// ---------------------------------------------------------------------------
// Acceptance criteria (#1225): log_error behaviour unchanged
// ---------------------------------------------------------------------------

/// `log_error` on a registered code behaves exactly as before: it succeeds,
/// stores the user's recent code, and never consults the registry.
#[test]
fn test_log_error_unchanged_for_registered_codes() {
    let (env, contract_id, client, admin) = setup();
    let user = Address::generate(&env);

    // Register a description for code 101 — log_error must be indifferent.
    client.register_error(&admin, &101, &String::from_str(&env, "insufficient"));

    assert_eq!(client.log_error(&user, &101), ());
    let recent: Option<u32> = env.as_contract(&contract_id, || {
        env.storage()
            .temporary()
            .get(&DataKey::RecentErr(user.clone()))
    });
    assert_eq!(recent, Some(101), "log_error must store the recent code");

    // Overflow protection is also unchanged: even a registered description
    // for u32::MAX does not soften the Overflow rejection.
    client.register_error(&admin, &u32::MAX, &String::from_str(&env, "overflow"));
    assert_eq!(
        client.try_log_error(&user, &u32::MAX).unwrap_err().unwrap(),
        Error::Overflow
    );
}
