extern crate std;

use callora_checkpoint::{
    CalloraCheckpoint, CalloraCheckpointClient, CheckpointRecord, MAX_BATCH_SIZE, MAX_PAGE_SIZE,
    MAX_PRUNE_BATCH,
};
use proptest::prelude::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{xdr::ToXdr, Address, Bytes, BytesN, Env, Symbol, Vec as SorobanVec};
use std::panic::{catch_unwind, AssertUnwindSafe};

// ---------------------------------------------------------------------------
// Action enum – represents every state-mutating operation on the contract
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
enum CheckpointAction {
    CreateSingle { balance: i128 },
    CreateBatch { count: u32, balance: i128 },
    QueryRange { start_id: u64, limit: u32 },
    QueryLatest,
}

fn checkpoint_action_strategy() -> impl Strategy<Value = CheckpointAction> {
    prop_oneof![
        5 => (0_i128..=1_000_000_i128).prop_map(|balance| CheckpointAction::CreateSingle { balance }),
        3 => (1_u32..=MAX_BATCH_SIZE, 0_i128..=1_000_000_i128)
            .prop_map(|(count, balance)| CheckpointAction::CreateBatch { count, balance }),
        2 => (0_u64..=200_u64, 0_u32..=150_u32)
            .prop_map(|(start_id, limit)| CheckpointAction::QueryRange { start_id, limit }),
        1 => Just(CheckpointAction::QueryLatest),
    ]
}

#[derive(Clone, Debug)]
enum ChainAction {
    Create { balance: i128 },
    CreateBatch { count: u32, balance: i128 },
    Prune { id: u64, max_remove: u32 },
}

fn chain_action_strategy() -> impl Strategy<Value = ChainAction> {
    prop_oneof![
        5 => (0_i128..=10_000_i128).prop_map(|balance| ChainAction::Create { balance }),
        3 => (1_u32..=8, 0_i128..=10_000_i128)
            .prop_map(|(count, balance)| ChainAction::CreateBatch { count, balance }),
        2 => (0_u64..=80_u64, 1_u32..=MAX_PRUNE_BATCH)
            .prop_map(|(id, max_remove)| ChainAction::Prune { id, max_remove }),
    ]
}

/// Test-side fold of one record: `sha256(prev || xdr(record))`.
fn hash_step(env: &Env, prev: &BytesN<32>, record: &CheckpointRecord) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.append(&Bytes::from(prev));
    preimage.append(&record.clone().to_xdr(env));
    env.crypto().sha256(&preimage).into()
}

// ---------------------------------------------------------------------------
// Property: CheckpointCount == NextCheckpointId after every mutation
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(8))]

    #[test]
    fn checkpoint_count_equals_next_id(
        actions in prop::collection::vec(checkpoint_action_strategy(), 1..=64)
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(CalloraCheckpoint, ());
        let client = CalloraCheckpointClient::new(&env, &contract_id);
        let subject = Address::generate(&env);
        let token = Address::generate(&env);
        let meta = Symbol::new(&env, "proptest");

        client.init(&admin);

        // Both counters start at 0 and equal.
        prop_assert_eq!(client.get_checkpoint_count(), client.get_latest_checkpoint_id());

        for action in actions {
            match action {
                CheckpointAction::CreateSingle { balance } => {
                    if balance >= 0 {
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            client.create_checkpoint(&admin, &subject, &token, &balance, &meta);
                        }));
                    } else {
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            client.create_checkpoint(&admin, &subject, &token, &balance, &meta);
                        }));
                    }
                }
                CheckpointAction::CreateBatch { count, balance } => {
                    if count == 0 {
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            let items: SorobanVec<(Address, Address, i128, Symbol)> = SorobanVec::new(&env);
                            client.batch_create_checkpoints(&admin, &items);
                        }));
                    } else if balance < 0 {
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            let mut items = SorobanVec::new(&env);
                            for _ in 0..count {
                                items.push_back((subject.clone(), token.clone(), balance, meta.clone()));
                            }
                            client.batch_create_checkpoints(&admin, &items);
                        }));
                    } else {
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            let mut items = SorobanVec::new(&env);
                            for _ in 0..count {
                                items.push_back((subject.clone(), token.clone(), balance, meta.clone()));
                            }
                            client.batch_create_checkpoints(&admin, &items);
                        }));
                    }
                }
                CheckpointAction::QueryRange { start_id, limit } => {
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        let _ = client.get_checkpoints_range(&start_id, &limit);
                    }));
                }
                CheckpointAction::QueryLatest => {
                    let _ = client.get_latest_checkpoint();
                }
            }

            // INVARIANT: count and next-id counters must always be equal
            let count = client.get_checkpoint_count();
            let latest_id = client.get_latest_checkpoint_id();
            prop_assert_eq!(
                count, latest_id,
                "invariant violated: checkpoint_count ({}) != latest_checkpoint_id ({})",
                count, latest_id
            );
        }
    }

    // -----------------------------------------------------------------------
    // Property: checkpoint IDs are strictly sequential starting at 1
    // -----------------------------------------------------------------------

    #[test]
    fn sequential_id_assignment(
        batch_sizes in prop::collection::vec(1_u32..=5, 1..=20)
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(CalloraCheckpoint, ());
        let client = CalloraCheckpointClient::new(&env, &contract_id);
        let subject = Address::generate(&env);
        let token = Address::generate(&env);
        let meta = Symbol::new(&env, "seq");

        client.init(&admin);

        let mut expected_id: u64 = 0;

        for batch_size in batch_sizes {
            let size = batch_size.min(MAX_BATCH_SIZE);
            let mut items = SorobanVec::new(&env);
            for _ in 0..size {
                items.push_back((subject.clone(), token.clone(), 100i128, meta.clone()));
            }

            let result = catch_unwind(AssertUnwindSafe(|| {
                client.batch_create_checkpoints(&admin, &items)
            }));

            if let Ok(ids) = result {
                for i in 0..ids.len() {
                    expected_id += 1;
                    let assigned_id = ids.get(i).unwrap();
                    prop_assert_eq!(
                        assigned_id, expected_id,
                        "IDs not sequential: got {}, expected {}",
                        assigned_id, expected_id
                    );
                }
            }

            // After each batch, count must equal expected_id
            let count = client.get_checkpoint_count();
            prop_assert_eq!(
                count, expected_id,
                "checkpoint_count ({}) != expected ({})",
                count, expected_id
            );
        }
    }

    // -----------------------------------------------------------------------
    // Property: checkpoint records are immutable once written
    // -----------------------------------------------------------------------

    #[test]
    fn checkpoint_records_are_immutable(
        balances in prop::collection::vec(0_i128..=500_000_i128, 1..=30)
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(CalloraCheckpoint, ());
        let client = CalloraCheckpointClient::new(&env, &contract_id);
        let subject = Address::generate(&env);
        let token = Address::generate(&env);
        let meta = Symbol::new(&env, "immut");

        client.init(&admin);

        // Phase 1: create checkpoints and record their data
        let mut records: std::vec::Vec<(u64, i128)> = std::vec::Vec::new();

        for &balance in &balances {
            let id = client.create_checkpoint(&admin, &subject, &token, &balance, &meta);
            records.push((id, balance));
        }

        // Phase 2: create more checkpoints (to exercise the counter further)
        let extra_balances: std::vec::Vec<i128> = balances.iter().map(|b| b + 1).collect();
        for &balance in &extra_balances {
            let _ = client.create_checkpoint(&admin, &subject, &token, &balance, &meta);
        }

        // Phase 3: verify original records are unchanged
        for (id, expected_balance) in &records {
            let record = client.get_checkpoint(id);
            prop_assert_eq!(
                record.id, *id,
                "record id mismatch: stored {} != expected {}",
                record.id, id
            );
            prop_assert_eq!(
                record.balance, *expected_balance,
                "record balance mutated: stored {} != expected {}",
                record.balance, expected_balance
            );
            prop_assert_eq!(
                record.subject, subject.clone(),
                "record subject mutated"
            );
            prop_assert_eq!(
                record.token, token.clone(),
                "record token mutated"
            );
            prop_assert_eq!(
                record.metadata, meta.clone(),
                "record metadata mutated"
            );
        }

        // Phase 4: total count should be len(balances) + len(extra_balances)
        let expected_total = (balances.len() + extra_balances.len()) as u64;
        let count = client.get_checkpoint_count();
        prop_assert_eq!(
            count, expected_total,
            "total count mismatch: {} != {}",
            count, expected_total
        );
    }

    // -----------------------------------------------------------------------
    // Property: range queries return correct count and bounds
    // -----------------------------------------------------------------------

    #[test]
    fn range_query_invariants(
        total_checkpoints in 0_u32..=100_u32,
        start_id in 0_u64..=120_u64,
        limit in 0_u32..=150_u32,
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(CalloraCheckpoint, ());
        let client = CalloraCheckpointClient::new(&env, &contract_id);
        let subject = Address::generate(&env);
        let token = Address::generate(&env);
        let meta = Symbol::new(&env, "range");

        client.init(&admin);

        // Create checkpoints
        for i in 1..=total_checkpoints {
            let _ = client.create_checkpoint(&admin, &subject, &token, &(i as i128), &meta);
        }

        let total = client.get_checkpoint_count();

        // limit=0 should always fail
        if limit == 0 {
            let result = client.try_get_checkpoints_range(&start_id, &limit);
            prop_assert!(result.is_err(), "limit=0 should return error");
            return Ok(());
        }

        let result = client.get_checkpoints_range(&start_id, &limit);

        let effective_limit = limit.min(MAX_PAGE_SIZE);

        // Result length should never exceed effective_limit
        prop_assert!(
            result.len() <= effective_limit,
            "result len {} > effective_limit {}",
            result.len(),
            effective_limit
        );

        // If start_id > total, result must be empty
        if start_id > total {
            prop_assert!(
                result.is_empty(),
                "start_id ({}) > total ({}) but result is non-empty (len={})",
                start_id, total, result.len()
            );
        }

        // All returned records must have IDs >= start_id
        for i in 0..result.len() {
            let record = result.get(i).unwrap();
            prop_assert!(
                record.id >= start_id,
                "record id {} < start_id {}",
                record.id, start_id
            );
        }

        // IDs in result must be strictly increasing
        for i in 1..result.len() {
            let prev = result.get(i - 1).unwrap();
            let curr = result.get(i).unwrap();
            prop_assert!(
                curr.id > prev.id,
                "IDs not increasing: prev {} >= curr {}",
                prev.id, curr.id
            );
        }

        // Each returned record can be fetched individually
        for i in 0..result.len() {
            let record = result.get(i).unwrap();
            let fetched = client.get_checkpoint(&record.id);
            prop_assert_eq!(record, fetched);
        }
    }

    // -----------------------------------------------------------------------
    // Property: batch size limits are respected
    // -----------------------------------------------------------------------

    #[test]
    fn batch_size_limits(
        batch_sizes in prop::collection::vec(0_u32..=(MAX_BATCH_SIZE + 5), 1..=10)
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(CalloraCheckpoint, ());
        let client = CalloraCheckpointClient::new(&env, &contract_id);
        let subject = Address::generate(&env);
        let token = Address::generate(&env);
        let meta = Symbol::new(&env, "batchlim");

        client.init(&admin);

        let mut expected_count: u64 = 0;

        for &size in &batch_sizes {
            let count_before = client.get_checkpoint_count();

            if size == 0 {
                // Empty batch should fail
                let items: SorobanVec<(Address, Address, i128, Symbol)> = SorobanVec::new(&env);
                let result = client.try_batch_create_checkpoints(&admin, &items);
                prop_assert!(result.is_err(), "empty batch should fail");

                // Count unchanged
                prop_assert_eq!(client.get_checkpoint_count(), count_before);
            } else if size > MAX_BATCH_SIZE {
                // Oversized batch should fail
                let mut items = SorobanVec::new(&env);
                for _ in 0..size {
                    items.push_back((subject.clone(), token.clone(), 100i128, meta.clone()));
                }
                let result = client.try_batch_create_checkpoints(&admin, &items);
                prop_assert!(result.is_err(), "oversized batch ({}) should fail", size);

                // Count unchanged
                prop_assert_eq!(client.get_checkpoint_count(), count_before);
            } else {
                // Valid batch: count must increase by exactly `size`
                let mut items = SorobanVec::new(&env);
                for _ in 0..size {
                    items.push_back((subject.clone(), token.clone(), 100i128, meta.clone()));
                }
                let result = client.try_batch_create_checkpoints(&admin, &items);
                prop_assert!(result.is_ok(), "valid batch ({}) should succeed", size);

                expected_count += size as u64;
                let count_after = client.get_checkpoint_count();
                prop_assert_eq!(
                    count_after, expected_count,
                    "count mismatch after batch of {}: got {}, expected {}",
                    size, count_after, expected_count
                );
            }
        }
    }

    // -----------------------------------------------------------------------
    // Property: negative balances are always rejected
    // -----------------------------------------------------------------------

    #[test]
    fn negative_balance_always_rejected(
        balances in prop::collection::vec((-1_000_000_i128)..=(-1_i128), 1..=20)
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(CalloraCheckpoint, ());
        let client = CalloraCheckpointClient::new(&env, &contract_id);
        let subject = Address::generate(&env);
        let token = Address::generate(&env);
        let meta = Symbol::new(&env, "neg");

        client.init(&admin);

        for &balance in &balances {
            let count_before = client.get_checkpoint_count();

            // Single create should fail
            let result = client.try_create_checkpoint(&admin, &subject, &token, &balance, &meta);
            prop_assert!(result.is_err(), "negative balance {} should be rejected", balance);

            // Count must remain unchanged
            prop_assert_eq!(
                client.get_checkpoint_count(),
                count_before,
                "checkpoint count changed after rejected negative balance"
            );

            // Batch create with one negative item should also fail
            let mut items = SorobanVec::new(&env);
            items.push_back((subject.clone(), token.clone(), balance, meta.clone()));
            let result = client.try_batch_create_checkpoints(&admin, &items);
            prop_assert!(result.is_err(), "batch with negative balance {} should fail", balance);

            prop_assert_eq!(
                client.get_checkpoint_count(),
                count_before,
                "checkpoint count changed after rejected negative batch"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Property: latest checkpoint always has the highest ID
    // -----------------------------------------------------------------------

    #[test]
    fn latest_checkpoint_has_highest_id(
        create_counts in prop::collection::vec(1_u32..=5, 1..=15)
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(CalloraCheckpoint, ());
        let client = CalloraCheckpointClient::new(&env, &contract_id);
        let subject = Address::generate(&env);
        let token = Address::generate(&env);
        let meta = Symbol::new(&env, "latest");

        client.init(&admin);

        // Before any creation, latest is None
        prop_assert_eq!(client.get_latest_checkpoint(), None);

        let mut expected_id: u64 = 0;

        for &count in &create_counts {
            let c = count.min(MAX_BATCH_SIZE);
            let mut items = SorobanVec::new(&env);
            for _ in 0..c {
                items.push_back((subject.clone(), token.clone(), 50i128, meta.clone()));
            }

            let result = catch_unwind(AssertUnwindSafe(|| {
                client.batch_create_checkpoints(&admin, &items)
            }));

            if let Ok(ids) = result {
                expected_id = ids.last().unwrap();
            }

            let latest = client.get_latest_checkpoint();
            prop_assert!(latest.is_some(), "latest should be Some after creation");

            let latest_record = latest.unwrap();
            prop_assert_eq!(
                latest_record.id, expected_id,
                "latest record id ({}) != expected ({})",
                latest_record.id, expected_id
            );

            // latest ID should equal count
            prop_assert_eq!(
                client.get_latest_checkpoint_id(),
                client.get_checkpoint_count(),
                "latest_id != count"
            );
        }
    }

    // -----------------------------------------------------------------------
    // Property: the retained digest chain is reproducible off-chain after
    // any mix of creation, batch creation, and (possibly partial) pruning.
    // -----------------------------------------------------------------------

    #[test]
    fn digest_chain_matches_offchain_recomputation(
        actions in prop::collection::vec(chain_action_strategy(), 1..=48)
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let contract_id = env.register(CalloraCheckpoint, ());
        let client = CalloraCheckpointClient::new(&env, &contract_id);
        let subject = Address::generate(&env);
        let token = Address::generate(&env);
        let meta = Symbol::new(&env, "chain");
        client.init(&admin);

        let zero = BytesN::from_array(&env, &[0u8; 32]);
        let mut head = zero.clone();
        let mut boundary = zero.clone();
        let mut retained: std::vec::Vec<(u64, CheckpointRecord)> = std::vec::Vec::new();

        for action in actions {
            match action {
                ChainAction::Create { balance } => {
                    let id = client.create_checkpoint(&admin, &subject, &token, &balance, &meta);
                    let record = client.get_checkpoint(&id);
                    head = hash_step(&env, &head, &record);
                    retained.push((id, record));
                }
                ChainAction::CreateBatch { count, balance } => {
                    let mut items = SorobanVec::new(&env);
                    for _ in 0..count {
                        items.push_back((subject.clone(), token.clone(), balance, meta.clone()));
                    }
                    let ids = client.batch_create_checkpoints(&admin, &items);
                    for id in ids.iter() {
                        let record = client.get_checkpoint(&id);
                        head = hash_step(&env, &head, &record);
                        retained.push((id, record));
                    }
                }
                ChainAction::Prune { id, max_remove } => {
                    let before = client.get_pruned_before();
                    if client.try_prune_before(&admin, &id, &max_remove).is_ok() {
                        let after = client.get_pruned_before();
                        prop_assert!(after >= before, "prune boundary must be monotonic");
                        for pruned_id in before..after {
                            if let Some(pos) = retained.iter().position(|(id, _)| *id == pruned_id)
                            {
                                let (_, record) = retained.remove(pos);
                                boundary = hash_step(&env, &boundary, &record);
                            }
                        }
                    }
                }
            }

            prop_assert_eq!(client.get_digest_head(), head.clone(), "head drifted from model");
            prop_assert_eq!(
                client.get_digest_at_boundary(),
                boundary.clone(),
                "boundary drifted from model"
            );

            let mut recomputed = client.get_digest_at_boundary();
            for (_, record) in retained.iter() {
                recomputed = hash_step(&env, &recomputed, record);
            }
            prop_assert_eq!(
                recomputed,
                head.clone(),
                "boundary plus retained records must reproduce the head"
            );
            prop_assert_eq!(
                client.get_checkpoint_count(),
                client.get_latest_checkpoint_id(),
                "pruning must not change counters"
            );
        }
    }
}
