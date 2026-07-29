#![cfg(test)]

extern crate std;

use callora_upgrade::admin::{self, UpgradeError, DEFAULT_COOLDOWN_SECONDS};
use soroban_sdk::testutils::{Address as _, Events as _, Ledger as _};
use soroban_sdk::{contract, contractimpl, Address, Env, IntoVal, Symbol};

// ===========================================================================
// Mock contract - UpgradeHarness
// ===========================================================================

#[contract]
pub struct UpgradeHarness;

#[contractimpl]
impl UpgradeHarness {
    pub fn set_cooldown(env: Env, caller: Address, cooldown: u64) {
        admin::set_cooldown(&env, &caller, cooldown);
    }

    pub fn get_cooldown(env: Env) -> u64 {
        admin::get_cooldown(&env)
    }

    pub fn check_and_record_upgrade(env: Env, caller: Address) -> Result<(), UpgradeError> {
        admin::check_and_record_upgrade(&env, &caller)
    }

    pub fn check_and_record_upgrade_then_panic(
        env: Env,
        caller: Address,
    ) -> Result<(), UpgradeError> {
        admin::check_and_record_upgrade(&env, &caller)?;
        panic!("upgrade callee panicked after recording upgrade");
    }

    pub fn get_last_upgrade_time(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&Symbol::new(&env, "last_upg_tm"))
            .unwrap_or(0)
    }
}

// ===========================================================================
// Mock contract - UpgradeCaller
// ===========================================================================

#[contract]
pub struct UpgradeCaller;

#[contractimpl]
impl UpgradeCaller {
    pub fn call_set_cooldown(env: Env, upgrade: Address, caller: Address, cooldown: u64) {
        let client = UpgradeHarnessClient::new(&env, &upgrade);
        client.set_cooldown(&caller, &cooldown);
    }

    pub fn call_check_and_record_upgrade(
        env: Env,
        upgrade: Address,
        caller: Address,
    ) -> Result<(), UpgradeError> {
        let client = UpgradeHarnessClient::new(&env, &upgrade);
        match client.try_check_and_record_upgrade(&caller) {
            Ok(Ok(())) => Ok(()),
            Err(Ok(e)) => Err(e),
            other => panic!("unexpected result from upgrade callee: {other:?}"),
        }
    }

    pub fn call_check_and_record_upgrade_then_panic(env: Env, upgrade: Address, caller: Address) {
        let client = UpgradeHarnessClient::new(&env, &upgrade);
        client.check_and_record_upgrade_then_panic(&caller);
    }

    pub fn call_get_cooldown(env: Env, upgrade: Address) -> u64 {
        let client = UpgradeHarnessClient::new(&env, &upgrade);
        client.get_cooldown()
    }

    pub fn call_get_last_upgrade_time(env: Env, upgrade: Address) -> u64 {
        let client = UpgradeHarnessClient::new(&env, &upgrade);
        client.get_last_upgrade_time()
    }
}

// ===========================================================================
// Helpers
// ===========================================================================

struct TestContext {
    env: Env,
    admin: Address,
    upgrade_id: Address,
    upgrade_client: UpgradeHarnessClient<'static>,
    caller_client: UpgradeCallerClient<'static>,
}

fn setup() -> TestContext {
    let env = Env::default();
    env.mock_all_auths_allowing_non_root_auth();

    let admin = Address::generate(&env);
    let upgrade_id = env.register(UpgradeHarness, ());
    let upgrade_client = UpgradeHarnessClient::new(&env, &upgrade_id);

    let caller_id = env.register(UpgradeCaller, ());
    let caller_client = UpgradeCallerClient::new(&env, &caller_id);

    TestContext {
        env,
        admin,
        upgrade_id,
        upgrade_client,
        caller_client,
    }
}

fn events_with_topic(env: &Env, topic: &str) -> usize {
    let needle = Symbol::new(env, topic);
    env.events()
        .all()
        .iter()
        .filter(|event| {
            !event.1.is_empty() && {
                let t0: Symbol = event.1.get(0).unwrap().into_val(env);
                t0 == needle
            }
        })
        .count()
}

// ===========================================================================
// Cross-contract success and callee failure tests
// ===========================================================================

#[test]
fn test_xcontract_check_and_record_upgrade_success() {
    let ctx = setup();
    ctx.env.ledger().set_timestamp(100);

    ctx.caller_client
        .call_check_and_record_upgrade(&ctx.upgrade_id, &ctx.admin);

    assert_eq!(
        ctx.caller_client
            .call_get_last_upgrade_time(&ctx.upgrade_id),
        100
    );
    assert_eq!(events_with_topic(&ctx.env, "upgrade_started"), 1);
    assert_eq!(events_with_topic(&ctx.env, "upgrade_recorded"), 1);
}

#[test]
fn test_xcontract_callee_revert_preserves_upgrade_timestamp_and_events() {
    let ctx = setup();
    ctx.env.ledger().set_timestamp(100);
    ctx.caller_client
        .call_check_and_record_upgrade(&ctx.upgrade_id, &ctx.admin);

    ctx.env
        .ledger()
        .set_timestamp(100 + DEFAULT_COOLDOWN_SECONDS - 1);
    let result = ctx
        .caller_client
        .try_call_check_and_record_upgrade(&ctx.upgrade_id, &ctx.admin);
    match result {
        Err(Ok(UpgradeError::CooldownNotElapsed)) => {}
        other => panic!("expected Err(Ok(CooldownNotElapsed)), got {other:?}"),
    }

    assert_eq!(
        ctx.caller_client
            .call_get_last_upgrade_time(&ctx.upgrade_id),
        100
    );
    assert_eq!(events_with_topic(&ctx.env, "upgrade_started"), 1);
    assert_eq!(events_with_topic(&ctx.env, "upgrade_recorded"), 1);

    ctx.env
        .ledger()
        .set_timestamp(100 + DEFAULT_COOLDOWN_SECONDS);
    ctx.caller_client
        .call_check_and_record_upgrade(&ctx.upgrade_id, &ctx.admin);
    assert_eq!(
        ctx.caller_client
            .call_get_last_upgrade_time(&ctx.upgrade_id),
        100 + DEFAULT_COOLDOWN_SECONDS
    );
}

#[test]
fn test_xcontract_callee_overflow_revert_preserves_upgrade_timestamp() {
    let ctx = setup();
    ctx.env.ledger().set_timestamp(200);
    ctx.caller_client
        .call_check_and_record_upgrade(&ctx.upgrade_id, &ctx.admin);

    ctx.env.ledger().set_timestamp(199);
    let result = ctx
        .caller_client
        .try_call_check_and_record_upgrade(&ctx.upgrade_id, &ctx.admin);
    match result {
        Err(Ok(UpgradeError::Overflow)) => {}
        other => panic!("expected Err(Ok(Overflow)), got {other:?}"),
    }

    assert_eq!(
        ctx.caller_client
            .call_get_last_upgrade_time(&ctx.upgrade_id),
        200
    );
    assert_eq!(events_with_topic(&ctx.env, "upgrade_started"), 1);
    assert_eq!(events_with_topic(&ctx.env, "upgrade_recorded"), 1);
}

#[test]
fn test_xcontract_callee_panic_rolls_back_recorded_upgrade() {
    let ctx = setup();
    ctx.env.ledger().set_timestamp(500);

    let result = ctx
        .caller_client
        .try_call_check_and_record_upgrade_then_panic(&ctx.upgrade_id, &ctx.admin);
    assert!(result.is_err(), "expected panic from upgrade callee");

    assert_eq!(
        ctx.caller_client
            .call_get_last_upgrade_time(&ctx.upgrade_id),
        0
    );
    assert_eq!(events_with_topic(&ctx.env, "upgrade_started"), 0);
    assert_eq!(events_with_topic(&ctx.env, "upgrade_recorded"), 0);

    ctx.caller_client
        .call_check_and_record_upgrade(&ctx.upgrade_id, &ctx.admin);
    assert_eq!(
        ctx.caller_client
            .call_get_last_upgrade_time(&ctx.upgrade_id),
        500
    );
    assert_eq!(events_with_topic(&ctx.env, "upgrade_started"), 1);
    assert_eq!(events_with_topic(&ctx.env, "upgrade_recorded"), 1);
}

#[test]
fn test_xcontract_set_cooldown_survives_cross_contract_call() {
    let ctx = setup();

    ctx.caller_client
        .call_set_cooldown(&ctx.upgrade_id, &ctx.admin, &60);

    assert_eq!(ctx.caller_client.call_get_cooldown(&ctx.upgrade_id), 60);
    assert_eq!(ctx.upgrade_client.get_cooldown(), 60);
}
