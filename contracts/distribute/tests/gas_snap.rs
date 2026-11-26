//! # Gas & Memory Snapshot Tests — `callora-distribute`
//!
//! Each test exercises one public fund-moving entrypoint, then reads the
//! host's CPU and memory counters via `env.cost_estimate().resources()` and
//! prints a single JSON line to stdout:
//!
//! ```json
//! {"contract":"callora-distribute","entrypoint":"distribute","cpu":341234,"mem":33210}
//! ```
//!
//! `scripts/gas-regression.sh` harvests those lines, compares them against
//! `contracts/.gas-baseline.json`, and fails CI when any metric grows by more
//! than 5 %.

use callora_distribute::{Distribute, DistributeClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token;
use soroban_sdk::{Address, Env, Vec};

fn emit(entrypoint: &str, cpu: u64, mem: u64) {
    println!(
        "{{\"contract\":\"callora-distribute\",\"entrypoint\":\"{entrypoint}\",\"cpu\":{cpu},\"mem\":{mem}}}"
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
    admin: Address,
    client: DistributeClient<'a>,
}

/// Deploy and initialise the contract with a real Stellar Asset Contract
/// used as USDC, funding the contract so transfers succeed.
fn setup() -> Fixture<'static> {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let admin = Address::generate(env);
    let usdc_addr = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let usdc = token::StellarAssetClient::new(env, &usdc_addr);

    let contract = env.register(Distribute, ());
    let client = DistributeClient::new(env, &contract);
    client.init(&admin, &usdc_addr);
    usdc.mint(&contract, &1_000_000);

    Fixture {
        env: env.clone(),
        admin,
        client,
    }
}

#[test]
fn gas_snap_distribute() {
    let f = setup();
    let recipient = Address::generate(&f.env);
    measure!(f.env, "distribute", {
        f.client.distribute(&f.admin, &recipient, &1_000);
    });
}

#[test]
fn gas_snap_batch_distribute() {
    let f = setup();
    let mut payments = Vec::new(&f.env);
    payments.push_back((Address::generate(&f.env), 500_i128));
    payments.push_back((Address::generate(&f.env), 750_i128));
    measure!(f.env, "batch_distribute", {
        f.client.batch_distribute(&f.admin, &payments);
    });
}
