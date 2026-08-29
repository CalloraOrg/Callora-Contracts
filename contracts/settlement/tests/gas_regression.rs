#![cfg(test)]

//! Gas / resource regression limits for settlement entry points (issue #1069).
//!
//! These tests pin the resource budget of the looping/batch entry points so
//! that input size, batch size, loops, and storage growth stay bounded and
//! predictable for both normal and adversarial inputs. They measure the
//! instruction (CPU) and memory consumed by representative calls using the
//! Soroban host budget tracker and fail if a call exceeds its documented
//! ceiling or if a failure path leaves partial state behind.
//!
//! Ceilings are "representative budgets" with ~2x headroom over the measured
//! native-test cost, per the SDK caveat that native test runs underestimate
//! WASM cost. They are intentionally loose enough to avoid CI flake while
//! still catching regressions that multiply cost (e.g. unbounded loops, batch
//! caps removed, per-item scans reintroduced).

extern crate std;

use callora_settlement::{CalloraSettlement, CalloraSettlementClient, SettlementError};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, Env, String, Vec};

// ─── Documented resource budgets (CPU instructions / memory bytes) ──────────

/// Ceiling for a single-item `batch_receive_payment` (measured ~172k / ~26k).
const BRP_ONE_CPU_CEILING: u64 = 400_000;
const BRP_ONE_MEM_CEILING: u64 = 60_000;

/// Ceiling for a max-size (50-item) `batch_receive_payment` (measured ~10.8M / ~2.1M).
const BRP_MAX_CPU_CEILING: u64 = 22_000_000;
const BRP_MAX_MEM_CEILING: u64 = 4_200_000;

/// Hard budget enforced for a max-size batch: the call must fit or the test fails.
const BRP_MAX_CPU_BUDGET: u64 = 25_000_000;
const BRP_MAX_MEM_BUDGET: u64 = 5_000_000;

/// Ceiling for a single-item `batch_settle` (measured ~74k / ~10k).
const BS_ONE_CPU_CEILING: u64 = 200_000;
const BS_ONE_MEM_CEILING: u64 = 30_000;

/// Ceiling for an over-cap (65-item) `batch_settle` rejection (measured ~239k / ~38k).
const BS_OVER_CAP_CPU_CEILING: u64 = 600_000;
const BS_OVER_CAP_MEM_CEILING: u64 = 100_000;

/// Ceiling for a full-page (100-record) paginated read (measured ~4.1M / ~0.5M).
const PAGE_CPU_CEILING: u64 = 10_000_000;
const PAGE_MEM_CEILING: u64 = 1_200_000;

/// Ceiling for cheap rejection paths (over-limit / invalid input).
const REJECT_CPU_CEILING: u64 = 500_000;
const REJECT_MEM_CEILING: u64 = 80_000;

// ─── Helpers ────────────────────────────────────────────────────────────────

fn setup(env: &Env) -> (Address, Address, CalloraSettlementClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let contract_id = env.register(CalloraSettlement, ());
    let client = CalloraSettlementClient::new(env, &contract_id);
    client.init(&admin, &vault);
    (admin, vault, client)
}

// ─── batch_receive_payment ──────────────────────────────────────────────────

#[test]
fn batch_receive_payment_single_item_within_budget() {
    let env = Env::default();
    let (_admin, vault, client) = setup(&env);
    let token = Address::generate(&env);
    let dev = Address::generate(&env);

    let mut items = Vec::new(&env);
    items.push_back((dev.clone(), 100_i128));

    env.budget().reset_tracker();
    client.batch_receive_payment(&vault, &items, &token, &1);
    let used_cpu = env.budget().cpu_instruction_cost();
    let used_mem = env.budget().memory_bytes_cost();

    assert!(
        used_cpu <= BRP_ONE_CPU_CEILING,
        "single-item batch_receive_payment exceeded CPU ceiling: {used_cpu} > {BRP_ONE_CPU_CEILING}"
    );
    assert!(
        used_mem <= BRP_ONE_MEM_CEILING,
        "single-item batch_receive_payment exceeded memory ceiling: {used_mem} > {BRP_ONE_MEM_CEILING}"
    );
    assert_eq!(client.get_developer_balance(&dev, &token), 100);
}

#[test]
fn batch_receive_payment_max_batch_within_budget() {
    let env = Env::default();
    let (_admin, vault, client) = setup(&env);
    let token = Address::generate(&env);

    let mut items = Vec::new(&env);
    for _ in 0..50 {
        items.push_back((Address::generate(&env), 100_i128));
    }

    // Enforce the documented budget: exceeding it aborts the test.
    env.budget()
        .reset_limits(BRP_MAX_CPU_BUDGET, BRP_MAX_MEM_BUDGET);
    env.budget().reset_tracker();
    client.batch_receive_payment(&vault, &items, &token, &1);
    let used_cpu = env.budget().cpu_instruction_cost();
    let used_mem = env.budget().memory_bytes_cost();

    assert!(
        used_cpu <= BRP_MAX_CPU_CEILING,
        "max batch_receive_payment exceeded CPU ceiling: {used_cpu} > {BRP_MAX_CPU_CEILING}"
    );
    assert!(
        used_mem <= BRP_MAX_MEM_CEILING,
        "max batch_receive_payment exceeded memory ceiling: {used_mem} > {BRP_MAX_MEM_CEILING}"
    );
}

#[test]
fn batch_receive_payment_over_limit_rejected_cheaply_and_atomically() {
    let env = Env::default();
    let (_admin, vault, client) = setup(&env);
    let token = Address::generate(&env);
    let dev = Address::generate(&env);

    let mut items = Vec::new(&env);
    items.push_back((dev.clone(), 100_i128));
    for _ in 0..50 {
        items.push_back((Address::generate(&env), 100_i128));
    }

    env.budget().reset_tracker();
    let res = client.try_batch_receive_payment(&vault, &items, &token, &1);
    let used_cpu = env.budget().cpu_instruction_cost();
    let used_mem = env.budget().memory_bytes_cost();

    assert!(res.is_err(), "over-limit batch must be rejected");
    assert!(
        used_cpu <= REJECT_CPU_CEILING,
        "over-limit rejection consumed too much CPU: {used_cpu} > {REJECT_CPU_CEILING}"
    );
    assert!(
        used_mem <= REJECT_MEM_CEILING,
        "over-limit rejection consumed too much memory: {used_mem} > {REJECT_MEM_CEILING}"
    );
    // No partial state: the batch is rejected before any balance is written.
    assert_eq!(
        client.get_developer_balance(&dev, &token),
        0,
        "rejected batch must not write balances"
    );
}

#[test]
fn batch_receive_payment_invalid_amount_leaves_no_partial_state() {
    let env = Env::default();
    let (_admin, vault, client) = setup(&env);
    let token = Address::generate(&env);
    let dev = Address::generate(&env);

    let mut items = Vec::new(&env);
    items.push_back((dev.clone(), 100_i128));
    // Invalid amount in the middle of the batch.
    items.push_back((Address::generate(&env), 0_i128));
    for _ in 0..10 {
        items.push_back((Address::generate(&env), 100_i128));
    }

    env.budget().reset_tracker();
    let res = client.try_batch_receive_payment(&vault, &items, &token, &1);
    let used_cpu = env.budget().cpu_instruction_cost();
    let used_mem = env.budget().memory_bytes_cost();

    assert!(res.is_err(), "batch with invalid amount must be rejected");
    assert!(
        used_cpu <= REJECT_CPU_CEILING,
        "invalid-amount rejection consumed too much CPU: {used_cpu} > {REJECT_CPU_CEILING}"
    );
    assert!(
        used_mem <= REJECT_MEM_CEILING,
        "invalid-amount rejection consumed too much memory: {used_mem} > {REJECT_MEM_CEILING}"
    );
    // Validation runs before any state mutation: nothing may be credited.
    assert_eq!(
        client.get_developer_balance(&dev, &token),
        0,
        "rejected batch must not credit the valid leading item"
    );
}

#[test]
fn batch_receive_payment_storage_growth_is_bounded() {
    let env = Env::default();
    let (_admin, vault, client) = setup(&env);
    let token = Address::generate(&env);

    // Seed 100 distinct developers in two max-size batches.
    let mut first = Vec::new(&env);
    for _ in 0..50 {
        first.push_back((Address::generate(&env), 50_i128));
    }
    client.batch_receive_payment(&vault, &first, &token, &1);

    // Re-credit the SAME 50 developers: the developer index must not grow
    // (sorted_insert dedupes), so storage growth stays bounded.
    client.batch_receive_payment(&vault, &first, &token, &2);

    let page = client.get_developer_balances_page(&_admin, &0, &100, &token);
    assert_eq!(page.len(), 50, "index must not grow on re-credit");

    // A second distinct set of 50 grows the index to exactly 100.
    let mut second = Vec::new(&env);
    for _ in 0..50 {
        second.push_back((Address::generate(&env), 50_i128));
    }
    client.batch_receive_payment(&vault, &second, &token, &3);
    let page = client.get_developer_balances_page(&_admin, &0, &100, &token);
    assert_eq!(
        page.len(),
        100,
        "index must track distinct developers exactly"
    );
}

// ─── batch_settle ───────────────────────────────────────────────────────────

#[test]
fn batch_settle_single_item_within_budget() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let dev = Address::generate(&env);
    let token = Address::generate(&env);
    // Configure USDC so the per-item withdrawal path is fully exercised
    // (fails with InsufficientBalance instead of UsdcTokenNotConfigured).
    client.set_usdc_token(&admin, &token);

    let mut settles = Vec::new(&env);
    settles.push_back(callora_settlement::batch::SettleInput {
        developer: dev.clone(),
        amount: 100,
        to: None,
    });

    env.budget().reset_tracker();
    let outcomes = client.batch_settle(&settles);
    let used_cpu = env.budget().cpu_instruction_cost();
    let used_mem = env.budget().memory_bytes_cost();

    assert_eq!(outcomes.len(), 1);
    assert_eq!(
        outcomes.get(0).unwrap(),
        callora_settlement::batch::SettleOutcome::InsufficientBalance,
        "unfunded developer must report a bounded per-item outcome"
    );
    assert!(
        used_cpu <= BS_ONE_CPU_CEILING,
        "single-item batch_settle exceeded CPU ceiling: {used_cpu} > {BS_ONE_CPU_CEILING}"
    );
    assert!(
        used_mem <= BS_ONE_MEM_CEILING,
        "single-item batch_settle exceeded memory ceiling: {used_mem} > {BS_ONE_MEM_CEILING}"
    );
}

#[test]
fn batch_settle_over_cap_rejected_within_budget() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);

    let mut settles = Vec::new(&env);
    for _ in 0..65 {
        settles.push_back(callora_settlement::batch::SettleInput {
            developer: Address::generate(&env),
            amount: 100,
            to: None,
        });
    }

    env.budget().reset_tracker();
    let outcomes = client.batch_settle(&settles);
    let used_cpu = env.budget().cpu_instruction_cost();
    let used_mem = env.budget().memory_bytes_cost();

    assert_eq!(outcomes.len(), 65, "outcome count must match input count");
    for i in 0..65 {
        assert_eq!(
            outcomes.get(i).unwrap(),
            callora_settlement::batch::SettleOutcome::OtherError,
            "over-cap item {i} must be OtherError"
        );
    }
    assert!(
        used_cpu <= BS_OVER_CAP_CPU_CEILING,
        "over-cap batch_settle exceeded CPU ceiling: {used_cpu} > {BS_OVER_CAP_CPU_CEILING}"
    );
    assert!(
        used_mem <= BS_OVER_CAP_MEM_CEILING,
        "over-cap batch_settle exceeded memory ceiling: {used_mem} > {BS_OVER_CAP_MEM_CEILING}"
    );
}

// ─── Pagination ─────────────────────────────────────────────────────────────

#[test]
fn get_developer_balances_page_full_page_within_budget() {
    let env = Env::default();
    let (admin, vault, client) = setup(&env);
    let token = Address::generate(&env);

    for _ in 0..2 {
        let mut items = Vec::new(&env);
        for _ in 0..50 {
            items.push_back((Address::generate(&env), 50_i128));
        }
        client.batch_receive_payment(&vault, &items, &token, &1);
    }

    env.budget().reset_tracker();
    let page = client.get_developer_balances_page(&admin, &0, &100, &token);
    let used_cpu = env.budget().cpu_instruction_cost();
    let used_mem = env.budget().memory_bytes_cost();

    assert_eq!(page.len(), 100);
    assert!(
        used_cpu <= PAGE_CPU_CEILING,
        "full-page read exceeded CPU ceiling: {used_cpu} > {PAGE_CPU_CEILING}"
    );
    assert!(
        used_mem <= PAGE_MEM_CEILING,
        "full-page read exceeded memory ceiling: {used_mem} > {PAGE_MEM_CEILING}"
    );
}

// ─── Explicit input-size limits ─────────────────────────────────────────────

#[test]
fn broadcast_message_length_is_capped() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);

    // Exactly at the cap: accepted.
    let ok_msg = String::from_str(&env, &"x".repeat(1024));
    client.broadcast(&admin, &callora_settlement::Severity::Info, &ok_msg);

    // One byte over the cap: rejected before any event is emitted.
    let too_long = String::from_str(&env, &"x".repeat(1025));
    let res = client.try_broadcast(&admin, &callora_settlement::Severity::Info, &too_long);
    assert!(res.is_err(), "over-length broadcast must be rejected");

    // Non-admin callers are still rejected (auth unchanged).
    let stranger = Address::generate(&env);
    env.set_auths(&[]);
    let res = client.try_broadcast(&stranger, &callora_settlement::Severity::Info, &ok_msg);
    assert!(res.is_err(), "non-admin broadcast must be rejected");
}

#[test]
fn batch_withdraw_balance_cursor_cap_enforced() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);

    let mut developers = Vec::new(&env);
    let mut amounts = Vec::new(&env);
    for _ in 0..51 {
        developers.push_back(Address::generate(&env));
        amounts.push_back(100_i128);
    }

    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        client.batch_withdraw_balance_cursor(&developers, &amounts, &0, &50);
    }))
    .is_err();
    assert!(panicked, "over-cap batch_withdraw must be rejected");
}
