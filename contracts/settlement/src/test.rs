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
        let third_party = Address::generate(&env);
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

        client.receive_payment(&vault, &1000i128, &true, &None);

        let global_pool = client.get_global_pool();
        assert_eq!(global_pool.total_balance, 1000i128);
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

        client.receive_payment(&vault, &500i128, &false, &Some(developer.clone()));

        assert_eq!(client.get_developer_balance(&developer), 500i128);
        assert_eq!(client.get_global_pool().total_balance, 0);
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

        assert_eq!(client.get_developer_balance(&developer), 250i128);
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
    fn test_admin_can_receive_payment_to_pool() {
        // Admin can route payments directly to global pool (not just via vault)
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);
        client.receive_payment(&admin, &100i128, &true, &None);
    }

    #[test]
    fn test_admin_can_receive_payment_to_developer() {
        // Admin routing a payment directly to a developer (not via vault)
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.receive_payment(&admin, &200i128, &false, &Some(developer.clone()));

        assert_eq!(client.get_developer_balance(&developer), 200i128);
    }

    #[test]
    fn test_pool_accumulates_across_multiple_payments() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.receive_payment(&vault, &400i128, &true, &None);
        client.receive_payment(&vault, &600i128, &true, &None);

        assert_eq!(client.get_global_pool().total_balance, 1000i128);
    }

    #[test]
    fn test_get_developer_balance_returns_zero_for_unknown() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let stranger = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        assert_eq!(client.get_developer_balance(&stranger), 0i128);
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
    fn test_get_all_developer_balances_empty() {
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
    fn test_set_admin_two_step() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.set_admin(&admin, &new_admin);
        assert_eq!(client.get_admin(), admin); // Still old admin

        client.accept_admin();
        assert_eq!(client.get_admin(), new_admin);
    }

    #[test]
    #[should_panic(expected = "no admin transfer pending")]
    fn test_accept_admin_fails_if_not_nominated() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.accept_admin();
    }

    #[test]
    #[should_panic(expected = "unauthorized: caller is not admin")]
    fn test_set_admin_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        // Third party cannot set admin
        client.set_admin(&vault, &new_admin);
    }

    #[test]
    #[should_panic(expected = "unauthorized: caller is not admin")]
    fn test_set_vault_unauthorized() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        let attacker = Address::generate(&env);
        client.set_vault(&attacker, &new_vault);
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

    // â”€â”€ admin rotation edge cases â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_set_admin_to_same_address_succeeds() {
        // Admin can nominate themselves again (useful for re-confirming control)
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.set_admin(&admin, &admin);
        // Still current admin until accept
        assert_eq!(client.get_admin(), admin);

        client.accept_admin();
        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    fn test_set_vault_to_same_address_succeeds() {
        // Admin can update vault to same address (no-op but valid)
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.set_vault(&admin, &vault);
        assert_eq!(client.get_vault(), vault);
    }

    #[test]
    fn test_rapid_consecutive_admin_updates() {
        // Admin can change nomination before acceptance
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_admin1 = Address::generate(&env);
        let new_admin2 = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        // First nomination
        client.set_admin(&admin, &new_admin1);
        // Change nomination before acceptance
        client.set_admin(&admin, &new_admin2);
        // Only second nominee can accept
        client.accept_admin();
        assert_eq!(client.get_admin(), new_admin2);
    }

    #[test]
    fn test_admin_cannot_accept_own_nomination() {
        // Current admin cannot bypass two-step process
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.set_admin(&admin, &admin);
        // Admin must still accept to complete transfer
        client.accept_admin();
        assert_eq!(client.get_admin(), admin);
    }

    #[test]
    #[should_panic(expected = "unauthorized: caller is not admin")]
    fn test_pending_admin_cannot_set_admin() {
        // Pending admin has no privileges until accepted
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.set_admin(&admin, &new_admin);
        // New admin tries to set another admin before accepting
        client.set_admin(&new_admin, &vault);
    }

    #[test]
    fn test_vault_update_after_admin_rotation() {
        // Ensure vault updates work correctly after admin change
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let new_vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        // Rotate admin
        client.set_admin(&admin, &new_admin);
        client.accept_admin();

        // New admin updates vault
        client.set_vault(&new_admin, &new_vault);
        assert_eq!(client.get_vault(), new_vault);
        assert_eq!(client.get_admin(), new_admin);
    }

    #[test]
    fn test_admin_rotation_preserves_state() {
        // Admin rotation doesn't affect pool or balances
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        // Add some balance
        client.receive_payment(&vault, &1000i128, &false, &Some(developer.clone()));
        let dev_balance_before = client.get_developer_balance(&developer);
        let pool_before = client.get_global_pool();

        // Rotate admin
        client.set_admin(&admin, &new_admin);
        client.accept_admin();

        // State preserved
        assert_eq!(client.get_developer_balance(&developer), dev_balance_before);
        assert_eq!(
            client.get_global_pool().total_balance,
            pool_before.total_balance
        );
    }

    // â”€â”€ event emission tests â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_set_admin_emits_nomination_event() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::{IntoVal, Symbol};

        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.set_admin(&admin, &new_admin);

        let events = env.events().all();
        let nom_ev = events
            .iter()
            .find(|e| {
                !e.1.is_empty() && {
                    let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                    t == Symbol::new(&env, "admin_nominated")
                }
            })
            .expect("expected admin_nominated event");

        let topic_current: Address = nom_ev.1.get(1).unwrap().into_val(&env);
        let topic_new: Address = nom_ev.1.get(2).unwrap().into_val(&env);
        assert_eq!(topic_current, admin);
        assert_eq!(topic_new, new_admin);
    }

    #[test]
    fn test_accept_admin_emits_accepted_event() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::{IntoVal, Symbol};

        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.set_admin(&admin, &new_admin);
        client.accept_admin();

        let events = env.events().all();
        let acc_ev = events
            .iter()
            .find(|e| {
                !e.1.is_empty() && {
                    let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                    t == Symbol::new(&env, "admin_accepted")
                }
            })
            .expect("expected admin_accepted event");

        let topic_old: Address = acc_ev.1.get(1).unwrap().into_val(&env);
        let topic_new: Address = acc_ev.1.get(2).unwrap().into_val(&env);
        assert_eq!(topic_old, admin);
        assert_eq!(topic_new, new_admin);
    }

    // â”€â”€ panic / error paths â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

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
    #[should_panic(expected = "amount must be positive")]
    fn test_receive_payment_negative_amount() {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.receive_payment(&vault, &-1i128, &true, &None);
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

    // â”€â”€ event shape tests â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€

    #[test]
    fn test_payment_received_event_to_pool() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::{IntoVal, Symbol};

        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.receive_payment(&vault, &1000i128, &true, &None);

        let events = env.events().all();
        let ev = events
            .iter()
            .find(|e| {
                !e.1.is_empty() && {
                    let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                    t == Symbol::new(&env, "payment_received")
                }
            })
            .expect("expected payment_received event");

        let topic1: Address = ev.1.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, vault);

        let data: crate::PaymentReceivedEvent = ev.2.into_val(&env);
        assert_eq!(data.from_vault, vault);
        assert_eq!(data.amount, 1000i128);
        assert!(data.to_pool);
        assert!(data.developer.is_none());
    }

    #[test]
    fn test_payment_received_and_balance_credited_events_to_developer() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::{IntoVal, Symbol};

        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.receive_payment(&vault, &500i128, &false, &Some(developer.clone()));

        let events = env.events().all();

        let pr_ev = events
            .iter()
            .find(|e| {
                !e.1.is_empty() && {
                    let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                    t == Symbol::new(&env, "payment_received")
                }
            })
            .expect("expected payment_received event");

        let pr_data: crate::PaymentReceivedEvent = pr_ev.2.into_val(&env);
        assert!(!pr_data.to_pool);
        assert_eq!(pr_data.developer, Some(developer.clone()));

        let bc_ev = events
            .iter()
            .find(|e| {
                !e.1.is_empty() && {
                    let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                    t == Symbol::new(&env, "balance_credited")
                }
            })
            .expect("expected balance_credited event");

        let topic1: Address = bc_ev.1.get(1).unwrap().into_val(&env);
        assert_eq!(topic1, developer);

        let bc_data: crate::BalanceCreditedEvent = bc_ev.2.into_val(&env);
        assert_eq!(bc_data.developer, developer);
        assert_eq!(bc_data.amount, 500i128);
        assert_eq!(bc_data.new_balance, 500i128);
    }

    #[test]
    fn test_balance_credited_new_balance_is_cumulative() {
        use soroban_sdk::testutils::Events as _;
        use soroban_sdk::{IntoVal, Symbol};

        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        client.receive_payment(&vault, &300i128, &false, &Some(developer.clone()));
        client.receive_payment(&vault, &200i128, &false, &Some(developer.clone()));

        // grab the last balance_credited event
        let events = env.events().all();
        let bc_ev = events
            .iter()
            .rev()
            .find(|e| {
                !e.1.is_empty() && {
                    let t: Symbol = e.1.get(0).unwrap().into_val(&env);
                    t == Symbol::new(&env, "balance_credited")
                }
            })
            .expect("expected balance_credited event");

        let bc_data: crate::BalanceCreditedEvent = bc_ev.2.into_val(&env);
        assert_eq!(bc_data.new_balance, 500i128);
    }

    // â”€â”€ regression tests: ensure settlement logic intact after rotation â”€â”€â”€â”€â”€

    #[test]
    fn test_receive_payment_works_after_admin_rotation() {
        // Ensure payment processing still works after admin change
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        // Rotate admin
        client.set_admin(&admin, &new_admin);
        client.accept_admin();

        // Vault can still send payments
        client.receive_payment(&vault, &1000i128, &true, &None);
        assert_eq!(client.get_global_pool().total_balance, 1000i128);
    }

    #[test]
    fn test_receive_payment_works_after_vault_update() {
        // Ensure payment processing works with new vault address
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        // Update vault
        client.set_vault(&admin, &new_vault);

        // Old vault cannot send payments
        let result = catch_unwind(AssertUnwindSafe(|| {
            client.receive_payment(&vault, &1000i128, &true, &None);
        }));
        assert!(result.is_err());
        assert!(panic_message(result.unwrap_err()).contains("unauthorized"));

        // New vault can send payments
        client.receive_payment(&new_vault, &1000i128, &true, &None);
        assert_eq!(client.get_global_pool().total_balance, 1000i128);
    }

    #[test]
    fn test_developer_withdrawal_after_admin_rotation() {
        // Ensure developer balances accessible after admin change
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        // Credit developer
        client.receive_payment(&vault, &500i128, &false, &Some(developer.clone()));

        // Rotate admin
        client.set_admin(&admin, &new_admin);
        client.accept_admin();

        // Balance still accessible
        assert_eq!(client.get_developer_balance(&developer), 500i128);

        // Admin can still view all balances
        let all_balances = client.get_all_developer_balances();
        assert_eq!(all_balances.len(), 1);
        assert_eq!(all_balances.get(0).unwrap().balance, 500i128);
    }

    #[test]
    fn test_multiple_payments_accumulate_after_vault_update() {
        // Ensure accumulation logic works correctly after vault changes
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        // Some payments from old vault
        client.receive_payment(&vault, &100i128, &false, &Some(developer.clone()));

        // Update vault
        client.set_vault(&admin, &new_vault);

        // More payments from new vault
        client.receive_payment(&new_vault, &150i128, &false, &Some(developer.clone()));
        client.receive_payment(&new_vault, &200i128, &false, &Some(developer.clone()));

        // Total should accumulate correctly
        assert_eq!(client.get_developer_balance(&developer), 450i128);
    }

    #[test]
    fn test_global_pool_timestamp_updates_after_admin_change() {
        // Ensure pool timestamp updates correctly regardless of admin
        let env = Env::default();
        env.mock_all_auths();
        env.ledger().set_timestamp(1_700_000_000);

        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let new_admin = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);
        client.init(&admin, &vault);

        // Initial payment
        client.receive_payment(&vault, &1000i128, &true, &None);
        let pool_before = client.get_global_pool();
        assert_eq!(pool_before.last_updated, 1_700_000_000);

        // Rotate admin and advance time
        client.set_admin(&admin, &new_admin);
        client.accept_admin();
        env.ledger().set_timestamp(1_700_000_100);

        // New payment updates timestamp
        client.receive_payment(&vault, &500i128, &true, &None);
        let pool_after = client.get_global_pool();
        assert_eq!(pool_after.last_updated, 1_700_000_100);
        assert_eq!(pool_after.total_balance, 1500i128);
    }

    /// `last_updated` reflects the ledger timestamp at the moment of each pool credit.
    #[test]
    fn test_global_pool_last_updated_on_receive_payment() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);

        env.ledger().set_timestamp(1_000);
        client.init(&admin, &vault);
        assert_eq!(client.get_global_pool().last_updated, 1_000);

        // Advance time and credit pool ï¿½ last_updated must change
        env.ledger().set_timestamp(2_000);
        client.receive_payment(&vault, &100i128, &true, &None);
        let pool = client.get_global_pool();
        assert_eq!(pool.last_updated, 2_000);
        assert_eq!(pool.total_balance, 100i128);

        // Advance again ï¿½ each credit stamps the new time
        env.ledger().set_timestamp(3_000);
        client.receive_payment(&vault, &50i128, &true, &None);
        let pool2 = client.get_global_pool();
        assert_eq!(pool2.last_updated, 3_000);
        assert_eq!(pool2.total_balance, 150i128);
    }

    /// Routing to a developer does NOT update `last_updated` on the global pool.
    #[test]
    fn test_global_pool_last_updated_unchanged_for_developer_payment() {
        let env = Env::default();
        env.mock_all_auths();

        let admin = Address::generate(&env);
        let vault = Address::generate(&env);
        let developer = Address::generate(&env);
        let addr = env.register(CalloraSettlement, ());
        let client = CalloraSettlementClient::new(&env, &addr);

        env.ledger().set_timestamp(1_000);
        client.init(&admin, &vault);

        env.ledger().set_timestamp(5_000);
        client.receive_payment(&vault, &200i128, &false, &Some(developer.clone()));

        // Pool timestamp must still be the init timestamp
        assert_eq!(client.get_global_pool().last_updated, 1_000);
        assert_eq!(client.get_global_pool().total_balance, 0);
        assert_eq!(client.get_developer_balance(&developer), 200i128);
    }
}
