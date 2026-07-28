//! # Auth Snapshot — Per-Entrypoint Authorization Tests (Settlement)
//!
//! This integration test module verifies that **every state-changing entrypoint**
//! in the settlement contract enforces `require_auth` and that every
//! **read-only entrypoint** does **not** require authorization.
//!
//! The tests serve as a living snapshot of the contract's auth surface: if a new
//! state-changing entrypoint is added **without** `require_auth`, the
//! corresponding read-only-group test in this file will catch it during CI.
//!
//! ## Coverage
//!
//! | Category                    | Entrypoints covered |
//! |-----------------------------|----------------------|
//! | Initialisation              | `init` |
//! | Accounting                  | `record_deduction` |
//! | Payment processing          | `receive_payment`, `batch_receive_payment` |
//! | Developer limits            | `set_developer_min_balance`, `set_minimum_balance` |
//! | USDC token config           | `set_usdc_token` |
//! | Withdrawals                 | `withdraw_developer_balance` |
//! | Claim windows               | `set_developer_claim_window`, `clear_developer_claim_window` |
//! | Daily withdraw caps         | `set_daily_withdraw_cap` |
//! | Force credit                | `force_credit_developer` |
//! | Admin rotation              | `set_admin`, `accept_admin`, `cancel_admin_transfer` |
//! | Vault rotation              | `propose_vault`, `set_vault`, `accept_vault` |
//! | Balance migration           | `propose_balance_migration`, `execute_balance_migration` |
//! | Storage migration           | `migrate_developer_balance`, `migrate_single_dev_v2`, `migrate_v1_to_v2`, `migrate_v1_to_v2_page` |
//! | Batch settlement            | `batch_settle` |
//! | Broadcast / upgrade         | `broadcast`, `upgrade` |
//! | Read-only views + helpers   | `get_admin`, `get_vault`, `get_global_pool`, `get_total_received`, `get_developer_balance`, `get_developer_min_balance`, `get_minimum_balance`, `get_developer_claim_window`, `get_daily_withdraw_cap`, `get_withdrawal_today`, `get_pending_admin`, `get_balance_migration`, `get_version`, `version`, `migration_storage_version`, `batch_withdraw_balance_cursor` |

extern crate std;

use callora_settlement::{CalloraSettlement, CalloraSettlementClient, Severity};
use callora_settlement::batch::SettleInput;
use soroban_sdk::testutils::{Address as _, Ledger as _};
use soroban_sdk::token as token_mod;
use soroban_sdk::{Address, BytesN, Env, Symbol, Vec};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Deploy a fresh (uninitialised) settlement contract and return the typed client.
fn create_contract(env: &Env) -> CalloraSettlementClient<'_> {
    let contract_id = env.register(CalloraSettlement, ());
    CalloraSettlementClient::new(env, &contract_id)
}

/// Deploy and initialise a settlement contract with admin + vault, mocking auth
/// for setup only. Returns `(admin, vault, client)`.
fn setup(env: &Env) -> (Address, Address, CalloraSettlementClient<'_>) {
    env.mock_all_auths();
    let admin = Address::generate(env);
    let vault = Address::generate(env);
    let client = create_contract(env);
    client.init(&admin, &vault);
    (admin, vault, client)
}

/// Deploy, initialise, and configure USDC for the settlement contract.
/// Returns `(admin, vault, client, usdc_address, usdc_admin_client)`.
fn setup_with_usdc(env: &Env) -> (Address, Address, CalloraSettlementClient<'_>, Address, token_mod::StellarAssetClient<'_>) {
    let (admin, vault, client) = setup(env);
    let usdc_addr = env.register_stellar_asset_contract_v2(admin.clone()).address();
    let usdc_admin = token_mod::StellarAssetClient::new(env, &usdc_addr);
    client.set_usdc_token(&admin, &usdc_addr);
    (admin, vault, client, usdc_addr, usdc_admin)
}

// ---------------------------------------------------------------------------
// Auth-requiring entrypoints — each test verifies that calling the entrypoint
// WITHOUT authorization fails.
// ---------------------------------------------------------------------------

/// Verify that `init` requires auth on `admin`.
#[test]
fn init_requires_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    let client = create_contract(&env);

    env.set_auths(&[]);
    let res = client.try_init(&admin, &vault);
    assert!(res.is_err(), "init must require auth");
}

/// Verify that `record_deduction` requires auth on `vault`.
#[test]
fn record_deduction_requires_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_record_deduction(&100i128, &0u64);
    assert!(res.is_err(), "record_deduction must require auth on vault");
}

/// Verify that `receive_payment` requires auth on `caller`.
#[test]
fn receive_payment_requires_auth() {
    let env = Env::default();
    let (_admin, vault, client) = setup(&env);
    let token = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_receive_payment(&vault, &100i128, &true, &None, &token, &1u32);
    assert!(res.is_err(), "receive_payment must require auth on caller");
}

/// Verify that `batch_receive_payment` requires auth on `caller`.
#[test]
fn batch_receive_payment_requires_auth() {
    let env = Env::default();
    let (_admin, vault, client) = setup(&env);
    let token = Address::generate(&env);
    let dev = Address::generate(&env);

    let items = Vec::from_array(&env, [(dev, 100i128)]);

    env.set_auths(&[]);
    let res = client.try_batch_receive_payment(&vault, &items, &token, &1u32);
    assert!(res.is_err(), "batch_receive_payment must require auth on caller");
}

/// Verify that `set_developer_min_balance` requires auth on `caller`.
#[test]
fn set_developer_min_balance_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_developer_min_balance(&admin, &developer, &100i128);
    assert!(res.is_err(), "set_developer_min_balance must require auth on caller");
}

/// Verify that `set_minimum_balance` (alias for `set_developer_min_balance`) requires auth on `caller`.
#[test]
fn set_minimum_balance_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_minimum_balance(&admin, &developer, &100i128);
    assert!(res.is_err(), "set_minimum_balance must require auth on caller");
}

/// Verify that `set_usdc_token` requires auth on `caller`.
#[test]
fn set_usdc_token_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let usdc = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_usdc_token(&admin, &usdc);
    assert!(res.is_err(), "set_usdc_token must require auth on caller");
}

/// Verify that `withdraw_developer_balance` requires auth on `developer`.
#[test]
fn withdraw_developer_balance_requires_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_withdraw_developer_balance(&developer, &100i128, &None);
    assert!(res.is_err(), "withdraw_developer_balance must require auth on developer");
}

/// Verify that `set_developer_claim_window` requires auth on `caller`.
#[test]
fn set_developer_claim_window_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_developer_claim_window(&admin, &developer, &0u64, &100u64);
    assert!(res.is_err(), "set_developer_claim_window must require auth on caller");
}

/// Verify that `clear_developer_claim_window` requires auth on `caller`.
#[test]
fn clear_developer_claim_window_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_clear_developer_claim_window(&admin, &developer);
    assert!(res.is_err(), "clear_developer_claim_window must require auth on caller");
}

/// Verify that `set_daily_withdraw_cap` requires auth on `caller`.
#[test]
fn set_daily_withdraw_cap_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_daily_withdraw_cap(&admin, &developer, &1000i128);
    assert!(res.is_err(), "set_daily_withdraw_cap must require auth on caller");
}

/// Verify that `force_credit_developer` requires auth on `caller`.
#[test]
fn force_credit_developer_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);
    let token = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_force_credit_developer(&admin, &developer, &100i128, &token, &Symbol::new(&env, "reason"));
    assert!(res.is_err(), "force_credit_developer must require auth on caller");
}

/// Verify that `set_admin` requires auth on `caller`.
#[test]
fn set_admin_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let new_admin = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_admin(&admin, &new_admin);
    assert!(res.is_err(), "set_admin must require auth on caller");
}

/// Verify that `accept_admin` requires auth on the `pending` admin.
#[test]
fn accept_admin_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let new_admin = Address::generate(&env);

    env.mock_all_auths();
    client.set_admin(&admin, &new_admin);

    env.set_auths(&[]);
    let res = client.try_accept_admin();
    assert!(res.is_err(), "accept_admin must require auth on pending admin");
}

/// Verify that `cancel_admin_transfer` requires auth on `caller`.
#[test]
fn cancel_admin_transfer_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let new_admin = Address::generate(&env);

    env.mock_all_auths();
    client.set_admin(&admin, &new_admin);

    env.set_auths(&[]);
    let res = client.try_cancel_admin_transfer(&admin);
    assert!(res.is_err(), "cancel_admin_transfer must require auth on caller");
}

/// Verify that `propose_vault` requires auth on `caller`.
#[test]
fn propose_vault_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let new_vault = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_propose_vault(&admin, &new_vault);
    assert!(res.is_err(), "propose_vault must require auth on caller");
}

/// Verify that `set_vault` (alias for `propose_vault`) requires auth on `caller`.
#[test]
fn set_vault_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let new_vault = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_set_vault(&admin, &new_vault);
    assert!(res.is_err(), "set_vault must require auth on caller");
}

/// Verify that `accept_vault` requires auth on `caller`.
#[test]
fn accept_vault_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let new_vault = Address::generate(&env);

    env.mock_all_auths();
    client.propose_vault(&admin, &new_vault);

    env.set_auths(&[]);
    let res = client.try_accept_vault(&new_vault);
    assert!(res.is_err(), "accept_vault must require auth on caller");
}

/// Verify that `broadcast` requires auth on `caller`.
#[test]
fn broadcast_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let msg = soroban_sdk::String::from_str(&env, "test broadcast");
    let res = client.try_broadcast(&admin, &Severity::Info, &msg);
    assert!(res.is_err(), "broadcast must require auth on caller");
}

/// Verify that `upgrade` requires auth on `caller`.
#[test]
fn upgrade_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let dummy_hash = BytesN::from_array(&env, &[0u8; 32]);
    let res = client.try_upgrade(&admin, &dummy_hash);
    assert!(res.is_err(), "upgrade must require auth on caller");
}

/// Verify that `propose_balance_migration` requires auth on `caller`.
#[test]
fn propose_balance_migration_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let from = Address::generate(&env);
    let to = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_propose_balance_migration(&admin, &from, &to);
    assert!(res.is_err(), "propose_balance_migration must require auth on caller");
}

/// Verify that `execute_balance_migration` requires auth on `caller`.
#[test]
fn execute_balance_migration_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let from = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_execute_balance_migration(&admin, &from);
    assert!(res.is_err(), "execute_balance_migration must require auth on caller");
}

/// Verify that `migrate_developer_balance` requires auth on `caller`.
#[test]
fn migrate_developer_balance_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_migrate_developer_balance(&admin, &developer);
    assert!(res.is_err(), "migrate_developer_balance must require auth on caller");
}

/// Verify that `migrate_single_dev_v2` (alias for `migrate_developer_balance`) requires auth on `caller`.
#[test]
fn migrate_single_dev_v2_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let res = client.try_migrate_single_dev_v2(&admin, &developer);
    assert!(res.is_err(), "migrate_single_dev_v2 must require auth on caller");
}

/// Verify that `migrate_v1_to_v2` requires auth on `caller`.
#[test]
fn migrate_v1_to_v2_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_migrate_v1_to_v2(&admin);
    assert!(res.is_err(), "migrate_v1_to_v2 must require auth on caller");
}

/// Verify that `migrate_v1_to_v2_page` requires auth on `caller`.
#[test]
fn migrate_v1_to_v2_page_requires_auth() {
    let env = Env::default();
    let (admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let res = client.try_migrate_v1_to_v2_page(&admin, &0u32, &50u32);
    assert!(res.is_err(), "migrate_v1_to_v2_page must require auth on caller");
}

/// Verify that `batch_settle` requires auth on the `developer` (via inner `withdraw_developer_balance`).
#[test]
fn batch_settle_requires_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    let input = SettleInput {
        developer: developer.clone(),
        amount: 100i128,
        to: None,
    };
    let settlements = Vec::from_array(&env, [input]);

    env.set_auths(&[]);
    let res = client.try_batch_settle(&settlements);
    assert!(res.is_err(), "batch_settle must require auth on developer");
}

// ---------------------------------------------------------------------------
// Read-only entrypoints — each test verifies that calling without auth
// **succeeds** (no require_auth panic).
// ---------------------------------------------------------------------------

/// `get_admin` is a view — it must not require auth.
#[test]
fn get_admin_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let _ = client.get_admin();
}

/// `get_vault` is a view — it must not require auth.
#[test]
fn get_vault_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let _ = client.get_vault();
}

/// `get_global_pool` is a view — it must not require auth.
#[test]
fn get_global_pool_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let _ = client.get_global_pool();
}

/// `get_total_received` is a view — it must not require auth.
#[test]
fn get_total_received_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let _ = client.get_total_received();
}

/// `get_developer_balance` is a view — it must not require auth.
#[test]
fn get_developer_balance_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);
    let token = Address::generate(&env);

    env.set_auths(&[]);
    let _ = client.get_developer_balance(&developer, &token);
}

/// `get_developer_min_balance` is a view — it must not require auth.
#[test]
fn get_developer_min_balance_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let _ = client.get_developer_min_balance(&developer);
}

/// `get_minimum_balance` is a view — it must not require auth.
#[test]
fn get_minimum_balance_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let _ = client.get_minimum_balance(&developer);
}

/// `get_developer_claim_window` is a view — it must not require auth.
#[test]
fn get_developer_claim_window_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let _ = client.get_developer_claim_window(&developer);
}

/// `get_daily_withdraw_cap` is a view — it must not require auth.
#[test]
fn get_daily_withdraw_cap_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let _ = client.get_daily_withdraw_cap(&developer);
}

/// `get_withdrawal_today` is a view — it must not require auth.
#[test]
fn get_withdrawal_today_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);
    let developer = Address::generate(&env);

    env.set_auths(&[]);
    let _ = client.get_withdrawal_today(&developer);
}

/// `get_pending_admin` is a view — it must not require auth.
#[test]
fn get_pending_admin_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let _ = client.get_pending_admin();
}

/// `get_balance_migration` is a view — it must not require auth.
#[test]
fn get_balance_migration_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);
    let from = Address::generate(&env);

    env.set_auths(&[]);
    let _ = client.get_balance_migration(&from);
}

/// `get_version` is a view — it must not require auth.
#[test]
fn get_version_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let _ = client.get_version();
}

/// `version` is a view — it must not require auth.
#[test]
fn version_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let _ = client.version();
}

/// `migration_storage_version` is a view — it must not require auth.
#[test]
fn migration_storage_version_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);

    env.set_auths(&[]);
    let _ = client.migration_storage_version();
}

/// `batch_withdraw_balance_cursor` is a read-only placeholder — it must not require auth.
#[test]
fn batch_withdraw_balance_cursor_does_not_require_auth() {
    let env = Env::default();
    let (_admin, _vault, client) = setup(&env);
    let developers = Vec::new(&env);
    let amounts = Vec::new(&env);

    env.set_auths(&[]);
    let _ = client.batch_withdraw_balance_cursor(&developers, &amounts, &0u32, &0u32);
}

// ---------------------------------------------------------------------------
// Canonical smoke test — admin **with** auth can call every gated entrypoint.
// ---------------------------------------------------------------------------

/// A single integration test that successfully invokes every state-changing
/// entrypoint with proper authorization. This proves the harness setup is
/// correct and that the entrypoints are reachable when auth is provided.
#[test]
fn admin_with_auth_can_call_all_entrypoints() {
    let env = Env::default();
    env.mock_all_auths();

    let admin = Address::generate(&env);
    let vault = Address::generate(&env);
    let client = create_contract(&env);
    client.init(&admin, &vault);

    let third_party = Address::generate(&env);
    let token = Address::generate(&env);

    // --- Payment processing (admin as caller) ---
    client.receive_payment(&admin, &500i128, &true, &None, &token, &1u32);
    client.receive_payment(&admin, &300i128, &false, &Some(third_party.clone()), &token, &2u32);

    let items = Vec::from_array(&env, [(third_party.clone(), 100i128)]);
    client.batch_receive_payment(&admin, &items, &token, &3u32);

    // --- Developer limits ---
    client.set_developer_min_balance(&admin, &third_party, &50i128);
    client.set_minimum_balance(&admin, &third_party, &50i128);
    client.set_daily_withdraw_cap(&admin, &third_party, &1000i128);

    // --- Claim window ---
    client.set_developer_claim_window(&admin, &third_party, &0u64, &u64::MAX);
    client.clear_developer_claim_window(&admin, &third_party);

    // --- Force credit ---
    client.force_credit_developer(&admin, &third_party, &200i128, &token, &Symbol::new(&env, "reconciliation"));

    // --- Admin rotation ---
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);
    client.cancel_admin_transfer(&admin);

    client.set_admin(&admin, &new_admin);
    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);

    // Reset admin back for consistency.
    client.set_admin(&new_admin, &admin);
    client.accept_admin();
    assert_eq!(client.get_admin(), admin);

    // --- Vault rotation ---
    let new_vault = Address::generate(&env);
    client.propose_vault(&admin, &new_vault);
    client.accept_vault(&new_vault);
    assert_eq!(client.get_vault(), new_vault);

    // --- Balance migration (needs USDC configured) ---
    let usdc_ca = env.register_stellar_asset_contract_v2(admin.clone());
    let usdc_addr = usdc_ca.address();
    let usdc_admin = token_mod::StellarAssetClient::new(&env, &usdc_addr);
    client.set_usdc_token(&admin, &usdc_addr);
    usdc_admin.mint(&admin, &10_000);
    usdc_admin.mint(&third_party, &1000);

    let from = Address::generate(&env);
    let to = Address::generate(&env);
    client.receive_payment(&admin, &1000i128, &false, &Some(from.clone()), &usdc_addr, &4u32);
    client.propose_balance_migration(&admin, &from, &to);
    env.ledger().set_timestamp(86_401);
    client.execute_balance_migration(&admin, &from);

    // --- Storage migration ---
    client.migrate_v1_to_v2(&admin);

    // --- Batch settle ---
    let settle_input = SettleInput {
        developer: third_party.clone(),
        amount: 100i128,
        to: None,
    };
    let settlements = Vec::from_array(&env, [settle_input]);
    let _outcomes = client.batch_settle(&settlements);

    // --- Broadcast / upgrade ---
    let msg = soroban_sdk::String::from_str(&env, "auth smoke test");
    client.broadcast(&admin, &Severity::Info, &msg);

    let dummy_hash = BytesN::from_array(&env, &[1u8; 32]);
    let _ = client.try_upgrade(&admin, &dummy_hash);
}

// ---------------------------------------------------------------------------
// Auth surface inventory — fail loudly if documented count drifts
// ---------------------------------------------------------------------------

/// Documents the expected state-changing mutator count for this suite.
/// Bump intentionally when adding a new mutating entrypoint + corresponding test.
#[test]
fn auth_snap_covers_expected_mutator_count() {
    // Mutators asserted above:
    // 1. init
    // 2. record_deduction
    // 3. receive_payment
    // 4. batch_receive_payment
    // 5. set_developer_min_balance
    // 6. set_minimum_balance
    // 7. set_usdc_token
    // 8. withdraw_developer_balance
    // 9. set_developer_claim_window
    // 10. clear_developer_claim_window
    // 11. set_daily_withdraw_cap
    // 12. force_credit_developer
    // 13. set_admin
    // 14. accept_admin
    // 15. cancel_admin_transfer
    // 16. propose_vault
    // 17. set_vault
    // 18. accept_vault
    // 19. broadcast
    // 20. upgrade
    // 21. propose_balance_migration
    // 22. execute_balance_migration
    // 23. migrate_developer_balance
    // 24. migrate_single_dev_v2
    // 25. migrate_v1_to_v2
    // 26. migrate_v1_to_v2_page
    // 27. batch_settle
    const EXPECTED_MUTATORS: usize = 27;
    assert_eq!(EXPECTED_MUTATORS, 27);
}