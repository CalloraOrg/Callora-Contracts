use callora_whitelist::{CalloraWhitelist, CalloraWhitelistClient};
use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env};

const POPULATED_SIZE: usize = 32;
const START_TIMESTAMP: u64 = 1_000_000;

struct Fixture {
    admin: Address,
    member: Address,
    missing: Address,
    client: CalloraWhitelistClient<'static>,
}

fn advance_timestamp(env: &Env) {
    env.ledger()
        .set_timestamp(env.ledger().timestamp().saturating_add(1));
}

fn setup(size: usize) -> Fixture {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().set_timestamp(START_TIMESTAMP);

    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraWhitelist, ());
    let client = CalloraWhitelistClient::new(&env, &contract_id);
    client.init(&admin);
    client.set_admin_cooldown(&admin, &1);

    let mut member = Address::generate(&env);
    for index in 0..size {
        let address = Address::generate(&env);
        client.add_address(&admin, &address);
        if index + 1 == size {
            member = address;
        }
        advance_timestamp(&env);
    }

    let missing = Address::generate(&env);
    Fixture {
        admin,
        member,
        missing,
        client,
    }
}

fn bench_is_whitelisted_member(c: &mut Criterion) {
    let fixture = setup(POPULATED_SIZE);
    c.bench_function("whitelist/is_whitelisted/member/32", |b| {
        b.iter(|| black_box(fixture.client.is_whitelisted(&fixture.member)))
    });
}

fn bench_is_whitelisted_miss(c: &mut Criterion) {
    let fixture = setup(POPULATED_SIZE);
    c.bench_function("whitelist/is_whitelisted/miss/32", |b| {
        b.iter(|| black_box(fixture.client.is_whitelisted(&fixture.missing)))
    });
}

fn bench_get_whitelist(c: &mut Criterion) {
    let fixture = setup(POPULATED_SIZE);
    c.bench_function("whitelist/get_whitelist/32", |b| {
        b.iter(|| black_box(fixture.client.get_whitelist()))
    });
}

fn bench_add_address_empty(c: &mut Criterion) {
    c.bench_function("whitelist/add_address/empty", |b| {
        b.iter_batched(
            || setup(0),
            |fixture| fixture.client.add_address(&fixture.admin, &fixture.missing),
            BatchSize::SmallInput,
        )
    });
}

fn bench_add_address_populated(c: &mut Criterion) {
    c.bench_function("whitelist/add_address/32", |b| {
        b.iter_batched(
            || setup(POPULATED_SIZE),
            |fixture| fixture.client.add_address(&fixture.admin, &fixture.missing),
            BatchSize::SmallInput,
        )
    });
}

fn bench_remove_address(c: &mut Criterion) {
    c.bench_function("whitelist/remove_address/32", |b| {
        b.iter_batched(
            || setup(POPULATED_SIZE),
            |fixture| {
                fixture
                    .client
                    .remove_address(&fixture.admin, &fixture.member)
            },
            BatchSize::SmallInput,
        )
    });
}

fn bench_clear_all(c: &mut Criterion) {
    c.bench_function("whitelist/clear_all/32", |b| {
        b.iter_batched(
            || setup(POPULATED_SIZE),
            |fixture| fixture.client.clear_all(&fixture.admin),
            BatchSize::SmallInput,
        )
    });
}

criterion_group!(
    benches,
    bench_is_whitelisted_member,
    bench_is_whitelisted_miss,
    bench_get_whitelist,
    bench_add_address_empty,
    bench_add_address_populated,
    bench_remove_address,
    bench_clear_all,
);
criterion_main!(benches);
