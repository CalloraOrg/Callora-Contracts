#[cfg(test)]
mod settlement_tests {
    extern crate std;

    use crate::{CalloraSettlement, CalloraSettlementClient};
    use soroban_sdk::testutils::{Address as _, Ledger as _};
    use soroban_sdk::{Address, Env, Map, Symbol};
    use std::any::Any;
    use std::panic::{catch_unwind, AssertUnwindSafe};

    fn setup_contract() -> (Env, Address, Address, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let third_party = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);
        (env, addr, admin, vault, third_party)
    }

    fn panic_message(err: std::boxed::Box<dyn Any + Send>) -> std::string::String {
        if let Some(message) = err.downcast_ref::<&str>() {
            std::string::String::from(*message)
        } else if let Some(message) = err.downcast_ref::<std::string::String>() {
            message.clone()
        } else {
            std::string::String::from("<non-string panic payload>")
        }
    }

    #[test]
    fn test_settlement_initialization() {
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_700_000_000);
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);

        client.init(&admin, &vault);

        env.as_contract(&addr, || {
            let inst = env.storage().instance();
            assert!(inst.has(&Symbol::new(&env, "admin")));
            assert!(inst.has(&Symbol::new(&env, "vault")));
            assert!(inst.has(&Symbol::new(&env, "developer_balances")));
            assert!(inst.has(&Symbol::new(&env, "global_pool")));

            let balances: Map<Address, i128> =
                inst.get(&Symbol::new(&env, "developer_balances")).unwrap();
            assert_eq!(balances.len(), 0);
        });

        assert_eq!(client.get_admin(), admin);
        assert_eq!(client.get_vault(), vault);

        let global_pool = client.get_global_pool();
        assert_eq!(global_pool.total_balance, 0);
        assert_eq!(global_pool.last_updated, 1_700_000_000);

        let all_balances = client.get_all_developer_balances();
        assert_eq!(all_balances.len(), 0);
        assert_eq!(client.get_developer_balance(&developer), 0);
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
    fn test_get_developer_balance_when_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        let balance = client.get_developer_balance(&developer);
        assert_eq!(balance, 0);
    }

    #[test]
    fn test_get_all_developer_balances_when_empty() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        let all = client.get_all_developer_balances();
        assert_eq!(all.len(), 0);
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
    fn test_receive_payment_authorization_matrix() {
        enum CallerRole {
            Vault,
            Admin,
            ThirdParty,
        }

        struct Case {
            name: &'static str,
            role: CallerRole,
            expected: Result<(), &'static str>,
        }

        let cases = [
            Case {
                name: "vault address succeeds",
                role: CallerRole::Vault,
                expected: Ok(()),
            },
            Case {
                name: "admin address succeeds",
                role: CallerRole::Admin,
                expected: Ok(()),
            },
            Case {
                name: "third party fails",
                role: CallerRole::ThirdParty,
                expected: Err("unauthorized: caller must be vault or admin"),
            },
        ];

        for case in cases {
            let (env, addr, admin, vault, third_party) = setup_contract();
            let client = CalloraSettlementClient::new(&env, &addr);
            let caller = match case.role {
                CallerRole::Vault => vault,
                CallerRole::Admin => admin,
                CallerRole::ThirdParty => third_party,
            };

            let result = catch_unwind(AssertUnwindSafe(|| {
                client.receive_payment(&caller, &100i128, &true, &None);
            }));

            match case.expected {
                Ok(()) => {
                    assert!(result.is_ok(), "expected success for case: {}", case.name);
                    let global_pool = client.get_global_pool();
                    assert_eq!(global_pool.total_balance, 100i128);
                }
                Err(expected_panic) => {
                    let err = result.expect_err("expected panic but call succeeded");
                    let message = panic_message(err);
                    assert!(
                        message.contains(expected_panic),
                        "case: {} (got panic: {})",
                        case.name,
                        message
                    );
                }
            }
        }
    }
}
