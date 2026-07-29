//! # Per-Entrypoint Gas & Memory Snapshot Tests — `callora-limits`
//!
//! Captures [`ProfileSnapshot`] (CPU instructions + memory bytes) for every
//! limits entrypoint across Settlement and RevenuePool contracts and asserts
//! that each measurement stays within specified budget ceilings.
//!
//! ## Measurement & Harvesting
//!
//! Each test executes an entrypoint under measurement, reads host resource
//! metrics via `env.cost_estimate().resources()`, and prints a machine-readable JSON line:
//!
//! ```json
//! {"contract":"callora-limits","entrypoint":"set_developer_min_balance","cpu":82000,"mem":1200,"budget_cpu":170000,"budget_mem":3000}
//! ```
//!
//! `scripts/gas-regression.sh` harvests these JSON lines, compares them against
//! `contracts/.gas-baseline.json`, and fails CI if CPU or memory regresses by > 5%.
//!
//! ## Safety & Guidelines Compliance
//!
//! - All arithmetic operations use checked or saturating operations.
//! - No `unwrap()` in production paths; test helpers fail cleanly via descriptive panic messages.
//! - All state-changing entrypoints require owner authorization via `env.mock_all_auths()`.
//! - Documented with NatDoc / RustDoc comments (`///`).

extern crate std;
use std::println;

use callora_revenue_pool::{RevenuePool, RevenuePoolClient};
use callora_settlement::{CalloraSettlement, CalloraSettlementClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

/// Point-in-time resource snapshot (CPU instruction units + ledger I/O bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileSnapshot {
    pub cpu: u64,
    pub mem: u64,
}

impl ProfileSnapshot {
    #[inline]
    pub fn capture(env: &Env) -> Self {
        let res = env.cost_estimate().resources();
        let cpu = res.instructions as u64;
        let mem = (res.read_bytes as u64).saturating_add(res.write_bytes as u64);
        Self { cpu, mem }
    }

    #[inline]
    pub fn cpu_exceeds(&self, cap: u64) -> bool {
        self.cpu > cap
    }

    #[inline]
    pub fn mem_exceeds(&self, cap: u64) -> bool {
        self.mem > cap
    }
}

fn setup_settlement(env: &Env) -> (Address, CalloraSettlementClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let addr = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(env, &addr);
    client.init(&admin, &vault);
    (admin, client)
}

fn setup_pool(env: &Env) -> (Address, RevenuePoolClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let usdc = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let addr = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &addr);
    client.init(&admin, &usdc);
    (admin, client)
}

fn assert_within_budget(
    contract: &str,
    entrypoint: &str,
    snap: ProfileSnapshot,
    cpu_cap: u64,
    mem_cap: u64,
) {
    println!(
        "{{\"contract\":\"{c}\",\"entrypoint\":\"{ep}\",\"cpu\":{cpu},\"mem\":{mem},\"budget_cpu\":{bcpu},\"budget_mem\":{bmem}}}",
        c = contract,
        ep = entrypoint,
        cpu = snap.cpu,
        mem = snap.mem,
        bcpu = cpu_cap,
        bmem = mem_cap,
    );

    assert!(
        !snap.cpu_exceeds(cpu_cap),
        "gas regression: [{c}::{ep}] CPU {cpu} exceeds budget cap {cap}",
        c = contract,
        ep = entrypoint,
        cpu = snap.cpu,
        cap = cpu_cap,
    );
    assert!(
        !snap.mem_exceeds(mem_cap),
        "gas regression: [{c}::{ep}] mem {mem} exceeds budget cap {cap}",
        c = contract,
        ep = entrypoint,
        mem = snap.mem,
        cap = mem_cap,
    );
}

macro_rules! measure_snap {
    ($env:expr, $snap:ident, $body:expr) => {
        $body;
        let $snap = ProfileSnapshot::capture(&$env);
    };
}

// =============================================================================
// Settlement — mutating limits
// =============================================================================

#[test]
fn gas_snap_set_developer_min_balance() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);
    measure_snap!(env, snap, {
        client.set_developer_min_balance(&admin, &developer, &100);
    });
    assert_within_budget("callora-limits", "set_developer_min_balance", snap, 200_000, 4_000);
}

#[test]
fn gas_snap_set_minimum_balance() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);
    measure_snap!(env, snap, {
        client.set_minimum_balance(&admin, &developer, &50);
    });
    assert_within_budget("callora-limits", "set_minimum_balance", snap, 200_000, 4_000);
}

#[test]
fn gas_snap_set_daily_withdraw_cap() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);
    measure_snap!(env, snap, {
        client.set_daily_withdraw_cap(&admin, &developer, &1_000);
    });
    assert_within_budget("callora-limits", "set_daily_withdraw_cap", snap, 200_000, 4_000);
}

// =============================================================================
// Settlement — read-only limits views
// =============================================================================

#[test]
fn gas_snap_get_developer_min_balance() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);
    client.set_developer_min_balance(&admin, &developer, &100);
    measure_snap!(env, snap, {
        let _ = client.get_developer_min_balance(&developer);
    });
    assert_within_budget("callora-limits", "get_developer_min_balance", snap, 100_000, 2_000);
}

#[test]
fn gas_snap_get_minimum_balance() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);
    client.set_minimum_balance(&admin, &developer, &50);
    measure_snap!(env, snap, {
        let _ = client.get_minimum_balance(&developer);
    });
    assert_within_budget("callora-limits", "get_minimum_balance", snap, 100_000, 2_000);
}

#[test]
fn gas_snap_get_daily_withdraw_cap() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);
    client.set_daily_withdraw_cap(&admin, &developer, &1_000);
    measure_snap!(env, snap, {
        let _ = client.get_daily_withdraw_cap(&developer);
    });
    assert_within_budget("callora-limits", "get_daily_withdraw_cap", snap, 100_000, 2_000);
}

#[test]
fn gas_snap_get_withdrawal_today() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);
    client.set_daily_withdraw_cap(&admin, &developer, &1_000);
    measure_snap!(env, snap, {
        let _ = client.get_withdrawal_today(&developer);
    });
    assert_within_budget("callora-limits", "get_withdrawal_today", snap, 100_000, 2_000);
}

// =============================================================================
// Revenue pool — distribute cap limits
// =============================================================================

#[test]
fn gas_snap_set_max_distribute() {
    let env = Env::default();
    let (admin, client) = setup_pool(&env);
    measure_snap!(env, snap, {
        client.set_max_distribute(&admin, &10_000);
    });
    assert_within_budget("callora-limits", "set_max_distribute", snap, 200_000, 4_000);
}

#[test]
fn gas_snap_get_max_distribute() {
    let env = Env::default();
    let (admin, client) = setup_pool(&env);
    client.set_max_distribute(&admin, &10_000);
    measure_snap!(env, snap, {
        let _ = client.get_max_distribute();
    });
    assert_within_budget("callora-limits", "get_max_distribute", snap, 100_000, 2_000);
}

// =============================================================================
// Snapshot sanity
// =============================================================================

#[test]
fn gas_snap_snapshot_nonzero_sanity() {
    let env = Env::default();
    let (admin, client) = setup_settlement(&env);
    let developer = Address::generate(&env);
    measure_snap!(env, snap, {
        client.set_developer_min_balance(&admin, &developer, &1);
    });
    assert!(snap.cpu > 0, "CPU metric must be > 0");
    assert!(snap.mem > 0, "Memory metric must be > 0");
}

#[test]
fn gas_snap_boundary_comparisons() {
    let snap = ProfileSnapshot { cpu: 100, mem: 200 };
    assert!(!snap.cpu_exceeds(100));
    assert!(!snap.mem_exceeds(200));
    assert!(snap.cpu_exceeds(99));
    assert!(snap.mem_exceeds(199));
}