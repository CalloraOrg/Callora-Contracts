//! # Per-Call CPU/Memory Profile Snapshot Tests — `callora-vault`
//!
//! This module captures a [`ProfileSnapshot`] (CPU instructions + memory bytes)
//! for every public `callora-vault` entrypoint and asserts that each measurement
//! stays within a known budget ceiling.  The ceilings are intentionally generous
//! (2× the current baseline in `contracts/.gas-baseline.json`) so that they trip
//! only on genuine regressions rather than on natural measurement noise.
//!
//! ## How regression detection works
//!
//! 1. Each test calls a single entrypoint and records its [`ProfileSnapshot`].
//! 2. The snapshot is compared to a hard-coded **budget cap** derived from the
//!    baseline values.  A 100 % headroom means a doubling of cost fails the test.
//! 3. [`assert_within_budget`] prints a JSON line to `stdout` so
//!    `scripts/gas-regression.sh` can harvest these values alongside the
//!    existing `test_gas_budget` output:
//!    ```text
//!    {"contract":"callora-vault","entrypoint":"deposit","cpu":235124,"mem":2496,"budget_cpu":500000,"budget_mem":10000}
//!    ```
//!
//! ## Filtering
//!
//! Run only profile tests with:
//! ```bash
//! cargo test --package callora-vault profile_ -- --nocapture
//! ```
//!
//! ## Adding a new entrypoint
//!
//! 1. Write a test that calls the entrypoint inside `measure_profile!`.
//! 2. Set `cpu_cap` and `mem_cap` to 2× the value in `.gas-baseline.json`.
//! 3. Prefix the test name with `profile_` for easy filtering.
//!
//! ## Design constraints
//!
//! - No `unwrap()` in production helper paths; `unwrap()` is used only on test
//!   infrastructure (address generation, token registration) where a panic is
//!   the correct failure mode.
//! - Every state-changing entrypoint has `require_auth` on the contract side;
//!   tests satisfy this via `env.mock_all_auths()`.
//! - Overflow-safe: all arithmetic in assertion helpers uses checked operations.
//! - Snapshot capture is pure read: it mutates no contract state.

extern crate std;
use std::println;

use callora_settlement::CalloraSettlement;
use callora_vault::{CalloraVault, CalloraVaultClient, DeductItem};
use soroban_sdk::{testutils::Address as _, token, Address, Env, Symbol, Vec};

// ─────────────────────────────────────────────────────────────────────────────
// Core types
// ─────────────────────────────────────────────────────────────────────────────

/// A point-in-time resource snapshot captured immediately after a contract call.
///
/// Both fields are raw host-metered values returned by
/// `Env::cost_estimate().resources()`:
///
/// - `cpu` — Soroban *instruction* units (not wall-clock time).
/// - `mem` — sum of `read_bytes + write_bytes`; a proxy for storage pressure.
///
/// Snapshots are immutable once created.  Use [`assert_within_budget`] or
/// [`assert_no_regression`] to validate them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileSnapshot {
    /// CPU instruction units consumed up to the capture point.
    pub cpu: u64,
    /// Ledger I/O pressure (`read_bytes + write_bytes`) up to the capture point.
    pub mem: u64,
}

impl ProfileSnapshot {
    /// Capture the current host resource counters from `env`.
    ///
    /// Call this **after** the operation under test and **before** any
    /// subsequent contract call that would add to the counters.
    #[inline]
    pub fn capture(env: &Env) -> Self {
        let res = env.cost_estimate().resources();
        let cpu = res.instructions as u64;
        // Saturating add: theoretically can't overflow in test environments,
        // but defensive against future SDK changes.
        let mem = (res.read_bytes as u64).saturating_add(res.write_bytes as u64);
        Self { cpu, mem }
    }

    /// Return `true` when this snapshot's CPU exceeds `cap`.
    #[inline]
    pub fn cpu_exceeds(&self, cap: u64) -> bool {
        self.cpu > cap
    }

    /// Return `true` when this snapshot's memory exceeds `cap`.
    #[inline]
    pub fn mem_exceeds(&self, cap: u64) -> bool {
        self.mem > cap
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Assertion helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Assert that `snap` stays within `cpu_cap` and `mem_cap`, and emit a
/// machine-readable JSON line to stdout for `scripts/gas-regression.sh`.
///
/// The JSON line is emitted **before** asserting so that even a failing run
/// produces a complete log.
///
/// # Panics
///
/// Panics with a descriptive message when either cap is exceeded, surfacing
/// the regression clearly in `cargo test` output.
fn assert_within_budget(entrypoint: &str, snap: ProfileSnapshot, cpu_cap: u64, mem_cap: u64) {
    println!(
        "{{\"contract\":\"callora-vault\",\"entrypoint\":\"{ep}\",\
         \"cpu\":{cpu},\"mem\":{mem},\
         \"budget_cpu\":{bcpu},\"budget_mem\":{bmem}}}",
        ep = entrypoint,
        cpu = snap.cpu,
        mem = snap.mem,
        bcpu = cpu_cap,
        bmem = mem_cap,
    );

    assert!(
        !snap.cpu_exceeds(cpu_cap),
        "profile regression: {ep} CPU {cpu} exceeds budget cap {cap}",
        ep = entrypoint,
        cpu = snap.cpu,
        cap = cpu_cap,
    );
    assert!(
        !snap.mem_exceeds(mem_cap),
        "profile regression: {ep} mem {mem} exceeds budget cap {cap}",
        ep = entrypoint,
        mem = snap.mem,
        cap = mem_cap,
    );
}

/// Assert that `after` has not regressed by more than `pct` percent relative
/// to `before` for either CPU or memory.
///
/// `pct = 50` allows up to 50 % growth; `pct = 0` enforces exact equality.
///
/// # Panics
///
/// Panics when the relative growth exceeds the threshold.
fn assert_no_regression(
    entrypoint: &str,
    before: ProfileSnapshot,
    after: ProfileSnapshot,
    pct: u64,
) {
    // cap = before * (100 + pct) / 100  — checked to avoid overflow
    let cpu_cap = before
        .cpu
        .checked_mul(100u64.saturating_add(pct))
        .and_then(|v| v.checked_div(100))
        .expect("overflow computing cpu regression cap");
    let mem_cap = before
        .mem
        .checked_mul(100u64.saturating_add(pct))
        .and_then(|v| v.checked_div(100))
        .expect("overflow computing mem regression cap");

    assert_within_budget(entrypoint, after, cpu_cap, mem_cap);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test fixture
// ─────────────────────────────────────────────────────────────────────────────

/// Shared test fixture: a fully initialised vault with a mock USDC token and
/// a linked `CalloraSettlement` contract.
struct Fixture<'a> {
    env: Env,
    client: CalloraVaultClient<'a>,
    owner: Address,
}

/// Construct a fresh, fully-initialised vault fixture with `initial` USDC.
///
/// - Mints `initial` USDC to the vault address before calling `init` so that
///   the on-ledger balance check inside `init` passes.
/// - Registers and links a `CalloraSettlement` contract via `set_settlement`
///   so that `deduct` and `batch_deduct` can route funds.
/// - Calls `env.mock_all_auths()` to satisfy `require_auth` on all
///   state-changing calls without needing real key material.
fn setup(initial: i128) -> Fixture<'static> {
    // Leak the Env onto the heap so it outlives the function.
    // SAFETY: tests are single-threaded; the leaked pointer is only accessed
    // through the returned Fixture which is not shared across threads.
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let owner = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &vault_addr);

    // Register USDC token contract
    let usdc_addr = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    let usdc_admin = token::StellarAssetClient::new(env, &usdc_addr);

    // Pre-fund vault so init's on-ledger balance assertion passes
    if initial > 0 {
        usdc_admin.mint(&vault_addr, &initial);
    }

    client.init(
        &owner,
        &usdc_addr,
        &Some(initial),
        &None, // authorized_caller
        &None, // min_deposit
        &None, // revenue_pool
        &None, // max_deduct
    );

    // Register settlement and link it; required for deduct paths
    let settlement_addr = env.register(CalloraSettlement, ());
    callora_settlement::CalloraSettlementClient::new(env, &settlement_addr)
        .init(&owner, &vault_addr);
    client.set_settlement(&owner, &settlement_addr);

    Fixture {
        env: env.clone(),
        client,
        owner,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Measurement macro
// ─────────────────────────────────────────────────────────────────────────────

/// Execute `$body`, then capture a [`ProfileSnapshot`] and bind it to `$snap`.
///
/// Example:
/// ```ignore
/// measure_profile!(env, snap, { client.balance() });
/// assert_within_budget("balance", snap, 80_000, 1_100);
/// ```
macro_rules! measure_profile {
    ($env:expr, $snap:ident, $body:expr) => {
        $body;
        let $snap = ProfileSnapshot::capture(&$env);
    };
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-entrypoint snapshot tests
// ─────────────────────────────────────────────────────────────────────────────
//
// Budget caps are set to 2× the values in `contracts/.gas-baseline.json`.
// This headroom absorbs measurement jitter and minor refactors while still
// catching genuine performance regressions.

/// Snapshot `init` — first write that stores owner, token, and config into
/// instance storage.
///
/// Budget caps (2× baseline: cpu 119 549, mem 1 424):
/// - CPU cap: 240 000
/// - mem cap:   3 000
#[test]
fn profile_init() {
    let env = Env::default();
    env.mock_all_auths();

    let owner = Address::generate(&env);
    let vault_addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &vault_addr);

    let usdc_addr = env
        .register_stellar_asset_contract_v2(owner.clone())
        .address();
    let usdc_admin = token::StellarAssetClient::new(&env, &usdc_addr);
    usdc_admin.mint(&vault_addr, &1_000);

    measure_profile!(env, snap, {
        client.init(
            &owner,
            &usdc_addr,
            &Some(1_000),
            &None,
            &None,
            &None,
            &None,
        );
    });

    assert_within_budget("init", snap, 240_000, 3_000);
}

/// Snapshot `deposit` — transfers USDC from caller to vault and updates the
/// tracked balance in instance storage.
///
/// Budget caps (2× baseline: cpu 235 124, mem 2 496):
/// - CPU cap: 500 000
/// - mem cap:   5 000
#[test]
fn profile_deposit() {
    let f = setup(1_000);

    let usdc_addr = f.client.get_usdc_token();
    let usdc_admin = token::StellarAssetClient::new(&f.env, &usdc_addr);
    let usdc_client = token::Client::new(&f.env, &usdc_addr);

    // Mint extra USDC to the owner and approve the vault to pull it
    usdc_admin.mint(&f.owner, &500);
    usdc_client.approve(&f.owner, &f.env.current_contract_address(), &500, &10_000);

    measure_profile!(f.env, snap, {
        f.client.deposit(&f.owner, &500);
    });

    assert_within_budget("deposit", snap, 500_000, 5_000);
}

/// Snapshot `deduct` — single authorised deduction; routes funds to settlement.
///
/// Budget caps (2× baseline: cpu 329 283, mem 3 236):
/// - CPU cap: 700 000
/// - mem cap:   7 000
#[test]
fn profile_deduct() {
    let f = setup(5_000);

    measure_profile!(f.env, snap, {
        f.client
            .deduct(&f.owner, &100, &Some(Symbol::new(&f.env, "req001")));
    });

    assert_within_budget("deduct", snap, 700_000, 7_000);
}

/// Snapshot `batch_deduct` with a three-item batch.
///
/// Budget caps (2× baseline: cpu 343 235, mem 3 236):
/// - CPU cap: 700 000
/// - mem cap:   7 000
#[test]
fn profile_batch_deduct() {
    let f = setup(10_000);

    let items = Vec::from_array(
        &f.env,
        [
            DeductItem {
                amount: 100,
                request_id: Some(Symbol::new(&f.env, "b001")),
            },
            DeductItem {
                amount: 200,
                request_id: Some(Symbol::new(&f.env, "b002")),
            },
            DeductItem {
                amount: 300,
                request_id: Some(Symbol::new(&f.env, "b003")),
            },
        ],
    );

    measure_profile!(f.env, snap, {
        f.client.batch_deduct(&f.owner, &items);
    });

    assert_within_budget("batch_deduct", snap, 700_000, 7_000);
}

/// Snapshot `pause` — circuit-breaker activation; single boolean write.
///
/// Budget caps (2× baseline: cpu 79 791, mem 1 116):
/// - CPU cap: 160 000
/// - mem cap:   3 000
#[test]
fn profile_pause() {
    let f = setup(1_000);

    measure_profile!(f.env, snap, {
        f.client.pause(&f.owner);
    });

    assert_within_budget("pause", snap, 160_000, 3_000);
}

/// Snapshot `unpause` — circuit-breaker deactivation; single boolean write.
///
/// Budget caps (2× baseline: cpu 86 024, mem 1 152):
/// - CPU cap: 180 000
/// - mem cap:   3 000
#[test]
fn profile_unpause() {
    let f = setup(1_000);
    f.client.pause(&f.owner);

    measure_profile!(f.env, snap, {
        f.client.unpause(&f.owner);
    });

    assert_within_budget("unpause", snap, 180_000, 3_000);
}

/// Snapshot `set_max_deduct` — owner-only single integer write.
///
/// Budget caps (2× baseline: cpu 78 523, mem 1 080):
/// - CPU cap: 160 000
/// - mem cap:   3 000
#[test]
fn profile_set_max_deduct() {
    let f = setup(1_000);

    measure_profile!(f.env, snap, {
        f.client.set_max_deduct(&5_000);
    });

    assert_within_budget("set_max_deduct", snap, 160_000, 3_000);
}

/// Snapshot `get_meta` — pure view; reads owner, balance, authorized_caller,
/// and min_deposit from instance storage.
///
/// Budget caps (2× baseline: cpu 41 950, mem 504):
/// - CPU cap:  90 000
/// - mem cap:   1 100
#[test]
fn profile_get_meta() {
    let f = setup(1_000);

    measure_profile!(f.env, snap, {
        let _ = f.client.get_meta();
    });

    assert_within_budget("get_meta", snap, 90_000, 1_100);
}

/// Snapshot `balance` — cheapest view; single instance read.
///
/// Budget caps (2× baseline: cpu 38 333, mem 504):
/// - CPU cap:  80 000
/// - mem cap:   1 100
#[test]
fn profile_balance() {
    let f = setup(1_000);

    measure_profile!(f.env, snap, {
        let _ = f.client.balance();
    });

    assert_within_budget("balance", snap, 80_000, 1_100);
}

/// Snapshot `is_paused` — boolean view; single instance read.
///
/// Budget caps (2× baseline: cpu 37 551, mem 504):
/// - CPU cap:  80 000
/// - mem cap:   1 100
#[test]
fn profile_is_paused() {
    let f = setup(1_000);

    measure_profile!(f.env, snap, {
        let _ = f.client.is_paused();
    });

    assert_within_budget("is_paused", snap, 80_000, 1_100);
}

/// Snapshot `get_max_deduct` — integer view; single instance read.
///
/// Budget caps (2× baseline: cpu 39 588, mem 504):
/// - CPU cap:  80 000
/// - mem cap:   1 100
#[test]
fn profile_get_max_deduct() {
    let f = setup(1_000);

    measure_profile!(f.env, snap, {
        let _ = f.client.get_max_deduct();
    });

    assert_within_budget("get_max_deduct", snap, 80_000, 1_100);
}

// ─────────────────────────────────────────────────────────────────────────────
// Relative-regression tests
// ─────────────────────────────────────────────────────────────────────────────

/// `deduct` cost on the second call must not exceed the first call by more
/// than 50 %.  This guards against accidentally cumulative-cost paths (e.g. a
/// storage-grow triggered once that is charged again on every subsequent call).
#[test]
fn profile_deduct_second_call_within_50pct_of_first() {
    let f = setup(10_000);

    // First call — may pay one-time storage-init costs
    measure_profile!(f.env, snap_first, {
        f.client
            .deduct(&f.owner, &100, &Some(Symbol::new(&f.env, "warmup")));
    });

    // Second call — must not regress more than 50 % relative to the first
    measure_profile!(f.env, snap_second, {
        f.client
            .deduct(&f.owner, &100, &Some(Symbol::new(&f.env, "follow")));
    });

    assert_no_regression("deduct_second_vs_first", snap_first, snap_second, 50);
}

/// `batch_deduct` with a single item must cost no more than a plain `deduct`
/// call plus 50 % overhead.  This catches accidental per-batch fixed costs.
#[test]
fn profile_batch_single_item_within_50pct_of_deduct() {
    let f = setup(10_000);

    // Measure single deduct as the baseline
    measure_profile!(f.env, snap_single, {
        f.client
            .deduct(&f.owner, &100, &Some(Symbol::new(&f.env, "s001")));
    });

    // Measure batch_deduct with one item
    let items = Vec::from_array(
        &f.env,
        [DeductItem {
            amount: 100,
            request_id: Some(Symbol::new(&f.env, "b001")),
        }],
    );
    measure_profile!(f.env, snap_batch, {
        f.client.batch_deduct(&f.owner, &items);
    });

    assert_no_regression(
        "batch_deduct_single_vs_deduct",
        snap_single,
        snap_batch,
        50,
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Sanity tests
// ─────────────────────────────────────────────────────────────────────────────

/// Verifies that [`ProfileSnapshot::capture`] returns non-zero values after a
/// real contract call.  A zero reading would indicate the cost-estimate API is
/// unavailable in this SDK version.
#[test]
fn profile_snapshot_nonzero_after_real_call() {
    let f = setup(1_000);

    measure_profile!(f.env, snap, {
        let _ = f.client.balance();
    });

    assert!(
        snap.cpu > 0,
        "ProfileSnapshot::cpu must be > 0 after a real contract call"
    );
    assert!(
        snap.mem > 0,
        "ProfileSnapshot::mem must be > 0 after a real contract call"
    );
}

/// `cpu_exceeds` and `mem_exceeds` return the correct comparison result
/// at boundary values without panicking.
#[test]
fn profile_snapshot_boundary_comparisons() {
    let snap = ProfileSnapshot { cpu: 100, mem: 200 };

    // Exact equality: must NOT exceed
    assert!(
        !snap.cpu_exceeds(100),
        "cpu equal to cap should not be flagged as exceeding"
    );
    assert!(
        !snap.mem_exceeds(200),
        "mem equal to cap should not be flagged as exceeding"
    );

    // One unit over: must exceed
    assert!(
        snap.cpu_exceeds(99),
        "cpu one above cap should be flagged as exceeding"
    );
    assert!(
        snap.mem_exceeds(199),
        "mem one above cap should be flagged as exceeding"
    );
}
