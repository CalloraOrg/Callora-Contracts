//! Cross-contract cumulative conservation invariant tests.
//!
//! Verifies end-to-end that every unit deducted from the vault is exactly
//! accounted for in the settlement contract:
//!
//! ```text
//! vault.get_total_deducted() == settlement.get_total_received()
//! ```
//!
//! Runs 64 deterministic seeds × 48 steps each (3 072 invariant checks total).

extern crate std;

use soroban_sdk::{testutils::Address as _, token, Address, Env, Vec};

use super::*;
use callora_settlement::CalloraSettlement;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn setup(
    env: &Env,
    deposit: i128,
) -> (
    CalloraVaultClient<'_>,
    callora_settlement::CalloraSettlementClient<'_>,
    Address, // owner
) {
    let owner = Address::generate(env);
    let vault_addr = env.register(CalloraVault, ());
    let vault_client = CalloraVaultClient::new(env, &vault_addr);

    // Create USDC token.
    let usdc_addr = env.register_stellar_asset_contract_v2(owner.clone()).address();
    let usdc_admin = token::StellarAssetClient::new(env, &usdc_addr);

    // Fund vault on-ledger then init.
    usdc_admin.mint(&vault_addr, &deposit);
    env.mock_all_auths();
    vault_client.init(&owner, &usdc_addr, &Some(deposit), &None, &None, &None, &None);

    // Register settlement with vault as the trusted caller.
    let settle_addr = env.register(CalloraSettlement, ());
    let settle_client = callora_settlement::CalloraSettlementClient::new(env, &settle_addr);
    settle_client.init(&owner, &vault_addr);

    // Point vault at settlement.
    vault_client.set_settlement(&owner, &settle_addr);

    (vault_client, settle_client, owner)
}

/// Pseudo-random u64 (xorshift64).
#[inline]
fn next_rand(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

// ---------------------------------------------------------------------------
// 64 seeds × 48 steps — conservation check after every step
// ---------------------------------------------------------------------------

#[test]
fn cross_contract_conservation_ladder() {
    const NUM_SEEDS: u64 = 64;
    const STEPS: u64 = 48;
    const DEPOSIT: i128 = 1_000_000;

    for base_seed in 0..NUM_SEEDS {
        let env = Env::default();
        env.mock_all_auths();

        let (vault, settle, owner) = setup(&env, DEPOSIT);
        let mut seed: u64 = (base_seed + 1).wrapping_mul(0x9e3779b97f4a7c15);
        let mut total_deducted: i128 = 0;

        for step in 0..STEPS {
            let amount = ((next_rand(&mut seed) % 10_000) + 1) as i128;

            if vault.balance() >= amount {
                if step % 2 == 0 {
                    vault.deduct(&owner, &amount, &None, &u16::MAX);
                } else {
                    let items = Vec::from_array(
                        &env,
                        [DeductItem { amount, request_id: None }],
                    );
                    vault.batch_deduct(&owner, &items);
                }
                total_deducted += amount;
            }

            // Invariant 1: vault counter matches local accumulator.
            assert_eq!(
                vault.get_total_deducted(),
                total_deducted,
                "seed={base_seed} step={step}: vault total_deducted mismatch"
            );

            // Invariant 2: cross-contract conservation.
            assert_eq!(
                vault.get_total_deducted(),
                settle.get_total_received(),
                "seed={base_seed} step={step}: vault.get_total_deducted() != settlement.get_total_received()"
            );

            // Invariant 3: settlement internal conservation
            // (deducts always go to_pool=true so pool balance == total_received).
            let pool = settle.get_global_pool();
            assert_eq!(
                settle.get_total_received(),
                pool.total_balance,
                "seed={base_seed} step={step}: settlement internal conservation broken"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Unit: get_total_deducted
// ---------------------------------------------------------------------------

#[test]
fn get_total_deducted_starts_at_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (vault, _settle, _owner) = setup(&env, 1_000);
    assert_eq!(vault.get_total_deducted(), 0);
}

#[test]
fn get_total_deducted_after_single_deduct() {
    let env = Env::default();
    env.mock_all_auths();
    let (vault, _settle, owner) = setup(&env, 1_000);
    vault.deduct(&owner, &300, &None, &u16::MAX);
    assert_eq!(vault.get_total_deducted(), 300);
}

#[test]
fn get_total_deducted_after_batch_deduct() {
    let env = Env::default();
    env.mock_all_auths();
    let (vault, _settle, owner) = setup(&env, 1_000);
    let items = Vec::from_array(
        &env,
        [
            DeductItem { amount: 100, request_id: None },
            DeductItem { amount: 200, request_id: None },
        ],
    );
    vault.batch_deduct(&owner, &items);
    assert_eq!(vault.get_total_deducted(), 300);
}

// ---------------------------------------------------------------------------
// Unit: get_total_received
// ---------------------------------------------------------------------------

#[test]
fn get_total_received_starts_at_zero() {
    let env = Env::default();
    env.mock_all_auths();
    let (_vault, settle, _owner) = setup(&env, 1_000);
    assert_eq!(settle.get_total_received(), 0);
}

#[test]
fn get_total_received_after_deduct() {
    let env = Env::default();
    env.mock_all_auths();
    let (vault, settle, owner) = setup(&env, 1_000);
    vault.deduct(&owner, &400, &None, &u16::MAX);
    assert_eq!(settle.get_total_received(), 400);
}

#[test]
fn get_total_received_accumulates() {
    let env = Env::default();
    env.mock_all_auths();
    let (vault, settle, owner) = setup(&env, 1_000);
    vault.deduct(&owner, &100, &None, &u16::MAX);
    vault.deduct(&owner, &150, &None, &u16::MAX);
    vault.deduct(&owner, &250, &None, &u16::MAX);
    assert_eq!(settle.get_total_received(), 500);
    assert_eq!(vault.get_total_deducted(), 500);
}
