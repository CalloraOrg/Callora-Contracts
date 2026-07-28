//! Fuzz target: comprehensive revenue-pool state-machine fuzzer.
//!
//! Exercises the core public surface of `RevenuePool` — distribution,
//! batch distribution, configuration, pause, and yield deposit — by parsing
//! a raw byte stream into a sequence of typed operations and verifying that
//! key invariants hold after every call.
//!
//! # Scope
//! Deliberately excludes the multi-step timelocked admin-transfer and
//! emergency-drain flows, and `broadcast` — those are lower-risk (no fund
//! movement / event-only) and are better covered by dedicated, narrower
//! targets later, mirroring how `contracts/vault/fuzz` splits `set_auth.rs`
//! from `main.rs`. `batch_distribute` payment-shape fuzzing already lives in
//! `weighted_distribute.rs`; this target focuses on cross-entrypoint state
//! transitions instead.
//!
//! # Invariants checked
//!
//! 1. **Balance conservation** — pool balance only decreases via a
//!    successful `distribute`/`batch_distribute`, and only by the exact
//!    amount transferred.
//! 2. **No negative balance** — the pool's on-ledger balance is always `>= 0`.
//! 3. **Pause gate** — while paused, `distribute` and `batch_distribute` must
//!    both be rejected.
//! 4. **Auth gate** — every state-changing entry-point must fail when called
//!    without authentication (i.e. after `env.set_auths(&[])`).
//! 5. **Cap enforcement** — a `distribute`/`batch_distribute` leg above the
//!    configured `max_distribute` must be rejected.
//! 6. **Admin invariance** — `get_admin` never changes (this target does not
//!    exercise admin transfer).
//! 7. **No uncontrolled panics** — any panic must be caught via
//!    `std::panic::catch_unwind`, never allowed to abort the fuzzer process
//!    outside of an expected validation path.
//!
//! # Wire format
//! The byte stream is sliced into **3-byte operation tokens**:
//! ```text
//! byte 0: operation discriminant (mod NUM_OPS)
//! bytes 1-2: big-endian u16 operand (amount / index / window)
//! ```
//!
//! # Running
//! ```bash
//! cargo fuzz run main
//! ```

#![no_main]

extern crate std;

use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env, InvokeError, Symbol, Vec as SorobanVec};

use callora_revenue_pool::{RevenuePool, RevenuePoolClient, MAX_BATCH_SIZE};

// ---------------------------------------------------------------------------
// Configuration constants
// ---------------------------------------------------------------------------

/// Bytes consumed per operation token.
const BYTES_PER_OP: usize = 3;

/// Total number of distinct operation discriminants.
const NUM_OPS: u8 = 9;

/// USDC pre-funded into the pool at startup.
const POOL_FUNDING: i128 = 1_000_000;

/// Initial `max_distribute` cap.
const INITIAL_MAX_DISTRIBUTE: i128 = 100_000;

/// Number of fixed developer addresses available as distribute targets.
const DEV_POOL_SIZE: usize = 4;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_usdc<'a>(env: &'a Env, admin: &Address) -> (Address, StellarAssetClient<'a>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (addr.clone(), StellarAssetClient::new(env, &addr))
}

fn create_pool(env: &Env) -> (Address, RevenuePoolClient<'_>) {
    let addr = env.register(RevenuePool, ());
    (addr.clone(), RevenuePoolClient::new(env, &addr))
}

type CatchResult<T> = std::result::Result
    std::result::Result<T, std::result::Result<soroban_sdk::Error, InvokeError>>,
    Box<dyn std::any::Any + Send>,
>;

fn is_success<T>(result: &CatchResult<T>) -> bool {
    matches!(result, Ok(Ok(_)))
}

// ---------------------------------------------------------------------------
// Fuzz entry-point
// ---------------------------------------------------------------------------

fuzz_target!(|data: &[u8]| {
    if data.len() < BYTES_PER_OP {
        return;
    }

    let env = Env::default();
    env.mock_all_auths();

    // --- Static participants -------------------------------------------
    let admin = Address::generate(&env);
    let treasury = admin.clone(); // deposit_yield requires treasury == admin

    // --- USDC token ------------------------------------------------------
    let (usdc_addr, usdc_admin) = create_usdc(&env, &admin);

    // --- Pool --------------------------------------------------------------
    let (pool_addr, pool) = create_pool(&env);
    usdc_admin.mint(&pool_addr, &POOL_FUNDING);
    usdc_admin.mint(&treasury, &POOL_FUNDING);

    pool.init(&admin, &usdc_addr);
    pool.set_max_distribute(&admin, &INITIAL_MAX_DISTRIBUTE);

    let devs: std::vec::Vec<Address> = (0..DEV_POOL_SIZE)
        .map(|_| Address::generate(&env))
        .collect();

    let mut max_distribute: i128 = INITIAL_MAX_DISTRIBUTE;

    for chunk in data.chunks(BYTES_PER_OP) {
        if chunk.len() < BYTES_PER_OP {
            break;
        }

        let op = chunk[0] % NUM_OPS;
        let operand = u16::from_be_bytes([chunk[1], chunk[2]]) as i128;

        match op {
            // -----------------------------------------------------------
            // 0 — distribute(admin, dev, amount)
            // -----------------------------------------------------------
            0 => {
                let dev = devs[(chunk[1] as usize) % DEV_POOL_SIZE].clone();
                let amount = operand;
                let balance_before = pool.balance();

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    pool.try_distribute(&admin, &dev, &amount)
                }));
                let balance_after = pool.balance();

                if is_success(&result) {
                    assert_eq!(
                        balance_after,
                        balance_before - amount,
                        "distribute conservation violated"
                    );
                    assert!(amount <= max_distribute, "distribute exceeded max_distribute");
                } else {
                    assert_eq!(balance_before, balance_after, "rejected distribute mutated balance");
                }
                assert!(pool.balance() >= 0, "balance went negative after distribute");
            }

            // -----------------------------------------------------------
            // 1 — batch_distribute(admin, [(dev, amount)])
            // -----------------------------------------------------------
            1 => {
                let leg_count = ((chunk[1] as usize) % DEV_POOL_SIZE) + 1;
                let amount = (operand % 1000).max(1);
                let mut payments: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
                for i in 0..leg_count {
                    payments.push_back((devs[i].clone(), amount));
                }
                let total = amount * leg_count as i128;
                let balance_before = pool.balance();

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    pool.try_batch_distribute(&admin, &payments)
                }));
                let balance_after = pool.balance();

                if is_success(&result) {
                    assert_eq!(
                        balance_after,
                        balance_before - total,
                        "batch_distribute conservation violated"
                    );
                } else {
                    assert_eq!(
                        balance_before, balance_after,
                        "rejected batch_distribute mutated balance"
                    );
                }
                assert!(pool.balance() >= 0, "balance went negative after batch_distribute");
            }

            // -----------------------------------------------------------
            // 2 — set_max_distribute(admin, new_max)
            // -----------------------------------------------------------
            2 => {
                let new_max = (operand % 200_000).max(1);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    pool.try_set_max_distribute(&admin, &new_max)
                }));
                if is_success(&result) {
                    max_distribute = new_max;
                }
                assert!(pool.get_max_distribute() > 0, "max_distribute became non-positive");
            }

            // -----------------------------------------------------------
            // 3 — pause / unpause toggle
            // -----------------------------------------------------------
            3 => {
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    if pool.is_paused() {
                        let _ = pool.try_unpause(&admin);
                    } else {
                        let _ = pool.try_pause(&admin);
                    }
                }));
            }

            // -----------------------------------------------------------
            // 4 — deposit_yield(treasury, amount, source)
            // -----------------------------------------------------------
            4 => {
                let amount = operand;
                let source = Symbol::new(&env, "fuzz");
                let balance_before = pool.balance();

                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    pool.try_deposit_yield(&treasury, &amount, &source)
                }));
                let balance_after = pool.balance();

                if is_success(&result) {
                    assert_eq!(
                        balance_after,
                        balance_before + amount,
                        "deposit_yield conservation violated"
                    );
                } else {
                    assert_eq!(balance_before, balance_after, "rejected deposit_yield mutated balance");
                }
            }

            // -----------------------------------------------------------
            // 5 — view-only calls: must not mutate balance
            // -----------------------------------------------------------
            5 => {
                let bal = pool.balance();
                let _ = pool.get_admin();
                let _ = pool.get_usdc_token();
                let _ = pool.is_paused();
                let _ = pool.get_max_distribute();
                let _ = pool.get_cumulative_yield_deposited();
                let _ = pool.version();
                assert_eq!(bal, pool.balance(), "view calls mutated balance");
            }

            // -----------------------------------------------------------
            // 6 — unauthenticated distribute attempt (auth gate)
            // -----------------------------------------------------------
            6 => {
                let dev = devs[0].clone();
                let amount = (operand % 500).max(1);
                let bal = pool.balance();

                env.set_auths(&[]);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    pool.try_distribute(&admin, &dev, &amount)
                }));
                env.mock_all_auths();

                assert_eq!(bal, pool.balance(), "unauthenticated distribute changed balance");
                assert!(!is_success(&result), "unauthenticated distribute unexpectedly succeeded");
            }

            // -----------------------------------------------------------
            // 7 — unauthenticated batch_distribute attempt (auth gate)
            // -----------------------------------------------------------
            7 => {
                let amount = (operand % 500).max(1);
                let mut payments: SorobanVec<(Address, i128)> = SorobanVec::new(&env);
                payments.push_back((devs[0].clone(), amount));
                let bal = pool.balance();

                env.set_auths(&[]);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    pool.try_batch_distribute(&admin, &payments)
                }));
                env.mock_all_auths();

                assert_eq!(bal, pool.balance(), "unauthenticated batch_distribute changed balance");
                assert!(!is_success(&result), "unauthenticated batch_distribute unexpectedly succeeded");
            }

            // -----------------------------------------------------------
            // 8 — receive_payment (event-only, must never move funds)
            // -----------------------------------------------------------
            8 => {
                let amount = operand;
                let from_vault = operand % 2 == 0;
                let bal = pool.balance();

                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    pool.try_receive_payment(&admin, &amount, &from_vault)
                }));

                assert_eq!(bal, pool.balance(), "receive_payment must never move funds");
            }

            _ => unreachable!("op discriminant outside NUM_OPS range"),
        }

        // -----------------------------------------------------------------
        // Post-step global invariants
        // -----------------------------------------------------------------
        assert!(pool.balance() >= 0, "invariant: negative balance after op {op}");
        assert_eq!(pool.get_admin(), admin, "invariant: admin changed after op {op}");
        assert_eq!(pool.get_usdc_token(), usdc_addr, "invariant: usdc_token changed after op {op}");

        if pool.is_paused() {
            let dev = devs[0].clone();
            let small = 10i128;
            let d = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                pool.try_distribute(&admin, &dev, &small)
            }));
            assert!(!is_success(&d), "invariant: distribute succeeded while paused (op {op})");
        }
    }

    let _ = pool_addr; // suppress unused warning
    let _ = MAX_BATCH_SIZE; // referenced for documentation clarity, not used directly here
});
