//! Criterion benchmark harness for `callora-registry` hot entrypoints.
//!
//! Measures the simulated Soroban CPU cost of the registry's most
//! frequently-called entrypoints so regressions are visible in CI. Each
//! benchmark uses a fully-initialized contract environment backed by the
//! Soroban test runner and a mock catalog callee.
//!
//! # Covered entrypoints
//! * [`CalloraRegistry::init`] — first-time initialization
//! * [`CalloraRegistry::register_offering`] — happy-path registration
//! * [`CalloraRegistry::register_offering_with_gate`] — balance-gated registration
//! * [`CalloraRegistry::is_offering_registered`] — read-only lookup (registered)
//! * [`CalloraRegistry::registered_count`] — read count view
//! * [`CalloraRegistry::get_offering`] — full record fetch
//!
//! # Running
//! ```bash
//! cargo bench -p callora-registry
//! ```

use callora_registry::{CalloraRegistry, CalloraRegistryClient};
use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, contractimpl, token, Address, Env, String};

// ---------------------------------------------------------------------------
// Mock catalog — accepts every put_offering call without error
// ---------------------------------------------------------------------------

#[contract]
struct MockCatalog;

#[contractimpl]
impl MockCatalog {
    /// Accepts any offering registration forwarded by the registry.
    pub fn put_offering(_env: Env, _registry: Address, _offering_id: String, _metadata: String) {}
}

// ---------------------------------------------------------------------------
// Mock token with a fixed balance per developer
// ---------------------------------------------------------------------------

#[contract]
struct MockToken;

#[contractimpl]
impl MockToken {
    pub fn balance(_env: Env, _id: Address) -> i128 {
        1_000_000_000
    }
}

// ---------------------------------------------------------------------------
// Shared setup helpers
// ---------------------------------------------------------------------------

/// Register the registry and mock-catalog contracts and return initialized
/// `(Env, admin_address, CalloraRegistryClient)`.
fn setup() -> (Env, Address, CalloraRegistryClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let catalog_id = env.register(MockCatalog, ());
    let registry_id = env.register(CalloraRegistry, ());
    let client = CalloraRegistryClient::new(&env, &registry_id);
    client.init(&admin, &catalog_id);
    (env, admin, client)
}

/// Register a single offering and return the `(Env, admin, client)` triple
/// with one entry already stored.
fn setup_with_offering() -> (Env, Address, CalloraRegistryClient<'static>) {
    let (env, admin, client) = setup();
    let developer = Address::generate(&env);
    client.register_offering(
        &admin,
        &developer,
        &String::from_str(&env, "bench-offering-0"),
        &String::from_str(&env, "https://meta.example.com/offering-0"),
    );
    (env, admin, client)
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

/// Benchmark the first-time `init` call, registering fresh contracts each
/// iteration so each call sees an un-initialized state.
fn bench_init(c: &mut Criterion) {
    c.bench_function("registry/init", |b| {
        b.iter(|| {
            let env = Env::default();
            env.mock_all_auths();
            let admin = Address::generate(&env);
            let catalog_id = env.register(MockCatalog, ());
            let registry_id = env.register(CalloraRegistry, ());
            let client = CalloraRegistryClient::new(&env, &registry_id);
            criterion::black_box(client.init(&admin, &catalog_id));
        })
    });
}

/// Benchmark `register_offering` on an already-initialized registry.
///
/// Each iteration uses a distinct offering-id so no `OfferingAlreadyRegistered`
/// error fires.
fn bench_register_offering(c: &mut Criterion) {
    let (env, admin, client) = setup();
    let developer = Address::generate(&env);
    let mut counter: u32 = 0;
    c.bench_function("registry/register_offering", |b| {
        b.iter(|| {
            counter += 1;
            // Soroban String cannot be built from a runtime-computed Rust
            // String in no_std mode, so we use a fixed set of ids that are
            // long enough to be realistic but distinct across iterations.
            let id_str = alloc_id_string(&env, counter);
            criterion::black_box(client.register_offering(
                &admin,
                &developer,
                &id_str,
                &String::from_str(&env, "https://meta.example.com/bench"),
            ))
        })
    });
}

/// Benchmark `register_offering_with_gate` on an already-initialized registry.
///
/// Uses a mock token that returns a fixed high balance so the gate passes.
/// Each iteration uses a distinct offering-id to avoid duplicate errors.
fn bench_register_offering_with_gate(c: &mut Criterion) {
    let (env, admin, client) = setup();
    let developer = Address::generate(&env);
    let token_id = env.register(MockToken, ());
    let mut counter: u32 = 0;
    c.bench_function("registry/register_offering_with_gate", |b| {
        b.iter(|| {
            counter += 1;
            let id_str = alloc_id_string(&env, counter);
            criterion::black_box(client.register_offering_with_gate(
                &admin,
                &developer,
                &token_id,
                &1_000i128,
                &id_str,
                &String::from_str(&env, "https://meta.example.com/bench"),
            ))
        })
    });
}

/// Benchmark `is_offering_registered` for an offering that is present.
fn bench_is_offering_registered_hit(c: &mut Criterion) {
    let (env, _admin, client) = setup_with_offering();
    let id = String::from_str(&env, "bench-offering-0");
    c.bench_function("registry/is_offering_registered (hit)", |b| {
        b.iter(|| criterion::black_box(client.is_offering_registered(&id)))
    });
}

/// Benchmark `is_offering_registered` for an offering that is absent.
fn bench_is_offering_registered_miss(c: &mut Criterion) {
    let (env, _admin, client) = setup();
    let id = String::from_str(&env, "no-such-offering");
    c.bench_function("registry/is_offering_registered (miss)", |b| {
        b.iter(|| criterion::black_box(client.is_offering_registered(&id)))
    });
}

/// Benchmark `registered_count` view.
fn bench_registered_count(c: &mut Criterion) {
    let (_env, _admin, client) = setup_with_offering();
    c.bench_function("registry/registered_count", |b| {
        b.iter(|| criterion::black_box(client.registered_count()))
    });
}

/// Benchmark `get_offering` record retrieval for a registered id.
fn bench_get_offering(c: &mut Criterion) {
    let (env, _admin, client) = setup_with_offering();
    let id = String::from_str(&env, "bench-offering-0");
    c.bench_function("registry/get_offering", |b| {
        b.iter(|| criterion::black_box(client.get_offering(&id)))
    });
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a distinct offering-id `String` for each iteration counter.
///
/// We use a fixed-width decimal suffix (zero-padded to 8 digits) so the
/// string always satisfies `MAX_OFFERING_ID_LEN` (64 chars) and avoids
/// collisions across benchmark iterations.
fn alloc_id_string(env: &Env, counter: u32) -> String {
    // Max u32 is 10 digits; pad to 8 with leading zeros.
    let digits = [
        b'0' + ((counter / 10_000_000) % 10) as u8,
        b'0' + ((counter / 1_000_000) % 10) as u8,
        b'0' + ((counter / 100_000) % 10) as u8,
        b'0' + ((counter / 10_000) % 10) as u8,
        b'0' + ((counter / 1_000) % 10) as u8,
        b'0' + ((counter / 100) % 10) as u8,
        b'0' + ((counter / 10) % 10) as u8,
        b'0' + (counter % 10) as u8,
    ];
    // Safety: all bytes are ASCII digits.
    let suffix = core::str::from_utf8(&digits).expect("digits are valid ASCII");
    let id = std::format!("bench-offering-{suffix}");
    String::from_str(env, &id)
}

criterion_group!(
    benches,
    bench_init,
    bench_register_offering,
    bench_register_offering_with_gate,
    bench_is_offering_registered_hit,
    bench_is_offering_registered_miss,
    bench_registered_count,
    bench_get_offering,
);
criterion_main!(benches);
