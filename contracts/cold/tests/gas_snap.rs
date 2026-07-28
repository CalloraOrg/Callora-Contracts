/// # Gas Budget Regression Tests — `callora-cold`
///
/// Each test exercises one public entrypoint, then reads the host's CPU and
/// memory counters via `env.cost_estimate().resources()` and prints a single
/// JSON line to stdout:
///
/// ```json
/// {"contract":"callora-cold","entrypoint":"capabilities","cpu":341234,"mem":33210}
/// ```
///
/// `scripts/gas-regression.sh` harvests those lines, compares them against
/// `contracts/.gas-baseline.json`, and fails CI when any metric grows by more
/// than 5 %.
use soroban_sdk::{Env};

use callora_cold::{CalloraCold, CalloraColdClient};

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

fn setup(env: &Env) -> CalloraColdClient {
    let contract_id = env.register(CalloraCold, ());
    CalloraColdClient::new(env, &contract_id)
}

// ---------------------------------------------------------------------------
// Gas snapshot tests (one per entrypoint)
// ---------------------------------------------------------------------------

#[test]
fn gas_budget_capabilities() {
    let env = Env::default();
    let client = setup(&env);
    measure!(env, "capabilities", {
        let _ = client.capabilities();
    });
}

/// Sanity: verify budget API returns non-zero values after a real call.
#[test]
fn gas_budget_sanity_nonzero() {
    let env = Env::default();
    let client = setup(&env);
    client.capabilities();
    let res = env.cost_estimate().resources();
    assert!(res.instructions > 0, "CPU must be >0");
    assert!(res.read_bytes + res.write_bytes > 0, "mem must be >0");
}
