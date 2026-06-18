/// Tests for request_id idempotency in `deduct` and `batch_deduct`.
///
/// # Coverage
/// - Duplicate `Some(request_id)` is rejected with `DuplicateRequestId`.
/// - Distinct `request_id` values each succeed independently.
/// - `None` request_id is never deduplicated (fire-and-forget).
/// - `batch_deduct` rejects a batch containing a duplicate id atomically.
/// - `batch_deduct` rejects a batch where two items share the same new id.
/// - `is_request_processed` view reflects processed state correctly.
/// - Failed deducts (insufficient balance, paused) do NOT mark the id.
extern crate std;

use soroban_sdk::testutils::Address as _;
use soroban_sdk::{token, Address, Env, Symbol};

use super::*;

use callora_settlement::CalloraSettlement;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let ca = env.register_stellar_asset_contract_v2(admin.clone());
    let addr = ca.address();
    (
        addr.clone(),
        token::Client::new(env, &addr),
        token::StellarAssetClient::new(env, &addr),
    )
}

fn create_vault(env: &Env) -> (Address, CalloraVaultClient<'_>) {
    let addr = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &addr);
    (addr, client)
}

/// Register and initialize the settlement contract.
fn create_settlement(env: &Env, admin: &Address, vault_address: &Address) -> Address {
    let settlement_address = env.register(CalloraSettlement, ());
    let settlement_client = callora_settlement::CalloraSettlementClient::new(env, &settlement_address);
    settlement_client.init(admin, vault_address);
    settlement_address
}

/// Set up a vault with `balance` USDC, a settlement address, and return
/// `(vault_addr, client, settlement_addr, owner)`.
fn setup_vault(env: &Env, balance: i128) -> (Address, CalloraVaultClient<'_>, Address, Address) {
    env.mock_all_auths();
    let owner = Address::generate(env);
    let (vault_addr, client) = create_vault(env);
    let (usdc, _, usdc_admin) = create_usdc(env, &owner);
    usdc_admin.mint(&vault_addr, &balance);
    client.init(&owner, &usdc, &Some(balance), &None, &None, &None, &None);
    let settlement = create_settlement(env, &owner, &vault_addr);
    client.set_settlement(&owner, &settlement);
    (vault_addr, client, settlement, owner)
}

// ---------------------------------------------------------------------------
// deduct — single call idempotency
// ---------------------------------------------------------------------------

/// Pin the numeric error code so a future renumber regression is caught immediately.
#[test]
fn duplicate_request_id_error_code_is_29() {
    assert_eq!(VaultError::DuplicateRequestId as u32, 29);
}

/// A `Some(request_id)` deduct succeeds on first call and is rejected on retry.
#[test]
fn deduct_duplicate_request_id_rejected() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 1_000);

    let rid = Symbol::new(&env, "req_001");

    // First call — must succeed.
    let remaining = client.deduct(&owner, &100, &Some(rid.clone()));
    assert_eq!(remaining, 900);

    // Second call with same request_id — must be rejected.
    let result = client.try_deduct(&owner, &100, &Some(rid.clone()));
    assert!(
        result.is_err(),
        "duplicate request_id must be rejected"
    );

    // Balance must be unchanged after the rejected retry.
    assert_eq!(client.balance(), 900, "balance must not change on duplicate");
}

/// Two distinct `request_id` values each succeed independently.
#[test]
fn deduct_distinct_request_ids_both_succeed() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 1_000);

    let rid_a = Symbol::new(&env, "req_a");
    let rid_b = Symbol::new(&env, "req_b");

    let after_a = client.deduct(&owner, &100, &Some(rid_a.clone()));
    assert_eq!(after_a, 900);

    let after_b = client.deduct(&owner, &200, &Some(rid_b.clone()));
    assert_eq!(after_b, 700);

    assert_eq!(client.balance(), 700);
}

/// `None` request_id is never deduplicated — multiple calls all go through.
#[test]
fn deduct_none_request_id_not_deduplicated() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 1_000);

    // Three calls with None — all must succeed.
    assert_eq!(client.deduct(&owner, &100, &None), 900);
    assert_eq!(client.deduct(&owner, &100, &None), 800);
    assert_eq!(client.deduct(&owner, &100, &None), 700);
    assert_eq!(client.balance(), 700);
}

/// A failed deduct (insufficient balance) must NOT mark the request_id as processed.
#[test]
fn deduct_failed_due_to_insufficient_balance_does_not_mark_id() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 50);

    let rid = Symbol::new(&env, "req_fail");

    // Attempt to deduct more than the balance — must fail.
    let result = client.try_deduct(&owner, &100, &Some(rid.clone()));
    assert!(result.is_err(), "expected insufficient balance error");

    // The id must NOT be marked — a retry with sufficient balance should succeed.
    // Top up the vault first.
    // (We can't deposit here without a depositor setup, so we verify via is_request_processed.)
    assert!(
        !client.is_request_processed(&rid),
        "failed deduct must not mark request_id"
    );
}

/// A failed deduct (vault paused) must NOT mark the request_id as processed.
#[test]
fn deduct_failed_due_to_paused_does_not_mark_id() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 500);

    let rid = Symbol::new(&env, "req_paused");

    client.pause(&owner);
    let result = client.try_deduct(&owner, &100, &Some(rid.clone()));
    assert!(result.is_err(), "expected paused error");

    assert!(
        !client.is_request_processed(&rid),
        "paused deduct must not mark request_id"
    );
}

// ---------------------------------------------------------------------------
// is_request_processed view
// ---------------------------------------------------------------------------

/// `is_request_processed` returns false before any deduct.
#[test]
fn is_request_processed_false_before_deduct() {
    let env = Env::default();
    let (_, client, _, _) = setup_vault(&env, 500);

    let rid = Symbol::new(&env, "unseen");
    assert!(!client.is_request_processed(&rid));
}

/// `is_request_processed` returns true after a successful deduct with that id.
#[test]
fn is_request_processed_true_after_successful_deduct() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 500);

    let rid = Symbol::new(&env, "seen");
    client.deduct(&owner, &50, &Some(rid.clone()));

    assert!(
        client.is_request_processed(&rid),
        "is_request_processed must return true after successful deduct"
    );
}

/// `is_request_processed` returns false for a different id even after another was processed.
#[test]
fn is_request_processed_false_for_different_id() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 500);

    let rid_a = Symbol::new(&env, "id_a");
    let rid_b = Symbol::new(&env, "id_b");

    client.deduct(&owner, &50, &Some(rid_a.clone()));

    assert!(client.is_request_processed(&rid_a));
    assert!(!client.is_request_processed(&rid_b));
}

// ---------------------------------------------------------------------------
// batch_deduct — idempotency
// ---------------------------------------------------------------------------

/// A batch containing a previously-processed `request_id` is rejected atomically.
#[test]
fn batch_deduct_duplicate_request_id_rejected_atomically() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 1_000);

    let rid = Symbol::new(&env, "batch_dup");

    // First single deduct marks the id.
    client.deduct(&owner, &100, &Some(rid.clone()));
    assert_eq!(client.balance(), 900);

    // Batch that reuses the same id — must be rejected atomically.
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 50,
            request_id: Some(rid.clone()),
        },
        DeductItem {
            amount: 50,
            request_id: None,
        },
    ];
    let result = client.try_batch_deduct(&owner, &items);
    assert!(result.is_err(), "batch with duplicate id must be rejected");

    // Balance must be unchanged — full atomicity.
    assert_eq!(client.balance(), 900, "balance must not change on duplicate batch");
}

/// A batch where two items share the same new `request_id` is rejected.
#[test]
fn batch_deduct_two_items_same_new_id_rejected() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 1_000);

    let rid = Symbol::new(&env, "shared_id");

    // Both items carry the same id — the second one is a duplicate of the first.
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: Some(rid.clone()),
        },
        DeductItem {
            amount: 100,
            request_id: Some(rid.clone()),
        },
    ];
    let result = client.try_batch_deduct(&owner, &items);
    assert!(
        result.is_err(),
        "batch with two items sharing the same new id must be rejected"
    );

    // Balance must be unchanged.
    assert_eq!(client.balance(), 1_000);
    // The id must NOT have been marked (batch was rejected).
    assert!(
        !client.is_request_processed(&rid),
        "rejected batch must not mark request_id"
    );
}

/// A batch with all distinct `Some` ids succeeds and marks all of them.
#[test]
fn batch_deduct_distinct_ids_all_succeed_and_marked() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 1_000);

    let rid_1 = Symbol::new(&env, "b_id_1");
    let rid_2 = Symbol::new(&env, "b_id_2");
    let rid_3 = Symbol::new(&env, "b_id_3");

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: Some(rid_1.clone()),
        },
        DeductItem {
            amount: 200,
            request_id: Some(rid_2.clone()),
        },
        DeductItem {
            amount: 50,
            request_id: Some(rid_3.clone()),
        },
    ];
    let remaining = client.batch_deduct(&owner, &items);
    assert_eq!(remaining, 650);

    // All three ids must now be marked.
    assert!(client.is_request_processed(&rid_1));
    assert!(client.is_request_processed(&rid_2));
    assert!(client.is_request_processed(&rid_3));
}

/// A batch with `None` ids succeeds and does not mark anything.
#[test]
fn batch_deduct_none_ids_not_marked() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 1_000);

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: None,
        },
        DeductItem {
            amount: 200,
            request_id: None,
        },
    ];
    let remaining = client.batch_deduct(&owner, &items);
    assert_eq!(remaining, 700);

    // No ids were provided — nothing should be marked.
    // We verify by checking a sentinel id is still unprocessed.
    let sentinel = Symbol::new(&env, "sentinel");
    assert!(!client.is_request_processed(&sentinel));
}

/// A batch that fails due to insufficient balance does NOT mark any ids.
#[test]
fn batch_deduct_failed_insufficient_balance_does_not_mark_ids() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 100);

    let rid_a = Symbol::new(&env, "fail_a");
    let rid_b = Symbol::new(&env, "fail_b");

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 60,
            request_id: Some(rid_a.clone()),
        },
        DeductItem {
            amount: 60, // cumulative 120 > 100
            request_id: Some(rid_b.clone()),
        },
    ];
    let result = client.try_batch_deduct(&owner, &items);
    assert!(result.is_err(), "expected insufficient balance error");

    // Neither id must be marked.
    assert!(!client.is_request_processed(&rid_a));
    assert!(!client.is_request_processed(&rid_b));
    assert_eq!(client.balance(), 100);
}

/// After a successful deduct, retrying with the same id returns DuplicateRequestId
/// regardless of the amount.
#[test]
fn deduct_retry_with_different_amount_still_rejected() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 1_000);

    let rid = Symbol::new(&env, "retry_amt");

    client.deduct(&owner, &100, &Some(rid.clone()));

    // Retry with a different amount — still rejected.
    let result = client.try_deduct(&owner, &50, &Some(rid.clone()));
    assert!(result.is_err(), "retry with different amount must be rejected");
    assert_eq!(client.balance(), 900);
}

/// Mixed batch: some items have `Some` ids, some have `None`.
/// All `Some` ids are marked; `None` items are not.
#[test]
fn batch_deduct_mixed_ids_marks_only_some_ids() {
    let env = Env::default();
    let (_, client, _, owner) = setup_vault(&env, 1_000);

    let rid_x = Symbol::new(&env, "mix_x");
    let rid_z = Symbol::new(&env, "mix_z");

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: Some(rid_x.clone()),
        },
        DeductItem {
            amount: 50,
            request_id: None,
        },
        DeductItem {
            amount: 75,
            request_id: Some(rid_z.clone()),
        },
    ];
    let remaining = client.batch_deduct(&owner, &items);
    assert_eq!(remaining, 775);

    assert!(client.is_request_processed(&rid_x));
    assert!(client.is_request_processed(&rid_z));

    // Retrying either Some id must fail.
    assert!(client.try_deduct(&owner, &10, &Some(rid_x)).is_err());
    assert!(client.try_deduct(&owner, &10, &Some(rid_z)).is_err());

    // None deducts still go through.
    assert_eq!(client.deduct(&owner, &10, &None), 765);
}
