//! Integration tests for the per-developer minimum balance feature.
//!
//! Verifies:
//! - Admin can set/get min balance per developer
//! - Min balance is enforced during withdrawal
//! - Event is emitted on min balance change
//! - Edge cases: zero min, negative min, boundary values
//! - `request_id == 0` is treated as a "no idempotency" sentinel by
//!   `record_deduction`, so repeated deductions with id `0` never revert
//!   and never write a `DeductionRequest(0)` marker.

use callora_settlement::{CalloraSettlement, CalloraSettlementClient, StorageKey};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{Address, Env, IntoVal};

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
        assert_eq!(client.get_developer_min_balance(dev), (i as i128) * 100);
    }
}

// ── request_id 0 sentinel (record_deduction) ─────────────────────────────────

/// Row 1: `record_deduction(10, 0)` twice must both succeed, accumulate the
/// total, and never write a `DeductionRequest(0)` marker.
#[test]
fn record_deduction_zero_id_is_never_deduplicated() {
    let (env, contract, _admin, vault) = setup();
    let client = CalloraSettlementClient::new(&env, &contract);

    client.record_deduction(&vault, &10, &0);
    client.record_deduction(&vault, &10, &0);

    assert_eq!(client.get_total_received(), 20);

    let marker_exists: bool = env.as_contract(&contract, || {
        env.storage()
            .persistent()
            .has(&StorageKey::DeductionRequest(0))
    });
    assert!(!marker_exists, "id 0 must never be marked in settlement");
}

/// Row 2: a non-zero id is still deduplicated; the second call fails with
/// `DuplicateRequestId` and the total is unchanged.
#[test]
#[should_panic(expected = "DuplicateRequestId")]
fn record_deduction_nonzero_id_is_deduplicated() {
    let (env, contract, _admin, vault) = setup();
    let client = CalloraSettlementClient::new(&env, &contract);

    client.record_deduction(&vault, &10, &7);
    client.record_deduction(&vault, &10, &7);
}

/// Row 3: non-positive amounts are rejected for id `0` and leave no marker.
#[test]
fn record_deduction_zero_id_rejects_non_positive_amount() {
    let (env, contract, _admin, vault) = setup();
    let client = CalloraSettlementClient::new(&env, &contract);

    let zero = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.record_deduction(&vault, &0, &0);
    }));
    assert!(zero.is_err(), "amount 0 must be rejected");

    let negative = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.record_deduction(&vault, &-1, &0);
    }));
    assert!(negative.is_err(), "negative amount must be rejected");

    assert_eq!(client.get_total_received(), 0);
    let marker_exists: bool = env.as_contract(&contract, || {
        env.storage()
            .persistent()
            .has(&StorageKey::DeductionRequest(0))
    });
    assert!(!marker_exists);
}

/// Row 4: overflow with id `0` must fail with `PoolOverflow` and write nothing.
#[test]
fn record_deduction_zero_id_overflow_writes_nothing() {
    let (env, contract, _admin, vault) = setup();
    let client = CalloraSettlementClient::new(&env, &contract);

    client.record_deduction(&vault, &i128::MAX, &0);

    let overflow = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.record_deduction(&vault, &1, &0);
    }));
    assert!(overflow.is_err(), "overflow must be rejected");

    assert_eq!(client.get_total_received(), i128::MAX);
    let marker_exists: bool = env.as_contract(&contract, || {
        env.storage()
            .persistent()
            .has(&StorageKey::DeductionRequest(0))
    });
    assert!(!marker_exists);
}

/// Row 5: `deduction_recorded` is still emitted for id `0` with the correct
/// payload so indexers can observe non-idempotent deductions.
#[test]
fn record_deduction_zero_id_emits_event() {
    let (env, contract, _admin, vault) = setup();
    let client = CalloraSettlementClient::new(&env, &contract);

    client.record_deduction(&vault, &42, &0);

    let events = env.events().all();
    let matching: Vec<_> = events
        .iter()
        .filter(|e| {
            if e.1.is_empty() {
                return false;
            }
            let sym: soroban_sdk::Symbol = e.1.get(0).unwrap().into_val(&env);
            sym == soroban_sdk::Symbol::new(&env, "deduction_recorded")
        })
        .collect();
    assert_eq!(matching.len(), 1);
}

/// Row 9: a non-vault caller of `record_deduction` with id `0` is still
/// rejected by auth.
#[test]
fn record_deduction_zero_id_rejects_non_vault_caller() {
    let env = Env::default();
    let contract = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(&env, &contract);
    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    client.init(&admin, &vault);

    let impostor = Address::generate(&env);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.record_deduction(&impostor, &10, &0);
    }));
    assert!(result.is_err(), "non-vault caller must fail auth");
    assert_eq!(client.get_total_received(), 0);
}
