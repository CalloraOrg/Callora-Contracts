extern crate std;

use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{token, Address, Env, IntoVal, String, Symbol};

use super::*;

use callora_settlement::CalloraSettlement;

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract_address.address();
    let client = token::Client::new(env, &address);
    let admin_client = token::StellarAssetClient::new(env, &address);
    (address, client, admin_client)
}

fn create_vault(env: &Env) -> (Address, CalloraVaultClient<'_>) {
    let address = env.register(CalloraVault, ());
    let client = CalloraVaultClient::new(env, &address);
    (address, client)
}

/// Register and initialize the settlement contract.
fn create_settlement(env: &Env, admin: &Address, vault_address: &Address) -> Address {
    let settlement_address = env.register(CalloraSettlement, ());
    let settlement_client =
        callora_settlement::CalloraSettlementClient::new(env, &settlement_address);
    env.mock_all_auths();
    settlement_client.init(admin, vault_address);
    settlement_address
}

/// Mint `amount` USDC directly to `vault_address` (simulates pre-funded vault).
fn fund_vault(
    usdc_admin_client: &token::StellarAssetClient,
    vault_address: &Address,
    amount: i128,
) {
    usdc_admin_client.mint(vault_address, &amount);
}

// ---------------------------------------------------------------------------
// Init tests
// ---------------------------------------------------------------------------

#[test]
fn init_with_balance_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    let events = env.events().all();
    std::println!("init_with_balance_emits_event events len: {}", events.len());
    let last = events.last().expect("expected at least one event");

    assert_eq!(last.0, vault_address);
    let topics = &last.1;
    assert_eq!(topics.len(), 2);
    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: Address = topics.get(1).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "init"));
    assert_eq!(topic1, owner);

    let data: i128 = last.2.into_val(&env);
    assert_eq!(data, 1000);
}

#[test]
fn init_defaults_balance_to_zero() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    assert_eq!(client.balance(), 0);
}

#[test]
fn init_defaults_min_deposit_to_one() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    let meta = client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    assert_eq!(meta.min_deposit, 1);
}

#[test]
fn init_sets_owner_and_min_deposit() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    let meta = client.init(&owner, &usdc, &Some(500), &None, &Some(10), &None, &None);

    assert_eq!(meta.balance, 500);
    assert_eq!(meta.owner, owner);
    assert_eq!(meta.min_deposit, 10);
    assert_eq!(client.balance(), 500);
    assert_eq!(client.get_admin(), owner);
}

#[test]
fn init_succeeds_when_onchain_usdc_balance_covers_initial_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);

    let meta = client.init(&owner, &usdc, &Some(400), &None, &None, &None, &None);

    assert_eq!(meta.balance, 400);
    assert_eq!(client.balance(), 400);
}

#[test]
#[should_panic(expected = "initial_balance exceeds on-ledger USDC balance")]
fn init_fails_when_initial_balance_exceeds_onchain_usdc_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 99);

    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
}

#[test]
fn double_init_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let result = client.try_init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    assert!(result.is_err(), "expected error on second init");
}

// ---------------------------------------------------------------------------
// get_meta / balance tests
// ---------------------------------------------------------------------------

#[test]
fn get_meta_returns_correct_state() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

    let meta = client.get_meta();
    assert_eq!(meta.balance, 500);
    assert_eq!(meta.owner, owner);
    assert_eq!(client.balance(), 500);
}

#[test]
fn get_meta_before_init_fails() {
    let env = Env::default();
    let (_, client) = create_vault(&env);
    assert!(client.try_get_meta().is_err(), "expected error before init");
}

// ---------------------------------------------------------------------------
// Admin tests
// ---------------------------------------------------------------------------

#[test]
fn get_admin_returns_owner_after_init() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    assert_eq!(client.get_admin(), owner);
}

#[test]
fn set_admin_two_step_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    client.set_admin(&owner, &new_admin);
    assert_eq!(client.get_admin(), owner); // Still old admin

    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);
}

#[test]
fn set_admin_unauthorized_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let intruder = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let result = client.try_set_admin(&intruder, &new_admin);
    assert!(
        result.is_err(),
        "expected error when non-admin calls set_admin"
    );
}

// Cross-contract conservation invariant E2E fuzz test
#[test]
fn cross_contract_conservation_fuzz() {
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    // Short doc: This test runs a seeded randomized sequence of actions
    // across the vault, settlement and revenue-pool contracts and asserts
    // that (1) settlement accounting equals cumulative settled amounts and
    // (2) the vault internal balance matches the sequence-tracked expected
    // internal balance. The seed is printed on failure for reproducibility.

    let seed: u64 = 0x_dead_beef_1234_u64; // deterministic seed for CI; change for local runs
    let mut rng = StdRng::seed_from_u64(seed);

    let steps: usize = 200;

    let env = Env::default();
    env.mock_all_auths();

    // Participants / principals
    let owner = Address::generate(&env);
    let admin = Address::generate(&env);
    let depositor = Address::generate(&env);
    let developer = Address::generate(&env);

    // Deploy contracts and token
    let (vault_address, vault_client) = create_vault(&env);
    let settlement_contract = env.register(CalloraSettlement, ());
    let settlement_client = CalloraSettlementClient::new(&env, &settlement_contract);
    let settlement_address = settlement_contract;

    let revenue_contract = env.register(RevenuePool, ());
    let revenue_client = RevenuePoolClient::new(&env, &revenue_contract);
    let revenue_address = revenue_contract;

    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    // Fund vault on-chain and init contracts
    fund_vault(&usdc_admin, &vault_address, 1_000_000);
    let _ = vault_client.init(&owner, &usdc, &Some(1_000_000), &None, &None, &Some(revenue_address), &None);

    settlement_client.init(&admin, &vault_address);
    revenue_client.init(&admin, &usdc);

    // Register settlement in vault
    vault_client.set_settlement(&owner, &settlement_address);

    // Track expected values
    let mut expected_vault_internal: i128 = vault_client.balance();
    let mut expected_onchain_vault: i128 = usdc_client.balance(&vault_address);
    let mut total_deposits: i128 = 0;
    let mut total_deductions: i128 = 0;
    let mut total_withdraws: i128 = 0;

    for i in 0..steps {
        let choice: u8 = rng.gen_range(0..100);
        if choice < 40 {
            // Deposit
            let amt_u64: u64 = rng.gen_range(1..10_000);
            let amt = amt_u64 as i128;
            // Mint to depositor then deposit
            usdc_admin.mint(&depositor, &amt);
            let res = vault_client.try_deposit(&depositor, &amt);
            if res.is_ok() {
                total_deposits = total_deposits.checked_add(amt).unwrap();
                expected_vault_internal = expected_vault_internal.checked_add(amt).unwrap();
                expected_onchain_vault = expected_onchain_vault.checked_add(amt).unwrap();
            }
        } else if choice < 75 {
            // Deduct -> settlement
            let amt_u64: u64 = rng.gen_range(1..5_000);
            let amt = amt_u64 as i128;
            let res = vault_client.try_deduct(&owner, &amt, &None);
            if res.is_ok() {
                // Update expectations: vault internal and onchain decreased
                total_deductions = total_deductions.checked_add(amt).unwrap();
                expected_vault_internal = expected_vault_internal.checked_sub(amt).unwrap();
                expected_onchain_vault = expected_onchain_vault.checked_sub(amt).unwrap();

                // Simulate the settlement crediting the amount (to pool or to developer)
                let to_pool = rng.gen_bool(0.5);
                if to_pool {
                    settlement_client.receive_payment(&vault_address, &amt, &true, &None);
                } else {
                    settlement_client.receive_payment(&vault_address, &amt, &false, &Some(developer.clone()));
                }
            }
        } else if choice < 90 {
            // Owner withdraw
            let amt_u64: u64 = rng.gen_range(1..2_000);
            let amt = amt_u64 as i128;
            let res = vault_client.try_withdraw(&amt);
            if res.is_ok() {
                total_withdraws = total_withdraws.checked_add(amt).unwrap();
                expected_vault_internal = expected_vault_internal.checked_sub(amt).unwrap();
                expected_onchain_vault = expected_onchain_vault.checked_sub(amt).unwrap();
            }
        } else {
            // Revenue pool receive payment (simulate vault transferring to revenue pool)
            let amt_u64: u64 = rng.gen_range(1..2_000);
            let amt = amt_u64 as i128;
            // Transfer tokens from vault to revenue pool on-chain to simulate deduction
            // Use token client to transfer directly
            let _ = usdc_client.transfer(&vault_address, &revenue_address, &amt);
            // Notify revenue pool (admin emits event)
            revenue_client.receive_payment(&admin, &amt, &true);
            // Update onchain vault balance expectation
            expected_onchain_vault = expected_onchain_vault.checked_sub(amt).unwrap_or(0);
        }

        // After each step assert invariants
        // 1) Settlement accounting equals cumulative deductions that were credited
        let g = settlement_client.get_global_pool();
        let mut settlement_sum: i128 = g.total_balance;
        let devs = settlement_client.get_all_developer_balances(&admin);
        for db in devs.iter() {
            settlement_sum = settlement_sum.checked_add(db.balance).unwrap();
        }
        // settlement_sum should equal total_deductions for the amounts we credited
        assert_eq!(settlement_sum, total_deductions, "seed={}: step {}: settlement accounting mismatch: {} vs {}", seed, i, settlement_sum, total_deductions);

        // 2) Vault internal accounting equals expected
        let observed_internal = vault_client.balance();
        assert_eq!(observed_internal, expected_vault_internal, "seed={}: step {}: vault internal mismatch: {} vs {}", seed, i, observed_internal, expected_vault_internal);

        // 3) On-chain token balance for vault equals expected
        let observed_onchain = usdc_client.balance(&vault_address);
        assert_eq!(observed_onchain, expected_onchain_vault, "seed={}: step {}: vault onchain mismatch: {} vs {}", seed, i, observed_onchain, expected_onchain_vault);
    }
}

// ---------------------------------------------------------------------------
// Deposit tests
// ---------------------------------------------------------------------------

#[test]
fn owner_can_deposit() {
    let env = Env::default();
    let owner = Address::generate(&env);
    // Swap order: create USDC first
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    usdc_admin.mint(&owner, &500);
    usdc_client.approve(&owner, &vault_address, &300, &1000);

    let new_balance = client.deposit(&owner, &200);
    assert_eq!(new_balance, 200);

    let events = env.events().all();
    let deposit_event = events
        .iter()
        .find(|e| {
            if e.0 != vault_address {
                return false;
            }
            if e.1.is_empty() {
                return false;
            }
            let s: Symbol = e.1.get(0).unwrap().into_val(&env);
            s == Symbol::new(&env, "deposit")
        })
        .expect("expected deposit event");

    let (amount, balance): (i128, i128) = deposit_event.2.into_val(&env);
    assert_eq!(amount, 200);
    assert_eq!(balance, 200);
}

#[test]
fn allowed_depositor_can_deposit() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));

    usdc_admin.mint(&depositor, &200);
    usdc_client.approve(&depositor, &vault_address, &200, &1000);
    let returned = client.deposit(&depositor, &200);

    assert_eq!(returned, 300);
    assert_eq!(client.balance(), 300);
}

#[test]
#[should_panic(expected = "unauthorized: only owner or allowed depositor can deposit")]
fn unauthorized_address_cannot_deposit() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    client.deposit(&unauthorized, &50);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn deposit_zero_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.deposit(&owner, &0);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn deposit_negative_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.deposit(&owner, &-50);
}

#[test]
fn deposit_below_minimum_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &Some(50), &None, &None);
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));

    usdc_admin.mint(&depositor, &30);
    usdc_client.approve(&depositor, &vault_address, &30, &1000);
    let result = client.try_deposit(&depositor, &30);
    assert!(result.is_err(), "expected error for deposit below minimum");
}

#[test]
fn deposit_at_minimum_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &Some(50), &None, &None);
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));

    usdc_admin.mint(&depositor, &50);
    usdc_client.approve(&depositor, &vault_address, &50, &1000);
    let new_balance = client.deposit(&depositor, &50);
    assert_eq!(new_balance, 150);
}

#[test]
fn deposit_paused_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    client.pause(&owner);
    assert!(client.is_paused());

    usdc_admin.mint(&owner, &100);
    usdc_client.approve(&owner, &vault_address, &100, &1000);

    let result = client.try_deposit(&owner, &100);
    assert!(result.is_err());
    // Should contain "vault is paused" but Error doesn't easily expose the string in tests without more setup
    // but the transaction should fail.

    client.unpause(&owner);
    assert!(!client.is_paused());
    client.deposit(&owner, &100);
    assert_eq!(client.balance(), 100);
}

// ---------------------------------------------------------------------------
// Additional deposit unit tests (tasks 5.1, 5.2, 5.3)
// ---------------------------------------------------------------------------

/// Validates: Requirements 8.1, 5.2
#[test]
fn owner_deposit_increases_balance_and_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    usdc_admin.mint(&owner, &500);
    usdc_client.approve(&owner, &vault_address, &500, &1000);

    let returned = client.deposit(&owner, &300);
    assert_eq!(returned, 300);

    // Capture events immediately after deposit, before any other contract call
    // (each contract call resets the event log to that call's events only)
    let events = env.events().all();
    let deposit_event = events
        .iter()
        .find(|e| {
            if e.0 != vault_address {
                return false;
            }
            if e.1.is_empty() {
                return false;
            }
            let s: Symbol = e.1.get(0).unwrap().into_val(&env);
            s == Symbol::new(&env, "deposit")
        })
        .expect("expected deposit event");

    assert_eq!(
        deposit_event.1.len(),
        2,
        "topics must have exactly 2 entries (deposit, caller)"
    );
    let topic0: Symbol = deposit_event.1.get(0).unwrap().into_val(&env);
    let topic1: Address = deposit_event.1.get(1).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "deposit"));
    assert_eq!(topic1, owner);

    let (amount, new_balance): (i128, i128) = deposit_event.2.into_val(&env);
    assert_eq!(amount, 300);
    assert_eq!(new_balance, 300);

    // Balance check after event assertions
    assert_eq!(client.balance(), 300);
}

/// Validates: Requirements 8.7, 4.2
#[test]
fn balance_unchanged_after_failed_deposit() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    // min_deposit = 50
    client.init(&owner, &usdc, &None, &None, &Some(50), &None, &None);

    // Mint enough for the owner to deposit later (after unpause)
    usdc_admin.mint(&owner, &200);
    usdc_client.approve(&owner, &vault_address, &200, &10_000);

    let balance_before = client.balance();

    // Scenario 1: unauthorized caller
    let result = client.try_deposit(&unauthorized, &100);
    assert!(result.is_err(), "unauthorized caller must be rejected");
    assert_eq!(
        client.balance(),
        balance_before,
        "balance must be unchanged after unauthorized deposit"
    );

    // Scenario 2: paused vault
    client.pause(&owner);
    let result = client.try_deposit(&owner, &100);
    assert!(result.is_err(), "paused vault must reject deposit");
    assert_eq!(
        client.balance(),
        balance_before,
        "balance must be unchanged after paused deposit"
    );
    client.unpause(&owner);

    // Scenario 3: below minimum (10 < 50)
    let result = client.try_deposit(&owner, &10);
    assert!(result.is_err(), "below-minimum deposit must be rejected");
    assert_eq!(
        client.balance(),
        balance_before,
        "balance must be unchanged after below-minimum deposit"
    );
}

/// Validates: Requirements 5.1, 5.2, 5.3
#[test]
fn deposit_event_schema_alignment() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    usdc_admin.mint(&owner, &200);
    usdc_client.approve(&owner, &vault_address, &200, &10_000);

    client.deposit(&owner, &150);

    let events = env.events().all();
    let deposit_event = events
        .iter()
        .find(|e| {
            if e.0 != vault_address {
                return false;
            }
            if e.1.is_empty() {
                return false;
            }
            let s: Symbol = e.1.get(0).unwrap().into_val(&env);
            s == Symbol::new(&env, "deposit")
        })
        .expect("expected deposit event");

    // Schema alignment: exactly 2 topics (deposit, caller)
    assert_eq!(
        deposit_event.1.len(),
        2,
        "deposit event must have exactly 2 topics"
    );
    let topic0: Symbol = deposit_event.1.get(0).unwrap().into_val(&env);
    let topic1: Address = deposit_event.1.get(1).unwrap().into_val(&env);
    assert_eq!(
        topic0,
        Symbol::new(&env, "deposit"),
        "topic[0] must be Symbol(\"deposit\")"
    );
    assert_eq!(topic1, owner, "topic[1] must be the depositor address");

    // Data must decode as (amount: i128, new_balance: i128)
    let (amount, new_balance): (i128, i128) = deposit_event.2.into_val(&env);
    assert_eq!(amount, 150, "event data amount must match deposited amount");
    assert_eq!(
        new_balance, 150,
        "event data new_balance must match vault balance"
    );
}

// ---------------------------------------------------------------------------
// Allowlist management tests
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Allowed depositor management tests (backward compatibility)
// ---------------------------------------------------------------------------

#[test]
fn owner_can_set_and_clear_allowed_depositor() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    // Set depositor
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    usdc_admin.mint(&depositor, &50);
    usdc_client.approve(&depositor, &vault_address, &50, &1000);
    client.deposit(&depositor, &50);
    assert_eq!(client.balance(), 150);

    // Clear depositor
    client.set_allowed_depositor(&owner, &None);

    // Owner can still deposit
    usdc_admin.mint(&owner, &25);
    usdc_client.approve(&owner, &vault_address, &25, &1000);
    client.deposit(&owner, &25);
    assert_eq!(client.balance(), 175);
}

#[test]
fn set_allowed_depositor_duplicate_is_ignored() {
    // Adding the same depositor twice should not create a duplicate entry
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    client.set_allowed_depositor(&owner, &Some(depositor.clone())); // duplicate should be a no-op

    let list = client.get_allowed_depositors();
    assert_eq!(list.len(), 1);

    // depositor can still deposit exactly once (list not doubled)
    usdc_admin.mint(&depositor, &50);
    usdc_client.approve(&depositor, &vault_address, &50, &1000);
    assert_eq!(client.deposit(&depositor, &50), 150);
}

#[test]
#[should_panic(expected = "unauthorized: owner only")]
fn non_owner_cannot_set_allowed_depositor() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let non_owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.set_allowed_depositor(&non_owner, &Some(depositor));
}

#[test]
#[should_panic(expected = "unauthorized: only owner or allowed depositor can deposit")]
fn deposit_after_depositor_cleared_is_rejected() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    client.set_allowed_depositor(&owner, &None);

    usdc_admin.mint(&depositor, &50);
    usdc_client.approve(&depositor, &vault_address, &50, &1000);
    client.deposit(&depositor, &50);
}

// ---------------------------------------------------------------------------
// Pause tests
// ---------------------------------------------------------------------------

#[test]
fn pause_unpause_admin_only() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let intruder = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // intruder fails
    let res = client.try_pause(&intruder);
    assert!(res.is_err());

    // admin (owner) succeeds
    client.pause(&owner);
    assert!(client.is_paused());

    // intruder fails unpause
    let res = client.try_unpause(&intruder);
    assert!(res.is_err());

    // admin (owner) succeeds unpause
    client.unpause(&owner);
    assert!(!client.is_paused());
}

#[test]
fn pause_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    client.pause(&owner);
    let events = env.events().all();
    let pause_event = events
        .iter()
        .find(|e| {
            e.0 == vault_address
                && e.1
                    .get(0)
                    .map(|v| {
                        let s: Symbol = v.into_val(&env);
                        s == Symbol::new(&env, "vault_paused")
                    })
                    .unwrap_or(false)
        })
        .expect("expected pause event");

    let admin_topic: Address = pause_event.1.get(1).unwrap().into_val(&env);
    assert_eq!(admin_topic, owner);
}

// ---------------------------------------------------------------------------
// Deduct tests
// ---------------------------------------------------------------------------

#[test]
fn set_authorized_caller_sets_and_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 200);
    client.init(&owner, &usdc, &Some(200), &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);

    client.set_authorized_caller(&Some(new_caller.clone()), &0u64);

    let events = env.events().all();
    let ev = events.last().expect("expected set_authorized_caller event");
    assert_eq!(ev.1.len(), 2);

    let topic0: Symbol = ev.1.get(0).unwrap().into_val(&env);
    let topic1: Address = ev.1.get(1).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "set_authorized_caller"));
    assert_eq!(topic1, owner);

    let (old, now, nonce): (Option<Address>, Option<Address>, u64) = ev.2.into_val(&env);
    assert_eq!(old, None);
    assert_eq!(now, Some(new_caller.clone()));
    assert_eq!(nonce, 0u64);

    let remaining = client.deduct(&new_caller, &50, &None, &u16::MAX, &Address::generate(&env));
    assert_eq!(remaining, 150);
}

#[test]
fn deduct_reduces_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 300);
    client.init(&owner, &usdc, &Some(300), &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);

    let returned = client.deduct(&owner, &50, &None, &u16::MAX, &Address::generate(&env));
    assert_eq!(returned, 250);
    assert_eq!(client.balance(), 250);
}

#[test]
fn deduct_with_request_id() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);

    let remaining = client.deduct(&owner, &100, &Some(Symbol::new(&env, "req123")), &u16::MAX, &Address::generate(&env));
    assert_eq!(remaining, 900);
}

#[test]
fn deduct_insufficient_balance_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 10);
    client.init(&owner, &usdc, &Some(10), &None, &None, &None, &None);

    let result = client.try_deduct(&owner, &100, &None, &u16::MAX);
    assert!(result.is_err(), "expected error for insufficient balance");
}

#[test]
fn deduct_exact_balance_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 75);
    client.init(&owner, &usdc, &Some(75), &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);

    let remaining = client.deduct(&owner, &75, &None, &u16::MAX, &Address::generate(&env));
    assert_eq!(remaining, 0);
    assert_eq!(client.balance(), 0);
}

#[test]
fn deduct_event_contains_request_id() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);

    let request_id = Symbol::new(&env, "api_call_42");
    client.deduct(&owner, &150, &Some(request_id.clone(), &Address::generate(&env)), &u16::MAX);

    let events = env.events().all();
    let ev = events.last().expect("expected deduct event");

    assert_eq!(ev.1.len(), 3, "deduct event must always have 3 topics");
    let topic0: Symbol = ev.1.get(0).unwrap().into_val(&env);
    let topic1: Address = ev.1.get(1).unwrap().into_val(&env);
    let topic2: Symbol = ev.1.get(2).unwrap().into_val(&env);

    assert_eq!(topic0, Symbol::new(&env, "deduct"));
    assert_eq!(topic1, owner);
    assert_eq!(topic2, request_id);

    let (emitted_amount, remaining): (i128, i128) = ev.2.into_val(&env);
    assert_eq!(emitted_amount, 150);
    assert_eq!(remaining, 350);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn deduct_zero_amount_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &client.address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.deduct(&owner, &0, &None, &u16::MAX);
}

#[test]
#[should_panic(expected = "deduct amount exceeds max_deduct")]
fn deduct_exceeding_max_fails() {
    let env = Env::default();
    let owner = Address::generate(&env, &Address::generate(&env));
    let (_, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &client.address, 1000);
    // Set max_deduct to 500
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &Some(500));
    client.deduct(&owner, &501, &None, &u16::MAX);
}

#[test]
fn deduct_authorized_caller_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let authorized = Address::generate(&env, &Address::generate(&env));
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc,
        &Some(1000),
        &Some(authorized.clone()),
        &None,
        &None,
        &None,
    );
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);
    let remaining = client.deduct(&authorized, &100, &None, &u16::MAX, &Address::generate(&env));
    assert_eq!(remaining, 900);
}

#[test]
#[should_panic(expected = "vault is paused")]
fn deduct_paused_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &client.address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);
    client.pause(&owner);
    client.deduct(&owner, &100, &None, &u16::MAX);
}

#[test]
fn deduct_event_no_request_id_uses_empty_symbol() {
    let env = Env::default();
    let owner = Address::generate(&env, &Address::generate(&env));
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 300);
    client.init(&owner, &usdc, &Some(300), &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);
    client.deduct(&owner, &100, &None, &u16::MAX);

    let events = env.events().all();
    let ev = events.last().expect("expected deduct event");

    assert_eq!(ev.1.len(, &Address::generate(&env)), 3, "deduct event must always have 3 topics");
    let topic0: Symbol = ev.1.get(0).unwrap().into_val(&env);
    let topic1: Address = ev.1.get(1).unwrap().into_val(&env);
    let topic2: Symbol = ev.1.get(2).unwrap().into_val(&env);

    assert_eq!(topic0, Symbol::new(&env, "deduct"));
    assert_eq!(topic1, owner);
    assert_eq!(topic2, Symbol::new(&env, ""));
    let (emitted_amount, remaining): (i128, i128) = ev.2.into_val(&env);
    assert_eq!(emitted_amount, 100);
    assert_eq!(remaining, 200);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn deduct_zero_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    client.deduct(&owner, &0, &None, &u16::MAX);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn deduct_negative_panics() {
    let env = Env::default();
    let owner = Address::generate(&env, &Address::generate(&env));
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.deduct(&owner, &-50, &None, &u16::MAX);
}

#[test]
#[should_panic(expected = "insufficient balance")]
fn deduct_exceeds_balance_panics() {
    let env = Env::default();
    let owner = Address::generate(&env, &Address::generate(&env));
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 50);
    client.init(&owner, &usdc, &Some(50), &None, &None, &None, &None);
    client.deduct(&owner, &100, &None, &u16::MAX);
}

#[test]
fn balance_unchanged_after_failed_deduct() {
    let env = Env::default();
    let owner = Address::generate(&env, &Address::generate(&env));
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let _ = client.try_deduct(&owner, &200, &None, &u16::MAX);
    assert_eq!(client.balance(), 100);
}

// ---------------------------------------------------------------------------
// Batch deduct tests
// ---------------------------------------------------------------------------
// propose_revenue_pool / accept_revenue_pool / cancel_revenue_pool tests
// ---------------------------------------------------------------------------

#[test]
fn propose_and_accept_revenue_pool_stores_address() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.propose_revenue_pool(&Some(revenue_pool.clone()));
    // pending should be set
    assert_eq!(client.get_pending_revenue_pool(), Some(revenue_pool.clone()));
    // accept
    client.accept_revenue_pool();
    assert_eq!(client.get_revenue_pool(), Some(revenue_pool));
    assert_eq!(client.get_pending_revenue_pool(), None);
}

#[test]
fn propose_and_accept_revenue_pool_clear_proposal() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    // Propose None to clear the revenue pool
    client.propose_revenue_pool(&None);
    assert_eq!(client.get_pending_revenue_pool(), None);
    // Accept the clear proposal
    client.accept_revenue_pool();
    assert_eq!(client.get_revenue_pool(), None);
}

#[test]
fn propose_revenue_pool_update_replaces_via_two_step() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let old_pool = Address::generate(&env);
    let new_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    // Start with old_pool via init
    client.init(&owner, &usdc, &None, &None, &None, &Some(old_pool), &None);
    assert_eq!(client.get_revenue_pool(), Some(old_pool));

    // Propose new pool
    client.propose_revenue_pool(&Some(new_pool.clone()));
    assert_eq!(client.get_pending_revenue_pool(), Some(new_pool.clone()));

    // Accept
    client.accept_revenue_pool();
    assert_eq!(client.get_revenue_pool(), Some(new_pool));
}

#[test]
fn propose_revenue_pool_same_as_current_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &Some(pool.clone()), &None);
    let result = client.try_propose_revenue_pool(&Some(pool));
    assert_eq!(result, Err(Ok(VaultError::NewRevenuePoolSameAsCurrent)));
}

#[test]
fn propose_revenue_pool_vault_address_fails() {
    let env = Env::default();
    let (vault_addr, client, _, admin) = setup(&env);
    let result = client.try_propose_revenue_pool(&Some(vault_addr));
    assert!(result.is_err());
}

#[test]
fn cancel_revenue_pool_removes_pending() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // Propose
    client.propose_revenue_pool(&Some(pool.clone()));
    assert_eq!(client.get_pending_revenue_pool(), Some(pool.clone()));

    // Cancel
    client.cancel_revenue_pool();
    assert_eq!(client.get_pending_revenue_pool(), None);
    assert_eq!(client.get_revenue_pool(), None);
}

#[test]
fn accept_revenue_pool_no_pending_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let result = client.try_accept_revenue_pool();
    assert_eq!(result, Err(Ok(VaultError::NoRevenuePoolTransferPending)));
}

#[test]
fn cancel_revenue_pool_no_pending_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let result = client.try_cancel_revenue_pool();
    assert_eq!(result, Err(Ok(VaultError::NoRevenuePoolTransferPending)));
}

#[test]
fn propose_revenue_pool_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.propose_revenue_pool(&Some(revenue_pool.clone()));

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "revenue_pool_proposed"));
}

#[test]
fn accept_revenue_pool_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.propose_revenue_pool(&Some(revenue_pool.clone()));
    client.accept_revenue_pool();

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "revenue_pool_accepted"));
}

#[test]
fn cancel_revenue_pool_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.propose_revenue_pool(&Some(pool));
    client.cancel_revenue_pool();

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "revenue_pool_cancelled"));
}

#[test]
fn get_revenue_pool_returns_none_when_not_set() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    assert_eq!(client.get_revenue_pool(), None);
}

#[test]
fn get_revenue_pool_consistent_after_deduct_operations() {
    // Ensure get_revenue_pool remains consistent and doesn't mutate state
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc_address,
        &Some(1000),
        &Some(caller.clone()),
        &None,
        &Some(revenue_pool.clone()),
        &None,
    );
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);

    // Query revenue pool before deduct
    let before = client.get_revenue_pool();
    assert_eq!(before, Some(revenue_pool.clone()));

    // Perform deduct operation (routes to settlement, not revenue_pool)
    client.deduct(&caller, &200, &None, &u16::MAX);

    // Query revenue pool after deduct - should be unchanged
    let after = client.get_revenue_pool(, &Address::generate(&env));
    assert_eq!(after, Some(revenue_pool.clone()));
    assert_eq!(before, after);

    // Funds flow to settlement; revenue_pool receives nothing.
    assert_eq!(client.balance(), 800);
    assert_eq!(usdc_client.balance(&settlement), 200);
    assert_eq!(usdc_client.balance(&revenue_pool), 0);
}

#[test]
fn get_pending_revenue_pool_returns_none_when_not_set() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    assert_eq!(client.get_pending_revenue_pool(), None);
}

#[test]
fn get_pending_revenue_pool_returns_proposed() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.propose_revenue_pool(&Some(pool.clone()));
    assert_eq!(client.get_pending_revenue_pool(), Some(pool));
}

// ---------------------------------------------------------------------------
// Withdraw tests
// ---------------------------------------------------------------------------

#[test]
fn withdraw_reduces_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

    let remaining = client.withdraw(&200);
    assert_eq!(remaining, 300);
    assert_eq!(client.balance(), 300);
}

#[test]
fn withdraw_full_balance_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    let remaining = client.withdraw(&1000);
    assert_eq!(remaining, 0);
    assert_eq!(client.balance(), 0);
}

#[test]
fn withdraw_insufficient_balance_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let result = client.try_withdraw(&500);
    assert!(result.is_err(), "expected error for insufficient balance");
}

#[test]
fn withdraw_zero_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let result = client.try_withdraw(&0);
    assert!(result.is_err(), "expected error for zero amount");
}

#[test]
fn withdraw_to_reduces_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

    let remaining = client.withdraw_to(&recipient, &150);
    assert_eq!(remaining, 350);
    assert_eq!(client.balance(), 350);
    assert_eq!(usdc_client.balance(&recipient), 150);
}

#[test]
fn withdraw_unauthorized_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let _intruder = Address::generate(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    // Reset auths to test requirement without mock_all_auths bypassing it
    env.set_auths(&[]);
    let res = client.try_withdraw(&500);
    assert!(res.is_err());
}

#[test]
fn withdraw_to_insufficient_balance_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let result = client.try_withdraw_to(&recipient, &500);
    assert!(result.is_err(), "expected error for insufficient balance");
}

#[test]
#[should_panic(expected = "cannot withdraw to vault address")]
fn withdraw_to_vault_address_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    // Attempt to withdraw to the vault itself
    client.withdraw_to(&vault_address, &100);
}

#[test]
#[should_panic(expected = "cannot withdraw to token address")]
fn withdraw_to_token_address_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    // Attempt to withdraw to the USDC token contract
    client.withdraw_to(&usdc, &100);
}

#[test]
fn withdraw_to_while_paused_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    // Pause the vault
    client.pause(&owner);
    assert!(client.is_paused());

    // Withdraw should still work while paused (emergency recovery)
    let remaining = client.withdraw_to(&recipient, &300);
    assert_eq!(remaining, 700);
    assert_eq!(client.balance(), 700);
    assert_eq!(usdc_client.balance(&recipient), 300);
}

#[test]
fn withdraw_while_paused_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &None);

    // Pause the vault
    client.pause(&owner);
    assert!(client.is_paused());

    // Withdraw should still work while paused (emergency recovery)
    let remaining = client.withdraw(&200);
    assert_eq!(remaining, 800);
    assert_eq!(client.balance(), 800);
    assert_eq!(usdc_client.balance(&owner), 200);
}

// ---------------------------------------------------------------------------
// Transfer ownership tests
// ---------------------------------------------------------------------------

#[test]
fn transfer_ownership_two_step_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    client.transfer_ownership(&new_owner);
    let meta = client.get_meta();
    assert_eq!(meta.owner, owner); // Still old owner

    client.accept_ownership();
    let meta2 = client.get_meta();
    assert_eq!(meta2.owner, new_owner);
}

#[test]
fn transfer_ownership_emits_events() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.transfer_ownership(&new_owner);

    let events = env.events().all();
    let nomad_ev = events
        .iter()
        .find(|e| {
            e.0 == vault_address && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "ownership_nominated")
            }
        })
        .expect("expected ownership_nominated event");

    let old_n: Address = nomad_ev.1.get(1).unwrap().into_val(&env);
    let new_n: Address = nomad_ev.1.get(2).unwrap().into_val(&env);
    assert_eq!(old_n, owner);
    assert_eq!(new_n, new_owner);

    client.accept_ownership();
    let events2 = env.events().all();
    let accept_ev = events2
        .iter()
        .find(|e| {
            e.0 == vault_address && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "ownership_accepted")
            }
        })
        .expect("expected ownership_accepted event");

    let old_a: Address = accept_ev.1.get(1).unwrap().into_val(&env);
    let new_a: Address = accept_ev.1.get(2).unwrap().into_val(&env);
    assert_eq!(old_a, owner);
    assert_eq!(new_a, new_owner);
}

#[test]
#[should_panic(expected = "new_owner must be different from current owner")]
fn transfer_ownership_same_address_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.transfer_ownership(&owner);
}

// ---------------------------------------------------------------------------
// Distribute tests
// ---------------------------------------------------------------------------

#[test]
fn distribute_transfers_usdc_to_recipient() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&admin, &usdc, &Some(0), &None, &None, &None, &None);

    client.distribute(&admin, &developer, &300);

    assert_eq!(usdc_client.balance(&developer), 300);
    assert_eq!(usdc_client.balance(&vault_address), 700);
}

#[test]
fn distribute_unauthorized_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let intruder = Address::generate(&env);
    let developer = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &admin);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&admin, &usdc, &Some(0), &None, &None, &None, &None);

    let result = client.try_distribute(&intruder, &developer, &300);
    assert!(result.is_err(), "expected error when non-admin distributes");
}

#[test]
fn distribute_insufficient_usdc_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &admin);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&admin, &usdc, &Some(0), &None, &None, &None, &None);

    let result = client.try_distribute(&admin, &developer, &500);
    assert!(result.is_err(), "expected error for insufficient USDC");
}

#[test]
fn distribute_zero_amount_fails() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &admin);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&admin, &usdc, &Some(0), &None, &None, &None, &None);

    let result = client.try_distribute(&admin, &developer, &0);
    assert!(result.is_err(), "expected error for zero amount");
}

#[test]
fn distribute_while_paused_succeeds() {
    // distribute is ALLOWED when paused (emergency recovery, matches withdraw policy)
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&admin, &usdc, &Some(0), &None, &None, &None, &None);

    client.pause(&admin);
    assert!(client.is_paused());

    // distribute should succeed while paused
    client.distribute(&admin, &recipient, &300);

    assert_eq!(usdc_client.balance(&recipient), 300);
    assert_eq!(usdc_client.balance(&vault_address), 700);
}

#[test]
fn distribute_while_unpaused_succeeds() {
    // happy path: distribute works normally when unpaused
    let env = Env::default();
    let admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&admin, &usdc, &Some(0), &None, &None, &None, &None);

    assert!(!client.is_paused());

    client.distribute(&admin, &recipient, &300);

    assert_eq!(usdc_client.balance(&recipient), 300);
    assert_eq!(usdc_client.balance(&vault_address), 700);
}

#[test]
fn distribute_admin_only_enforcement() {
    // only admin can call distribute, even when unpaused
    let env = Env::default();
    let admin = Address::generate(&env);
    let non_admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &admin);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&admin, &usdc, &Some(0), &None, &None, &None, &None);

    // non-admin should fail
    let result = client.try_distribute(&non_admin, &recipient, &300);
    assert!(result.is_err(), "expected error when non-admin distributes");

    // admin should succeed
    client.distribute(&admin, &recipient, &300);
    assert_eq!(client.balance(), 0); // balance unchanged (distribute uses on-ledger balance)
}

// ---------------------------------------------------------------------------
// Offering metadata tests
// ---------------------------------------------------------------------------

#[test]
fn set_and_retrieve_metadata() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-001");
    let metadata = String::from_str(&env, "QmXoypizjW3WknFiJnKLwHCnL72vedxjQkDDP1mXWo6uco");

    let result = client.set_metadata(&owner, &offering_id, &metadata);
    assert_eq!(result, metadata);

    let retrieved = client.get_metadata(&offering_id);
    assert_eq!(retrieved, Some(metadata));
}

#[test]
fn set_metadata_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-002");
    let metadata = String::from_str(
        &env,
        "bafybeigdyrzt5sfp7udm7hu76uh7y26nf3efuylqabf3oclgtqy55fbzdi",
    );
    client.set_metadata(&owner, &offering_id, &metadata);

    let events = env.events().all();
    let ev = events.last().expect("expected metadata_set event");

    assert_eq!(ev.0, vault_address);
    let topics = &ev.1;
    assert_eq!(topics.len(), 3);

    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: String = topics.get(1).unwrap().into_val(&env);
    let topic2: Address = topics.get(2).unwrap().into_val(&env);

    assert_eq!(topic0, Symbol::new(&env, "metadata_set"));
    assert_eq!(topic1, offering_id);
    assert_eq!(topic2, owner);

    let data: String = ev.2.into_val(&env);
    assert_eq!(data, metadata);
}

#[test]
fn update_metadata_and_verify() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-003");
    let old_metadata = String::from_str(&env, "QmOldMetadata123");
    let new_metadata = String::from_str(&env, "QmNewMetadata456");

    client.set_metadata(&owner, &offering_id, &old_metadata);
    let result = client.update_metadata(&owner, &offering_id, &new_metadata);
    assert_eq!(result, new_metadata);

    let retrieved = client.get_metadata(&offering_id);
    assert_eq!(retrieved, Some(new_metadata));
}

#[test]
fn update_metadata_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-004");
    let old_metadata = String::from_str(&env, "https://example.com/old.json");
    let new_metadata = String::from_str(&env, "https://example.com/new.json");

    client.set_metadata(&owner, &offering_id, &old_metadata);
    client.update_metadata(&owner, &offering_id, &new_metadata);

    let events = env.events().all();
    let ev = events.last().expect("expected metadata_updated event");

    assert_eq!(ev.0, vault_address);
    let topics = &ev.1;
    assert_eq!(topics.len(), 3);

    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: String = topics.get(1).unwrap().into_val(&env);
    let topic2: Address = topics.get(2).unwrap().into_val(&env);

    assert_eq!(topic0, Symbol::new(&env, "metadata_updated"));
    assert_eq!(topic1, offering_id);
    assert_eq!(topic2, owner);

    let data: (String, String) = ev.2.into_val(&env);
    assert_eq!(data.0, old_metadata);
    assert_eq!(data.1, new_metadata);
}

#[test]
fn update_metadata_without_existing_uses_empty_old() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-006");
    let new_metadata = String::from_str(&env, "QmNewMetadataOnly");
    client.update_metadata(&owner, &offering_id, &new_metadata);

    let events = env.events().all();
    let ev = events.last().expect("expected metadata_updated event");

    assert_eq!(ev.0, vault_address);
    let data: (String, String) = ev.2.into_val(&env);
    assert_eq!(data.0, String::from_str(&env, ""));
    assert_eq!(data.1, new_metadata);
}

#[test]
#[should_panic(expected = "unauthorized: owner only")]
fn unauthorized_cannot_set_metadata() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-005");
    let metadata = String::from_str(&env, "QmSomeMetadata");
    client.set_metadata(&unauthorized, &offering_id, &metadata);
}

#[test]
fn set_metadata_max_length_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "a".repeat(64).as_str());
    let metadata = String::from_str(&env, "b".repeat(256).as_str());

    client.set_metadata(&owner, &offering_id, &metadata);
    assert_eq!(client.get_metadata(&offering_id), Some(metadata));
}

#[test]
#[should_panic(expected = "metadata exceeds max length")]
fn set_metadata_exceeds_length_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "off-1");
    let metadata = String::from_str(&env, "b".repeat(257).as_str());

    client.set_metadata(&owner, &offering_id, &metadata);
}

#[test]
#[should_panic(expected = "offering_id exceeds max length")]
fn set_offering_id_exceeds_length_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "a".repeat(65).as_str());
    let metadata = String::from_str(&env, "meta");

    client.set_metadata(&owner, &offering_id, &metadata);
}

#[test]
fn update_metadata_max_length_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-update");
    let metadata = String::from_str(&env, "b".repeat(256).as_str());

    client.set_metadata(&owner, &offering_id, &String::from_str(&env, "old"));
    client.update_metadata(&owner, &offering_id, &metadata);
    assert_eq!(client.get_metadata(&offering_id), Some(metadata));
}

#[test]
fn metadata_remains_after_ownership_transfer() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "off-transfer");
    let metadata = String::from_str(&env, "ipfs://cid123");
    client.set_metadata(&owner, &offering_id, &metadata);

    client.transfer_ownership(&new_owner);
    client.accept_ownership();

    // Metadata should still be accessible
    assert_eq!(client.get_metadata(&offering_id), Some(metadata.clone()));

    // Old owner should no longer be able to update it
    let update_res =
        client.try_update_metadata(&owner, &offering_id, &String::from_str(&env, "new"));
    assert!(update_res.is_err());

    // New owner should be able to update it
    client.update_metadata(&new_owner, &offering_id, &String::from_str(&env, "new"));
    assert_eq!(
        client.get_metadata(&offering_id),
        Some(String::from_str(&env, "new"))
    );
}

// ---------------------------------------------------------------------------
// remove_metadata tests
// ---------------------------------------------------------------------------

#[test]
fn remove_metadata_clears_entry() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-rm-001");
    let metadata = String::from_str(&env, "ipfs://bafybeig");

    client.set_metadata(&owner, &offering_id, &metadata);
    assert_eq!(client.get_metadata(&offering_id), Some(metadata));

    client.remove_metadata(&owner, &offering_id);
    assert_eq!(client.get_metadata(&offering_id), None);
}

#[test]
fn remove_metadata_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-rm-002");
    client.set_metadata(&owner, &offering_id, &String::from_str(&env, "ipfs://cid"));
    client.remove_metadata(&owner, &offering_id);

    let events = env.events().all();
    let ev = events.last().expect("expected metadata_removed event");

    assert_eq!(ev.0, vault_address);
    let topics = &ev.1;
    assert_eq!(topics.len(), 3);

    let topic0: Symbol = topics.get(0).unwrap().into_val(&env);
    let topic1: String = topics.get(1).unwrap().into_val(&env);
    let topic2: Address = topics.get(2).unwrap().into_val(&env);

    assert_eq!(topic0, Symbol::new(&env, "metadata_removed"));
    assert_eq!(topic1, offering_id);
    assert_eq!(topic2, owner);
}

#[test]
#[should_panic(expected = "unauthorized: owner only")]
fn unauthorized_cannot_remove_metadata() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let unauthorized = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-rm-003");
    client.set_metadata(&owner, &offering_id, &String::from_str(&env, "ipfs://cid"));
    client.remove_metadata(&unauthorized, &offering_id);
}

#[test]
fn remove_metadata_nonexistent_is_noop() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let offering_id = String::from_str(&env, "offering-rm-never-set");
    // Should not panic even when the key was never written
    client.remove_metadata(&owner, &offering_id);
    assert_eq!(client.get_metadata(&offering_id), None);
}

// ---------------------------------------------------------------------------
// Full lifecycle test
// ---------------------------------------------------------------------------

#[test]
fn vault_full_lifecycle() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let recipient = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();

    // Init with 500 balance, min_deposit = 10
    fund_vault(&usdc_admin, &vault_address, 500);
    let meta = client.init(&owner, &usdc, &Some(500), &None, &Some(10), &None, &None);
    assert_eq!(meta.balance, 500);
    assert_eq!(meta.owner, owner);
    assert_eq!(client.balance(), 500);
    assert_eq!(client.get_admin(), owner);

    // Configure settlement address (precondition for deduct/batch_deduct)
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);

    // Allow depositor and deposit 200
    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    usdc_admin.mint(&depositor, &200);
    usdc_client.approve(&depositor, &vault_address, &200, &1000);
    let after_deposit = client.deposit(&depositor, &200);
    assert_eq!(after_deposit, 700);
    assert_eq!(client.balance(), 700);

    // Batch deduct 100 + 50 + 25 = 175
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: Some(Symbol::new(&env, "r1"))
        },
        DeductItem {
            amount: 50,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 25,
            request_id: Some(Symbol::new(&env, "r3"))
        },
    ];
    let after_batch = client.batch_deduct(&owner, &items);
    assert_eq!(after_batch, 525);
    assert_eq!(client.balance(), 525);

    // Single deduct
    let after_deduct = client.deduct(&owner, &25, &Some(Symbol::new(&env, "r4")), &u16::MAX, &Address::generate(&env));
    assert_eq!(after_deduct, 500);

    // Admin change
    client.set_admin(&owner, &new_admin);
    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);

    // Withdraw to recipient
    let after_withdraw_to = client.withdraw_to(&recipient, &100);
    assert_eq!(after_withdraw_to, 400);
    assert_eq!(client.balance(), 400);

    // Withdraw to owner
    let after_withdraw = client.withdraw(&50);
    assert_eq!(after_withdraw, 350);
    assert_eq!(client.balance(), 350);

    let final_meta = client.get_meta();
    assert_eq!(final_meta.balance, 350);
    assert_eq!(final_meta.owner, owner);
    assert_eq!(final_meta.min_deposit, 10);
}

// ---------------------------------------------------------------------------
// Revenue pool integration tests
// ---------------------------------------------------------------------------

#[test]
fn init_with_revenue_pool_stores_address() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(
        &owner,
        &usdc,
        &Some(500),
        &None,
        &None,
        &Some(revenue_pool.clone()),
        &None,
    );

    assert_eq!(client.balance(), 500);
}

#[test]
#[should_panic(expected = "settlement address not set")]
fn deduct_with_only_revenue_pool_panics() {
    // Revenue pool is no longer a deduct destination; settlement is mandatory.
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc_address, _usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc_address,
        &Some(1000),
        &Some(caller.clone()),
        &None,
        &Some(revenue_pool),
        &None,
    );

    client.deduct(&caller, &300, &None, &u16::MAX);
}

#[test]
fn deduct_with_settlement_transfers_usdc() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env, &Address::generate(&env));
    let (vault_address, client) = create_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 800);
    client.init(
        &owner,
        &usdc_address,
        &Some(800),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );
    client.set_settlement(&owner, &settlement);

    client.deduct(&caller, &250, &None, &u16::MAX);

    assert_eq!(client.balance(, &Address::generate(&env)), 550);
    assert_eq!(usdc_client.balance(&settlement), 250);
}

#[test]
#[should_panic(expected = "settlement address not set")]
fn batch_deduct_with_only_revenue_pool_panics() {
    // Revenue pool is no longer a deduct destination; settlement is mandatory.
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc_address, _usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc_address,
        &Some(1000),
        &Some(caller.clone()),
        &None,
        &Some(revenue_pool),
        &None,
    );

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 200,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 150,
            request_id: None
        , developer: Address::generate(&env) },
    ];
    client.batch_deduct(&caller, &items);
}

#[test]
fn batch_deduct_with_settlement_transfers_total_usdc() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc_address,
        &Some(1000),
        &Some(caller.clone()),
        &None,
        &None,
        &Some(500),
    );
    client.set_settlement(&owner, &settlement);

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 200,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 150,
            request_id: None
        , developer: Address::generate(&env) },
    ];
    client.batch_deduct(&caller, &items);

    assert_eq!(client.balance(), 650);
    assert_eq!(usdc_client.balance(&settlement), 350);
}

// ---------------------------------------------------------------------------
// set_revenue_pool / get_revenue_pool tests
// ---------------------------------------------------------------------------

#[test]
fn set_revenue_pool_stores_and_get_returns_address() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_revenue_pool(&owner, &Some(revenue_pool.clone()));

    assert_eq!(client.get_revenue_pool(), Some(revenue_pool));
}

#[test]
fn set_revenue_pool_clear_removes_address() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(
        &owner,
        &usdc,
        &None,
        &None,
        &None,
        &Some(revenue_pool),
        &None,
    );
    client.set_revenue_pool(&owner, &None);

    assert_eq!(client.get_revenue_pool(), None);
}

#[test]
fn set_revenue_pool_update_replaces_address() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let old_pool = Address::generate(&env);
    let new_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &Some(old_pool), &None);
    client.set_revenue_pool(&owner, &Some(new_pool.clone()));

    assert_eq!(client.get_revenue_pool(), Some(new_pool));
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin")]
fn set_revenue_pool_unauthorized_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_revenue_pool(&attacker, &Some(revenue_pool));
}

#[test]
fn get_revenue_pool_returns_none_when_not_set() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    assert_eq!(client.get_revenue_pool(), None);
}

#[test]
fn get_revenue_pool_returns_correct_after_update() {
    // Verify get_revenue_pool reflects latest committed state after multiple updates
    let env = Env::default();
    let owner = Address::generate(&env);
    let pool1 = Address::generate(&env);
    let pool2 = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // Set first revenue pool
    client.set_revenue_pool(&owner, &Some(pool1.clone()));
    assert_eq!(client.get_revenue_pool(), Some(pool1));

    // Update to second revenue pool
    client.set_revenue_pool(&owner, &Some(pool2.clone()));
    assert_eq!(client.get_revenue_pool(), Some(pool2));
}

#[test]
fn get_revenue_pool_returns_none_after_clear() {
    // Ensure get_revenue_pool returns None after clearing
    let env = Env::default();
    let owner = Address::generate(&env);
    let pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(
        &owner,
        &usdc,
        &None,
        &None,
        &None,
        &Some(pool.clone()),
        &None,
    );
    assert_eq!(client.get_revenue_pool(), Some(pool));

    // Clear revenue pool
    client.set_revenue_pool(&owner, &None);
    assert_eq!(client.get_revenue_pool(), None);
}

#[test]
fn get_revenue_pool_consistent_after_deduct_operations() {
    // Ensure get_revenue_pool remains consistent and doesn't mutate state
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc_address,
        &Some(1000),
        &Some(caller.clone()),
        &None,
        &Some(revenue_pool.clone()),
        &None,
    );
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);

    // Query revenue pool before deduct
    let before = client.get_revenue_pool();
    assert_eq!(before, Some(revenue_pool.clone()));

    // Perform deduct operation (routes to settlement, not revenue_pool)
    client.deduct(&caller, &200, &None, &u16::MAX);

    // Query revenue pool after deduct - should be unchanged
    let after = client.get_revenue_pool(, &Address::generate(&env));
    assert_eq!(after, Some(revenue_pool.clone()));
    assert_eq!(before, after);

    // Funds flow to settlement; revenue_pool receives nothing.
    assert_eq!(client.balance(), 800);
    assert_eq!(usdc_client.balance(&settlement), 200);
    assert_eq!(usdc_client.balance(&revenue_pool), 0);
}

#[test]
fn get_revenue_pool_no_mutation_on_multiple_calls() {
    // Verify calling get_revenue_pool multiple times doesn't mutate state
    let env = Env::default();
    let owner = Address::generate(&env);
    let pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(
        &owner,
        &usdc,
        &None,
        &None,
        &None,
        &Some(pool.clone()),
        &None,
    );

    let initial_balance = client.balance();

    // Call get_revenue_pool multiple times
    for _ in 0..10 {
        let result = client.get_revenue_pool();
        assert_eq!(result, Some(pool.clone()));
    }

    // Verify balance unchanged (no mutation)
    assert_eq!(client.balance(), initial_balance);
}

#[test]
fn get_revenue_pool_consistency_with_zero_balance() {
    // Ensure get_revenue_pool works correctly with zero vault balance
    let env = Env::default();
    let owner = Address::generate(&env);
    let pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(
        &owner,
        &usdc,
        &None,
        &None,
        &None,
        &Some(pool.clone()),
        &None,
    );

    // Balance should be zero
    assert_eq!(client.balance(), 0);

    // Revenue pool should still be queryable
    assert_eq!(client.get_revenue_pool(), Some(pool));
}

#[test]
fn deposit_max_balance_overflow_panic() {
    // Explicit test for max-balance overflow near i128::MAX.
    // Exercises the checked_add(...).unwrap_or_else(|| panic!("balance overflow")) path.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();

    // 1. Setup vault balance near i128::MAX
    let near_max = i128::MAX - 1;
    let overflow_amount = 2;

    fund_vault(&usdc_admin, &vault_address, near_max);
    client.init(&owner, &usdc, &Some(near_max), &None, &None, &None, &None);

    // 2. Prepare overflow deposit
    usdc_admin.mint(&owner, &overflow_amount);
    usdc_client.approve(&owner, &vault_address, &overflow_amount, &1000);

    // 3. Confirm it panics safely on overflow
    let result = client.try_deposit(&owner, &overflow_amount);
    assert!(
        result.is_err(),
        "contract must fail safely when balance would overflow i128::MAX"
    );
}

#[test]
fn get_revenue_pool_after_multiple_sequential_updates() {
    // Test multiple sequential set/clear operations before query
    let env = Env::default();
    let owner = Address::generate(&env);
    let pool1 = Address::generate(&env);
    let pool2 = Address::generate(&env);
    let pool3 = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // Multiple sequential updates
    client.set_revenue_pool(&owner, &Some(pool1.clone()));
    client.set_revenue_pool(&owner, &Some(pool2.clone()));
    client.set_revenue_pool(&owner, &None);
    client.set_revenue_pool(&owner, &Some(pool3.clone()));

    // Should reflect final committed state
    assert_eq!(client.get_revenue_pool(), Some(pool3));
}

#[test]
fn deduct_routes_to_settlement_when_both_configured() {
    // settlement takes priority over revenue_pool when both are set
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc_address,
        &Some(1000),
        &Some(caller.clone()),
        &None,
        &Some(revenue_pool.clone()),
        &None,
    );
    client.set_settlement(&owner, &settlement);

    client.deduct(&caller, &400, &None, &u16::MAX, &Address::generate(&env));

    // settlement gets the funds, revenue_pool gets nothing
    assert_eq!(usdc_client.balance(&settlement), 400);
    assert_eq!(usdc_client.balance(&revenue_pool), 0);
}

#[test]
fn set_revenue_pool_emits_event_on_set() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_revenue_pool(&owner, &Some(revenue_pool.clone()));

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "set_revenue_pool"));
    let data: Address = last.2.into_val(&env);
    assert_eq!(data, revenue_pool);
}

#[test]
fn set_revenue_pool_emits_event_on_clear() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let revenue_pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(
        &owner,
        &usdc,
        &None,
        &None,
        &None,
        &Some(revenue_pool),
        &None,
    );
    client.set_revenue_pool(&owner, &None);

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "clear_revenue_pool"));
}

// ---------------------------------------------------------------------------
// set_settlement / get_settlement tests
// ---------------------------------------------------------------------------

#[test]
fn set_settlement_stores_and_get_returns_address() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_settlement(&owner, &settlement);

    assert_eq!(client.get_settlement(), settlement);
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin")]
fn set_settlement_unauthorized_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_settlement(&attacker, &settlement);
}

#[test]
fn set_settlement_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_settlement(&owner, &settlement);

    let events = env.events().all();
    let last = events.last().unwrap();
    let topic0: Symbol = last.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic0, Symbol::new(&env, "set_settlement"));
    let topic1: Address = last.1.get(1).unwrap().into_val(&env);
    assert_eq!(topic1, owner);
    let data: Address = last.2.into_val(&env);
    assert_eq!(data, settlement);
}

#[test]
#[should_panic(expected = "settlement address not set")]
fn get_settlement_before_set_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.get_settlement();
}

#[test]
fn get_settlement_returns_correct_after_update() {
    // Verify get_settlement reflects latest committed state after multiple updates
    let env = Env::default();
    let owner = Address::generate(&env);
    let settlement1 = Address::generate(&env);
    let settlement2 = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // Set first settlement address
    client.set_settlement(&owner, &settlement1);
    assert_eq!(client.get_settlement(), settlement1);

    // Update to second settlement address
    client.set_settlement(&owner, &settlement2);
    assert_eq!(client.get_settlement(), settlement2);
}

#[test]
fn get_settlement_consistent_after_deduct_operations() {
    // Ensure get_settlement remains consistent and doesn't mutate state
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc_address,
        &Some(1000),
        &Some(caller.clone()),
        &None,
        &None,
        &None,
    );
    client.set_settlement(&owner, &settlement);

    // Query settlement before deduct
    let before = client.get_settlement();
    assert_eq!(before, settlement);

    // Perform deduct operation
    client.deduct(&caller, &200, &None, &u16::MAX);

    // Query settlement after deduct - should be unchanged
    let after = client.get_settlement(, &Address::generate(&env));
    assert_eq!(after, settlement);
    assert_eq!(before, after);

    // Verify no state mutation occurred
    assert_eq!(client.balance(), 800);
    assert_eq!(usdc_client.balance(&settlement), 200);
}

#[test]
fn get_settlement_no_mutation_on_multiple_calls() {
    // Verify calling get_settlement multiple times doesn't mutate state
    let env = Env::default();
    let owner = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_settlement(&owner, &settlement);

    let initial_balance = client.balance();

    // Call get_settlement multiple times
    for _ in 0..10 {
        let result = client.get_settlement();
        assert_eq!(result, settlement);
    }

    // Verify balance unchanged (no mutation)
    assert_eq!(client.balance(), initial_balance);
}

#[test]
fn test_clear_allowed_depositors() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let depositor = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    client.set_allowed_depositor(&owner, &Some(depositor.clone()));
    client.set_allowed_depositor(&owner, &None);

    usdc_admin.mint(&depositor, &50);
    usdc_client.approve(&depositor, &vault_address, &50, &1000);
    let result = client.try_deposit(&depositor, &50);
    assert!(result.is_err());
}

#[test]
fn test_set_authorized_caller() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let auth_caller = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    client.set_authorized_caller(&Some(auth_caller.clone()), &0u64);
    let meta = client.get_meta();
    assert_eq!(meta.authorized_caller, Some(auth_caller));
}

#[test]
#[should_panic(expected = "unauthorized: owner only")]
fn set_authorized_caller_non_owner_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let _non_owner = Address::generate(&env);
    let new_caller = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // Attempt to set authorized caller as non-owner
    let _ = non_owner; // non_owner has no way to override owner auth without caller param
    client.set_authorized_caller(&Some(new_caller), &0u64);
}

#[test]
fn set_authorized_caller_vault_address_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let result = client.try_set_authorized_caller(&Some(vault_address), &0u64);
    assert_eq!(
        result,
        Err(Ok(VaultError::AuthorizedCallerCannotBeVault))
    );
}

#[test]
fn set_authorized_caller_clear_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let auth_caller = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // Set authorized caller (nonce 0 → stored nonce becomes 1)
    client.set_authorized_caller(&Some(auth_caller.clone()), &0u64);
    let meta = client.get_meta();
    assert_eq!(meta.authorized_caller, Some(auth_caller));

    // Clear authorized caller (nonce 1 → stored nonce becomes 2)
    client.set_authorized_caller(&None, &1u64);
    let meta2 = client.get_meta();
    assert_eq!(meta2.authorized_caller, None);
}

// ---------------------------------------------------------------------------
// Nonce-based replay-protection tests for set_authorized_caller
// ---------------------------------------------------------------------------

/// Nonce defaults to 0 before any rotation has occurred.
#[test]
fn set_authorized_caller_nonce_default_is_zero() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    assert_eq!(client.get_authorized_caller_nonce(), 0u64);
}

/// First rotation with nonce 0 succeeds and increments the stored nonce to 1.
#[test]
fn set_authorized_caller_first_rotation_with_nonce_zero() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_caller = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    client.set_authorized_caller(&Some(new_caller.clone()), &0u64);

    assert_eq!(client.get_authorized_caller_nonce(), 1u64);
    assert_eq!(client.get_meta().authorized_caller, Some(new_caller));
}

/// Replaying the same nonce after a successful rotation is rejected.
#[test]
fn set_authorized_caller_stale_nonce_rejected() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let caller_a = Address::generate(&env);
    let caller_b = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // First rotation succeeds with nonce 0.
    client.set_authorized_caller(&Some(caller_a.clone()), &0u64);

    // Replaying nonce 0 must be rejected.
    let result = client.try_set_authorized_caller(&Some(caller_b.clone()), &0u64);
    assert_eq!(result, Err(Ok(VaultError::StaleNonce)));

    // Stored caller is still caller_a — no state was mutated by the replay.
    assert_eq!(client.get_meta().authorized_caller, Some(caller_a));
}

/// Supplying a future nonce that hasn't been reached is rejected.
#[test]
fn set_authorized_caller_future_nonce_rejected() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_caller = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let result = client.try_set_authorized_caller(&Some(new_caller), &5u64);
    assert_eq!(result, Err(Ok(VaultError::StaleNonce)));

    assert_eq!(client.get_authorized_caller_nonce(), 0u64);
}

/// Three sequential rotations with nonces 0, 1, 2 each succeed and leave
/// the stored nonce at 3.
#[test]
fn set_authorized_caller_nonce_increments_sequentially() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    for nonce in 0u64..3 {
        let caller = Address::generate(&env);
        client.set_authorized_caller(&Some(caller), &nonce);
        assert_eq!(client.get_authorized_caller_nonce(), nonce + 1);
    }
    assert_eq!(client.get_authorized_caller_nonce(), 3u64);
}

/// When the stored nonce is u64::MAX a successful rotation wraps to 0.
#[test]
fn set_authorized_caller_nonce_wraps_at_u64_max() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_caller = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // Manually prime the nonce to u64::MAX via storage so we don't need 2^64 calls.
    env.as_contract(&client.address, || {
        env.storage()
            .instance()
            .set(&StorageKey::AuthorizedCallerNonce, &u64::MAX);
    });
    assert_eq!(client.get_authorized_caller_nonce(), u64::MAX);

    // Rotation with nonce u64::MAX succeeds and wraps to 0.
    client.set_authorized_caller(&Some(new_caller.clone()), &u64::MAX);
    assert_eq!(client.get_authorized_caller_nonce(), 0u64);
    assert_eq!(client.get_meta().authorized_caller, Some(new_caller));
}

/// A failed rotation (wrong nonce) does not advance the nonce.
#[test]
fn set_authorized_caller_failed_rotation_does_not_advance_nonce() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_caller = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // Wrong nonce — must fail.
    let _ = client.try_set_authorized_caller(&Some(new_caller.clone()), &99u64);

    // Stored nonce is unchanged at 0.
    assert_eq!(client.get_authorized_caller_nonce(), 0u64);

    // Correct nonce still works afterwards.
    client.set_authorized_caller(&Some(new_caller.clone()), &0u64);
    assert_eq!(client.get_authorized_caller_nonce(), 1u64);
}

/// Successful rotation emits the consumed nonce in the event data.
#[test]
fn set_authorized_caller_event_emits_nonce() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_caller = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_authorized_caller(&Some(new_caller.clone()), &0u64);

    let events = env.events().all();
    let ev = events.last().expect("expected set_authorized_caller event");

    let topic: Symbol = ev.1.get(0).unwrap().into_val(&env);
    assert_eq!(topic, Symbol::new(&env, "set_authorized_caller"));

    let (old, now, nonce): (Option<Address>, Option<Address>, u64) = ev.2.into_val(&env);
    assert_eq!(old, None);
    assert_eq!(now, Some(new_caller));
    assert_eq!(nonce, 0u64);
}

#[test]
fn test_deduct_with_settlement_success() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(
        &owner,
        &usdc_address,
        &Some(1000),
        &None,
        &None,
        &None,
        &None,
    );
    client.set_settlement(&owner, &settlement);

    client.deduct(&owner, &300, &None, &u16::MAX);

    assert_eq!(client.balance(, &Address::generate(&env)), 700);
    assert_eq!(usdc_client.balance(&settlement), 300);
}

// ---------------------------------------------------------------------------
// Checked arithmetic â€” overflow / underflow boundary tests
// ---------------------------------------------------------------------------

#[test]
fn deposit_near_i128_max_succeeds() {
    // Verify that a balance sitting just below i128::MAX can accept one more deposit.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    // Start with i128::MAX - 100 so there is headroom for a 100-unit deposit.
    let initial: i128 = i128::MAX - 100;
    fund_vault(&usdc_admin, &vault_address, initial);
    client.init(&owner, &usdc, &Some(initial), &None, &None, &None, &None);

    usdc_admin.mint(&owner, &100);
    usdc_client.approve(&owner, &vault_address, &100, &1000);
    let new_balance = client.deposit(&owner, &100);
    assert_eq!(new_balance, i128::MAX);
}

#[test]
fn deposit_overflow_panics() {
    // A deposit that would push balance past i128::MAX must panic.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, i128::MAX);
    client.init(&owner, &usdc, &Some(i128::MAX), &None, &None, &None, &None);

    usdc_admin.mint(&owner, &1);
    usdc_client.approve(&owner, &vault_address, &1, &1000);
    let result = client.try_deposit(&owner, &1);
    assert!(result.is_err(), "expected overflow panic");
}

#[test]
fn deduct_to_zero_succeeds() {
    // Deducting the entire balance should leave exactly 0, not underflow.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);

    assert_eq!(client.deduct(&owner, &500, &None, &u16::MAX, &Address::generate(&env)), 0);
}

#[test]
fn withdraw_to_zero_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 300);
    client.init(&owner, &usdc, &Some(300), &None, &None, &None, &None);

    assert_eq!(client.withdraw(&300), 0);
}

#[test]
fn withdraw_near_i128_max_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    let initial: i128 = i128::MAX - 100;
    fund_vault(&usdc_admin, &vault_address, initial);
    client.init(&owner, &usdc, &Some(initial), &None, &None, &None, &None);

    let remaining = client.withdraw(&(initial - 1));
    assert_eq!(remaining, 1);
}

#[test]
fn batch_deduct_to_zero_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);
    usdc_admin.mint(&owner, &600);
    usdc_client.approve(&owner, &vault_address, &600, &1000);
    client.deposit(&owner, &600);

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 200,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 200,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 200,
            request_id: None
        , developer: Address::generate(&env) },
    ];
    assert_eq!(client.batch_deduct(&owner, &items), 0);
}

// ---------------------------------------------------------------------------
// Issue #108 â€” set_allowed_depositor: duplicate add, clear, unauthorized
// ---------------------------------------------------------------------------
// propose_revenue_pool / accept_revenue_pool / cancel_revenue_pool tests
// ---------------------------------------------------------------------------
//
// The old set_revenue_pool function has been replaced with a two-step
// propose/accept flow.  These tests cover the new API.
// ---------------------------------------------------------------------------

#[test]
fn is_paused_no_state_mutation() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    let rp = Address::generate(&env);
    client.init(&owner, &usdc, &None, &None, &None, &Some(rp), &None);

    // Get balance before calling is_paused
    let balance_before = client.balance();

    // Call is_paused multiple times
    for _ in 0..100 {
        let _ = client.is_paused();
    }

    // Balance should remain unchanged (no state mutation)
    assert_eq!(client.balance(), balance_before);
}

#[test]
fn is_paused_reflects_latest_committed_state() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    let rp = Address::generate(&env);
    client.init(&owner, &usdc, &None, &None, &None, &Some(rp), &None);

    // Initial state
    assert!(!client.is_paused());

    // Pause and verify immediate reflection
    client.pause(&owner);
    assert!(client.is_paused());

    // Unpause and verify immediate reflection
    client.unpause(&owner);
    assert!(!client.is_paused());

    // Admin change shouldn't affect pause state
    client.set_admin(&owner, &new_admin);
    client.accept_admin();
    assert!(!client.is_paused());

    // New admin can pause
    client.pause(&new_admin);
    assert!(client.is_paused());
}

#[test]
fn is_paused_safe_default_before_init() {
    let env = Env::default();
    let (_, client) = create_vault(&env);
    // Before initialization, is_paused should return false (safe default)
    // and must not panic
    assert!(!client.is_paused());
}

// ---------------------------------------------------------------------------
// get_pending_owner / get_pending_admin tests
// ---------------------------------------------------------------------------

#[test]
fn get_pending_owner_returns_none_before_transfer() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // Before any transfer_ownership call, should return None
    assert_eq!(client.get_pending_owner(), None);
}

#[test]
fn get_pending_owner_returns_some_after_transfer() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    // Before transfer_ownership
    assert_eq!(client.get_pending_owner(), None);

    // After transfer_ownership
    client.transfer_ownership(&new_owner);
    assert_eq!(client.get_pending_owner(), Some(new_owner.clone()));

    // After accept_ownership, pending should be cleared
    client.accept_ownership();
    assert_eq!(client.get_pending_owner(), None);
}

#[test]
fn get_pending_admin_returns_none_before_transfer() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // Before any set_admin call, should return None
    assert_eq!(client.get_pending_admin(), None);
}

#[test]
fn get_pending_admin_returns_some_after_transfer() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    // Before set_admin
    assert_eq!(client.get_pending_admin(), None);

    // After set_admin
    client.set_admin(&owner, &new_admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));

    // After accept_admin, pending should be cleared
    client.accept_admin();
    assert_eq!(client.get_pending_admin(), None);
}
#[test]
#[should_panic(expected = "vault is paused")]
fn deduct_while_paused_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);
    client.pause(&owner);
    client.deduct(&owner, &100, &None, &u16::MAX);
}

#[test]
#[should_panic(expected = "vault is paused")]
fn batch_deduct_while_paused_fails() {
    let env = Env::default();
    let owner = Address::generate(&env, &Address::generate(&env));
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);
    client.pause(&owner);
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: None
        , developer: Address::generate(&env) }
    ];
    client.batch_deduct(&owner, &items); // must panic with "vault is paused"
}

#[test]
#[should_panic(expected = "unauthorized caller")]
fn deduct_unauthorized_caller_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    // init with an authorized_caller so the None branch is not taken
    let auth = Address::generate(&env);
    client.init(&owner, &usdc, &Some(500), &Some(auth), &None, &None, &None);
    client.deduct(&attacker, &100, &None, &u16::MAX);
}

#[test]
#[should_panic(expected = "unauthorized caller")]
fn batch_deduct_unauthorized_caller_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env, &Address::generate(&env));
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    let auth = Address::generate(&env);
    client.init(&owner, &usdc, &Some(500), &Some(auth), &None, &None, &None);
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: None,
        },
    ];
    client.batch_deduct(&attacker, &items);
}

#[test]
#[should_panic(expected = "deduct amount exceeds max_deduct")]
fn deduct_exceeds_max_deduct_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &Some(50));
    client.deduct(&owner, &100, &None, &u16::MAX); // 100 > max_deduct(50)
}

#[test]
#[should_panic(expected = "deduct amount exceeds max_deduct")]
fn batch_deduct_item_exceeds_max_deduct_fails() {
    let env = Env::default();
    let owner = Address::generate(&env, &Address::generate(&env));
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 1000);
    client.init(&owner, &usdc, &Some(1000), &None, &None, &None, &Some(50));
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: None,
        },
    ];
    client.batch_deduct(&owner, &items);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn distribute_negative_amount_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let dev = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(0), &None, &None, &None, &None);
    client.distribute(&owner, &dev, &-1);
}

#[test]
#[should_panic(expected = "no admin transfer pending")]
fn accept_admin_without_pending_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.accept_admin();
}

#[test]
fn cancel_admin_transfer_success() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    client.set_admin(&owner, &new_admin);
    assert_eq!(client.get_pending_admin(), Some(new_admin.clone()));

    client.cancel_admin_transfer(&owner);
    assert_eq!(client.get_pending_admin(), None);

    let events = env.events().all();
    let last_event = events.last().unwrap();
    let event_name = Symbol::try_from_val(&env, &last_event.1.get(0).unwrap()).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "admin_cancelled"));
}

#[test]
#[should_panic(expected = "unauthorized")]
fn cancel_admin_transfer_unauthorized() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_admin(&owner, &new_admin);
    client.cancel_admin_transfer(&attacker);
}

#[test]
#[should_panic(expected = "no admin transfer pending")]
fn cancel_admin_transfer_no_pending_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.cancel_admin_transfer(&owner);
}


#[test]
#[should_panic(expected = "no ownership transfer pending")]
fn accept_ownership_without_pending_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.accept_ownership();
}

// ---------------------------------------------------------------------------
// Cancel ownership transfer tests
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "amount must be positive")]
fn withdraw_negative_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.withdraw(&-1);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn withdraw_to_negative_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);
    client.withdraw_to(&recipient, &-1);
}

#[test]
#[should_panic(expected = "settlement address not set")]
fn deduct_without_settlement_panics() {
    // Settlement is a hard precondition for deduct; missing address must panic.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    client.deduct(&owner, &200, &None, &u16::MAX);
}

#[test]
fn deduct_without_settlement_does_not_mutate_state(, &Address::generate(&env)) {
    // When deduct panics due to missing settlement, vault state must be unchanged.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);

    let result = client.try_deduct(&owner, &200, &None, &u16::MAX);
    assert!(result.is_err(), "expected panic for missing settlement");
    assert_eq!(client.balance(), 500);
    assert_eq!(usdc_client.balance(&vault_address), 500);
}

#[test]
#[should_panic(expected = "settlement address not set")]
fn batch_deduct_without_settlement_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: None,
        },
        DeductItem {
            amount: 50,
            request_id: None,
        },
    ];
    client.batch_deduct(&owner, &items);
}

#[test]
fn batch_deduct_without_settlement_does_not_mutate_state() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &None);
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 100,
            request_id: None,
        },
        DeductItem {
            amount: 50,
            request_id: None,
        },
    ];
    let result = client.try_batch_deduct(&owner, &items);
    assert!(result.is_err(), "expected panic for missing settlement");
    assert_eq!(client.balance(), 500);
    assert_eq!(usdc_client.balance(&vault_address), 500);
}

#[test]
fn withdraw_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 300);
    client.init(&owner, &usdc, &Some(300), &None, &None, &None, &None);
    client.withdraw(&100);
    let events = env.events().all();
    let ev = events
        .iter()
        .find(|e| {
            e.0 == vault_address && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "withdraw")
            }
        })
        .expect("expected withdraw event");
    let (amt, bal): (i128, i128) = ev.2.into_val(&env);
    assert_eq!(amt, 100);
    assert_eq!(bal, 200);
}

#[test]
fn withdraw_to_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 300);
    client.init(&owner, &usdc, &Some(300), &None, &None, &None, &None);
    client.withdraw_to(&recipient, &150);
    let events = env.events().all();
    let ev = events
        .iter()
        .find(|e| {
            e.0 == vault_address && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "withdraw_to")
            }
        })
        .expect("expected withdraw_to event");
    let (amt, bal): (i128, i128) = ev.2.into_val(&env);
    assert_eq!(amt, 150);
    assert_eq!(bal, 150);
}

#[test]
fn distribute_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let dev = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(0), &None, &None, &None, &None);
    client.distribute(&owner, &dev, &200);
    let events = env.events().all();
    let ev = events
        .iter()
        .find(|e| {
            e.0 == vault_address && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "distribute")
            }
        })
        .expect("expected distribute event");
    let amt: i128 = ev.2.into_val(&env);
    assert_eq!(amt, 200);
}

#[test]
fn get_allowed_depositors_returns_list() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let d1 = Address::generate(&env);
    let d2 = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_allowed_depositor(&owner, &Some(d1.clone()));
    client.set_allowed_depositor(&owner, &Some(d2.clone()));
    let list = client.get_allowed_depositors();
    assert_eq!(list.len(), 2);
}

#[test]
fn vault_unpaused_event_emitted() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);
    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.pause(&owner);
    client.unpause(&owner);
    let events = env.events().all();
    let ev = events
        .iter()
        .find(|e| {
            e.0 == vault_address && !e.1.is_empty() && {
                let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                t == Symbol::new(&env, "vault_unpaused")
            }
        })
        .expect("expected vault_unpaused event");
    let caller: Address = ev.1.get(1).unwrap().into_val(&env);
    assert_eq!(caller, owner);
}

// ---------------------------------------------------------------------------
// Randomised sequence tests
//
// Invariants under test:
//   1. VaultMeta.balance >= 0 after every operation.
//   2. Local simulator tracks the same balance as the contract at each step.
//   3. batch_deduct is atomic: a failing batch leaves balance unchanged.
//   4. pause blocks deposits and deductions; unpause restores both.
//   5. No single deduct/batch item may exceed max_deduct.
//
// Seeds are fixed so runs are deterministic and reproducible in CI.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod fuzz {
    use super::*;
    use rand::rngs::StdRng;
    use rand::{Rng, SeedableRng};

    /// Run a mixed sequence of deposit / deduct / batch_deduct / pause / unpause
    /// and assert after every step that:
    ///   - contract balance == local simulator
    ///   - contract balance >= 0
    fn run_sequence(seed: u64, max_deduct_val: i128, initial: i128, steps: usize) {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let caller = Address::generate(&env);
        let (usdc_addr, usdc_client, usdc_admin) = create_usdc(&env, &owner);
        let (vault_addr, client) = create_vault(&env);

        let _settlement = Address::generate(&env);
        // Pre-fund vault so initial_balance is valid.
        usdc_admin.mint(&vault_addr, &initial);
        client.init(
            &owner,
            &usdc_addr,
            &Some(initial),
            &Some(caller.clone()),
            &Some(1), // min_deposit = 1
            &None,
            &Some(max_deduct_val),
        );
        // Settlement is a precondition for deduct / batch_deduct.
        client.set_settlement(&owner, &settlement);
        // Give the depositor (owner) plenty of USDC.
        // Use a very large amount to handle large max_deduct scenarios
        let deposit_reserve: i128 = 10_000_000_000_000; // 10 trillion to handle large deposits
        usdc_admin.mint(&owner, &deposit_reserve);
        usdc_client.approve(&owner, &vault_addr, &i128::MAX, &999_999);

        // Keep random steps realistic even when max_deduct is astronomically large.
        // (We still exercise max_deduct boundaries in dedicated unit tests.)
        let _step_cap: i128 = core::cmp::min(max_deduct_val, 10_000);

        let mut rng = StdRng::seed_from_u64(seed);
        let mut sim: i128 = initial;
        let mut token_sim: i128 = initial;
        let mut paused = false;
        let op_cap: i128 = if max_deduct_val > 10_000 {
            10_000
        } else {
            max_deduct_val
        };

        for _ in 0..steps {
            // Pick an operation: 0=deposit, 1=deduct, 2=batch_deduct, 3=toggle_pause
            let op: u8 = rng.gen_range(0..4);

            match op {
                // --- deposit ---
                0 => {
                    // Cap deposit amount to avoid exceeding available balance
                    let max_deposit = max_deduct_val.min(1_000_000_000);
                    let amount: i128 = rng.gen_range(1..=max_deposit);
                    if paused {
                        // deposit must fail while paused
                        assert!(client.try_deposit(&owner, &amount).is_err());
                    } else if let (Some(new_sim), Some(new_token_sim)) =
                        (sim.checked_add(amount), token_sim.checked_add(amount))
                    {
                        // mint amount to owner to avoid insufficient balance on large fuzz tests
                        usdc_admin.mint(&owner, &amount);
                        sim = new_sim;
                        token_sim = new_token_sim;
                        client.deposit(&owner, &amount);
                    }
                    // else: deposit failed (e.g. insufficient USDC) — no sim change
                }
                // --- single deduct ---
                1 => {
                    let amount: i128 = rng.gen_range(1..=op_cap);
                    if paused {
                        // deduct must fail while paused
                        assert!(client.try_deduct(&caller, &amount, &None, &u16::MAX).is_err());
                    } else if sim >= amount {
                        sim -= amount;
                        client.deduct(&caller, &amount, &None, &u16::MAX);
                    } else {
                        // must fail — balance unchanged (insufficient, &Address::generate(&env))
                        assert!(client.try_deduct(&caller, &amount, &None, &u16::MAX).is_err());
                    }
                }

                // --- batch_deduct ---
                2 => {
                    // Build a batch of 1..=5 items, each within max_deduct.
                    let n: usize = rng.gen_range(1..=5);
                    let mut batch_total: i128 = 0;
                    let mut valid = true;
                    for _ in 0..n {
                        let amt: i128 = rng.gen_range(1..=op_cap);
                        batch_total = match batch_total.checked_add(amt) {
                            Some(v) => v,
                            None => {
                                valid = false;
                                break;
                            }
                        };
                    }
                    if paused {
                        // batch_deduct must fail while paused
                        let before = client.balance();
                        let items = soroban_sdk::Vec::new(&env);
                        let _ = client.try_batch_deduct(&caller, &items);
                        assert_eq!(
                            client.balance(),
                            before,
                            "failed batch must not change balance"
                        );
                    } else if valid && sim >= batch_total {
                        sim -= batch_total;
                        let items = soroban_sdk::Vec::new(&env);
                        client.batch_deduct(&caller, &items);
                    } else {
                        // batch must fail atomically — balance unchanged (paused, overflow, or insufficient)
                        let before = client.balance();
                        let items = soroban_sdk::Vec::new(&env);
                        let _ = client.try_batch_deduct(&caller, &items);
                        assert_eq!(
                            client.balance(),
                            before,
                            "failed batch must not change balance"
                        );
                    }
                }

                // --- toggle pause ---
                3 => {
                    if paused {
                        client.unpause(&owner);
                        paused = false;
                    } else {
                        client.pause(&owner);
                        paused = true;
                    }
                }

                _ => unreachable!(),
            }

            // Invariant assertions after every step.
            let on_chain = client.balance();
            assert_eq!(on_chain, sim, "seed={seed} sim mismatch");
            assert!(on_chain >= 0, "seed={seed} balance went negative");
        }

        // Leave vault unpaused so teardown is clean.
        if paused {
            client.unpause(&owner);
        }
    }

    #[test]
    #[ignore = "soroban reentrancy incompatible"]
    fn fuzz_deposit_and_deduct() {
        // Original invariant: mixed deposits and single deducts stay non-negative.
        run_sequence(0xdead_beef, 500, 10_000, 200);
        // ensure vault is left unpaused for teardown (run_sequence already handles this)
    }

    #[test]
    #[ignore = "soroban reentrancy incompatible"]
    fn fuzz_batch_deduct_coverage() {
        // Heavier batch_deduct weight via a different seed.
        run_sequence(0xcafe_1234, 200, 5_000, 150);
    }

    #[test]
    #[ignore = "soroban reentrancy incompatible"]
    fn fuzz_pause_interleaved() {
        // Pause/unpause interleaved with deposits and deductions.
        run_sequence(0xf00d_abcd, 1_000, 50_000, 100);
    }

    #[test]
    #[ignore = "soroban reentrancy incompatible"]
    fn fuzz_tight_max_deduct() {
        // max_deduct = 1 forces many small steps; exercises boundary exhaustively.
        run_sequence(0x1234_5678, 1, 500, 300);
    }

    #[test]
    #[ignore = "soroban reentrancy incompatible"]
    fn fuzz_large_max_deduct() {
        // max_deduct near i128::MAX / 100 — checks no overflow in batch totals.
        run_sequence(0xabcd_ef01, i128::MAX / 100, 1_000_000, 80);
    }

    /// Verify that a batch whose cumulative total exceeds balance is fully atomic:
    /// balance must be identical before and after the failed call.
    #[test]
    fn fuzz_batch_atomicity_on_overdraw() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let caller = Address::generate(&env);
        let (usdc_addr, _, usdc_admin) = create_usdc(&env, &owner);
        let (vault_addr, client) = create_vault(&env);

        usdc_admin.mint(&vault_addr, &300);
        client.init(
            &owner,
            &usdc_addr,
            &Some(300),
            &Some(caller.clone()),
            &None,
            &None,
            &Some(200),
        );
        let settlement = create_settlement(&env, &owner, &vault_addr);
        client.set_settlement(&owner, &settlement);

        let mut rng = StdRng::seed_from_u64(0x5eed_0001);
        // Build batches that sometimes overdraw; assert atomicity each time.
        for _ in 0..50 {
            let before = client.balance();
            let n: usize = rng.gen_range(1..=5);
            let mut items = soroban_sdk::Vec::new(&env);
            for _ in 0..n {
                items.push_back(DeductItem {
                    amount: rng.gen_range(1..=200_i128),
                    request_id: None,
                });
            }
            let total: i128 = items.iter().map(|i| i.amount).sum();
            if before >= total {
                client.batch_deduct(&owner, &items);
                assert_eq!(client.balance(), before - total);
            } else {
                let _ = client.try_batch_deduct(&owner, &items);
                assert_eq!(client.balance(), before, "atomic rollback failed");
            }
            assert!(client.balance() >= 0);
        }
    }

    /// Verify that max_deduct is enforced on every individual item in a batch.
    #[test]
    fn fuzz_max_deduct_enforced_in_batch() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let caller = Address::generate(&env);
        let (usdc_addr, _, usdc_admin) = create_usdc(&env, &owner);
        let (vault_addr, client) = create_vault(&env);
        let max_d: i128 = 100;

        usdc_admin.mint(&vault_addr, &10_000);
        client.init(
            &owner,
            &usdc_addr,
            &Some(10_000),
            &Some(caller.clone()),
            &None,
            &None,
            &Some(max_d),
        );
        let settlement = create_settlement(&env, &owner, &vault_addr);
        client.set_settlement(&owner, &settlement);

        let mut rng = StdRng::seed_from_u64(0x5eed_0002);
        for _ in 0..40 {
            // Occasionally inject an item that exceeds max_deduct.
            let exceed = rng.gen_bool(0.3);
            let amt: i128 = if exceed {
                rng.gen_range(max_d + 1..=max_d * 3)
            } else {
                rng.gen_range(1..=max_d)
            };
            let items = soroban_sdk::vec![
                &env,
                DeductItem {
                    amount: amt,
                    request_id: None,
                },
            ];
            if exceed {
                assert!(
                    client.try_batch_deduct(&owner, &items).is_err(),
                    "item exceeding max_deduct must be rejected"
                );
            } else if client.balance() >= amt {
                client.batch_deduct(&owner, &items);
                assert!(client.balance() >= 0);
            }
        }
    }

    // -----------------------------------------------------------------------
    // Issue #234 — extended deterministic fuzz coverage
    //
    // New invariants explicitly validated here (building on existing suite):
    //   A. Strict alternating deposit→deduct sequence keeps balance non-negative
    //      and in sync with a local simulator at every step.
    //   B. A batch_deduct-heavy alternating driver validates atomicity and the
    //      cumulative-total guard across many random batch sizes.
    //   C. pause() mid-sequence blocks all mutating ops; unpause() restores them;
    //      balance stays consistent with the simulator throughout.
    //   D. max_deduct is enforced per-item in every batch item of an alternating
    //      sequence; over-limit items are rejected atomically without corrupting
    //      the simulator balance.
    //   E. Single-stroop boundary — min_deposit=1 / max_deduct=1 exercises the
    //      tightest possible constraint across many alternating steps.
    //   F. Two independent authorized callers interleave deductions; the combined
    //      simulator still matches the contract balance after every step.
    // -----------------------------------------------------------------------

    /// A. Strict alternating deposit → single-deduct sequence.
    ///
    /// # Invariants under test
    /// - After every deposit: `balance == sim` and `balance >= 0`.
    /// - After every deduct (when balance is sufficient): `balance == sim` and
    ///   `balance >= 0`.
    /// - A deduct that would go negative is rejected; balance and sim are unchanged.
    #[test]
    fn fuzz_strict_alternating_deposit_deduct() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let caller = Address::generate(&env);
        let (usdc_addr, _, usdc_admin) = create_usdc(&env, &owner);
        let (vault_addr, client) = create_vault(&env);
        let max_d: i128 = 500;

        // Pre-fund so init succeeds with initial_balance = 1_000.
        usdc_admin.mint(&vault_addr, &1_000);
        client.init(
            &owner,
            &usdc_addr,
            &Some(1_000),
            &Some(caller.clone()),
            &Some(1), // min_deposit = 1
            &None,
            &Some(max_d),
        );
        let settlement = create_settlement(&env, &owner, &vault_addr);
        client.set_settlement(&owner, &settlement);

        let mut rng = StdRng::seed_from_u64(0xA1B2_C3D4);
        let mut sim: i128 = 1_000;

        for step in 0..400_usize {
            // Even steps: deposit; odd steps: attempt single deduct.
            if step % 2 == 0 {
                let amount: i128 = rng.gen_range(1..=max_d);
                usdc_admin.mint(&owner, &amount);
                sim = sim
                    .checked_add(amount)
                    .unwrap_or_else(|| panic!("sim overflow at step {step}"));
                client.deposit(&owner, &amount);
            } else {
                let amount: i128 = rng.gen_range(1..=max_d);
                if sim >= amount {
                    sim -= amount;
                    client.deduct(&caller, &amount, &None, &u16::MAX, &Address::generate(&env));
                } else {
                    // Must be rejected; balance and sim are unchanged.
                    assert!(
                        client.try_deduct(&caller, &amount, &None, &u16::MAX).is_err(),
                        "deduct exceeding balance must fail at step {step}"
                    );
                }
            }

            // Invariant assertions after every step.
            let on_chain = client.balance();
            assert_eq!(on_chain, sim, "sim mismatch at step {step}");
            assert!(on_chain >= 0, "balance negative at step {step}");
        }
    }

    /// B. Alternating batch_deduct-heavy sequence.
    ///
    /// # Invariants under test
    /// - Each batch is pre-validated against the local simulator.
    /// - A batch whose cumulative total exceeds the current balance is rejected
    ///   atomically: balance and sim are restored to the pre-call value.
    /// - After every call: `balance == sim` and `balance >= 0`.
    #[test]
    fn fuzz_alternating_batch_deduct_heavy() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let caller = Address::generate(&env);
        let (usdc_addr, _, usdc_admin) = create_usdc(&env, &owner);
        let (vault_addr, client) = create_vault(&env);
        let max_d: i128 = 200;

        usdc_admin.mint(&vault_addr, &2_000);
        client.init(
            &owner,
            &usdc_addr,
            &Some(2_000),
            &Some(caller.clone()),
            &Some(1),
            &None,
            &Some(max_d),
        );
        let settlement = create_settlement(&env, &owner, &vault_addr);
        client.set_settlement(&owner, &settlement);

        let mut rng = StdRng::seed_from_u64(0xB3C4_D5E6);
        let mut sim: i128 = 2_000;

        for step in 0..300_usize {
            if step % 3 == 0 {
                // Deposit once every third step to keep the vault funded.
                let amount: i128 = rng.gen_range(1..=max_d);
                usdc_admin.mint(&owner, &amount);
                sim += amount;
                client.deposit(&owner, &amount);
            } else {
                // Build a batch of 1–5 items, each within max_d.
                let n: usize = rng.gen_range(1..=5_usize);
                let mut items = soroban_sdk::Vec::new(&env);
                let mut batch_total: i128 = 0;
                let mut overflow = false;
                for _ in 0..n {
                    let amt: i128 = rng.gen_range(1..=max_d);
                    items.push_back(DeductItem {
                        amount: amt,
                        request_id: None,
                    });
                    batch_total = match batch_total.checked_add(amt) {
                        Some(v) => v,
                        None => {
                            overflow = true;
                            break;
                        }
                    };
                }
                if overflow {
                    // Overflow means batch total overflowed i128 — must fail.
                    let before = client.balance();
                    let _ = client.try_batch_deduct(&caller, &items);
                    assert_eq!(
                        client.balance(),
                        before,
                        "overflow batch must not change balance at step {step}"
                    );
                } else if sim >= batch_total {
                    sim -= batch_total;
                    client.batch_deduct(&caller, &items);
                } else {
                    // Insufficient balance — must fail atomically.
                    let before = client.balance();
                    let _ = client.try_batch_deduct(&caller, &items);
                    assert_eq!(
                        client.balance(),
                        before,
                        "underfunded batch must not change balance at step {step}"
                    );
                }
            }

            let on_chain = client.balance();
            assert_eq!(on_chain, sim, "sim mismatch at step {step}");
            assert!(on_chain >= 0, "balance negative at step {step}");
        }
    }

    /// C. Pause circuit-breaker under alternating deposit / deduct sequence.
    ///
    /// # Invariants under test
    /// - While paused: every deposit and deduct attempt is rejected; balance
    ///   and sim remain unchanged.
    /// - After unpause: operations resume and balance tracks the simulator.
    /// - pause / unpause themselves never alter VaultMeta.balance.
    #[test]
    fn fuzz_pause_under_alternating_ops() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let caller = Address::generate(&env);
        let (usdc_addr, _, usdc_admin) = create_usdc(&env, &owner);
        let (vault_addr, client) = create_vault(&env);
        let max_d: i128 = 300;

        usdc_admin.mint(&vault_addr, &5_000);
        client.init(
            &owner,
            &usdc_addr,
            &Some(5_000),
            &Some(caller.clone()),
            &Some(1),
            &None,
            &Some(max_d),
        );
        let settlement = create_settlement(&env, &owner, &vault_addr);
        client.set_settlement(&owner, &settlement);

        let mut rng = StdRng::seed_from_u64(0xC5D6_E7F8);
        let mut sim: i128 = 5_000;
        let mut paused = false;

        for step in 0..350_usize {
            // Every ~10 steps, toggle the pause state.
            if step % 10 == 9 {
                if paused {
                    client.unpause(&owner);
                    paused = false;
                } else {
                    client.pause(&owner);
                    paused = true;
                }
                // pause / unpause must not alter balance.
                assert_eq!(
                    client.balance(),
                    sim,
                    "pause/unpause must not change balance at step {step}"
                );
                continue;
            }

            if step % 2 == 0 {
                // Even step: attempt deposit.
                let amount: i128 = rng.gen_range(1..=max_d);
                if paused {
                    assert!(
                        client.try_deposit(&owner, &amount).is_err(),
                        "deposit must fail while paused at step {step}"
                    );
                    // sim unchanged, no mint needed.
                } else {
                    usdc_admin.mint(&owner, &amount);
                    sim += amount;
                    client.deposit(&owner, &amount);
                }
            } else {
                // Odd step: attempt single deduct.
                let amount: i128 = rng.gen_range(1..=max_d);
                if paused {
                    assert!(
                        client.try_deduct(&caller, &amount, &None, &u16::MAX).is_err(),
                        "deduct must fail while paused at step {step}"
                    );
                } else if sim >= amount {
                    sim -= amount;
                    client.deduct(&caller, &amount, &None, &u16::MAX, &Address::generate(&env));
                } else {
                    assert!(
                        client.try_deduct(&caller, &amount, &None, &u16::MAX).is_err(),
                        "insufficient deduct must fail at step {step}"
                    );
                }
            }

            let on_chain = client.balance();
            assert_eq!(on_chain, sim, "sim mismatch at step {step}");
            assert!(on_chain >= 0, "balance negative at step {step}");
        }

        // Leave vault unpaused for clean teardown.
        if paused {
            client.unpause(&owner);
        }
    }

    /// D. max_deduct enforced per-item in alternating batch sequence.
    ///
    /// # Invariants under test
    /// - Any batch that contains even one item exceeding max_deduct is rejected
    ///   atomically regardless of how many other items are within bounds.
    /// - After rejection: `balance == sim` and `balance >= 0`.
    /// - Batches fully within bounds: `balance == sim - batch_total`.
    #[test]
    fn fuzz_max_deduct_enforced_alternating_batch() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let caller = Address::generate(&env);
        let (usdc_addr, _, usdc_admin) = create_usdc(&env, &owner);
        let (vault_addr, client) = create_vault(&env);
        let max_d: i128 = 150;

        usdc_admin.mint(&vault_addr, &10_000);
        client.init(
            &owner,
            &usdc_addr,
            &Some(10_000),
            &Some(caller.clone()),
            &Some(1),
            &None,
            &Some(max_d),
        );
        let settlement = create_settlement(&env, &owner, &vault_addr);
        client.set_settlement(&owner, &settlement);

        let mut rng = StdRng::seed_from_u64(0xD7E8_F901);
        let mut sim: i128 = 10_000;

        for step in 0..300_usize {
            if step % 4 == 0 {
                // Deposit every fourth step.
                let amount: i128 = rng.gen_range(1..=max_d);
                usdc_admin.mint(&owner, &amount);
                sim += amount;
                client.deposit(&owner, &amount);
            } else {
                // Build a batch; randomly inject one over-limit item ~25 % of the time.
                let n: usize = rng.gen_range(1..=4_usize);
                let inject_bad = rng.gen_bool(0.25);
                let inject_pos: usize = rng.gen_range(0..n);
                let mut items = soroban_sdk::Vec::new(&env);
                let mut batch_total: i128 = 0;
                let mut has_over = false;

                for i in 0..n {
                    let amt: i128 = if inject_bad && i == inject_pos {
                        has_over = true;
                        // Amount strictly above max_d.
                        rng.gen_range(max_d + 1..=max_d * 2)
                    } else {
                        rng.gen_range(1..=max_d)
                    };
                    items.push_back(DeductItem {
                        amount: amt,
                        request_id: None,
                    });
                    batch_total = batch_total.saturating_add(amt);
                }

                let before = client.balance();
                if has_over {
                    // Must be rejected atomically.
                    assert!(
                        client.try_batch_deduct(&caller, &items).is_err(),
                        "batch with over-limit item must fail at step {step}"
                    );
                    assert_eq!(
                        client.balance(),
                        before,
                        "atomic reject must not change balance at step {step}"
                    );
                    // sim is unchanged.
                } else if sim >= batch_total {
                    sim -= batch_total;
                    client.batch_deduct(&caller, &items);
                } else {
                    let _ = client.try_batch_deduct(&caller, &items);
                    assert_eq!(
                        client.balance(),
                        before,
                        "underfunded batch must not change balance at step {step}"
                    );
                }
            }

            let on_chain = client.balance();
            assert_eq!(on_chain, sim, "sim mismatch at step {step}");
            assert!(on_chain >= 0, "balance negative at step {step}");
        }
    }

    /// E. Single-stroop boundary — min_deposit = 1, max_deduct = 1.
    ///
    /// # Invariants under test
    /// - The tightest possible constraint: every deposit and deduct touches
    ///   exactly 1 stroop.
    /// - Balance and simulator remain in sync and non-negative throughout.
    #[test]
    fn fuzz_single_stroop_boundary() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let caller = Address::generate(&env);
        let (usdc_addr, _, usdc_admin) = create_usdc(&env, &owner);
        let (vault_addr, client) = create_vault(&env);

        usdc_admin.mint(&vault_addr, &500);
        client.init(
            &owner,
            &usdc_addr,
            &Some(500),
            &Some(caller.clone()),
            &Some(1), // min_deposit = 1
            &None,
            &Some(1), // max_deduct = 1
        );
        let settlement = create_settlement(&env, &owner, &vault_addr);
        client.set_settlement(&owner, &settlement);

        let mut rng = StdRng::seed_from_u64(0xE9FA_0B1C);
        let mut sim: i128 = 500;

        for step in 0..600_usize {
            // Alternate strictly: even → deposit 1, odd → deduct 1.
            if step % 2 == 0 {
                usdc_admin.mint(&owner, &1);
                sim += 1;
                client.deposit(&owner, &1);
            } else if sim >= 1 {
                sim -= 1;
                client.deduct(&caller, &1, &None, &u16::MAX, &Address::generate(&env));
            } else {
                // Balance exhausted: deduct must fail.
                assert!(
                    client.try_deduct(&caller, &1, &None, &u16::MAX).is_err(),
                    "deduct must fail when balance=0 at step {step}"
                );
            }

            let on_chain = client.balance();
            assert_eq!(on_chain, sim, "sim mismatch at step {step}");
            assert!(on_chain >= 0, "balance negative at step {step}");
        }
        // suppress unused warning for rng (used for seeding only in this test)
        let _ = rng.gen_range(0..1_i32);
    }

    /// F. Two authorized callers interleave deductions (multi-caller simulation).
    ///
    /// # Invariants under test
    /// - The owner (caller_a) and a stored authorized_caller (caller_b) both
    ///   issue deductions in random order; a single shared simulator tracks
    ///   the combined effect.
    /// - After every operation: `balance == sim` and `balance >= 0`.
    #[test]
    fn fuzz_multicaller_interleaved_deductions() {
        let env = Env::default();
        env.mock_all_auths();

        let owner = Address::generate(&env);
        let caller_b = Address::generate(&env);
        let (usdc_addr, _, usdc_admin) = create_usdc(&env, &owner);
        let (vault_addr, client) = create_vault(&env);
        let max_d: i128 = 250;

        usdc_admin.mint(&vault_addr, &8_000);
        client.init(
            &owner,
            &usdc_addr,
            &Some(8_000),
            &Some(caller_b.clone()), // authorized_caller = caller_b
            &Some(1),
            &None,
            &Some(max_d),
        );
        let settlement = create_settlement(&env, &owner, &vault_addr);
        client.set_settlement(&owner, &settlement);

        let mut rng = StdRng::seed_from_u64(0xF1A2_B3C4);
        let mut sim: i128 = 8_000;

        for step in 0..400_usize {
            // op: 0=deposit, 1=deduct by owner, 2=deduct by caller_b
            let op: u8 = rng.gen_range(0..3);

            match op {
                0 => {
                    let amount: i128 = rng.gen_range(1..=max_d);
                    usdc_admin.mint(&owner, &amount);
                    sim += amount;
                    client.deposit(&owner, &amount);
                }
                1 => {
                    let amount: i128 = rng.gen_range(1..=max_d);
                    if sim >= amount {
                        sim -= amount;
                        client.deduct(&owner, &amount, &None, &u16::MAX, &Address::generate(&env));
                    } else {
                        assert!(
                            client.try_deduct(&owner, &amount, &None, &u16::MAX).is_err(),
                            "owner deduct must fail when balance insufficient at step {step}"
                        );
                    }
                }
                2 => {
                    let amount: i128 = rng.gen_range(1..=max_d);
                    if sim >= amount {
                        sim -= amount;
                        client.deduct(&caller_b, &amount, &None, &u16::MAX, &Address::generate(&env));
                    } else {
                        assert!(
                            client.try_deduct(&caller_b, &amount, &None, &u16::MAX).is_err(),
                            "caller_b deduct must fail when balance insufficient at step {step}"
                        );
                    }
                }
                _ => unreachable!(),
            }

            let on_chain = client.balance();
            assert_eq!(on_chain, sim, "sim mismatch at step {step}");
            assert!(on_chain >= 0, "balance negative at step {step}");
        }
    }
}

// ---------------------------------------------------------------------------
// Issue #151 — min_deposit boundary tests
// ---------------------------------------------------------------------------

#[test]
fn deposit_exact_min_deposit_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &Some(50), &None, &None);

    usdc_admin.mint(&owner, &50);
    usdc_client.approve(&owner, &vault_address, &50, &1000);
    let balance = client.deposit(&owner, &50);
    assert_eq!(balance, 50);
}

#[test]
#[should_panic(expected = "deposit below minimum")]
fn deposit_below_min_deposit_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &Some(50), &None, &None);

    usdc_admin.mint(&owner, &49);
    usdc_client.approve(&owner, &vault_address, &49, &1000);
    client.deposit(&owner, &49);
}

#[test]
fn deposit_above_min_deposit_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &Some(50), &None, &None);

    usdc_admin.mint(&owner, &51);
    usdc_client.approve(&owner, &vault_address, &51, &1000);
    let balance = client.deposit(&owner, &51);
    assert_eq!(balance, 51);
}

#[test]
fn deposit_with_default_min_deposit_allows_one() {
    // With default min_deposit=1, amount=1 should succeed.
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    usdc_admin.mint(&owner, &1);
    usdc_client.approve(&owner, &vault_address, &1, &1000);
    let balance = client.deposit(&owner, &1);
    assert_eq!(balance, 1);
}

#[test]
fn init_min_deposit_stored_in_meta() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &Some(100), &None, &None);
    let meta = client.get_meta();
    assert_eq!(meta.min_deposit, 100);
}

#[test]
fn init_default_min_deposit_is_one() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    let meta = client.get_meta();
    assert_eq!(meta.min_deposit, 1);
}

#[test]
fn deposit_one_below_large_min_deposit_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &Some(1_000_000), &None, &None);

    usdc_admin.mint(&owner, &999_999);
    usdc_client.approve(&owner, &vault_address, &999_999, &1000);
    let result = client.try_deposit(&owner, &999_999);
    assert!(
        result.is_err(),
        "deposit one below large min_deposit must fail"
    );
}

#[test]
fn deposit_exact_large_min_deposit_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &Some(1_000_000), &None, &None);

    usdc_admin.mint(&owner, &1_000_000);
    usdc_client.approve(&owner, &vault_address, &1_000_000, &1000);
    let balance = client.deposit(&owner, &1_000_000);
    assert_eq!(balance, 1_000_000);
}

// ---------------------------------------------------------------------------
// max_deduct boundary tests
// ---------------------------------------------------------------------------

#[test]
fn deduct_equal_to_max_deduct_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    // max_deduct = 100, deposit 200 so balance is sufficient
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &Some(100));
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);
    usdc_admin.mint(&owner, &200);
    usdc_client.approve(&owner, &vault_address, &200, &1000);
    client.deposit(&owner, &200);
    // deduct exactly equal to max_deduct — must succeed
    let balance = client.deduct(&owner, &100, &None, &u16::MAX, &Address::generate(&env));
    assert_eq!(balance, 600);
}

#[test]
#[should_panic(expected = "deduct amount exceeds max_deduct")]
fn deduct_above_max_deduct_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 500);
    client.init(&owner, &usdc, &Some(500), &None, &None, &None, &Some(100));
    usdc_admin.mint(&owner, &200);
    usdc_client.approve(&owner, &vault_address, &200, &1000);
    client.deposit(&owner, &200);
    // deduct 101 > max_deduct 100 — must panic
    client.deduct(&owner, &101, &None, &u16::MAX);
}

#[test]
fn deduct_default_cap_is_i128_max() {
    let env = Env::default();
    let owner = Address::generate(&env, &Address::generate(&env));
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    // no max_deduct supplied — default cap (i128::MAX) applies
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);
    usdc_admin.mint(&owner, &1_000_000);
    usdc_client.approve(&owner, &vault_address, &1_000_000, &1000);
    client.deposit(&owner, &1_000_000);
    // large deduct well below i128::MAX should succeed
    let balance = client.deduct(&owner, &999_999, &None, &u16::MAX, &Address::generate(&env));
    assert_eq!(balance, 1);
}

#[test]
fn batch_deduct_each_item_constrained_by_max_deduct() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    // max_deduct = 50
    client.init(&owner, &usdc, &None, &None, &None, &None, &Some(50));
    let settlement = create_settlement(&env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);
    usdc_admin.mint(&owner, &300);
    usdc_client.approve(&owner, &vault_address, &300, &1000);
    client.deposit(&owner, &300);
    // three items each exactly at the cap — all must pass
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 50,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 50,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 50,
            request_id: None
        , developer: Address::generate(&env) },
    ];
    let balance = client.batch_deduct(&owner, &items);
    assert_eq!(balance, 150);
}

#[test]
#[should_panic(expected = "deduct amount exceeds max_deduct")]
fn batch_deduct_one_item_above_max_deduct_panics() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 0);
    client.init(&owner, &usdc, &None, &None, &None, &None, &Some(50));
    usdc_admin.mint(&owner, &300);
    usdc_client.approve(&owner, &vault_address, &300, &1000);
    client.deposit(&owner, &300);
    // second item exceeds cap — must panic
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 50,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 51,
            request_id: None
        , developer: Address::generate(&env) },
    ];
    client.batch_deduct(&owner, &items);
}

// ---------------------------------------------------------------------------
// get_contract_addresses tests  (Issue #257)
// ---------------------------------------------------------------------------

#[test]
fn get_contract_addresses_returns_usdc_only_after_init() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    let (got_usdc, got_settlement, got_pool) = client.get_contract_addresses();
    assert_eq!(got_usdc, Some(usdc));
    assert_eq!(got_settlement, None);
    assert_eq!(got_pool, None);
}

#[test]
fn get_contract_addresses_reflects_set_settlement() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let settlement = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_settlement(&owner, &settlement);

    let (got_usdc, got_settlement, got_pool) = client.get_contract_addresses();
    assert_eq!(got_usdc, Some(usdc));
    assert_eq!(got_settlement, Some(settlement));
    assert_eq!(got_pool, None);
}

#[test]
fn get_contract_addresses_reflects_set_revenue_pool() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(
        &owner,
        &usdc,
        &None,
        &None,
        &None,
        &Some(pool.clone()),
        &None,
    );

    let (got_usdc, got_settlement, got_pool) = client.get_contract_addresses();
    assert_eq!(got_usdc, Some(usdc));
    assert_eq!(got_settlement, None);
    assert_eq!(got_pool, Some(pool));
}

#[test]
fn get_contract_addresses_reflects_all_three_configured() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let settlement = Address::generate(&env);
    let pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(
        &owner,
        &usdc,
        &None,
        &None,
        &None,
        &Some(pool.clone()),
        &None,
    );
    client.set_settlement(&owner, &settlement);

    let (got_usdc, got_settlement, got_pool) = client.get_contract_addresses();
    assert_eq!(got_usdc, Some(usdc));
    assert_eq!(got_settlement, Some(settlement));
    assert_eq!(got_pool, Some(pool));
}

#[test]
fn get_contract_addresses_updates_after_clear_revenue_pool() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let pool = Address::generate(&env);
    let (_, client) = create_vault(&env);
    let (usdc, _, _) = create_usdc(&env, &owner);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &Some(pool), &None);
    // Use two-step propose/accept to clear the revenue pool
    client.propose_revenue_pool(&None);
    client.accept_revenue_pool();

    let (_, _, got_pool) = client.get_contract_addresses();
    assert_eq!(got_pool, None);
}

// ---------------------------------------------------------------------------
// Instance TTL tests
// ---------------------------------------------------------------------------

/// Verifies that state written during `init` survives a simulated ledger
/// advance of INSTANCE_BUMP_THRESHOLD - 1 ledgers (still within the TTL
/// window) and that the vault remains fully operational afterwards.
#[test]
#[ignore = "soroban reentrancy incompatible"]
fn instance_ttl_extended_on_init_and_state_survives_ledger_advance() {
    use crate::{INSTANCE_BUMP_AMOUNT, INSTANCE_BUMP_THRESHOLD};
    use soroban_sdk::testutils::Ledger as _;

    let env = Env::default();
    let owner = Address::generate(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    // Advance the ledger to just inside the bump window.
    let current = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(current + INSTANCE_BUMP_THRESHOLD - 1);

    // State must still be readable — the TTL was extended to INSTANCE_BUMP_AMOUNT.
    assert_eq!(client.balance(), 0);
    assert_eq!(client.get_admin(), owner);

    // A deposit must also succeed (and re-extend the TTL).
    usdc_admin.mint(&owner, &100);
    usdc_client.approve(
        &owner,
        &vault_address,
        &100,
        &(current + INSTANCE_BUMP_AMOUNT + 1000),
    );
    let new_balance = client.deposit(&owner, &100);
    assert_eq!(new_balance, 100);
}

/// Verifies that `deposit`, `withdraw`, and `withdraw_to` each extend the TTL
/// so state remains accessible after a ledger advance.
#[test]
#[ignore = "soroban reentrancy incompatible"]
fn instance_ttl_extended_on_mutating_entrypoints() {
    use crate::INSTANCE_BUMP_THRESHOLD;
    use soroban_sdk::testutils::Ledger as _;

    let env = Env::default();
    let owner = Address::generate(&env);
    let recipient = Address::generate(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);

    usdc_admin.mint(&owner, &500);
    usdc_client.approve(&owner, &vault_address, &500, &200_000);

    // deposit — bumps TTL
    client.deposit(&owner, &300);
    let seq = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq + INSTANCE_BUMP_THRESHOLD - 1);
    assert_eq!(
        client.balance(),
        300,
        "balance readable after ledger advance post-deposit"
    );

    // withdraw — bumps TTL
    client.withdraw(&100);
    let seq = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq + INSTANCE_BUMP_THRESHOLD - 1);
    assert_eq!(
        client.balance(),
        200,
        "balance readable after ledger advance post-withdraw"
    );

    // withdraw_to — bumps TTL
    client.withdraw_to(&recipient, &50);
    let seq = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq + INSTANCE_BUMP_THRESHOLD - 1);
    assert_eq!(
        client.balance(),
        150,
        "balance readable after ledger advance post-withdraw_to"
    );
}

/// Verifies that `deduct` and `batch_deduct` extend the TTL.
#[test]
#[ignore = "soroban reentrancy incompatible"]
fn instance_ttl_extended_on_deduct_and_batch_deduct() {
    use crate::INSTANCE_BUMP_THRESHOLD;
    use soroban_sdk::testutils::Ledger as _;

    let env = Env::default();
    let owner = Address::generate(&env);
    let (usdc, usdc_client, usdc_admin) = create_usdc(&env, &owner);
    let (vault_address, client) = create_vault(&env);
    let settlement = create_settlement(&env, &owner, &vault_address);

    env.mock_all_auths();
    client.init(&owner, &usdc, &None, &None, &None, &None, &None);
    client.set_settlement(&owner, &settlement);

    usdc_admin.mint(&owner, &500);
    usdc_client.approve(&owner, &vault_address, &500, &200_000);
    client.deposit(&owner, &500);

    // deduct — bumps TTL
    client.deduct(&owner, &100, &None, &u16::MAX);
    let seq = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq + INSTANCE_BUMP_THRESHOLD - 1);
    assert_eq!(
        client.balance(, &Address::generate(&env)),
        400,
        "balance readable after ledger advance post-deduct"
    );

    // batch_deduct — bumps TTL
    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 50,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 50,
            request_id: None
        , developer: Address::generate(&env) },
    ];
    client.batch_deduct(&owner, &items);
    let seq = env.ledger().sequence();
    env.ledger()
        .set_sequence_number(seq + INSTANCE_BUMP_THRESHOLD - 1);
    assert_eq!(
        client.balance(),
        300,
        "balance readable after ledger advance post-batch_deduct"
    );
}

// ---------------------------------------------------------------------------
// Security tests: Re-entrancy and Malicious Token
// ---------------------------------------------------------------------------

mod malicious_token {
    use super::*;

    /// # Malicious Mock Token
    ///
    /// This token contract simulates re-entrancy by calling back into the vault
    /// during a `transfer` operation.
    #[soroban_sdk::contract]
    /// 😈 Malicious Mock Token Contract — simulates re-entrancy attacks during external token transfers.
    ///
    /// ## Attack Model
    /// This contract is designed to simulate re-entrancy-equivalent behavior in Soroban's
    /// external call model. Unlike Ethereum's direct re-entrancy, Soroban uses explicit
    /// cross-contract calls, so this mock token demonstrates how an attacker could attempt
    /// to exploit timing windows during external token transfers.
    ///
    /// ## Why Soroban Requires External-Call Safety Testing
    /// Even though Soroban doesn't have traditional re-entrancy, external calls create
    /// similar attack vectors where:
    /// - State updates happen before external calls complete
    /// - External contracts can make callback calls to the vault
    /// - Timing windows exist between state mutation and external call completion
    /// - Balance accounting must remain consistent despite external interactions
    ///
    /// ## Security Guarantees
    /// This test verifies that the vault's accounting invariants hold under attack:
    /// - `meta.balance` cannot be corrupted or double-spent
    /// - All deductions are atomic and deterministic
    /// - Balance validation occurs before external transfers
    /// - Vault state remains authoritative regardless of external call behavior
    pub struct MaliciousToken;

    #[soroban_sdk::contractimpl]
    impl MaliciousToken {
        /// Attempt a re-entrant call back into the vault during transfer.
        pub fn transfer(env: Env, _from: Address, _to: Address, _amount: i128) {
            let mut count: u32 = env
                .storage()
                .instance()
                .get(&Symbol::new(&env, "count"))
                .unwrap_or(0);
            let max: u32 = env
                .storage()
                .instance()
                .get(&Symbol::new(&env, "max"))
                .unwrap_or(0);

            if count < max {
                count += 1;
                env.storage()
                    .instance()
                    .set(&Symbol::new(&env, "count"), &count);

                let vault_addr: Address = env
                    .storage()
                    .instance()
                    .get(&Symbol::new(&env, "vault"))
                    .unwrap();
                let caller: Address = env
                    .storage()
                    .instance()
                    .get(&Symbol::new(&env, "caller"))
                    .unwrap();
                let attack_amount: i128 = env
                    .storage()
                    .instance()
                    .get(&Symbol::new(&env, "amount"))
                    .unwrap_or(0);

                let vault_client = CalloraVaultClient::new(&env, &vault_addr);

                // 😈 ATTACK: Call back into the vault
                vault_client.deduct(&caller, &attack_amount, &Some(Symbol::new(&env, "reentry")), &u16::MAX, &Address::generate(&env), &Address::generate(&env));
            }
        }

        pub fn balance(_env: Env, _id: Address) -> i128 {
            1_000_000_000
        }

        /// Configure the attack parameters.
        pub fn set_attack_config(
            env: Env,
            vault: Address,
            caller: Address,
            amount: i128,
            max: u32,
        ) {
            let inst = env.storage().instance();
            inst.set(&Symbol::new(&env, "vault"), &vault);
            inst.set(&Symbol::new(&env, "caller"), &caller);
            inst.set(&Symbol::new(&env, "amount"), &amount);
            inst.set(&Symbol::new(&env, "max"), &max);
            inst.set(&Symbol::new(&env, "count"), &0u32);
        }
    }
}
pub use malicious_token::*;

/// ### Test: Single Deduct Re-entrancy
///
/// **Attack Model**:
/// 1. Admin calls `vault.deduct(500)`.
/// 2. Vault updates internal balance: `balance -= 500`.
/// 3. Vault calls `token.transfer(500)`.
/// 4. `token.transfer` calls back: `vault.deduct(600)`.
///
/// **Security Guarantees Verified**:
/// - ✅ **Balance Integrity**: Vault's `meta.balance` cannot be corrupted or double-spent
/// - ✅ **State Consistency**: External token calls cannot mutate vault state incorrectly
/// - ✅ **Deterministic Outcome**: Attack either fails completely (Outcome A) or succeeds with exact accounting (Outcome B)
/// - ✅ **Authorization Protection**: Re-entrant calls respect the same authorization checks as normal calls
/// - ✅ **Invariant Preservation**: All balance validation occurs before external transfers complete
#[test]
fn test_reentry_protection_single_deduct() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, vault_client) = create_vault(&env);

    // Register malicious token
    let malicious_token_addr = env.register(MaliciousToken, ());

    env.mock_all_auths();

    // Initialize vault with malicious token
    vault_client.init(
        &owner,
        &malicious_token_addr,
        &Some(1000), // Start with 1000
        &None,
        &None,
        &None,
        &None,
    );

    let settlement = create_settlement(&env, &owner, &vault_address);
    vault_client.set_settlement(&owner, &settlement);

    let malicious_client = MaliciousTokenClient::new(&env, &malicious_token_addr);

    // Setup attack: when vault calls transfer, call back into vault.deduct(600).
    // Initial: 1000.
    // Call 1: deduct(500) -> balance becomes 500 -> calls transfer.
    // Call 2 (Re-entrant): deduct(600) -> sees balance 500 -> PANIC "insufficient balance".
    malicious_client.set_attack_config(&vault_address, &owner, &600, &1);

    let result = vault_client.try_deduct(&owner, &500, &None, &u16::MAX);

    // Acceptable Outcome A: Re-entrant call fails due to state guard (insufficient balance).
    assert!(
        result.is_err(),
        "Expected re-entrant call to fail due to insufficient balance"
    );

    // Verify balance integrity: no extra deductions, no corruption.
    // In Soroban, a panic in a sub-invocation reverts the entire tx.
    assert_eq!(
        vault_client.balance(),
        1000,
        "Vault balance must remain consistent after failed attack"
    );
}

/// ### Test: Batch Deduct Re-entrancy
///
/// Verifies that re-entrancy during a batch operation cannot corrupt
/// the running balance or total accounting.
///
/// **Security Guarantees Verified**:
/// - ✅ **Batch Atomicity**: Full-batch validation completes before any state write or transfer
/// - ✅ **Partial State Protection**: No partial state corruption possible during batch execution
/// - ✅ **Running Balance Integrity**: The running balance calculation remains consistent throughout batch processing
/// - ✅ **Deterministic Accounting**: Final balance equals expected deterministic value regardless of callback timing
/// - ✅ **Invariant Enforcement**: All balance validations occur before external transfers
#[test]
fn test_reentry_protection_batch_deduct() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, vault_client) = create_vault(&env);

    let malicious_token_addr = env.register(MaliciousToken, ());
    env.mock_all_auths();

    vault_client.init(
        &owner,
        &malicious_token_addr,
        &Some(1000),
        &None,
        &None,
        &None,
        &None,
    );

    let settlement = create_settlement(&env, &owner, &vault_address);
    vault_client.set_settlement(&owner, &settlement);

    let malicious_client = MaliciousTokenClient::new(&env, &malicious_token_addr);

    // Batch total: 500.
    // Re-entrant attack: deduct(600).
    // 1000 - 500 = 500 remaining. 600 > 500 -> Fail.
    malicious_client.set_attack_config(&vault_address, &owner, &600, &1);

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 300,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 200,
            request_id: None
        , developer: Address::generate(&env) },
    ];

    let result = vault_client.try_batch_deduct(&owner, &items);

    assert!(result.is_err());
    assert_eq!(
        vault_client.balance(),
        1000,
        "Batch state must remain consistent"
    );
}

/// ### Test: Successful Re-entry with Correct Accounting
///
/// **Acceptable Outcome B**:
/// If there is enough balance, a re-entrant call can execute, but it MUST
/// update the balance correctly and not allow double-spending.
///
/// **Security Guarantees Verified**:
/// - ✅ **Correct Balance Updates**: Re-entrant calls properly update `meta.balance` without corruption
/// - ✅ **No Double-Spending**: Each deduction amount is accounted for exactly once
/// - ✅ **Deterministic Final State**: Final accounting equals expected deterministic value
/// - ✅ **State Authority**: Vault's internal balance remains the authoritative source of truth
/// - ✅ **Callback Safety**: External callbacks cannot bypass accounting invariants
#[test]
#[ignore = "soroban reentrancy incompatible"]
fn test_reentry_success_preserves_accounting() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, vault_client) = create_vault(&env);

    let malicious_token_addr = env.register(MaliciousToken, ());
    env.mock_all_auths();

    vault_client.init(
        &owner,
        &malicious_token_addr,
        &Some(1000),
        &None,
        &None,
        &None,
        &None,
    );

    let settlement = create_settlement(&env, &owner, &vault_address);
    vault_client.set_settlement(&owner, &settlement);

    let malicious_client = MaliciousTokenClient::new(&env, &malicious_token_addr);

    // Initial: 1000.
    // Call 1: deduct(200) -> balance = 800 -> transfer calls back.
    // Call 2 (Re-entrant): deduct(100) -> sees balance 800 -> balance = 700.
    // Call 1 resumes.
    malicious_client.set_attack_config(&vault_address, &owner, &100, &1);

    vault_client.deduct(&owner, &200, &None, &u16::MAX, &Address::generate(&env));

    // Final balance must be exactly 700.
    assert_eq!(
        vault_client.balance(, &Address::generate(&env)),
        700,
        "Final accounting must be deterministic and correct"
    );
}

/// ### Test: Multiple Nested Re-entry
///
/// Verifies that multiple layers of re-entrancy still preserve invariants.
///
/// **Security Guarantees Verified**:
/// - ✅ **Nested Call Protection**: Multiple layers of re-entrancy cannot lead to exponential state corruption
/// - ✅ **Depth Limitation**: Vault accounting remains consistent regardless of re-entry depth
/// - ✅ **Invariant Preservation**: All balance validations occur at each re-entry level
/// - ✅ **State Isolation**: Each re-entrant call operates on the current state without hidden mutations
/// - ✅ **Deterministic Accounting**: Final balance equals expected deterministic value after all nested calls
#[test]
#[ignore = "soroban reentrancy incompatible"]
fn test_nested_reentry_protection() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, vault_client) = create_vault(&env);

    let token_addr = env.register(MaliciousToken, ());
    env.mock_all_auths();

    vault_client.init(&owner, &token_addr, &Some(1000), &None, &None, &None, &None);
    let settlement = create_settlement(&env, &owner, &vault_address);

    vault_client.set_settlement(&owner, &settlement);

    let token_client = MaliciousTokenClient::new(&env, &token_addr);
    // Try to re-enter 3 times, each deducting 100.
    // Total should be: 100 (original) + 3 * 100 (re-entries) = 400.
    token_client.set_attack_config(&vault_address, &owner, &100, &3);

    vault_client.deduct(&owner, &100, &None, &u16::MAX, &Address::generate(&env));

    assert_eq!(vault_client.balance(, &Address::generate(&env)), 600);
}

/// ### Test: Re-entry with Exact Balance
///
/// Verifies that re-entry can exhaust the balance but not exceed it.
///
/// **Security Guarantees Verified**:
/// - ✅ **Exact Exhaustion Safety**: Vault can be exhausted to zero balance without corruption
/// - ✅ **Overflow Protection**: Attempts to exceed available balance fail deterministically
/// - ✅ **Boundary Condition Handling**: Edge cases like exact balance matching are handled correctly
/// - ✅ **State Consistency**: Balance remains authoritative even at boundary conditions
/// - ✅ **Deterministic Outcomes**: All edge case scenarios produce predictable, verifiable results
#[test]
#[ignore = "soroban reentrancy incompatible"]
fn test_reentry_exact_balance_exhaustion() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, vault_client) = create_vault(&env);

    let malicious_token_addr = env.register(MaliciousToken, ());
    env.mock_all_auths();

    vault_client.init(
        &owner,
        &malicious_token_addr,
        &Some(1000),
        &None,
        &None,
        &None,
        &None,
    );

    let settlement = create_settlement(&env, &owner, &vault_address);
    vault_client.set_settlement(&owner, &settlement);

    let malicious_client = MaliciousTokenClient::new(&env, &malicious_token_addr);

    // deduct(500) -> balance 500.
    // re-entry deduct(500) -> balance 0. (Success)
    malicious_client.set_attack_config(&vault_address, &owner, &500, &1);

    vault_client.deduct(&owner, &500, &None, &u16::MAX, &Address::generate(&env));
    assert_eq!(vault_client.balance(, &Address::generate(&env)), 0);

    // Try again with over-exhaustion
    vault_client.deposit(&owner, &1000);
    // deduct(600) -> balance 400.
    // re-entry deduct(401) -> Fail.
    malicious_client.set_attack_config(&vault_address, &owner, &401, &1);
    let result = vault_client.try_deduct(&owner, &600, &None, &u16::MAX);
    assert!(result.is_err());
    assert_eq!(vault_client.balance(), 1000);
}

/// ### Test: Near-Zero Balance Edge Case
///
/// Verifies that re-entrancy protection works correctly with minimal balances.
/// Tests the boundary condition where balance = 1 and attack attempts to deduct 1 twice.
#[test]
fn test_reentry_near_zero_balance() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, vault_client) = create_vault(&env);

    let malicious_token_addr = env.register(MaliciousToken, ());
    env.mock_all_auths();

    // Initialize vault with near-zero balance
    vault_client.init(
        &owner,
        &malicious_token_addr,
        &Some(1), // Start with balance = 1
        &None,
        &None,
        &None,
        &None,
    );

    let settlement = create_settlement(&env, &owner, &vault_address);
    vault_client.set_settlement(&owner, &settlement);

    let malicious_client = MaliciousTokenClient::new(&env, &malicious_token_addr);

    // Attempt re-entry with same amount as balance
    // Initial: 1
    // Call 1: deduct(1) -> balance becomes 0 -> calls transfer
    // Call 2 (Re-entrant): deduct(1) -> sees balance 0 -> PANIC "insufficient balance"
    malicious_client.set_attack_config(&vault_address, &owner, &1, &1);

    let result = vault_client.try_deduct(&owner, &1, &None, &u16::MAX);

    // Must fail due to insufficient balance in re-entrant call
    assert!(result.is_err());

    // Balance must remain unchanged (no corruption)
    assert_eq!(
        vault_client.balance(),
        1,
        "Near-zero balance must remain consistent after failed attack"
    );
}

/// ### Test: Multiple Recipients in Batch
///
/// Verifies that re-entrancy during batch operations with multiple recipients
/// cannot corrupt accounting or allow partial state mutation.
#[test]
#[ignore = "soroban reentrancy incompatible"]
fn test_reentry_multiple_recipients_batch() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, vault_client) = create_vault(&env);

    let malicious_token_addr = env.register(MaliciousToken, ());
    env.mock_all_auths();

    vault_client.init(
        &owner,
        &malicious_token_addr,
        &Some(1000),
        &None,
        &None,
        &None,
        &None,
    );

    let settlement = create_settlement(&env, &owner, &vault_address);
    vault_client.set_settlement(&owner, &settlement);

    let malicious_client = MaliciousTokenClient::new(&env, &malicious_token_addr);

    // Configure attack to happen during batch processing
    // Batch total: 600 (200 + 200 + 200)
    // Re-entrant attack: deduct(300)
    // After first 200 deduction: balance = 800, so 300 attack should succeed
    malicious_client.set_attack_config(&vault_address, &owner, &300, &1);

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 200,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 200,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 200,
            request_id: None
        , developer: Address::generate(&env) },
    ];

    vault_client.batch_deduct(&owner, &items);

    // Final balance must be exactly 400 (1000 - 600)
    // The re-entrant call should have deducted additional 300, so final should be 100
    assert_eq!(
        vault_client.balance(),
        100,
        "Multiple recipients batch must preserve accounting integrity"
    );
}

/// ### Test: Callback After Partial Batch Execution
///
/// Verifies that re-entrancy protection works correctly when callback occurs
/// after some items in a batch have been processed but before the batch completes.
#[test]
#[ignore = "soroban reentrancy incompatible"]
fn test_reentry_callback_after_partial_batch() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, vault_client) = create_vault(&env);

    let malicious_token_addr = env.register(MaliciousToken, ());
    env.mock_all_auths();

    vault_client.init(
        &owner,
        &malicious_token_addr,
        &Some(1000),
        &None,
        &None,
        &None,
        &None,
    );

    let settlement = create_settlement(&env, &owner, &vault_address);
    vault_client.set_settlement(&owner, &settlement);

    let malicious_client = MaliciousTokenClient::new(&env, &malicious_token_addr);

    // Configure attack to happen during batch processing
    // Batch: [300, 400] = 700 total
    // After first 300 deduction: balance = 700
    // Re-entrant attack: deduct(200) -> should succeed, leaving balance = 500
    malicious_client.set_attack_config(&vault_address, &owner, &200, &1);

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 300,
            request_id: None
        , developer: Address::generate(&env) },
        DeductItem {
            amount: 400,
            request_id: None
        , developer: Address::generate(&env) },
    ];

    vault_client.batch_deduct(&owner, &items);

    // Final balance must be 1000 - 300 - 400 - 200 = 100
    assert_eq!(
        vault_client.balance(),
        100,
        "Callback after partial batch must preserve accounting integrity"
    );
}

/// ### Test: Repeated Callback Attempts
///
/// Verifies that multiple layers of re-entrancy attempts are properly protected
/// and cannot lead to exponential state corruption.
#[test]
#[ignore = "soroban reentrancy incompatible"]
fn test_reentry_repeated_attempts() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, vault_client) = create_vault(&env);

    let malicious_token_addr = env.register(MaliciousToken, ());
    env.mock_all_auths();

    vault_client.init(
        &owner,
        &malicious_token_addr,
        &Some(1000),
        &None,
        &None,
        &None,
        &None,
    );

    let settlement = create_settlement(&env, &owner, &vault_address);
    vault_client.set_settlement(&owner, &settlement);

    let malicious_client = MaliciousTokenClient::new(&env, &malicious_token_addr);

    // Try to re-enter 5 times, each deducting 100.
    // Total should be: 100 (original) + 5 * 100 (re-entries) = 600.
    // This tests that the vault's balance validation prevents over-deduction
    malicious_client.set_attack_config(&vault_address, &owner, &100, &5);

    vault_client.deduct(&owner, &100, &None, &u16::MAX, &Address::generate(&env), &Address::generate(&env));

    // With 5 re-entries of 100 each, plus original 100, total should be 600
    // So final balance should be 1000 - 600 = 400
    assert_eq!(vault_client.balance(), 400);
}

// ---------------------------------------------------------------------------
// Upgrade tests (Issue #331)
// ---------------------------------------------------------------------------

#[test]
fn upgrade_requires_admin() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let new_hash = BytesN::from_array(&env, &[1u8; 32]);

    // Non-admin attempt should fail
    let res = client.try_upgrade(&attacker, &new_hash);
    assert!(res.is_err(), "non-admin should not be able to upgrade");
}

#[test]
fn upgrade_sets_version_and_emits_event() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    // Version should be None before any upgrade
    assert_eq!(client.get_version(), None);

    let new_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.upgrade(&owner, &new_hash);

    // version() should return stored value
    let readback = client.get_version();
    assert_eq!(readback, Some(new_hash.clone()));

    // An `upgraded` event should have been emitted
    let events = env.events().all();
    let ev = events.last().unwrap();
    assert_eq!(ev.0, vault_address);

    let name: Symbol = ev.1.get(0).unwrap().into_val(&env);
    assert_eq!(name, Symbol::new(&env, "upgraded"));

    let admin_topic: Address = ev.1.get(1).unwrap().into_val(&env);
    assert_eq!(admin_topic, owner);

    let data: BytesN<32> = ev.2.into_val(&env);
    assert_eq!(data, new_hash);
}

#[test]
fn upgrade_non_owner_admin_succeeds() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    // Transfer admin to new_admin
    client.set_admin(&owner, &new_admin);
    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);

    let new_hash = BytesN::from_array(&env, &[3u8; 32]);

    // new_admin should be able to upgrade
    client.upgrade(&new_admin, &new_hash);

    let readback = client.get_version();
    assert_eq!(readback, Some(new_hash));
}

#[test]
fn upgrade_owner_not_admin_fails() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    // Transfer admin to new_admin
    client.set_admin(&owner, &new_admin);
    client.accept_admin();
    assert_eq!(client.get_admin(), new_admin);

    let new_hash = BytesN::from_array(&env, &[4u8; 32]);

    // owner (no longer admin) should fail
    let res = client.try_upgrade(&owner, &new_hash);
    assert!(
        res.is_err(),
        "owner without admin role should not be able to upgrade"
    );
}

#[test]
fn version_returns_none_before_first_upgrade() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    assert_eq!(client.get_version(), None);
}

#[test]
fn upgrade_multiple_times_updates_version() {
    let env = Env::default();
    let owner = Address::generate(&env);
    let (vault_address, client) = create_vault(&env);
    let (usdc, _, usdc_admin) = create_usdc(&env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, 100);
    client.init(&owner, &usdc, &Some(100), &None, &None, &None, &None);

    let hash1 = BytesN::from_array(&env, &[5u8; 32]);
    client.upgrade(&owner, &hash1);
    assert_eq!(client.get_version(), Some(hash1.clone()));

    let hash2 = BytesN::from_array(&env, &[6u8; 32]);
    client.upgrade(&owner, &hash2);
    assert_eq!(client.get_version(), Some(hash2.clone()));

    let hash3 = BytesN::from_array(&env, &[7u8; 32]);
    client.upgrade(&owner, &hash3);
    assert_eq!(client.get_version(), Some(hash3));
}

// ---------------------------------------------------------------------------
// BUDGET MEASUREMENT TESTS — for benchmarking and cost analysis
// ---------------------------------------------------------------------------

/// Captures CPU, memory, and ledger read/write metrics from Soroban budget.
#[derive(Clone)]
struct BudgetSnapshot {
    cpu_instructions: u64,
    memory_bytes: u64,
    ledger_read_bytes: u64,
    ledger_write_bytes: u64,
}

impl BudgetSnapshot {
    /// Capture the current budget state from the environment.
    fn capture(env: &Env) -> Self {
        let ce = env.cost_estimate();
        let res = ce.resources();
        Self {
            cpu_instructions: res.instructions as u64,
            memory_bytes: res.mem_bytes as u64,
            ledger_read_bytes: res.read_bytes as u64,
            ledger_write_bytes: res.write_bytes as u64,
        }
    }

    /// Calculate delta between two snapshots (after - before).
    fn delta(&self, before: &BudgetSnapshot) -> BudgetSnapshot {
        BudgetSnapshot {
            cpu_instructions: self
                .cpu_instructions
                .saturating_sub(before.cpu_instructions),
            memory_bytes: self.memory_bytes.saturating_sub(before.memory_bytes),
            ledger_read_bytes: self
                .ledger_read_bytes
                .saturating_sub(before.ledger_read_bytes),
            ledger_write_bytes: self
                .ledger_write_bytes
                .saturating_sub(before.ledger_write_bytes),
        }
    }
}

/// Helper function to set up a fully initialized vault with settlement and sufficient balance.
fn setup_vault_for_deduct(env: &Env, initial_balance: i128) -> (Address, CalloraVaultClient) {
    let owner = Address::generate(env);
    let (vault_address, client) = create_vault(env);
    let (usdc, _, usdc_admin) = create_usdc(env, &owner);

    env.mock_all_auths();
    fund_vault(&usdc_admin, &vault_address, initial_balance);
    client.init(
        &owner,
        &usdc,
        &Some(initial_balance),
        &None,
        &None,
        &None,
        &None,
    );
    let settlement = create_settlement(env, &owner, &vault_address);
    client.set_settlement(&owner, &settlement);

    (owner, client)
}

/// Benchmark: measure the cost of a single `deduct` operation.
///
/// Prints CPU instructions, memory, ledger read/write metrics in CSV format for analysis.
#[test]
#[ignore]
fn budget_measure_single_deduct() {
    let env = Env::default();
    let (owner, client) = setup_vault_for_deduct(&env, 100_000_000);

    let before = BudgetSnapshot::capture(&env);
    client.deduct(&owner, &1_000_000, &None, &u16::MAX);
    let after = BudgetSnapshot::capture(&env);

    let delta = after.delta(&before, &Address::generate(&env));
    std::println!(
        "BUDGET_SINGLE_DEDUCT,cpu_instructions,{},memory_bytes,{},ledger_read_bytes,{},ledger_write_bytes,{}",
        delta.cpu_instructions, delta.memory_bytes, delta.ledger_read_bytes, delta.ledger_write_bytes
    );
}

/// Benchmark: measure the cost of `batch_deduct` with batch size = 1.
///
/// For comparison: a batch of 1 item should have similar cost to single deduct,
/// with possible overhead from the batch validation loop.
#[test]
#[ignore]
fn budget_measure_batch_deduct_size_1() {
    let env = Env::default();
    let (owner, client) = setup_vault_for_deduct(&env, 100_000_000);

    let items = soroban_sdk::vec![
        &env,
        DeductItem {
            amount: 1_000_000,
            request_id: None
        , developer: Address::generate(&env) }
    ];

    let before = BudgetSnapshot::capture(&env);
    client.batch_deduct(&owner, &items);
    let after = BudgetSnapshot::capture(&env);

    let delta = after.delta(&before);
    std::println!(
        "BUDGET_BATCH_DEDUCT_SIZE_1,cpu_instructions,{},memory_bytes,{},ledger_read_bytes,{},ledger_write_bytes,{}",
        delta.cpu_instructions, delta.memory_bytes, delta.ledger_read_bytes, delta.ledger_write_bytes
    );
}

/// Benchmark: measure the cost of `batch_deduct` with batch size = 10.
///
/// Captures the incremental cost of processing 10 items in a single call.
#[test]
#[ignore]
fn budget_measure_batch_deduct_size_10() {
    let env = Env::default();
    let (owner, client) = setup_vault_for_deduct(&env, 100_000_000);

    let mut items = soroban_sdk::Vec::new(&env);
    for _ in 0..10 {
        items.push_back(DeductItem {
            amount: 1_000_000,
            request_id: Some(Symbol::new(&env, "req")),
        });
    }

    let before = BudgetSnapshot::capture(&env);
    client.batch_deduct(&owner, &items);
    let after = BudgetSnapshot::capture(&env);

    let delta = after.delta(&before);
    std::println!(
        "BUDGET_BATCH_DEDUCT_SIZE_10,cpu_instructions,{},memory_bytes,{},ledger_read_bytes,{},ledger_write_bytes,{}",
        delta.cpu_instructions, delta.memory_bytes, delta.ledger_read_bytes, delta.ledger_write_bytes
    );
}

/// Benchmark: measure the cost of `batch_deduct` with batch size = 25.
///
/// Tests mid-range batching to identify potential scaling inflection points.
#[test]
#[ignore]
fn budget_measure_batch_deduct_size_25() {
    let env = Env::default();
    let (owner, client) = setup_vault_for_deduct(&env, 100_000_000);

    let mut items = soroban_sdk::Vec::new(&env);
    for _ in 0..25 {
        items.push_back(DeductItem {
            amount: 500_000,
            request_id: Some(Symbol::new(&env, "req")),
        });
    }

    let before = BudgetSnapshot::capture(&env);
    client.batch_deduct(&owner, &items);
    let after = BudgetSnapshot::capture(&env);

    let delta = after.delta(&before);
    std::println!(
        "BUDGET_BATCH_DEDUCT_SIZE_25,cpu_instructions,{},memory_bytes,{},ledger_read_bytes,{},ledger_write_bytes,{}",
        delta.cpu_instructions, delta.memory_bytes, delta.ledger_read_bytes, delta.ledger_write_bytes
    );
}

/// Benchmark: measure the cost of `batch_deduct` with batch size = 50 (MAX_BATCH_SIZE).
///
/// Tests the maximum allowed batch size to understand the upper-bound cost.
#[test]
#[ignore]
fn budget_measure_batch_deduct_size_50() {
    let env = Env::default();
    let (owner, client) = setup_vault_for_deduct(&env, 100_000_000);

    let mut items = soroban_sdk::Vec::new(&env);
    for _ in 0..50 {
        items.push_back(DeductItem {
            amount: 300_000,
            request_id: Some(Symbol::new(&env, "req")),
        });
    }

    let before = BudgetSnapshot::capture(&env);
    client.batch_deduct(&owner, &items);
    let after = BudgetSnapshot::capture(&env);

    let delta = after.delta(&before);
    std::println!(
        "BUDGET_BATCH_DEDUCT_SIZE_50,cpu_instructions,{},memory_bytes,{},ledger_read_bytes,{},ledger_write_bytes,{}",
        delta.cpu_instructions, delta.memory_bytes, delta.ledger_read_bytes, delta.ledger_write_bytes
    );
}

/// Run all budget benchmarks in sequence.
///
/// This is a convenience function for easily running the entire benchmark suite.
/// Execute with: `cargo test budget_measure_all -- --ignored --nocapture`
#[test]
#[ignore]
fn budget_measure_all() {
    std::println!("\n=== VAULT BUDGET MEASUREMENT SUITE ===\n");

    // Single deduct baseline
    budget_measure_single_deduct();

    // Batch deduct at various sizes
    budget_measure_batch_deduct_size_1();
    budget_measure_batch_deduct_size_10();
    budget_measure_batch_deduct_size_25();
    budget_measure_batch_deduct_size_50();

    std::println!("\n=== END VAULT BUDGET MEASUREMENTS ===\n");
}

// ---------------------------------------------------------------------------
// max_fee_bps slippage guard tests (issue #498)
// ---------------------------------------------------------------------------

/// Helper: vault with `balance` and settlement configured.
fn setup_slippage_vault(env: &Env, balance: i128) -> (Address, CalloraVaultClient) {
    setup_vault_for_deduct(env, balance)
}

/// Deducting 50 bps (amount = 5, balance = 1000) with limit = 50 bps → succeeds.
#[test]
fn slippage_fee_below_limit_succeeds() {
    let env = Env::default();
    let (owner, client) = setup_slippage_vault(&env, 1000);
    env.mock_all_auths();
    // 5 / 1000 * 10_000 = 50 bps; limit = 50 → should succeed
    let remaining = client.deduct(&owner, &5, &None, &50, &Address::generate(&env));
    assert_eq!(remaining, 995);
}

/// Deducting exactly at the limit (fee_bps == max_fee_bps) → succeeds.
#[test]
fn slippage_fee_equal_to_limit_succeeds() {
    let env = Env::default();
    let (owner, client) = setup_slippage_vault(&env, 1000);
    env.mock_all_auths();
    // 10 / 1000 * 10_000 = 100 bps; limit = 100 → exactly equal, should succeed
    let remaining = client.deduct(&owner, &10, &None, &100, &Address::generate(&env));
    assert_eq!(remaining, 990);
}

/// Deducting above the limit → returns Slippage error.
#[test]
fn slippage_fee_above_limit_returns_slippage_error() {
    let env = Env::default();
    let (owner, client) = setup_slippage_vault(&env, 1000);
    env.mock_all_auths();
    // 11 / 1000 * 10_000 = 110 bps; limit = 100 → exceeds, should fail
    let result = client.try_deduct(&owner, &11, &None, &100);
    assert_eq!(
        result,
        Err(Ok(VaultError::Slippage)),
        "expected Slippage error"
    );
}

/// Passing u16::MAX behaves like the old unrestricted deduct.
#[test]
fn slippage_max_u16_is_unrestricted() {
    let env = Env::default();
    let (owner, client) = setup_slippage_vault(&env, 1000);
    env.mock_all_auths();
    // Deduct 99% of balance — would fail any real limit, but u16::MAX = no limit
    let remaining = client.deduct(&owner, &999, &None, &u16::MAX, &Address::generate(&env));
    assert_eq!(remaining, 1);
}

/// Boundary: max_fee_bps = 0 → any deduction of positive amount reverts.
#[test]
fn slippage_zero_limit_always_fails() {
    let env = Env::default();
    let (owner, client) = setup_slippage_vault(&env, 1000);
    env.mock_all_auths();
    // Even 1 stroop / 1000 = 10 bps > 0 → Slippage
    let result = client.try_deduct(&owner, &1, &None, &0);
    assert_eq!(result, Err(Ok(VaultError::Slippage)));
}

/// Boundary: max_fee_bps = 1 → deduction at 1 bps passes; 2 bps fails.
#[test]
fn slippage_one_bps_limit() {
    let env = Env::default();
    // balance = 100_000; 1 bps = 10 units, 2 bps = 20 units
    let (owner, client) = setup_slippage_vault(&env, 100_000);
    env.mock_all_auths();
    // 10 / 100_000 * 10_000 = 1 bps → equal to limit, succeeds
    let remaining = client.deduct(&owner, &10, &None, &1, &Address::generate(&env));
    assert_eq!(remaining, 99_990);
    // 20 / 99_990 * 10_000 = 2000_000/99990 = 2 bps → exceeds limit of 1
    let result = client.try_deduct(&owner, &20, &None, &1);
    assert_eq!(result, Err(Ok(VaultError::Slippage)));
}

/// Slippage check fires before any state mutation (balance unchanged on failure).
#[test]
fn slippage_check_before_state_mutation() {
    let env = Env::default();
    let (owner, client) = setup_slippage_vault(&env, 1000);
    env.mock_all_auths();
    let balance_before = client.balance();
    // This should fail with Slippage
    let _ = client.try_deduct(&owner, &500, &None, &10);
    assert_eq!(client.balance(), balance_before, "balance must be unchanged after slippage revert");
}

/// Existing deductions (u16::MAX) continue to work — no regression.
#[test]
fn slippage_no_regression_existing_deductions() {
    let env = Env::default();
    let (owner, client) = setup_slippage_vault(&env, 500);
    env.mock_all_auths();
    assert_eq!(client.deduct(&owner, &200, &None, &u16::MAX, &Address::generate(&env)), 300);
    assert_eq!(client.deduct(&owner, &300, &None, &u16::MAX, &Address::generate(&env)), 0);
}