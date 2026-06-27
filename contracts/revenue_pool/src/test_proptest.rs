
extern crate std;

use crate::{RevenuePool, RevenuePoolClient};
use proptest::prelude::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{self, StellarAssetClient};
use soroban_sdk::{Address, Env, Vec};
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
        let mut payments = Vec::new(&env);
        let mut seen = std::collections::HashSet::new();
        let mut has_duplicates = false;

        for (r, a) in recipients.iter().zip(amounts.iter()) {
            let dev = &dev_pool[(*r as usize) % dev_pool.len()];
            if seen.contains(dev) {
                has_duplicates = true;
            }
            seen.insert(dev.clone());
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
