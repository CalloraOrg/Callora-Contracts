//! Encoding and byte-length validation tests for `callora-registry`.
//!
//! Verifies that `register_offering` rejects:
//! - Empty strings for offering_id and metadata
//! - Strings exceeding byte-length limits
//! - Strings containing non-visible-ASCII bytes (control chars, DEL, etc.)
//! - Strings with leading or trailing whitespace
//!
//! Positive cases verify that valid visible-ASCII strings are accepted.

extern crate std;

use callora_registry::{CalloraRegistry, CalloraRegistryClient, RegistryError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, contractimpl, Address, Env, String};

/// Mock catalog that accepts all registrations.
#[contract]
pub struct MockCatalog;

#[contractimpl]
impl MockCatalog {
    pub fn put_offering(_env: Env, _registry: Address, _offering_id: String, _metadata: String) {}
}

fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let catalog = env.register(MockCatalog, ());
    let addr = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &addr);
    client.init(&admin, &catalog);
    let developer = Address::generate(&env);
    (env, addr, admin, developer)
}

#[test]
fn register_offering_valid_visible_ascii() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let result = client.try_register_offering(
        &admin,
        &developer,
        &String::from_str(&env, "offering-123"),
        &String::from_str(&env, "https://example.com/meta"),
    );
    assert!(result.is_ok(), "valid visible ASCII should be accepted");
}

#[test]
fn register_offering_rejects_empty_offering_id() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let result = client.try_register_offering(
        &admin,
        &developer,
        &String::from_str(&env, ""),
        &String::from_str(&env, "valid metadata"),
    );
    assert!(is_error(&result, RegistryError::InvalidOfferingId));
}

#[test]
fn register_offering_rejects_empty_metadata() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let result = client.try_register_offering(
        &admin,
        &developer,
        &String::from_str(&env, "offering-1"),
        &String::from_str(&env, ""),
    );
    assert!(is_error(&result, RegistryError::InvalidOfferingId));
}

#[test]
fn register_offering_rejects_control_char_in_offering_id() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let result = client.try_register_offering(
        &admin,
        &developer,
        &String::from_str(&env, "offer\x01id"),
        &String::from_str(&env, "valid metadata"),
    );
    assert!(is_error(&result, RegistryError::InvalidEncoding));
}

#[test]
fn register_offering_rejects_control_char_in_metadata() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let result = client.try_register_offering(
        &admin,
        &developer,
        &String::from_str(&env, "offering-1"),
        &String::from_str(&env, "meta\x00data"),
    );
    assert!(is_error(&result, RegistryError::InvalidEncoding));
}

#[test]
fn register_offering_rejects_del_char_in_metadata() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let result = client.try_register_offering(
        &admin,
        &developer,
        &String::from_str(&env, "offering-1"),
        &String::from_str(&env, "meta\x7Fdata"),
    );
    assert!(is_error(&result, RegistryError::InvalidEncoding));
}

#[test]
fn register_offering_rejects_leading_space_in_offering_id() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let result = client.try_register_offering(
        &admin,
        &developer,
        &String::from_str(&env, " offering"),
        &String::from_str(&env, "valid metadata"),
    );
    assert!(is_error(&result, RegistryError::InvalidEncoding));
}

#[test]
fn register_offering_rejects_trailing_space_in_metadata() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let result = client.try_register_offering(
        &admin,
        &developer,
        &String::from_str(&env, "offering-1"),
        &String::from_str(&env, "metadata "),
    );
    assert!(is_error(&result, RegistryError::InvalidEncoding));
}

#[test]
fn register_offering_rejects_too_long_offering_id() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let long_id: std::string::String = "a".repeat(65);
    let result = client.try_register_offering(
        &admin,
        &developer,
        &String::from_str(&env, &long_id),
        &String::from_str(&env, "valid metadata"),
    );
    assert!(is_error(&result, RegistryError::InvalidOfferingId));
}

#[test]
fn register_offering_rejects_too_long_metadata() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let long_meta: std::string::String = "a".repeat(257);
    let result = client.try_register_offering(
        &admin,
        &developer,
        &String::from_str(&env, "offering-1"),
        &String::from_str(&env, &long_meta),
    );
    assert!(is_error(&result, RegistryError::InvalidOfferingId));
}

#[test]
fn register_offering_boundary_length_offering_id_accepted() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let boundary_id: std::string::String = "a".repeat(64);
    let result = client.try_register_offering(
        &admin,
        &developer,
        &String::from_str(&env, &boundary_id),
        &String::from_str(&env, "valid metadata"),
    );
    assert!(
        result.is_ok(),
        "boundary-length offering_id should be accepted"
    );
}

#[test]
fn register_offering_boundary_length_metadata_accepted() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let boundary_meta: std::string::String = "a".repeat(256);
    let result = client.try_register_offering(
        &admin,
        &developer,
        &String::from_str(&env, "offering-1"),
        &String::from_str(&env, &boundary_meta),
    );
    assert!(
        result.is_ok(),
        "boundary-length metadata should be accepted"
    );
}

#[test]
fn register_offering_with_gate_rejects_invalid_encoding() {
    let (env, contract, admin, developer) = setup();
    let client = CalloraRegistryClient::new(&env, &contract);
    let token = Address::generate(&env);
    let result = client.try_register_offering_with_gate(
        &admin,
        &developer,
        &token,
        &100i128,
        &String::from_str(&env, "offer\x01"),
        &String::from_str(&env, "valid metadata"),
    );
    assert!(is_error(&result, RegistryError::InvalidEncoding));
}

fn is_error<V, CE: Into<soroban_sdk::Error>, E: Into<soroban_sdk::Error>>(
    result: &Result<Result<V, CE>, Result<E, soroban_sdk::InvokeError>>,
    expected: RegistryError,
) -> bool {
    let expected_code = expected as u32;
    match result {
        Err(Ok(e)) => e.into().get_code() == expected_code,
        _ => false,
    }
}
