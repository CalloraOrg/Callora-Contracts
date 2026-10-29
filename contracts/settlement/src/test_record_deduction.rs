extern crate std;

use crate::{
    CalloraSettlement, CalloraSettlementClient, DeductionRecordedEvent, SettlementError,
    StorageKey, INSTANCE_BUMP_AMOUNT, MAX_BATCH_SIZE, PERSISTENT_BUMP_AMOUNT,
};
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{Address, Env, FromVal, IntoVal, Symbol};

fn setup() -> (Env, Address, Address) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    let contract_id = env.register(CalloraSettlement, ());
    CalloraSettlementClient::new(&env, &contract_id).init(&admin, &vault);
    (env, contract_id, vault)
}

fn has_marker(env: &Env, contract_id: &Address, request_id: u64) -> bool {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .has(&StorageKey::DeductionRequest(request_id))
    })
}

fn has_marker_batch(env: &Env, contract_id: &Address, request_id: u64) -> bool {
    env.as_contract(contract_id, || {
        env.storage()
            .persistent()
            .has(&StorageKey::DeductionRequest(request_id))
    })
}

#[test]
fn record_deduction_rejects_non_positive_without_consuming_request_id() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    client.record_deduction(&50, &1);

    for amount in [0, -1, i128::MIN] {
        assert_eq!(
            client.try_record_deduction(&amount, &2),
            Err(Ok(SettlementError::AmountNotPositive.into()))
        );
        assert!(env.events().all().is_empty());
        assert_eq!(client.get_total_received(), 50);
        assert!(!has_marker(&env, &contract_id, 2));
    }
    client.record_deduction(&25, &2);
    assert_eq!(client.get_total_received(), 75);
}

#[test]
fn record_deduction_rejects_replay_without_changing_total_or_marker() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    client.record_deduction(&100, &42);
    // A changed amount must not bypass replay detection either.
    for amount in [100, 200] {
        assert_eq!(
            client.try_record_deduction(&amount, &42),
            Err(Ok(SettlementError::DuplicateRequestId.into()))
        );
        assert!(env.events().all().is_empty());
        assert_eq!(client.get_total_received(), 100);
        assert!(has_marker(&env, &contract_id, 42));
    }
    client.record_deduction(&200, &43);
    assert_eq!(client.get_total_received(), 300);
}

#[test]
fn record_deduction_emits_exact_payload_and_only_updates_accounting() {
    let (env, contract_id, vault) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    let pool_before = client.get_global_pool();

    // All u64 IDs are opaque and valid, including zero and the maximum.
    for request_id in [0, u64::MAX] {
        client.record_deduction(&75, &request_id);
        assert_eq!(env.events().all().len(), 1);
        let (emitter, topics, data) = env.events().all().last().unwrap();
        assert_eq!(emitter, contract_id);
        assert_eq!(
            topics,
            soroban_sdk::vec![&env, Symbol::new(&env, "deduction_recorded").into_val(&env)]
        );
        assert_eq!(
            DeductionRecordedEvent::from_val(&env, &data),
            DeductionRecordedEvent {
                amount: 75,
                request_id,
            }
        );
        assert_eq!(
            env.auths(),
            std::vec![(
                vault.clone(),
                soroban_sdk::testutils::AuthorizedInvocation {
                    function: soroban_sdk::testutils::AuthorizedFunction::Contract((
                        contract_id.clone(),
                        Symbol::new(&env, "record_deduction"),
                        (75i128, request_id).into_val(&env),
                    )),
                    sub_invocations: std::vec![],
                },
            ),]
        );
    }
    assert_eq!(client.get_total_received(), 150);
    assert_eq!(client.get_global_pool(), pool_before);
}

#[test]
fn record_deduction_bumps_persistent_marker_and_instance_ttl() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    client.record_deduction(&1, &7);
    env.as_contract(&contract_id, || {
        assert_eq!(
            env.storage()
                .persistent()
                .get_ttl(&StorageKey::DeductionRequest(7)),
            PERSISTENT_BUMP_AMOUNT
        );
        assert_eq!(env.storage().instance().get_ttl(), INSTANCE_BUMP_AMOUNT);
    });

    env.ledger()
        .set_sequence_number(env.ledger().sequence() + 100);
    assert_eq!(
        client.try_record_deduction(&1, &7),
        Err(Ok(SettlementError::DuplicateRequestId.into()))
    );
    env.as_contract(&contract_id, || {
        assert_eq!(
            env.storage()
                .persistent()
                .get_ttl(&StorageKey::DeductionRequest(7)),
            PERSISTENT_BUMP_AMOUNT - 100
        );
    });
    assert_eq!(client.get_total_received(), 1);
    client.record_deduction(&1, &8);
    env.as_contract(&contract_id, || {
        assert_eq!(
            env.storage()
                .persistent()
                .get_ttl(&StorageKey::DeductionRequest(8)),
            PERSISTENT_BUMP_AMOUNT
        );
    });
}

#[test]
fn record_deduction_overflow_does_not_consume_request_id() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&StorageKey::TotalReceived, &(i128::MAX - 1));
    });
    assert_eq!(
        client.try_record_deduction(&2, &9),
        Err(Ok(SettlementError::PoolOverflow.into()))
    );
    assert!(env.events().all().is_empty());
    assert_eq!(client.get_total_received(), i128::MAX - 1);
    assert!(!has_marker(&env, &contract_id, 9));

    client.record_deduction(&1, &9);
    assert_eq!(client.get_total_received(), i128::MAX);
    assert!(has_marker(&env, &contract_id, 9));
}

#[test]
fn record_deduction_requires_vault_authorization() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    env.mock_auths(&[]);
    assert!(client.try_record_deduction(&100, &10).is_err());
    assert!(env.events().all().is_empty());
    assert_eq!(client.get_total_received(), 0);
    assert!(!has_marker(&env, &contract_id, 10));

    env.mock_all_auths();
    client.record_deduction(&100, &10);
    assert_eq!(client.get_total_received(), 100);
}

#[test]
fn record_deductions_accepts_valid_items_and_updates_accounting() {
    let (env, contract_id, vault) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    let pool_before = client.get_global_pool();

    let items = soroban_sdk::vec![&env, (10i128, 1u64), (20i128, 2u64), (30i128, 3u64)];
    client.record_deductions(&items);

    assert_eq!(client.get_total_received(), 60);
    assert_eq!(client.get_global_pool(), pool_before);
    for id in [1u64, 2, 3] {
        assert!(has_marker_batch(&env, &contract_id, id));
    }
    assert_eq!(env.events().all().len(), 3);
    assert_eq!(
        env.auths(),
        std::vec![(
            vault.clone(),
            soroban_sdk::testutils::AuthorizedInvocation {
                function: soroban_sdk::testutils::AuthorizedFunction::Contract((
                    contract_id.clone(),
                    Symbol::new(&env, "record_deductions"),
                    (items.clone(),).into_val(&env),
                )),
                sub_invocations: std::vec![],
            },
        ),]
    );
}

#[test]
fn record_deductions_rejects_empty_and_oversized_batches() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);

    let empty: soroban_sdk::Vec<(i128, u64)> = soroban_sdk::vec![&env];
    assert_eq!(
        client.try_record_deductions(&empty),
        Err(Ok(SettlementError::EmptyBatch.into()))
    );
    assert!(env.events().all().is_empty());
    assert_eq!(client.get_total_received(), 0);

    let mut too_many: soroban_sdk::Vec<(i128, u64)> = soroban_sdk::vec![&env];
    for i in 0..(MAX_BATCH_SIZE + 1) {
        too_many.push_back((1i128, i as u64));
    }
    assert_eq!(
        client.try_record_deductions(&too_many),
        Err(Ok(SettlementError::BatchTooLarge.into()))
    );
    assert!(env.events().all().is_empty());
    assert_eq!(client.get_total_received(), 0);
}

#[test]
fn record_deductions_rejects_non_positive_amount_atomically() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);

    for bad in [0i128, -1, i128::MIN] {
        let items = soroban_sdk::vec![&env, (10i128, 1u64), (bad, 2u64), (30i128, 3u64)];
        assert_eq!(
            client.try_record_deductions(&items),
            Err(Ok(SettlementError::AmountNotPositive.into()))
        );
        assert!(env.events().all().is_empty());
        assert_eq!(client.get_total_received(), 0);
        for id in [1u64, 2, 3] {
            assert!(!has_marker_batch(&env, &contract_id, id));
        }
    }
}

#[test]
fn record_deductions_rejects_duplicate_within_batch_atomically() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);

    let items = soroban_sdk::vec![&env, (10i128, 1u64), (20i128, 2u64), (30i128, 1u64)];
    assert_eq!(
        client.try_record_deductions(&items),
        Err(Ok(SettlementError::DuplicateRequestId.into()))
    );
    assert!(env.events().all().is_empty());
    assert_eq!(client.get_total_received(), 0);
    for id in [1u64, 2] {
        assert!(!has_marker_batch(&env, &contract_id, id));
    }
}

#[test]
fn record_deductions_rejects_already_recorded_id_atomically() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    client.record_deduction(&5, &2);
    let total_before = client.get_total_received();
    let events_before = env.events().all().len();

    let items = soroban_sdk::vec![&env, (10i128, 1u64), (20i128, 2u64)];
    assert_eq!(
        client.try_record_deductions(&items),
        Err(Ok(SettlementError::DuplicateRequestId.into()))
    );
    assert_eq!(env.events().all().len(), events_before);
    assert_eq!(client.get_total_received(), total_before);
    assert!(!has_marker_batch(&env, &contract_id, 1));
}

#[test]
fn record_deductions_rejects_overflow_atomically() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    env.as_contract(&contract_id, || {
        env.storage()
            .instance()
            .set(&StorageKey::TotalReceived, &(i128::MAX - 5));
    });

    let items = soroban_sdk::vec![&env, (3i128, 1u64), (3i128, 2u64)];
    assert_eq!(
        client.try_record_deductions(&items),
        Err(Ok(SettlementError::PoolOverflow.into()))
    );
    assert!(env.events().all().is_empty());
    assert_eq!(client.get_total_received(), i128::MAX - 5);
    for id in [1u64, 2] {
        assert!(!has_marker_batch(&env, &contract_id, id));
    }
}

#[test]
fn record_deductions_requires_vault_authorization() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    env.mock_auths(&[]);
    let items = soroban_sdk::vec![&env, (100i128, 10u64)];
    assert!(client.try_record_deductions(&items).is_err());
    assert!(env.events().all().is_empty());
    assert_eq!(client.get_total_received(), 0);
    assert!(!has_marker_batch(&env, &contract_id, 10));
}

#[test]
fn record_deductions_event_parity_with_single_calls() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);

    let items = soroban_sdk::vec![&env, (10i128, 1u64), (20i128, 2u64), (30i128, 3u64)];
    client.record_deductions(&items);
    let batch_events = env.events().all();
    assert_eq!(batch_events.len(), 3);
    for (idx, (amount, request_id)) in [(10i128, 1u64), (20, 2), (30, 3)].iter().enumerate() {
        let (emitter, topics, data) = batch_events.get(idx as u32).unwrap();
        assert_eq!(emitter, contract_id);
        assert_eq!(
            topics,
            soroban_sdk::vec![&env, Symbol::new(&env, "deduction_recorded").into_val(&env)]
        );
        assert_eq!(
            DeductionRecordedEvent::from_val(&env, &data),
            DeductionRecordedEvent {
                amount: *amount,
                request_id: *request_id,
            }
        );
    }
}

#[test]
fn record_deductions_bumps_persistent_markers_and_instance_ttl() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    let items = soroban_sdk::vec![&env, (1i128, 7u64), (1i128, 8u64)];
    client.record_deductions(&items);
    env.as_contract(&contract_id, || {
        assert_eq!(
            env.storage()
                .persistent()
                .get_ttl(&StorageKey::DeductionRequest(7)),
            PERSISTENT_BUMP_AMOUNT
        );
        assert_eq!(
            env.storage()
                .persistent()
                .get_ttl(&StorageKey::DeductionRequest(8)),
            PERSISTENT_BUMP_AMOUNT
        );
        assert_eq!(env.storage().instance().get_ttl(), INSTANCE_BUMP_AMOUNT);
    });
    assert_eq!(client.get_total_received(), 2);
}

#[test]
fn record_deductions_accepts_max_batch_size() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    let mut items: soroban_sdk::Vec<(i128, u64)> = soroban_sdk::vec![&env];
    for i in 0..MAX_BATCH_SIZE {
        items.push_back((1i128, i as u64));
    }
    client.record_deductions(&items);
    assert_eq!(client.get_total_received(), MAX_BATCH_SIZE as i128);
    assert_eq!(env.events().all().len(), MAX_BATCH_SIZE);
}

#[test]
fn record_deductions_id_zero_follows_single_call_rule() {
    let (env, contract_id, _) = setup();
    let client = CalloraSettlementClient::new(&env, &contract_id);
    let items = soroban_sdk::vec![&env, (5i128, 0u64)];
    client.record_deductions(&items);
    assert_eq!(client.get_total_received(), 5);
    assert!(has_marker_batch(&env, &contract_id, 0));

    assert_eq!(
        client.try_record_deductions(&items),
        Err(Ok(SettlementError::DuplicateRequestId.into()))
    );
    assert_eq!(client.get_total_received(), 5);
}
