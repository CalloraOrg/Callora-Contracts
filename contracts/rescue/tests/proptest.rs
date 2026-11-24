//! Property-based invariant tests for the rescue contract (#867).
//!
//! # Invariants
//!
//! 1. **Balance sync** — After every operation, the sum of the per-token
//!    `total_rescued_for(token)` counter plus the on-ledger token balance held
//!    by the contract must equal the initial contract balance.  Rejected
//!    operations must leave both values unchanged.
//!
//! 2. **Counter monotonicity** — `total_rescued_for(token)` is monotonically
//!    non-decreasing; it increases only by the exact `amount` of a successful
//!    rescue and never changes on failure.
//!
//! 3. **Admin gating** — Every state-changing entrypoint enforces
//!    `require_auth` and `assert_admin`.  Calling with a non-admin address
//!    always returns [`RescueError::Unauthorized`] and leaves state untouched.
//!
//! 4. **Amount validation** — Zero and negative amounts are always rejected
//!    with [`RescueError::AmountNotPositive`] regardless of the caller.
//!
//! 5. **Cap enforcement** — `rescue_capped` with `amount > cap` always fails
//!    with [`RescueError::ExceedsCap`].  `cap <= 0` always fails with
//!    [`RescueError::InvalidCap`].
//!
//! 6. **Self-transfer guard** — `rescue` / `rescue_capped` with
//!    `to == current_contract_address()` always fails with
//!    [`RescueError::InvalidRecipient`].
//!
//! # Generator
//!
//! A deterministic xorshift64 PRNG (no external `proptest`/`rand` dependency)
//! drives `SEED_COUNT` independent traces of `TRACE_LEN` randomly chosen
//! operations each, so any failure is fully reproducible from the printed
//! `seed` and `step`.

use callora_rescue::{CalloraRescue, CalloraRescueClient, RescueError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env};

const SEED_COUNT: u64 = 32;
const TRACE_LEN: u32 = 48;

/// Initial token balance minted to the contract for each trace.
const INITIAL_BALANCE: i128 = 1_000_000;

/// Maximum single-rescue amount used in valid operations.
const MAX_RESCUE: i128 = 100_000;

// ---------------------------------------------------------------------------
// Deterministic PRNG (xorshift64) — fully reproducible, no external dep.
// ---------------------------------------------------------------------------

struct Prng(u64);

impl Prng {
    fn new(seed: u64) -> Self {
        // Ensure non-zero state (xorshift stalls on zero).
        Self(seed.wrapping_add(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    /// Uniform random value in `[lo, hi]` (inclusive).
    fn range_i128(&mut self, lo: i128, hi: i128) -> i128 {
        if lo >= hi {
            return lo;
        }
        let span = (hi - lo) as u64 + 1;
        lo + (self.next_u64() % span) as i128
    }
}

// ---------------------------------------------------------------------------
// Operation types
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
enum Op {
    /// Successful rescue: positive amount, sufficient balance, correct admin.
    Rescue,
    /// Rescue with amount > on-ledger balance — must be rejected.
    RescueInsufficient,
    /// Rescue with amount == 0 — must be rejected.
    RescueZero,
    /// Rescue with amount < 0 — must be rejected.
    RescueNegative,
    /// Successful rescue_capped: amount <= cap, sufficient balance.
    RescueCapped,
    /// rescue_capped with amount > cap — must be rejected.
    RescueCappedExceedsCap,
    /// rescue_capped with cap <= 0 — must be rejected with InvalidCap.
    RescueCappedZeroCap,
    /// rescue / rescue_capped with a non-admin caller — must be rejected.
    RescueUnauthorized,
}

fn pick_op(rng: &mut Prng) -> Op {
    match rng.next_u64() % 9 {
        0 => Op::Rescue,
        1 => Op::RescueInsufficient,
        2 => Op::RescueZero,
        3 => Op::RescueNegative,
        4 => Op::RescueCapped,
        5 => Op::RescueCappedExceedsCap,
        6 => Op::RescueCappedZeroCap,
        _ => Op::RescueUnauthorized,
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Create a fresh environment with a registered rescue contract and a
/// Stellar asset contract pre-funded with `initial_balance` tokens.
fn setup(
    env: &Env,
    initial_balance: i128,
) -> (
    CalloraRescueClient<'_>,
    Address,
    token::Client<'_>,
    token::StellarAssetClient<'_>,
) {
    env.mock_all_auths();

    let admin = Address::generate(env);
    let contract_id = env.register(CalloraRescue, ());
    let client = CalloraRescueClient::new(env, &contract_id);

    let token_admin = Address::generate(env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(env, &token_addr);
    let token_admin_client = token::StellarAssetClient::new(env, &token_addr);

    if initial_balance > 0 {
        token_admin_client.mint(&contract_id, &initial_balance);
    }

    // Mint some tokens to the admin so we can attempt transfers later.
    token_admin_client.mint(&admin, &initial_balance);

    client.init(&admin);

    (client, admin, token_client, token_admin_client)
}

/// Assert the core invariant: `total_rescued_for(token)` + on-ledger balance
/// == initial balance, and the counter is non-negative.
fn assert_invariant(
    client: &CalloraRescueClient<'_>,
    token_client: &token::Client<'_>,
    initial_balance: i128,
    expected_total: i128,
    seed: u64,
    step: u32,
) {
    let total = client.total_rescued_for(&token_client.address);
    let on_ledger = token_client.balance(&client.address);

    // total_rescued_for must never go negative
    assert!(
        total >= 0,
        "seed={seed} step={step}: total_rescued_for ({total}) went negative"
    );

    // total_rescued_for must equal what we expect
    assert_eq!(
        total, expected_total,
        "seed={seed} step={step}: total_rescued_for ({total}) != expected ({expected_total})"
    );

    // on-ledger balance must be exactly initial minus total_rescued_for
    let expected_on_ledger = initial_balance - total;
    assert_eq!(
        on_ledger, expected_on_ledger,
        "seed={seed} step={step}: on-ledger balance ({on_ledger}) != expected ({expected_on_ledger})",
    );
}

// ---------------------------------------------------------------------------
// Seeded property traces
// ---------------------------------------------------------------------------

fn run_trace(seed: u64) {
    let env = Env::default();
    let (client, admin, token_client, _token_admin) = setup(&env, INITIAL_BALANCE);

    let mut rng = Prng::new(seed);
    let mut expected_total: i128 = 0;
    let recovered: Address = Address::generate(&env);

    assert_invariant(&client, &token_client, INITIAL_BALANCE, 0, seed, 0);

    for step in 1..=TRACE_LEN {
        let total_before = client.total_rescued_for(&token_client.address);
        let balance_before = token_client.balance(&client.address);

        match pick_op(&mut rng) {
            // --- Successful rescue -------------------------------------------------
            Op::Rescue => {
                // Pick amount that is guaranteed to be ≤ current balance
                let max_possible = balance_before.min(MAX_RESCUE);
                let amount = if max_possible > 0 {
                    rng.range_i128(1, max_possible)
                } else {
                    1_i128 // will fail with InsufficientBalance — that's fine
                };

                let result = client.try_rescue(&admin, &token_client.address, &recovered, &amount);

                if max_possible > 0 && amount <= max_possible {
                    assert!(
                        result.is_ok(),
                        "seed={seed} step={step}: valid rescue({amount}) unexpectedly failed"
                    );
                    expected_total += amount;
                } else {
                    assert!(
                        result.is_err(),
                        "seed={seed} step={step}: rescue({amount}) with insufficient balance must fail"
                    );
                }
            }

            // --- Insufficient balance ----------------------------------------------
            Op::RescueInsufficient => {
                let amount = rng.range_i128(balance_before + 1, balance_before + MAX_RESCUE);
                let result = client.try_rescue(&admin, &token_client.address, &recovered, &amount);
                assert!(
                    result.is_err(),
                    "seed={seed} step={step}: rescue({amount}) > balance({balance_before}) must fail"
                );
            }

            // --- Zero amount -------------------------------------------------------
            Op::RescueZero => {
                let result = client.try_rescue(&admin, &token_client.address, &recovered, &0);
                assert_eq!(
                    result.unwrap_err().unwrap(),
                    RescueError::AmountNotPositive,
                    "seed={seed} step={step}: rescue(0) must return AmountNotPositive"
                );
            }

            // --- Negative amount ---------------------------------------------------
            Op::RescueNegative => {
                let amount = rng.range_i128(-MAX_RESCUE, -1);
                let result = client.try_rescue(&admin, &token_client.address, &recovered, &amount);
                assert_eq!(
                    result.unwrap_err().unwrap(),
                    RescueError::AmountNotPositive,
                    "seed={seed} step={step}: rescue({amount}) must return AmountNotPositive"
                );
            }

            // --- Successful capped rescue ------------------------------------------
            Op::RescueCapped => {
                let max_possible = balance_before.min(MAX_RESCUE);
                let amount = if max_possible > 0 {
                    rng.range_i128(1, max_possible)
                } else {
                    1_i128
                };
                let cap = amount + rng.range_i128(0, MAX_RESCUE);
                let result = client.try_rescue_capped(
                    &admin,
                    &token_client.address,
                    &recovered,
                    &amount,
                    &cap,
                );

                if max_possible > 0 && amount <= max_possible {
                    assert!(
                        result.is_ok(),
                        "seed={seed} step={step}: valid rescue_capped({amount}, cap={cap}) unexpectedly failed"
                    );
                    expected_total += amount;
                } else {
                    assert!(
                        result.is_err(),
                        "seed={seed} step={step}: rescue_capped({amount}) with insufficient balance must fail"
                    );
                }
            }

            // --- Capped rescue exceeding cap ---------------------------------------
            Op::RescueCappedExceedsCap => {
                // cap must be at least 1 (positive), amount must exceed it
                let cap = rng.range_i128(1, MAX_RESCUE);
                let amount = rng.range_i128(cap + 1, cap + MAX_RESCUE);
                let result = client.try_rescue_capped(
                    &admin,
                    &token_client.address,
                    &recovered,
                    &amount,
                    &cap,
                );
                assert_eq!(
                    result.unwrap_err().unwrap(),
                    RescueError::ExceedsCap,
                    "seed={seed} step={step}: rescue_capped({amount}, cap={cap}) must return ExceedsCap"
                );
            }

            // --- Capped rescue with zero/negative cap — InvalidCap ----------------
            Op::RescueCappedZeroCap => {
                let amount = rng.range_i128(1, MAX_RESCUE);
                // Use either 0 or a negative cap to trigger InvalidCap.
                let cap = rng.range_i128(-MAX_RESCUE, 0);
                let result = client.try_rescue_capped(
                    &admin,
                    &token_client.address,
                    &recovered,
                    &amount,
                    &cap,
                );
                assert_eq!(
                    result.unwrap_err().unwrap(),
                    RescueError::InvalidCap, // changed from ExceedsCap
                    "seed={seed} step={step}: rescue_capped({amount}, cap={cap}) must return InvalidCap"
                );
            }

            // --- Unauthorized caller -----------------------------------------------
            Op::RescueUnauthorized => {
                let imposter = Address::generate(&env);
                env.set_auths(&[]); // clear all auths so imposter fails

                let amount = rng.range_i128(1, MAX_RESCUE);
                let result =
                    client.try_rescue(&imposter, &token_client.address, &recovered, &amount);
                assert!(
                    result.is_err(),
                    "seed={seed} step={step}: rescue by imposter must fail"
                );

                // Restore mock auths for remaining operations in this trace.
                env.mock_all_auths();
            }
        }

        // **Invariant**: after every operation, state must be consistent.
        assert_invariant(
            &client,
            &token_client,
            INITIAL_BALANCE,
            expected_total,
            seed,
            step,
        );

        // **Monotonicity**: total_rescued_for must not have decreased.
        let total_after = client.total_rescued_for(&token_client.address);
        assert!(
            total_after >= total_before,
            "seed={seed} step={step}: total_rescued_for decreased from {total_before} to {total_after}"
        );
    }
}

// ---------------------------------------------------------------------------
// Top-level test entry point
// ---------------------------------------------------------------------------

/// Run all `SEED_COUNT` deterministic seeded traces (each `TRACE_LEN` steps).
#[test]
fn rescue_invariant_holds_across_seeded_traces() {
    for seed in 0..SEED_COUNT {
        run_trace(seed);
    }
}

// ---------------------------------------------------------------------------
// Dedicated edge-case invariant tests
// ---------------------------------------------------------------------------

/// Rescue with amount exactly equal to the on-ledger balance succeeds and
/// leaves the contract with zero tokens.
#[test]
fn rescue_exact_full_balance_succeeds_and_drains() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraRescue, ());
    let client = CalloraRescueClient::new(&env, &contract_id);
    let to = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &token_addr);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);

    token_admin_client.mint(&contract_id, &5000);
    client.init(&admin);

    // Rescue the entire balance
    client.rescue(&admin, &token_addr, &to, &5000);

    assert_eq!(client.total_rescued_for(&token_addr), 5000);
    assert_eq!(token_client.balance(&contract_id), 0);
}

/// After a rejected operation (AmountNotPositive), total_rescued_for and
/// on-ledger balance must be unchanged.
#[test]
fn rejected_amount_not_positive_does_not_mutate_state() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraRescue, ());
    let client = CalloraRescueClient::new(&env, &contract_id);
    let to = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &token_addr);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);

    token_admin_client.mint(&contract_id, &1000);
    client.init(&admin);

    // Perform a successful rescue first to have non-zero state
    client.rescue(&admin, &token_addr, &to, &500);
    assert_eq!(client.total_rescued_for(&token_addr), 500);

    // Now try zero amount
    let total_before = client.total_rescued_for(&token_addr);
    let balance_before = token_client.balance(&contract_id);

    let result = client.try_rescue(&admin, &token_addr, &to, &0);
    assert_eq!(result.unwrap_err().unwrap(), RescueError::AmountNotPositive);

    assert_eq!(
        client.total_rescued_for(&token_addr),
        total_before,
        "total_rescued_for changed after rejected zero-amount rescue"
    );
    assert_eq!(
        token_client.balance(&contract_id),
        balance_before,
        "on-ledger balance changed after rejected zero-amount rescue"
    );
}

/// After a rejected Unauthorized operation, total_rescued_for and on-ledger
/// balance must be unchanged.
#[test]
fn rejected_unauthorized_does_not_mutate_state() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraRescue, ());
    let client = CalloraRescueClient::new(&env, &contract_id);
    let to = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &token_addr);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);

    token_admin_client.mint(&contract_id, &1000);
    client.init(&admin);

    // Perform a successful rescue first
    client.rescue(&admin, &token_addr, &to, &300);
    assert_eq!(client.total_rescued_for(&token_addr), 300);

    let imposter = Address::generate(&env);
    env.set_auths(&[]);

    let total_before = client.total_rescued_for(&token_addr);
    let balance_before = token_client.balance(&contract_id);

    let result = client.try_rescue(&imposter, &token_addr, &to, &100);
    assert!(result.is_err());

    assert_eq!(
        client.total_rescued_for(&token_addr),
        total_before,
        "total_rescued_for changed after rejected unauthorized rescue"
    );
    assert_eq!(
        token_client.balance(&contract_id),
        balance_before,
        "on-ledger balance changed after rejected unauthorized rescue"
    );
}

/// Multiple rescues from the same token accumulate correctly in
/// total_rescued_for and deplete the on-ledger balance accordingly.
#[test]
fn multiple_rescues_accumulate_correctly() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraRescue, ());
    let client = CalloraRescueClient::new(&env, &contract_id);
    let to = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &token_addr);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);

    token_admin_client.mint(&contract_id, &10_000);
    client.init(&admin);

    let amounts: [i128; 5] = [100, 200, 300, 400, 500];

    for (i, &amount) in amounts.iter().enumerate() {
        client.rescue(&admin, &token_addr, &to, &amount);

        let expected_total: i128 = amounts[..=i].iter().sum();
        let expected_balance: i128 = 10_000 - expected_total;

        assert_eq!(
            client.total_rescued_for(&token_addr),
            expected_total,
            "total_rescued_for mismatch after rescue {}",
            i + 1
        );
        assert_eq!(
            token_client.balance(&contract_id),
            expected_balance,
            "on-ledger balance mismatch after rescue {}",
            i + 1
        );
    }
}

/// Rescue_capped correctly enforces the cap even when the current balance
/// would otherwise be sufficient.
#[test]
fn rescue_capped_enforces_cap_above_balance() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraRescue, ());
    let client = CalloraRescueClient::new(&env, &contract_id);
    let to = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &token_addr);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);

    token_admin_client.mint(&contract_id, &10_000);
    client.init(&admin);

    // amount (500) exceeds cap (100) -> ExceedsCap (balance is sufficient)
    let err = client
        .try_rescue_capped(&admin, &token_addr, &to, &500, &100)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, RescueError::ExceedsCap);

    // State must be unchanged
    assert_eq!(client.total_rescued_for(&token_addr), 0);
    assert_eq!(token_client.balance(&contract_id), 10_000);
}

/// rescue_capped with cap == 0 must return InvalidCap, not ExceedsCap.
#[test]
fn rescue_capped_zero_cap_returns_invalid_cap_not_exceeds_cap() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraRescue, ());
    let client = CalloraRescueClient::new(&env, &contract_id);
    let to = Address::generate(&env);

    let token_admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);
    token_admin_client.mint(&contract_id, &1000);
    client.init(&admin);

    let err = client
        .try_rescue_capped(&admin, &token_addr, &to, &1, &0)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, RescueError::InvalidCap);
}

/// Self-transfer via rescue must return InvalidRecipient and not mutate state.
#[test]
fn rescue_self_transfer_returns_invalid_recipient() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraRescue, ());
    let client = CalloraRescueClient::new(&env, &contract_id);

    let token_admin = Address::generate(&env);
    let token_addr = env
        .register_stellar_asset_contract_v2(token_admin.clone())
        .address();
    let token_client = token::Client::new(&env, &token_addr);
    let token_admin_client = token::StellarAssetClient::new(&env, &token_addr);
    token_admin_client.mint(&contract_id, &1000);
    client.init(&admin);

    let err = client
        .try_rescue(&admin, &token_addr, &contract_id, &100)
        .unwrap_err()
        .unwrap();
    assert_eq!(err, RescueError::InvalidRecipient);

    // State must be completely unchanged
    assert_eq!(client.total_rescued_for(&token_addr), 0);
    assert_eq!(token_client.balance(&contract_id), 1000);
}

// ---------------------------------------------------------------------------
// `rescue_many` conservation property
// ---------------------------------------------------------------------------

/// Token balance minted to the rescue contract for each batch trace.
const BATCH_INITIAL_BALANCE: i128 = 10_000_000;

/// Maximum amount moved by a single batch item.
const BATCH_MAX_RESCUE: i128 = 50_000;

const BATCH_TRACE_LEN: u32 = 24;

/// Register `count` Stellar Asset Contracts, each pre-funded with
/// [`BATCH_INITIAL_BALANCE`], and return their addresses.
fn setup_batch_tokens(env: &Env, contract_id: &Address, count: u32) -> std::vec::Vec<Address> {
    let token_admin = Address::generate(env);
    let mut tokens = std::vec::Vec::new();
    for _ in 0..count {
        let token_addr = env
            .register_stellar_asset_contract_v2(token_admin.clone())
            .address();
        token::StellarAssetClient::new(env, &token_addr).mint(contract_id, &BATCH_INITIAL_BALANCE);
        tokens.push(token_addr);
    }
    tokens
}

/// Assert that contract balances, recipient balances and per-token counters
/// all agree with `expected`.
fn assert_batch_invariant(
    env: &Env,
    client: &CalloraRescueClient<'_>,
    tokens: &[Address],
    expected: &[i128],
    recipient: &Address,
    seed: u64,
    step: u32,
) {
    for (i, token_addr) in tokens.iter().enumerate() {
        let token_client = token::Client::new(env, token_addr);
        let want = expected[i];

        assert_eq!(
            client.total_rescued_for(token_addr),
            want,
            "seed={seed} step={step}: total_rescued_for(token {i}) mismatch"
        );
        assert_eq!(
            token_client.balance(&client.address),
            BATCH_INITIAL_BALANCE - want,
            "seed={seed} step={step}: contract balance for token {i} mismatch"
        );
        assert_eq!(
            token_client.balance(recipient),
            want,
            "seed={seed} step={step}: recipient received wrong amount of token {i}"
        );
    }
}

fn run_batch_trace(seed: u64) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraRescue, ());
    let client = CalloraRescueClient::new(&env, &contract_id);
    client.init(&admin);

    let recipient = Address::generate(&env);
    let tokens = setup_batch_tokens(&env, &contract_id, 2);
    let mut expected = [0_i128, 0_i128];

    let mut rng = Prng::new(seed ^ 0x0B47_C0DE);

    for step in 1..=BATCH_TRACE_LEN {
        let total_before = client.total_rescued_for(&tokens[0]);

        match rng.next_u64() % 5 {
            // --- Valid batch of one or two distinct tokens ------------------
            0 | 1 => {
                let k = 1 + (rng.next_u64() as usize % tokens.len());
                let first = rng.next_u64() as usize % tokens.len();
                let mut picks = std::vec![first];
                if k == 2 {
                    picks.push((first + 1) % tokens.len());
                }

                let mut batch = soroban_sdk::Vec::new(&env);
                let mut moved = std::vec::Vec::new();
                for &i in picks.iter() {
                    let remaining = BATCH_INITIAL_BALANCE - expected[i];
                    let amount = rng.range_i128(1, remaining.min(BATCH_MAX_RESCUE));
                    batch.push_back((tokens[i].clone(), amount));
                    moved.push((i, amount));
                }

                client.rescue_many(&admin, &recipient, &batch);
                for (i, amount) in moved {
                    expected[i] += amount;
                }
            }

            // --- One insufficient item rolls the whole batch back -----------
            2 => {
                let good = rng.next_u64() as usize % tokens.len();
                let bad = (good + 1) % tokens.len();

                let good_remaining = BATCH_INITIAL_BALANCE - expected[good];
                let good_amount = rng.range_i128(1, good_remaining.min(BATCH_MAX_RESCUE));
                let bad_balance = BATCH_INITIAL_BALANCE - expected[bad];
                let bad_amount = bad_balance + rng.range_i128(1, 1_000);

                let mut batch = soroban_sdk::Vec::new(&env);
                batch.push_back((tokens[good].clone(), good_amount));
                batch.push_back((tokens[bad].clone(), bad_amount));

                assert_eq!(
                    client
                        .try_rescue_many(&admin, &recipient, &batch)
                        .unwrap_err()
                        .unwrap(),
                    RescueError::InsufficientBalance,
                    "seed={seed} step={step}: batch with an unbacked item must fail"
                );
            }

            // --- Zero amount --------------------------------------------------
            3 => {
                let i = rng.next_u64() as usize % tokens.len();
                let mut batch = soroban_sdk::Vec::new(&env);
                batch.push_back((tokens[i].clone(), 0_i128));

                assert_eq!(
                    client
                        .try_rescue_many(&admin, &recipient, &batch)
                        .unwrap_err()
                        .unwrap(),
                    RescueError::AmountNotPositive,
                    "seed={seed} step={step}: zero-amount item must fail"
                );
            }

            // --- Duplicate token ---------------------------------------------
            4 => {
                let i = rng.next_u64() as usize % tokens.len();
                let mut batch = soroban_sdk::Vec::new(&env);
                batch.push_back((tokens[i].clone(), 1_i128));
                batch.push_back((tokens[i].clone(), 2_i128));

                assert_eq!(
                    client
                        .try_rescue_many(&admin, &recipient, &batch)
                        .unwrap_err()
                        .unwrap(),
                    RescueError::DuplicateToken,
                    "seed={seed} step={step}: duplicate tokens must be rejected"
                );
            }

            _ => {
                let empty: soroban_sdk::Vec<(Address, i128)> = soroban_sdk::Vec::new(&env);
                assert_eq!(
                    client
                        .try_rescue_many(&admin, &recipient, &empty)
                        .unwrap_err()
                        .unwrap(),
                    RescueError::BatchEmpty,
                    "seed={seed} step={step}: empty batch must be rejected"
                );
            }
        }

        assert_batch_invariant(&env, &client, &tokens, &expected, &recipient, seed, step);

        assert!(
            client.total_rescued_for(&tokens[0]) >= total_before,
            "seed={seed} step={step}: per-token counter decreased"
        );
    }
}

/// Conservation: across seeded traces of valid and rejected batches,
/// `contract balance + rescued == initial balance` and
/// `recipient balance == rescued` for every token, while rejected batches
/// leave both untouched.
#[test]
fn rescue_many_conservation_holds_across_seeded_batches() {
    for seed in 0..SEED_COUNT {
        run_batch_trace(seed);
    }
}
