//! Gas and memory snapshots for the Callora Allowlist contract.
//!
//! Each test executes one entrypoint, reads the host resource counters and
//! prints a machine-readable line:
//!
//! ```json
//! {"contract":"callora-allowlist","entrypoint":"add_address","cpu":82000,"mem":1200,"budget_cpu":300000,"budget_mem":8000}
//! ```
//!
//! `scripts/gas-regression.sh` harvests these lines with
//! `cargo test -p callora-allowlist -- gas_snap --nocapture` and compares them
//! against `contracts/.gas-baseline.json`. The in-test budget caps are
//! deliberately generous (roughly 2x the observed cost) so the snapshot test
//! catches pathological regressions without being brittle.

#![cfg(test)]
extern crate std;

use callora_allowlist::{CalloraAllowlist, CalloraAllowlistClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env};

/// Point-in-time resource snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileSnapshot {
    /// CPU instruction units.
    pub cpu: u64,
    /// Sum of read and written ledger bytes.
    pub mem: u64,
}

impl ProfileSnapshot {
    /// Capture the host resource counters.
    pub fn capture(env: &Env) -> Self {
        let res = env.cost_estimate().resources();
        Self {
            cpu: res.instructions as u64,
            mem: (res.read_bytes as u64).saturating_add(res.write_bytes as u64),
        }
    }
}

struct Fixture {
    env: Env,
    client: CalloraAllowlistClient<'static>,
    admin: Address,
}

fn setup() -> Fixture {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();
    let contract = env.register(CalloraAllowlist, ());
    let client = CalloraAllowlistClient::new(env, &contract);
    let admin = Address::generate(env);
    client.init(&admin);

    Fixture {
        env: env.clone(),
        client,
        admin,
    }
}

fn assert_within_budget(entrypoint: &str, snap: ProfileSnapshot, cpu_cap: u64, mem_cap: u64) {
    println!(
        "{{\"contract\":\"callora-allowlist\",\"entrypoint\":\"{ep}\",\"cpu\":{cpu},\"mem\":{mem},\"budget_cpu\":{bcpu},\"budget_mem\":{bmem}}}",
        ep = entrypoint,
        cpu = snap.cpu,
        mem = snap.mem,
        bcpu = cpu_cap,
        bmem = mem_cap,
    );
    assert!(
        snap.cpu <= cpu_cap,
        "gas regression: [callora-allowlist::{ep}] CPU {cpu} exceeds budget {cap}",
        ep = entrypoint,
        cpu = snap.cpu,
        cap = cpu_cap,
    );
    assert!(
        snap.mem <= mem_cap,
        "gas regression: [callora-allowlist::{ep}] mem {mem} exceeds budget {cap}",
        ep = entrypoint,
        mem = snap.mem,
        cap = mem_cap,
    );
}

#[test]
fn gas_snap_init() {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();
    let contract = env.register(CalloraAllowlist, ());
    let client = CalloraAllowlistClient::new(env, &contract);
    let admin = Address::generate(env);

    client.init(&admin);
    let snap = ProfileSnapshot::capture(env);
    assert_within_budget("init", snap, 400_000, 10_000);
}

#[test]
fn gas_snap_add_address() {
    let f = setup();
    let address = Address::generate(&f.env);

    f.client.add_address(&f.admin, &address);
    let snap = ProfileSnapshot::capture(&f.env);
    assert_within_budget("add_address", snap, 600_000, 16_000);
}

#[test]
fn gas_snap_remove_address() {
    let f = setup();
    let address = Address::generate(&f.env);
    f.client.add_address(&f.admin, &address);

    f.client.remove_address(&f.admin, &address);
    let snap = ProfileSnapshot::capture(&f.env);
    assert_within_budget("remove_address", snap, 600_000, 16_000);
}

#[test]
fn gas_snap_is_allowed() {
    let f = setup();
    let address = Address::generate(&f.env);
    f.client.add_address(&f.admin, &address);

    let _ = f.client.is_allowed(&address);
    let snap = ProfileSnapshot::capture(&f.env);
    assert_within_budget("is_allowed", snap, 300_000, 8_000);
}

#[test]
fn gas_snap_allowlist_count() {
    let f = setup();
    let _ = f.client.allowlist_count();
    let snap = ProfileSnapshot::capture(&f.env);
    assert_within_budget("allowlist_count", snap, 200_000, 6_000);
}

#[test]
fn gas_snap_get_allowlist_page() {
    let f = setup();
    for _ in 0..5 {
        f.client.add_address(&f.admin, &Address::generate(&f.env));
    }

    let _ = f.client.get_allowlist_page(&0, &5);
    let snap = ProfileSnapshot::capture(&f.env);
    assert_within_budget("get_allowlist_page", snap, 700_000, 20_000);
}

#[test]
fn gas_snap_clear_all() {
    let f = setup();
    for _ in 0..3 {
        f.client.add_address(&f.admin, &Address::generate(&f.env));
    }

    f.client.clear_all(&f.admin);
    let snap = ProfileSnapshot::capture(&f.env);
    assert_within_budget("clear_all", snap, 900_000, 24_000);
}

#[test]
fn gas_snap_set_admin() {
    let f = setup();
    let nominee = Address::generate(&f.env);

    f.client.set_admin(&f.admin, &nominee);
    let snap = ProfileSnapshot::capture(&f.env);
    assert_within_budget("set_admin", snap, 400_000, 10_000);
}

#[test]
fn gas_snap_accept_admin() {
    let f = setup();
    let nominee = Address::generate(&f.env);
    f.client.set_admin(&f.admin, &nominee);

    f.client.accept_admin();
    let snap = ProfileSnapshot::capture(&f.env);
    assert_within_budget("accept_admin", snap, 400_000, 10_000);
}
