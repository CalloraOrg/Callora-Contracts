extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::token;
use soroban_sdk::TryFromVal;
use soroban_sdk::{Env, Symbol, TryIntoVal};

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

fn create_pool(env: &Env) -> (Address, RevenuePoolClient<'_>) {
    let address = env.register(RevenuePool, ());
    let client = RevenuePoolClient::new(env, &address);
    (address, client)
}

fn fund_pool(usdc_admin_client: &token::StellarAssetClient, pool_address: &Address, amount: i128) {
    usdc_admin_client.mint(pool_address, &amount);
}

#[test]
fn init_success() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_pool_addr, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.balance(), 0);
}

#[test]
#[should_panic(expected = "revenue pool already initialized")]
fn init_double_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.init(&admin, &usdc);
}

#[test]
fn distribute_success() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1_000);
    client.distribute(&admin, &developer, &400);

    assert_eq!(usdc_client.balance(&pool_addr), 600);
    assert_eq!(usdc_client.balance(&developer), 400);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn distribute_zero_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.distribute(&admin, &developer, &0);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn distribute_negative_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.distribute(&admin, &developer, &-1);
}

#[test]
#[should_panic(expected = "insufficient USDC balance")]
fn distribute_excess_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 100);
    client.distribute(&admin, &developer, &101);
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin")]
fn distribute_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 500);
    client.distribute(&attacker, &developer, &100);
}

#[test]
fn set_admin_transfers_control() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 300);
    client.set_admin(&admin, &new_admin);
    assert_eq!(client.get_admin(), new_admin);

    client.distribute(&new_admin, &developer, &100);
    assert_eq!(usdc_client.balance(&developer), 100);
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin")]
fn set_admin_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.set_admin(&attacker, &new_admin);
}

#[test]
fn receive_payment_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.receive_payment(&admin, &500, &true);
    let events = env.events().all();
    assert!(!events.is_empty());
}

#[test]
fn receive_payment_from_non_vault() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.receive_payment(&admin, &250, &false);

    let events = env.events().all();
    assert!(!events.is_empty());
}

// #[test]
// #[should_panic(expected = "revenue pool not initialized")]
// fn balance_before_init_panics() {
//     let env = Env::default();
//     let (_, client) = create_pool(&env);
//     client.balance();
// }

// #[test]
// fn distribute_negative_panics() {
//     let env = Env::default();
//     env.mock_all_auths();
//     let admin = Address::generate(&env);
//     let developer = Address::generate(&env);
//     let (_, client) = create_pool(&env);
//     let (usdc, _, _) = create_usdc(&env, &admin);

//     client.init(&admin, &usdc);
//     let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
//         client.distribute(&admin, &developer, &-1);
//     }));
//     assert!(result.is_err());
// }

// #[test]
// fn receive_payment_from_non_vault() {
//     let env = Env::default();
//     env.mock_all_auths();
//     let admin = Address::generate(&env);
//     let (_, client) = create_pool(&env);
//     let (usdc, _, _) = create_usdc(&env, &admin);

//     client.init(&admin, &usdc);
//     client.receive_payment(&admin, &250, &false);

//     let events = env.events().all();
//     assert!(!events.is_empty());
// }

/// Full lifecycle test: init, get_admin, balance, distribute, receive_payment, set_admin.
#[test]
fn full_lifecycle() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    assert_eq!(client.get_admin(), admin);

    fund_pool(&usdc_admin, &pool_addr, 1000);
    assert_eq!(client.balance(), 1000);

    client.distribute(&admin, &developer, &400);
    assert_eq!(usdc_client.balance(&developer), 400);
    assert_eq!(client.balance(), 600);

    client.receive_payment(&admin, &100, &true);

    client.set_admin(&admin, &new_admin);
    assert_eq!(client.get_admin(), new_admin);

    client.distribute(&new_admin, &developer, &100);
    assert_eq!(usdc_client.balance(&developer), 500);
    assert_eq!(client.balance(), 500);
}

#[test]
fn batch_distribute_success() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1000);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 300_i128));
    payments.push_back((dev2.clone(), 200_i128));
    client.batch_distribute(&admin, &payments);

    assert_eq!(usdc_client.balance(&dev1), 300);
    assert_eq!(usdc_client.balance(&dev2), 200);
    assert_eq!(client.balance(), 500);
}

// /// Full lifecycle test: init, get_admin, balance, distribute, receive_payment, set_admin.
// #[test]
// fn full_lifecycle() {
//     let env = Env::default();
//     env.mock_all_auths();
//     let admin = Address::generate(&env);
//     let new_admin = Address::generate(&env);
//     let developer = Address::generate(&env);
//     let attacker = Address::generate(&env);
//     let dev = Address::generate(&env);
//     let (pool_addr, client) = create_pool(&env);
//     let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

//     // Init
//     client.init(&admin, &usdc_address);
//     assert_eq!(client.get_admin(), admin);

//     // Fund and check balance
//     fund_pool(&usdc_admin, &pool_addr, 1000);
//     assert_eq!(client.balance(), 1000);

//     // Distribute
//     client.distribute(&admin, &developer, &400);
//     assert_eq!(usdc_client.balance(&developer), 400);
//     assert_eq!(client.balance(), 600);

//     // Receive payment event
//     client.receive_payment(&admin, &100, &true);

//     // Set admin
//     client.set_admin(&admin, &new_admin);
//     assert_eq!(client.get_admin(), new_admin);

//     // New admin can distribute
//     client.distribute(&new_admin, &developer, &100);
//     assert_eq!(usdc_client.balance(&developer), 500);
//     assert_eq!(client.balance(), 500);
//     fund_pool(&usdc_admin, &pool_addr, 500);

//     let mut payments: Vec<(Address, i128)> = Vec::new(&env);
//     payments.push_back((dev.clone(), 100_i128));
//     client.batch_distribute(&attacker, &payments);
// }

#[test]
#[should_panic(expected = "amount must be positive")]
fn batch_distribute_zero_amount_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 500);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev.clone(), 0_i128));
    client.batch_distribute(&admin, &payments);
}

#[test]
#[should_panic(expected = "insufficient USDC balance")]
fn batch_distribute_insufficient_balance_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 50);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev.clone(), 100_i128));
    client.batch_distribute(&admin, &payments);
}

// #[test]
// fn batch_distribute_success() {
//     let env = Env::default();
//     env.mock_all_auths();
//     let admin = Address::generate(&env);
//     let dev1 = Address::generate(&env);
//     let dev2 = Address::generate(&env);
//     let (pool_addr, client) = create_pool(&env);
//     let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

//     client.init(&admin, &usdc_address);
//     fund_pool(&usdc_admin, &pool_addr, 1_000);

//     let payments = soroban_sdk::vec![&env, (dev1.clone(), 300_i128), (dev2.clone(), 200_i128)];
//     client.batch_distribute(&admin, &payments);

//     assert_eq!(usdc_client.balance(&dev1), 300);
//     assert_eq!(usdc_client.balance(&dev2), 200);
//     assert_eq!(usdc_client.balance(&pool_addr), 500);
// }

#[test]
#[should_panic(expected = "unauthorized: caller is not admin")]
fn batch_distribute_unauthorized_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let dev = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 500);

    let payments = soroban_sdk::vec![&env, (dev.clone(), 100_i128)];
    client.batch_distribute(&attacker, &payments);
}

// #[test]
// #[should_panic(expected = "amount must be positive")]
// fn batch_distribute_zero_amount_panics() {
//     let env = Env::default();
//     env.mock_all_auths();
//     let admin = Address::generate(&env);
//     let dev = Address::generate(&env);
//     let (pool_addr, client) = create_pool(&env);
//     let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

//     client.init(&admin, &usdc_address);
//     fund_pool(&usdc_admin, &pool_addr, 500);

//     let payments = soroban_sdk::vec![&env, (dev.clone(), 0_i128)];
//     client.batch_distribute(&admin, &payments);
// }

// #[test]
// #[should_panic(expected = "insufficient USDC balance")]
// fn batch_distribute_insufficient_balance_panics() {
//     let env = Env::default();
//     env.mock_all_auths();
//     let admin = Address::generate(&env);
//     let dev = Address::generate(&env);
//     let (pool_addr, client) = create_pool(&env);
//     let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

//     client.init(&admin, &usdc_address);
//     fund_pool(&usdc_admin, &pool_addr, 100);

//     let payments = soroban_sdk::vec![&env, (dev.clone(), 200_i128)];
//     client.batch_distribute(&admin, &payments);
// }

#[test]
fn batch_distribute_success_events() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let dev3 = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1000);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 300_i128));
    payments.push_back((dev2.clone(), 200_i128));
    payments.push_back((dev3.clone(), 400_i128));
    client.batch_distribute(&admin, &payments);

    let events = env.events().all();

    // SHould capture Transfer and batch_distribute events
    assert!(
        events.len() == 6,
        "No events captured immediately after batch_distribute!"
    );

    for i in 0..events.len() {
        let (_event_contract_id, topics, data) = events.get(i).unwrap();
        let event_name: Symbol = topics.get(0).unwrap().try_into_val(&env).unwrap();

        let value: i128 = i128::try_from_val(&env, &data).unwrap();

        // Should Capture 3 batch_distribute events
        // Events emittedfollows the order of payments vec
        if event_name == Symbol::new(&env, "batch_distribute") {
            let dev_address: Address = topics.get(1).unwrap().try_into_val(&env).unwrap();

            assert_eq!(dev_address, payments.get(i / 2).unwrap().0);
            assert_eq!(value, payments.get(i / 2).unwrap().1);
        }

        // //Should Capture 3 transfer events
        if event_name == Symbol::new(&env, "transfer") {
            let dev_address: Address = topics.get(2).unwrap().try_into_val(&env).unwrap();

            assert_eq!(dev_address, payments.get(i / 2).unwrap().0);
            assert_eq!(value, payments.get(i / 2).unwrap().1);
        }
    }
}
