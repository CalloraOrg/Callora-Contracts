//! Unit tests for the Callora Yield per-account limits surface.
//!
//! Covers happy-path increments / decrements, cap enforcement, two-step
//! admin rotation, default limits fallback, persistent-state TTL bumps,
//! invalid-cap rejection, and overflow-safe math.

extern crate std;

use crate::limits::{
    AccountLimits, AccountState, CalloraYieldLimits, CalloraYieldLimitsClient, WindowLimits,
    WindowState, DEFAULT_LIMITS, DEFAULT_WINDOW_LIMITS, MAX_CAP, MAX_WINDOW_LEDGERS,
};
use crate::YieldLimitError;
use proptest::prelude::*;
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{Address, Env, IntoVal, Symbol, TryFromVal, Val};

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
    let (_contract, admin, _) = setup_admin(&env);

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

    client.place_bet(&alice);
    client.place_bet(&alice);
    let state = client.get_account_state(&alice);
    assert_eq!(state.bets, 2);

    client.clear_bet(&alice);
    let state = client.get_account_state(&alice);
    assert_eq!(state.bets, 1);
}

#[test]
fn place_bet_respects_per_account_cap() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &3u32, &3u32, &3u32);

    client.place_bet(&alice);
    client.place_bet(&alice);
    client.place_bet(&alice);
    assert_eq!(
        client.try_place_bet(&alice),
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

    client.open_position(&alice);
    client.open_position(&alice);
    assert_eq!(
        client.try_open_position(&alice),
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

    client.subscribe(&alice);
    client.subscribe(&alice);
    assert_eq!(
        client.try_subscribe(&alice),
        Err(Ok(YieldLimitError::SubscriptionsAtCap))
    );
    let state = client.get_account_state(&alice);
    assert_eq!(state.subscriptions, 2);
}

#[test]
fn clear_bet_underflow_returns_typed_error() {
    let env = Env::default();
    let (_, _, client, alice) = setup_with_user(&env);
    assert_eq!(
        client.try_clear_bet(&alice),
        Err(Ok(YieldLimitError::CounterUnderflow))
    );
}

#[test]
fn close_position_underflow_returns_typed_error() {
    let env = Env::default();
    let (_, _, client, alice) = setup_with_user(&env);
    assert_eq!(
        client.try_close_position(&alice),
        Err(Ok(YieldLimitError::CounterUnderflow))
    );
}

#[test]
fn unsubscribe_underflow_returns_typed_error() {
    let env = Env::default();
    let (_, _, client, alice) = setup_with_user(&env);
    assert_eq!(
        client.try_unsubscribe(&alice),
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

    client.place_bet(&alice);
    client.place_bet(&alice);
    client.place_bet(&bob);
    client.place_bet(&bob);
    client.place_bet(&bob);
    client.place_bet(&bob);
    assert_eq!(client.get_account_state(&alice).bets, 2);
    assert_eq!(client.get_account_state(&bob).bets, 4);
}

#[test]
fn place_bet_with_cap_zero_rejects() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &0u32, &0u32, &0u32);
    assert_eq!(
        client.try_place_bet(&alice),
        Err(Ok(YieldLimitError::BetsAtCap))
    );
    assert_eq!(
        client.try_open_position(&alice),
        Err(Ok(YieldLimitError::PositionsAtCap))
    );
    assert_eq!(
        client.try_subscribe(&alice),
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
    client.place_bet(&alice);
    assert!(client.can_place_bet(&alice));
    client.place_bet(&alice);
    assert!(!client.can_place_bet(&alice));
}

#[test]
fn can_open_position_true_until_cap() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &5u32, &1u32, &5u32);
    assert!(client.can_open_position(&alice));
    client.open_position(&alice);
    assert!(!client.can_open_position(&alice));
}

#[test]
fn can_subscribe_true_until_cap() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &5u32, &5u32, &1u32);
    assert!(client.can_subscribe(&alice));
    client.subscribe(&alice);
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
///   window pair (`set_window_ledgers`, `set_window_limits`) (2) +
///   user counter sextet (`place_bet`, `clear_bet`, `open_position`,
///                        `close_position`, `subscribe`, `unsubscribe`) (6) +
///   `upgrade` (1) = 15.
///
/// Excludes `init` because it is the deployment boundary (no `caller`
/// parameter) and is treated separately via
/// `init_twice_fails` / `get_admin_before_init_fails` plus the constructor
/// exemption documented at the top of `tests/auth_snap.rs`.
#[test]
fn mutator_count_matches_documented_surface() {
    const DOCUMENTED_MUTATOR_COUNT: usize = 15;
    assert_eq!(
        DOCUMENTED_MUTATOR_COUNT, 15,
        "bump DOCUMENTED_MUTATOR_COUNT and add an auth-snap test when new mutators land"
    );
}

// ---------------------------------------------------------------------
// Rolling windows (Issue #1522)
// ---------------------------------------------------------------------

/// Advance the mocked ledger sequence by `delta` ledgers.
fn advance_ledger(env: &Env, delta: u32) {
    env.ledger()
        .with_mut(|li| li.sequence_number = li.sequence_number.saturating_add(delta));
}

#[test]
fn window_disabled_by_default_preserves_old_behaviour() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &2u32, &2u32, &2u32);

    assert_eq!(client.get_window_ledgers(), 0);
    assert_eq!(
        client.get_window_limits(),
        WindowLimits::uniform(MAX_CAP),
        "per-window caps default to MAX_CAP (permissive)"
    );

    advance_ledger(&env, 50);
    client.place_bet(&alice);
    client.place_bet(&alice);
    assert_eq!(
        client.try_place_bet(&alice),
        Err(Ok(YieldLimitError::BetsAtCap))
    );

    let window_state = client.get_window_state(&alice);
    assert_eq!(
        window_state,
        WindowState::zero(),
        "disabled windows must not allocate window state"
    );
}

#[test]
fn window_default_limits_match_constant() {
    assert_eq!(DEFAULT_WINDOW_LIMITS, WindowLimits::uniform(MAX_CAP));
    assert!(DEFAULT_WINDOW_LIMITS.is_valid());
    assert_eq!(WindowLimits::uniform(7).max_subscriptions, 7);
    assert!(!WindowLimits {
        max_bets: MAX_CAP + 1,
        ..WindowLimits::uniform(1)
    }
    .is_valid());
}

#[test]
fn set_window_ledgers_requires_admin() {
    let env = Env::default();
    let (_, _, client) = setup_admin(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_set_window_ledgers(&intruder, &10u32),
        Err(Ok(YieldLimitError::Unauthorized))
    );
}

#[test]
fn set_window_ledgers_rejects_out_of_bounds() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    assert_eq!(
        client.try_set_window_ledgers(&admin, &(MAX_WINDOW_LEDGERS + 1)),
        Err(Ok(YieldLimitError::InvalidLimit))
    );
    client.set_window_ledgers(&admin, &MAX_WINDOW_LEDGERS);
    assert_eq!(client.get_window_ledgers(), MAX_WINDOW_LEDGERS);
    client.set_window_ledgers(&admin, &1u32);
    assert_eq!(client.get_window_ledgers(), 1);
    // Zero explicitly disables windows and is always valid.
    client.set_window_ledgers(&admin, &0u32);
    assert_eq!(client.get_window_ledgers(), 0);
}

#[test]
fn set_window_limits_requires_admin_and_validates() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    let intruder = Address::generate(&env);
    assert_eq!(
        client.try_set_window_limits(&intruder, &1u32, &1u32, &1u32),
        Err(Ok(YieldLimitError::Unauthorized))
    );
    assert_eq!(
        client.try_set_window_limits(&admin, &(MAX_CAP + 1), &1u32, &1u32),
        Err(Ok(YieldLimitError::InvalidLimit))
    );
    client.set_window_limits(&admin, &3u32, &4u32, &5u32);
    let caps = client.get_window_limits();
    assert_eq!(
        caps,
        WindowLimits {
            max_bets: 3,
            max_positions: 4,
            max_subscriptions: 5,
        }
    );
}

#[test]
fn window_cap_blocks_until_the_window_rolls_over() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &100u32, &100u32, &100u32);
    client.set_window_ledgers(&admin, &10u32);
    client.set_window_limits(&admin, &2u32, &2u32, &2u32);

    let start = env.ledger().sequence();
    client.place_bet(&alice);
    client.place_bet(&alice);
    assert_eq!(
        client.try_place_bet(&alice),
        Err(Ok(YieldLimitError::BetsWindowCap))
    );
    // The failing call must not consume window capacity or concurrent state.
    let state = client.get_account_state(&alice);
    assert_eq!(state.bets, 2);
    assert_eq!(client.get_window_state(&alice).bets_opened, 2);

    // One ledger before the boundary the window is still exhausted.
    advance_ledger(&env, 9);
    assert_eq!(
        client.try_place_bet(&alice),
        Err(Ok(YieldLimitError::BetsWindowCap))
    );

    // Exactly at window_start + W the window rolls and capacity returns.
    advance_ledger(&env, 1);
    client.place_bet(&alice);
    let window_state = client.get_window_state(&alice);
    assert_eq!(window_state.bets_opened, 1);
    assert_eq!(window_state.window_start, start + 10);
    assert_eq!(client.get_account_state(&alice).bets, 3);
}

#[test]
fn window_roll_skips_multiple_windows() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &100u32, &100u32, &100u32);
    client.set_window_ledgers(&admin, &5u32);
    client.set_window_limits(&admin, &1u32, &1u32, &1u32);

    client.place_bet(&alice);
    assert_eq!(
        client.try_place_bet(&alice),
        Err(Ok(YieldLimitError::BetsWindowCap))
    );

    // Jump three full windows at once; a single lazy roll is sufficient.
    let before = env.ledger().sequence();
    advance_ledger(&env, 17);
    client.place_bet(&alice);
    let window_state = client.get_window_state(&alice);
    assert_eq!(window_state.window_start, before + 17);
    assert_eq!(window_state.bets_opened, 1);
}

#[test]
fn decrements_do_not_refund_window_counts() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &100u32, &100u32, &100u32);
    client.set_window_ledgers(&admin, &20u32);
    client.set_window_limits(&admin, &2u32, &2u32, &2u32);

    client.place_bet(&alice);
    client.place_bet(&alice);
    client.clear_bet(&alice);
    client.clear_bet(&alice);
    assert_eq!(client.get_account_state(&alice).bets, 0);
    assert_eq!(client.get_window_state(&alice).bets_opened, 2);

    assert_eq!(
        client.try_place_bet(&alice),
        Err(Ok(YieldLimitError::BetsWindowCap))
    );
    advance_ledger(&env, 20);
    client.place_bet(&alice);
    assert_eq!(client.get_window_state(&alice).bets_opened, 1);
}

#[test]
fn window_caps_apply_to_positions_and_subscriptions() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &100u32, &100u32, &100u32);
    client.set_window_ledgers(&admin, &10u32);
    client.set_window_limits(&admin, &1u32, &1u32, &1u32);

    client.open_position(&alice);
    assert_eq!(
        client.try_open_position(&alice),
        Err(Ok(YieldLimitError::PositionsWindowCap))
    );
    client.subscribe(&alice);
    assert_eq!(
        client.try_subscribe(&alice),
        Err(Ok(YieldLimitError::SubscriptionsWindowCap))
    );

    let window_state = client.get_window_state(&alice);
    assert_eq!(window_state.positions_opened, 1);
    assert_eq!(window_state.subscriptions_added, 1);
}

#[test]
fn windows_are_independent_per_account() {
    let env = Env::default();
    let (_, admin, client) = setup_admin(&env);
    let alice = Address::generate(&env);
    let bob = Address::generate(&env);
    client.set_window_ledgers(&admin, &10u32);
    client.set_window_limits(&admin, &1u32, &1u32, &1u32);

    client.place_bet(&alice);
    assert_eq!(
        client.try_place_bet(&alice),
        Err(Ok(YieldLimitError::BetsWindowCap))
    );
    // Bob has his own window and is unaffected by Alice's exhaustion.
    client.place_bet(&bob);
    assert_eq!(client.get_window_state(&bob).bets_opened, 1);
}

#[test]
fn can_views_account_for_window_caps_and_expiry() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &100u32, &100u32, &100u32);
    client.set_window_ledgers(&admin, &10u32);
    client.set_window_limits(&admin, &1u32, &1u32, &1u32);

    assert!(client.can_place_bet(&alice));
    client.place_bet(&alice);
    client.open_position(&alice);
    client.subscribe(&alice);
    assert!(
        !client.can_place_bet(&alice),
        "window exhaustion must be visible through the dry-run view"
    );
    assert!(!client.can_open_position(&alice));
    assert!(!client.can_subscribe(&alice));

    // Views must not mutate storage: the stored window is still at start.
    let stored = client.get_window_state(&alice);

    advance_ledger(&env, 10);
    assert!(client.can_place_bet(&alice));
    assert!(client.can_open_position(&alice));
    assert!(client.can_subscribe(&alice));
    assert_eq!(
        client.get_window_state(&alice),
        stored,
        "get_window_state is a pure read; rolling happens on mutation"
    );
}

#[test]
fn can_views_still_respect_concurrent_caps_with_windows_enabled() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &1u32, &1u32, &1u32);
    client.set_window_ledgers(&admin, &10u32);
    client.set_window_limits(&admin, &100u32, &100u32, &100u32);

    client.place_bet(&alice);
    assert!(!client.can_place_bet(&alice));
    advance_ledger(&env, 100);
    assert!(
        !client.can_place_bet(&alice),
        "window roll must not bypass the concurrent cap"
    );
}

#[test]
fn window_sequence_overflow_is_safe() {
    // `window_start + window_ledgers` must saturate rather than wrap: a
    // window near `u32::MAX` must not appear expired immediately.
    let state = WindowState::started_at(u32::MAX - 5);
    // The end of the window saturates at `u32::MAX`; the window is still open
    // on the ledgers before that point rather than wrapping to a tiny value.
    assert!(!state.is_expired(u32::MAX - 4, 10));
    // At the saturated boundary the window rolls instead of wrapping around
    // and appearing freshly started.
    assert!(state.is_expired(u32::MAX, 10));
    assert!(!WindowState::started_at(u32::MAX - 1).is_expired(u32::MAX - 1, 10));
}

/// Return `true` when the most recent invocation emitted `topic`.
fn last_invocation_emitted(env: &Env, topic: &str) -> bool {
    let wanted = Symbol::new(env, topic);
    for event in env.events().all().iter() {
        if event.1.is_empty() {
            continue;
        }
        if let Ok(first) = Symbol::try_from_val(env, &event.1.get(0).unwrap()) {
            if first == wanted {
                return true;
            }
        }
    }
    false
}

#[test]
fn window_events_are_emitted() {
    let env = Env::default();
    let (_, admin, client, alice) = setup_with_user(&env);
    client.set_account_limits(&admin, &alice, &100u32, &100u32, &100u32);

    client.set_window_ledgers(&admin, &10u32);
    assert!(last_invocation_emitted(&env, "window_ledgers_set"));

    client.set_window_limits(&admin, &1u32, &1u32, &1u32);
    assert!(last_invocation_emitted(&env, "window_limits_set"));

    client.place_bet(&alice);
    advance_ledger(&env, 10);
    client.place_bet(&alice);
    assert!(last_invocation_emitted(&env, "window_rolled"));
}

// ---------------------------------------------------------------------
// Proptest: random operation sequences against a simple model
// ---------------------------------------------------------------------

/// A deliberately simple model of the concurrent and window caps. It mirrors
/// the contract's documented semantics rather than its implementation: caps
/// gate opens, decrements never refund window counts, and a window rolls when
/// `sequence >= window_start + window_ledgers`.
#[derive(Default)]
struct LimitsModel {
    bets: u32,
    positions: u32,
    subscriptions: u32,
    window_start: Option<u32>,
    bets_opened: u32,
    positions_opened: u32,
    subscriptions_added: u32,
    sequence: u32,
}

impl LimitsModel {
    const CONCURRENT_CAP: u32 = 6;
    const WINDOW_CAP: u32 = 3;
    const WINDOW_LEDGERS: u32 = 7;

    /// Effective window counters at the current sequence, rolling lazily
    /// without committing anything to the model yet. The contract only
    /// persists a roll when the mutation succeeds, so failed opens must leave
    /// the model's window untouched too.
    fn effective_window(&self) -> (u32, u32, u32, u32) {
        match self.window_start {
            Some(start) if self.sequence < start.saturating_add(Self::WINDOW_LEDGERS) => (
                start,
                self.bets_opened,
                self.positions_opened,
                self.subscriptions_added,
            ),
            _ => (self.sequence, 0, 0, 0),
        }
    }

    fn open(&mut self, kind: u8) -> Result<(), YieldLimitError> {
        let (concurrent, cap, err_concurrent) = match kind {
            0 => (self.bets, Self::CONCURRENT_CAP, YieldLimitError::BetsAtCap),
            1 => (
                self.positions,
                Self::CONCURRENT_CAP,
                YieldLimitError::PositionsAtCap,
            ),
            _ => (
                self.subscriptions,
                Self::CONCURRENT_CAP,
                YieldLimitError::SubscriptionsAtCap,
            ),
        };
        // Concurrent caps are checked before the window, mirroring the
        // contract: a concurrent-cap rejection must not roll the window.
        if concurrent >= cap {
            return Err(err_concurrent);
        }

        let (window_start, bets_opened, positions_opened, subscriptions_added) =
            self.effective_window();
        let (window, err_window) = match kind {
            0 => (bets_opened, YieldLimitError::BetsWindowCap),
            1 => (positions_opened, YieldLimitError::PositionsWindowCap),
            _ => (subscriptions_added, YieldLimitError::SubscriptionsWindowCap),
        };
        if window >= Self::WINDOW_CAP {
            return Err(err_window);
        }

        // Commit the (possibly rolled) window alongside the open.
        self.window_start = Some(window_start);
        self.bets_opened = bets_opened;
        self.positions_opened = positions_opened;
        self.subscriptions_added = subscriptions_added;

        match kind {
            0 => {
                self.bets += 1;
                self.bets_opened += 1;
            }
            1 => {
                self.positions += 1;
                self.positions_opened += 1;
            }
            _ => {
                self.subscriptions += 1;
                self.subscriptions_added += 1;
            }
        }
        Ok(())
    }

    fn close(&mut self, kind: u8) -> Result<(), YieldLimitError> {
        let counter = match kind {
            0 => &mut self.bets,
            1 => &mut self.positions,
            _ => &mut self.subscriptions,
        };
        if *counter == 0 {
            return Err(YieldLimitError::CounterUnderflow);
        }
        *counter -= 1;
        Ok(())
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// Compare the contract against the model across randomised open/close
    /// sequences interleaved with ledger advancement (including ledger jumps
    /// that skip multiple windows).
    #[test]
    fn proptest_window_ops_match_model(ops in prop::collection::vec((0u8..6, 0u8..12), 1..40)) {
        let env = Env::default();
        env.mock_all_auths();
        let contract = env.register(CalloraYieldLimits, ());
        let client = CalloraYieldLimitsClient::new(&env, &contract);
        let admin = Address::generate(&env);
        let user = Address::generate(&env);
        client.init(&admin);
        client.set_window_ledgers(&admin, &LimitsModel::WINDOW_LEDGERS);
        client.set_window_limits(
            &admin,
            &LimitsModel::WINDOW_CAP,
            &LimitsModel::WINDOW_CAP,
            &LimitsModel::WINDOW_CAP,
        );
        client.set_account_limits(
            &admin,
            &user,
            &LimitsModel::CONCURRENT_CAP,
            &LimitsModel::CONCURRENT_CAP,
            &LimitsModel::CONCURRENT_CAP,
        );

        let mut model = LimitsModel {
            sequence: env.ledger().sequence(),
            ..LimitsModel::default()
        };

        for (op, advance) in ops {
            if advance > 0 {
                env.ledger()
                    .with_mut(|li| li.sequence_number = li.sequence_number.saturating_add(advance as u32));
                model.sequence = model.sequence.saturating_add(advance as u32);
            }

            let (actual, expected) = match op {
                0..=2 => {
                    let actual = match op {
                        0 => client.try_place_bet(&user),
                        1 => client.try_open_position(&user),
                        _ => client.try_subscribe(&user),
                    };
                    (actual, model.open(op))
                }
                3..=5 => {
                    let actual = match op {
                        3 => client.try_clear_bet(&user),
                        4 => client.try_close_position(&user),
                        _ => client.try_unsubscribe(&user),
                    };
                    (actual, model.close(op - 3))
                }
                _ => unreachable!(),
            };

            let actual = match actual {
                Ok(Ok(())) => Ok(()),
                Ok(Err(_)) => panic!("unexpected conversion error"),
                Err(Ok(err)) => Err(err),
                Err(Err(_)) => panic!("unexpected invocation error"),
            };
            prop_assert_eq!(actual, expected);

            let state = client.get_account_state(&user);
            prop_assert_eq!(state.bets, model.bets);
            prop_assert_eq!(state.positions, model.positions);
            prop_assert_eq!(state.subscriptions, model.subscriptions);

            let window = client.get_window_state(&user);
            prop_assert_eq!(window.window_start, model.window_start.unwrap_or(0));
            prop_assert_eq!(window.bets_opened, model.bets_opened);
            prop_assert_eq!(window.positions_opened, model.positions_opened);
            prop_assert_eq!(window.subscriptions_added, model.subscriptions_added);
        }
    }
}
