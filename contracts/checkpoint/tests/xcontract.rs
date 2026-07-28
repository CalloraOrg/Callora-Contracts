#![cfg(test)]

extern crate std;

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{contract, contractimpl, Address, Env, Symbol, Vec};

use callora_checkpoint::errors::CheckpointError;
use callora_checkpoint::{CalloraCheckpoint, CalloraCheckpointClient, CheckpointRecord};

// ===========================================================================
// Mock contract — CheckpointCaller
// ===========================================================================

#[contract]
pub struct CheckpointCaller;

#[contractimpl]
impl CheckpointCaller {
    pub fn call_create_checkpoint(
        env: Env,
        checkpoint: Address,
        caller: Address,
        subject: Address,
        token: Address,
        balance: i128,
        metadata: Symbol,
    ) -> Result<u64, CheckpointError> {
        let client = CalloraCheckpointClient::new(&env, &checkpoint);
        match client.try_create_checkpoint(&caller, &subject, &token, &balance, &metadata) {
            Ok(Ok(id)) => Ok(id),
            Err(Ok(e)) => Err(e),
            _ => panic!("unexpected result from checkpoint"),
        }
    }

    pub fn call_create_checkpoint_and_panic(
        env: Env,
        checkpoint: Address,
        caller: Address,
        subject: Address,
        token: Address,
        balance: i128,
        metadata: Symbol,
    ) -> u64 {
        let client = CalloraCheckpointClient::new(&env, &checkpoint);
        let _id = client.create_checkpoint(&caller, &subject, &token, &balance, &metadata);
        panic!("caller panicking after successful cross-contract call");
    }

    pub fn call_batch_create_checkpoints(
        env: Env,
        checkpoint: Address,
        caller: Address,
        items: Vec<(Address, Address, i128, Symbol)>,
    ) -> Result<Vec<u64>, CheckpointError> {
        let client = CalloraCheckpointClient::new(&env, &checkpoint);
        match client.try_batch_create_checkpoints(&caller, &items) {
            Ok(Ok(ids)) => Ok(ids),
            Err(Ok(e)) => Err(e),
            _ => panic!("unexpected result from checkpoint"),
        }
    }

    pub fn call_get_checkpoint(
        env: Env,
        checkpoint: Address,
        id: u64,
    ) -> Result<CheckpointRecord, CheckpointError> {
        let client = CalloraCheckpointClient::new(&env, &checkpoint);
        match client.try_get_checkpoint(&id) {
            Ok(Ok(record)) => Ok(record),
            Err(Ok(e)) => Err(e),
            _ => panic!("unexpected result from checkpoint"),
        }
    }

    pub fn call_get_checkpoints_range(
        env: Env,
        checkpoint: Address,
        start_id: u64,
        limit: u32,
    ) -> Result<Vec<CheckpointRecord>, CheckpointError> {
        let client = CalloraCheckpointClient::new(&env, &checkpoint);
        match client.try_get_checkpoints_range(&start_id, &limit) {
            Ok(Ok(records)) => Ok(records),
            Err(Ok(e)) => Err(e),
            _ => panic!("unexpected result from checkpoint"),
        }
    }

    pub fn call_accept_admin(env: Env, checkpoint: Address, caller: Address) {
        let client = CalloraCheckpointClient::new(&env, &checkpoint);
        client.accept_admin(&caller);
    }
}

// ===========================================================================
// Helpers
// ===========================================================================

struct TestContext {
    env: Env,
    admin: Address,
    checkpoint_id: Address,
    checkpoint_client: CalloraCheckpointClient<'static>,
    caller_client: CheckpointCallerClient<'static>,
}

fn setup() -> TestContext {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let checkpoint_id = env.register(CalloraCheckpoint, ());
    let checkpoint_client = CalloraCheckpointClient::new(&env, &checkpoint_id);
    checkpoint_client.init(&admin);

    let caller_id = env.register(CheckpointCaller, ());
    let caller_client = CheckpointCallerClient::new(&env, &caller_id);

    TestContext {
        env,
        admin,
        checkpoint_id,
        checkpoint_client,
        caller_client,
    }
}

// ===========================================================================
// Success-path tests
// ===========================================================================

#[test]
fn test_xcontract_create_checkpoint_success() {
    let ctx = setup();
    let subject = Address::generate(&ctx.env);
    let token = Address::generate(&ctx.env);
    let metadata = Symbol::new(&ctx.env, "xcontract");

    let id = ctx.caller_client.call_create_checkpoint(
        &ctx.checkpoint_id,
        &ctx.admin,
        &subject,
        &token,
        &1000i128,
        &metadata,
    );
    assert_eq!(id, 1);

    let record = ctx.checkpoint_client.get_checkpoint(&1);
    assert_eq!(record.subject, subject);
    assert_eq!(record.token, token);
    assert_eq!(record.balance, 1000);
    assert_eq!(record.metadata, metadata);
}

#[test]
fn test_xcontract_get_checkpoint_success() {
    let ctx = setup();
    let subject = Address::generate(&ctx.env);
    let token = Address::generate(&ctx.env);
    let metadata = Symbol::new(&ctx.env, "query");

    let id = ctx
        .checkpoint_client
        .create_checkpoint(&ctx.admin, &subject, &token, &500i128, &metadata);

    let record = ctx
        .caller_client
        .call_get_checkpoint(&ctx.checkpoint_id, &id);
    assert_eq!(record.id, 1);
    assert_eq!(record.subject, subject);
    assert_eq!(record.balance, 500);
}

#[test]
fn test_xcontract_batch_create_success() {
    let ctx = setup();
    let subject_a = Address::generate(&ctx.env);
    let subject_b = Address::generate(&ctx.env);
    let token = Address::generate(&ctx.env);
    let meta = Symbol::new(&ctx.env, "batch");

    let items = Vec::from_array(
        &ctx.env,
        [
            (subject_a.clone(), token.clone(), 100i128, meta.clone()),
            (subject_b.clone(), token.clone(), 200i128, meta.clone()),
        ],
    );

    let ids =
        ctx.caller_client
            .call_batch_create_checkpoints(&ctx.checkpoint_id, &ctx.admin, &items);
    assert_eq!(ids.len(), 2);
    assert_eq!(ids.get(0).unwrap(), 1);
    assert_eq!(ids.get(1).unwrap(), 2);
    assert_eq!(ctx.checkpoint_client.get_checkpoint_count(), 2);
}

#[test]
fn test_xcontract_get_checkpoints_range_empty() {
    let ctx = setup();

    let records = ctx
        .caller_client
        .call_get_checkpoints_range(&ctx.checkpoint_id, &1u64, &10u32);
    assert!(records.is_empty());
}

#[test]
fn test_xcontract_multiple_cross_contract_calls() {
    let ctx = setup();
    let subject = Address::generate(&ctx.env);
    let token = Address::generate(&ctx.env);
    let meta = Symbol::new(&ctx.env, "multi");

    for i in 1..=5u64 {
        let id = ctx.caller_client.call_create_checkpoint(
            &ctx.checkpoint_id,
            &ctx.admin,
            &subject,
            &token,
            &((i * 100) as i128),
            &meta,
        );
        assert_eq!(id, i);
    }

    assert_eq!(ctx.checkpoint_client.get_checkpoint_count(), 5);

    let records = ctx
        .caller_client
        .call_get_checkpoints_range(&ctx.checkpoint_id, &1u64, &10u32);
    assert_eq!(records.len(), 5);
}

#[test]
fn test_xcontract_create_checkpoint_zero_balance() {
    let ctx = setup();
    let subject = Address::generate(&ctx.env);
    let token = Address::generate(&ctx.env);
    let metadata = Symbol::new(&ctx.env, "zero");

    let id = ctx.caller_client.call_create_checkpoint(
        &ctx.checkpoint_id,
        &ctx.admin,
        &subject,
        &token,
        &0i128,
        &metadata,
    );
    assert_eq!(id, 1);

    let record = ctx
        .caller_client
        .call_get_checkpoint(&ctx.checkpoint_id, &1);
    assert_eq!(record.balance, 0);
}

// ===========================================================================
// Error-return tests  (checkpoint returns Err)
// ===========================================================================

#[test]
fn test_xcontract_create_checkpoint_negative_balance() {
    let ctx = setup();
    let subject = Address::generate(&ctx.env);
    let token = Address::generate(&ctx.env);
    let metadata = Symbol::new(&ctx.env, "negative");

    let result = ctx.caller_client.try_call_create_checkpoint(
        &ctx.checkpoint_id,
        &ctx.admin,
        &subject,
        &token,
        &(-1i128),
        &metadata,
    );
    match result {
        Err(Ok(CheckpointError::AmountNegative)) => {}
        other => panic!("expected Err(Ok(AmountNegative)), got {other:?}"),
    }
    assert_eq!(ctx.checkpoint_client.get_checkpoint_count(), 0);
}

#[test]
fn test_xcontract_create_checkpoint_unauthorized() {
    let ctx = setup();
    let non_admin = Address::generate(&ctx.env);
    let subject = Address::generate(&ctx.env);
    let token = Address::generate(&ctx.env);
    let metadata = Symbol::new(&ctx.env, "unauth");

    let result = ctx.caller_client.try_call_create_checkpoint(
        &ctx.checkpoint_id,
        &non_admin,
        &subject,
        &token,
        &100i128,
        &metadata,
    );
    match result {
        Err(Ok(CheckpointError::Unauthorized)) => {}
        other => panic!("expected Err(Ok(Unauthorized)), got {other:?}"),
    }
    assert_eq!(ctx.checkpoint_client.get_checkpoint_count(), 0);
}

#[test]
fn test_xcontract_create_checkpoint_before_init() {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let checkpoint_id = env.register(CalloraCheckpoint, ());
    let caller_id = env.register(CheckpointCaller, ());
    let caller_client = CheckpointCallerClient::new(&env, &caller_id);

    let admin = Address::generate(&env);
    let subject = Address::generate(&env);
    let token = Address::generate(&env);
    let metadata = Symbol::new(&env, "pre_init");

    let result = caller_client.try_call_create_checkpoint(
        &checkpoint_id,
        &admin,
        &subject,
        &token,
        &100i128,
        &metadata,
    );
    match result {
        Err(Ok(CheckpointError::NotInitialized)) => {}
        other => panic!("expected Err(Ok(NotInitialized)), got {other:?}"),
    }
}

#[test]
fn test_xcontract_get_checkpoint_not_found() {
    let ctx = setup();

    let result = ctx
        .caller_client
        .try_call_get_checkpoint(&ctx.checkpoint_id, &999);
    match result {
        Err(Ok(CheckpointError::CheckpointNotFound)) => {}
        other => panic!("expected Err(Ok(CheckpointNotFound)), got {other:?}"),
    }
}

#[test]
fn test_xcontract_get_checkpoints_range_invalid_limit() {
    let ctx = setup();

    let result = ctx
        .caller_client
        .try_call_get_checkpoints_range(&ctx.checkpoint_id, &1u64, &0u32);
    match result {
        Err(Ok(CheckpointError::InvalidPageSize)) => {}
        other => panic!("expected Err(Ok(InvalidPageSize)), got {other:?}"),
    }
}

#[test]
fn test_xcontract_batch_create_empty() {
    let ctx = setup();
    let items: Vec<(Address, Address, i128, Symbol)> = Vec::new(&ctx.env);

    let result =
        ctx.caller_client
            .try_call_batch_create_checkpoints(&ctx.checkpoint_id, &ctx.admin, &items);
    match result {
        Err(Ok(CheckpointError::BatchEmpty)) => {}
        other => panic!("expected Err(Ok(BatchEmpty)), got {other:?}"),
    }
}

#[test]
fn test_xcontract_batch_create_too_large() {
    let ctx = setup();
    let subject = Address::generate(&ctx.env);
    let token = Address::generate(&ctx.env);
    let meta = Symbol::new(&ctx.env, "big");

    let mut items: Vec<(Address, Address, i128, Symbol)> = Vec::new(&ctx.env);
    for _ in 0..(callora_checkpoint::MAX_BATCH_SIZE + 1) {
        items.push_back((subject.clone(), token.clone(), 100i128, meta.clone()));
    }

    let result =
        ctx.caller_client
            .try_call_batch_create_checkpoints(&ctx.checkpoint_id, &ctx.admin, &items);
    match result {
        Err(Ok(CheckpointError::BatchTooLarge)) => {}
        other => panic!("expected Err(Ok(BatchTooLarge)), got {other:?}"),
    }
}

#[test]
fn test_xcontract_batch_create_negative_balance() {
    let ctx = setup();
    let subject = Address::generate(&ctx.env);
    let token = Address::generate(&ctx.env);
    let meta = Symbol::new(&ctx.env, "neg");

    let items = Vec::from_array(
        &ctx.env,
        [
            (subject.clone(), token.clone(), 100i128, meta.clone()),
            (subject.clone(), token.clone(), -50i128, meta.clone()),
        ],
    );

    let result =
        ctx.caller_client
            .try_call_batch_create_checkpoints(&ctx.checkpoint_id, &ctx.admin, &items);
    match result {
        Err(Ok(CheckpointError::AmountNegative)) => {}
        other => panic!("expected Err(Ok(AmountNegative)), got {other:?}"),
    }
    assert_eq!(ctx.checkpoint_client.get_checkpoint_count(), 0);
}

// ===========================================================================
// Panic-propagation tests  (checkpoint panics / calling contract panics)
// ===========================================================================

#[test]
fn test_xcontract_accept_admin_panics_when_no_transfer_pending() {
    let ctx = setup();
    let caller = Address::generate(&ctx.env);

    let result = ctx
        .caller_client
        .try_call_accept_admin(&ctx.checkpoint_id, &caller);
    assert!(
        result.is_err(),
        "expected panic propagation from checkpoint"
    );
}

#[test]
fn test_xcontract_create_then_panic_rolls_back_checkpoint() {
    let ctx = setup();
    let subject = Address::generate(&ctx.env);
    let token = Address::generate(&ctx.env);
    let metadata = Symbol::new(&ctx.env, "panic_rollback");

    let result = ctx.caller_client.try_call_create_checkpoint_and_panic(
        &ctx.checkpoint_id,
        &ctx.admin,
        &subject,
        &token,
        &1000i128,
        &metadata,
    );
    assert!(result.is_err(), "expected panic from the calling contract");

    assert_eq!(ctx.checkpoint_client.get_checkpoint_count(), 0);
}
