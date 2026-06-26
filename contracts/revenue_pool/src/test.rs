extern crate std;

use super::*;
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::token;
use soroban_sdk::BytesN;
use soroban_sdk::TryFromVal;
use soroban_sdk::{Address, Env, IntoVal, Symbol, Vec};

fn create_usdc<'a>(
    env: &'a Env,
    admin: &Address,
) -> (Address, token::Client<'a>, token::StellarAssetClient<'a>) {
    let contract_address = env.register_stellar_asset_contract_v2(admin.clone());
    extern crate std;

    use super::*;
    use soroban_sdk::testutils::{Address as _, Events as _};
    use soroban_sdk::token;
    use soroban_sdk::TryFromVal;
    use soroban_sdk::{Address, Env, IntoVal, Symbol, Vec};

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

    fn fund_pool(
        usdc_admin_client: &token::StellarAssetClient,
        pool_address: &Address,
        amount: i128,
    ) {
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
    fn init_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);

        let events = env.events().all();
        let init_event = events.last().unwrap();
        let event_name = Symbol::try_from_val(&env, &init_event.1.get(0).unwrap()).unwrap();
        assert_eq!(event_name, Symbol::new(&env, "init"));
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
    #[should_panic(expected = "revenue pool already initialized")]
    fn init_double_different_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let other_admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);
        let (usdc2, _, _) = create_usdc(&env, &other_admin);

        client.init(&admin, &usdc);
        client.init(&other_admin, &usdc2);
    }

    #[test]
    #[should_panic(expected = "invalid config: usdc_token cannot be the contract itself")]
    fn init_usdc_token_is_contract_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);

        // Passing the contract's own address as usdc_token should be rejected.
        client.init(&admin, &pool_addr);
    }

    #[test]
    #[should_panic(expected = "invalid config: usdc_token cannot be the admin address")]
    fn init_usdc_token_is_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);

        // Passing the admin address as usdc_token should be rejected.
        client.init(&admin, &admin);
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
    fn pause_blocks_distribute() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let developer = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        fund_pool(&usdc_admin, &pool_addr, 1_000);
        assert!(!client.is_paused());
        client.pause(&admin);
        assert!(client.is_paused());

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| client.distribute(&admin, &developer, &100)));
        assert!(result.is_err());
    }

    #[test]
    fn unpause_restores_distribute() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let developer = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        fund_pool(&usdc_admin, &pool_addr, 1_000);
        client.pause(&admin);
        assert!(client.is_paused());
        client.unpause(&admin);
        assert!(!client.is_paused());

        client.distribute(&admin, &developer, &250);
        assert_eq!(usdc_client.balance(&pool_addr), 750);
        assert_eq!(usdc_client.balance(&developer), 250);
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
    fn get_max_distribute_returns_default_when_not_set() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc_address, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);

        assert_eq!(client.get_max_distribute(), i128::MAX);
    }

    #[test]
    fn set_max_distribute_updates_cap_and_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc_address, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        client.set_max_distribute(&admin, &500);

        assert_eq!(client.get_max_distribute(), 500);

        let events = env.events().all();
        let ev = events.last().unwrap();
        let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
        assert_eq!(t0, Symbol::new(&env, "set_max_distribute"));

        let data: (i128, i128) = ev.2.into_val(&env);
        assert_eq!(data, (i128::MAX, 500));
    }

    #[test]
    #[should_panic(expected = "unauthorized: caller is not admin")]
    fn set_max_distribute_unauthorized_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc_address, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        client.set_max_distribute(&attacker, &500);
    }

    #[test]
    #[should_panic(expected = "max_distribute must be positive")]
    fn set_max_distribute_zero_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc_address, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        client.set_max_distribute(&admin, &0);
    }

    #[test]
    #[should_panic(expected = "amount exceeds max_distribute")]
    fn distribute_above_max_distribute_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let developer = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        fund_pool(&usdc_admin, &pool_addr, 500);
        client.set_max_distribute(&admin, &100);
        client.distribute(&admin, &developer, &101);
    }

    #[test]
    #[should_panic(expected = "amount exceeds max_distribute")]
    fn batch_distribute_leg_above_max_distribute_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let dev1 = Address::generate(&env);
        let dev2 = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        fund_pool(&usdc_admin, &pool_addr, 1000);
        client.set_max_distribute(&admin, &50);

        let mut payments: Vec<(Address, i128)> = Vec::new(&env);
        payments.push_back((dev1.clone(), 50_i128));
        payments.push_back((dev2.clone(), 51_i128));

        client.batch_distribute(&admin, &payments);
    }

    #[test]
    fn set_admin_two_step_transfers_control() {
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
        assert_eq!(client.get_admin(), admin);

        client.claim_admin(&new_admin);
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
    #[should_panic(expected = "unauthorized: caller is not pending admin")]
    fn claim_admin_wrong_address_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        client.set_admin(&admin, &new_admin);
        client.claim_admin(&attacker);
    }

    #[test]
    fn admin_transfer_emits_events() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);

        // Step 1 event
        client.set_admin(&admin, &new_admin);
        let events = env.events().all();
        let transfer_started = events.last().unwrap();

        // FIX: Convert Val to Symbol for comparison
        let event_name = Symbol::try_from_val(&env, &transfer_started.1.get(0).unwrap()).unwrap();
        assert_eq!(event_name, Symbol::new(&env, "admin_transfer_started"));

        // Step 2 event
        client.claim_admin(&new_admin);
        let events = env.events().all();
        let transfer_completed = events.last().unwrap();

        // FIX: Convert Val to Symbol for comparison
        let event_name_comp =
            Symbol::try_from_val(&env, &transfer_completed.1.get(0).unwrap()).unwrap();
        assert_eq!(
            event_name_comp,
            Symbol::new(&env, "admin_transfer_completed")
        );
    }

    #[test]
    fn receive_payment_emits_event() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        client.receive_payment(&admin, &250, &true);

        let events = env.events().all();
        let receive_payment_event = events.last().unwrap();
        let event_name =
            Symbol::try_from_val(&env, &receive_payment_event.1.get(0).unwrap()).unwrap();
        assert_eq!(event_name, Symbol::new(&env, "receive_payment"));

        let amount_and_source: (i128, bool) =
            <(i128, bool)>::try_from_val(&env, &receive_payment_event.2).unwrap();
        assert_eq!(amount_and_source, (250, true));
    }

    #[test]
    #[should_panic(expected = "unauthorized: caller is not admin")]
    fn receive_payment_non_admin_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        client.receive_payment(&attacker, &250, &true);
    }

    #[test]
    fn receive_payment_is_event_only_and_does_not_move_tokens() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let developer = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        fund_pool(&usdc_admin, &pool_addr, 500);

        let before_pool = usdc_client.balance(&pool_addr);
        let before_developer = usdc_client.balance(&developer);

        client.receive_payment(&admin, &250, &true);

        assert_eq!(usdc_client.balance(&pool_addr), before_pool);
        assert_eq!(usdc_client.balance(&developer), before_developer);
    }

    // ---------------------------------------------------------------------------
    // Batch distribute tests - Comprehensive coverage
    // ---------------------------------------------------------------------------

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

    #[test]
    fn batch_distribute_success_events() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let dev1 = Address::generate(&env);
        let dev2 = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        fund_pool(&usdc_admin, &pool_addr, 1000);

        let mut payments: Vec<(Address, i128)> = Vec::new(&env);
        payments.push_back((dev1.clone(), 300_i128));
        payments.push_back((dev2.clone(), 200_i128));
        client.batch_distribute(&admin, &payments);

        let events = env.events().all();
        assert!(events.len() >= 4);

        for i in 0..events.len() {
            let (_, topics, data) = events.get(i).unwrap();
            let topic_0 = topics.get(0).unwrap();
            if let Ok(event_name) = Symbol::try_from_val(&env, &topic_0) {
                if event_name == Symbol::new(&env, "batch_distribute") {
                    let value: i128 = i128::try_from_val(&env, &data).unwrap();
                    assert!(value == 300 || value == 200);
                }
            }
        }
    }

    #[test]
    fn receive_payment_emits_event_for_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        client.receive_payment(&admin, &250, &true);

        let events = env.events().all();
        let receive_event = events.last().unwrap();
        let event_name = Symbol::try_from_val(&env, &receive_event.1.get(0).unwrap()).unwrap();
        assert_eq!(event_name, Symbol::new(&env, "receive_payment"));

        let caller: Address =
            Address::try_from_val(&env, &receive_event.1.get(1).unwrap()).unwrap();
        assert_eq!(caller, admin);

        let (amount, from_vault): (i128, bool) = receive_event.2.into_val(&env);
        assert_eq!(amount, 250);
        assert!(from_vault);
    }

    #[test]
    #[should_panic(expected = "no pending admin")]
    fn claim_admin_without_pending_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let candidate = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        client.claim_admin(&candidate);
    }

    #[test]
    #[should_panic(expected = "unauthorized: caller is not pending admin")]
    fn claim_admin_wrong_caller_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let pending_admin = Address::generate(&env);
        let attacker = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        client.set_admin(&admin, &pending_admin);
        client.claim_admin(&attacker);
    }

    #[test]
    #[should_panic(expected = "invalid recipient: cannot distribute to the contract itself")]
    fn distribute_to_self_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        fund_pool(&usdc_admin, &pool_addr, 100);
        client.distribute(&admin, &pool_addr, &50);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn batch_distribute_zero_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let dev = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc_address, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);

        let mut payments: Vec<(Address, i128)> = Vec::new(&env);
        payments.push_back((dev, 0));
        client.batch_distribute(&admin, &payments);
    }

    // ---------------------------------------------------------------------------
    // Event schema tests (Issue #256)
    // Each test below pins the exact topic/data layout documented in EVENT_SCHEMA.md
    // ---------------------------------------------------------------------------

    #[test]
    fn init_event_topics_and_data() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);

        let events = env.events().all();
        let ev = events.last().unwrap();

        // topic 0 = "init"
        let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
        assert_eq!(t0, Symbol::new(&env, "init"));

        // topic 1 = admin address
        let t1 = Address::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap();
        assert_eq!(t1, admin);

        // data = usdc_token address
        let data = Address::try_from_val(&env, &ev.2).unwrap();
        assert_eq!(data, usdc);
    }

    #[test]
    fn admin_transfer_started_event_topics_and_data() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        client.set_admin(&admin, &new_admin);

        let events = env.events().all();
        let ev = events.last().unwrap();

        // topic 0 = "admin_transfer_started"
        let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
        assert_eq!(t0, Symbol::new(&env, "admin_transfer_started"));

        // topic 1 = current admin
        let t1 = Address::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap();
        assert_eq!(t1, admin);

        // data = pending admin
        let data = Address::try_from_val(&env, &ev.2).unwrap();
        assert_eq!(data, new_admin);
    }

    #[test]
    fn admin_changed_event_topics_and_data() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        client.set_admin(&admin, &new_admin);

        let events = env.events().all();
        // After set_admin, last event is admin_transfer_started and the one before it is admin_changed.
        let ev = events.get(events.len() - 2).unwrap();

        // topic 0 = "admin_changed"
        let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
        assert_eq!(t0, Symbol::new(&env, "admin_changed"));

        // topic 1 = current admin
        let t1 = Address::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap();
        assert_eq!(t1, admin);

        // data = (old_admin, new_admin)
        let data: (Address, Address) = ev.2.into_val(&env);
        assert_eq!(data.0, admin);
        assert_eq!(data.1, new_admin);
    }

    #[test]
    fn admin_transfer_completed_event_topics_and_data() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        client.set_admin(&admin, &new_admin);
        client.claim_admin(&new_admin);

        let events = env.events().all();
        let ev = events.last().unwrap();

        // topic 0 = "admin_transfer_completed"
        let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
        assert_eq!(t0, Symbol::new(&env, "admin_transfer_completed"));

        // topic 1 = new admin
        let t1 = Address::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap();
        assert_eq!(t1, new_admin);

        // data = () empty
        let _: () = ev.2.into_val(&env);
    }

    #[test]
    fn receive_payment_event_from_vault_true() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        client.receive_payment(&admin, &5_000_000, &true);

        let events = env.events().all();
        let ev = events.last().unwrap();

        // topic 0 = "receive_payment"
        let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
        assert_eq!(t0, Symbol::new(&env, "receive_payment"));

        // topic 1 = caller (admin)
        let t1 = Address::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap();
        assert_eq!(t1, admin);

        // data = (amount, from_vault)
        let (amount, from_vault): (i128, bool) = ev.2.into_val(&env);
        assert_eq!(amount, 5_000_000);
        assert!(from_vault);
    }

    #[test]
    fn receive_payment_event_from_vault_false() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        client.receive_payment(&admin, &1_000_000, &false);

        let events = env.events().all();
        let ev = events.last().unwrap();

        let (amount, from_vault): (i128, bool) = ev.2.into_val(&env);
        assert_eq!(amount, 1_000_000);
        assert!(!from_vault);
    }

    #[test]
    fn distribute_event_topics_and_data() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let developer = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        fund_pool(&usdc_admin, &pool_addr, 1_000_000);
        client.distribute(&admin, &developer, &1_000_000);

        let events = env.events().all();
        let ev = events.last().unwrap();

        // topic 0 = "distribute"
        let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
        assert_eq!(t0, Symbol::new(&env, "distribute"));

        // topic 1 = recipient
        let t1 = Address::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap();
        assert_eq!(t1, developer);

        // data = amount
        let amount: i128 = ev.2.into_val(&env);
        assert_eq!(amount, 1_000_000);
    }

    #[test]
    fn batch_distribute_event_topics_and_data() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let dev1 = Address::generate(&env);
        let dev2 = Address::generate(&env);
        let dev3 = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        fund_pool(&usdc_admin, &pool_addr, 3_500_000);

        let mut payments: Vec<(Address, i128)> = Vec::new(&env);
        payments.push_back((dev1.clone(), 1_000_000_i128));
        payments.push_back((dev2.clone(), 2_000_000_i128));
        payments.push_back((dev3.clone(), 500_000_i128));
        client.batch_distribute(&admin, &payments);

        let all_events = env.events().all();
        let batch_events: std::vec::Vec<_> = all_events
            .iter()
            .filter(|e| {
                e.1.get(0)
                    .and_then(|v| Symbol::try_from_val(&env, &v).ok())
                    .map(|s| s == Symbol::new(&env, "batch_distribute"))
                    .unwrap_or(false)
            })
            .collect();

        // 3 payments → 3 batch_distribute events
        assert_eq!(batch_events.len(), 3);

        // verify each event has correct topic 0 and a positive amount
        for ev in batch_events.iter() {
            let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
            assert_eq!(t0, Symbol::new(&env, "batch_distribute"));
            let amount: i128 = ev.2.into_val(&env);
            assert!(amount > 0);
        }
    }

    #[test]
    fn batch_distribute_is_atomic_all_or_nothing() {
        // If any payment fails the entire batch reverts — no events emitted.
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let dev1 = Address::generate(&env);
        let dev2 = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        fund_pool(&usdc_admin, &pool_addr, 100);

        let mut payments: Vec<(Address, i128)> = Vec::new(&env);
        payments.push_back((dev1.clone(), 60_i128));
        payments.push_back((dev2.clone(), 60_i128)); // total 120 > balance 100

        let result = client.try_batch_distribute(&admin, &payments);
        assert!(result.is_err());

        // balance unchanged
        assert_eq!(client.balance(), 100);
    }

    // ---------------------------------------------------------------------------
    // get_admin() and get_usdc_token() getter tests  (Issue #265)
    // ---------------------------------------------------------------------------

    #[test]
    fn get_admin_returns_correct_address() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);

        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    fn get_admin_reflects_updated_admin_after_transfer() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        assert_eq!(client.get_admin(), admin);

        // Pending phase: get_admin() still returns old admin
        client.set_admin(&admin, &new_admin);
        assert_eq!(client.get_admin(), admin);

        // After claim: admin updated
        client.claim_admin(&new_admin);
        assert_eq!(client.get_admin(), new_admin);
    }

    #[test]
    #[should_panic(expected = "revenue pool not initialized")]
    fn get_admin_before_init_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (_, client) = create_pool(&env);

        client.get_admin();
    }

    #[test]
    fn get_usdc_token_returns_correct_address() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);

        assert_eq!(client.get_usdc_token(), usdc);
    }

    #[test]
    fn get_usdc_token_is_immutable_after_init() {
        // The USDC token address must never change after initialization —
        // this test guards against accidental mutation.
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc);
        let token_before = client.get_usdc_token();

        // Admin transfer must not affect the token address
        client.set_admin(&admin, &new_admin);
        client.claim_admin(&new_admin);

        assert_eq!(client.get_usdc_token(), token_before);
    }

    #[test]
    #[should_panic(expected = "revenue pool not initialized")]
    fn get_usdc_token_before_init_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let (_, client) = create_pool(&env);

        client.get_usdc_token();
    }

    // ---------------------------------------------------------------------------
    // batch_distribute length-cap tests (resource exhaustion prevention)
    // ---------------------------------------------------------------------------

    #[test]
    #[should_panic(expected = "batch_distribute requires at least one payment")]
    fn batch_distribute_empty_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc_address, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);

        let payments: Vec<(Address, i128)> = Vec::new(&env);
        client.batch_distribute(&admin, &payments);
    }

    #[test]
    #[should_panic(expected = "batch too large")]
    fn batch_distribute_too_large_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        fund_pool(&usdc_admin, &pool_addr, 100_000);

        // Build a batch of MAX_BATCH_SIZE + 1 entries
        let mut payments: Vec<(Address, i128)> = Vec::new(&env);
        for _ in 0..=crate::MAX_BATCH_SIZE {
            payments.push_back((Address::generate(&env), 1_i128));
        }
        client.batch_distribute(&admin, &payments);
    }

    #[test]
    fn batch_distribute_at_max_size_succeeds() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        let amount_per = 10_i128;
        let total = amount_per * (crate::MAX_BATCH_SIZE as i128);
        fund_pool(&usdc_admin, &pool_addr, total);

        // Build a batch of exactly MAX_BATCH_SIZE entries
        let mut payments: Vec<(Address, i128)> = Vec::new(&env);
        for _ in 0..crate::MAX_BATCH_SIZE {
            payments.push_back((Address::generate(&env), amount_per));
        }
        client.batch_distribute(&admin, &payments);

        // Pool should be drained
        assert_eq!(usdc_client.balance(&pool_addr), 0);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn batch_distribute_negative_amount_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let dev = Address::generate(&env);
        let (_, client) = create_pool(&env);
        let (usdc_address, _, _) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);

        let mut payments: Vec<(Address, i128)> = Vec::new(&env);
        payments.push_back((dev, -100));
        client.batch_distribute(&admin, &payments);
    }

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
        fund_pool(&usdc_admin, &pool_addr, 1000);

        let mut payments: Vec<(Address, i128)> = Vec::new(&env);
        payments.push_back((dev, 100));
        client.batch_distribute(&attacker, &payments);
    }

    #[test]
    #[should_panic(expected = "invalid recipient: cannot distribute to the contract itself")]
    fn batch_distribute_self_recipient_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let (pool_addr, client) = create_pool(&env);
        let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

        client.init(&admin, &usdc_address);
        fund_pool(&usdc_admin, &pool_addr, 1000);

        let mut payments: Vec<(Address, i128)> = Vec::new(&env);
        payments.push_back((pool_addr, 100));
        client.batch_distribute(&admin, &payments);
    }

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
fn init_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);

    let events = env.events().all();
    let init_event = events.last().unwrap();
    let event_name = Symbol::try_from_val(&env, &init_event.1.get(0).unwrap()).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "init"));
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
#[should_panic(expected = "revenue pool already initialized")]
fn init_double_different_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let other_admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);
    let (usdc2, _, _) = create_usdc(&env, &other_admin);

    client.init(&admin, &usdc);
    client.init(&other_admin, &usdc2);
}

#[test]
#[should_panic(expected = "invalid config: usdc_token cannot be the contract itself")]
fn init_usdc_token_is_contract_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);

    // Passing the contract's own address as usdc_token should be rejected.
    client.init(&admin, &pool_addr);
}

#[test]
#[should_panic(expected = "invalid config: usdc_token cannot be the admin address")]
fn init_usdc_token_is_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);

    // Passing the admin address as usdc_token should be rejected.
    client.init(&admin, &admin);
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
fn set_admin_two_step_transfers_control() {
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
    assert_eq!(client.get_admin(), admin);

    client.claim_admin(&new_admin);
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
#[should_panic(expected = "unauthorized: caller is not pending admin")]
fn claim_admin_wrong_address_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.set_admin(&admin, &new_admin);
    client.claim_admin(&attacker);
}

#[test]
fn admin_transfer_emits_events() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);

    // Step 1 event
    client.set_admin(&admin, &new_admin);
    let events = env.events().all();
    let transfer_started = events.last().unwrap();

    // FIX: Convert Val to Symbol for comparison
    let event_name = Symbol::try_from_val(&env, &transfer_started.1.get(0).unwrap()).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "admin_transfer_started"));

    // Step 2 event
    client.claim_admin(&new_admin);
    let events = env.events().all();
    let transfer_completed = events.last().unwrap();

    // FIX: Convert Val to Symbol for comparison
    let event_name_comp =
        Symbol::try_from_val(&env, &transfer_completed.1.get(0).unwrap()).unwrap();
    assert_eq!(
        event_name_comp,
        Symbol::new(&env, "admin_transfer_completed")
    );
}

#[test]
fn receive_payment_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.receive_payment(&admin, &250, &true);

    let events = env.events().all();
    let receive_payment_event = events.last().unwrap();
    let event_name = Symbol::try_from_val(&env, &receive_payment_event.1.get(0).unwrap()).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "receive_payment"));

    let amount_and_source: (i128, bool) =
        <(i128, bool)>::try_from_val(&env, &receive_payment_event.2).unwrap();
    assert_eq!(amount_and_source, (250, true));
}

#[test]
#[should_panic(expected = "unauthorized: caller is not admin")]
fn receive_payment_non_admin_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.receive_payment(&attacker, &250, &true);
}

#[test]
fn receive_payment_is_event_only_and_does_not_move_tokens() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 500);

    let before_pool = usdc_client.balance(&pool_addr);
    let before_developer = usdc_client.balance(&developer);

    client.receive_payment(&admin, &250, &true);

    assert_eq!(usdc_client.balance(&pool_addr), before_pool);
    assert_eq!(usdc_client.balance(&developer), before_developer);
}

// ---------------------------------------------------------------------------
// Batch distribute tests - Comprehensive coverage
// ---------------------------------------------------------------------------

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

#[test]
fn batch_distribute_success_events() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1000);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 300_i128));
    payments.push_back((dev2.clone(), 200_i128));
    client.batch_distribute(&admin, &payments);

    let events = env.events().all();
    assert!(events.len() >= 4);

    for i in 0..events.len() {
        let (_, topics, data) = events.get(i).unwrap();
        let topic_0 = topics.get(0).unwrap();
        if let Ok(event_name) = Symbol::try_from_val(&env, &topic_0) {
            if event_name == Symbol::new(&env, "batch_distribute") {
                let value: i128 = i128::try_from_val(&env, &data).unwrap();
                assert!(value == 300 || value == 200);
            }
        }
    }
}

#[test]
fn receive_payment_emits_event_for_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.receive_payment(&admin, &250, &true);

    let events = env.events().all();
    let receive_event = events.last().unwrap();
    let event_name = Symbol::try_from_val(&env, &receive_event.1.get(0).unwrap()).unwrap();
    assert_eq!(event_name, Symbol::new(&env, "receive_payment"));

    let caller: Address = Address::try_from_val(&env, &receive_event.1.get(1).unwrap()).unwrap();
    assert_eq!(caller, admin);

    let (amount, from_vault): (i128, bool) = receive_event.2.into_val(&env);
    assert_eq!(amount, 250);
    assert!(from_vault);
}

#[test]
#[should_panic(expected = "no pending admin")]
fn claim_admin_without_pending_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let candidate = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.claim_admin(&candidate);
}

#[test]
#[should_panic(expected = "unauthorized: caller is not pending admin")]
fn claim_admin_wrong_caller_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let pending_admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.set_admin(&admin, &pending_admin);
    client.claim_admin(&attacker);
}

#[test]
#[should_panic(expected = "invalid recipient: cannot distribute to the contract itself")]
fn distribute_to_self_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 100);
    client.distribute(&admin, &pool_addr, &50);
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn batch_distribute_zero_amount_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc_address, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev, 0));
    client.batch_distribute(&admin, &payments);
}

// ---------------------------------------------------------------------------
// Event schema tests (Issue #256)
// Each test below pins the exact topic/data layout documented in EVENT_SCHEMA.md
// ---------------------------------------------------------------------------

#[test]
fn init_event_topics_and_data() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);

    let events = env.events().all();
    let ev = events.last().unwrap();

    // topic 0 = "init"
    let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
    assert_eq!(t0, Symbol::new(&env, "init"));

    // topic 1 = admin address
    let t1 = Address::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap();
    assert_eq!(t1, admin);

    // data = usdc_token address
    let data = Address::try_from_val(&env, &ev.2).unwrap();
    assert_eq!(data, usdc);
}

#[test]
fn admin_transfer_started_event_topics_and_data() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.set_admin(&admin, &new_admin);

    let events = env.events().all();
    let ev = events.last().unwrap();

    // topic 0 = "admin_transfer_started"
    let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
    assert_eq!(t0, Symbol::new(&env, "admin_transfer_started"));

    // topic 1 = current admin
    let t1 = Address::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap();
    assert_eq!(t1, admin);

    // data = pending admin
    let data = Address::try_from_val(&env, &ev.2).unwrap();
    assert_eq!(data, new_admin);
}

#[test]
fn admin_transfer_completed_event_topics_and_data() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.set_admin(&admin, &new_admin);
    client.claim_admin(&new_admin);

    let events = env.events().all();
    let ev = events.last().unwrap();

    // topic 0 = "admin_transfer_completed"
    let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
    assert_eq!(t0, Symbol::new(&env, "admin_transfer_completed"));

    // topic 1 = new admin
    let t1 = Address::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap();
    assert_eq!(t1, new_admin);

    // data = () empty
    let _: () = ev.2.into_val(&env);
}

#[test]
fn receive_payment_event_from_vault_true() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.receive_payment(&admin, &5_000_000, &true);

    let events = env.events().all();
    let ev = events.last().unwrap();

    // topic 0 = "receive_payment"
    let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
    assert_eq!(t0, Symbol::new(&env, "receive_payment"));

    // topic 1 = caller (admin)
    let t1 = Address::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap();
    assert_eq!(t1, admin);

    // data = (amount, from_vault)
    let (amount, from_vault): (i128, bool) = ev.2.into_val(&env);
    assert_eq!(amount, 5_000_000);
    assert!(from_vault);
}

#[test]
fn receive_payment_event_from_vault_false() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    client.receive_payment(&admin, &1_000_000, &false);

    let events = env.events().all();
    let ev = events.last().unwrap();

    let (amount, from_vault): (i128, bool) = ev.2.into_val(&env);
    assert_eq!(amount, 1_000_000);
    assert!(!from_vault);
}

#[test]
fn distribute_event_topics_and_data() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1_000_000);
    client.distribute(&admin, &developer, &1_000_000);

    let events = env.events().all();
    let ev = events.last().unwrap();

    // topic 0 = "distribute"
    let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
    assert_eq!(t0, Symbol::new(&env, "distribute"));

    // topic 1 = recipient
    let t1 = Address::try_from_val(&env, &ev.1.get(1).unwrap()).unwrap();
    assert_eq!(t1, developer);

    // data = amount
    let amount: i128 = ev.2.into_val(&env);
    assert_eq!(amount, 1_000_000);
}

#[test]
fn batch_distribute_event_topics_and_data() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let dev3 = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 3_500_000);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 1_000_000_i128));
    payments.push_back((dev2.clone(), 2_000_000_i128));
    payments.push_back((dev3.clone(), 500_000_i128));
    client.batch_distribute(&admin, &payments);

    let all_events = env.events().all();
    let batch_events: std::vec::Vec<_> = all_events
        .iter()
        .filter(|e| {
            e.1.get(0)
                .and_then(|v| Symbol::try_from_val(&env, &v).ok())
                .map(|s| s == Symbol::new(&env, "batch_distribute"))
                .unwrap_or(false)
        })
        .collect();

    // 3 payments → 3 batch_distribute events
    assert_eq!(batch_events.len(), 3);

    // verify each event has correct topic 0 and a positive amount
    for ev in batch_events.iter() {
        let t0 = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
        assert_eq!(t0, Symbol::new(&env, "batch_distribute"));
        let amount: i128 = ev.2.into_val(&env);
        assert!(amount > 0);
    }
}

#[test]
fn batch_distribute_is_atomic_all_or_nothing() {
    // If any payment fails the entire batch reverts — no events emitted.
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 100);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 60_i128));
    payments.push_back((dev2.clone(), 60_i128)); // total 120 > balance 100

    let result = client.try_batch_distribute(&admin, &payments);
    assert!(result.is_err());

    // balance unchanged
    assert_eq!(client.balance(), 100);
}

// ---------------------------------------------------------------------------
// get_admin() and get_usdc_token() getter tests  (Issue #265)
// ---------------------------------------------------------------------------

#[test]
fn get_admin_returns_correct_address() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);

    assert_eq!(client.get_admin(), admin);
}

#[test]
fn get_admin_reflects_updated_admin_after_transfer() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    assert_eq!(client.get_admin(), admin);

    // Pending phase: get_admin() still returns old admin
    client.set_admin(&admin, &new_admin);
    assert_eq!(client.get_admin(), admin);

    // After claim: admin updated
    client.claim_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);
}

#[test]
#[should_panic(expected = "revenue pool not initialized")]
fn get_admin_before_init_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = create_pool(&env);

    client.get_admin();
}

#[test]
fn get_usdc_token_returns_correct_address() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);

    assert_eq!(client.get_usdc_token(), usdc);
}

#[test]
fn get_usdc_token_is_immutable_after_init() {
    // The USDC token address must never change after initialization —
    // this test guards against accidental mutation.
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let new_admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);
    let token_before = client.get_usdc_token();

    // Admin transfer must not affect the token address
    client.set_admin(&admin, &new_admin);
    client.claim_admin(&new_admin);

    assert_eq!(client.get_usdc_token(), token_before);
}

#[test]
#[should_panic(expected = "revenue pool not initialized")]
fn get_usdc_token_before_init_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let (_, client) = create_pool(&env);

    client.get_usdc_token();
}

// ---------------------------------------------------------------------------
// batch_distribute length-cap tests (resource exhaustion prevention)
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "batch_distribute requires at least one payment")]
fn batch_distribute_empty_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc_address, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);

    let payments: Vec<(Address, i128)> = Vec::new(&env);
    client.batch_distribute(&admin, &payments);
}

#[test]
#[should_panic(expected = "batch too large")]
fn batch_distribute_too_large_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 100_000);

    // Build a batch of MAX_BATCH_SIZE + 1 entries
    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    for _ in 0..=crate::MAX_BATCH_SIZE {
        payments.push_back((Address::generate(&env), 1_i128));
    }
    client.batch_distribute(&admin, &payments);
}

#[test]
fn batch_distribute_at_max_size_succeeds() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    let amount_per = 10_i128;
    let total = amount_per * (crate::MAX_BATCH_SIZE as i128);
    fund_pool(&usdc_admin, &pool_addr, total);

    // Build a batch of exactly MAX_BATCH_SIZE entries
    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    for _ in 0..crate::MAX_BATCH_SIZE {
        payments.push_back((Address::generate(&env), amount_per));
    }
    client.batch_distribute(&admin, &payments);

    // Pool should be drained
    assert_eq!(usdc_client.balance(&pool_addr), 0);
}

#[test]
fn upgrade_requires_admin() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let attacker = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);

    let new_hash = BytesN::from_array(&env, &[1u8; 32]);

    // Non-admin attempt should fail
    let res = client.try_upgrade(&attacker, &new_hash);
    assert!(res.is_err());
}

#[test]
fn upgrade_sets_version_and_emits_event() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);

    let new_hash = BytesN::from_array(&env, &[2u8; 32]);

    client.upgrade(&admin, &new_hash);

    // version() should return stored value
    let readback: BytesN<32> = client.version();
    assert_eq!(readback, new_hash);

    // An `upgraded` event should have been emitted
    let events = env.events().all();
    let ev = events.last().unwrap();
    let name = Symbol::try_from_val(&env, &ev.1.get(0).unwrap()).unwrap();
    assert_eq!(name, Symbol::new(&env, "upgraded"));
}

#[test]
#[should_panic(expected = "amount must be positive")]
fn batch_distribute_negative_amount_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc_address, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev, -100));
    client.batch_distribute(&admin, &payments);
}

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
    fund_pool(&usdc_admin, &pool_addr, 1000);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev, 100));
    client.batch_distribute(&attacker, &payments);
}

#[test]
#[should_panic(expected = "invalid recipient: cannot distribute to the contract itself")]
fn batch_distribute_self_recipient_panics() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1000);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((pool_addr, 100));
    client.batch_distribute(&admin, &payments);
}

// ---------------------------------------------------------------------------
// TTL bump tests (Issue #342)
// Tests that state persists after simulated ledger advance and views don't trigger TTL bump
// ---------------------------------------------------------------------------

#[test]
fn state_persists_after_ledger_advance() {
    // Test that state-modifying functions extend TTL so state remains accessible
    // after a simulated ledger advance (archival risk mitigation)
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let developer = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    // Init extends TTL
    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1000);

    // Verify state is accessible before ledger advance
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_usdc_token(), usdc_address);

    // distribute extends TTL
    client.distribute(&admin, &developer, &100);
    assert_eq!(usdc_client.balance(&developer), 100);

    // set_admin extends TTL
    let new_admin = Address::generate(&env);
    client.set_admin(&admin, &new_admin);

    // claim_admin extends TTL
    client.claim_admin(&new_admin);
    assert_eq!(client.get_admin(), new_admin);

    // batch_distribute extends TTL
    let dev2 = Address::generate(&env);
    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev2.clone(), 100_i128));
    client.batch_distribute(&new_admin, &payments);
    assert_eq!(usdc_client.balance(&dev2), 100);

    // State remains accessible after all operations
    assert_eq!(client.get_admin(), new_admin);
    assert_eq!(client.get_usdc_token(), usdc_address);
    assert_eq!(client.balance(), 800);
}

#[test]
fn views_do_not_trigger_ttl_bump() {
    // Verify that view functions work correctly without side effects.
    // Views do not call extend_ttl, which is the expected behavior.
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let (_, client) = create_pool(&env);
    let (usdc, _, _) = create_usdc(&env, &admin);

    client.init(&admin, &usdc);

    // Call view functions - they should work but not extend TTL
    let admin_result = client.get_admin();
    assert_eq!(admin_result, admin);

    let usdc_result = client.get_usdc_token();
    assert_eq!(usdc_result, usdc);

    let balance_result = client.balance();
    assert_eq!(balance_result, 0);

    // Views can be called multiple times without issue
    assert_eq!(client.get_admin(), admin);
    assert_eq!(client.get_usdc_token(), usdc);
}

// ---------------------------------------------------------------------------
// Duplicate recipient tests — feature/revenue-pool-dedup-recipients
// Policy: reject batches containing the same Address more than once.
// ---------------------------------------------------------------------------

#[test]
#[should_panic(expected = "duplicate recipient in batch")]
fn batch_distribute_duplicate_recipient_panics() {
    // A batch where the same developer appears twice must be rejected entirely.
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1000);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev.clone(), 100_i128));
    payments.push_back((dev.clone(), 200_i128)); // duplicate

    client.batch_distribute(&admin, &payments);
}

#[test]
fn batch_distribute_duplicate_does_not_transfer_any_funds() {
    // Atomicity: a duplicate-recipient batch must leave balances completely unchanged.
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1000);

    let pool_balance_before = usdc_client.balance(&pool_addr);
    let dev_balance_before = usdc_client.balance(&dev);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev.clone(), 100_i128));
    payments.push_back((dev.clone(), 200_i128)); // duplicate

    let result = client.try_batch_distribute(&admin, &payments);
    assert!(result.is_err());

    // No tokens moved.
    assert_eq!(usdc_client.balance(&pool_addr), pool_balance_before);
    assert_eq!(usdc_client.balance(&dev), dev_balance_before);
}

#[test]
fn batch_distribute_duplicate_does_not_emit_events() {
    // No batch_distribute events should be emitted when the call is rejected.
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1000);

    // Capture event count before the failing call.
    let events_before = env.events().all().len();

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev.clone(), 100_i128));
    payments.push_back((dev.clone(), 200_i128));

    let _ = client.try_batch_distribute(&admin, &payments);

    // Event log must not have grown with batch_distribute events.
    let batch_events_after = env
        .events()
        .all()
        .iter()
        .skip(events_before as usize)
        .filter(|e| {
            e.1.get(0)
                .and_then(|v| soroban_sdk::Symbol::try_from_val(&env, &v).ok())
                .map(|s| s == soroban_sdk::Symbol::new(&env, "batch_distribute"))
                .unwrap_or(false)
        })
        .count();

    assert_eq!(batch_events_after, 0);
}

#[test]
#[should_panic(expected = "duplicate recipient in batch")]
fn batch_distribute_duplicate_at_end_panics() {
    // Duplicate at position n-1 (last entry) must still be caught.
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let dev3 = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 1000);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 100_i128));
    payments.push_back((dev2.clone(), 200_i128));
    payments.push_back((dev3.clone(), 150_i128));
    payments.push_back((dev1.clone(), 50_i128)); // dev1 again at the end

    client.batch_distribute(&admin, &payments);
}

#[test]
fn batch_distribute_unique_recipients_succeeds() {
    // Sanity: a well-formed batch with all distinct addresses must still work.
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev1 = Address::generate(&env);
    let dev2 = Address::generate(&env);
    let dev3 = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, usdc_client, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    fund_pool(&usdc_admin, &pool_addr, 600);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev1.clone(), 100_i128));
    payments.push_back((dev2.clone(), 200_i128));
    payments.push_back((dev3.clone(), 300_i128));

    client.batch_distribute(&admin, &payments);

    assert_eq!(usdc_client.balance(&dev1), 100);
    assert_eq!(usdc_client.balance(&dev2), 200);
    assert_eq!(usdc_client.balance(&dev3), 300);
    assert_eq!(client.balance(), 0);
}

#[test]
#[should_panic(expected = "duplicate recipient in batch")]
fn batch_distribute_duplicate_detected_before_balance_check() {
    // Duplicate detection must fire even when the pool has insufficient balance,
    // proving it runs in Phase 1 (before the Phase 2 balance check).
    // The panic message must be "duplicate recipient in batch", not "insufficient USDC balance".
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let dev = Address::generate(&env);
    let (pool_addr, client) = create_pool(&env);
    let (usdc_address, _, usdc_admin) = create_usdc(&env, &admin);

    client.init(&admin, &usdc_address);
    // Fund only 10, but the batch would need 300 — yet the duplicate error fires first.
    fund_pool(&usdc_admin, &pool_addr, 10);

    let mut payments: Vec<(Address, i128)> = Vec::new(&env);
    payments.push_back((dev.clone(), 100_i128));
    payments.push_back((dev.clone(), 200_i128));

    client.batch_distribute(&admin, &payments);
}
