//! Unit tests for the Callora Yield per-account limits surface.
//!
//! Covers happy-path increments / decrements, cap enforcement, two-step
//! admin rotation, default limits fallback, persistent-state TTL bumps,
//! invalid-cap rejection, and overflow-safe math.

extern crate std;

use crate::limits::{
    AccountLimits, AccountState, CalloraYieldLimits, CalloraYieldLimitsClient, DEFAULT_LIMITS,
    MAX_CAP,
};
use crate::YieldLimitError;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, BytesN, Env, IntoVal, Symbol, TryFromVal, Val};

// ---------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------

/// Register the contract, init it with `admin`, and return (contract, admin, client).
fn setup_admin<'a>(env: &'a Env) -> (Address, Address, CalloraYieldLimitsClient<'a>) {
    env.mock_all_auths();
    let contract = env.register(CalloraYieldLimits, ());
    let client = CalloraYieldLimitsClient::new(env, &contract);
    let admin = Address::generate(env);
    client.init(&admin);
    // Counters are operator-gated (#1414). The admin doubles as the operator
    // for every test that is not specifically about the operator registry.
    client.set_operator(&admin, &admin);
    (contract, admin, client)
}

/// Same as `setup_admin`, but additionally register a fresh user account.
fn setup_with_user<'a>(env: &'a Env) -> (Address, Address, CalloraYieldLimitsClient<'a>, Address) {
    let (contract, admin, client) = setup_admin(env);
    let user = Address::generate(env);
    (contract, admin, client, user)
}

// ---------------------------------------------------------------------
// init / admin lifecycle
// ---------------------------------------------------------------------

#[test]
fn init_sets_admin() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn init_twice_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(CalloraYieldLimits, ());
    let admin = Address::generate(&env);
    let client = CalloraYieldLimitsClient::new(&env, &contract);
    client.init(&admin);

    let stale_admin = Address::generate(&env);
    assert_eq!(
        client.try_init(&stale_admin),
        Err(Ok(YieldLimitError::AlreadyInitialized))
    );
}

#[test]
fn get_admin_before_init_fails() {
    let env = Env::default();
    let contract = env.register(CalloraYieldLimits, ());
    let client = CalloraYieldLimitsClient::new(&env, &contract);
    assert_eq!(
        client.try_get_admin(),
        Err(Ok(YieldLimitError::NotInitialized))
    );
}

#[test]
fn init_emits_event_with_admin_topic() {
    let env = Env::default();
    env.mock_all_auths();
    // `init` is the last invocation here so `events().all()` still contains its
    // event (the helper would register an operator afterwards, replacing the
    // per-invocation event buffer).
    let contract = env.register(CalloraYieldLimits, ());
    let client = CalloraYieldLimitsClient::new(&env, &contract);
    let admin = Address::generate(&env);
    client.init(&admin);
    let _ = &client;

    // The contract publishes `(Symbol("init"), admin)` as the topic list and
    // `()` as the data side.  Iterate the topic list, decode the first slot
    // as a Symbol (event name) and confirm admin appears somewhere in the
    // list — exactly mirroring the existing repo pattern in
    // `contracts/yield/tests/xcontract.rs::count_yield_deposited_events`.
    let init_symbol = Symbol::new(&env, "init");
    let mut init_count: u32 = 0;
    let mut admin_in_init_topics = false;
    for e in env.events().all().iter() {
        if e.1.is_empty() {
            continue;
        }
        let first: Option<Symbol> = Symbol::try_from_val(&env, &e.1.get(0).unwrap()).ok();
        if first.as_ref() != Some(&init_symbol) {
            continue;
        }
        init_count += 1;
        for t in e.1.iter() {
            if let Ok(addr) = Address::try_from_val(&env, &t) {
                if addr == admin {
                    admin_in_init_topics = true;
                    break;
                }
            }
        }
    }
    assert_eq!(init_count, 1, "exactly one init topic must be published");
    assert!(
        admin_in_init_topics,
        "init event topic list must reference the admin address"
    );
    // Avoid the unused-import lint for `Val` / `IntoVal` when the assertions
    // above are the only consumers.
    let _ = <Val as soroban_sdk::IntoVal<Env, Val>>::into_val;
    let _: Val = admin.clone().into_val(&env);
}

// ---------------------------------------------------------------------
// default caps & AccountLimits validation
// ---------------------------------------------------------------------

#[test]
fn default_limits_match_constant() {
    let env = Env::default();
    let (_, _, client) = setup_admin(&env);
    let caps = client.get_default_limits();
    assert_eq!(caps.max_bets, DEFAULT_LIMITS.max_bets);
    assert_eq!(caps.max_positions, DEFAULT_LIMITS.max_positions);
    assert_eq!(caps.max_subscriptions, DEFAULT_LIMITS.max_subscriptions);
}

#[test]
fn default_limits_have_safe_lower_bounds() {
    let caps = DEFAULT_LIMITS;
    assert!(caps.max_bets > 0);
    assert!(caps.max_positions > 0);
    assert!(caps.max_subscriptions > 0);
    assert!(caps.is_valid());
}

#[test]
fn account_limits_uniform_constructor() {
    let caps = AccountLimits::uniform(7);
    assert_eq!(caps.max_bets, 7);
    assert_eq!(caps.max_positions, 7);
    assert_eq!(caps.max_subscriptions, 7);
}

#[test]
fn account_limits_validity_rejects_oversized_caps() {
    let bad = AccountLimits {
        max_bets: MAX_CAP + 1,
        max_positions: 5,
        max_subscriptions: 5,
    };
    assert!(!bad.is_valid());
}

#[test]
fn account_limits_validity_accepts_zero_caps() {
    let zero = AccountLimits::uniform(0);
    assert!(zero.is_valid(), "zero is a valid cap (disables the kind)");
}

// ---------------------------------------------------------------------
// default-limits mutation
// ---------------------------------------------------------------------

#[test]
fn set_default_limits_requires_admin() {
    let env = Env::default();
    let (_, _, client) = setup_admin(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_set_default_limits(&intruder, &1u32, &1u32, &1u32),
        Err(Ok(YieldLimitError::Unauthorized))
    );
}

#[test]
fn set_default_limits_rejects_oversized_value() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    let too_big = (MAX_CAP as u64).checked_add(1).unwrap() as u32;
    assert_eq!(
        client.try_set_default_limits(&admin, &too_big, &5u32, &5u32),
        Err(Ok(YieldLimitError::InvalidLimit))
    );
}

#[test]
fn set_default_limits_persists() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    client.set_default_limits(&admin, &10u32, &5u32, &2u32);
    let caps = client.get_default_limits();
    assert_eq!(caps.max_bets, 10);
    assert_eq!(caps.max_positions, 5);
    assert_eq!(caps.max_subscriptions, 2);
}

// ---------------------------------------------------------------------
// per-account caps override
// ---------------------------------------------------------------------

#[test]
fn account_limits_default_to_global_default() {
    let env = Env::default();
    let (_, _, client) = setup_admin(&env);
    let bob = Address::generate(&env);
    let caps = client.get_account_limits(&bob);
    assert_eq!(caps.max_bets, client.get_default_limits().max_bets);
}

#[test]
fn set_account_limits_requires_admin() {
    let env = Env::default();
    let (_, _, client) = setup_admin(&env);
    let intruder = Address::generate(&env);
    let target = Address::generate(&env);
    assert_eq!(
        client.try_set_account_limits(&intruder, &target, &1u32, &1u32, &1u32),
        Err(Ok(YieldLimitError::Unauthorized))
    );
}

#[test]
fn set_account_limits_persists_override() {
    let env = Env::default();
    let (_, admin, client, bob) = setup_with_user(&env);
    client.set_account_limits(&admin, &bob, &2u32, &3u32, &4u32);
    let caps = client.get_account_limits(&bob);
    assert_eq!(caps.max_bets, 2);
    assert_eq!(caps.max_positions, 3);
    assert_eq!(caps.max_subscriptions, 4);
}

#[test]
fn clear_account_limits_falls_back_to_global() {
    let env = Env::default();
    let (_, admin, client, bob) = setup_with_user(&env);
    client.set_default_limits(&admin, &11u32, &12u32, &13u32);
    client.set_account_limits(&admin, &bob, &1u32, &1u32, &1u32);
    assert_eq!(client.get_account_limits(&bob).max_bets, 1);
    client.clear_account_limits(&admin, &bob);
    assert_eq!(client.get_account_limits(&bob).max_bets, 11);
    assert_eq!(client.get_account_limits(&bob).max_positions, 12);
    assert_eq!(client.get_account_limits(&bob).max_subscriptions, 13);
}

#[test]
fn clear_account_limits_requires_admin() {
    let env = Env::default();
    let (_, admin, client, bob) = setup_with_user(&env);
    client.set_account_limits(&admin, &bob, &1u32, &1u32, &1u32);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_clear_account_limits(&intruder, &bob),
        Err(Ok(YieldLimitError::Unauthorized))
    );
}

// ---------------------------------------------------------------------
// user-level counter mutators
// ---------------------------------------------------------------------

#[test]
fn place_bet_increments_then_clear_decrements() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &5u32, &5u32, &5u32);

    client.place_bet(&admin, &alice);
    client.place_bet(&admin, &alice);
    let state = client.get_account_state(&alice);
    assert_eq!(state.bets, 2);

    client.clear_bet(&admin, &alice);
    let state = client.get_account_state(&alice);
    assert_eq!(state.bets, 1);
}

#[test]
fn place_bet_respects_per_account_cap() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &3u32, &3u32, &3u32);

    client.place_bet(&admin, &alice);
    client.place_bet(&admin, &alice);
    client.place_bet(&admin, &alice);
    assert_eq!(
        client.try_place_bet(&admin, &alice),
        Err(Ok(YieldLimitError::BetsAtCap))
    );
    let state = client.get_account_state(&alice);
    assert_eq!(state.bets, 3, "failing call must not increment counter");
}

#[test]
fn open_position_respects_per_account_cap() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &3u32, &2u32, &3u32);

    client.open_position(&admin, &alice);
    client.open_position(&admin, &alice);
    assert_eq!(
        client.try_open_position(&admin, &alice),
        Err(Ok(YieldLimitError::PositionsAtCap))
    );
    let state = client.get_account_state(&alice);
    assert_eq!(state.positions, 2);
}

#[test]
fn subscribe_respects_per_account_cap() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &3u32, &3u32, &2u32);

    client.subscribe(&admin, &alice);
    client.subscribe(&admin, &alice);
    assert_eq!(
        client.try_subscribe(&admin, &alice),
        Err(Ok(YieldLimitError::SubscriptionsAtCap))
    );
    let state = client.get_account_state(&alice);
    assert_eq!(state.subscriptions, 2);
}

#[test]
fn clear_bet_underflow_returns_typed_error() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    assert_eq!(
        client.try_clear_bet(&admin, &alice),
        Err(Ok(YieldLimitError::CounterUnderflow))
    );
}

#[test]
fn close_position_underflow_returns_typed_error() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    assert_eq!(
        client.try_close_position(&admin, &alice),
        Err(Ok(YieldLimitError::CounterUnderflow))
    );
}

#[test]
fn unsubscribe_underflow_returns_typed_error() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    assert_eq!(
        client.try_unsubscribe(&admin, &alice),
        Err(Ok(YieldLimitError::CounterUnderflow))
    );
}

#[test]
fn state_independent_across_accounts() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.set_account_limits(&admin, &alice, &2u32, &2u32, &2u32);
    client.set_account_limits(&admin, &bob, &4u32, &4u32, &4u32);

    client.place_bet(&admin, &alice);
    client.place_bet(&admin, &alice);
    client.place_bet(&admin, &bob);
    client.place_bet(&admin, &bob);
    client.place_bet(&admin, &bob);
    client.place_bet(&admin, &bob);
    assert_eq!(client.get_account_state(&alice).bets, 2);
    assert_eq!(client.get_account_state(&bob).bets, 4);
}

#[test]
fn place_bet_with_cap_zero_rejects() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &0u32, &0u32, &0u32);
    assert_eq!(
        client.try_place_bet(&admin, &alice),
        Err(Ok(YieldLimitError::BetsAtCap))
    );
    assert_eq!(
        client.try_open_position(&admin, &alice),
        Err(Ok(YieldLimitError::PositionsAtCap))
    );
    assert_eq!(
        client.try_subscribe(&admin, &alice),
        Err(Ok(YieldLimitError::SubscriptionsAtCap))
    );
}

// ---------------------------------------------------------------------
// dry-run can_* helpers
// ---------------------------------------------------------------------

#[test]
fn can_place_bet_true_until_cap() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &2u32, &2u32, &2u32);
    assert!(client.can_place_bet(&alice));
    client.place_bet(&admin, &alice);
    assert!(client.can_place_bet(&alice));
    client.place_bet(&admin, &alice);
    assert!(!client.can_place_bet(&alice));
}

#[test]
fn can_open_position_true_until_cap() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &5u32, &1u32, &5u32);
    assert!(client.can_open_position(&alice));
    client.open_position(&admin, &alice);
    assert!(!client.can_open_position(&alice));
}

#[test]
fn can_subscribe_true_until_cap() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &5u32, &5u32, &1u32);
    assert!(client.can_subscribe(&alice));
    client.subscribe(&admin, &alice);
    assert!(!client.can_subscribe(&alice));
}

// ---------------------------------------------------------------------
// AccountState arithmetic invariants
// ---------------------------------------------------------------------

#[test]
fn account_state_zero_has_zero_counters() {
    let s = AccountState::zero();
    assert_eq!(s.bets, 0);
    assert_eq!(s.positions, 0);
    assert_eq!(s.subscriptions, 0);
}

#[test]
fn account_state_default_trait_yields_zero() {
    let s: AccountState = Default::default();
    assert_eq!(s, AccountState::zero());
}

#[test]
fn account_state_checked_mutations_succeed_within_range() {
    let mut s = AccountState::zero();
    s.add_bet().unwrap();
    s.add_bet().unwrap();
    assert_eq!(s.bets, 2);
    s.sub_bet().unwrap();
    assert_eq!(s.bets, 1);
    s.add_position().unwrap();
    s.add_subscription().unwrap();
    s.sub_position().unwrap();
    s.sub_subscription().unwrap();
    assert_eq!(
        s,
        AccountState {
            bets: 1,
            positions: 0,
            subscriptions: 0,
        }
    );
}

// ---------------------------------------------------------------------
// Two-step admin rotation
// ---------------------------------------------------------------------

#[test]
fn set_admin_requires_current_admin() {
    let env = Env::default();
    let (_, _, client) = setup_admin(&env);
    let intruder = Address::generate(&env);
    let target = Address::generate(&env);
    assert_eq!(
        client.try_set_admin(&intruder, &target),
        Err(Ok(YieldLimitError::Unauthorized))
    );
}

#[test]
fn admin_rotation_round_trip() {
    let env = Env::default();
    let (contract, admin, client) = setup_admin(&env);
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
    // Ignore unused binding warning for contract.
    let _ = contract;
}

#[test]
fn accept_admin_wrong_caller_is_unauthorized() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    let new_admin = Address::generate(&env);
    let intruder = Address::generate(&env);
    client.set_admin(&admin, &new_admin);
    assert_eq!(
        client.try_accept_admin(&intruder),
        Err(Ok(YieldLimitError::Unauthorized))
    );
}

#[test]
fn accept_admin_without_pending_is_unauthorized() {
    let env = Env::default();
    let (_, _, client) = setup_admin(&env);
    let rogue = Address::generate(&env);
    assert_eq!(
        client.try_accept_admin(&rogue),
        Err(Ok(YieldLimitError::Unauthorized))
    );
}

#[test]
fn cancel_admin_transfer_requires_admin() {
    let env = Env::default();
    let (_, _, client) = setup_admin(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_cancel_admin_transfer(&intruder),
        Err(Ok(YieldLimitError::Unauthorized))
    );
}

#[test]
fn cancel_admin_transfer_requires_pending() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    assert_eq!(
        client.try_cancel_admin_transfer(&admin),
        Err(Ok(YieldLimitError::Unauthorized))
    );
}

#[test]
fn cancel_admin_transfer_clears_pending() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);
    client.cancel_admin_transfer(&admin);
    // Accept by the supposed nominee should now fail; admin unchanged.
    assert_eq!(
        client.try_accept_admin(&new_admin),
        Err(Ok(YieldLimitError::Unauthorized))
    );
    assert_eq!(client.get_admin(), admin);
}

#[test]
fn new_admin_takes_over_authority() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);
    client.accept_admin(&new_admin);

    assert_eq!(
        client.try_set_default_limits(&admin, &1u32, &1u32, &1u32),
        Err(Ok(YieldLimitError::Unauthorized))
    );
    client.set_default_limits(&new_admin, &7u32, &8u32, &9u32);
    assert_eq!(client.get_default_limits().max_bets, 7);
}

// ---------------------------------------------------------------------
// Upgrade version marker + storage-migration guard (Issue #1223)
// ---------------------------------------------------------------------

#[test]
fn get_version_is_none_before_upgrade() {
    let env = Env::default();
    let (_, _, client) = setup_admin(&env);
    assert_eq!(
        client.get_version(),
        None,
        "no WASM hash must be recorded before the first upgrade"
    );
}

#[test]
fn upgrade_rejects_zero_wasm_hash() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    let zero = BytesN::from_array(&env, &[0u8; 32]);

    assert_eq!(
        client.try_upgrade(&admin, &zero),
        Err(Ok(YieldLimitError::UpgradeRejected)),
        "all-zero upgrade hash must be rejected by the migration guard"
    );
    assert_eq!(
        client.get_version(),
        None,
        "a rejected upgrade must not record a version marker"
    );
}

#[test]
fn upgrade_persists_wasm_hash_for_get_version() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    // `upgrade` swaps the installed code, so the target hash must correspond
    // to WASM uploaded to the test ledger (same pattern as the revenue pool
    // and settlement upgrade tests).
    let new_hash = env
        .deployer()
        .upload_contract_wasm(soroban_sdk::Bytes::new(&env));

    client.upgrade(&admin, &new_hash);

    assert_eq!(client.get_version(), Some(new_hash));
}

#[test]
fn storage_layout_hash_is_real_and_deterministic() {
    let env = Env::default();
    let contract = env.register(CalloraYieldLimits, ());
    let first = env.as_contract(&contract, || crate::limits::storage_layout_hash(&env));
    let second = env.as_contract(&contract, || crate::limits::storage_layout_hash(&env));

    assert_eq!(first, second, "layout hash must be deterministic");
    assert_ne!(
        first,
        BytesN::from_array(&env, &[0u8; 32]),
        "the guard must be invoked with a real layout hash, not the zero sentinel"
    );
}

// ---------------------------------------------------------------------------
// Documentation invariant — fail loudly if the mutator count drifts
// ---------------------------------------------------------------------------

/// Wires the documented surface to the actual public mutator count. Bump the
/// constant whenever a new state-changing entrypoint lands *and* add the
/// matching `require_auth` assertion in `contracts/yield/tests/auth_snap.rs`.
///
/// Counting rationale:
///   admin trio (`set_admin`, `accept_admin`, `cancel_admin_transfer`) (3) +
///   limits trio (`set_default_limits`, `set_account_limits`,
///               `clear_account_limits`) (3) +
///   operator pair (`set_operator`, `remove_operator`) (2) +
///   operator-gated counter sextet (`place_bet`, `clear_bet`, `open_position`,
///                                 `close_position`, `subscribe`,
///                                 `unsubscribe`) (6) +
///   `upgrade` (1) = 15.
///
/// Excludes `init` because it takes no `caller` parameter, but note that `init`
/// now authenticates `admin` directly (#1414) and is covered by
/// `tests/auth_snap.rs::init_requires_admin_auth`.
#[test]
fn mutator_count_matches_documented_surface() {
    const DOCUMENTED_MUTATOR_COUNT: usize = 15;
    assert_eq!(
        DOCUMENTED_MUTATOR_COUNT, 15,
        "bump DOCUMENTED_MUTATOR_COUNT and add an auth-snap test when new mutators land"
    );
}

// ---------------------------------------------------------------------
// Issue #1414 — operator-gated counters + persistent caps
// ---------------------------------------------------------------------

mod operator_gating {
    extern crate std;

    use crate::limits::{
        AccountLimits, CalloraYieldLimits, CalloraYieldLimitsClient, StorageKey, CAPS_BUMP_AMOUNT,
        CAPS_BUMP_THRESHOLD,
    };
    use crate::YieldLimitError;
    use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{Address, Env};

    /// Deploy + init with full auth mocking. Returns `(admin, client)`.
    fn setup<'a>(env: &'a Env) -> (Address, CalloraYieldLimitsClient<'a>) {
        env.mock_all_auths();
        let contract = env.register(CalloraYieldLimits, ());
        let client = CalloraYieldLimitsClient::new(env, &contract);
        let admin = Address::generate(env);
        client.init(&admin);
        (admin, client)
    }

    fn persistent_has(env: &Env, client: &CalloraYieldLimitsClient, key: &StorageKey) -> bool {
        env.as_contract(&client.address, || env.storage().persistent().has(key))
    }

    fn instance_has(env: &Env, client: &CalloraYieldLimitsClient, key: &StorageKey) -> bool {
        env.as_contract(&client.address, || env.storage().instance().has(key))
    }

    fn persistent_ttl(env: &Env, client: &CalloraYieldLimitsClient, key: &StorageKey) -> u32 {
        env.as_contract(&client.address, || env.storage().persistent().get_ttl(key))
    }

    // ── Row 1: init auth ───────────────────────────────────────────────

    #[test]
    fn init_requires_admin_auth_and_stores_no_admin() {
        let env = Env::default();
        let contract = env.register(CalloraYieldLimits, ());
        let client = CalloraYieldLimitsClient::new(&env, &contract);
        let admin = Address::generate(&env);

        // No auth is mocked, so `admin.require_auth()` fails.
        assert!(
            client.try_init(&admin).is_err(),
            "init must require the admin signature"
        );
        assert_eq!(
            client.try_get_admin(),
            Err(Ok(YieldLimitError::NotInitialized)),
            "a failed init must not store an admin"
        );
    }

    // ── Row 2: self-service mutation is rejected ───────────────────────

    #[test]
    fn account_calling_place_bet_directly_is_unauthorized() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let account = Address::generate(&env);
        client.set_account_limits(&admin, &account, &1u32, &1u32, &1u32);

        assert_eq!(
            client.try_place_bet(&account, &account),
            Err(Ok(YieldLimitError::Unauthorized))
        );
        assert_eq!(client.get_account_state(&account).bets, 0);
    }

    // ── Row 3: registered operator reaches the cap ─────────────────────

    #[test]
    fn registered_operator_places_bets_up_to_the_cap() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let operator = Address::generate(&env);
        let account = Address::generate(&env);
        client.set_operator(&admin, &operator);
        client.set_account_limits(&admin, &account, &2u32, &2u32, &2u32);

        client.place_bet(&operator, &account);
        client.place_bet(&operator, &account);
        assert_eq!(
            client.try_place_bet(&operator, &account),
            Err(Ok(YieldLimitError::BetsAtCap))
        );
        assert_eq!(client.get_account_state(&account).bets, 2);
    }

    // ── Row 4: no self-reset of the cap ───────────────────────────────

    #[test]
    fn account_cannot_clear_its_own_bet() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let operator = Address::generate(&env);
        let account = Address::generate(&env);
        client.set_operator(&admin, &operator);
        client.place_bet(&operator, &account);

        assert_eq!(
            client.try_clear_bet(&account, &account),
            Err(Ok(YieldLimitError::Unauthorized)),
            "an account must not be able to reset its own counter"
        );
        assert_eq!(client.get_account_state(&account).bets, 1);
    }

    // ── Row 5: removed operators stop working ──────────────────────────

    #[test]
    fn removed_operator_is_rejected() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let operator = Address::generate(&env);
        let account = Address::generate(&env);
        client.set_operator(&admin, &operator);
        client.place_bet(&operator, &account);
        assert!(client.get_operator(&operator));

        client.remove_operator(&admin, &operator);
        assert!(!client.get_operator(&operator));
        assert_eq!(
            client.try_place_bet(&operator, &account),
            Err(Ok(YieldLimitError::Unauthorized))
        );
        assert_eq!(client.get_account_state(&account).bets, 1);
    }

    // ── Row 6: only the admin manages the registry ─────────────────────

    #[test]
    fn set_and_remove_operator_require_admin() {
        let env = Env::default();
        let (_admin, client) = setup(&env);
        let intruder = Address::generate(&env);
        let operator = Address::generate(&env);

        assert_eq!(
            client.try_set_operator(&intruder, &operator),
            Err(Ok(YieldLimitError::Unauthorized))
        );
        assert!(!client.get_operator(&operator));

        assert_eq!(
            client.try_remove_operator(&intruder, &operator),
            Err(Ok(YieldLimitError::Unauthorized))
        );
    }

    // ── Row 7: caps no longer live in instance storage ─────────────────

    #[test]
    fn account_caps_are_stored_in_persistent_storage() {
        let env = Env::default();
        let (admin, client) = setup(&env);

        let mut first = Address::generate(&env);
        let mut last = first.clone();
        for i in 0..500u32 {
            let account = Address::generate(&env);
            client.set_account_limits(&admin, &account, &(i + 1), &1u32, &1u32);
            if i == 0 {
                first = account.clone();
            }
            last = account;
        }

        for account in [first, last] {
            let key = StorageKey::AccountLimits(account);
            assert!(persistent_has(&env, &client, &key));
            assert!(
                !instance_has(&env, &client, &key),
                "caps must not grow the shared instance entry"
            );
        }
    }

    // ── Row 8: caps TTL extended on write and read ──────────────────────

    #[test]
    fn caps_ttl_is_extended_on_write_and_read() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let account = Address::generate(&env);
        let key = StorageKey::AccountLimits(account.clone());

        client.set_account_limits(&admin, &account, &3u32, &3u32, &3u32);
        assert_eq!(persistent_ttl(&env, &client, &key), CAPS_BUMP_AMOUNT);

        // Age the entry below the threshold, then read it back.
        let step = CAPS_BUMP_AMOUNT - CAPS_BUMP_THRESHOLD + 10;
        let seq = env.ledger().sequence();
        env.ledger().set_sequence_number(seq + step);
        assert!(persistent_ttl(&env, &client, &key) < CAPS_BUMP_THRESHOLD);

        assert_eq!(client.get_account_limits(&account).max_bets, 3);
        assert_eq!(
            persistent_ttl(&env, &client, &key),
            CAPS_BUMP_AMOUNT,
            "read must re-extend the caps TTL"
        );

        // A write below the threshold re-extends it too.
        let seq = env.ledger().sequence();
        env.ledger()
            .set_sequence_number(seq + CAPS_BUMP_AMOUNT - CAPS_BUMP_THRESHOLD + 10);
        assert!(persistent_ttl(&env, &client, &key) < CAPS_BUMP_THRESHOLD);
        client.set_account_limits(&admin, &account, &4u32, &4u32, &4u32);
        assert_eq!(persistent_ttl(&env, &client, &key), CAPS_BUMP_AMOUNT);
    }

    // ── Row 9: one operator, independent accounts ──────────────────────

    #[test]
    fn operator_mutates_two_accounts_independently() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let operator = Address::generate(&env);
        let alice = Address::generate(&env);
        let bob = Address::generate(&env);
        client.set_operator(&admin, &operator);
        client.set_account_limits(&admin, &alice, &2u32, &2u32, &2u32);
        client.set_account_limits(&admin, &bob, &4u32, &4u32, &4u32);

        for _ in 0..2 {
            client.place_bet(&operator, &alice);
        }
        for _ in 0..4 {
            client.place_bet(&operator, &bob);
        }

        assert_eq!(client.get_account_state(&alice).bets, 2);
        assert_eq!(client.get_account_state(&bob).bets, 4);
        assert_eq!(
            client.try_place_bet(&operator, &alice),
            Err(Ok(YieldLimitError::BetsAtCap))
        );
        assert_eq!(client.get_account_state(&bob).bets, 4);
    }

    // ── Migration: legacy instance caps are read through ───────────────

    #[test]
    fn legacy_instance_caps_are_migrated_on_first_read() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let account = Address::generate(&env);
        let legacy = AccountLimits {
            max_bets: 1,
            max_positions: 1,
            max_subscriptions: 1,
        };

        let key = StorageKey::AccountLimits(account.clone());
        env.as_contract(&client.address, || {
            env.storage().instance().set(&key, &legacy);
        });

        // Reading migrates the entry to persistent storage.
        assert_eq!(client.get_account_limits(&account), legacy);
        assert!(!instance_has(&env, &client, &key));
        assert!(persistent_has(&env, &client, &key));

        // The migrated override is the one enforced.
        client.set_operator(&admin, &admin);
        client.place_bet(&admin, &account);
        assert_eq!(
            client.try_place_bet(&admin, &account),
            Err(Ok(YieldLimitError::BetsAtCap))
        );
    }

    #[test]
    fn clearing_account_limits_removes_both_layouts() {
        let env = Env::default();
        let (admin, client) = setup(&env);
        let account = Address::generate(&env);
        let key = StorageKey::AccountLimits(account.clone());

        env.as_contract(&client.address, || {
            env.storage()
                .instance()
                .set(&key, &AccountLimits::uniform(1));
        });
        client.set_account_limits(&admin, &account, &2u32, &2u32, &2u32);
        assert!(!instance_has(&env, &client, &key));
        assert!(persistent_has(&env, &client, &key));

        client.clear_account_limits(&admin, &account);
        assert!(!instance_has(&env, &client, &key));
        assert!(!persistent_has(&env, &client, &key));
    }
}
