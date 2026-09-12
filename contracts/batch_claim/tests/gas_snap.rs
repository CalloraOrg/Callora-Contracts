#![cfg(test)]

extern crate std;
use std::println;

use callora_batch_claim::{CalloraBatchClaim, CalloraBatchClaimClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env, Vec};

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
        "{{\"contract\":\"callora-batch-claim\",\"entrypoint\":\"{entrypoint}\",\"cpu\":{cpu},\"mem\":{mem}}}",
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

fn make_id(env: &Env, val: u8) -> BytesN<32> {
    let mut bytes = [0u8; 32];
    bytes[0] = val;
    BytesN::from_array(env, &bytes)
}

fn setup() -> (Env, CalloraBatchClaimClient<'static>, Address) {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract_id = env.register(CalloraBatchClaim, ());
    let client = CalloraBatchClaimClient::new(env, &contract_id);
    client.init(&admin);
    (env.clone(), client, admin)
}

#[test]
fn gas_snap_init() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(CalloraBatchClaim, ());
    let client = CalloraBatchClaimClient::new(&env, &contract_id);
    measure_snap!(env, snap, {
        client.init(&admin);
    });
    emit("init", snap);
}

#[test]
fn gas_snap_add_claim() {
    let (env, client, admin) = setup();
    let claimant = Address::generate(&env);
    let claim_id = make_id(&env, 1);
    measure_snap!(env, snap, {
        client.add_claim(&admin, &claimant, &100, &claim_id);
    });
    emit("add_claim", snap);
}

#[test]
fn gas_snap_batch_claim() {
    let (env, client, admin) = setup();
    let claimant = Address::generate(&env);
    let claim_id = make_id(&env, 2);
    client.add_claim(&admin, &claimant, &100, &claim_id);
    let mut batch = Vec::new(&env);
    batch.push_back((claimant, claim_id));
    measure_snap!(env, snap, {
        let _ = client.batch_claim(&batch);
    });
    emit("batch_claim", snap);
}

#[test]
fn gas_snap_cancel_claim() {
    let (env, client, admin) = setup();
    let claimant = Address::generate(&env);
    let claim_id = make_id(&env, 3);
    client.add_claim(&admin, &claimant, &100, &claim_id);
    measure_snap!(env, snap, {
        client.cancel_claim(&admin, &claimant);
    });
    emit("cancel_claim", snap);
}

#[test]
fn gas_snap_extend_claim_consumed_ttl() {
    let (env, client, admin) = setup();
    let claimant = Address::generate(&env);
    let claim_id = make_id(&env, 4);
    client.add_claim(&admin, &claimant, &100, &claim_id);
    measure_snap!(env, snap, {
        let _ = client.extend_claim_consumed_ttl(&claim_id);
    });
    emit("extend_claim_consumed_ttl", snap);
}

#[test]
fn gas_snap_get_claim() {
    let (env, client, admin) = setup();
    let claimant = Address::generate(&env);
    let claim_id = make_id(&env, 5);
    client.add_claim(&admin, &claimant, &100, &claim_id);
    measure_snap!(env, snap, {
        let _ = client.get_claim(&claimant);
    });
    emit("get_claim", snap);
}

#[test]
fn gas_snap_has_claim() {
    let (env, client, admin) = setup();
    let claimant = Address::generate(&env);
    let claim_id = make_id(&env, 6);
    client.add_claim(&admin, &claimant, &100, &claim_id);
    measure_snap!(env, snap, {
        let _ = client.has_claim(&claimant);
    });
    emit("has_claim", snap);
}

#[test]
fn gas_snap_claim_id_consumed() {
    let (env, client, admin) = setup();
    let claimant = Address::generate(&env);
    let claim_id = make_id(&env, 7);
    client.add_claim(&admin, &claimant, &100, &claim_id);
    measure_snap!(env, snap, {
        let _ = client.claim_id_consumed(&claim_id);
    });
    emit("claim_id_consumed", snap);
}

#[test]
fn gas_snap_claim_id_owner() {
    let (env, client, admin) = setup();
    let claimant = Address::generate(&env);
    let claim_id = make_id(&env, 8);
    client.add_claim(&admin, &claimant, &100, &claim_id);
    measure_snap!(env, snap, {
        let _ = client.claim_id_owner(&claim_id);
    });
    emit("claim_id_owner", snap);
}

#[test]
fn gas_snap_claim_id_reserved() {
    let (env, client, admin) = setup();
    let claimant = Address::generate(&env);
    let claim_id = make_id(&env, 9);
    client.add_claim(&admin, &claimant, &100, &claim_id);
    measure_snap!(env, snap, {
        let _ = client.claim_id_reserved(&claim_id);
    });
    emit("claim_id_reserved", snap);
}

#[test]
fn gas_snap_total_claims() {
    let (env, client, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.total_claims();
    });
    emit("total_claims", snap);
}

#[test]
fn gas_snap_get_admin() {
    let (env, client, _) = setup();
    measure_snap!(env, snap, {
        let _ = client.get_admin();
    });
    emit("get_admin", snap);
}
