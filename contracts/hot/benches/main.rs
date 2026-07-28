use callora_hot::{CalloraHot, CalloraHotClient, ACTION_PAUSE};
use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::{Address, Env, Symbol};

const COOLDOWN_SECS: u64 = 1;

fn setup() -> (Env, Address, CalloraHotClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let signer = Address::generate(&env);
    let contract_id = env.register(CalloraHot, ());
    let client = CalloraHotClient::new(&env, &contract_id);
    client.init(&admin, &signer, &Some(COOLDOWN_SECS));
    (env, admin, client)
}

fn bench_is_paused(c: &mut Criterion) {
    let (_env, _admin, client) = setup();
    c.bench_function("hot/is_paused", |b| b.iter(|| client.is_paused()));
}

fn bench_get_admin(c: &mut Criterion) {
    let (_env, _admin, client) = setup();
    c.bench_function("hot/get_admin", |b| {
        b.iter(|| criterion::black_box(client.get_admin()))
    });
}

fn bench_get_signer(c: &mut Criterion) {
    let (_env, _admin, client) = setup();
    c.bench_function("hot/get_signer", |b| {
        b.iter(|| criterion::black_box(client.get_signer()))
    });
}

fn bench_get_cooldown(c: &mut Criterion) {
    let (_env, _admin, client) = setup();
    c.bench_function("hot/get_cooldown", |b| {
        b.iter(|| criterion::black_box(client.get_cooldown()))
    });
}

fn bench_get_pending_admin(c: &mut Criterion) {
    let (_env, _admin, client) = setup();
    c.bench_function("hot/get_pending_admin", |b| {
        b.iter(|| criterion::black_box(client.get_pending_admin()))
    });
}

fn bench_cooldown_remaining(c: &mut Criterion) {
    let (env, _admin, client) = setup();
    let action = Symbol::new(&env, ACTION_PAUSE);
    c.bench_function("hot/cooldown_remaining", |b| {
        b.iter(|| criterion::black_box(client.cooldown_remaining(&action)))
    });
}

fn bench_is_ready(c: &mut Criterion) {
    let (env, _admin, client) = setup();
    let action = Symbol::new(&env, ACTION_PAUSE);
    c.bench_function("hot/is_ready", |b| {
        b.iter(|| criterion::black_box(client.is_ready(&action)))
    });
}

fn bench_pause(c: &mut Criterion) {
    let (env, admin, client) = setup();
    c.bench_function("hot/pause", |b| {
        b.iter(|| {
            env.ledger()
                .set_timestamp(env.ledger().timestamp() + COOLDOWN_SECS + 1);
            client.pause(&admin)
        })
    });
}

fn bench_unpause(c: &mut Criterion) {
    let (env, admin, client) = setup();
    c.bench_function("hot/unpause", |b| {
        b.iter(|| {
            env.ledger()
                .set_timestamp(env.ledger().timestamp() + COOLDOWN_SECS + 1);
            client.unpause(&admin)
        })
    });
}

fn bench_rotate_signer(c: &mut Criterion) {
    let (env, admin, client) = setup();
    c.bench_function("hot/rotate_signer", |b| {
        b.iter(|| {
            env.ledger()
                .set_timestamp(env.ledger().timestamp() + COOLDOWN_SECS + 1);
            let new_signer = Address::generate(&env);
            client.rotate_signer(&admin, &new_signer)
        })
    });
}

fn bench_set_cooldown(c: &mut Criterion) {
    let (_env, admin, client) = setup();
    c.bench_function("hot/set_cooldown", |b| {
        b.iter(|| client.set_cooldown(&admin, &60))
    });
}

fn bench_set_admin(c: &mut Criterion) {
    let (env, admin, client) = setup();
    c.bench_function("hot/set_admin", |b| {
        b.iter(|| {
            let new_admin = Address::generate(&env);
            client.set_admin(&admin, &new_admin)
        })
    });
}

fn bench_accept_admin(c: &mut Criterion) {
    let (env, admin, client) = setup();
    let mut current_admin = admin;
    let mut new_admin = Address::generate(&env);
    c.bench_function("hot/accept_admin", |b| {
        b.iter(|| {
            client.set_admin(&current_admin, &new_admin);
            client.accept_admin(&new_admin);
            current_admin = new_admin.clone();
            new_admin = Address::generate(&env);
        })
    });
}

criterion_group!(
    benches,
    bench_is_paused,
    bench_get_admin,
    bench_get_signer,
    bench_get_cooldown,
    bench_get_pending_admin,
    bench_cooldown_remaining,
    bench_is_ready,
    bench_pause,
    bench_unpause,
    bench_rotate_signer,
    bench_set_cooldown,
    bench_set_admin,
    bench_accept_admin,
);
criterion_main!(benches);
