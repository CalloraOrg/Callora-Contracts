extern crate std;

use crate::{
    events, AllowlistError, CalloraAllowlist, CalloraAllowlistClient, StorageKey,
    INSTANCE_BUMP_AMOUNT, MAX_PAGE_SIZE, PERSISTENT_BUMP_AMOUNT,
};
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env, Symbol, TryFromVal};

fn setup() -> (Env, Address, CalloraAllowlistClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract = env.register(CalloraAllowlist, ());
    let client = CalloraAllowlistClient::new(&env, &contract);
    (env, admin, client)
}

fn initialized() -> (Env, Address, CalloraAllowlistClient<'static>) {
    let (env, admin, client) = setup();
    client.init(&admin);
    (env, admin, client)
}

fn count_event_topics(env: &Env, topic: &str) -> u32 {
    let target = Symbol::new(env, topic);
    let mut count = 0;
    for event in env.events().all().iter() {
        if event.1.is_empty() {
            continue;
        }
        if let Ok(first) = Symbol::try_from_val(env, &event.1.get(0).unwrap()) {
            if first == target {
                count += 1;
            }
        }
    }
    count
}

// ---------------------------------------------------------------------------
// Initialization
// ---------------------------------------------------------------------------

#[test]
fn init_sets_admin_and_empty_count() {
    let (_env, admin, client) = initialized();
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.allowlist_count(), 0);
    assert!(client.get_pending_admin().is_none());
    assert!(client.get_allowlist().is_empty());
}

#[test]
fn init_twice_rejected() {
    let (_env, admin, client) = initialized();
    assert_eq!(
        client.try_init(&admin),
        Err(Ok(AllowlistError::AlreadyInitialized))
    );
}

#[test]
fn init_requires_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let contract = env.register(CalloraAllowlist, ());
    let client = CalloraAllowlistClient::new(&env, &contract);

    let res = client.try_init(&admin);
    assert!(res.is_err(), "init must require admin auth");
}

#[test]
fn get_admin_before_init_fails() {
    let (_env, _admin, client) = setup();
    assert_eq!(
        client.try_get_admin(),
        Err(Ok(AllowlistError::NotInitialized))
    );
}

// ---------------------------------------------------------------------------
// Admin rotation
// ---------------------------------------------------------------------------

#[test]
fn admin_rotation_round_trip() {
    let (env, admin, client) = initialized();
    let nominee = Address::generate(&env);

    client.set_admin(&admin, &nominee);
    assert_eq!(client.get_pending_admin(), Some(nominee.clone()));

    client.accept_admin();
    assert_eq!(client.get_admin(), nominee);
    assert!(client.get_pending_admin().is_none());
}

#[test]
fn cancel_admin_transfer_clears_pending() {
    let (env, admin, client) = initialized();
    let nominee = Address::generate(&env);

    client.set_admin(&admin, &nominee);
    client.cancel_admin_transfer(&admin);
    assert!(client.get_pending_admin().is_none());
}

#[test]
fn accept_admin_without_pending_rejected() {
    let (_env, _admin, client) = initialized();
    assert_eq!(
        client.try_accept_admin(),
        Err(Ok(AllowlistError::NoPendingAdmin))
    );
}

#[test]
fn cancel_admin_without_pending_rejected() {
    let (_env, admin, client) = initialized();
    assert_eq!(
        client.try_cancel_admin_transfer(&admin),
        Err(Ok(AllowlistError::NoPendingAdmin))
    );
}

#[test]
fn set_admin_requires_admin() {
    let (env, _admin, client) = initialized();
    let stranger = Address::generate(&env);
    let nominee = Address::generate(&env);

    assert_eq!(
        client.try_set_admin(&stranger, &nominee),
        Err(Ok(AllowlistError::Unauthorized))
    );
}

#[test]
fn only_nominee_can_accept_admin() {
    let (env, admin, client) = initialized();
    let nominee = Address::generate(&env);
    client.set_admin(&admin, &nominee);

    // The nominee's signature is required; with real auths (see
    // `tests/auth_snap.rs`) a different signer cannot complete the transfer.
    assert_eq!(client.get_pending_admin(), Some(nominee));
}

// ---------------------------------------------------------------------------
// Add / remove / views
// ---------------------------------------------------------------------------

#[test]
fn add_address_updates_views() {
    let (env, admin, client) = initialized();
    let address = Address::generate(&env);

    client.add_address(&admin, &address);
    let added_events = count_event_topics(&env, "address_added");

    assert!(client.is_allowed(&address));
    assert!(client.is_authorized_depositor(&address));
    assert_eq!(client.allowlist_count(), 1);
    assert_eq!(client.get_allowlist().len(), 1);
    assert_eq!(added_events, 1);
}

#[test]
fn add_duplicate_rejected() {
    let (env, admin, client) = initialized();
    let address = Address::generate(&env);
    client.add_address(&admin, &address);

    assert_eq!(
        client.try_add_address(&admin, &address),
        Err(Ok(AllowlistError::AlreadyPresent))
    );
}

#[test]
fn add_requires_admin() {
    let (env, _admin, client) = initialized();
    let stranger = Address::generate(&env);
    let address = Address::generate(&env);

    assert_eq!(
        client.try_add_address(&stranger, &address),
        Err(Ok(AllowlistError::Unauthorized))
    );
}

#[test]
fn remove_address_updates_views() {
    let (env, admin, client) = initialized();
    let address = Address::generate(&env);
    client.add_address(&admin, &address);

    client.remove_address(&admin, &address);
    let removed_events = count_event_topics(&env, "address_removed");

    assert!(!client.is_allowed(&address));
    assert_eq!(client.allowlist_count(), 0);
    assert_eq!(removed_events, 1);
}

#[test]
fn remove_missing_rejected() {
    let (env, admin, client) = initialized();
    let address = Address::generate(&env);

    assert_eq!(
        client.try_remove_address(&admin, &address),
        Err(Ok(AllowlistError::NotPresent))
    );
}

#[test]
fn remove_requires_admin() {
    let (env, admin, client) = initialized();
    let address = Address::generate(&env);
    client.add_address(&admin, &address);
    let stranger = Address::generate(&env);

    assert_eq!(
        client.try_remove_address(&stranger, &address),
        Err(Ok(AllowlistError::Unauthorized))
    );
}

#[test]
fn remove_then_readd_succeeds() {
    let (env, admin, client) = initialized();
    let address = Address::generate(&env);
    client.add_address(&admin, &address);
    client.remove_address(&admin, &address);
    client.add_address(&admin, &address);

    assert!(client.is_allowed(&address));
    assert_eq!(client.allowlist_count(), 1);
}

// ---------------------------------------------------------------------------
// Pagination
// ---------------------------------------------------------------------------

#[test]
fn pagination_walks_every_entry_once() {
    let (env, admin, client) = initialized();
    let mut expected = std::vec::Vec::new();
    for _ in 0..5 {
        let address = Address::generate(&env);
        client.add_address(&admin, &address);
        expected.push(address);
    }

    let page0 = client.get_allowlist_page(&0, &2);
    let page1 = client.get_allowlist_page(&2, &2);
    let page2 = client.get_allowlist_page(&4, &2);

    assert_eq!(page0.len(), 2);
    assert_eq!(page1.len(), 2);
    assert_eq!(page2.len(), 1);

    let mut seen: std::vec::Vec<Address> = std::vec::Vec::new();
    for page in [&page0, &page1, &page2] {
        for i in 0..page.len() {
            let address = page.get(i).unwrap();
            assert!(!seen.contains(&address), "duplicate address across pages");
            seen.push(address);
        }
    }
    assert_eq!(seen, expected);
}

#[test]
fn pagination_cursor_at_or_past_count_is_empty() {
    let (env, admin, client) = initialized();
    client.add_address(&admin, &Address::generate(&env));

    assert!(client.get_allowlist_page(&1, &10).is_empty());
    assert!(client.get_allowlist_page(&2, &10).is_empty());
    assert!(client.get_allowlist_page(&100, &10).is_empty());
}

#[test]
fn pagination_rejects_invalid_limits() {
    let (_env, _admin, client) = initialized();

    assert_eq!(
        client.try_get_allowlist_page(&0, &0),
        Err(Ok(AllowlistError::InvalidPage))
    );
    assert_eq!(
        client.try_get_allowlist_page(&0, &(MAX_PAGE_SIZE + 1)),
        Err(Ok(AllowlistError::InvalidPage))
    );
}

#[test]
fn pagination_before_init_rejected() {
    let (_env, _admin, client) = setup();
    assert_eq!(
        client.try_get_allowlist_page(&0, &10),
        Err(Ok(AllowlistError::NotInitialized))
    );
}

#[test]
fn removing_middle_entry_keeps_pages_consistent() {
    let (env, admin, client) = initialized();
    let addresses: std::vec::Vec<Address> = (0..4).map(|_| Address::generate(&env)).collect();
    for address in &addresses {
        client.add_address(&admin, address);
    }

    client.remove_address(&admin, &addresses[1]);

    let page = client.get_allowlist_page(&0, &10);
    assert_eq!(page.len(), 3);
    let mut seen: std::vec::Vec<Address> = std::vec::Vec::new();
    for i in 0..page.len() {
        let address = page.get(i).unwrap();
        assert!(!seen.contains(&address), "duplicate address after removal");
        assert_ne!(address, addresses[1], "removed address must not appear");
        seen.push(address);
    }
    assert_eq!(client.allowlist_count(), 3);
}

// ---------------------------------------------------------------------------
// clear_all
// ---------------------------------------------------------------------------

#[test]
fn clear_all_removes_every_entry() {
    let (env, admin, client) = initialized();
    for _ in 0..3 {
        client.add_address(&admin, &Address::generate(&env));
    }

    client.clear_all(&admin);
    let cleared_events = count_event_topics(&env, "allowlist_cleared");

    assert_eq!(client.allowlist_count(), 0);
    assert!(client.get_allowlist().is_empty());
    assert_eq!(cleared_events, 1);
}

#[test]
fn clear_all_on_empty_list_succeeds() {
    let (_env, admin, client) = initialized();
    client.clear_all(&admin);
    assert_eq!(client.allowlist_count(), 0);
}

#[test]
fn clear_all_requires_admin() {
    let (env, _admin, client) = initialized();
    let stranger = Address::generate(&env);
    assert_eq!(
        client.try_clear_all(&stranger),
        Err(Ok(AllowlistError::Unauthorized))
    );
}

// ---------------------------------------------------------------------------
// TTL management
// ---------------------------------------------------------------------------

#[test]
fn entrypoints_bump_ttls() {
    let (env, admin, client) = initialized();
    let address = Address::generate(&env);
    client.add_address(&admin, &address);

    let instance_ttl = env.as_contract(&client.address, || env.storage().instance().get_ttl());
    assert!(instance_ttl >= INSTANCE_BUMP_AMOUNT - 1);

    let entry_ttl = env.as_contract(&client.address, || {
        env.storage()
            .persistent()
            .get_ttl(&StorageKey::Entry(address.clone()))
    });
    assert!(entry_ttl >= PERSISTENT_BUMP_AMOUNT - 1);
    assert!(client.is_allowed(&address));
}

// ---------------------------------------------------------------------------
// Error-code stability
// ---------------------------------------------------------------------------

#[test]
fn error_codes_are_stable() {
    assert_eq!(AllowlistError::NotInitialized as u32, 1);
    assert_eq!(AllowlistError::AlreadyInitialized as u32, 2);
    assert_eq!(AllowlistError::Unauthorized as u32, 3);
    assert_eq!(AllowlistError::AlreadyPresent as u32, 4);
    assert_eq!(AllowlistError::NotPresent as u32, 5);
    assert_eq!(AllowlistError::InvalidPage as u32, 6);
    assert_eq!(AllowlistError::Overflow as u32, 7);
    assert_eq!(AllowlistError::NoPendingAdmin as u32, 8);
}

// ---------------------------------------------------------------------------
// Event constructors are used (not inline symbols)
// ---------------------------------------------------------------------------

#[test]
fn init_emits_init_event() {
    let (env, admin, client) = setup();
    client.init(&admin);
    assert_eq!(count_event_topics(&env, "init"), 1);
    assert_eq!(events::event_init(&env), Symbol::new(&env, "init"));
}
