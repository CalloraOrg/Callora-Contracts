//! cargo-fuzz target: hammer freeze (pause circuit-breaker) with malformed inputs.
//!
//! # Properties checked on every execution
//! 1. **Pause bit integrity** — `is_paused` is always a coherent boolean after
//!    every op; unauthorized freeze/unfreeze never flips it.
//! 2. **Frozen distribute rejection** — while frozen, `distribute` must not
//!    succeed; pool USDC balance is unchanged.
//! 3. **Malformed amounts** — zero / negative distribute amounts never succeed.
//! 4. **No unexpected abort** — expected panics from auth / already-paused /
//!    not-paused paths are caught; the harness itself must not unwind.
//!
//! # Running
//! ```bash
//! cargo fuzz run main
//! # or, from workspace:
//! cargo test -p callora-freeze
//! ```
//!
//! Closes CalloraOrg/Callora-Contracts#710.

#![no_main]

extern crate std;

use std::panic::{catch_unwind, AssertUnwindSafe};

use callora_freeze::{FreezeOp, MAX_FREEZE_OPS};
use callora_revenue_pool::{RevenuePool, RevenuePoolClient};
use libfuzzer_sys::fuzz_target;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env};

const POOL_FUNDING: i128 = 1_000_000;

fn run_sequence(ops: &[FreezeOp]) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let guardian = Address::generate(&env);
    let outsider = Address::generate(&env);
    let recipient = Address::generate(&env);

    let pool_addr = env.register(RevenuePool, ());
    let pool = RevenuePoolClient::new(&env, &pool_addr);

    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let usdc_admin = StellarAssetClient::new(&env, &usdc_addr);
    let usdc = soroban_sdk::token::Client::new(&env, &usdc_addr);

    pool.init(&admin, &usdc_addr);
    usdc_admin.mint(&pool_addr, &POOL_FUNDING);

    let mut guardian_set = false;
    let mut paused = false;

    for op in ops {
        match *op {
            FreezeOp::FreezeAsAdmin => {
                let before = pool.is_paused();
                let result = catch_unwind(AssertUnwindSafe(|| pool.pause(&admin)));
                if result.is_ok() {
                    paused = true;
                    assert!(pool.is_paused());
                } else {
                    // Already paused, or unexpected — pause bit must be unchanged.
                    assert_eq!(pool.is_paused(), before);
                    assert_eq!(pool.is_paused(), paused);
                }
            }
            FreezeOp::FreezeAsGuardian => {
                let before = pool.is_paused();
                if !guardian_set {
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        pool.set_pause_guardian(&admin, &guardian);
                    }));
                    guardian_set = true;
                }
                let result = catch_unwind(AssertUnwindSafe(|| pool.pause(&guardian)));
                if result.is_ok() {
                    paused = true;
                    assert!(pool.is_paused());
                } else {
                    assert_eq!(pool.is_paused(), before);
                    assert_eq!(pool.is_paused(), paused);
                }
            }
            FreezeOp::FreezeAsOutsider => {
                let before = pool.is_paused();
                let result = catch_unwind(AssertUnwindSafe(|| pool.pause(&outsider)));
                assert!(
                    result.is_err(),
                    "outsider must never successfully freeze the pool"
                );
                assert_eq!(pool.is_paused(), before);
                assert_eq!(pool.is_paused(), paused);
            }
            FreezeOp::UnfreezeAsAdmin => {
                let before = pool.is_paused();
                let result = catch_unwind(AssertUnwindSafe(|| pool.unpause(&admin)));
                if result.is_ok() {
                    paused = false;
                    assert!(!pool.is_paused());
                } else {
                    assert_eq!(pool.is_paused(), before);
                    assert_eq!(pool.is_paused(), paused);
                }
            }
            FreezeOp::UnfreezeAsOutsider => {
                let before = pool.is_paused();
                let result = catch_unwind(AssertUnwindSafe(|| pool.unpause(&outsider)));
                assert!(
                    result.is_err(),
                    "outsider must never successfully unfreeze the pool"
                );
                assert_eq!(pool.is_paused(), before);
                assert_eq!(pool.is_paused(), paused);
            }
            FreezeOp::Distribute { amount } => {
                let bal_before = usdc.balance(&pool_addr);
                let result =
                    catch_unwind(AssertUnwindSafe(|| pool.distribute(&admin, &recipient, &amount)));
                let bal_after = usdc.balance(&pool_addr);

                if paused || amount <= 0 {
                    assert!(
                        result.is_err(),
                        "distribute must fail when frozen or amount is non-positive \
                         (paused={paused}, amount={amount})"
                    );
                    assert_eq!(
                        bal_before, bal_after,
                        "failed distribute must not move pool USDC"
                    );
                } else if result.is_ok() {
                    assert_eq!(bal_after, bal_before - amount);
                    assert!(!pool.is_paused());
                } else {
                    // Insufficient balance / max-cap / other validation — no move.
                    assert_eq!(bal_before, bal_after);
                }
                assert_eq!(pool.is_paused(), paused);
            }
            FreezeOp::SetGuardian => {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    pool.set_pause_guardian(&admin, &guardian);
                }));
                guardian_set = true;
                assert_eq!(pool.is_paused(), paused);
            }
            FreezeOp::ClearGuardian => {
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    pool.clear_pause_guardian(&admin);
                }));
                guardian_set = false;
                assert_eq!(pool.is_paused(), paused);
            }
        }
    }

    // Final coherence: model matches on-chain pause bit.
    assert_eq!(pool.is_paused(), paused);
}

fuzz_target!(|data: &[u8]| {
    let ops = FreezeOp::decode_sequence(data, MAX_FREEZE_OPS);
    if ops.is_empty() {
        return;
    }
    run_sequence(&ops);
});
