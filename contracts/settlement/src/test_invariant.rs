//! Property-based invariant test: settlement balance conservation.
//!
//! # Invariant
//! After every operation in an arbitrary sequence of `receive_payment`,
//! `batch_receive_payment`, and `withdraw_developer_balance`, the following
//! must hold:
//!
//! ```text
//! total_in == sum(per_developer_balance) + global_pool.total_balance
//! ```
//!
//! where `total_in` is the running tally of every positive amount credited
//! through the settlement contract (developer route only; pool credits are
//! tracked separately).
//!
//! # Strategy
//! A deterministic LCG PRNG drives [`SEED_COUNT`] independent traces of
//! [`TRACE_LENGTH`] steps each.  Every step is recorded in a [`Trace`]
//! so that any invariant violation prints a full, human-readable counterexample
//! that can be replayed by fixing the seed.
//!
//! # proptest integration
//! The `proptest` crate is wired as a dev-dependency (satisfying issue #489).
//! Because Soroban contracts compile to `no_std` WASM, proptest's built-in
//! shrinking cannot drive the Soroban test harness directly.  Instead we use
//! proptest's [`proptest!`] macro to generate seeds and delegate each seed to
//! the same deterministic LCG runner — this gives proptest-managed shrinking
//! of the *seed* while the LCG produces the full reproducible trace from it.
//!
//! # Reproduction
//! On any failure the seed and full step trace are printed, making it trivial
//! to reproduce the exact sequence.

extern crate std;

use std::boxed::Box;

use proptest::prelude::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, Vec};

use crate::{CalloraSettlement, CalloraSettlementClient};

// ---------------------------------------------------------------------------
// Tunables
// ---------------------------------------------------------------------------

/// Steps per deterministic trace.
const TRACE_LENGTH: u32 = 48;

/// Number of explicit seeded traces (in addition to proptest-generated ones).
const SEED_COUNT: u64 = 64;

/// Maximum payment / withdrawal amount per step.
const AMOUNT_CAP: i128 = 10_000;

/// Maximum items in a single `batch_receive_payment` call.
const MAX_BATCH: usize = 5;

/// Pool of developer addresses reused across a trace (keeps the index non-trivial).
const DEV_POOL_SIZE: usize = 4;

// ---------------------------------------------------------------------------
// Deterministic PRNG — no `std::rand`, no external crates in no_std context
// ---------------------------------------------------------------------------

/// 64-bit Multiplicative LCG (same constants as glibc).
struct Prng {
    state: u64,
}

impl Prng {
    fn new(seed: u64) -> Self {
        // Mix the seed so that seed=0 doesn't produce a trivial sequence.
        Self {
            state: seed.wrapping_add(0x9E37_79B9_7F4A_7C15),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    /// Uniform integer in `[min, max]` (inclusive).
    fn gen_i128(&mut self, min: i128, max: i128) -> i128 {
        if min >= max {
            return min;
        }
        let span = (max - min) as u64 + 1;
        min + (self.next_u64() % span) as i128
    }

    fn gen_usize(&mut self, min: usize, max: usize) -> usize {
        if min >= max {
            return min;
        }
        min + (self.next_u64() as usize) % (max - min + 1)
    }

    fn gen_bool(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }
}

// ---------------------------------------------------------------------------
// Trace — records every step for counterexample reporting
// ---------------------------------------------------------------------------

struct TraceStep {
    index: u32,
    op: &'static str,
    detail: std::string::String,
}

struct Trace {
    seed: u64,
    steps: std::vec::Vec<TraceStep>,
}

impl Trace {
    fn new(seed: u64) -> Self {
        Self {
            seed,
            steps: std::vec::Vec::new(),
        }
    }

    fn push(&mut self, index: u32, op: &'static str, detail: impl Into<std::string::String>) {
        self.steps.push(TraceStep {
            index,
            op,
            detail: detail.into(),
        });
    }

    /// Panic with a full human-readable counterexample.
    ///
    /// Prints:
    /// - The invariant that was violated
    /// - The failing values
    /// - The seed (for deterministic replay)
    /// - Every recorded step
    fn panic_invariant(
        &self,
        step: u32,
        expected_total_in: i128,
        actual_dev_sum: i128,
        pool_balance: i128,
    ) -> ! {
        let mut msg = std::format!(
            "\n=== INVARIANT VIOLATION ===\n\
             total_in ({expected_total_in}) != sum(dev_balances) ({actual_dev_sum}) + pool ({pool_balance})\n\
             combined rhs = {}\n\
             seed = {}  step = {step}\n\
             --- trace ---\n",
            actual_dev_sum + pool_balance,
            self.seed,
        );
        for s in &self.steps {
            msg.push_str(&std::format!(
                "  [{:>3}] {:30} {}\n",
                s.index, s.op, s.detail
            ));
        }
        msg.push_str("==========================\n");
        panic!("{msg}");
    }
}

// ---------------------------------------------------------------------------
// Test harness helpers
// ---------------------------------------------------------------------------

fn make_usdc<'a>(
    env: &'a Env,
    mint_to: &Address,
    amount: i128,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let admin = Address::generate(env);
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    let client = token::Client::new(env, &addr);
    let sac = token::StellarAssetClient::new(env, &addr);
    // Pre-fund the settlement contract so withdrawals can succeed.
    sac.mint(mint_to, &amount);
    (addr, client, sac)
}

fn setup_env() -> (
    Env,
    Address,          // contract address
    CalloraSettlementClient<'static>,
    Address,          // admin
    Address,          // vault
    Address,          // usdc token
    token::StellarAssetClient<'static>, // usdc SAC (for minting)
) {
    // SAFETY: We immediately tie the 'static lifetime to `env` via Box::leak.
    // The Env is leaked so the client can borrow it for the duration of the test.
    let env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let contract = env.register(CalloraSettlement, ());

    // Mint a large enough USDC reserve so withdrawals don't run out.
    let (usdc_addr, _usdc_client, _usdc_sac) = make_usdc(env, &contract, i128::MAX / 2);

    let client = CalloraSettlementClient::new(env, &contract);
    client.init(&admin, &vault);
    client.set_usdc_token(&admin, &usdc_addr);

    let usdc_sac_static: token::StellarAssetClient<'static> =
        token::StellarAssetClient::new(env, &usdc_addr);

    (
        (*env).clone(),
        contract,
        client,
        admin,
        vault,
        usdc_addr,
        usdc_sac_static,
    )
}

// ---------------------------------------------------------------------------
// Invariant checker
// ---------------------------------------------------------------------------

/// Verify `total_in_dev == sum(all developer balances)`.
///
/// The global pool is tracked separately; this checks only the developer side.
/// A full conservation check is: `total_in == dev_sum + pool`.
fn check_invariant(
    _env: &Env,
    client: &CalloraSettlementClient<'_>,
    admin: &Address,
    expected_dev_total: i128,
    expected_pool_total: i128,
    trace: &Trace,
    step: u32,
) {
    // Sum all developer balances via the paginated view.
    let balances = client.get_all_developer_balances(admin);
    let dev_sum: i128 = balances.iter().map(|b| b.balance).sum();
    let pool = client.get_global_pool().total_balance;

    // Developer balance sum must equal our running tally.
    if dev_sum != expected_dev_total || pool != expected_pool_total {
        trace.panic_invariant(step, expected_dev_total + expected_pool_total, dev_sum, pool);
    }
}

// ---------------------------------------------------------------------------
// Operation alphabet
// ---------------------------------------------------------------------------

#[repr(u8)]
enum Op {
    /// `receive_payment` → developer
    ReceiveDev = 0,
    /// `receive_payment` → pool
    ReceivePool = 1,
    /// `batch_receive_payment` (1–MAX_BATCH items, all developers)
    BatchReceiveDev = 2,
    /// `withdraw_developer_balance` (partial or full)
    Withdraw = 3,
}

const OP_COUNT: u64 = 4;

// ---------------------------------------------------------------------------
// Core trace runner
// ---------------------------------------------------------------------------

/// Run one fully deterministic property trace for `seed`.
///
/// Generates [`TRACE_LENGTH`] operations, checks the invariant after each one,
/// and panics with a full counterexample on any violation.
fn run_trace(seed: u64) {
    // Leak the Env so 'static clients work.
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let mut rng = Prng::new(seed);
    let mut trace = Trace::new(seed);

    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let contract = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(env, &contract);

    // Pre-fund contract with enough USDC to cover all possible withdrawals.
    let usdc_admin = Address::generate(env);
    let usdc_ca = env.register_stellar_asset_contract_v2(usdc_admin.clone());
    let usdc_addr = usdc_ca.address();
    let usdc_sac = token::StellarAssetClient::new(env, &usdc_addr);
    usdc_sac.mint(&contract, &(AMOUNT_CAP * TRACE_LENGTH as i128 * MAX_BATCH as i128 * 2));

    client.init(&admin, &vault);
    client.set_usdc_token(&admin, &usdc_addr);

    // Pre-generate a small pool of developer addresses to encourage balance accumulation.
    let devs: std::vec::Vec<Address> = (0..DEV_POOL_SIZE)
        .map(|_| Address::generate(env))
        .collect();

    // Running tallies — our "expected" state that must match contract storage.
    let mut expected_dev_total: i128 = 0;
    let mut expected_pool_total: i128 = 0;

    // Check invariant at t=0 (empty state).
    check_invariant(env, &client, &admin, 0, 0, &trace, 0);

    for step in 1..=TRACE_LENGTH {
        let op = (rng.next_u64() % OP_COUNT) as u8;

        match op {
            x if x == Op::ReceiveDev as u8 => {
                let dev = devs[rng.gen_usize(0, DEV_POOL_SIZE - 1)].clone();
                let amount = rng.gen_i128(1, AMOUNT_CAP);
                client.receive_payment(&vault, &amount, &false, &Some(dev.clone()));
                expected_dev_total = expected_dev_total
                    .checked_add(amount)
                    .expect("test tally overflow");
                trace.push(
                    step,
                    "receive_payment(dev)",
                    std::format!("dev={dev:?} amount={amount}"),
                );
            }

            x if x == Op::ReceivePool as u8 => {
                let amount = rng.gen_i128(1, AMOUNT_CAP);
                client.receive_payment(&vault, &amount, &true, &None);
                expected_pool_total = expected_pool_total
                    .checked_add(amount)
                    .expect("test tally overflow");
                trace.push(step, "receive_payment(pool)", std::format!("amount={amount}"));
            }

            x if x == Op::BatchReceiveDev as u8 => {
                let n = rng.gen_usize(1, MAX_BATCH);
                let mut items: Vec<(Address, i128)> = Vec::new(env);
                let mut batch_total: i128 = 0;
                for _ in 0..n {
                    let dev = devs[rng.gen_usize(0, DEV_POOL_SIZE - 1)].clone();
                    let amount = rng.gen_i128(1, AMOUNT_CAP);
                    items.push_back((dev, amount));
                    batch_total = batch_total.checked_add(amount).expect("batch tally overflow");
                }
                client.batch_receive_payment(&vault, &items);
                expected_dev_total = expected_dev_total
                    .checked_add(batch_total)
                    .expect("test tally overflow");
                trace.push(
                    step,
                    "batch_receive_payment",
                    std::format!("n={n} total={batch_total}"),
                );
            }

            x if x == Op::Withdraw as u8 => {
                // Pick a developer who has a positive balance.
                let dev = devs[rng.gen_usize(0, DEV_POOL_SIZE - 1)].clone();
                let current: i128 = client.get_developer_balance(&dev);
                if current > 0 {
                    let amount = rng.gen_i128(1, current.min(AMOUNT_CAP));
                    let result = client.try_withdraw_developer_balance(&dev, &amount, &None);
                    if result.is_ok() {
                        expected_dev_total = expected_dev_total
                            .checked_sub(amount)
                            .expect("test tally underflow");
                        trace.push(
                            step,
                            "withdraw(ok)",
                            std::format!("dev={dev:?} amount={amount} remaining={}", current - amount),
                        );
                    } else {
                        trace.push(
                            step,
                            "withdraw(err)",
                            std::format!("dev={dev:?} amount={amount} err={result:?}"),
                        );
                    }
                } else {
                    trace.push(
                        step,
                        "withdraw(skip-zero)",
                        std::format!("dev={dev:?}"),
                    );
                }
            }

            _ => unreachable!(),
        }

        check_invariant(
            env,
            &client,
            &admin,
            expected_dev_total,
            expected_pool_total,
            &trace,
            step,
        );
    }
}

// ---------------------------------------------------------------------------
// Tests — deterministic seeded traces
// ---------------------------------------------------------------------------

/// Run [`SEED_COUNT`] deterministic traces (seeds 0..63), each [`TRACE_LENGTH`] steps.
///
/// Invariant: `total_in == sum(per_developer_balance) + global_pool.total_balance`
/// must hold after every operation.
#[test]
fn test_settlement_balance_invariant_seeded() {
    for seed in 0..SEED_COUNT {
        run_trace(seed);
    }
}

/// Edge case: only pool credits — developer balances stay zero.
#[test]
fn test_invariant_pool_only() {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let contract = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(env, &contract);

    let usdc_admin = Address::generate(env);
    let ca = env.register_stellar_asset_contract_v2(usdc_admin.clone());
    let usdc_addr = ca.address();
    let sac = token::StellarAssetClient::new(env, &usdc_addr);
    sac.mint(&contract, &1_000_000);

    client.init(&admin, &vault);
    client.set_usdc_token(&admin, &usdc_addr);

    let amounts = [100i128, 200, 300, 50, 1];
    let mut expected_pool: i128 = 0;
    for (i, &amount) in amounts.iter().enumerate() {
        client.receive_payment(&vault, &amount, &true, &None);
        expected_pool += amount;
        let pool = client.get_global_pool().total_balance;
        assert_eq!(
            pool, expected_pool,
            "pool invariant failed at step {i}: expected {expected_pool}, got {pool}"
        );
        let dev_sum: i128 = client.get_all_developer_balances(&admin).iter().map(|b| b.balance).sum();
        assert_eq!(dev_sum, 0, "no developer should have a balance (step {i})");
    }
}

/// Edge case: single developer receives multiple payments then fully withdraws.
#[test]
fn test_invariant_single_dev_full_withdraw() {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let dev = Address::generate(env);
    let contract = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(env, &contract);

    let usdc_admin = Address::generate(env);
    let ca = env.register_stellar_asset_contract_v2(usdc_admin.clone());
    let usdc_addr = ca.address();
    let sac = token::StellarAssetClient::new(env, &usdc_addr);
    sac.mint(&contract, &10_000);

    client.init(&admin, &vault);
    client.set_usdc_token(&admin, &usdc_addr);

    // Credit the developer.
    client.receive_payment(&vault, &1_000, &false, &Some(dev.clone()));
    client.receive_payment(&vault, &2_000, &false, &Some(dev.clone()));
    client.receive_payment(&vault, &500, &false, &Some(dev.clone()));

    let balance = client.get_developer_balance(&dev);
    assert_eq!(balance, 3_500);

    let dev_sum: i128 = client.get_all_developer_balances(&admin).iter().map(|b| b.balance).sum();
    assert_eq!(dev_sum, 3_500, "dev sum before withdraw");

    // Full withdraw.
    client.withdraw_developer_balance(&dev, &3_500, &None);

    let dev_sum_after: i128 = client
        .get_all_developer_balances(&admin)
        .iter()
        .map(|b| b.balance)
        .sum();
    assert_eq!(dev_sum_after, 0, "dev sum must be 0 after full withdraw");
    assert_eq!(client.get_global_pool().total_balance, 0, "pool must stay 0");
}

/// Edge case: batch payments with duplicated developer in same batch accumulate correctly.
#[test]
fn test_invariant_batch_duplicate_dev() {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let dev = Address::generate(env);
    let contract = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(env, &contract);

    let usdc_admin = Address::generate(env);
    let ca = env.register_stellar_asset_contract_v2(usdc_admin.clone());
    let usdc_addr = ca.address();
    let sac = token::StellarAssetClient::new(env, &usdc_addr);
    sac.mint(&contract, &1_000_000);

    client.init(&admin, &vault);
    client.set_usdc_token(&admin, &usdc_addr);

    // Same developer appears twice in batch → balance should be 300.
    let mut items: Vec<(Address, i128)> = Vec::new(env);
    items.push_back((dev.clone(), 100));
    items.push_back((dev.clone(), 200));
    client.batch_receive_payment(&vault, &items);

    let dev_sum: i128 = client.get_all_developer_balances(&admin).iter().map(|b| b.balance).sum();
    assert_eq!(dev_sum, 300, "batch duplicate dev: expected 300, got {dev_sum}");
    assert_eq!(client.get_developer_balance(&dev), 300);
}

/// Edge case: interleaved developer and pool payments preserve the full conservation invariant.
#[test]
fn test_invariant_interleaved_dev_and_pool() {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let dev1 = Address::generate(env);
    let dev2 = Address::generate(env);
    let contract = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(env, &contract);

    let usdc_admin = Address::generate(env);
    let ca = env.register_stellar_asset_contract_v2(usdc_admin.clone());
    let usdc_addr = ca.address();
    let sac = token::StellarAssetClient::new(env, &usdc_addr);
    sac.mint(&contract, &1_000_000);

    client.init(&admin, &vault);
    client.set_usdc_token(&admin, &usdc_addr);

    let ops: &[(bool, i128, bool)] = &[
        // (to_pool, amount, is_dev1)
        (false, 100, true),
        (true, 50, false),
        (false, 200, false),
        (true, 75, false),
        (false, 300, true),
    ];

    let mut exp_dev: i128 = 0;
    let mut exp_pool: i128 = 0;

    for &(to_pool, amount, is_dev1) in ops {
        if to_pool {
            client.receive_payment(&vault, &amount, &true, &None);
            exp_pool += amount;
        } else {
            let dev = if is_dev1 { dev1.clone() } else { dev2.clone() };
            client.receive_payment(&vault, &amount, &false, &Some(dev));
            exp_dev += amount;
        }
        let dev_sum: i128 = client.get_all_developer_balances(&admin).iter().map(|b| b.balance).sum();
        let pool = client.get_global_pool().total_balance;
        assert_eq!(dev_sum, exp_dev, "dev sum mismatch");
        assert_eq!(pool, exp_pool, "pool mismatch");
    }
}

// ---------------------------------------------------------------------------
// proptest — seed-driven property test (satisfies issue #489 requirement)
// ---------------------------------------------------------------------------

proptest! {
    /// Property: for any seed in [0, u32::MAX], the settlement balance invariant holds
    /// across [`TRACE_LENGTH`] generated operations.
    ///
    /// proptest manages seed shrinking: on failure it finds the minimal seed that
    /// reproduces the violation, then `run_trace` provides the full step trace.
    #[test]
    fn proptest_settlement_balance_invariant(seed in 0u64..=u64::from(u32::MAX)) {
        run_trace(seed);
    }
}
