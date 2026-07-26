//! Focused freeze tests with deliberately malformed inputs (no libFuzzer required).

extern crate std;

use callora_freeze::{FreezeOp, MAX_FREEZE_OPS};
use callora_revenue_pool::{RevenuePool, RevenuePoolClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::StellarAssetClient;
use soroban_sdk::{Address, Env};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn setup(env: &Env) -> (Address, Address, Address, Address, RevenuePoolClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let outsider = Address::generate(env);
    let recipient = Address::generate(env);
    let pool_addr = env.register(RevenuePool, ());
    let pool = RevenuePoolClient::new(env, &pool_addr);
    let usdc = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    StellarAssetClient::new(env, &usdc).mint(&pool_addr, &1_000_000);
    pool.init(&admin, &usdc);
    (admin, outsider, recipient, pool_addr, pool)
}

#[test]
fn outsider_cannot_freeze() {
    let env = Env::default();
    let (admin, outsider, _, _, pool) = setup(&env);
    assert!(!pool.is_paused());
    assert!(catch_unwind(AssertUnwindSafe(|| pool.pause(&outsider))).is_err());
    assert!(!pool.is_paused());
    // Admin can still freeze afterwards.
    pool.pause(&admin);
    assert!(pool.is_paused());
}

#[test]
fn freeze_blocks_distribute_even_with_valid_amount() {
    let env = Env::default();
    let (admin, _, recipient, pool_addr, pool) = setup(&env);
    let usdc_addr = pool.get_usdc_token();
    let usdc = soroban_sdk::token::Client::new(&env, &usdc_addr);

    pool.pause(&admin);
    let before = usdc.balance(&pool_addr);
    assert!(catch_unwind(AssertUnwindSafe(|| pool.distribute(&admin, &recipient, &100))).is_err());
    assert_eq!(usdc.balance(&pool_addr), before);
}

#[test]
fn malformed_zero_and_negative_distribute_rejected() {
    let env = Env::default();
    let (admin, _, recipient, pool_addr, pool) = setup(&env);
    let usdc_addr = pool.get_usdc_token();
    let usdc = soroban_sdk::token::Client::new(&env, &usdc_addr);
    let before = usdc.balance(&pool_addr);

    assert!(catch_unwind(AssertUnwindSafe(|| pool.distribute(&admin, &recipient, &0))).is_err());
    assert!(catch_unwind(AssertUnwindSafe(|| pool.distribute(&admin, &recipient, &-1))).is_err());
    assert_eq!(usdc.balance(&pool_addr), before);
}

#[test]
fn double_freeze_is_rejected_without_clearing_state() {
    let env = Env::default();
    let (admin, _, _, _, pool) = setup(&env);
    pool.pause(&admin);
    assert!(pool.is_paused());
    assert!(catch_unwind(AssertUnwindSafe(|| pool.pause(&admin))).is_err());
    assert!(pool.is_paused());
}

#[test]
fn decode_sequence_covers_malformed_byte_streams() {
    let empty = FreezeOp::decode_sequence(&[], MAX_FREEZE_OPS);
    assert!(empty.is_empty());

    // Truncated trailing bytes still produce ops.
    let ops = FreezeOp::decode_sequence(&[0x00, 0xff], MAX_FREEZE_OPS);
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0], FreezeOp::FreezeAsAdmin);

    // Long input is capped.
    let long = vec![0u8; 300];
    let capped = FreezeOp::decode_sequence(&long, MAX_FREEZE_OPS);
    assert!(capped.len() <= MAX_FREEZE_OPS);
}

#[test]
fn fuzz_style_sequence_with_malformed_mix() {
    let env = Env::default();
    let (admin, outsider, recipient, pool_addr, pool) = setup(&env);
    let usdc_addr = pool.get_usdc_token();
    let usdc = soroban_sdk::token::Client::new(&env, &usdc_addr);

    let ops = [
        FreezeOp::FreezeAsOutsider,
        FreezeOp::Distribute { amount: 0 },
        FreezeOp::Distribute { amount: -42 },
        FreezeOp::FreezeAsAdmin,
        FreezeOp::Distribute { amount: 50 },
        FreezeOp::UnfreezeAsOutsider,
        FreezeOp::UnfreezeAsAdmin,
        FreezeOp::Distribute { amount: 25 },
        FreezeOp::FreezeAsAdmin,
        FreezeOp::FreezeAsAdmin, // double freeze
    ];

    let mut paused = false;
    for op in ops {
        match op {
            FreezeOp::FreezeAsOutsider => {
                assert!(catch_unwind(AssertUnwindSafe(|| pool.pause(&outsider))).is_err());
            }
            FreezeOp::FreezeAsAdmin => {
                let r = catch_unwind(AssertUnwindSafe(|| pool.pause(&admin)));
                if r.is_ok() {
                    paused = true;
                }
            }
            FreezeOp::UnfreezeAsOutsider => {
                assert!(catch_unwind(AssertUnwindSafe(|| pool.unpause(&outsider))).is_err());
            }
            FreezeOp::UnfreezeAsAdmin => {
                let r = catch_unwind(AssertUnwindSafe(|| pool.unpause(&admin)));
                if r.is_ok() {
                    paused = false;
                }
            }
            FreezeOp::Distribute { amount } => {
                let before = usdc.balance(&pool_addr);
                let r = catch_unwind(AssertUnwindSafe(|| {
                    pool.distribute(&admin, &recipient, &amount)
                }));
                if paused || amount <= 0 {
                    assert!(r.is_err());
                    assert_eq!(usdc.balance(&pool_addr), before);
                }
            }
            _ => {}
        }
        assert_eq!(pool.is_paused(), paused);
    }
}
