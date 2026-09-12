#![cfg(test)]

extern crate std;
use std::println;

use callora_rescue::{CalloraRescue, CalloraRescueClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env};

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
}

fn emit(entrypoint: &str, snap: ProfileSnapshot) {
    println!(
        "{{\"contract\":\"callora-rescue\",\"entrypoint\":\"{entrypoint}\",\"cpu\":{cpu},\"mem\":{mem}}}",
        cpu = snap.cpu,
        mem = snap.mem,
    );
}

macro_rules! measure_snap {
    ($env:expr, $snap:ident, $body:expr) => {
        $body;
        let $snap = ProfileSnapshot::capture(&$env);
    };
}

fn setup() -> (Env, CalloraRescueClient<'static>, Address, Address) {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token_addr = env.register_stellar_asset_contract_v2(token_admin.clone()).address();
    let contract_id = env.register(CalloraRescue, ());
    let client = CalloraRescueClient::new(env, &contract_id);
    client.init(&admin);
    let asset_client = token::StellarAssetClient::new(env, &token_addr);
    asset_client.mint(&client.address, &10_000);
    (env.clone(), client, admin, token_addr)
}

#[test]
fn gas_snap_init() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraRescue, ());
    let client = CalloraRescueClient::new(&env, &contract_id);
    measure_snap!(env, snap, {
        client.init(&admin);
    });
    emit("init", snap);
}

#[test]
fn gas_snap_rescue() {
    let (env, client, admin, token_addr) = setup();
    let to = Address::generate(&env);
    measure_snap!(env, snap, {
        client.rescue(&admin, &token_addr, &to, &100);
    });
    emit("rescue", snap);
}

#[test]
fn gas_snap_rescue_capped() {
    let (env, client, admin, token_addr) = setup();
    let to = Address::generate(&env);
    measure_snap!(env, snap, {
        client.rescue_capped(&admin, &token_addr, &to, &100, &500);
    });
    emit("rescue_capped", snap);
}

#[test]
fn gas_snap_total_rescued() {
    let (env, client, _, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.total_rescued();
    });
    emit("total_rescued", snap);
}

#[test]
fn gas_snap_get_admin() {
    let (env, client, _, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.get_admin();
    });
    emit("get_admin", snap);
}
