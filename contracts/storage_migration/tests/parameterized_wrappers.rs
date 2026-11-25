//! Parameterized integration tests running identical scenarios against all three
//! migration contract wrappers: callora-migrate, callora-emergency, and callora-stake.

extern crate std;

use callora_emergency::migrate::{
    EmergencyMigrate, EmergencyMigrateClient, LegacyEmergency, StorageKey as EmergencyKey,
};
use callora_migrate::{
    EmergencyMigration, EmergencyMigrationClient, LegacyData, StorageKey as MigrateKey,
};
use callora_stake::migrate::{
    CalloraStakeMigrate, CalloraStakeMigrateClient, LegacyStake, StorageKey as StakeKey,
};
use soroban_sdk::testutils::Address as _;
use soroban_sdk::{Address, BytesN, Env};

trait MigrationHarness {
    fn name(&self) -> &'static str;
    fn register(&self, env: &Env) -> Address;
    fn try_init(
        &self,
        env: &Env,
        contract: &Address,
        admin: &Address,
        version: u32,
    ) -> Result<(), ()>;
    fn stage_legacy(&self, env: &Env, contract: &Address);
    fn try_migrate(
        &self,
        env: &Env,
        contract: &Address,
        caller: &Address,
        expected: u32,
        target: u32,
    ) -> Result<(), ()>;
    fn try_authorize_upgrade(
        &self,
        env: &Env,
        contract: &Address,
        caller: &Address,
        target: u32,
        hash: &BytesN<32>,
    ) -> Result<(), ()>;
    fn version(&self, env: &Env, contract: &Address) -> Option<u32>;
    fn is_upgrade_authorized(&self, env: &Env, contract: &Address, hash: &BytesN<32>) -> bool;
    fn rejects_zero_hash(&self) -> bool;
}

// ─── Migrate Harness ─────────────────────────────────────────────────────────

struct MigrateHarness;

impl MigrationHarness for MigrateHarness {
    fn name(&self) -> &'static str {
        "callora-migrate"
    }

    fn register(&self, env: &Env) -> Address {
        env.register(EmergencyMigration, ())
    }

    fn try_init(
        &self,
        env: &Env,
        contract: &Address,
        admin: &Address,
        version: u32,
    ) -> Result<(), ()> {
        let client = EmergencyMigrationClient::new(env, contract);
        client
            .try_init(admin, &version)
            .map(|r| r.map_err(|_| ()))
            .unwrap_or(Err(()))
    }

    fn stage_legacy(&self, env: &Env, contract: &Address) {
        env.as_contract(contract, || {
            env.storage().instance().set(
                &MigrateKey::Legacy,
                &LegacyData {
                    balance: 500,
                    last_updated: 42,
                },
            );
        });
    }

    fn try_migrate(
        &self,
        env: &Env,
        contract: &Address,
        caller: &Address,
        expected: u32,
        target: u32,
    ) -> Result<(), ()> {
        let client = EmergencyMigrationClient::new(env, contract);
        client
            .try_migrate(caller, &expected, &target)
            .map(|r| r.map(|_| ()).map_err(|_| ()))
            .unwrap_or(Err(()))
    }

    fn try_authorize_upgrade(
        &self,
        env: &Env,
        contract: &Address,
        caller: &Address,
        target: u32,
        hash: &BytesN<32>,
    ) -> Result<(), ()> {
        let client = EmergencyMigrationClient::new(env, contract);
        client
            .try_authorize_upgrade(caller, &target, hash)
            .map(|r| r.map_err(|_| ()))
            .unwrap_or(Err(()))
    }

    fn version(&self, env: &Env, contract: &Address) -> Option<u32> {
        EmergencyMigrationClient::new(env, contract).version()
    }

    fn is_upgrade_authorized(&self, env: &Env, contract: &Address, hash: &BytesN<32>) -> bool {
        EmergencyMigrationClient::new(env, contract).is_upgrade_authorized(hash)
    }

    fn rejects_zero_hash(&self) -> bool {
        true
    }
}

// ─── Emergency Harness ───────────────────────────────────────────────────────

struct EmergencyHarness;

impl MigrationHarness for EmergencyHarness {
    fn name(&self) -> &'static str {
        "callora-emergency"
    }

    fn register(&self, env: &Env) -> Address {
        env.register(EmergencyMigrate, ())
    }

    fn try_init(
        &self,
        env: &Env,
        contract: &Address,
        admin: &Address,
        version: u32,
    ) -> Result<(), ()> {
        let client = EmergencyMigrateClient::new(env, contract);
        client
            .try_init(admin, &version)
            .map(|r| r.map_err(|_| ()))
            .unwrap_or(Err(()))
    }

    fn stage_legacy(&self, env: &Env, contract: &Address) {
        env.as_contract(contract, || {
            env.storage().instance().set(
                &EmergencyKey::Legacy,
                &LegacyEmergency {
                    balance: 500,
                    last_updated: 42,
                },
            );
        });
    }

    fn try_migrate(
        &self,
        env: &Env,
        contract: &Address,
        caller: &Address,
        expected: u32,
        target: u32,
    ) -> Result<(), ()> {
        let client = EmergencyMigrateClient::new(env, contract);
        client
            .try_migrate(caller, &expected, &target)
            .map(|r| r.map(|_| ()).map_err(|_| ()))
            .unwrap_or(Err(()))
    }

    fn try_authorize_upgrade(
        &self,
        env: &Env,
        contract: &Address,
        caller: &Address,
        target: u32,
        hash: &BytesN<32>,
    ) -> Result<(), ()> {
        let client = EmergencyMigrateClient::new(env, contract);
        client
            .try_authorize_upgrade(caller, &target, hash)
            .map(|r| r.map_err(|_| ()))
            .unwrap_or(Err(()))
    }

    fn version(&self, env: &Env, contract: &Address) -> Option<u32> {
        EmergencyMigrateClient::new(env, contract).version()
    }

    fn is_upgrade_authorized(&self, env: &Env, contract: &Address, hash: &BytesN<32>) -> bool {
        EmergencyMigrateClient::new(env, contract).is_upgrade_authorised(hash)
    }

    fn rejects_zero_hash(&self) -> bool {
        false
    }
}

// ─── Stake Harness ───────────────────────────────────────────────────────────

struct StakeHarness;

impl MigrationHarness for StakeHarness {
    fn name(&self) -> &'static str {
        "callora-stake"
    }

    fn register(&self, env: &Env) -> Address {
        env.register(CalloraStakeMigrate, ())
    }

    fn try_init(
        &self,
        env: &Env,
        contract: &Address,
        admin: &Address,
        version: u32,
    ) -> Result<(), ()> {
        let client = CalloraStakeMigrateClient::new(env, contract);
        client
            .try_init(admin, &version)
            .map(|r| r.map_err(|_| ()))
            .unwrap_or(Err(()))
    }

    fn stage_legacy(&self, env: &Env, contract: &Address) {
        env.as_contract(contract, || {
            env.storage().instance().set(
                &StakeKey::Legacy,
                &LegacyStake {
                    total_staked: 500,
                    last_checkpoint: 42,
                },
            );
        });
    }

    fn try_migrate(
        &self,
        env: &Env,
        contract: &Address,
        caller: &Address,
        expected: u32,
        target: u32,
    ) -> Result<(), ()> {
        let client = CalloraStakeMigrateClient::new(env, contract);
        client
            .try_migrate(caller, &expected, &target)
            .map(|r| r.map(|_| ()).map_err(|_| ()))
            .unwrap_or(Err(()))
    }

    fn try_authorize_upgrade(
        &self,
        env: &Env,
        contract: &Address,
        caller: &Address,
        target: u32,
        hash: &BytesN<32>,
    ) -> Result<(), ()> {
        let client = CalloraStakeMigrateClient::new(env, contract);
        client
            .try_authorize_upgrade(caller, &target, hash)
            .map(|r| r.map_err(|_| ()))
            .unwrap_or(Err(()))
    }

    fn version(&self, env: &Env, contract: &Address) -> Option<u32> {
        CalloraStakeMigrateClient::new(env, contract).version()
    }

    fn is_upgrade_authorized(&self, env: &Env, contract: &Address, hash: &BytesN<32>) -> bool {
        CalloraStakeMigrateClient::new(env, contract).is_upgrade_authorised(hash)
    }

    fn rejects_zero_hash(&self) -> bool {
        true
    }
}

// ─── Scenarios ───────────────────────────────────────────────────────────────

fn run_init_twice_scenario(harness: &dyn MigrationHarness) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract = harness.register(&env);

    assert!(
        harness.try_init(&env, &contract, &admin, 1).is_ok(),
        "{}: first init must succeed",
        harness.name()
    );
    assert_eq!(
        harness.version(&env, &contract),
        Some(1),
        "{}: version must match initial version",
        harness.name()
    );
    assert!(
        harness.try_init(&env, &contract, &admin, 2).is_err(),
        "{}: second init must fail",
        harness.name()
    );
}

fn run_migrate_without_init_scenario(harness: &dyn MigrationHarness) {
    let env = Env::default();
    env.mock_all_auths();
    let caller = Address::generate(&env);
    let contract = harness.register(&env);

    assert!(
        harness.try_migrate(&env, &contract, &caller, 0, 1).is_err(),
        "{}: migrate without init must fail",
        harness.name()
    );
}

fn run_version_skip_scenario(harness: &dyn MigrationHarness) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract = harness.register(&env);
    harness.try_init(&env, &contract, &admin, 1).unwrap();
    harness.stage_legacy(&env, &contract);

    assert!(
        harness.try_migrate(&env, &contract, &admin, 1, 3).is_err(),
        "{}: version skip (target = expected + 2) must be rejected",
        harness.name()
    );
}

fn run_rollback_scenario(harness: &dyn MigrationHarness) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract = harness.register(&env);
    harness.try_init(&env, &contract, &admin, 2).unwrap();
    harness.stage_legacy(&env, &contract);

    // Rollback attempt via migrate (target 1 < current 2)
    assert!(
        harness.try_migrate(&env, &contract, &admin, 2, 1).is_err(),
        "{}: rollback via migrate must be rejected",
        harness.name()
    );

    // Rollback attempt via authorize_upgrade (target 1 < current 2)
    let hash = BytesN::from_array(&env, &[0xAA; 32]);
    assert!(
        harness
            .try_authorize_upgrade(&env, &contract, &admin, 1, &hash)
            .is_err(),
        "{}: rollback via authorize_upgrade must be rejected",
        harness.name()
    );
}

fn run_zero_wasm_hash_scenario(harness: &dyn MigrationHarness) {
    let env = Env::default();
    env.mock_all_auths();
    let admin = Address::generate(&env);
    let contract = harness.register(&env);
    harness.try_init(&env, &contract, &admin, 1).unwrap();

    let zero_hash = BytesN::from_array(&env, &[0u8; 32]);
    let res = harness.try_authorize_upgrade(&env, &contract, &admin, 1, &zero_hash);
    if harness.rejects_zero_hash() {
        assert!(
            res.is_err(),
            "{}: all-zero wasm hash must be rejected",
            harness.name()
        );
    } else {
        assert!(
            res.is_ok(),
            "{}: all-zero wasm hash is accepted per specification",
            harness.name()
        );
        assert!(
            harness.is_upgrade_authorized(&env, &contract, &zero_hash),
            "{}: all-zero hash must be recognized as authorized",
            harness.name()
        );
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[test]
fn parameterized_init_twice_all_wrappers() {
    let harnesses: [&dyn MigrationHarness; 3] = [&MigrateHarness, &EmergencyHarness, &StakeHarness];
    for h in harnesses {
        run_init_twice_scenario(h);
    }
}

#[test]
fn parameterized_migrate_without_init_all_wrappers() {
    let harnesses: [&dyn MigrationHarness; 3] = [&MigrateHarness, &EmergencyHarness, &StakeHarness];
    for h in harnesses {
        run_migrate_without_init_scenario(h);
    }
}

#[test]
fn parameterized_version_skip_all_wrappers() {
    let harnesses: [&dyn MigrationHarness; 3] = [&MigrateHarness, &EmergencyHarness, &StakeHarness];
    for h in harnesses {
        run_version_skip_scenario(h);
    }
}

#[test]
fn parameterized_rollback_all_wrappers() {
    let harnesses: [&dyn MigrationHarness; 3] = [&MigrateHarness, &EmergencyHarness, &StakeHarness];
    for h in harnesses {
        run_rollback_scenario(h);
    }
}

#[test]
fn parameterized_zero_wasm_hash_all_wrappers() {
    let harnesses: [&dyn MigrationHarness; 3] = [&MigrateHarness, &EmergencyHarness, &StakeHarness];
    for h in harnesses {
        run_zero_wasm_hash_scenario(h);
    }
}
