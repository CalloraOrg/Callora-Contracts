//! # Gas & Memory Snapshot Tests — `callora-batch-claim`
//!
//! Each test exercises one public claim-lifecycle entrypoint, then reads the
//! host's CPU and memory counters via `env.cost_estimate().resources()` and
//! prints a single JSON line to stdout:
//!
//! ```json
//! {"contract":"callora-batch-claim","entrypoint":"batch_claim","cpu":341234,"mem":33210}
//! ```
//!
//! `scripts/gas-regression.sh` harvests those lines, compares them against
//! `contracts/.gas-baseline.json`, and fails CI when any metric grows by more
//! than 5 %.

use callora_batch_claim::{CalloraBatchClaim, CalloraBatchClaimClient};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token;
use soroban_sdk::{Address, BytesN, Env, Vec};

fn emit(entrypoint: &str, cpu: u64, mem: u64) {
    println!(
        "{{\"contract\":\"callora-batch-claim\",\"entrypoint\":\"{entrypoint}\",\"cpu\":{cpu},\"mem\":{mem}}}"
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
    client: CalloraBatchClaimClient<'a>,
    admin: Address,
    claimant: Address,
}

/// Deploy and initialise the contract with a real Stellar Asset Contract used
/// as the payout token, funding the contract so settlement succeeds.
fn setup() -> Fixture<'static> {
    let env: &'static Env = Box::leak(Box::new(Env::default()));
    env.mock_all_auths();

    let admin = Address::generate(env);
    let token = env
        .register_stellar_asset_contract_v2(admin.clone())
        .address();
    let usdc = token::StellarAssetClient::new(env, &token);

    let contract = env.register(CalloraBatchClaim, ());
    let client = CalloraBatchClaimClient::new(env, &contract);
    client.init(&admin);
    client.set_token(&admin, &token);
    usdc.mint(&contract, &1_000_000);

    Fixture {
        env: env.clone(),
        client,
        admin,
        claimant: Address::generate(env),
    }
}

fn claim_id(env: &Env, seed: u8) -> BytesN<32> {
    BytesN::from_array(env, &[seed; 32])
}

#[test]
fn gas_snap_add_claim() {
    let f = setup();
    let id = claim_id(&f.env, 1);
    measure!(f.env, "add_claim", {
        f.client.add_claim(&f.admin, &f.claimant, &1_000, &id);
    });
}

#[test]
fn gas_snap_batch_claim() {
    let f = setup();
    let id = claim_id(&f.env, 2);
    f.client.add_claim(&f.admin, &f.claimant, &1_000, &id);

    let mut claimants = Vec::new(&f.env);
    claimants.push_back((f.claimant.clone(), id));
    measure!(f.env, "batch_claim", {
        f.client.batch_claim(&claimants);
    });
}
