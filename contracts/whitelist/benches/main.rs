use criterion::{criterion_group, criterion_main, Criterion};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Vec};

use callora_vault::{CalloraVault, CalloraVaultClient};

fn setup() -> (Env, Address, Address, CalloraVaultClient<'static>) {
    let env = Env::default();
    env.mock_all_auths();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let contract_id = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(&env, &contract_id);
    client.init(&owner);
    (env, owner, depositor, client)
}

fn bench_is_authorized_depositor(c: &mut Criterion) {
    let (_env, _owner, depositor, client) = setup();
    c.bench_function("whitelist/is_authorized_depositor", |b| {
        b.iter(|| criterion::black_box(client.is_authorized_depositor(&depositor)))
    });
}

fn bench_get_allowlist(c: &mut Criterion) {
    let (_env, _owner, _depositor, client) = setup();
    c.bench_function("whitelist/get_allowlist", |b| {
        b.iter(|| criterion::black_box(client.get_allowlist()))
    });
}

fn bench_add_address(c: &mut Criterion) {
    let (_env, owner, depositor, client) = setup();
    c.bench_function("whitelist/add_address", |b| {
        b.iter(|| client.add_address(&owner, &depositor))
    });
}

fn bench_clear_all(c: &mut Criterion) {
    let (env, owner, depositor, client) = setup();
    client.add_address(&owner, &depositor);
    c.bench_function("whitelist/clear_all", |b| {
        b.iter(|| client.clear_all(&owner))
    });
}

fn bench_add_address_multiple(c: &mut Criterion) {
    let (env, owner, _depositor, client) = setup();
    let addresses: Vec<Address> = (0..10).map(|_| Address::generate(&env)).collect();
    c.bench_function("whitelist/add_address_10", |b| {
        b.iter(|| {
            for addr in addresses.iter() {
                client.add_address(&owner, addr);
            }
        })
    });
}

fn bench_is_authorized_depositor_with_list(c: &mut Criterion) {
    let (env, owner, depositor, client) = setup();
    client.add_address(&owner, &depositor);
    let stranger = Address::generate(&env);
    c.bench_function("whitelist/is_authorized_depositor_present", |b| {
        b.iter(|| {
            criterion::black_box(client.is_authorized_depositor(&depositor));
            criterion::black_box(client.is_authorized_depositor(&stranger));
        })
    });
}

criterion_group!(
    benches,
    bench_is_authorized_depositor,
    bench_get_allowlist,
    bench_add_address,
    bench_clear_all,
    bench_add_address_multiple,
    bench_is_authorized_depositor_with_list,
);
criterion_main!(benches);
