extern crate std;

use callora_revenue_pool::{RevenuePool, RevenuePoolClient, MAX_BATCH_SIZE};
use proptest::prelude::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{self, StellarAssetClient};
use soroban_sdk::{Address, Env, Vec as SorobanVec};
use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Clone, Debug)]
enum PoolAction {
    Fund(i128),
    Schedule(i128),
    Distribute {
        recipient_idx: usize,
        amount: i128,
    },
    BatchDistribute {
        start_idx: usize,
        amounts: std::vec::Vec<i128>,
    },
    Pause,
    Unpause,
    SetMaxDistribute(i128),
}

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

fn pool_action_strategy() -> impl Strategy<Value = PoolAction> {
    prop_oneof![
        3 => (1_i128..=1_000_000_i128).prop_map(PoolAction::Fund),
        4 => (1_i128..=1_000_000_i128).prop_map(PoolAction::Schedule),
        4 => (0_usize..10, 1_i128..=250_000_i128)
            .prop_map(|(recipient_idx, amount)| PoolAction::Distribute { recipient_idx, amount }),
        2 => (
            0_usize..10,
            prop::collection::vec(1_i128..=100_000_i128, 1..=MAX_BATCH_SIZE as usize)
        )
            .prop_map(|(start_idx, amounts)| PoolAction::BatchDistribute { start_idx, amounts }),
        1 => Just(PoolAction::Pause),
        1 => Just(PoolAction::Unpause),
        2 => (1_i128..=500_000_i128).prop_map(PoolAction::SetMaxDistribute),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    fn stateful_actions_preserve_scheduled_balance_invariant(
        actions in prop::collection::vec(pool_action_strategy(), 1..=64)
    ) {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let (pool_addr, pool) = create_pool(&env);
        let (usdc_addr, usdc, usdc_admin) = create_usdc(&env, &admin);
        let recipients: std::vec::Vec<Address> =
            (0..10).map(|_| Address::generate(&env)).collect();

        pool.init(&admin, &usdc_addr);

        let mut scheduled = 0_i128;
        let mut max_distribute = i128::MAX;

        for action in actions {
            match action {
                PoolAction::Fund(amount) => {
                    usdc_admin.mint(&pool_addr, &amount);
                }
                PoolAction::Schedule(amount) => {
                    usdc_admin.mint(&pool_addr, &amount);
                    scheduled += amount;
                }
                PoolAction::Distribute {
                    recipient_idx,
                    amount,
                } => {
                    let amount = amount.min(scheduled).min(max_distribute);
                    if amount > 0 {
                        let recipient = &recipients[recipient_idx % recipients.len()];
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            pool.distribute(&admin, recipient, &amount);
                        }));
                        if result.is_ok() {
                            scheduled -= amount;
                        }
                    }
                }
                PoolAction::BatchDistribute { start_idx, amounts } => {
                    let mut remaining = scheduled;
                    let mut total = 0_i128;
                    let mut payments = SorobanVec::new(&env);

                    for (offset, generated_amount) in amounts.iter().enumerate() {
                        if remaining <= 0 {
                            break;
                        }
                        let amount = (*generated_amount).min(remaining).min(max_distribute);
                        if amount <= 0 {
                            break;
                        }

                        let recipient = recipients[(start_idx + offset) % recipients.len()].clone();
                        payments.push_back((recipient, amount));
                        remaining -= amount;
                        total += amount;
                    }

                    if !payments.is_empty() {
                        let result = catch_unwind(AssertUnwindSafe(|| {
                            pool.batch_distribute(&admin, &payments);
                        }));
                        if result.is_ok() {
                            scheduled -= total;
                        }
                    }
                }
                PoolAction::Pause => {
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        pool.pause(&admin);
                    }));
                }
                PoolAction::Unpause => {
                    let _ = catch_unwind(AssertUnwindSafe(|| {
                        pool.unpause(&admin);
                    }));
                }
                PoolAction::SetMaxDistribute(new_max) => {
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        pool.set_max_distribute(&admin, &new_max);
                    }));
                    if result.is_ok() {
                        max_distribute = new_max;
                    }
                }
            }

            prop_assert!(
                usdc.balance(&pool_addr) >= scheduled,
                "pool balance dropped below scheduled liabilities"
            );
            prop_assert!(scheduled >= 0, "scheduled liabilities went negative");
        }
    }
}
