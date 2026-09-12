#![cfg(test)]

extern crate std;
use std::println;

use callora_distribute::{Distribute, DistributeClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, Vec};

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
        "{{\"contract\":\"callora-distribute\",\"entrypoint\":\"{entrypoint}\",\"cpu\":{cpu},\"mem\":{mem}}}",
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

fn setup() -> (Env, DistributeClient<'static>, Address, Address) {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();
    let admin = Address::generate(env);
    let usdc = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let contract_id = env.register(Distribute, ());
    let client = DistributeClient::new(env, &contract_id);
    client.init(&admin, &usdc);
    (env.clone(), client, admin, usdc)
}

#[test]
fn gas_snap_init() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let usdc = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let contract_id = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract_id);
    measure_snap!(env, snap, {
        client.init(&admin, &usdc);
    });
    emit("init", snap);
}

#[test]
fn gas_snap_distribute() {
    let (env, client, admin, usdc) = setup();
    let usdc_admin = token::StellarAssetClient::new(&env, &usdc);
    usdc_admin.mint(&client.address, &10_000);
    let recipient = Address::generate(&env);
    measure_snap!(env, snap, {
        client.distribute(&admin, &recipient, &100);
    });
    emit("distribute", snap);
}

#[test]
fn gas_snap_batch_distribute() {
    let (env, client, admin, usdc) = setup();
    let usdc_admin = token::StellarAssetClient::new(&env, &usdc);
    usdc_admin.mint(&client.address, &10_000);
    let r1 = Address::generate(&env);
    let r2 = Address::generate(&env);
    let mut payments = Vec::new(&env);
    payments.push_back((r1, 100));
    payments.push_back((r2, 200));
    measure_snap!(env, snap, {
        client.batch_distribute(&admin, &payments);
    });
    emit("batch_distribute", snap);
}

#[test]
fn gas_snap_set_admin() {
    let (env, client, admin, _) = setup();
    let new_admin = Address::generate(&env);
    measure_snap!(env, snap, {
        client.set_admin(&admin, &new_admin);
    });
    emit("set_admin", snap);
}

#[test]
fn gas_snap_accept_admin() {
    let (env, client, admin, _) = setup();
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);
    measure_snap!(env, snap, {
        client.accept_admin(&new_admin);
    });
    emit("accept_admin", snap);
}

#[test]
fn gas_snap_claim_admin() {
    let (env, client, admin, _) = setup();
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);
    measure_snap!(env, snap, {
        client.claim_admin(&new_admin);
    });
    emit("claim_admin", snap);
}

#[test]
fn gas_snap_cancel_admin_transfer() {
    let (env, client, admin, _) = setup();
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);
    measure_snap!(env, snap, {
        client.cancel_admin_transfer(&admin);
    });
    emit("cancel_admin_transfer", snap);
}

#[test]
fn gas_snap_pause() {
    let (env, client, admin, _) = setup();
    measure_snap!(env, snap, {
        client.pause(&admin);
    });
    emit("pause", snap);
}

#[test]
fn gas_snap_unpause() {
    let (env, client, admin, _) = setup();
    client.pause(&admin);
    measure_snap!(env, snap, {
        client.unpause(&admin);
    });
    emit("unpause", snap);
}

#[test]
fn gas_snap_set_max_distribute() {
    let (env, client, admin, _) = setup();
    measure_snap!(env, snap, {
        client.set_max_distribute(&admin, &500);
    });
    emit("set_max_distribute", snap);
}

#[test]
fn gas_snap_get_admin() {
    let (env, client, _, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.get_admin();
    });
    emit("get_admin", snap);
}

#[test]
fn gas_snap_get_usdc_token() {
    let (env, client, _, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.get_usdc_token();
    });
    emit("get_usdc_token", snap);
}

#[test]
fn gas_snap_get_pending_admin() {
    let (env, client, _, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.get_pending_admin();
    });
    emit("get_pending_admin", snap);
}

#[test]
fn gas_snap_get_paused() {
    let (env, client, _, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.get_paused();
    });
    emit("get_paused", snap);
}

#[test]
fn gas_snap_get_max_distribute() {
    let (env, client, _, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.get_max_distribute();
    });
    emit("get_max_distribute", snap);
}

#[test]
fn gas_snap_get_max_batch_size() {
    let (env, client, _, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.get_max_batch_size();
    });
    emit("get_max_batch_size", snap);
}

#[test]
fn gas_snap_balance() {
    let (env, client, _, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.balance();
    });
    emit("balance", snap);
}

#[test]
fn gas_snap_version() {
    let (env, client, _, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.version();
    });
    emit("version", snap);
}
