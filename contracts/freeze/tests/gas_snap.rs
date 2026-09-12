#![cfg(test)]

extern crate std;
use std::println;

use callora_freeze::{CalloraFreeze, CalloraFreezeClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, Symbol};

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
        "{{\"contract\":\"callora-freeze\",\"entrypoint\":\"{entrypoint}\",\"cpu\":{cpu},\"mem\":{mem}}}",
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

fn setup() -> (Env, CalloraFreezeClient<'static>, Address) {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract_id = env.register(CalloraFreeze, ());
    let client = CalloraFreezeClient::new(env, &contract_id);
    client.init(&admin);
    (env.clone(), client, admin)
}

#[test]
fn gas_snap_init() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraFreeze, ());
    let client = CalloraFreezeClient::new(&env, &contract_id);
    measure_snap!(env, snap, {
        client.init(&admin);
    });
    emit("init", snap);
}

#[test]
fn gas_snap_freeze() {
    let (env, client, admin) = setup();
    let reason = Symbol::new(&env, "emergency");
    measure_snap!(env, snap, {
        client.freeze(&admin, &reason);
    });
    emit("freeze", snap);
}

#[test]
fn gas_snap_unfreeze() {
    let (env, client, admin) = setup();
    let reason = Symbol::new(&env, "emergency");
    client.freeze(&admin, &reason);
    measure_snap!(env, snap, {
        client.unfreeze(&admin);
    });
    emit("unfreeze", snap);
}

#[test]
fn gas_snap_set_freeze_operator() {
    let (env, client, admin) = setup();
    let operator = Address::generate(&env);
    measure_snap!(env, snap, {
        client.set_freeze_operator(&admin, &Some(operator));
    });
    emit("set_freeze_operator", snap);
}

#[test]
fn gas_snap_is_frozen() {
    let (env, client, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.is_frozen();
    });
    emit("is_frozen", snap);
}

#[test]
fn gas_snap_get_admin() {
    let (env, client, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.get_admin();
    });
    emit("get_admin", snap);
}

#[test]
fn gas_snap_get_freeze_operator() {
    let (env, client, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.get_freeze_operator();
    });
    emit("get_freeze_operator", snap);
}
