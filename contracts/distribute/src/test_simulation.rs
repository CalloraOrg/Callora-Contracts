use crate::{errors::DistributeError as E, *};
use soroban_sdk::testutils::storage::{Instance as _, Persistent as _};
use soroban_sdk::testutils::{Address as _, EnvTestConfig, Events as _, Ledger as _};
use soroban_sdk::{vec, Error};

struct Fixture {
    env: Env,
    contract: Address,
    admin: Address,
    token: Address,
    recipient: Address,
}

impl Fixture {
    fn new(balance: i128, cap: i128, paused: bool) -> Self {
        let env = Env::new_with_config(EnvTestConfig {
            capture_snapshot_at_drop: false,
        });
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let token = env
            .register_stellar_asset_contract_v2(admin.clone())
            .address();
        let contract = env.register(Distribute, ());
        let recipient = Address::generate(&env);
        let client = DistributeClient::new(&env, &contract);
        client.init(&admin, &token);
        client.set_max_distribute(&admin, &cap);
        token::StellarAssetClient::new(&env, &token).mint(&contract, &balance);
        if paused {
            client.pause(&admin);
        }
        Self {
            env,
            contract,
            admin,
            token,
            recipient,
        }
    }

    fn client(&self) -> DistributeClient<'_> {
        DistributeClient::new(&self.env, &self.contract)
    }

    fn payments(&self, amounts: &[i128]) -> Vec<(Address, i128)> {
        let mut payments = Vec::new(&self.env);
        for amount in amounts {
            payments.push_back((self.recipient.clone(), *amount));
        }
        payments
    }

    fn parity(&self, payments: &Vec<(Address, i128)>) -> BatchSimulation {
        self.env.set_auths(&[]);
        let report = self.client().simulate_batch_distribute(payments);
        assert!(self.env.auths().is_empty());
        assert!(self.env.events().all().is_empty());
        assert_eq!(report.ok, report.errors.is_empty());
        assert_eq!(report.legs, payments.len());
        self.env.mock_all_auths();
        let actual = self.client().try_batch_distribute(&self.admin, payments);
        if report.ok {
            assert_eq!(actual, Ok(Ok(())));
        } else {
            assert_eq!(
                actual,
                Err(Ok(Error::from_contract_error(
                    report.errors.get(0).unwrap()
                )))
            );
        }
        report
    }
}

#[test]
fn simulation_exact_balance_and_maximum_batch_succeed() {
    for size in [1, 49, 50] {
        let f = Fixture::new(size as i128, 1, false);
        let mut payments = Vec::new(&f.env);
        for _ in 0..size {
            payments.push_back((f.recipient.clone(), 1));
        }
        let report = f.parity(&payments);
        assert!(report.ok);
        assert_eq!(report.total, size as i128);
        assert_eq!(report.contract_balance, report.total);
        assert_eq!(
            token::Client::new(&f.env, &f.token).balance(&f.recipient),
            report.total
        );
    }
}

#[test]
fn simulation_reports_all_errors_in_validation_order() {
    let f = Fixture::new(10, 20, true);
    let payments = vec![
        &f.env,
        (f.contract.clone(), 0),
        (f.contract.clone(), 30),
        (f.recipient.clone(), -1),
    ];
    let report = f.parity(&payments);
    assert!(report.paused);
    assert_eq!(report.total, 30);
    assert_eq!(
        report.errors,
        vec![
            &f.env,
            E::Paused as u32,
            E::AmountNotPositive as u32,
            E::InvalidRecipient as u32,
            E::AmountExceedsMaxDistribute as u32,
            E::InvalidRecipient as u32,
            E::AmountNotPositive as u32,
            E::InsufficientBalance as u32
        ]
    );
}

#[test]
fn simulation_size_and_individual_error_parity() {
    for (amounts, cap, code) in [
        (&[][..], 100, E::BatchEmpty),
        (&[0][..], 100, E::AmountNotPositive),
        (&[-1][..], 100, E::AmountNotPositive),
        (&[101][..], 100, E::AmountExceedsMaxDistribute),
        (&[60, 41][..], 100, E::InsufficientBalance),
    ] {
        let f = Fixture::new(100, cap, false);
        assert_eq!(
            f.parity(&f.payments(amounts)).errors.get(0),
            Some(code as u32)
        );
    }
    let f = Fixture::new(100, 100, false);
    let mut payments = f.payments(&[1; 51]);
    payments.set(50, (f.contract.clone(), -1));
    assert_eq!(
        f.parity(&payments).errors,
        vec![
            &f.env,
            E::BatchTooLarge as u32,
            E::AmountNotPositive as u32,
            E::InvalidRecipient as u32
        ]
    );
    assert_eq!(
        f.parity(&vec![&f.env, (f.contract.clone(), 1)]).errors,
        vec![&f.env, E::InvalidRecipient as u32]
    );
}

#[test]
fn simulation_overflow_is_saturated_and_cannot_be_offset_by_negative_legs() {
    let f = Fixture::new(i128::MAX, i128::MAX, false);
    let report = f.parity(&f.payments(&[i128::MAX, 1, -1]));
    assert_eq!(report.total, i128::MAX);
    assert_eq!(
        report.errors,
        vec![
            &f.env,
            E::Overflow as u32,
            E::AmountNotPositive as u32,
            E::InsufficientBalance as u32
        ]
    );
    let report = f.parity(&f.payments(&[i128::MAX]));
    assert!(report.ok);
}

#[test]
fn simulation_uninitialized_is_a_report_not_a_panic() {
    let env = Env::default();
    let contract = env.register(Distribute, ());
    let client = DistributeClient::new(&env, &contract);
    let payments = Vec::new(&env);
    let report = client.simulate_batch_distribute(&payments);
    assert!(!report.ok);
    assert_eq!(report.contract_balance, 0);
    assert_eq!(
        report.errors,
        vec![&env, E::NotInitialized as u32, E::BatchEmpty as u32]
    );
    env.mock_all_auths();
    assert_eq!(
        client.try_batch_distribute(&Address::generate(&env), &payments),
        Err(Ok(Error::from_contract_error(E::NotInitialized as u32)))
    );
}

#[test]
fn simulation_missing_token_preserves_validation_order() {
    let f = Fixture::new(100, 100, false);
    f.env.as_contract(&f.contract, || {
        f.env
            .storage()
            .instance()
            .remove(&Symbol::new(&f.env, USDC_KEY));
    });
    let report = f.parity(&f.payments(&[1]));
    assert_eq!(report.contract_balance, 0);
    assert_eq!(report.errors, vec![&f.env, E::NotInitialized as u32]);
    let report = f.parity(&Vec::new(&f.env));
    assert_eq!(
        report.errors,
        vec![&f.env, E::BatchEmpty as u32, E::NotInitialized as u32]
    );
}

#[test]
fn simulation_leaves_storage_ttl_balances_and_events_unchanged() {
    for paused in [false, true] {
        let f = Fixture::new(100, 100, paused);
        // Enter the refresh window so an accidental getter call is observable.
        f.env.as_contract(&f.contract, || {
            assert!(f.env.storage().instance().get_ttl() > INSTANCE_BUMP_THRESHOLD);
        });
        f.env.as_contract(&f.token, || {
            f.env
                .storage()
                .instance()
                .extend_ttl(INSTANCE_BUMP_AMOUNT, INSTANCE_BUMP_AMOUNT * 2);
            let persistent = f.env.storage().persistent();
            for key in persistent.all().keys() {
                persistent.extend_ttl(&key, INSTANCE_BUMP_AMOUNT, INSTANCE_BUMP_AMOUNT * 2);
            }
        });
        // Isolate Distribute's TTL from expiration of the fixture token's data.
        f.env
            .ledger()
            .set_sequence_number(f.env.ledger().sequence() + INSTANCE_BUMP_THRESHOLD + 1);
        let token = token::Client::new(&f.env, &f.token);
        let before_balance = token.balance(&f.contract);
        let before_recipient = token.balance(&f.recipient);
        let (before_state, before_ttl) = f.env.as_contract(&f.contract, || {
            (
                f.env.storage().instance().all(),
                f.env.storage().instance().get_ttl(),
            )
        });
        assert!(before_ttl <= INSTANCE_BUMP_THRESHOLD);
        f.env.set_auths(&[]);
        for payments in [f.payments(&[100]), f.payments(&[-1, 101]), Vec::new(&f.env)] {
            let report = f.client().simulate_batch_distribute(&payments);
            assert_eq!(report.contract_balance, before_balance);
            assert!(f.env.events().all().is_empty());
            assert!(f.env.auths().is_empty());
            f.env.as_contract(&f.contract, || {
                assert_eq!(f.env.storage().instance().all(), before_state);
                assert_eq!(f.env.storage().instance().get_ttl(), before_ttl);
            });
            assert_eq!(token.balance(&f.contract), before_balance);
            assert_eq!(token.balance(&f.recipient), before_recipient);
        }
    }
}

#[test]
fn simulation_seeded_batch_parity() {
    // Fixed-seed property-style coverage without another dependency. Include
    // successful and failing batches, repeated recipients and multiple bad legs.
    let mut seed = 1503_u64;
    for case in 0..128 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let f = Fixture::new((seed % 500) as i128, 100, case % 13 == 0);
        let mut payments = Vec::new(&f.env);
        for _ in 0..(seed % 8) {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            let recipient = if seed % 11 == 0 {
                &f.contract
            } else {
                &f.recipient
            };
            payments.push_back((recipient.clone(), (seed % 130) as i128 - 10));
        }
        f.parity(&payments);
    }
}
