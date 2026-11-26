//! # Gas & Memory Snapshot Tests — `callora-refund`
//!
//! Each test exercises one public refund-lifecycle entrypoint, then reads the
//! host's CPU and memory counters via `env.cost_estimate().resources()` and
//! prints a single JSON line to stdout:
//!
//! ```json
//! {"contract":"callora-refund","entrypoint":"request_refund","cpu":341234,"mem":33210}
//! ```
//!
//! `scripts/gas-regression.sh` harvests those lines, compares them against
//! `contracts/.gas-baseline.json`, and fails CI when any metric grows by more
//! than 5 %.

use callora_refund::{RefundContract, RefundContractClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token;
use soroban_sdk::{Address, Env, Symbol};

fn emit(entrypoint: &str, cpu: u64, mem: u64) {
    println!(
        "{{\"contract\":\"callora-refund\",\"entrypoint\":\"{entrypoint}\",\"cpu\":{cpu},\"mem\":{mem}}}"
    );
}

macro_rules! measure {
    ($env:expr, $ep:literal, $body:expr) => {{
        $body;
        let res = $env.cost_estimate().resources();
        let cpu = res.instructions as u64;
        let mem = res.read_bytes as u64 + res.write_bytes as u64;
        emit($ep, cpu, mem);
    }};
}

struct Fixture<'a> {
    env: Env,
    contract: Address,
    client: RefundContractClient<'a>,
    admin: Address,
    requester: Address,
    token: Address,
    usdc: token::StellarAssetClient<'a>,
}

/// Deploy and initialise the contract with a real Stellar Asset Contract used
/// as the refunded token.
fn setup() -> Fixture<'static> {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let usdc = token::StellarAssetClient::new(env, &token);

    let contract = env.register(RefundContract, ());
    let client = RefundContractClient::new(env, &contract);
    client.init(&admin, &250, &100);

    Fixture {
        env: env.clone(),
        contract,
        client,
        admin,
        requester: Address::generate(env),
        token,
        usdc,
    }
}

fn reason(env: &Env) -> Symbol {
    Symbol::new(env, "reason")
}

#[test]
fn gas_snap_request_refund() {
    let f = setup();
    measure!(f.env, "request_refund", {
        f.client
            .request_refund(&f.requester, &f.token, &1_000, &reason(&f.env));
    });
}

#[test]
fn gas_snap_approve_refund() {
    let f = setup();
    let request_id = f
        .client
        .request_refund(&f.requester, &f.token, &1_000, &reason(&f.env));
    measure!(f.env, "approve_refund", {
        f.client.approve_refund(&f.admin, &request_id);
    });
}

#[test]
fn gas_snap_process_refund() {
    let f = setup();
    let request_id = f
        .client
        .request_refund(&f.requester, &f.token, &1_000, &reason(&f.env));
    f.client.approve_refund(&f.admin, &request_id);
    f.usdc.mint(&f.contract, &1_000);
    measure!(f.env, "process_refund", {
        f.client.process_refund(&f.admin, &request_id);
    });
}
