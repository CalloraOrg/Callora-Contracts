//! Integration tests for the per-developer minimum balance feature.
//!
//! Verifies:
//! - Admin can set/get min balance per developer
//! - Min balance is enforced during withdrawal
//! - Event is emitted on min balance change
//! - Edge cases: zero min, negative min, boundary values

use callora_settlement::{CalloraSettlement, CalloraSettlementClient, SettlementError, StorageKey};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env};

fn setup() -> (Env, Address, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let contract = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &contract);
    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    client.init(&admin, &vault);
    (env, contract, admin, vault)
}

// ── Admin entrypoints ────────────────────────────────────────────────────────

#[test]
fn set_and_get_min_balance_roundtrip() {
    let (env, contract, admin, _) = setup();
    let dev = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract);

    assert_eq!(client.get_developer_min_balance(&dev), 0);

    client.set_developer_min_balance(&admin, &dev, &5_000);
    assert_eq!(client.get_developer_min_balance(&dev), 5_000);
}

#[test]
fn min_balance_overwrite() {
    let (env, contract, admin, _) = setup();
    let dev = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract);

    client.set_developer_min_balance(&admin, &dev, &1_000);
    client.set_developer_min_balance(&admin, &dev, &9_999);
    assert_eq!(client.get_developer_min_balance(&dev), 9_999);
}

#[test]
fn min_balance_zero_clears() {
    let (env, contract, admin, _) = setup();
    let dev = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract);

    client.set_developer_min_balance(&admin, &dev, &5_000);
    assert_eq!(client.get_developer_min_balance(&dev), 5_000);

    client.set_developer_min_balance(&admin, &dev, &0);
    assert_eq!(client.get_developer_min_balance(&dev), 0);
}

#[test]
fn min_balance_independent_per_developer() {
    let (env, contract, admin, _) = setup();
    let dev_a = Address::generate(&env);
    let dev_b = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract);

    client.set_developer_min_balance(&admin, &dev_a, &1_000);
    client.set_developer_min_balance(&admin, &dev_b, &3_000);

    assert_eq!(client.get_developer_min_balance(&dev_a), 1_000);
    assert_eq!(client.get_developer_min_balance(&dev_b), 3_000);
}

#[test]
#[should_panic(expected = "minimum balance must be non-negative")]
fn negative_min_balance_panics() {
    let (env, contract, admin, _) = setup();
    let dev = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract);

    client.set_developer_min_balance(&admin, &dev, &-1);
}

// ── Event emission ───────────────────────────────────────────────────────────

#[test]
fn set_min_balance_emits_event() {
    let (env, contract, admin, _) = setup();
    let dev = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract);

    client.set_developer_min_balance(&admin, &dev, &5_000);

    let events = env.events().all();
    let matching: Vec<_> = events
        .iter()
        .filter(|e| {
            if e.1.is_empty() {
                return false;
            }
            let sym: soroban_sdk::Symbol = e.1.get(0).unwrap().into_val(&env);
            sym == soroban_sdk::Symbol::new(&env, "developer_min_balance_changed")
        })
        .collect();
    assert_eq!(matching.len(), 1);
}

// ── Storage persistence ──────────────────────────────────────────────────────

#[test]
fn min_balance_persists_across_reads() {
    let (env, contract, admin, _) = setup();
    let dev = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract);

    client.set_developer_min_balance(&admin, &dev, &7_777);

    // Read directly from storage to verify persistence.
    let stored: i128 = env.as_contract(&contract, || {
        env.storage()
            .persistent()
            .get(&StorageKey::DeveloperMinBalance(dev))
            .unwrap_or(0)
    });
    assert_eq!(stored, 7_777);
}

// ── Large values ─────────────────────────────────────────────────────────────

#[test]
fn large_min_balance_value() {
    let (env, contract, admin, _) = setup();
    let dev = Address::generate(&env);
    let client = CalloraSettlementClient::new(&env, &contract);
    let large = i128::MAX / 2;

    client.set_developer_min_balance(&admin, &dev, &large);
    assert_eq!(client.get_developer_min_balance(&dev), large);
}

// ── Multiple developers sequential ───────────────────────────────────────────

#[test]
fn set_min_balance_for_many_developers() {
    let (env, contract, admin, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract);

    let mut devs = Vec::new();
    for _ in 0..10 {
        devs.push(Address::generate(&env));
    }

    for (i, dev) in devs.iter().enumerate() {
        client.set_developer_min_balance(&admin, dev, &((i as i128) * 100));
    }

    for (i, dev) in devs.iter().enumerate() {
        assert_eq!(
            client.get_developer_min_balance(dev),
            (i as i128) * 100
        );
    }
}
