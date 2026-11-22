//! End-to-end tests for the emergency pause coordinator (#1523).

#![cfg(test)]

extern crate std;

use callora_distribute::{Distribute, DistributeClient};
use callora_emergency::{
    CalloraEmergency, CalloraEmergencyClient, EmergencyError, PauseReport, MAX_REGISTRY_SIZE,
};
use callora_revenue_pool::{RevenuePool, RevenuePoolClient};
use soroban_sdk::testutils::{Address as _, Events as _};
use soroban_sdk::{
    contract, contractimpl, Address, Env, IntoVal, Symbol, Vec,
};

#[contract]
pub struct MockPausable;

#[contractimpl]
impl MockPausable {
    fn admin(env: &Env) -> Address {
        env.storage()
            .instance()
            .get(&Symbol::new(env, "admin"))
            .unwrap()
    }

    pub fn init(env: Env, admin: Address) {
        admin.require_auth();
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "admin"), &admin);
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "paused"), &false);
    }

    pub fn pause(env: Env, caller: Address) {
        caller.require_auth();
        if caller != Self::admin(&env) {
            panic!("unauthorized");
        }
        if Self::is_paused(env.clone()) {
            panic!("already paused");
        }
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "paused"), &true);
    }

    pub fn unpause(env: Env, caller: Address) {
        caller.require_auth();
        if caller != Self::admin(&env) {
            panic!("unauthorized");
        }
        if !Self::is_paused(env.clone()) {
            panic!("not paused");
        }
        env.storage()
            .instance()
            .set(&Symbol::new(&env, "paused"), &false);
    }

    pub fn is_paused(env: Env) -> bool {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "paused"))
            .unwrap_or(false)
    }
}

struct Fixture {
    env: Env,
    coordinator_id: Address,
    admin: Address,
}

impl Fixture {
    fn new() -> Self {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let coordinator_id = env.register(CalloraEmergency, ());
        let client = CalloraEmergencyClient::new(&env, &coordinator_id);
        client.init(&admin);
        Self {
            env,
            coordinator_id,
            admin,
        }
    }

    fn client(&self) -> CalloraEmergencyClient<'_> {
        CalloraEmergencyClient::new(&self.env, &self.coordinator_id)
    }

    fn deploy_mock(&self) -> (Address, MockPausableClient<'_>) {
        let id = self.env.register(MockPausable, ());
        let client = MockPausableClient::new(&self.env, &id);
        client.init(&self.coordinator_id);
        (id, client)
    }
}

#[test]
fn init_is_one_time_and_admin_is_viewable() {
    let f = Fixture::new();
    assert_eq!(f.client().get_admin(), f.admin.clone());
    assert_eq!(
        f.client().try_init(&f.admin),
        Err(Ok(EmergencyError::AlreadyInitialized))
    );
    assert_eq!(f.client().get_registry_size(), 0);
    assert_eq!(f.client().get_guardian(), None);
}

#[test]
fn functions_require_initialization() {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let id = env.register(CalloraEmergency, ());
    let client = CalloraEmergencyClient::new(&env, &id);

    assert_eq!(
        client.try_pause_all(&admin),
        Err(Ok(EmergencyError::NotInitialized))
    );
    assert_eq!(
        client.try_get_admin(),
        Err(Ok(EmergencyError::NotInitialized))
    );
}

#[test]
fn pause_all_pauses_every_registered_contract() {
    let f = Fixture::new();
    let (a, a_client) = f.deploy_mock();
    let (b, b_client) = f.deploy_mock();
    f.client().register_contract(&f.admin, &a);
    f.client().register_contract(&f.admin, &b);

    assert_eq!(f.client().pause_all(&f.admin), 2);
    assert!(a_client.is_paused());
    assert!(b_client.is_paused());

    let report = f.client().get_last_pause_report().unwrap();
    assert_eq!(report.caller, f.admin);
    assert_eq!(report.succeeded, 2);
    assert_eq!(report.failed.len(), 0);
    assert!(report.paused);
}

#[test]
fn pause_all_continues_after_a_failing_target() {
    let f = Fixture::new();
    let (a, a_client) = f.deploy_mock();
    let bad = Address::generate(&f.env);
    let (b, b_client) = f.deploy_mock();
    f.client().register_contract(&f.admin, &a);
    f.client().register_contract(&f.admin, &bad);
    f.client().register_contract(&f.admin, &b);

    assert_eq!(f.client().pause_all(&f.admin), 2);
    assert!(a_client.is_paused());
    assert!(b_client.is_paused());

    let report = f.client().get_last_pause_report().unwrap();
    assert_eq!(report.succeeded, 2);
    assert_eq!(report.failed, Vec::from_array(&f.env, [bad]));
}

#[test]
fn already_paused_target_is_reported_as_failed() {
    let f = Fixture::new();
    let (a, a_client) = f.deploy_mock();
    f.client().register_contract(&f.admin, &a);

    assert_eq!(f.client().pause_all(&f.admin), 1);
    assert_eq!(f.client().pause_all(&f.admin), 0);
    assert!(a_client.is_paused());
    let report = f.client().get_last_pause_report().unwrap();
    assert_eq!(report.succeeded, 0);
    assert_eq!(report.failed.len(), 1);
}

#[test]
fn unpause_all_restores_registered_contracts() {
    let f = Fixture::new();
    let (a, a_client) = f.deploy_mock();
    let (b, b_client) = f.deploy_mock();
    f.client().register_contract(&f.admin, &a);
    f.client().register_contract(&f.admin, &b);
    f.client().pause_all(&f.admin);

    assert_eq!(f.client().unpause_all(&f.admin), 2);
    assert!(!a_client.is_paused());
    assert!(!b_client.is_paused());
    let report = f.client().get_last_pause_report().unwrap();
    assert!(!report.paused);
    assert_eq!(report.succeeded, 2);
}

#[test]
fn guardian_can_pause_but_not_unpause_or_administer() {
    let f = Fixture::new();
    let guardian = Address::generate(&f.env);
    let (a, a_client) = f.deploy_mock();
    f.client().register_contract(&f.admin, &a);
    f.client().set_guardian(&f.admin, &guardian);
    assert_eq!(f.client().get_guardian(), Some(guardian.clone()));

    assert_eq!(f.client().pause_all(&guardian), 1);
    assert!(a_client.is_paused());
    assert_eq!(
        f.client().try_unpause_all(&guardian),
        Err(Ok(EmergencyError::Unauthorized))
    );
    assert_eq!(
        f.client().try_set_guardian(&guardian, &guardian),
        Err(Ok(EmergencyError::Unauthorized))
    );
    assert_eq!(f.client().unpause_all(&f.admin), 1);
    assert!(!a_client.is_paused());
}

#[test]
fn stranger_cannot_pause_or_register() {
    let f = Fixture::new();
    let stranger = Address::generate(&f.env);
    let target = Address::generate(&f.env);

    assert_eq!(
        f.client().try_pause_all(&stranger),
        Err(Ok(EmergencyError::Unauthorized))
    );
    assert_eq!(
        f.client().try_register_contract(&stranger, &target),
        Err(Ok(EmergencyError::Unauthorized))
    );
    assert_eq!(
        f.client().try_set_guardian(&stranger, &stranger),
        Err(Ok(EmergencyError::Unauthorized))
    );
}

#[test]
fn pause_all_requires_caller_auth() {
    let f = Fixture::new();
    f.env.set_auths(&[]);
    assert!(f.client().try_pause_all(&f.admin).is_err());
}

#[test]
fn registry_rejects_duplicates_and_self() {
    let f = Fixture::new();
    let target = Address::generate(&f.env);
    f.client().register_contract(&f.admin, &target);

    assert_eq!(
        f.client().try_register_contract(&f.admin, &target),
        Err(Ok(EmergencyError::AlreadyRegistered))
    );
    assert_eq!(
        f.client().try_register_contract(&f.admin, &f.coordinator_id),
        Err(Ok(EmergencyError::InvalidTarget))
    );
    assert_eq!(f.client().get_registry_size(), 1);
    assert!(f.client().is_registered(&target));
}

#[test]
fn registry_is_bounded() {
    let f = Fixture::new();
    for _ in 0..MAX_REGISTRY_SIZE {
        f.client()
            .register_contract(&f.admin, &Address::generate(&f.env));
    }
    assert_eq!(f.client().get_registry_size(), MAX_REGISTRY_SIZE);
    assert_eq!(
        f.client()
            .try_register_contract(&f.admin, &Address::generate(&f.env)),
        Err(Ok(EmergencyError::RegistryFull))
    );
}

#[test]
fn deregister_removes_contract_and_rejects_unknown() {
    let f = Fixture::new();
    let target = Address::generate(&f.env);
    f.client().register_contract(&f.admin, &target);
    f.client().deregister_contract(&f.admin, &target);
    assert!(!f.client().is_registered(&target));
    assert_eq!(
        f.client().try_deregister_contract(&f.admin, &target),
        Err(Ok(EmergencyError::NotRegistered))
    );
}

#[test]
fn registry_pagination_is_bounded() {
    let f = Fixture::new();
    let mut targets = Vec::new(&f.env);
    for _ in 0..3 {
        let target = Address::generate(&f.env);
        f.client().register_contract(&f.admin, &target);
        targets.push_back(target);
    }

    let first = f.client().get_registered(&0, &2);
    assert_eq!(first, Vec::from_array(&f.env, [targets.get(0).unwrap(), targets.get(1).unwrap()]));
    let second = f.client().get_registered(&2, &2);
    assert_eq!(second, Vec::from_array(&f.env, [targets.get(2).unwrap()]));
    assert_eq!(f.client().get_registered(&3, &2).len(), 0);
    assert_eq!(f.client().get_registered(&0, &0).len(), 3);
}

#[test]
fn admin_transfer_is_two_step() {
    let f = Fixture::new();
    let new_admin = Address::generate(&f.env);

    assert_eq!(
        f.client().try_accept_admin(&new_admin),
        Err(Ok(EmergencyError::NoPendingAdmin))
    );
    f.client().set_admin(&f.admin, &new_admin);
    assert_eq!(f.client().get_pending_admin(), Some(new_admin.clone()));
    // The old admin remains in control until the nominee accepts.
    assert_eq!(f.client().pause_all(&f.admin), 0);

    f.client().accept_admin(&new_admin);
    assert_eq!(f.client().get_admin(), new_admin.clone());
    assert_eq!(f.client().get_pending_admin(), None);
    assert_eq!(f.client().pause_all(&new_admin), 0);
    assert_eq!(
        f.client().try_pause_all(&f.admin),
        Err(Ok(EmergencyError::Unauthorized))
    );
}

#[test]
fn admin_transfer_can_be_cancelled() {
    let f = Fixture::new();
    let new_admin = Address::generate(&f.env);
    f.client().set_admin(&f.admin, &new_admin);
    f.client().cancel_admin_transfer(&f.admin);
    assert_eq!(f.client().get_pending_admin(), None);
    assert_eq!(
        f.client().try_cancel_admin_transfer(&f.admin),
        Err(Ok(EmergencyError::NoPendingAdmin))
    );
    assert_eq!(f.client().get_admin(), f.admin.clone());
}

#[test]
fn guardian_can_be_cleared() {
    let f = Fixture::new();
    let guardian = Address::generate(&f.env);
    f.client().set_guardian(&f.admin, &guardian);
    f.client().clear_guardian(&f.admin);
    assert_eq!(f.client().get_guardian(), None);
    assert_eq!(
        f.client().try_pause_all(&guardian),
        Err(Ok(EmergencyError::Unauthorized))
    );
}

#[test]
fn pause_all_event_carries_the_report() {
    let f = Fixture::new();
    let (target, _) = f.deploy_mock();
    f.client().register_contract(&f.admin, &target);
    f.client().pause_all(&f.admin);

    let events = f.env.events().all();
    let event = events.last().unwrap();
    let topic: Symbol = event.1.get(0).unwrap().into_val(&f.env);
    assert_eq!(topic, Symbol::new(&f.env, "pause_all_executed"));
    let report: PauseReport = event.2.into_val(&f.env);
    assert_eq!(report.caller, f.admin);
    assert_eq!(report.succeeded, 1);
    assert!(report.paused);
}

#[test]
fn coordinator_pauses_real_distribute_and_revenue_pool() {
    let f = Fixture::new();
    let token = Address::generate(&f.env);

    let distribute_id = f.env.register(Distribute, ());
    let distribute = DistributeClient::new(&f.env, &distribute_id);
    distribute.init(&f.coordinator_id, &token);

    let pool_id = f.env.register(RevenuePool, ());
    let pool = RevenuePoolClient::new(&f.env, &pool_id);
    pool.init(&f.coordinator_id, &token);

    f.client().register_contract(&f.admin, &distribute_id);
    f.client().register_contract(&f.admin, &pool_id);

    assert_eq!(f.client().pause_all(&f.admin), 2);
    assert!(distribute.get_paused());
    assert!(pool.is_paused());

    assert_eq!(f.client().unpause_all(&f.admin), 2);
    assert!(!distribute.get_paused());
    assert!(!pool.is_paused());
}
