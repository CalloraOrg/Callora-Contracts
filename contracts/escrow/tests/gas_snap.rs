//! # Gas & Memory Snapshot Tests — `callora-escrow`
//!
//! Each test exercises one public fund-moving entrypoint, then reads the
//! host's CPU and memory counters via `env.cost_estimate().resources()` and
//! prints a single JSON line to stdout:
//!
//! ```json
//! {"contract":"callora-escrow","entrypoint":"release","cpu":341234,"mem":33210}
//! ```
//!
//! `scripts/gas-regression.sh` harvests those lines, compares them against
//! `contracts/.gas-baseline.json`, and fails CI when any metric grows by more
//! than 5 %.

use callora_escrow::{CalloraEscrow, CalloraEscrowClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token;
use soroban_sdk::{Address, Env};

fn emit(entrypoint: &str, cpu: u64, mem: u64) {
    println!(
        "{{\"contract\":\"callora-escrow\",\"entrypoint\":\"{entrypoint}\",\"cpu\":{cpu},\"mem\":{mem}}}"
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
    client: CalloraEscrowClient<'a>,
    admin: Address,
    asset: Address,
    recipient: Address,
}

/// Deploy and initialise the contract, register an approved payment asset and
/// fund the admin so escrow creation can transfer tokens in.
fn setup() -> Fixture<'static> {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let admin = Address::generate(env);
    let signer = Address::generate(env);
    let asset = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let usdc = token::StellarAssetClient::new(env, &asset);

    let contract = env.register(CalloraEscrow, ());
    let client = CalloraEscrowClient::new(env, &contract);
    client.init(&admin, &signer, &None);
    client.add_approved_asset(&admin, &asset);
    usdc.mint(&admin, &1_000_000);

    Fixture {
        env: env.clone(),
        client,
        admin,
        asset,
        recipient: Address::generate(env),
    }
}

#[test]
fn gas_snap_create_escrow() {
    let f = setup();
    let expires_at = f.env.ledger().timestamp() + 1_000;
    measure!(f.env, "create_escrow", {
        f.client
            .create_escrow(&f.admin, &f.asset, &f.recipient, &1_000, &expires_at);
    });
}

#[test]
fn gas_snap_release() {
    let f = setup();
    let expires_at = f.env.ledger().timestamp() + 1_000;
    f.client
        .create_escrow(&f.admin, &f.asset, &f.recipient, &1_000, &expires_at);
    measure!(f.env, "release", {
        f.client.release(&f.admin, &f.asset, &f.recipient);
    });
}
