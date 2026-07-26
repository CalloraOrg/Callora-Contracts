/// # Gas Budget Regression Tests — `callora-cold`
///
/// Each test exercises one public entrypoint, then reads the host's CPU and
/// memory counters via `env.cost_estimate().resources()` and prints a single
/// JSON line to stdout:
///
/// ```json
/// {"contract":"callora-cold","entrypoint":"init","cpu":341234,"mem":33210}
/// ```
///
/// `scripts/gas-regression.sh` harvests those lines, compares them against
/// `contracts/.gas-baseline.json`, and fails CI when any metric grows by more
/// than 5 %.
use soroban_sdk::{testutils::Address as _, Address, Env, Vec};

use callora_cold::{ColdBalances, ColdConfig, ColdStorage, ColdStorageClient};

fn emit(entrypoint: &str, cpu: u64, mem: u64) {
    println!(
        "{{\"contract\":\"callora-cold\",\"entrypoint\":\"{entrypoint}\",\"cpu\":{cpu},\"mem\":{mem}}}"
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

fn addr(env: &Env, seed: u8) -> Address {
    let _ = seed;
    Address::generate(env)
}

fn make_config(env: &Env) -> ColdConfig {
    let mut signers: Vec<Address> = Vec::new(env);
    signers.push_back(addr(env, 1));
    signers.push_back(addr(env, 2));
    signers.push_back(addr(env, 3));
    ColdConfig {
        hot_bps: 2000,
        rebalance_threshold_bps: 500,
        cold_signers: signers,
        cold_threshold: 2,
    }
}

fn setup(env: &Env) -> (ColdStorageClient, Address) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let contract_id = env.register(ColdStorage, ());
    let client = ColdStorageClient::new(env, &contract_id);
    client.init(&admin).unwrap();
    (client, admin)
}

// ---------------------------------------------------------------------------
// Gas snapshot tests (one per entrypoint)
// ---------------------------------------------------------------------------

#[test]
fn gas_budget_init() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(ColdStorage, ());
    let client = ColdStorageClient::new(&env, &contract_id);
    measure!(env, "init", {
        let _ = client.init(&admin);
    });
}

#[test]
fn gas_budget_set_config() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let config = make_config(&env);
    measure!(env, "set_config", {
        let _ = client.set_config(&admin, &config);
    });
}

#[test]
fn gas_budget_get_config() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let config = make_config(&env);
    client.set_config(&admin, &config).unwrap();
    measure!(env, "get_config", {
        let _ = client.get_config();
    });
}

#[test]
fn gas_budget_set_balances() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let config = make_config(&env);
    client.set_config(&admin, &config).unwrap();
    let balances = ColdBalances {
        hot: 1000,
        cold: 4000,
    };
    measure!(env, "set_balances", {
        let _ = client.set_balances(&admin, &balances);
    });
}

#[test]
fn gas_budget_get_balances() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let balances = ColdBalances {
        hot: 1000,
        cold: 4000,
    };
    client.set_balances(&admin, &balances).unwrap();
    measure!(env, "get_balances", {
        let _ = client.get_balances();
    });
}

#[test]
fn gas_budget_propose_cold_sweep() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let config = make_config(&env);
    client.set_config(&admin, &config).unwrap();
    let signer1 = config.cold_signers.get(0);
    let dest = addr(&env, 99);
    measure!(env, "propose_cold_sweep", {
        let _ = client.propose_cold_sweep(&signer1, &500, &dest);
    });
}

#[test]
fn gas_budget_approve_cold_sweep() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let config = make_config(&env);
    client.set_config(&admin, &config).unwrap();
    let signer1 = config.cold_signers.get(0);
    let signer2 = config.cold_signers.get(1);
    let dest = addr(&env, 99);
    client.propose_cold_sweep(&signer1, &500, &dest).unwrap();
    measure!(env, "approve_cold_sweep", {
        let _ = client.approve_cold_sweep(&signer2);
    });
}

#[test]
fn gas_budget_get_cold_sweep() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let config = make_config(&env);
    client.set_config(&admin, &config).unwrap();
    let signer1 = config.cold_signers.get(0);
    let dest = addr(&env, 99);
    client.propose_cold_sweep(&signer1, &500, &dest).unwrap();
    measure!(env, "get_cold_sweep", {
        let _ = client.get_cold_sweep();
    });
}

#[test]
fn gas_budget_hot_share_bps() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let balances = ColdBalances {
        hot: 2000,
        cold: 8000,
    };
    client.set_balances(&admin, &balances).unwrap();
    measure!(env, "hot_share_bps", {
        let _ = client.hot_share_bps();
    });
}

#[test]
fn gas_budget_target_hot_amount() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let config = make_config(&env);
    client.set_config(&admin, &config).unwrap();
    let balances = ColdBalances {
        hot: 5000,
        cold: 5000,
    };
    client.set_balances(&admin, &balances).unwrap();
    measure!(env, "target_hot_amount", {
        let _ = client.target_hot_amount();
    });
}

#[test]
fn gas_budget_get_admin() {
    let env = Env::default();
    let (client, _) = setup(&env);
    measure!(env, "get_admin", {
        let _ = client.get_admin();
    });
}

#[test]
fn gas_budget_set_admin() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let new_admin = Address::generate(&env);
    measure!(env, "set_admin", {
        let _ = client.set_admin(&admin, &new_admin);
    });
}

#[test]
fn gas_budget_accept_admin() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin).unwrap();
    measure!(env, "accept_admin", {
        let _ = client.accept_admin(&new_admin);
    });
}

#[test]
fn gas_budget_get_pending_admin() {
    let env = Env::default();
    let (client, admin) = setup(&env);
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin).unwrap();
    measure!(env, "get_pending_admin", {
        let _ = client.get_pending_admin();
    });
}

/// Sanity: verify budget API returns non-zero values after a real call.
#[test]
fn gas_budget_sanity_nonzero() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract_id = env.register(ColdStorage, ());
    let client = ColdStorageClient::new(&env, &contract_id);
    client.init(&admin).unwrap();
    let res = env.cost_estimate().resources();
    assert!(res.instructions > 0, "CPU must be >0");
    assert!(res.read_bytes + res.write_bytes > 0, "mem must be >0");
}
