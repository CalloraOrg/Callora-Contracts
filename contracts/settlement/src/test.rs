#[cfg(test)]
mod settlement_tests {
    use crate::{CalloraSettlement, CalloraSettlementClient};
    use soroban_sdk::{testutils::Address as _, Address, Env};

    #[test]
    fn test_settlement_initialization() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);

        client.init(&admin, &vault);

        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_vault(), vault);

        let global_pool = client.get_global_pool();
        assert_eq!(global_pool.total_balance, 0);
    }

    #[test]
    fn test_receive_payment_to_pool() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        let payment_amount = 1000i128;
        client.receive_payment(&vault, &payment_amount, &true, &None);

        let global_pool = client.get_global_pool();
        assert_eq!(global_pool.total_balance, payment_amount);
    }

    #[test]
    fn test_receive_payment_to_developer() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        let payment_amount = 500i128;
        client.receive_payment(&vault, &payment_amount, &false, &Some(developer.clone()));

        let balance = client.get_developer_balance(&developer);
        assert_eq!(balance, payment_amount);

        let global_pool = client.get_global_pool();
        assert_eq!(global_pool.total_balance, 0);
    }

    #[test]
    #[should_panic(expected = "unauthorized: caller must be vault or admin")]
    fn test_receive_payment_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let unauthorized = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.receive_payment(&unauthorized, &100i128, &true, &None);
    }

    #[test]
    #[should_panic(expected = "amount must be positive")]
    fn test_receive_payment_zero_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.receive_payment(&vault, &0i128, &true, &None);
    }

    #[test]
    #[should_panic(expected = "developer address required when to_pool=false")]
    fn test_receive_payment_pool_false_no_developer() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.receive_payment(&vault, &100i128, &false, &None);
    }

    #[test]
    #[should_panic(expected = "settlement contract already initialized")]
    fn test_double_init_panics() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);
        client.init(&admin, &vault);
    }

    #[test]
    fn test_set_admin() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.set_admin(&admin, &new_admin);
        assert_eq!(client.get_admin(), new_admin);
    }

    #[test]
    #[should_panic(expected = "unauthorized: caller is not admin")]
    fn test_set_admin_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let attacker = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.set_admin(&attacker, &new_admin);
    }

    #[test]
    fn test_set_vault() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.set_vault(&admin, &new_vault);
        assert_eq!(client.get_vault(), new_vault);
    }

    #[test]
    #[should_panic(expected = "unauthorized: caller is not admin")]
    fn test_set_vault_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let attacker = Address::generate(&env);
        let new_vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.set_vault(&attacker, &new_vault);
    }

    #[test]
    fn test_get_all_developer_balances() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let dev1 = Address::generate(&env);
        let dev2 = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.receive_payment(&vault, &300i128, &false, &Some(dev1.clone()));
        client.receive_payment(&vault, &200i128, &false, &Some(dev2.clone()));

        let all = client.get_all_developer_balances();
        assert_eq!(all.len(), 2);
    }

    #[test]
    fn test_receive_multiple_payments_accumulate() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.receive_payment(&vault, &100i128, &false, &Some(developer.clone()));
        client.receive_payment(&vault, &150i128, &false, &Some(developer.clone()));

        let balance = client.get_developer_balance(&developer);
        assert_eq!(balance, 250i128);
    }

    #[test]
    fn test_admin_can_receive_payment() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.receive_payment(&admin, &500i128, &true, &None);

        let global_pool = client.get_global_pool();
        assert_eq!(global_pool.total_balance, 500i128);
    }
}
