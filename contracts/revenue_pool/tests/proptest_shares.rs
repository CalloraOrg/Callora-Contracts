extern crate std;

use callora_revenue_pool::{RevenuePool, RevenuePoolClient};
use proptest::prelude::*;
use soroban_sdk::testutils::Address as _;
use soroban_sdk::token::{self, StellarAssetClient};
use soroban_sdk::{Address, Env, Vec as SorobanVec};

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, StellarAssetClient<'a>) {
    let contract = env.register_stellar_asset_contract_v2(admin.clone());
    let address = contract.address();
    (
        address.clone(),
        token::Client::new(env, &address),
        StellarAssetClient::new(env, &address),
    )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn share_distribution_preserves_total(
        total in 1_i128..=1_000_000_000_i128,
        weights in prop::collection::vec(1_u32..=100_u32, 1..=50)
    ) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let pool = env.register(RevenuePool, ());
        let client = RevenuePoolClient::new(&env, &pool);
        let (usdc_address, usdc, usdc_admin) = create_usdc(&env, &admin);
        client.init(&admin, &usdc_address);

        let weight_sum: u64 = weights.iter().map(|weight| *weight as u64).sum();
        let mut shares = SorobanVec::new(&env);
        let mut recipients = std::vec::Vec::<Address>::new();
        let mut normalized = std::vec::Vec::<u32>::new();

        for weight in weights.iter() {
            let mut bps = ((*weight as u64 * 10_000) / weight_sum) as u32;
            if bps == 0 {
                bps = 1;
            }
            normalized.push(bps);
        }

        let mut bps_sum: u32 = normalized.iter().sum();
        if bps_sum < 10_000 {
            let remainder = 10_000 - bps_sum;
            normalized[0] = normalized[0] + remainder;
        } else if bps_sum > 10_000 {
            let mut excess = bps_sum - 10_000;
            for bps in normalized.iter_mut().rev() {
                if *bps > excess {
                    *bps -= excess;
                    break;
                }
                let removable = *bps - 1;
                if removable >= excess {
                    *bps -= excess;
                    break;
                }
                excess = excess.saturating_sub(removable);
                *bps = 1;
            }
        }

        bps_sum = normalized.iter().sum();
        prop_assert_eq!(bps_sum, 10_000);

        for bps in normalized {
            let recipient = Address::generate(&env);
            recipients.push(recipient.clone());
            shares.push_back((recipient, bps));
        }

        prop_assert_eq!(bps_sum, 10_000);

        usdc_admin.mint(&pool, &total);
        let pool_before = usdc.balance(&pool);
        let recipient_before: std::vec::Vec<i128> = recipients
            .iter()
            .map(|recipient| usdc.balance(recipient))
            .collect();

        let result = client.try_distribute_by_shares(&admin, &total, &shares);
        prop_assert!(result.is_ok());
        prop_assert_eq!(usdc.balance(&pool), pool_before - total);

        let mut recipient_delta = 0_i128;
        for (recipient, balance_before) in recipients.iter().zip(recipient_before.iter()) {
            let delta = usdc.balance(recipient) - *balance_before;
            recipient_delta += delta;
        }
        prop_assert_eq!(recipient_delta, total);
    }
}
