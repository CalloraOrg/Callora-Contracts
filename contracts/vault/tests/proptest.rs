//! Property-based invariant tests for the Callora Vault contract (#647).
//!
//! # Invariant
//! After every operation — successful or rejected — the vault's internally
//! tracked ledger balance (`CalloraVault::balance`) must equal the actual
//! on-chain USDC balance held by the vault contract address, and must never
//! go negative. The vault's current API has no `withdraw`/`distribute`
//! entrypoints, so `deposit` and `deduct`/`batch_deduct` are the only two
//! (symmetric) fund-movement paths in and out of the vault: every successful
//! call moves the exact same `amount` through both the token ledger and the
//! `DataKey::Balance` counter, and every rejected call (validation panic,
//! insufficient balance, or paused state) must leave both untouched. This
//! suite asserts that pairing never drifts apart across randomized sequences
//! of operations, including every validation-rejection branch.
//!
//! # Generator
//! A deterministic xorshift64 PRNG (no external `proptest`/`rand` dependency
//! needed) drives `SEED_COUNT` independent traces of `TRACE_LEN` randomly
//! chosen operations each, so any failure is reproducible from the printed
//! `seed`/`step`.
//!
//! # Scope note
//! `contracts/vault/src/lib.rs` currently exposes a simplified vault API
//! (no `Option<Symbol>` request-id dedup, no `withdraw`/`distribute`, plain
//! `u64` request ids forwarded to a no-op settlement stub in native/test
//! builds). This suite is written against that real, current API rather
//! than the richer API assumed by the pre-existing (unwired) test files in
//! `contracts/vault/src/test_balance_property.rs` and
//! `test_cross_invariant.rs`.

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, Vec};

use callora_vault::{CalloraVault, CalloraVaultClient};

const SEED_COUNT: u64 = 48;
const TRACE_LEN: u32 = 40;

const INITIAL_BALANCE: i128 = 1_000_000;
const MIN_DEPOSIT: i128 = 100;
const MAX_DEDUCT: i128 = 10_000;

// ---------------------------------------------------------------------------
// Deterministic PRNG (xorshift64) — no external dependency, fully reproducible.
// ---------------------------------------------------------------------------

struct Prng(u64);

impl Prng {
    fn new(seed: u64) -> Self {
        // Avoid a zero state, which would stall xorshift.
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

    /// Inclusive range.
    fn range_i128(&mut self, lo: i128, hi: i128) -> i128 {
        if lo >= hi {
            return lo;
        }
        let span = (hi - lo) as u64 + 1;
        lo + (self.next_u64() % span) as i128
    }

    fn range_usize(&mut self, lo: usize, hi: usize) -> usize {
        if lo >= hi {
            return lo;
        }
        lo + (self.next_u64() as usize) % (hi - lo)
    }
}

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

/// Registers a fresh vault + USDC token pair and initializes the vault with
/// `initial_balance` tokens pre-minted on-ledger so the tracked/on-ledger
/// invariant holds from step 0.
fn setup<'a>(
    env: &'a Env,
    initial_balance: i128,
    min_deposit: i128,
    max_deduct: i128,
) -> (
    CalloraVaultClient<'a>,
    Address, // owner
    Address, // authorized_caller
    token::Client<'a>,
    token::StellarAssetClient<'a>,
) {
    let owner = Address::generate(env);
    let authorized_caller = Address::generate(env);
    let settlement = Address::generate(env);

    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_addr);

    let usdc_addr = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    let usdc_client = token::Client::new(env, &usdc_addr);
    let usdc_admin = token::StellarAssetClient::new(env, &usdc_addr);

    if initial_balance > 0 {
        usdc_admin.mint(&vault_addr, &initial_balance);
    }

    client.init(
        &owner,
        &usdc_addr,
        &initial_balance,
        &authorized_caller,
        &min_deposit,
        &None,
        &max_deduct,
        &settlement,
    );

    (client, owner, authorized_caller, usdc_client, usdc_admin)
}

/// Assert the core invariant: tracked balance == on-ledger USDC, and never negative.
fn assert_balance_in_sync(
    client: &CalloraVaultClient<'_>,
    usdc: &token::Client<'_>,
    seed: u64,
    step: u32,
) {
    let tracked = client.balance();
    let on_ledger = usdc.balance(&client.address);
    assert_eq!(
        tracked, on_ledger,
        "seed={seed} step={step}: tracked balance ({tracked}) != on-ledger USDC ({on_ledger})"
    );
    assert!(
        tracked >= 0,
        "seed={seed} step={step}: tracked balance went negative: {tracked}"
    );

    // Cross-check against the dedicated view: with no external donations to
    // the vault address in this trace, idle (untracked) balance must be zero.
    let preview = client.dry_run_sweep_idle_balance();
    assert_eq!(
        preview.idle_balance, 0,
        "seed={seed} step={step}: unexpected idle balance {}",
        preview.idle_balance
    );
    assert!(
        !preview.has_idle,
        "seed={seed} step={step}: has_idle unexpectedly true"
    );
}

// ---------------------------------------------------------------------------
// Seeded property traces
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Op {
    Deposit,
    DepositBelowMin,
    Deduct,
    DeductAboveMax,
    BatchDeduct,
    PauseToggle,
}

fn pick_op(rng: &mut Prng) -> Op {
    match rng.next_u64() % 6 {
        0 => Op::Deposit,
        1 => Op::DepositBelowMin,
        2 => Op::Deduct,
        3 => Op::DeductAboveMax,
        4 => Op::BatchDeduct,
        _ => Op::PauseToggle,
    }
}

fn run_trace(seed: u64) {
    let env = Env::default();
    env.mock_all_auths();

    let (client, owner, authorized_caller, usdc_client, usdc_admin) =
        setup(&env, INITIAL_BALANCE, MIN_DEPOSIT, MAX_DEDUCT);
    usdc_admin.mint(&owner, &(INITIAL_BALANCE * 100));

    let mut rng = Prng::new(seed);
    let mut paused = false;

    assert_balance_in_sync(&client, &usdc_client, seed, 0);

    for step in 1..=TRACE_LEN {
        let balance_before = client.balance();

        match pick_op(&mut rng) {
            Op::Deposit => {
                let amount = rng.range_i128(MIN_DEPOSIT, MIN_DEPOSIT * 20);
                let result = client.try_deposit(&owner, &amount);
                if paused {
                    assert!(
                        result.is_err(),
                        "seed={seed} step={step}: deposit must fail while paused"
                    );
                } else {
                    assert!(
                        result.is_ok(),
                        "seed={seed} step={step}: valid deposit unexpectedly failed"
                    );
                }
            }
            Op::DepositBelowMin => {
                let amount = rng.range_i128(1, MIN_DEPOSIT - 1);
                let result = client.try_deposit(&owner, &amount);
                assert!(
                    result.is_err(),
                    "seed={seed} step={step}: below-minimum deposit must fail"
                );
            }
            Op::Deduct => {
                let amount = rng.range_i128(MIN_DEPOSIT, MAX_DEDUCT);
                let request_id = rng.next_u64();
                let result = client.try_deduct(&authorized_caller, &amount, &request_id);
                if paused {
                    assert!(
                        result.is_err(),
                        "seed={seed} step={step}: deduct must fail while paused"
                    );
                } else if balance_before < amount {
                    assert!(
                        result.is_err(),
                        "seed={seed} step={step}: deduct must fail when balance insufficient"
                    );
                } else {
                    assert!(
                        result.is_ok(),
                        "seed={seed} step={step}: valid deduct unexpectedly failed"
                    );
                }
            }
            Op::DeductAboveMax => {
                let amount = rng.range_i128(MAX_DEDUCT + 1, MAX_DEDUCT + 1_000);
                let request_id = rng.next_u64();
                let result = client.try_deduct(&authorized_caller, &amount, &request_id);
                assert!(
                    result.is_err(),
                    "seed={seed} step={step}: above-max-deduct must fail"
                );
            }
            Op::BatchDeduct => {
                let n = rng.range_usize(1, 4);
                let mut items = Vec::new(&env);
                let mut total: i128 = 0;
                for _ in 0..n {
                    let amount = rng.range_i128(MIN_DEPOSIT, MAX_DEDUCT);
                    total += amount;
                    items.push_back((amount, rng.next_u64()));
                }
                let result = client.try_batch_deduct(&authorized_caller, &items);
                if paused {
                    assert!(
                        result.is_err(),
                        "seed={seed} step={step}: batch_deduct must fail while paused"
                    );
                } else if balance_before < total {
                    assert!(
                        result.is_err(),
                        "seed={seed} step={step}: batch_deduct must fail when total exceeds balance"
                    );
                } else {
                    assert!(
                        result.is_ok(),
                        "seed={seed} step={step}: valid batch_deduct unexpectedly failed"
                    );
                }
            }
            Op::PauseToggle => {
                if paused {
                    client.unpause(&owner);
                    paused = false;
                } else {
                    client.pause(&owner);
                    paused = true;
                }
            }
        }

        assert_balance_in_sync(&client, &usdc_client, seed, step);
    }
}

/// Run all `SEED_COUNT` deterministic seeded traces (each `TRACE_LEN` steps).
#[test]
fn vault_balance_stays_in_sync_across_seeded_traces() {
    for seed in 0..SEED_COUNT {
        run_trace(seed);
    }
}

// ---------------------------------------------------------------------------
// Dedicated edge-case invariant tests
// ---------------------------------------------------------------------------

/// Deposit exactly at `min_deposit` succeeds; one unit below fails and leaves
/// state untouched.
#[test]
fn deposit_boundary_exact_minimum_succeeds_one_below_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, owner, _authorized_caller, usdc_client, usdc_admin) = setup(&env, 0, 100, 1_000);
    usdc_admin.mint(&owner, &10_000);

    client.deposit(&owner, &100);
    assert_eq!(client.balance(), 100);
    assert_eq!(usdc_client.balance(&client.address), 100);

    let result = client.try_deposit(&owner, &99);
    assert!(result.is_err(), "deposit below min_deposit must fail");
    assert_eq!(
        client.balance(),
        100,
        "rejected deposit must not mutate tracked balance"
    );
    assert_eq!(usdc_client.balance(&client.address), 100);
}

/// Deduct exactly at `max_deduct` succeeds; one unit above fails and leaves
/// state untouched.
#[test]
fn deduct_boundary_exact_maximum_succeeds_one_above_fails() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _owner, authorized_caller, usdc_client, _usdc_admin) =
        setup(&env, 5_000, 100, 1_000);

    client.deduct(&authorized_caller, &1_000, &1u64);
    assert_eq!(client.balance(), 4_000);
    assert_eq!(usdc_client.balance(&client.address), 4_000);

    let result = client.try_deduct(&authorized_caller, &1_001, &2u64);
    assert!(result.is_err(), "deduct above max_deduct must fail");
    assert_eq!(
        client.balance(),
        4_000,
        "rejected deduct must not mutate tracked balance"
    );
    assert_eq!(usdc_client.balance(&client.address), 4_000);
}

/// While paused, deposit/deduct/batch_deduct are all rejected and state is
/// untouched; unpausing restores normal operation.
#[test]
fn pause_blocks_deposit_and_deduct_unpause_restores() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, owner, authorized_caller, usdc_client, usdc_admin) =
        setup(&env, 1_000, 100, 1_000);
    usdc_admin.mint(&owner, &10_000);

    client.pause(&owner);
    assert!(client.is_paused());

    assert!(client.try_deposit(&owner, &500).is_err());
    assert!(client.try_deduct(&authorized_caller, &500, &1u64).is_err());
    let batch = Vec::from_array(&env, [(500i128, 2u64)]);
    assert!(client.try_batch_deduct(&authorized_caller, &batch).is_err());

    assert_eq!(
        client.balance(),
        1_000,
        "paused rejections must not mutate tracked balance"
    );
    assert_eq!(usdc_client.balance(&client.address), 1_000);

    client.unpause(&owner);
    assert!(!client.is_paused());

    client.deposit(&owner, &500);
    assert_eq!(client.balance(), 1_500);
    assert_eq!(usdc_client.balance(&client.address), 1_500);
}

/// A batch whose total exceeds the vault's balance is rejected in full — no
/// partial deduction — even though every individual item is within bounds.
#[test]
fn batch_deduct_is_all_or_nothing() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, _owner, authorized_caller, usdc_client, _usdc_admin) =
        setup(&env, 150, 100, 1_000);

    let items = Vec::from_array(&env, [(100i128, 1u64), (100i128, 2u64)]);
    let result = client.try_batch_deduct(&authorized_caller, &items);
    assert!(
        result.is_err(),
        "batch total exceeding balance must fail atomically"
    );
    assert_eq!(
        client.balance(),
        150,
        "failed batch must not partially mutate tracked balance"
    );
    assert_eq!(usdc_client.balance(&client.address), 150);
}

/// Zero and negative amounts are rejected on both deposit and deduct.
#[test]
fn zero_and_negative_amounts_are_rejected() {
    let env = Env::default();
    env.mock_all_auths();
    let (client, owner, authorized_caller, _usdc_client, usdc_admin) =
        setup(&env, 1_000, 100, 1_000);
    usdc_admin.mint(&owner, &10_000);

    assert!(client.try_deposit(&owner, &0).is_err());
    assert!(client.try_deposit(&owner, &-50).is_err());
    assert!(client.try_deduct(&authorized_caller, &0, &1u64).is_err());
    assert!(client.try_deduct(&authorized_caller, &-50, &2u64).is_err());

    assert_eq!(
        client.balance(),
        1_000,
        "all-rejected calls must not mutate tracked balance"
    );
}
