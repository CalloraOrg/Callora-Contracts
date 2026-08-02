//! # Per-Entrypoint Gas & Memory Snapshot Tests — `callora-registry`
//!
//! Captures CPU instruction count and memory byte usage for every public
//! entrypoint of the [`CalloraRegistry`] contract and asserts that each
//! measurement stays within specified budget ceilings.
//!
//! ## Measurement & Harvesting
//!
//! Each test executes a registry entrypoint under measurement, reads host
//! resource metrics via `env.cost_estimate().resources()`, and prints a
//! machine-readable JSON line to stdout:
//!
//! ```json
//! {"contract":"callora-registry","entrypoint":"init","cpu":82000,"mem":1200,"budget_cpu":170000,"budget_mem":3000}
//! ```
//!
//! `scripts/gas-regression.sh` harvests these JSON lines and compares them
//! against `contracts/.gas-baseline.json`, failing CI if CPU or memory
//! regresses by more than 5 %.
//!
//! ## Coverage
//!
//! | Entrypoint                 | Auth required | Test                                              |
//! |----------------------------|---------------|---------------------------------------------------|
//! | `init`                     | ✓             | [`gas_snap_init`]                                |
//! | `register_offering`        | ✓             | [`gas_snap_register_offering`]                   |
//! | `register_offering_with_gate` | ✓          | [`gas_snap_register_offering_with_gate`]         |
//! | `is_offering_registered`   | ✗             | [`gas_snap_is_offering_registered`]              |
//! | `registered_count`         | ✗             | [`gas_snap_registered_count`]                    |
//! | `get_offering`             | ✗             | [`gas_snap_get_offering`]                        |
//!
//! Closes CalloraOrg/Callora-Contracts#853.

extern crate std;
use std::println;

use callora_registry::{CalloraRegistry, CalloraRegistryClient};
use soroban_sdk::testutils::{Address as _, Ledger};
use soroban_sdk::{contract, contractimpl, Address, Env, String};

// ---------------------------------------------------------------------------
// Mock callees
// ---------------------------------------------------------------------------

/// Mock catalog — accepts every `put_offering` call forwarded by the registry.
#[contract]
struct MockCatalog;

#[contractimpl]
impl MockCatalog {
    /// Accepts any offering registration forwarded by the registry.
    pub fn put_offering(_env: Env, _registry: Address, _offering_id: String, _metadata: String) {}
}

/// Mock token — reports a fixed, always-sufficient developer balance.
#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    /// Returns a fixed balance so the balance gate always passes.
    pub fn balance(_env: Env, _id: Address) -> i128 {
        1_000_000_000
    }
}

// ---------------------------------------------------------------------------
// ProfileSnapshot — lightweight resource counter capture
// ---------------------------------------------------------------------------

/// Point-in-time snapshot of host resource counters (CPU + memory).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProfileSnapshot {
    /// Host CPU instruction units consumed up to the measurement point.
    pub cpu: u64,
    /// Sum of `read_bytes + write_bytes` consumed up to the measurement point.
    pub mem: u64,
}

impl ProfileSnapshot {
    /// Capture host resource counters at this instant.
    #[inline]
    pub fn capture(env: &Env) -> Self {
        let res = env.cost_estimate().resources();
        let cpu = res.instructions as u64;
        let mem = (res.read_bytes as u64).saturating_add(res.write_bytes as u64);
        Self { cpu, mem }
    }

    /// Returns `true` if the CPU count exceeds `cap`.
    #[inline]
    pub fn cpu_exceeds(&self, cap: u64) -> bool {
        self.cpu > cap
    }

    /// Returns `true` if the memory byte count exceeds `cap`.
    #[inline]
    pub fn mem_exceeds(&self, cap: u64) -> bool {
        self.mem > cap
    }
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Shared registry fixture: deployed registry + mock catalog token.
struct RegistryFixture<'a> {
    env: Env,
    client: CalloraRegistryClient<'a>,
    admin: Address,
    token: Address,
}

/// Instantiate and initialize the registry with a mock catalog + token.
fn setup() -> RegistryFixture<'static> {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let admin = Address::generate(env);
    let catalog = env.register(MockCatalog, ());
    let token = env.register(MockToken, ());
    let contract_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(env, &contract_id);
    client.init(&admin, &catalog);

    RegistryFixture {
        env: env.clone(),
        client,
        admin,
        token,
    }
}

/// Register a single offering, returning its id string.
fn register_one(f: &RegistryFixture<'_>) -> String {
    let developer = Address::generate(&f.env);
    let id = String::from_str(&f.env, "snap-offering");
    let meta = String::from_str(&f.env, "https://meta.example.com/snap");
    f.client.register_offering(&f.admin, &developer, &id, &meta);
    id
}

// ---------------------------------------------------------------------------
// Budget assertion helpers
// ---------------------------------------------------------------------------

/// Assert a snapshot stays within `cpu_cap`/`mem_cap` and emit a JSON line
/// for `gas-regression.sh`.
fn assert_within_budget(entrypoint: &str, snap: ProfileSnapshot, cpu_cap: u64, mem_cap: u64) {
    println!(
        "{{\"contract\":\"callora-registry\",\"entrypoint\":\"{ep}\",\"cpu\":{cpu},\"mem\":{mem},\
         \"budget_cpu\":{bcpu},\"budget_mem\":{bmem}}}",
        ep = entrypoint,
        cpu = snap.cpu,
        mem = snap.mem,
        bcpu = cpu_cap,
        bmem = mem_cap,
    );
    assert!(
        !snap.cpu_exceeds(cpu_cap),
        "gas regression: [callora-registry::{ep}] CPU {cpu} exceeds budget {cap}",
        ep = entrypoint,
        cpu = snap.cpu,
        cap = cpu_cap,
    );
    assert!(
        !snap.mem_exceeds(mem_cap),
        "gas regression: [callora-registry::{ep}] mem {mem} exceeds budget {cap}",
        ep = entrypoint,
        mem = snap.mem,
        cap = mem_cap,
    );
}

/// Verify that `after` has not grown by more than `pct`% over `before`.
fn assert_no_regression(
    entrypoint: &str,
    before: ProfileSnapshot,
    after: ProfileSnapshot,
    pct: u64,
) {
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

// ---------------------------------------------------------------------------
// Measurement macro
// ---------------------------------------------------------------------------

macro_rules! measure_snap {
    ($env:expr, $snap:ident, $body:expr) => {
        $body;
        let $snap = ProfileSnapshot::capture(&$env);
    };
}

// ===========================================================================
// Per-entrypoint gas snapshot tests
// ===========================================================================

/// Snapshot `init` — deploy and initialize the registry.
#[test]
fn gas_snap_init() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let catalog = env.register(MockCatalog, ());
    let contract_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &contract_id);

    measure_snap!(env, snap, {
        client.init(&admin, &catalog);
    });
    assert_within_budget("init", snap, 500_000, 12_000);
}

/// Snapshot `register_offering` — admin registers a new offering.
#[test]
fn gas_snap_register_offering() {
    let f = setup();
    let developer = Address::generate(&f.env);
    let id = String::from_str(&f.env, "payments-offering");
    let meta = String::from_str(&f.env, "https://meta.example.com/payments");

    measure_snap!(f.env, snap, {
        f.client.register_offering(&f.admin, &developer, &id, &meta);
    });
    assert_within_budget("register_offering", snap, 700_000, 16_000);
}

/// Snapshot `register_offering_with_gate` — admin gates on developer balance.
#[test]
fn gas_snap_register_offering_with_gate() {
    let f = setup();
    let developer = Address::generate(&f.env);
    let id = String::from_str(&f.env, "gated-offering");
    let meta = String::from_str(&f.env, "https://meta.example.com/gated");

    measure_snap!(f.env, snap, {
        f.client
            .register_offering_with_gate(&f.admin, &developer, &f.token, &100i128, &id, &meta);
    });
    assert_within_budget("register_offering_with_gate", snap, 800_000, 18_000);
}

/// Snapshot `is_offering_registered` — read-only existence check (hit).
///
/// A registered offering must report `true` on every read path.
#[test]
fn gas_snap_is_offering_registered() {
    let f = setup();
    let id = register_one(&f);

    measure_snap!(f.env, snap, {
        let registered = f.client.is_offering_registered(&id);
        assert!(registered, "freshly registered offering must be present");
    });
    assert_within_budget("is_offering_registered", snap, 400_000, 8_000);
}

/// Snapshot `registered_count` — read-only total registered count view.
#[test]
fn gas_snap_registered_count() {
    let f = setup();
    register_one(&f);

    measure_snap!(f.env, snap, {
        let count = f.client.registered_count();
        assert_eq!(count, 1);
    });
    assert_within_budget("registered_count", snap, 300_000, 6_000);
}

/// Snapshot `get_offering` — read-only fetch of a registered offering.
#[test]
fn gas_snap_get_offering() {
    let f = setup();
    let id = register_one(&f);

    measure_snap!(f.env, snap, {
        let record = f.client.get_offering(&id);
        assert_eq!(record.offering_id, id);
    });
    assert_within_budget("get_offering", snap, 400_000, 8_000);
}

// ===========================================================================
// Relative growth & sanity tests
// ===========================================================================

/// A second `register_offering` must not cost more than 50 % more than the
/// first, confirming O(1) behaviour for individual registrations.
///
/// The ledger timestamp is advanced past the admin cooldown window so the
/// second registration is not rejected by [`RegistryError::AdminCooldownActive`].
#[test]
fn gas_snap_register_offering_second_relative() {
    let f = setup();
    let dev1 = Address::generate(&f.env);
    let meta = String::from_str(&f.env, "https://meta.example.com/rel");

    measure_snap!(f.env, snap_first, {
        f.client.register_offering(
            &f.admin,
            &dev1,
            &String::from_str(&f.env, "rel-first"),
            &meta,
        );
    });

    f.env
        .ledger()
        .with_mut(|l| l.timestamp = l.timestamp.saturating_add(3601));

    let dev2 = Address::generate(&f.env);
    measure_snap!(f.env, snap_second, {
        f.client.register_offering(
            &f.admin,
            &dev2,
            &String::from_str(&f.env, "rel-second"),
            &meta,
        );
    });

    assert_no_regression(
        "register_offering_second_vs_first",
        snap_first,
        snap_second,
        50,
    );
}

/// Repeated `is_offering_registered` calls on the same id must stay bounded.
#[test]
fn gas_snap_is_offering_registered_repeated_relative() {
    let f = setup();
    let id = register_one(&f);

    let id_str = id.clone();
    measure_snap!(f.env, snap_first, {
        let _ = f.client.is_offering_registered(&id_str);
    });

    measure_snap!(f.env, snap_second, {
        let _ = f.client.is_offering_registered(&id_str);
    });

    assert_no_regression(
        "is_offering_registered_repeated_second_vs_first",
        snap_first,
        snap_second,
        50,
    );
}

/// `ProfileSnapshot::capture` must return non-zero metrics after a real call.
#[test]
fn gas_snap_snapshot_nonzero_sanity() {
    let f = setup();
    let id = register_one(&f);

    measure_snap!(f.env, snap, {
        let _ = f.client.is_offering_registered(&id);
    });
    assert!(snap.cpu > 0, "CPU metric must be > 0 after a real call");
    assert!(snap.mem > 0, "Memory metric must be > 0 after a real call");
}

/// Boundary: `cpu_exceeds` / `mem_exceeds` are inclusive-boundary checks.
#[test]
fn gas_snap_boundary_comparisons() {
    let snap = ProfileSnapshot { cpu: 100, mem: 200 };
    assert!(!snap.cpu_exceeds(100), "equal to cap must NOT exceed");
    assert!(!snap.mem_exceeds(200), "equal to cap must NOT exceed");
    assert!(snap.cpu_exceeds(99), "one above cap must exceed");
    assert!(snap.mem_exceeds(199), "one above cap must exceed");
}
