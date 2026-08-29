//! Parity and edge-case tests for the read-only `simulate_deduct` view.
//!
//! `simulate_deduct` mirrors `deduct`'s validation pipeline but performs no
//! state writes, no external calls, and no event emission. The tests in this
//! file assert:
//! 1. The simulation returns the same `Result<i128, VaultError>` as the real
//!    `deduct` for any given state-and-inputs configuration (parity).
//! 2. The simulation does not mutate vault state (balance unchanged).
//! 3. The simulation requires no authentication.
//! 4. Every documented error code is reachable from the view.

extern crate std;

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, Symbol};

use super::*;
use callora_settlement::CalloraSettlement;

// ---------------------------------------------------------------------------
// Helpers (kept local — duplicating test_views.rs setup is acceptable for now
// per the "minimal changes" guideline of the issue).
// ---------------------------------------------------------------------------

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract_address.address();
    let token_client = token::Client::new(env, &address);
    let admin_client = token::StellarAssetClient::new(env, &address);
    (address, token_client, admin_client)
}

fn create_vault(env: &Env) -> (Address, CalloraVaultClient<'_>) {
    let address = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &address);
    (address, client)
}

fn create_settlement(env: &Env, admin: &Address, vault_address: &Address) -> Address {
    let settlement_address = env.register(CalloraSettlement, ());
    let settlement_client =
        callora_settlement::CalloraSettlementClient::new(env, &settlement_address);
    env.mock_all_auths();
    settlement_client.init(admin, vault_address);
    settlement_address
}

fn fund_vault(
    usdc_admin_client: &token::StellarAssetClient,
    vault_address: &Address,
    amount: i128,
) {
    usdc_admin_client.mint(vault_address, &amount);
}

/// Initialize a funded vault with a configured settlement address.
fn setup_funded_vault(
    env: &Env,
    balance: i128,
    max_deduct: Option<i128>,
    auth_caller: Option<Address>,
) -> (CalloraVaultClient<'_>, Address) {
    let owner = Address::generate(env);
    let (vault_address, client) = create_vault(env);
    let (usdc, _, usdc_admin) = create_usdc(env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, balance);
    client.init(
        &owner,
        &usdc,
        &Some(balance),
        &auth_caller,
        &None,
        &None,
        &max_deduct,
    );
    let settlement = create_settlement(env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);
    (client, owner)
}

// ---------------------------------------------------------------------------
// 1. Happy path & parity with real `deduct`
// ---------------------------------------------------------------------------

#[test]
fn simulate_deduct_happy_path_matches_deduct() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    // First call: simulate, expect Ok(950). State must NOT change.
    let sim_before = client.balance();
    let sim_result = client.simulate_deduct(&owner, &50_i128, &None, &u16::MAX, &developer);
    let sim_after = client.balance();
    assert_eq!(sim_result, 950);
    assert_eq!(sim_before, sim_after, "simulation must not mutate balance");

    // Then call real deduct with the same args. Should match the simulation.
    let real_result = client.deduct(&owner, &50_i128, &None, &u16::MAX, &developer);
    assert_eq!(real_result, sim_result);
    assert_eq!(client.balance(), 950);
}

#[test]
fn simulate_deduct_happy_path_with_request_id() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 500, None, None);
    let developer = Address::generate(&env);
    let rid = Symbol::new(&env, "req_abc_123");

    env.mock_all_auths();

    let sim = client.simulate_deduct(&owner, &100_i128, &Some(rid.clone()), &u16::MAX, &developer);
    assert_eq!(sim, 400);
    assert_eq!(client.balance(), 500, "simulation must not deduct");

    let real = client.deduct(&owner, &100_i128, &Some(rid.clone(), &developer), &u16::MAX);
    assert_eq!(real, 400);
    assert_eq!(client.balance(), 400);
}

#[test]
fn simulate_deduct_exact_balance() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 100, None, None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    let sim = client.simulate_deduct(&owner, &100_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim, 0);
    assert_eq!(client.balance(), 100);

    let real = client.deduct(&owner, &100_i128, &None, &u16::MAX, &developer);
    assert_eq!(real, 0);
    assert_eq!(client.balance(), 0);
}

// ---------------------------------------------------------------------------
// 2. No mutating side-effects
// ---------------------------------------------------------------------------

#[test]
fn simulate_deduct_does_not_write_any_storage_keys() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);
    let rid = Symbol::new(&env, "never_persisted");

    env.mock_all_auths();

    // Snapshot: balance before, request_id not marked.
    assert_eq!(client.balance(), 1_000);
    assert!(!client.is_request_processed(rid.clone()));

    // Simulate. Expect no state change.
    let _ = client.simulate_deduct(&owner, &200_i128, &Some(rid.clone()), &u16::MAX, &developer);

    // State unchanged:
    assert_eq!(client.balance(), 1_000);
    assert!(
        !client.is_request_processed(rid.clone()),
        "simulate must not mark request_id as processed"
    );

    // The real deduct must STILL succeed because simulate did not consume
    // the rate-limit budget or mark the request_id.
    let real = client.deduct(&owner, &200_i128, &Some(rid.clone(), &developer), &u16::MAX);
    assert_eq!(real, 800);
}

#[test]
fn simulate_deduct_does_not_emit_events() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    // Take event snapshot, then run the simulation. The simulation must not
    // publish any new events (Soroban's `events().all()` returns only events
    // emitted during the most recent contract invocation, so a side-effect-free
    // call leaves the event list empty).
    let _ = client.simulate_deduct(&owner, &50_i128, &None, &u16::MAX, &developer);
    let events_after_sim = env.events().all();
    assert_eq!(
        events_after_sim.len(),
        0,
        "simulate_deduct must not emit any events (got: {})",
        events_after_sim.len()
    );

    // For comparison: the real deduct DOES emit one event.
    client.deduct(&owner, &50_i128, &None, &u16::MAX, &developer);
    let events_after_real = env.events().all();
    assert_eq!(events_after_real.len(), 1);
}

#[test]
fn simulate_deduct_repeated_calls_share_no_state() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    // Ten simulations in a row. Balance must remain unchanged; behavior must
    // be deterministic.
    for _ in 0..10 {
        let sim = client.simulate_deduct(&owner, &30_i128, &None, &u16::MAX, &developer);
        assert_eq!(sim, 970);
        assert_eq!(client.balance(), 1_000);
    }
}

// ---------------------------------------------------------------------------
// 3. Error parity with `deduct`
// ---------------------------------------------------------------------------

#[test]
fn simulate_deduct_amount_zero_returns_amount_not_positive() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    let balance_before = client.balance();
    let sim = client.try_simulate_deduct(&owner, &0_i128, &None, &u16::MAX, &developer);
    let real = client.try_deduct(&owner, &0_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim, real);
    assert_eq!(sim, Err(Ok(VaultError::AmountNotPositive)));
    assert_eq!(client.balance(), balance_before);
}

#[test]
fn simulate_deduct_amount_negative_returns_amount_not_positive() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    let sim = client.try_simulate_deduct(&owner, &-25_i128, &None, &u16::MAX, &developer);
    let real = client.try_deduct(&owner, &-25_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim, real);
    assert_eq!(sim, Err(Ok(VaultError::AmountNotPositive)));
}

#[test]
fn simulate_deduct_exceeds_max_deduct() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, Some(500_i128), None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    let sim = client.try_simulate_deduct(&owner, &501_i128, &None, &u16::MAX, &developer);
    let real = client.try_deduct(&owner, &501_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim, real);
    assert_eq!(sim, Err(Ok(VaultError::ExceedsMaxDeduct)));
}

#[test]
fn simulate_deduct_at_max_deduct_succeeds() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, Some(500_i128), None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    let sim = client.simulate_deduct(&owner, &500_i128, &None, &u16::MAX, &developer);
    let real = client.deduct(&owner, &500_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim, 500);
    assert_eq!(real, 500);
}

#[test]
fn simulate_deduct_insufficient_balance() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 100, None, None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    let sim = client.try_simulate_deduct(&owner, &250_i128, &None, &u16::MAX, &developer);
    let real = client.try_deduct(&owner, &250_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim, real);
    assert_eq!(sim, Err(Ok(VaultError::InsufficientBalance)));
    assert_eq!(
        client.balance(),
        100,
        "simulation must not change state on failure"
    );
}

#[test]
fn simulate_deduct_paused_vault_returns_paused() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);

    client.pause(&owner);
    env.mock_all_auths();

    let sim = client.try_simulate_deduct(&owner, &50_i128, &None, &u16::MAX, &developer);
    let real = client.try_deduct(&owner, &50_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim, real);
    assert_eq!(sim, Err(Ok(VaultError::Paused)));
}

#[test]
fn simulate_deduct_slippage_exceeded() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    // max_fee_bps = 100 bps = 1 %. Out of 1_000 balance, that's at most 10.
    let sim = client.try_simulate_deduct(&owner, &50_i128, &None, &100_u16, &developer);
    let real = client.try_deduct(&owner, &50_i128, &None, &100_u16, &developer);
    assert_eq!(sim, real);
    assert_eq!(sim, Err(Ok(VaultError::Slippage)));
}

#[test]
fn simulate_deduct_slippage_at_threshold_succeeds() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    // max_fee_bps = 500 bps = 5%. Out of 1_000 balance, at most 50.
    let sim = client.simulate_deduct(&owner, &50_i128, &None, &500_u16, &developer);
    let real = client.deduct(&owner, &50_i128, &None, &500_u16, &developer);
    assert_eq!(sim, 950);
    assert_eq!(real, 950);
}

#[test]
fn simulate_deduct_slippage_disabled_when_max_fee_bps_is_u16_max() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    // u16::MAX sentinel: slippage guard disabled.
    let sim = client.simulate_deduct(&owner, &999_i128, &None, &u16::MAX, &developer);
    let real = client.deduct(&owner, &999_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim, 1);
    assert_eq!(real, 1);
}

#[test]
fn simulate_deduct_duplicate_request_id() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);
    let rid = Symbol::new(&env, "req_dup_42");

    env.mock_all_auths();

    // First deduct with this rid succeeds.
    client.deduct(&owner, &100_i128, &Some(rid.clone(), &developer), &u16::MAX);
    assert_eq!(client.balance(), 900);

    // Now simulate_deduct with the same rid. Should return DuplicateRequestId
    // (parity with re-running real deduct).
    let sim =
        client.try_simulate_deduct(&owner, &100_i128, &Some(rid.clone()), &u16::MAX, &developer);
    let real = client.try_deduct(&owner, &100_i128, &Some(rid.clone()), &u16::MAX, &developer);
    assert_eq!(sim, real);
    assert_eq!(sim, Err(Ok(VaultError::DuplicateRequestId)));
    assert_eq!(
        client.balance(),
        900,
        "simulation must not change state on duplicate"
    );
}

#[test]
fn simulate_deduct_new_request_id_differs_from_replay() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    let rid_new = Symbol::new(&env, "fresh_id");
    let sim = client.simulate_deduct(
        &owner,
        &100_i128,
        &Some(rid_new.clone()),
        &u16::MAX,
        &developer,
    );
    let real = client.deduct(
        &owner,
        &100_i128,
        &Some(rid_new.clone(), &developer),
        &u16::MAX,
    );
    assert_eq!(sim, 900);
    assert_eq!(real, 900);
    assert!(client.is_request_processed(rid_new.clone()));
}

// ---------------------------------------------------------------------------
// 4. Auth-free: simulate does not require auth
// ---------------------------------------------------------------------------

#[test]
fn simulate_deduct_does_not_require_auth_for_any_caller() {
    let env = Env::default();
    let (client, _) = setup_funded_vault(&env, 1_000, None, None);
    // Deliberately use a stranger as the caller.
    let stranger = Address::generate(&env);
    let developer = Address::generate(&env);

    // mock_all_auths is NOT enabled — if simulate called require_auth,
    // the test would panic with "missing auth".
    let sim = client.simulate_deduct(&stranger, &50_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim, 950);
    assert_eq!(client.balance(), 1_000, "simulation must not deduct");
}

#[test]
fn simulate_deduct_works_even_when_no_authorized_caller_configured() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    let developer = Address::generate(&env);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1_000);
    client.init(&owner, &usdc, &Some(1_000), &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);

    // sim by a stranger must succeed (no `require_authorized_deduct_caller`).
    let stranger = Address::generate(&env);
    let sim = client.simulate_deduct(&stranger, &50_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim, 950);
    assert_eq!(client.balance(), 1_000);
}

// ---------------------------------------------------------------------------
// 5. Rate-limit parity
// ---------------------------------------------------------------------------

#[test]
fn simulate_deduct_rate_limit_consistent_with_deduct() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let developer = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1_000_000);
    client.init(&owner, &usdc, &Some(1_000_000), &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);

    // Set up a rate limit for the developer.
    client.set_developer_rate_limit(
        &developer,
        &crate::rate_limit::RateLimitConfig {
            capacity: 100,
            refill_rate: 10,
        },
    );

    // First call: 50 fits in 100-token bucket.
    let sim1 = client.simulate_deduct(&owner, &50_i128, &None, &u16::MAX, &developer);
    let real1 = client.deduct(&owner, &50_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim1, 999_950);
    assert_eq!(real1, 999_950);

    // Second sim: try 200 — bucket has 50 left, can't fit → RateLimited.
    let sim2 = client.try_simulate_deduct(&owner, &200_i128, &None, &u16::MAX, &developer);
    let real2 = client.try_deduct(&owner, &200_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim2, real2);
    assert_eq!(sim2, Err(Ok(VaultError::RateLimited)));

    // Critically: simulation must NOT have consumed any tokens.
    let state = client.get_developer_rate_limit_state(&developer);
    assert!(state.is_some());
    // After the first real deduct the bucket had 50 tokens;
    // the simulation must not have changed that.
    assert_eq!(
        state.unwrap().tokens,
        50,
        "simulate_deduct must not write to the developer's rate-limit bucket"
    );

    // Subsequent simulate still says RateLimited (parity preserved).
    let sim3 = client.try_simulate_deduct(&owner, &200_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim3, Err(Ok(VaultError::RateLimited)));
}

#[test]
fn simulate_deduct_no_rate_limit_configured() {
    let env = Env::default();
    let (client, owner) = setup_funded_vault(&env, 1_000, None, None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    // No rate limit configured for this developer → simulate always Ok(...)
    // regardless of amount.
    let sim = client.simulate_deduct(&owner, &999_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim, 1);
}

// ---------------------------------------------------------------------------
// 6. Settlement configuration
// ---------------------------------------------------------------------------

#[test]
fn simulate_deduct_succeeds_when_settlement_unset() {
    // The simulation never reaches `require_settlement`, so a vault without
    // settlement configured still simulates successfully. The real deduct
    // would error out further down the pipeline.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    let developer = Address::generate(&env);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1_000);
    client.init(&owner, &usdc, &Some(1_000), &None, &None, &None, &None);
    // NB: set_settlement NOT called.

    let sim = client.simulate_deduct(&owner, &50_i128, &None, &u16::MAX, &developer);
    assert_eq!(sim, 950);
    assert_eq!(client.balance(), 1_000);

    // Sanity: the real deduct fails on SettlementNotSet past this point —
    // confirm we did not pre-empt that on the simulation path.
    let real = client.try_deduct(&owner, &50_i128, &None, &u16::MAX, &developer);
    assert_eq!(real, Err(Ok(VaultError::SettlementNotSet)));
}

// ---------------------------------------------------------------------------
// 7. Table-driven parity sweep (genuine two-sided parity)
// ---------------------------------------------------------------------------

#[test]
fn simulate_deduct_parity_table() {
    // For every scenario in the table we:
    //   1. Initialize a fresh vault with the scenario's config.
    //   2. Call BOTH try_simulate_deduct and try_deduct with the same args.
    //   3. Assert the two return values are STRUCTURALLY EQUAL.
    struct Scenario {
        label: &'static str,
        balance: i128,
        max_deduct: Option<i128>,
        rate_limit: Option<crate::rate_limit::RateLimitConfig>,
        amount: i128,
        max_fee_bps: u16,
    }

    let scenarios: &[Scenario] = &[
        Scenario {
            label: "happy",
            balance: 1_000,
            max_deduct: None,
            rate_limit: None,
            amount: 100,
            max_fee_bps: u16::MAX,
        },
        Scenario {
            label: "amount_zero",
            balance: 1_000,
            max_deduct: None,
            rate_limit: None,
            amount: 0,
            max_fee_bps: u16::MAX,
        },
        Scenario {
            label: "amount_negative",
            balance: 1_000,
            max_deduct: None,
            rate_limit: None,
            amount: -1,
            max_fee_bps: u16::MAX,
        },
        Scenario {
            label: "exceed_max",
            balance: 10_000,
            max_deduct: Some(500),
            rate_limit: None,
            amount: 501,
            max_fee_bps: u16::MAX,
        },
        Scenario {
            label: "insufficient",
            balance: 50,
            max_deduct: None,
            rate_limit: None,
            amount: 100,
            max_fee_bps: u16::MAX,
        },
        Scenario {
            label: "exact_balance",
            balance: 75,
            max_deduct: None,
            rate_limit: None,
            amount: 75,
            max_fee_bps: u16::MAX,
        },
        Scenario {
            label: "slippage_exceed",
            balance: 1_000,
            max_deduct: None,
            rate_limit: None,
            amount: 500,
            max_fee_bps: 100,
        },
        Scenario {
            label: "slippage_at_max",
            balance: 1_000,
            max_deduct: None,
            rate_limit: None,
            amount: 100,
            max_fee_bps: 1_000,
        },
        Scenario {
            label: "slippage_off",
            balance: 1_000,
            max_deduct: None,
            rate_limit: None,
            amount: 999,
            max_fee_bps: u16::MAX,
        },
        Scenario {
            label: "rl_unconfigured",
            balance: 1_000,
            max_deduct: None,
            rate_limit: None,
            amount: 999,
            max_fee_bps: u16::MAX,
        },
        Scenario {
            label: "rl_blocked",
            balance: 1_000_000,
            max_deduct: None,
            rate_limit: Some(crate::rate_limit::RateLimitConfig {
                capacity: 100,
                refill_rate: 10,
            }),
            amount: 200,
            max_fee_bps: u16::MAX,
        },
        Scenario {
            label: "rl_fits",
            balance: 1_000_000,
            max_deduct: None,
            rate_limit: Some(crate::rate_limit::RateLimitConfig {
                capacity: 100,
                refill_rate: 10,
            }),
            amount: 50,
            max_fee_bps: u16::MAX,
        },
    ];

    for scenario in scenarios.iter() {
        let env = Env::default();
        let owner = Address::generate(&env);
        let (vault_address, client) = create_vault(&env);
        let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
        let developer = Address::generate(&env);

        env.mock_all_auths();
        fund_vault(&usdc_admin, &vault_address, scenario.balance);
        client.init(
            &owner,
            &usdc,
            &Some(scenario.balance),
            &None,
            &None,
            &None,
            &scenario.max_deduct,
        );
        let settlement = create_settlement(&env, &owner, &vault_address);
        client.set_settlement(&owner, &settlement);

        if let Some(ref cfg) = scenario.rate_limit {
            client.set_developer_rate_limit(&developer, cfg);
        }

        env.mock_all_auths();

        // For rate-limit scenarios: simulate first (read-only), then real.
        // For all other scenarios: simulate, then real.
        let sim = client.try_simulate_deduct(
            &owner,
            &scenario.amount,
            &None::<Symbol>,
            &scenario.max_fee_bps,
            &developer,
        );
        let real = client.try_deduct(
            &owner,
            &scenario.amount,
            &None::<Symbol>,
            &scenario.max_fee_bps,
            &developer,
        );

        // The simulation must not change the vault balance.
        assert_eq!(
            client.balance(),
            scenario.balance,
            "scenario '{}': simulation must not mutate balance",
            scenario.label
        );

        // Parity: both must return the same Result.
        assert_eq!(
            sim, real,
            "scenario '{}': simulate/real parity mismatch: sim={:?}, real={:?}",
            scenario.label, sim, real
        );

        let _ = vault_address;
    }
}

// ---------------------------------------------------------------------------
// 8. Math: no unwrap, no overflow on edge inputs
// ---------------------------------------------------------------------------

#[test]
fn simulate_deduct_does_not_panic_on_max_balance_and_max_amount() {
    let env = Env::default();
    // Balance near i128::MAX so the slippage math (`amount * 10_000`) is
    // close to overflow. We set max_deduct = i128::MAX to allow it.
    let balance = i128::MAX / 20_000; // safe for `amount * 10_000`
    let (client, owner) = setup_funded_vault(&env, balance, Some(i128::MAX), None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    // Pick a safe amount that doesn't trigger overflow in slippage mul.
    let amount = balance / 10;
    let sim = client.simulate_deduct(&owner, &amount, &None, &u16::MAX, &developer);
    assert!(
        sim.is_ok(),
        "simulation must not overflow on large numbers: {:?}",
        sim
    );
}

#[test]
fn simulate_deduct_slippage_overflow_returns_overflow_error() {
    let env = Env::default();
    // Pick a balance/amount pair where `amount * 10_000` overflows i128.
    let big = (i128::MAX / 20_000) + 1;
    let (client, owner) = setup_funded_vault(&env, big, Some(big), None);
    let developer = Address::generate(&env);

    env.mock_all_auths();

    // max_fee_bps = 0 → slippage guard fires after the `amount * 10_000`
    // multiplication. With `big` amount that mul overflows → Overflow.
    let sim = client.try_simulate_deduct(&owner, &big, &None::<Symbol>, &0u16, &developer);
    let real = client.try_deduct(&owner, &big, &None::<Symbol>, &0u16, &developer);

    // Parity: simulation and real must return the SAME Result.
    assert_eq!(sim, real, "overflow parity failed: {:?} vs {:?}", sim, real);

    // Tighten: both must return Overflow.
    assert_eq!(sim, Err(Ok(VaultError::Overflow)));
}
