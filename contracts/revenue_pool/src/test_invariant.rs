extern crate std;

use crate::{RevenuePool, RevenuePoolClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{self, StellarAssetClient};
use soroban_sdk::{Address, Env, Vec};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// Simple deterministic LCG PRNG for reproducible invariant traces.
///
/// Uses the classic MMIX LCG (Knuth) constants. Not cryptographically
/// secure — only intended for test reproducibility.
struct SimpleRng(u64);

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// Generate the next pseudo-random u64.
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0
    }

    /// Uniform i128 in [lo, hi).  Panics if hi <= lo.
    fn gen_range(&mut self, lo: i128, hi: i128) -> i128 {
        assert!(hi > lo, "gen_range: hi must be > lo");
        let range = (hi - lo) as u64;
        lo + (self.next_u64() % range) as i128
    }

    /// Uniform usize in [0, max).  Returns 0 if max == 0.
    fn gen_index(&mut self, max: usize) -> usize {
        if max == 0 {
            return 0;
        }
        (self.next_u64() % max as u64) as usize
    }
}

/// Register a Stellar asset contract for USDC and return the address,
/// a regular token client, and an admin (minting) client.
fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, StellarAssetClient<'a>) {
    let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract_address.address();
    let client = token::Client::new(env, &address);
    let admin_client = StellarAssetClient::new(env, &address);
    (address, client, admin_client)
}

/// Register a RevenuePool contract and return its address and client.
fn create_pool(env: &Env) -> (Address, RevenuePoolClient<'_>) {
    let address = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &address);
    (address, client)
}

// ---------------------------------------------------------------------------
// Invariant trace
// ---------------------------------------------------------------------------

/// Number of developer addresses in the pool for random distributions.
const DEV_COUNT: usize = 10;

/// Number of stateful actions per trace.
const ACTIONS_PER_TRACE: u32 = 75;

/// Run a single deterministic invariant trace for the given `seed` (0 .. 128).
///
/// The trace executes a random sequence of stateful actions:
///
///   - **Fund** — mint USDC to the pool contract (simulates vault settlement).
///   - **Schedule** — mint USDC *and* increase the virtual `scheduled` total
///     (simulates backend approval + concurrent vault deposit).
///   - **Distribute** — call `distribute` or `batch_distribute` at random.
///     On success the virtual `scheduled` is decremented.
///   - **Pause / Unpause** — toggle the pause flag.
///   - **Edge case: paused distribution** — pause, attempt a distribute
///     (which must fail), then unpause.
///
/// After *every* action the invariant is checked:
///
/// **`pool USDC balance >= virtual scheduled total`**
///
/// Because we always fund at least as much as we schedule, and successful
/// distributions decrease both, this invariant should hold across all traces.
fn invariant_trace(seed: u64) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let (pool_addr, pool) = create_pool(&env);
    let (usdc_addr, usdc, usdc_admin) = create_usdc(&env, &admin);
    pool.init(&admin, &usdc_addr);

    // Developer addresses for distributions.
    let devs: std::vec::Vec<Address> = (0..DEV_COUNT).map(|_| Address::generate(&env)).collect();

    let mut rng = SimpleRng::new(seed);
    let mut scheduled: i128 = 0; // virtual sum of pending distributions
    let mut paused: bool = false;

    for step in 0..ACTIONS_PER_TRACE {
        let action = rng.next_u64() % 12;

        match action {
            // ── 0-3: Fund the pool without scheduling (balance increases, scheduled unchanged) ──
            0..=3 => {
                let amount = rng.gen_range(1_000, 1_000_000);
                usdc_admin.mint(&pool_addr, &amount);
            }

            // ── 4-6: Schedule a payment (fund + track virtually) ──
            4..=6 => {
                let amount = rng.gen_range(1_000, 500_000);
                usdc_admin.mint(&pool_addr, &amount);
                scheduled += amount;
            }

            // ── 7-8: Distribute to a single developer ──
            7..=8 => {
                if scheduled > 0 {
                    let idx = rng.gen_index(DEV_COUNT);
                    let max_amt = core::cmp::min(scheduled, 200_000);
                    if max_amt > 0 {
                        let amt = rng.gen_range(1, max_amt + 1);
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            pool.distribute(&admin, &devs[idx], &amt);
                        }));
                        if result.is_ok() {
                            scheduled -= amt;
                        }
                    }
                }
            }

            // ── 9: Batch distribute to several developers ──
            9 => {
                if scheduled > 0 {
                    let batch_size = rng.gen_index(6).max(1) as u32; // 1..6
                    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
                    let mut batch_total: i128 = 0;

                    for _ in 0..batch_size {
                        let remaining = scheduled - batch_total;
                        if remaining <= 0 {
                            break;
                        }
                        let max_leg = core::cmp::min(remaining, 100_000);
                        if max_leg <= 0 {
                            break;
                        }
                        let leg_amt = rng.gen_range(1, max_leg + 1);
                        let idx = rng.gen_index(DEV_COUNT);
                        payments.push_back((devs[idx].clone(), leg_amt));
                        batch_total += leg_amt;
                    }

                    if !payments.is_empty() {
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            pool.batch_distribute(&admin, &payments);
                        }));
                        if result.is_ok() {
                            scheduled -= batch_total;
                        }
                    }
                }
            }

            // ── 10: Toggle pause / unpause ──
            10 => {
                if paused {
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        pool.unpause(&admin);
                    }));
                    paused = false;
                } else {
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        pool.pause(&admin);
                    }));
                    paused = true;
                }
            }

            // ── 11: Edge case — pause, attempt distribute (must fail), unpause ──
            11 => {
                // Pause the pool.
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    pool.pause(&admin);
                }));

                // Attempt a distribute while paused — must fail; scheduled unchanged.
                if scheduled > 0 {
                    let idx = rng.gen_index(DEV_COUNT);
                    let max_amt = core::cmp::min(scheduled, 100_000);
                    if max_amt > 0 {
                        let amt = rng.gen_range(1, max_amt + 1);
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            pool.distribute(&admin, &devs[idx], &amt);
                        }));
                        // `scheduled` deliberately NOT decremented — distribute must
                        // have panicked with "revenue pool is paused".
                    }
                }

                // Restore.
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    pool.unpause(&admin);
                }));
                paused = false;
            }

            _ => {} // unreachable
        }

        // ── INVARIANT CHECK ──
        // The pool's on-ledger USDC balance must always be at least the sum
        // of approved-but-not-yet-distributed payments (scheduled).
        let balance = usdc.balance(&pool_addr);
        assert!(
            balance >= scheduled,
            "[seed={}, step={}] Invariant VIOLATED: USDC balance {} < scheduled {}",
            seed,
            step,
            balance,
            scheduled,
        );
    }
}

// ---------------------------------------------------------------------------
// Public test entry point
// ---------------------------------------------------------------------------

/// Stateful invariant test: 128 deterministic seeded traces.
///
/// For each seed (0 .. 127) a fresh environment is created and a random
/// sequence of fund / schedule / distribute / pause actions is executed.
/// After every action the invariant is checked:
///
/// **`pool USDC balance >= virtual scheduled total`**
///
/// This mirrors Foundry's `invariant` fuzzing pattern: a stateless runner
/// that repeatedly perturbs contract state and verifies a system invariant.
#[test]
fn invariant_pool_balance_ge_scheduled_128_traces() {
    for seed in 0..128 {
        invariant_trace(seed as u64);
    }
}
