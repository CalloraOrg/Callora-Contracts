extern crate std;

use callora_registry::{CalloraRegistry, CalloraRegistryClient, RegistryError};
use soroban_sdk::testutils::{Address as _, Events, Ledger, LedgerInfo};
use soroban_sdk::{
    contract, contractimpl, Address, Env, IntoVal, String, Symbol, TryFromVal, Val, Vec,
};

pub mod recording_catalog {
    use super::*;

    #[contract]
    pub struct RecordingCatalog;

    #[contractimpl]
    impl RecordingCatalog {
        pub fn put_offering(env: Env, registry: Address, offering_id: String, metadata: String) {
            registry.require_auth();
            if metadata == String::from_str(&env, "ipfs://reject") {
                panic!("catalog rejected metadata");
            }
            env.storage().instance().set(&offering_id, &metadata);
        }

        pub fn metadata_of(env: Env, offering_id: String) -> Option<String> {
            env.storage().instance().get(&offering_id)
        }
    }
}

use recording_catalog::{RecordingCatalog, RecordingCatalogClient};

struct Setup<'a> {
    env: Env,
    admin: Address,
    developer: Address,
    registry: CalloraRegistryClient<'a>,
    catalog: RecordingCatalogClient<'a>,
    offering_id: String,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let catalog_id = env.register(RecordingCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());
    let registry = CalloraRegistryClient::new(&env, &registry_id);
    let catalog = RecordingCatalogClient::new(&env, &catalog_id);
    registry.init(&admin, &catalog_id);

    let offering_id = String::from_str(&env, "offering-lifecycle");
    registry.register_offering(&admin, &developer, &offering_id, &meta(&env, "ipfs://v1"));

    Setup {
        env,
        admin,
        developer,
        registry,
        catalog,
        offering_id,
    }
}

fn meta(env: &Env, value: &str) -> String {
    String::from_str(env, value)
}

fn pass_cooldown(env: &Env) {
    let current = env.ledger().get();
    env.ledger().set(LedgerInfo {
        timestamp: current.timestamp + callora_registry::admin::COOLDOWN_SECONDS,
        ..current
    });
}

fn last_registry_event(s: &Setup) -> (Vec<Val>, Val) {
    let (contract, topics, data) = s
        .env
        .events()
        .all()
        .iter()
        .filter(|(contract, _, _)| *contract == s.registry.address)
        .last()
        .expect("registry event");
    assert_eq!(contract, s.registry.address);
    (topics, data)
}

fn topics(s: &Setup, name: &str) -> Vec<Val> {
    (Symbol::new(&s.env, name), s.offering_id.clone()).into_val(&s.env)
}

#[test]
fn update_offering_replaces_metadata_and_forwards_to_catalog() {
    let s = setup();
    pass_cooldown(&s.env);
    let new_metadata = meta(&s.env, "ipfs://v2");

    s.registry
        .update_offering(&s.developer, &s.offering_id, &new_metadata);

    let (event_topics, data) = last_registry_event(&s);
    assert_eq!(event_topics, topics(&s, "offering_updated"));
    assert_eq!(
        <(String, String)>::try_from_val(&s.env, &data).unwrap(),
        (meta(&s.env, "ipfs://v1"), new_metadata.clone())
    );
    assert_eq!(
        s.registry.get_offering(&s.offering_id).metadata,
        new_metadata
    );
    assert_eq!(s.catalog.metadata_of(&s.offering_id), Some(new_metadata));
    assert_eq!(s.registry.registered_count(), 1);
}

#[test]
fn update_offering_rejects_non_owner_and_admin() {
    let s = setup();
    pass_cooldown(&s.env);
    let stranger = Address::generate(&s.env);

    for caller in [stranger, s.admin.clone()] {
        let result =
            s.registry
                .try_update_offering(&caller, &s.offering_id, &meta(&s.env, "ipfs://v2"));
        assert_eq!(result, Err(Ok(RegistryError::Unauthorized)));
    }
    assert_eq!(
        s.registry.get_offering(&s.offering_id).metadata,
        meta(&s.env, "ipfs://v1")
    );
}

#[test]
fn update_offering_unknown_id_is_not_found() {
    let s = setup();
    let result = s.registry.try_update_offering(
        &s.developer,
        &meta(&s.env, "missing"),
        &meta(&s.env, "ipfs://v2"),
    );
    assert_eq!(result, Err(Ok(RegistryError::OfferingNotFound)));
}

#[test]
fn update_offering_rejects_invalid_metadata() {
    let s = setup();
    pass_cooldown(&s.env);

    for bad in ["", " leading", "trailing ", "line\nbreak"] {
        let result =
            s.registry
                .try_update_offering(&s.developer, &s.offering_id, &meta(&s.env, bad));
        assert_eq!(result, Err(Ok(RegistryError::InvalidMetadata)), "{bad:?}");
    }
    let too_long = meta(
        &s.env,
        &"a".repeat(callora_registry::MAX_METADATA_LEN as usize + 1),
    );
    assert_eq!(
        s.registry
            .try_update_offering(&s.developer, &s.offering_id, &too_long),
        Err(Ok(RegistryError::InvalidMetadata))
    );
    assert_eq!(
        s.registry.get_offering(&s.offering_id).metadata,
        meta(&s.env, "ipfs://v1")
    );
}

#[test]
fn catalog_failure_on_update_leaves_state_unchanged() {
    let s = setup();
    pass_cooldown(&s.env);

    let result = s.registry.try_update_offering(
        &s.developer,
        &s.offering_id,
        &meta(&s.env, "ipfs://reject"),
    );

    assert!(result.is_err());
    assert_eq!(
        s.registry.get_offering(&s.offering_id).metadata,
        meta(&s.env, "ipfs://v1")
    );
    assert_eq!(
        s.catalog.metadata_of(&s.offering_id),
        Some(meta(&s.env, "ipfs://v1"))
    );
    s.registry
        .update_offering(&s.developer, &s.offering_id, &meta(&s.env, "ipfs://v2"));
}

#[test]
fn developer_can_deprecate_and_offering_still_resolves() {
    let s = setup();
    assert!(!s.registry.is_offering_deprecated(&s.offering_id));

    s.registry.deprecate_offering(&s.developer, &s.offering_id);

    let (event_topics, data) = last_registry_event(&s);
    assert_eq!(event_topics, topics(&s, "offering_deprecated"));
    assert_eq!(Address::try_from_val(&s.env, &data).unwrap(), s.developer);
    assert!(s.registry.is_offering_deprecated(&s.offering_id));
    assert!(s.registry.is_offering_registered(&s.offering_id));
    let record = s.registry.get_offering(&s.offering_id);
    assert_eq!(record.developer, s.developer);
    assert_eq!(record.metadata, meta(&s.env, "ipfs://v1"));
}

#[test]
fn admin_can_deprecate_but_stranger_cannot() {
    let s = setup();
    let stranger = Address::generate(&s.env);

    assert_eq!(
        s.registry.try_deprecate_offering(&stranger, &s.offering_id),
        Err(Ok(RegistryError::Unauthorized))
    );
    assert!(!s.registry.is_offering_deprecated(&s.offering_id));

    s.registry.deprecate_offering(&s.admin, &s.offering_id);
    assert!(s.registry.is_offering_deprecated(&s.offering_id));
}

#[test]
fn deprecating_twice_is_rejected() {
    let s = setup();
    s.registry.deprecate_offering(&s.developer, &s.offering_id);

    assert_eq!(
        s.registry.try_deprecate_offering(&s.admin, &s.offering_id),
        Err(Ok(RegistryError::OfferingAlreadyDeprecated))
    );
}

#[test]
fn deprecate_unknown_offering_is_not_found() {
    let s = setup();
    assert_eq!(
        s.registry
            .try_deprecate_offering(&s.developer, &meta(&s.env, "missing")),
        Err(Ok(RegistryError::OfferingNotFound))
    );
    assert!(!s.registry.is_offering_deprecated(&meta(&s.env, "missing")));
}

#[test]
fn deprecated_offering_cannot_be_updated_or_transferred() {
    let s = setup();
    let new_developer = Address::generate(&s.env);
    s.registry.deprecate_offering(&s.developer, &s.offering_id);
    pass_cooldown(&s.env);

    assert_eq!(
        s.registry
            .try_update_offering(&s.developer, &s.offering_id, &meta(&s.env, "ipfs://v2")),
        Err(Ok(RegistryError::OfferingDeprecated))
    );
    assert_eq!(
        s.registry
            .try_propose_offering_transfer(&s.developer, &s.offering_id, &new_developer),
        Err(Ok(RegistryError::OfferingDeprecated))
    );
}

#[test]
fn deprecation_drops_a_pending_transfer() {
    let s = setup();
    let new_developer = Address::generate(&s.env);
    pass_cooldown(&s.env);
    s.registry
        .propose_offering_transfer(&s.developer, &s.offering_id, &new_developer);

    s.registry.deprecate_offering(&s.admin, &s.offering_id);

    assert_eq!(
        s.registry
            .try_accept_offering_transfer(&new_developer, &s.offering_id),
        Err(Ok(RegistryError::NoPendingTransfer))
    );
    assert_eq!(
        s.registry.get_offering(&s.offering_id).developer,
        s.developer
    );
}

#[test]
fn transfer_changes_developer_only_after_acceptance() {
    let s = setup();
    let new_developer = Address::generate(&s.env);
    pass_cooldown(&s.env);

    s.registry
        .propose_offering_transfer(&s.developer, &s.offering_id, &new_developer);
    assert_eq!(
        s.registry.get_offering(&s.offering_id).developer,
        s.developer
    );

    s.registry
        .accept_offering_transfer(&new_developer, &s.offering_id);

    let (event_topics, data) = last_registry_event(&s);
    assert_eq!(event_topics, topics(&s, "offering_transferred"));
    assert_eq!(
        <(Address, Address)>::try_from_val(&s.env, &data).unwrap(),
        (s.developer.clone(), new_developer.clone())
    );
    assert_eq!(
        s.registry.get_offering(&s.offering_id).developer,
        new_developer
    );
    assert_eq!(s.registry.registered_count(), 1);

    assert_eq!(
        s.registry
            .try_update_offering(&s.developer, &s.offering_id, &meta(&s.env, "ipfs://old")),
        Err(Ok(RegistryError::Unauthorized))
    );
    s.registry
        .update_offering(&new_developer, &s.offering_id, &meta(&s.env, "ipfs://v2"));
    assert_eq!(
        s.registry
            .try_accept_offering_transfer(&new_developer, &s.offering_id),
        Err(Ok(RegistryError::NoPendingTransfer))
    );
}

#[test]
fn transfer_to_self_is_rejected() {
    let s = setup();
    pass_cooldown(&s.env);

    assert_eq!(
        s.registry
            .try_propose_offering_transfer(&s.developer, &s.offering_id, &s.developer),
        Err(Ok(RegistryError::InvalidTransferTarget))
    );
}

#[test]
fn non_owner_cannot_propose_transfer() {
    let s = setup();
    let stranger = Address::generate(&s.env);

    for caller in [stranger.clone(), s.admin.clone()] {
        assert_eq!(
            s.registry
                .try_propose_offering_transfer(&caller, &s.offering_id, &stranger),
            Err(Ok(RegistryError::Unauthorized))
        );
    }
}

#[test]
fn accept_by_wrong_address_is_rejected() {
    let s = setup();
    let new_developer = Address::generate(&s.env);
    let intruder = Address::generate(&s.env);
    pass_cooldown(&s.env);
    s.registry
        .propose_offering_transfer(&s.developer, &s.offering_id, &new_developer);

    for caller in [intruder, s.admin.clone(), s.developer.clone()] {
        assert_eq!(
            s.registry
                .try_accept_offering_transfer(&caller, &s.offering_id),
            Err(Ok(RegistryError::Unauthorized))
        );
    }
    assert_eq!(
        s.registry.get_offering(&s.offering_id).developer,
        s.developer
    );
    s.registry
        .accept_offering_transfer(&new_developer, &s.offering_id);
}

#[test]
fn accept_without_proposal_is_rejected() {
    let s = setup();
    let new_developer = Address::generate(&s.env);

    assert_eq!(
        s.registry
            .try_accept_offering_transfer(&new_developer, &s.offering_id),
        Err(Ok(RegistryError::NoPendingTransfer))
    );
}

#[test]
fn stale_proposal_cannot_be_accepted_after_replacement() {
    let s = setup();
    let first = Address::generate(&s.env);
    let second = Address::generate(&s.env);
    pass_cooldown(&s.env);
    s.registry
        .propose_offering_transfer(&s.developer, &s.offering_id, &first);
    pass_cooldown(&s.env);
    s.registry
        .propose_offering_transfer(&s.developer, &s.offering_id, &second);

    assert_eq!(
        s.registry
            .try_accept_offering_transfer(&first, &s.offering_id),
        Err(Ok(RegistryError::Unauthorized))
    );
    s.registry.accept_offering_transfer(&second, &s.offering_id);
    assert_eq!(s.registry.get_offering(&s.offering_id).developer, second);
}

#[test]
fn cancel_removes_the_pending_transfer() {
    let s = setup();
    let new_developer = Address::generate(&s.env);
    pass_cooldown(&s.env);
    s.registry
        .propose_offering_transfer(&s.developer, &s.offering_id, &new_developer);

    assert_eq!(
        s.registry
            .try_cancel_offering_transfer(&new_developer, &s.offering_id),
        Err(Ok(RegistryError::Unauthorized))
    );
    s.registry
        .cancel_offering_transfer(&s.developer, &s.offering_id);

    assert_eq!(
        s.registry
            .try_accept_offering_transfer(&new_developer, &s.offering_id),
        Err(Ok(RegistryError::NoPendingTransfer))
    );
    assert_eq!(
        s.registry
            .try_cancel_offering_transfer(&s.developer, &s.offering_id),
        Err(Ok(RegistryError::NoPendingTransfer))
    );
    assert_eq!(
        s.registry.get_offering(&s.offering_id).developer,
        s.developer
    );
}

#[test]
fn lifecycle_entrypoints_require_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let developer = Address::generate(&env);
    let registry = CalloraRegistryClient::new(&env, &env.register(CalloraRegistry, ()));
    let id = meta(&env, "offering-x");

    assert_eq!(
        registry.try_update_offering(&developer, &id, &meta(&env, "ipfs://v2")),
        Err(Ok(RegistryError::NotInitialized))
    );
    assert_eq!(
        registry.try_deprecate_offering(&developer, &id),
        Err(Ok(RegistryError::NotInitialized))
    );
    assert_eq!(
        registry.try_is_offering_deprecated(&id),
        Err(Ok(RegistryError::NotInitialized))
    );
}

#[test]
fn lifecycle_entrypoints_reject_invalid_offering_ids() {
    let s = setup();
    let bad_id = meta(&s.env, "Bad Id");

    assert_eq!(
        s.registry
            .try_update_offering(&s.developer, &bad_id, &meta(&s.env, "ipfs://v2")),
        Err(Ok(RegistryError::InvalidOfferingId))
    );
    assert_eq!(
        s.registry.try_deprecate_offering(&s.developer, &bad_id),
        Err(Ok(RegistryError::InvalidOfferingId))
    );
    assert_eq!(
        s.registry.try_is_offering_deprecated(&bad_id),
        Err(Ok(RegistryError::InvalidOfferingId))
    );
}
