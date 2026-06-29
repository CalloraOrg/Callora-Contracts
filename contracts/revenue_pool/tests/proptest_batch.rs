extern crate std;

use callora_revenue_pool::{RevenuePool, RevenuePoolClient, MAX_BATCH_SIZE};
use proptest::prelude::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{self, StellarAssetClient};
use soroban_sdk::{Address, Env, Vec as SorobanVec};

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
    fn batch_distribute_preserves_total_and_per_recipient_amounts(
        amounts in prop::collection::vec(1_i128..=1_000_000_000_i128, 1..=MAX_BATCH_SIZE as usize)
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let (pool_addr, pool) = create_pool(&env);
        let (usdc_addr, usdc, usdc_admin) = create_usdc(&env, &admin);
        pool.init(&admin, &usdc_addr);

        let recipients: std::vec::Vec<Address> = amounts
            .iter()
            .map(|_| Address::generate(&env))
            .collect();
        let mut payments = SorobanVec::new(&env);
        let mut total_amount = 0_i128;

        for (recipient, amount) in recipients.iter().zip(amounts.iter()) {
            payments.push_back((recipient.clone(), *amount));
            total_amount += *amount;
        }

        usdc_admin.mint(&pool_addr, &total_amount);

        let pool_balance_before = usdc.balance(&pool_addr);
        let recipient_balances_before: std::vec::Vec<i128> = recipients
            .iter()
            .map(|recipient| usdc.balance(recipient))
            .collect();

        let result = pool.try_batch_distribute(&admin, &payments);
        prop_assert!(result.is_ok());
        prop_assert_eq!(pool_balance_before, total_amount);
        prop_assert_eq!(usdc.balance(&pool_addr), pool_balance_before - total_amount);

        let mut total_recipient_delta = 0_i128;
        for ((recipient, expected_amount), balance_before) in recipients
            .iter()
            .zip(amounts.iter())
            .zip(recipient_balances_before.iter())
        {
            let balance_after = usdc.balance(recipient);
            let delta = balance_after - *balance_before;
            prop_assert_eq!(delta, *expected_amount);
            total_recipient_delta += delta;
        }

        prop_assert_eq!(total_recipient_delta, total_amount);
    }
}
