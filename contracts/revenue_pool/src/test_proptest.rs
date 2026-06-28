
extern crate std;

use crate::{RevenuePool, RevenuePoolClient, Severity};
use proptest::prelude::*;
use proptest::strategy::ValueTree;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{self, StellarAssetClient};
use soroban_sdk::{Address, Env};
use soroban_sdk::Vec as SorobanVec;
use std::panic::{catch_unwind, AssertUnwindSafe};

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, StellarAssetClient<'a>) {
    let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract_address.address();
    let client = token::Client::new(env, &address);
    let admin_client = StellarAssetClient::new(env, &address);
    (address, client, admin_client)
}

fn create_pool(env: &Env) -> (Address, RevenuePoolClient<'_>) {
    let address = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &address);
    (address, client)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn batch_distribute_duplicate_recipient_handling(
        recipients in prop::collection::vec(any::<u8>(), 1..=50),
        amounts in prop::collection::vec(1_i128..=1_000_000_i128, 1..=50)
    ) {
        // Ensure recipients and amounts vectors are the same length
        let min_len = recipients.len().min(amounts.len());
        let recipients = &recipients[..min_len];
        let amounts = &amounts[..min_len];

        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let (pool_addr, pool) = create_pool(&env);
        let (usdc_addr, usdc, usdc_admin) = create_usdc(&env, &admin);
        pool.init(&admin, &usdc_addr);

        // Create a pool of developer addresses
        let dev_pool: std::vec::Vec<Address> = (0..20).map(|_| Address::generate(&env)).collect();

        // Build the payments vector
        let mut payments = SorobanVec::new(&env);
        let mut seen = std::vec::Vec::new();
        let mut has_duplicates = false;

        for (r, a) in recipients.iter().zip(amounts.iter()) {
            let dev = &dev_pool[(*r as usize) % dev_pool.len()];
            if seen.contains(dev) {
                has_duplicates = true;
            }
            seen.push(dev.clone());
            payments.push_back((dev.clone(), *a));
        }

        // Fund the pool with enough USDC
        let total_amount: i128 = payments.iter().map(|p| p.1).sum();
        usdc_admin.mint(&pool_addr, &total_amount);

        // Track balance before
        let balance_before = usdc.balance(&pool_addr);

        // Call batch_distribute
        let result = catch_unwind(AssertUnwindSafe(|| {
            pool.batch_distribute(&admin, &payments);
        }));

        // Check invariants
        let balance_after = usdc.balance(&pool_addr);

        if has_duplicates {
            // Should have panicked with duplicate recipient
            prop_assert!(result.is_err());
            // Balance should not have changed
            prop_assert_eq!(balance_before, balance_after);
        } else {
            // Should have succeeded
            prop_assert!(result.is_ok());
            // Balance should have decreased by total_amount
            prop_assert_eq!(balance_after, balance_before - total_amount);
        }
    }
}

// ---------------------------------------------------------------------------
// Stateful testing harness
// ---------------------------------------------------------------------------

/// Generate a list of valid actions and run them
proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn stateful_invariant_runner(
        seeds in prop::collection::vec(any::<u64>(), 1..20)
    ) {
        const DEV_COUNT: usize = 10;
        const ADMIN_COUNT: usize = 3;

        let env = Env::default();
        env.mock_all_auths();

        let admins: std::vec::Vec<Address> = (0..ADMIN_COUNT).map(|_| Address::generate(&env)).collect();
        let devs: std::vec::Vec<Address> = (0..DEV_COUNT).map(|_| Address::generate(&env)).collect();

        let (pool_addr, pool) = create_pool(&env);
        let (usdc_addr, usdc, usdc_admin) = create_usdc(&env, &admins[0]);

        pool.init(&admins[0], &usdc_addr);

        let mut paused = false;
        let mut admin_idx = 0;
        let mut pending_admin_idx = None;
        let mut max_distribute = i128::MAX;
        let mut virtual_scheduled = 0;

        for &seed in &seeds {
            // Simple PRNG from seed
            let mut rng = seed;
            let mut next_rand = || {
                rng = rng.wrapping_mul(1103515245).wrapping_add(12345);
                rng
            };

            let action_idx = next_rand() % 12;

            match action_idx {
                // Fund
                0 | 1 => {
                    let amount = (next_rand() % 10_000_000) as i128 + 1000;
                    usdc_admin.mint(&pool_addr, &amount);
                    virtual_scheduled += amount;
                }
                // Distribute
                2 | 3 if !paused && virtual_scheduled > 0 => {
                    let idx = (next_rand() % DEV_COUNT as u64) as usize;
                    let amount = std::cmp::min(
                        (next_rand() % 1_000_000) as i128 + 1,
                        std::cmp::min(virtual_scheduled, max_distribute)
                    );
                    let admin = &admins[admin_idx];
                    let recipient = &devs[idx];
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        pool.distribute(admin, recipient, &amount);
                    }));
                    if result.is_ok() {
                        virtual_scheduled -= amount;
                    }
                }
                // Batch distribute
                4 | 5 if !paused && virtual_scheduled > 0 => {
                    let batch_size = (next_rand() % 10) as usize + 1;
                    let mut payments = SorobanVec::new(&env);
                    let mut total = 0;
                    for _ in 0..batch_size {
                        let idx = (next_rand() % DEV_COUNT as u64) as usize;
                        let remaining = virtual_scheduled - total;
                        if remaining <= 0 {
                            break;
                        }
                        let amount = std::cmp::min(
                            (next_rand() % 100_000) as i128 + 1,
                            std::cmp::min(remaining, max_distribute)
                        );
                        payments.push_back((devs[idx].clone(), amount));
                        total += amount;
                    }
                    if payments.len() > 0 {
                        let admin = &admins[admin_idx];
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            pool.batch_distribute(admin, &payments);
                        }));
                        if result.is_ok() {
                            virtual_scheduled -= total;
                        }
                    }
                }
                // Pause
                6 if !paused => {
                    let admin = &admins[admin_idx];
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        pool.pause(admin);
                    }));
                    paused = true;
                }
                // Unpause
                7 if paused => {
                    let admin = &admins[admin_idx];
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        pool.unpause(admin);
                    }));
                    paused = false;
                }
                // Set max distribute
                8 => {
                    let new_max = (next_rand() % 100_000_000) as i128 + 1;
                    let admin = &admins[admin_idx];
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        pool.set_max_distribute(admin, &new_max);
                    }));
                    max_distribute = new_max;
                }
                // Admin transfer start
                9 if pending_admin_idx.is_none() => {
                    let new_admin_idx = (next_rand() % ADMIN_COUNT as u64) as usize;
                    let admin = &admins[admin_idx];
                    let new_admin = &admins[new_admin_idx];
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        pool.set_admin(admin, new_admin);
                    }));
                    pending_admin_idx = Some(new_admin_idx);
                }
                // Admin transfer accept/cancel
                10 if pending_admin_idx.is_some() => {
                    if next_rand() % 2 == 0 {
                        let idx = pending_admin_idx.unwrap();
                        let pending_admin = &admins[idx];
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            pool.accept_admin(pending_admin);
                        }));
                        admin_idx = idx;
                        pending_admin_idx = None;
                    } else {
                        let admin = &admins[admin_idx];
                        let _ = catch_unwind(AssertUnwindSafe(|| {
                            pool.cancel_admin_transfer(admin);
                        }));
                        pending_admin_idx = None;
                    }
                }
                // Receive payment
                11 => {
                    let amount = (next_rand() % 10_000_000) as i128 + 1000;
                    let from_vault = next_rand() % 2 == 0;
                    let admin = &admins[admin_idx];
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        pool.receive_payment(admin, &amount, &from_vault);
                    }));
                    virtual_scheduled += amount;
                }
                _ => {}
            }

            // Verify invariant
            let balance = usdc.balance(&pool_addr);
            prop_assert!(
                balance >= virtual_scheduled,
                "Invariant violated: balance {} < virtual_scheduled {}",
                balance,
                virtual_scheduled
            );
        }
    }
}
