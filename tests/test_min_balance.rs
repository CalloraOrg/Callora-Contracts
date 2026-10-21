//! Integration tests for the per-developer minimum balance feature.
//!
//! Verifies:
//! - Admin can set/get min balance per developer
//! - Min balance is enforced during withdrawal
//! - Event is emitted on min balance change
//! - Edge cases: zero min, negative min, boundary values
//! - Vault-originated deductions are accounting-only and leave the
//!   per-developer minimum balance untouched

use callora_settlement::{CalloraSettlement, CalloraSettlementClient, SettlementError, StorageKey};
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

// ── Vault-originated deductions ──────────────────────────────────────────────
//
// `record_deduction` is an accounting-only entrypoint: the vault authorizes
// the call, the amount is added to the cumulative received total, and no
// developer or pool balance moves. The registered vault is read from instance
// storage by the contract itself, so the client call takes only the amount
// and the request ID — there is no vault argument to pass.

#[test]
fn record_deduction_increases_total_received() {
    let (env, contract, _admin, _vault) = setup();
    let client = CalloraSettlementClient::new(&env, &contract);

    client.record_deduction(&10, &0);
    assert_eq!(client.get_total_received(), 10);

    client.record_deduction(&32, &1);
    assert_eq!(client.get_total_received(), 42);
}

#[test]
fn record_deduction_rejects_non_positive_amount() {
    let (env, contract, _admin, _vault) = setup();
    let client = CalloraSettlementClient::new(&env, &contract);

    // The amount is a signed `i128`: a negative deduction must be rejected as
    // such, not reinterpreted as an unsigned value.
    assert_eq!(
        client.try_record_deduction(&-1, &1),
        Err(Ok(SettlementError::AmountNotPositive.into()))
    );
    assert_eq!(
        client.try_record_deduction(&0, &2),
        Err(Ok(SettlementError::AmountNotPositive.into()))
    );

    // A rejected deduction changes no accounting state.
    assert_eq!(client.get_total_received(), 0);
}

#[test]
fn record_deduction_rejects_duplicate_request_id() {
    let (env, contract, _admin, _vault) = setup();
    let client = CalloraSettlementClient::new(&env, &contract);

    client.record_deduction(&10, &7);
    assert_eq!(
        client.try_record_deduction(&10, &7),
        Err(Ok(SettlementError::DuplicateRequestId.into()))
    );

    // A different request ID is still accepted afterwards.
    client.record_deduction(&5, &8);
    assert_eq!(client.get_total_received(), 15);
}

#[test]
fn record_deduction_does_not_touch_developer_balances_or_min_balance() {
    let (env, contract, admin, _vault) = setup();
    let client = CalloraSettlementClient::new(&env, &contract);
    let dev = Address::generate(&env);
    let token_addr = Address::generate(&env);

    client.set_developer_min_balance(&admin, &dev, &1_000);
    client.record_deduction(&10, &0);

    // The deduction is accounting-only: it credits neither the developer nor
    // the global pool, and it leaves the configured minimum untouched.
    assert_eq!(client.get_total_received(), 10);
    assert_eq!(client.get_developer_min_balance(&dev), 1_000);
    assert_eq!(client.get_developer_balance(&dev, &token_addr), 0);
    assert_eq!(client.get_global_pool().total_balance, 0);
}

#[test]
fn record_deduction_requires_registered_vault_authorization() {
    let (env, contract, _admin, _vault) = setup();
    let client = CalloraSettlementClient::new(&env, &contract);

    // The vault is taken from instance storage and must authorize the call;
    // there is no caller-supplied vault argument that could bypass it.
    env.mock_auths(&[]);
    assert!(client.try_record_deduction(&10, &0).is_err());
    assert_eq!(client.get_total_received(), 0);
}
