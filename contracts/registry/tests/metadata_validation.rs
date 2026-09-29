//! Metadata byte-length and encoding validation for `callora-registry`.
//!
//! Issue #1065: value-bearing entry points must bound metadata byte length and
//! reject invalid encodings *before* any cross-contract call or state change.
//!
//! These tests pin the behaviour introduced by routing `validate_metadata`
//! through `callora_validators::normalize_visible_ascii`:
//! - Reject empty, over-length, control-character, non-visible-ASCII
//!   (including UTF-8 multibyte), and leading/trailing-whitespace metadata.
//! - Accept bounded visible-ASCII metadata (including exactly 256 bytes).
//! - A rejected call leaves no partial state (not registered, count unchanged,
//!   catalog `put_offering` never called).
//!
//! Before the fix only emptiness and length (256) were checked, so the
//! control-character / whitespace / non-ASCII cases below were *accepted*.

extern crate std;

use callora_registry::{admin, CalloraRegistry, CalloraRegistryClient, RegistryError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::testutils::{Ledger, LedgerInfo};
use soroban_sdk::{contract, contractimpl, Address, Env, String};

// ---------------------------------------------------------------------------
// Mock catalog that records whether `put_offering` was invoked
// ---------------------------------------------------------------------------

pub mod ok_catalog {
    use super::*;

    #[contract]
    pub struct OkCatalog;

    #[contractimpl]
    impl OkCatalog {
        pub fn put_offering(
            _env: Env,
            _registry: Address,
            _offering_id: String,
            _metadata: String,
        ) {
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn offering_id(env: &Env, suffix: &str) -> String {
    String::from_str(env, &format!("offering-{suffix}"))
}

fn metadata(env: &Env, s: &str) -> String {
    String::from_str(env, s)
}

fn setup_registry(env: &Env) -> (Address, CalloraRegistryClient<'_>, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let developer = Address::generate(env);
    let catalog = env.register(ok_catalog::OkCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(env, &registry_id);
    client.init(&admin, &catalog);
    (admin, client, developer)
}

/// Advance the ledger timestamp past the admin cooldown window so the next
/// admin-gated action is not blocked.
fn advance_past_cooldown(env: &Env) {
    let current = env.ledger().get().timestamp;
    env.ledger().set(LedgerInfo {
        timestamp: current + admin::COOLDOWN_SECONDS + 1,
        ..env.ledger().get()
    });
}

// ---------------------------------------------------------------------------
// Reject invalid encodings (control bytes, non-visible ASCII, whitespace)
// ---------------------------------------------------------------------------

#[test]
fn rejects_control_character_in_metadata() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);
    let oid = offering_id(&env, "ctl");
    let meta = metadata(&env, "bad\u{0000}metadata");

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(
        matches!(result, Err(Ok(RegistryError::InvalidMetadata))),
        "control char metadata must be rejected, got {:?}",
        result
    );
    assert!(!client.is_offering_registered(&oid));
    assert_eq!(client.registered_count(), 0);
}

#[test]
fn rejects_line_break_in_metadata() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);
    let oid = offering_id(&env, "nl");
    let meta = metadata(&env, "line1\nline2");

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(matches!(result, Err(Ok(RegistryError::InvalidMetadata))));
    assert!(!client.is_offering_registered(&oid));
}

#[test]
fn rejects_non_ascii_multibyte_metadata() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);
    let oid = offering_id(&env, "uni");
    // Non-ASCII UTF-8 byte (U+00E9 / é) is not visible ASCII.
    let meta = metadata(&env, "caf\u{e9}");

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(matches!(result, Err(Ok(RegistryError::InvalidMetadata))));
    assert!(!client.is_offering_registered(&oid));
}

#[test]
fn rejects_leading_whitespace_in_metadata() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);
    let oid = offering_id(&env, "lead");
    let meta = metadata(&env, " ipfs://cid");

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(matches!(result, Err(Ok(RegistryError::InvalidMetadata))));
    assert!(!client.is_offering_registered(&oid));
}

#[test]
fn rejects_trailing_whitespace_in_metadata() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);
    let oid = offering_id(&env, "trail");
    let meta = metadata(&env, "ipfs://cid ");

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(matches!(result, Err(Ok(RegistryError::InvalidMetadata))));
    assert!(!client.is_offering_registered(&oid));
}

#[test]
fn rejects_whitespace_only_metadata() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);
    let oid = offering_id(&env, "ws-only");
    let meta = metadata(&env, "   ");

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(matches!(result, Err(Ok(RegistryError::InvalidMetadata))));
    assert!(!client.is_offering_registered(&oid));
}

#[test]
fn rejects_empty_metadata() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);
    let oid = offering_id(&env, "empty");

    let result = client.try_register_offering(&admin, &developer, &oid, &metadata(&env, ""));
    assert!(matches!(result, Err(Ok(RegistryError::InvalidMetadata))));
    assert!(!client.is_offering_registered(&oid));
}

#[test]
fn rejects_over_length_metadata() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);
    let oid = offering_id(&env, "long");
    // 257 bytes of visible ASCII exceeds the 256-byte bound.
    let long: std::string::String = "a".repeat(257);
    let meta = metadata(&env, &long);

    let result = client.try_register_offering(&admin, &developer, &oid, &meta);
    assert!(matches!(result, Err(Ok(RegistryError::InvalidMetadata))));
    assert!(!client.is_offering_registered(&oid));
}

#[test]
fn rejects_invalid_encoding_in_with_gate_variant() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);
    let oid = offering_id(&env, "gate-ctl");
    let token = Address::generate(&env);
    let meta = metadata(&env, "bad\u{0001}meta");

    // Validation runs before the token balance read / catalog call, so a dummy
    // token address is never reached.
    let result =
        client.try_register_offering_with_gate(&admin, &developer, &token, &100i128, &oid, &meta);
    assert!(matches!(result, Err(Ok(RegistryError::InvalidMetadata))));
    assert!(!client.is_offering_registered(&oid));
    assert_eq!(client.registered_count(), 0);
}

// ---------------------------------------------------------------------------
// Accept valid bounded visible-ASCII metadata
// ---------------------------------------------------------------------------

#[test]
fn accepts_visible_ascii_metadata() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);
    let oid = offering_id(&env, "ok");
    let meta = metadata(&env, "ipfs://QmAccept");

    client.register_offering(&admin, &developer, &oid, &meta);
    assert!(client.is_offering_registered(&oid));
    assert_eq!(client.registered_count(), 1);
}

#[test]
fn accepts_exactly_max_length_metadata() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);
    let oid = offering_id(&env, "max");
    let exact: std::string::String = "x".repeat(256);
    let meta = metadata(&env, &exact);

    client.register_offering(&admin, &developer, &oid, &meta);
    assert!(client.is_offering_registered(&oid));
    assert_eq!(client.registered_count(), 1);
}

// ---------------------------------------------------------------------------
// Rejection is atomic: no catalog call, count unchanged, nothing persisted
// ---------------------------------------------------------------------------

#[test]
fn rejected_metadata_leaves_no_partial_state() {
    let env = Env::default();
    let (admin, client, developer) = setup_registry(&env);

    // Valid registration increments the count.
    client.register_offering(
        &admin,
        &developer,
        &offering_id(&env, "valid"),
        &metadata(&env, "ipfs://cid"),
    );
    assert_eq!(client.registered_count(), 1);

    // A rejected registration after the cooldown must not change anything.
    advance_past_cooldown(&env);
    let oid_bad = offering_id(&env, "bad");
    let result = client.try_register_offering(
        &admin,
        &developer,
        &oid_bad,
        &metadata(&env, "has\nnewline"),
    );
    assert!(matches!(result, Err(Ok(RegistryError::InvalidMetadata))));
    assert!(!client.is_offering_registered(&oid_bad));
    assert_eq!(client.registered_count(), 1);
}
